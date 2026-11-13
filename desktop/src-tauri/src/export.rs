//! Exporting a clip for upload: the video is stream-copied (trimmed), and every audio track (game,
//! mic, voice chat and any others an imported clip has) is mixed into one AAC track at the user's
//! chosen volumes.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::errors::{AppError, Result};
use crate::ffmpeg::{probe, Tools};

/// What the user chose in the editor.
#[derive(Debug, Clone, PartialEq)]
pub struct Edit {
    pub trim_start_ms: Option<u64>,
    pub trim_end_ms: Option<u64>,
    pub game_volume: f32,
    pub mic_volume: f32,
    pub voice_volume: f32,
    /// The whole mix, applied after the tracks are mixed.
    pub master_volume: f32,
    /// The other audio tracks, by track index; a track not listed plays at 1.
    pub extra_volumes: BTreeMap<u32, f32>,
    /// Blur the picture (re-encoded at most 720p): a clip where only the sound matters.
    pub blur_video: bool,
}

/// How strong "Blur video" is: a Gaussian sigma at 720p. Shapes and colours stay, details don't.
pub const BLUR_SIGMA: u32 = 8;
/// The tallest a blurred clip is encoded: more detail would only be blurred away.
const BLUR_MAX_HEIGHT: u32 = 720;

/// Which audio tracks besides the game (always track 0) a clip has, by track index.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tracks {
    pub mic: Option<u32>,
    pub voice: Option<u32>,
    /// Any other tracks (an imported clip's), in order.
    pub extra: Vec<u32>,
}

impl Tracks {
    /// The mic and voice chat tracks the library knows of; `present_in` adds the others.
    pub fn of(clip: &crate::library::Clip) -> Tracks {
        Tracks { mic: clip.mic_track(), voice: clip.voice_track(), extra: Vec::new() }
    }

    /// Matches the file's `audio_tracks` (how many it has): drops tracks it doesn't have and adds
    /// every track that is neither the game, the mic nor voice chat as an extra track.
    pub fn present_in(self, audio_tracks: u32) -> Tracks {
        let keep = |t: Option<u32>| t.filter(|i| *i < audio_tracks);
        let (mic, voice) = (keep(self.mic), keep(self.voice));
        let extra = (1..audio_tracks).filter(|i| Some(*i) != mic && Some(*i) != voice).collect();
        Tracks { mic, voice, extra }
    }
}

fn volume(v: f32) -> f32 {
    if v.is_nan() {
        1.0
    } else {
        v.clamp(0.0, 2.0)
    }
}

fn seconds(ms: u64) -> String {
    format!("{}.{:03}", ms / 1000, ms % 1000)
}

/// The ffmpeg arguments (without the program name) for exporting `input` to `output`.
/// A trim start before `-i` snaps to the keyframe at or before it; `-to` is then relative to it.
/// The video is copied as it is, or with `edit.blur_video` blurred and encoded with `encoder`.
pub fn export_args(input: &Path, output: &Path, edit: &Edit, tracks: &Tracks, encoder: &[&str]) -> Result<Vec<String>> {
    let start = edit.trim_start_ms;
    if let Some(end) = edit.trim_end_ms {
        if end <= start.unwrap_or(0) {
            return Err(AppError::InvalidInput("the trim end must be after the trim start".into()));
        }
    }
    let mut inputs = vec![(0, volume(edit.game_volume))];
    inputs.extend(tracks.mic.map(|t| (t, volume(edit.mic_volume))));
    inputs.extend(tracks.voice.map(|t| (t, volume(edit.voice_volume))));
    inputs.extend(tracks.extra.iter().map(|t| (*t, volume(edit.extra_volumes.get(t).copied().unwrap_or(1.0)))));
    // The master volume only appears when it changes something, so default exports stay as before.
    let master = volume(edit.master_volume);
    let master = if master == 1.0 { String::new() } else { format!(",volume={master}") };
    let graph = if inputs.len() == 1 {
        format!("[0:a:0]volume={}{master}[aout]", inputs[0].1)
    } else {
        let filters: Vec<String> =
            inputs.iter().enumerate().map(|(i, (track, vol))| format!("[0:a:{track}]volume={vol}[a{i}]")).collect();
        let labels: String = (0..inputs.len()).map(|i| format!("[a{i}]")).collect();
        format!("{};{labels}amix=inputs={}:normalize=0{master}[aout]", filters.join(";"), inputs.len())
    };

    let mut args: Vec<String> = ["-hide_banner", "-nostdin", "-y"].map(String::from).into();
    if let Some(start) = start {
        args.extend(["-ss".into(), seconds(start)]);
    }
    args.extend(["-i".into(), input.to_string_lossy().into_owned()]);
    if let Some(end) = edit.trim_end_ms {
        args.extend(["-to".into(), seconds(end - start.unwrap_or(0))]);
    }
    let graph = if edit.blur_video {
        format!("[0:v:0]scale=-2:'min({BLUR_MAX_HEIGHT},ih)',gblur=sigma={BLUR_SIGMA}[vout];{graph}")
    } else {
        graph
    };
    args.extend(["-filter_complex".into(), graph]);
    if edit.blur_video {
        args.extend(["-map", "[vout]"].map(String::from));
        args.extend(encoder.iter().map(|s| s.to_string()));
        // Like imported copies: a keyframe each second keeps trimming and seeking exact.
        args.extend(["-pix_fmt", "yuv420p", "-force_key_frames", "expr:gte(t,n_forced*1)"].map(String::from));
    } else {
        args.extend(["-map", "0:v:0", "-c:v", "copy"].map(String::from));
    }
    args.extend(
        ["-map", "[aout]", "-c:a", "aac", "-b:a", "160k", "-movflags", "+faststart", "-f", "mp4", "-progress", "pipe:1", "-nostats"]
            .map(String::from),
    );
    args.push(output.to_string_lossy().into_owned());
    Ok(args)
}

/// Exports to `<cache_dir>/exports/<clip_id>.mp4` and returns that path. `progress` gets 0..=1.
pub fn export(
    tools: &Tools,
    input: &Path,
    cache_dir: &Path,
    clip_id: &str,
    edit: &Edit,
    tracks: Tracks,
    mut progress: impl FnMut(f32),
) -> Result<PathBuf> {
    let info = probe(tools, input)?;
    // Never fail on a missing mic or voice track; never mix a track that isn't there.
    let tracks = tracks.present_in(info.audio_tracks);

    let start = edit.trim_start_ms.unwrap_or(0);
    let end = edit.trim_end_ms.unwrap_or(info.duration_ms).min(info.duration_ms.max(1));
    let expected_us = (end.saturating_sub(start).max(1) as f64) * 1000.0;

    let dir = cache_dir.join("exports");
    fs::create_dir_all(&dir)?;
    let out = dir.join(format!("{clip_id}.mp4"));
    let temp = dir.join(format!("{clip_id}.tmp.mp4"));

    let encoder = if edit.blur_video {
        let found = crate::import::detect_encoder(tools)
            .ok_or_else(|| AppError::ToolFailed { tool: "ffmpeg", detail: "no H.264 encoder to blur the video with".into() })?;
        found.args
    } else {
        Vec::new()
    };
    let args = export_args(input, &temp, edit, &tracks, &encoder)?;
    let result = run(tools, &args, expected_us, &mut progress).and_then(|_| match fs::metadata(&temp) {
        Ok(meta) if meta.len() > 0 => Ok(fs::rename(&temp, &out)?),
        _ => Err(AppError::ToolFailed { tool: "ffmpeg", detail: "export produced no output".into() }),
    });
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result?;
    progress(1.0);
    Ok(out)
}

fn run(tools: &Tools, args: &[String], expected_us: f64, progress: &mut impl FnMut(f32)) -> Result<()> {
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    let mut command = Command::new(&tools.ffmpeg);
    command.args(&args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let fail = |detail: String| AppError::ToolFailed { tool: "ffmpeg", detail };
    let mut child = command.spawn().map_err(|e| fail(format!("can't start {}: {e}", tools.ffmpeg.display())))?;
    // Drain stderr on its own thread so a full pipe can't stall ffmpeg.
    let mut stderr = child.stderr.take().expect("piped");
    let stderr_reader = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text);
        text
    });
    let stdout = child.stdout.take().expect("piped");
    for line in BufReader::new(stdout).lines().map_while(|l| l.ok()) {
        // out_time_us (and the misnamed out_time_ms) are both microseconds.
        let us = line.strip_prefix("out_time_us=").or_else(|| line.strip_prefix("out_time_ms="));
        if let Some(us) = us.and_then(|v| v.trim().parse::<f64>().ok()) {
            progress(((us / expected_us) as f32).clamp(0.0, 0.99));
        }
    }
    let status = child.wait().map_err(|e| fail(e.to_string()))?;
    let stderr = stderr_reader.join().unwrap_or_default();
    if status.success() {
        return Ok(());
    }
    let tail: Vec<&str> = stderr.lines().rev().take(5).collect();
    Err(fail(format!("exited with {status}: {}", tail.into_iter().rev().collect::<Vec<_>>().join(" | "))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffmpeg::tests::{fixture, tools};
    use std::io::Read;

    const MIC: Tracks = Tracks { mic: Some(1), voice: None, extra: Vec::new() };

    #[test]
    fn args_with_mic_and_voice_mix_three_tracks() {
        let tracks = Tracks { mic: Some(1), voice: Some(2), extra: vec![] };
        let joined = export_args(Path::new("i"), Path::new("o"), &edit(None, None), &tracks, &[]).unwrap().join(" ");
        assert!(joined.contains(
            "[0:a:0]volume=1[a0];[0:a:1]volume=0.5[a1];[0:a:2]volume=0.25[a2];[a0][a1][a2]amix=inputs=3:normalize=0[aout]"
        ), "{joined}");
        let voice_only = Tracks { mic: None, voice: Some(1), extra: vec![] };
        let joined = export_args(Path::new("i"), Path::new("o"), &edit(None, None), &voice_only, &[]).unwrap().join(" ");
        assert!(joined.contains("[0:a:1]volume=0.25[a1];[a0][a1]amix=inputs=2"), "{joined}");
    }

    #[test]
    fn master_volume_scales_the_mix() {
        let e = Edit { master_volume: 0.5, ..edit(None, None) };
        let joined = export_args(Path::new("i"), Path::new("o"), &e, &MIC, &[]).unwrap().join(" ");
        assert!(joined.contains("amix=inputs=2:normalize=0,volume=0.5[aout]"), "{joined}");
        let joined = export_args(Path::new("i"), Path::new("o"), &e, &Tracks::default(), &[]).unwrap().join(" ");
        assert!(joined.contains("[0:a:0]volume=1,volume=0.5[aout]"), "{joined}");
    }

    #[test]
    fn tracks_the_file_lacks_are_dropped() {
        let both = Tracks { mic: Some(1), voice: Some(2), extra: vec![] };
        assert_eq!(both.clone().present_in(2), MIC);
        assert_eq!(both.present_in(1), Tracks::default());
    }

    #[test]
    fn tracks_klipt_does_not_know_are_extra_tracks() {
        // An imported clip with three tracks and no mic or voice chat on record.
        assert_eq!(Tracks::default().present_in(3), Tracks { mic: None, voice: None, extra: vec![1, 2] });
        // A Klipt recording with game, mic and voice chat has nothing extra.
        let all = Tracks { mic: Some(1), voice: Some(2), extra: vec![] };
        assert_eq!(all.clone().present_in(3), all);
        // A mic on record plus a fourth track nobody knows.
        assert_eq!(MIC.present_in(3), Tracks { mic: Some(1), voice: None, extra: vec![2] });
    }

    #[test]
    fn extra_tracks_are_mixed_at_their_volumes() {
        let tracks = Tracks { mic: None, voice: None, extra: vec![1, 2] };
        let e = Edit { extra_volumes: BTreeMap::from([(2, 0.3)]), ..edit(None, None) };
        let joined = export_args(Path::new("i"), Path::new("o"), &e, &tracks, &[]).unwrap().join(" ");
        assert!(
            joined.contains("[0:a:0]volume=1[a0];[0:a:1]volume=1[a1];[0:a:2]volume=0.3[a2];[a0][a1][a2]amix=inputs=3"),
            "{joined}"
        );
    }

    #[test]
    fn blurred_exports_encode_a_blurred_picture() {
        let e = Edit { blur_video: true, ..edit(None, None) };
        let joined = export_args(Path::new("i"), Path::new("o"), &e, &MIC, &["-c:v", "libx264"]).unwrap().join(" ");
        assert!(joined.contains("[0:v:0]scale=-2:'min(720,ih)',gblur=sigma=8[vout];[0:a:0]volume=1[a0]"), "{joined}");
        assert!(joined.contains("-map [vout] -c:v libx264 -pix_fmt yuv420p -force_key_frames"), "{joined}");
        assert!(!joined.contains("-c:v copy"), "{joined}");
        // Not blurred: the picture is copied as it is, no encoder needed.
        let joined = export_args(Path::new("i"), Path::new("o"), &edit(None, None), &MIC, &[]).unwrap().join(" ");
        assert!(joined.contains("-map 0:v:0 -c:v copy") && !joined.contains("gblur"), "{joined}");
    }

    #[test]
    fn exporting_a_blurred_clip_makes_a_playable_h264_file() {
        let cache = tempfile::tempdir().unwrap();
        let e = Edit { blur_video: true, ..edit(Some(500), Some(2500)) };
        let out = export(&tools(), fixture(), cache.path(), "blur", &e, MIC, |_| {}).unwrap();
        let info = crate::ffmpeg::inspect(&tools(), &out).unwrap();
        assert_eq!(info.video_codec.as_deref(), Some("h264"));
        assert!(info.height <= 720 && info.height > 0, "{info:?}");
        assert!((1800..=2200).contains(&info.duration_ms), "the trim is kept: {info:?}");
        assert_eq!(info.audio_codecs, vec!["aac"]);
    }

    #[test]
    fn exporting_mixes_every_track_of_an_imported_clip() {
        // A file with three audio tracks and nothing on record about them: all three are mixed.
        let cache = tempfile::tempdir().unwrap();
        let three = cache.path().join("three-imported.mp4");
        let status = std::process::Command::new(&tools().ffmpeg)
            .args(["-nostdin", "-loglevel", "error", "-y", "-i"])
            .arg(fixture())
            .args(["-map", "0:v:0", "-map", "0:a:0", "-map", "0:a:1", "-map", "0:a:0", "-c", "copy"])
            .arg(&three)
            .status()
            .unwrap();
        assert!(status.success());
        let e = Edit { extra_volumes: BTreeMap::from([(1, 0.0)]), ..edit(None, None) };
        let out = export(&tools(), &three, cache.path(), "imp", &e, Tracks::default(), |_| {}).unwrap();
        assert_eq!(probe(&tools(), &out).unwrap().audio_tracks, 1);
    }

    fn edit(start: Option<u64>, end: Option<u64>) -> Edit {
        Edit { trim_start_ms: start, trim_end_ms: end, game_volume: 1.0, mic_volume: 0.5, voice_volume: 0.25, master_volume: 1.0, extra_volumes: BTreeMap::new(), blur_video: false }
    }

    #[test]
    fn args_with_a_mic() {
        let args = export_args(Path::new("in.mp4"), Path::new("out.mp4"), &edit(Some(2000), Some(4000)), &MIC, &[]).unwrap();
        assert_eq!(
            args.join(" "),
            "-hide_banner -nostdin -y -ss 2.000 -i in.mp4 -to 2.000 -filter_complex \
             [0:a:0]volume=1[a0];[0:a:1]volume=0.5[a1];[a0][a1]amix=inputs=2:normalize=0[aout] \
             -map 0:v:0 -c:v copy -map [aout] -c:a aac -b:a 160k -movflags +faststart -f mp4 -progress pipe:1 -nostats out.mp4"
        );
    }

    // Review Focus 5: no mic track, the mix comes from one track.
    #[test]
    fn args_without_a_mic_or_trim() {
        let args = export_args(Path::new("in.mp4"), Path::new("out.mp4"), &edit(None, None), &Tracks::default(), &[]).unwrap();
        assert_eq!(
            args.join(" "),
            "-hide_banner -nostdin -y -i in.mp4 -filter_complex [0:a:0]volume=1[aout] \
             -map 0:v:0 -c:v copy -map [aout] -c:a aac -b:a 160k -movflags +faststart -f mp4 -progress pipe:1 -nostats out.mp4"
        );
    }

    #[test]
    fn volumes_are_clamped_and_bad_ranges_rejected() {
        let e = Edit { trim_start_ms: None, trim_end_ms: Some(1500), game_volume: 5.0, mic_volume: -1.0, voice_volume: 1.0, master_volume: 1.0, extra_volumes: BTreeMap::new(), blur_video: false };
        let joined = export_args(Path::new("i"), Path::new("o"), &e, &MIC, &[]).unwrap().join(" ");
        assert!(joined.contains("volume=2[a0]") && joined.contains("volume=0[a1]") && joined.contains("-to 1.500"));
        for (s, e) in [(Some(4000), Some(2000)), (Some(2000), Some(2000)), (None, Some(0))] {
            assert!(matches!(
                export_args(Path::new("i"), Path::new("o"), &edit(s, e), &MIC, &[]),
                Err(AppError::InvalidInput(_))
            ));
        }
    }

    /// Top-level MP4 box types in file order.
    fn top_level_boxes(path: &Path) -> Vec<String> {
        let mut file = fs::File::open(path).unwrap();
        let len = file.metadata().unwrap().len();
        let (mut pos, mut types) = (0u64, vec![]);
        while pos + 8 <= len {
            let mut header = [0u8; 8];
            std::io::Seek::seek(&mut file, std::io::SeekFrom::Start(pos)).unwrap();
            file.read_exact(&mut header).unwrap();
            let mut size = u32::from_be_bytes(header[..4].try_into().unwrap()) as u64;
            types.push(String::from_utf8_lossy(&header[4..]).into_owned());
            if size == 1 {
                let mut big = [0u8; 8];
                file.read_exact(&mut big).unwrap();
                size = u64::from_be_bytes(big);
            } else if size == 0 {
                size = len - pos;
            }
            assert!(size >= 8, "corrupt box");
            pos += size;
        }
        types
    }

    #[test]
    fn exports_a_trimmed_mixed_faststart_clip() {
        let cache = tempfile::tempdir().unwrap();
        let mut reports = vec![];
        let out = export(&tools(), fixture(), cache.path(), "clip-1", &edit(Some(2000), Some(4000)), MIC, |p| {
            reports.push(p)
        })
        .unwrap();
        assert_eq!(out, cache.path().join("exports").join("clip-1.mp4"));
        let p = probe(&tools(), &out).unwrap();
        assert_eq!(p.audio_tracks, 1);
        assert_eq!((p.width, p.height), (320, 240));
        assert!((2_000..=3_100).contains(&p.duration_ms), "{}", p.duration_ms);
        let boxes = top_level_boxes(&out);
        let pos = |name: &str| boxes.iter().position(|b| b == name).unwrap_or_else(|| panic!("no {name} in {boxes:?}"));
        assert!(pos("moov") < pos("mdat"), "{boxes:?}");
        assert_eq!(reports.last(), Some(&1.0));
        assert!(reports.windows(2).all(|w| w[0] <= w[1]), "{reports:?}");
        assert_eq!(fs::read_dir(out.parent().unwrap()).unwrap().count(), 1, "no temp files left");
    }

    #[test]
    fn a_missing_mic_track_still_exports() {
        let cache = tempfile::tempdir().unwrap();
        // One-track source: demux the game audio plus video into a new clip.
        let one = cache.path().join("one.mp4");
        let status = Command::new(&tools().ffmpeg)
            .args(["-nostdin", "-loglevel", "error", "-y", "-i"])
            .arg(fixture())
            .args(["-map", "0:v:0", "-map", "0:a:0", "-c", "copy"])
            .arg(&one)
            .status()
            .unwrap();
        assert!(status.success());
        // has_mic claims a mic that isn't there; no trim at all.
        let out = export(&tools(), &one, cache.path(), "c", &edit(None, None), MIC, |_| {}).unwrap();
        let p = probe(&tools(), &out).unwrap();
        assert_eq!(p.audio_tracks, 1);
        assert!((5_900..=6_200).contains(&p.duration_ms), "{}", p.duration_ms);
    }

    #[test]
    fn exports_game_mic_and_voice_mixed_into_one_track() {
        let cache = tempfile::tempdir().unwrap();
        // Three audio tracks: the fixture's two plus its game track again as "voice".
        let three = cache.path().join("three.mp4");
        let status = Command::new(&tools().ffmpeg)
            .args(["-nostdin", "-loglevel", "error", "-y", "-i"])
            .arg(fixture())
            .args(["-map", "0:v:0", "-map", "0:a:0", "-map", "0:a:1", "-map", "0:a:0", "-c", "copy"])
            .arg(&three)
            .status()
            .unwrap();
        assert!(status.success());
        assert_eq!(probe(&tools(), &three).unwrap().audio_tracks, 3);
        let tracks = Tracks { mic: Some(1), voice: Some(2), extra: vec![] };
        let out = export(&tools(), &three, cache.path(), "v", &edit(Some(1000), Some(3000)), tracks, |_| {}).unwrap();
        assert_eq!(probe(&tools(), &out).unwrap().audio_tracks, 1);
    }

    // Review Focus 4: a missing input is an error and leaves nothing behind.
    #[test]
    fn missing_input_is_an_error() {
        let cache = tempfile::tempdir().unwrap();
        let r = export(&tools(), &cache.path().join("nope.mp4"), cache.path(), "x", &edit(None, None), MIC, |_| {});
        assert!(matches!(r, Err(AppError::ToolFailed { .. })));
        assert!(!cache.path().join("exports").join("x.mp4").exists());
    }
}
