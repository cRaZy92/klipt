//! "Combine": several clips joined in order into one new clip, e.g. to upload a few moments of one
//! evening as a single video. Each clip goes in as its upload would (its trim, cuts, zooms,
//! subtitles, blur and volumes), and the source clips and their files are left as they are.
//!
//! Each clip is first exported as for an upload (usually a fast stream copy), then the exports are
//! joined with ffmpeg's `concat` filter and the picture is encoded once more: clips can differ in
//! size, frame rate and encoder, which a plain stream join can't mix. A clip smaller than the
//! biggest one (or with another shape) is scaled to fit and centred on black.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::errors::{AppError, Result};
use crate::export::{self, Tracks};
use crate::ffmpeg::{self, Tools};
use crate::library::{Clip, ClipKind, ClipView, Library, NewClip};
use crate::parts::{file_stem, free_path};

/// The most clips one combine takes.
pub const MAX_CLIPS: usize = 50;
/// The highest frame rate the combined clip gets.
const MAX_FPS: u32 = 120;
/// Of the progress bar: the share the per-clip exports take (the join takes the rest).
const EXPORT_SHARE: f32 = 0.25;

/// What one clip's export looks like, for picking the combined clip's size and frame rate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PartInfo {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
}

/// The combined clip's picture: the size of the biggest clip (by area; even, as H.264 wants) and
/// the highest frame rate among them (30 when none says).
pub fn output_format(parts: &[PartInfo]) -> (u32, u32, u32) {
    let even = |v: u32| (v / 2 * 2).max(2);
    let (w, h) = parts
        .iter()
        .filter(|p| p.width > 0 && p.height > 0)
        .max_by_key(|p| u64::from(p.width) * u64::from(p.height))
        .map(|p| (p.width, p.height))
        .unwrap_or((1280, 720));
    let fps = parts.iter().map(|p| p.fps).filter(|f| f.is_finite() && *f > 0.0).fold(0.0, f64::max);
    let fps = if fps > 0.0 { (fps.round() as u32).clamp(1, MAX_FPS) } else { 30 };
    (even(w), even(h), fps)
}

/// The size a `w`×`h` picture is scaled to so it fits `out_w`×`out_h` keeping its shape (even,
/// so the chroma planes line up). A picture of the right size stays as it is.
pub fn fit(w: u32, h: u32, out_w: u32, out_h: u32) -> (u32, u32) {
    if w == 0 || h == 0 {
        return (out_w, out_h);
    }
    let scale = (f64::from(out_w) / f64::from(w)).min(f64::from(out_h) / f64::from(h));
    let even = |v: f64, max: u32| ((v.floor() as u32) / 2 * 2).clamp(2, max);
    (even(f64::from(w) * scale, out_w), even(f64::from(h) * scale, out_h))
}

/// The ffmpeg arguments (without the program name) that join `inputs` (exports: one video and one
/// AAC track each) in order into `output`, at `out_w`×`out_h` and `fps`, encoded with `encoder`.
pub fn join_args(
    inputs: &[(PathBuf, PartInfo)],
    output: &Path,
    (out_w, out_h, fps): (u32, u32, u32),
    encoder: &[&str],
) -> Vec<String> {
    let mut args: Vec<String> = ["-hide_banner", "-nostdin", "-y"].map(String::from).into();
    for (path, _) in inputs {
        args.extend(["-i".into(), path.to_string_lossy().into_owned()]);
    }
    let mut graph = Vec::new();
    let mut pairs = String::new();
    for (i, (_, info)) in inputs.iter().enumerate() {
        let (w, h) = fit(info.width, info.height, out_w, out_h);
        graph.push(format!(
            "[{i}:v:0]scale={w}:{h},pad={out_w}:{out_h}:(ow-iw)/2:(oh-ih)/2:color=black,setsar=1,fps={fps},format=yuv420p[v{i}]"
        ));
        // One sample rate and layout, or `concat` refuses the parts.
        graph.push(format!("[{i}:a:0]aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo[a{i}]"));
        pairs.push_str(&format!("[v{i}][a{i}]"));
    }
    graph.push(format!("{pairs}concat=n={}:v=1:a=1[vout][aout]", inputs.len()));
    args.extend(["-filter_complex".into(), graph.join(";"), "-map".into(), "[vout]".into()]);
    args.extend(encoder.iter().map(|s| s.to_string()));
    // Like imported copies: a keyframe each second keeps trimming and seeking exact.
    args.extend(["-pix_fmt", "yuv420p", "-force_key_frames", "expr:gte(t,n_forced*1)"].map(String::from));
    args.extend(
        [
            "-map",
            "[aout]",
            "-c:a",
            "aac",
            "-b:a",
            "160k",
            "-movflags",
            "+faststart",
            "-f",
            "mp4",
            "-progress",
            "pipe:1",
            "-nostats",
        ]
        .map(String::from),
    );
    args.push(output.to_string_lossy().into_owned());
    args
}

/// The game the combined clip is filed under: the one most of the clips have (the earliest of
/// those on a tie), with its Steam id and exe from the first clip of it. None when no clip has one.
pub fn common_game(clips: &[Clip]) -> (Option<String>, Option<u32>, Option<String>) {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for name in clips.iter().filter_map(|c| c.game_name.as_deref()) {
        *counts.entry(name).or_default() += 1;
    }
    let Some(most) = counts.values().copied().max() else {
        return (None, None, None);
    };
    let first = clips
        .iter()
        .find(|c| c.game_name.as_deref().is_some_and(|g| counts.get(g) == Some(&most)))
        .expect("a clip has the most common game");
    (first.game_name.clone(), first.steam_app_id, first.exe.clone())
}

/// The combined clip's thumbnail frame: the first clip's chosen frame (where it lands in its
/// upload), or a third of the way into the first clip, but no more than 1 s.
pub fn combined_thumbnail(first: &Clip, first_ms: u64) -> u64 {
    match first.thumbnail_ms {
        Some(_) => crate::commands::upload_thumbnail_ms(first).unwrap_or(0).min(first_ms.saturating_sub(1)),
        None => crate::recorder::thumbnail_at_ms(first_ms),
    }
}

/// Joins `clips` (in this order, each with its saved edit) into a new clip titled `title` in
/// `dir`. `progress` gets 0..=1. The source clips aren't changed.
pub fn combine(
    tools: &Tools,
    library: &Mutex<Library>,
    clips: &[Clip],
    title: &str,
    dir: &Path,
    cache_dir: &Path,
    mut progress: impl FnMut(f32),
) -> Result<ClipView> {
    if clips.len() < 2 || clips.len() > MAX_CLIPS {
        return Err(AppError::InvalidInput(format!("combine takes 2 to {MAX_CLIPS} clips, not {}", clips.len())));
    }
    if let Some(c) = clips.iter().find(|c| c.kind != ClipKind::Clip) {
        return Err(AppError::InvalidInput(format!("{} is a screenshot", c.id)));
    }
    if clips.iter().any(|c| !c.path.is_file()) {
        return Err(AppError::FileMissing);
    }
    let encoder = crate::import::detect_encoder(tools).ok_or_else(|| AppError::ToolFailed {
        tool: "ffmpeg",
        detail: "no H.264 encoder to join the clips with".into(),
    })?;

    // Unique per combine, so two at once (or an upload of one of the clips) never share a file.
    let run = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let mut exports: Vec<(PathBuf, PartInfo)> = Vec::new();
    let result = (|| {
        // Weighs each clip's export by its kept length.
        let lengths: Vec<u64> = clips
            .iter()
            .map(|c| {
                export::kept_ranges(&crate::editor::current_edit(c), c.duration_ms)
                    .iter()
                    .map(|(s, e)| e - s)
                    .sum::<u64>()
                    .max(1)
            })
            .collect();
        let total: u64 = lengths.iter().sum();
        let mut done = 0;
        for (i, clip) in clips.iter().enumerate() {
            let base = EXPORT_SHARE * done as f32 / total as f32;
            let share = EXPORT_SHARE * lengths[i] as f32 / total as f32;
            let id = format!("combine-{run}-{i}");
            let out = export::export(
                tools,
                &clip.path,
                cache_dir,
                &id,
                &crate::editor::current_edit(clip),
                Tracks::of(clip),
                |p| progress(base + share * p),
            )?;
            // Kept even if probing fails, so it's cleaned up below.
            exports.push((out.clone(), PartInfo { width: 0, height: 0, fps: 0.0 }));
            let probe = ffmpeg::probe(tools, &out)?;
            exports[i].1 = PartInfo { width: probe.width, height: probe.height, fps: probe.fps };
            done += lengths[i];
        }
        let format = output_format(&exports.iter().map(|(_, p)| *p).collect::<Vec<_>>());
        let path = free_path(dir, &file_stem(title));
        fs::create_dir_all(dir)?;
        let temp = cache_dir.join("exports").join(format!("combine-{run}.mp4"));
        let args = join_args(&exports, &temp, format, &encoder.args);
        let joined = export::run(tools, &args, cache_dir, total as f64 * 1000.0, &mut |p| {
            progress(EXPORT_SHARE + (1.0 - EXPORT_SHARE) * p)
        })
        .and_then(|()| match fs::metadata(&temp) {
            Ok(meta) if meta.len() > 0 => crate::editor::move_file(&temp, &path),
            _ => Err(AppError::ToolFailed { tool: "ffmpeg", detail: "joining the clips produced no output".into() }),
        });
        if joined.is_err() {
            let _ = fs::remove_file(&temp);
        }
        joined?;
        let added = add_to_library(tools, library, clips, title, &path, format, lengths[0]);
        if added.is_err() {
            let _ = fs::remove_file(&path);
        }
        added
    })();
    for (path, _) in &exports {
        let _ = fs::remove_file(path);
    }
    let view = result?;
    progress(1.0);
    Ok(view)
}

/// Files the joined video at `path` as a new clip, with a thumbnail from the first clip.
fn add_to_library(
    tools: &Tools,
    library: &Mutex<Library>,
    clips: &[Clip],
    title: &str,
    path: &Path,
    (width, height, _): (u32, u32, u32),
    first_ms: u64,
) -> Result<ClipView> {
    let probe = ffmpeg::probe(tools, path)?;
    let (game_name, steam_app_id, exe) = common_game(clips);
    let new = NewClip {
        path: path.to_path_buf(),
        created_at: SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0),
        duration_ms: probe.duration_ms,
        width: if probe.width > 0 { probe.width } else { width },
        height: if probe.height > 0 { probe.height } else { height },
        // The exports mix every track into one: the joined clip only has the mix.
        has_mic: false,
        has_voice: false,
        kind: ClipKind::Clip,
        is_recording: false,
        game_name,
        steam_app_id,
        exe,
        title: title.to_string(),
    };
    let thumb_ms = combined_thumbnail(&clips[0], first_ms).min(probe.duration_ms.saturating_sub(1));
    let (clip, thumb) = {
        let library = library.lock().unwrap_or_else(PoisonError::into_inner);
        let clip = library.insert(new)?;
        if let Err(e) = library.set_thumbnail(&clip.id, Some(thumb_ms)) {
            let _ = library.remove(&clip.id, false);
            return Err(e);
        }
        (library.get(&clip.id)?, library.thumb_path(&clip.id))
    };
    if let Err(e) = ffmpeg::thumbnail(tools, path, &thumb, thumb_ms) {
        crate::elog!("klipt: can't make a thumbnail for {}: {e}", path.display());
    }
    Ok(ClipView { file_missing: false, thumb_path: thumb.is_file().then_some(thumb), clip })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffmpeg::tests::{fixture, tools};
    use crate::library::SubtitleRange;
    use std::process::Command;

    fn part(width: u32, height: u32, fps: f64) -> PartInfo {
        PartInfo { width, height, fps }
    }

    #[test]
    fn the_biggest_clip_sets_the_size_and_the_fastest_the_frame_rate() {
        assert_eq!(output_format(&[part(1280, 720, 30.0), part(1920, 1080, 59.94)]), (1920, 1080, 60));
        assert_eq!(output_format(&[part(1081, 721, 0.0), part(0, 0, 0.0)]), (1080, 720, 30));
        assert_eq!(output_format(&[part(0, 0, 0.0)]), (1280, 720, 30));
        assert_eq!(output_format(&[part(640, 480, 240.0)]), (640, 480, 120));
    }

    #[test]
    fn a_picture_fits_keeping_its_shape() {
        assert_eq!(fit(1920, 1080, 1920, 1080), (1920, 1080));
        assert_eq!(fit(1280, 720, 1920, 1080), (1920, 1080));
        // 4:3 in 16:9: as tall, narrower (black bars at the sides).
        assert_eq!(fit(1440, 1080, 1920, 1080), (1440, 1080));
        // An ultrawide in 16:9: as wide, shorter.
        assert_eq!(fit(2560, 1080, 1920, 1080), (1920, 810));
        assert_eq!(fit(321, 241, 640, 480), (638, 480));
    }

    #[test]
    fn join_args_scale_pad_and_concat_every_clip() {
        let inputs =
            vec![(PathBuf::from("a.mp4"), part(1920, 1080, 60.0)), (PathBuf::from("b.mp4"), part(1280, 720, 30.0))];
        let joined = join_args(&inputs, Path::new("out.mp4"), (1920, 1080, 60), &["-c:v", "libx264"]).join(" ");
        assert!(joined.starts_with("-hide_banner -nostdin -y -i a.mp4 -i b.mp4 -filter_complex "), "{joined}");
        assert!(joined.contains("[1:v:0]scale=1920:1080,pad=1920:1080:(ow-iw)/2:(oh-ih)/2:color=black,setsar=1,fps=60,format=yuv420p[v1]"), "{joined}");
        assert!(
            joined.contains("[1:a:0]aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo[a1]"),
            "{joined}"
        );
        assert!(joined.contains("[v0][a0][v1][a1]concat=n=2:v=1:a=1[vout][aout] -map [vout] -c:v libx264"), "{joined}");
        assert!(
            joined.ends_with(
                "-map [aout] -c:a aac -b:a 160k -movflags +faststart -f mp4 -progress pipe:1 -nostats out.mp4"
            ),
            "{joined}"
        );
    }

    fn clip(id: &str, game: Option<&str>, steam: Option<u32>) -> Clip {
        let library = Library::open(Path::new(":memory:")).unwrap();
        let mut clip = library
            .insert(NewClip {
                path: PathBuf::from(format!("{id}.mp4")),
                created_at: 1,
                duration_ms: 6_000,
                width: 320,
                height: 240,
                has_mic: false,
                has_voice: false,
                kind: ClipKind::Clip,
                is_recording: false,
                game_name: game.map(String::from),
                steam_app_id: steam,
                exe: None,
                title: id.into(),
            })
            .unwrap();
        clip.id = id.into();
        clip
    }

    #[test]
    fn the_game_is_the_one_most_clips_have() {
        let clips = [
            clip("a", None, None),
            clip("b", Some("CS2"), Some(730)),
            clip("c", Some("Dota"), None),
            clip("d", Some("Dota"), Some(570)),
        ];
        assert_eq!(common_game(&clips), (Some("Dota".into()), None, None));
        // A tie: the earliest.
        assert_eq!(common_game(&clips[..3]), (Some("CS2".into()), Some(730), None));
        assert_eq!(common_game(&clips[..1]), (None, None, None));
    }

    #[test]
    fn the_thumbnail_is_the_first_clips() {
        let mut first = clip("a", None, None);
        assert_eq!(combined_thumbnail(&first, 3_000), 1_000);
        first.trim_start_ms = Some(2_000);
        first.thumbnail_ms = Some(2_500);
        assert_eq!(combined_thumbnail(&first, 3_000), 500);
    }

    /// A 2 s clip at another size and frame rate, with one audio track at another sample rate.
    fn small_clip(dir: &Path) -> PathBuf {
        let path = dir.join("small.mp4");
        let status = Command::new(&tools().ffmpeg)
            .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y"])
            .args(["-f", "lavfi", "-i", "testsrc2=size=160x160:rate=24"])
            .args(["-f", "lavfi", "-i", "sine=frequency=220:sample_rate=44100"])
            .args(["-t", "2", "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac"])
            .arg(&path)
            .status()
            .unwrap();
        assert!(status.success());
        path
    }

    #[test]
    fn clips_of_any_shape_join_into_one_new_clip() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("cache");
        let library = Mutex::new(Library::open(&dir.path().join("library.db")).unwrap());
        let a_path = dir.path().join("a.mp4");
        std::fs::copy(fixture(), &a_path).unwrap();
        let b_path = small_clip(dir.path());
        let (a, b) = {
            let lib = library.lock().unwrap();
            let new = |path: &Path, has_mic, game: &str| NewClip {
                path: path.to_path_buf(),
                created_at: 1,
                duration_ms: 6_000,
                width: 320,
                height: 240,
                has_mic,
                has_voice: false,
                kind: ClipKind::Clip,
                is_recording: false,
                game_name: Some(game.into()),
                steam_app_id: None,
                exe: None,
                title: "x".into(),
            };
            let a = lib.insert(new(&a_path, true, "WARDOGS")).unwrap();
            let b = lib.insert(NewClip { duration_ms: 2_000, ..new(&b_path, false, "WARDOGS") }).unwrap();
            // The first clip goes in trimmed to 1..3 s, with a subtitle.
            lib.update_edit(
                &a.id,
                crate::library::Trim { start_ms: Some(1_000), end_ms: Some(3_000) },
                &crate::library::Volumes::default(),
            )
            .unwrap();
            lib.set_subtitles(&a.id, &[SubtitleRange { start_ms: 1_500, end_ms: 2_500, text: "gg".into() }]).unwrap();
            (lib.get(&a.id).unwrap(), lib.get(&b.id).unwrap())
        };
        let before = std::fs::read(&a_path).unwrap();
        let mut seen = Vec::new();

        let view = combine(&tools(), &library, &[a.clone(), b.clone()], "Best of: tonight", dir.path(), &cache, |p| {
            seen.push(p)
        })
        .unwrap();
        let clip = view.clip;
        assert_eq!(clip.path, dir.path().join("Best of tonight.mp4"));
        assert_eq!((clip.title.as_str(), clip.game_name.as_deref()), ("Best of: tonight", Some("WARDOGS")));
        assert!(!clip.has_mic && !clip.is_recording);
        let probe = ffmpeg::probe(&tools(), &clip.path).unwrap();
        assert_eq!((probe.width, probe.height, probe.audio_tracks), (320, 240, 1));
        assert_eq!(probe.fps.round(), 30.0);
        // 2 s of the first clip and the whole 2 s of the second.
        assert!((3_800..=4_300).contains(&clip.duration_ms), "{}", clip.duration_ms);
        assert!(view.thumb_path.is_some_and(|t| t.is_file()));
        assert_eq!(seen.last(), Some(&1.0));
        assert!(seen.windows(2).all(|w| w[0] <= w[1]), "{seen:?}");

        // The sources are as they were, and no export is left over.
        assert_eq!(std::fs::read(&a_path).unwrap(), before);
        assert_eq!(library.lock().unwrap().get(&a.id).unwrap(), a);
        let leftovers: Vec<_> =
            std::fs::read_dir(cache.join("exports")).unwrap().flatten().map(|e| e.file_name()).collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    #[test]
    fn one_clip_or_a_screenshot_is_refused_and_a_missing_file_leaves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let library = Mutex::new(Library::open(&dir.path().join("library.db")).unwrap());
        let a = clip("a", None, None);
        let run =
            |clips: &[Clip]| combine(&tools(), &library, clips, "x", dir.path(), &dir.path().join("cache"), |_| {});
        assert!(matches!(run(std::slice::from_ref(&a)), Err(AppError::InvalidInput(_))));
        let shot = Clip { kind: ClipKind::Screenshot, ..a.clone() };
        assert!(matches!(run(&[a.clone(), shot]), Err(AppError::InvalidInput(_))));
        assert!(matches!(run(&[a.clone(), a]), Err(AppError::FileMissing)));
        assert!(!dir.path().join("x.mp4").exists());
        assert!(library.lock().unwrap().list(&Default::default()).unwrap().is_empty());
    }
}
