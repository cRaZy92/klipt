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
    /// Parts removed inside the trim (sorted): the kept parts are joined, re-encoded exactly.
    pub removed: Vec<crate::library::CutRange>,
    /// Zooms (sorted, never overlapping), in the file's time: the picture eases in and out.
    pub zooms: Vec<crate::library::ZoomRange>,
    /// Subtitles (sorted, never overlapping), in the file's time: drawn onto the picture.
    pub subtitles: Vec<crate::library::SubtitleRange>,
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

/// How long a zoom takes to ease in (and out): at most 400 ms, at most half of the zoom.
fn zoom_ramp_ms(z: &crate::library::ZoomRange) -> u64 {
    (z.end_ms.saturating_sub(z.start_ms) / 2).clamp(1, 400)
}

/// The video filter chain (no labels) that plays `zooms` on a `size` picture, or `None` without
/// zooms. Each frame is scaled up by the zoom at its time and cropped back to `size`, so the
/// output keeps the source's size. `offset_ms` is the file time of the input's `t = 0` (the trim
/// start when `-ss` is before `-i`). Uses the same easing as the editor's preview.
fn zoom_filter(zooms: &[crate::library::ZoomRange], size: (u32, u32), offset_ms: u64) -> Option<String> {
    if zooms.is_empty() {
        return None;
    }
    let (w, h) = size;
    let t = if offset_ms == 0 { "t".to_string() } else { format!("(t+{})", seconds(offset_ms)) };
    // Per zoom: the eased amount E (0..1, smoothstep of the ramps) and its extra zoom (zoom − 1).
    // Zooms never overlap, so at most one E is above 0 at a time and the terms just add up.
    let mut extra = Vec::new();
    let mut shift_x = Vec::new();
    let mut shift_y = Vec::new();
    for z in zooms {
        let (s, e, r) = (seconds(z.start_ms), seconds(z.end_ms), seconds(zoom_ramp_ms(z)));
        let u = format!("clip(({t}-{s})/{r},0,1)*clip(({e}-{t})/{r},0,1)");
        let ease = format!("({u})*({u})*(3-2*({u}))");
        let k = (z.zoom - 1.0).max(0.0);
        extra.push(format!("{k:.4}*{ease}"));
        shift_x.push(format!("{:.4}*{ease}", w as f32 * k * z.x.clamp(0.0, 1.0)));
        shift_y.push(format!("{:.4}*{ease}", h as f32 * k * z.y.clamp(0.0, 1.0)));
    }
    let zoom = format!("(1+{})", extra.join("+"));
    // Rounded half-up to even: never below the source size, so the crop always fits.
    let scaled_w = format!("trunc({w}*{zoom}/2+0.5)*2");
    let scaled_h = format!("trunc({h}*{zoom}/2+0.5)*2");
    // crop reads `iw`/`ih` from the first frame only, so the slack is worked out from `t` too.
    let x = format!("max(0,min({},{scaled_w}-{w}))", shift_x.join("+"));
    let y = format!("max(0,min({},{scaled_h}-{h}))", shift_y.join("+"));
    Some(format!("scale=w='{scaled_w}':h='{scaled_h}':eval=frame:flags=bicubic,crop={w}:{h}:x='{x}':y='{y}'"))
}

/// The zooms that show in an export: those overlapping the trim.
fn visible_zooms(edit: &Edit) -> Vec<crate::library::ZoomRange> {
    let start = edit.trim_start_ms.unwrap_or(0);
    let end = edit.trim_end_ms.unwrap_or(u64::MAX);
    edit.zooms.iter().filter(|z| z.end_ms > start && z.start_ms < end).copied().collect()
}

/// Whether an export has to encode the picture (instead of copying it).
fn reencodes(edit: &Edit) -> bool {
    edit.blur_video || !edit.removed.is_empty() || !visible_zooms(edit).is_empty()
}

/// The parts of the file an export keeps, in order: the trim minus the removed parts. A trim
/// that runs to the end ends at `duration_ms`.
pub fn kept_ranges(edit: &Edit, duration_ms: u64) -> Vec<(u64, u64)> {
    let mut at = edit.trim_start_ms.unwrap_or(0);
    let end = edit.trim_end_ms.unwrap_or(duration_ms);
    let mut kept = Vec::new();
    for r in &edit.removed {
        if r.start_ms > at {
            kept.push((at, r.start_ms.min(end)));
        }
        at = at.max(r.end_ms);
    }
    if end > at {
        kept.push((at, end));
    }
    kept.retain(|(s, e)| e > s);
    kept
}

/// Where `ms` (a time in the file) lands in the export of `kept`: a time in a removed part moves
/// to the next kept frame, one past the end to the last kept frame.
pub fn joined_time(ms: u64, kept: &[(u64, u64)]) -> u64 {
    let mut before = 0;
    for &(s, e) in kept {
        if ms < e {
            return before + ms.saturating_sub(s);
        }
        before += e - s;
    }
    before.saturating_sub(1)
}

/// The ffmpeg arguments (without the program name) for exporting `input` to `output`.
/// A trim start before `-i` snaps to the keyframe at or before it; `-to` is then relative to it.
/// The video is copied as it is, or (zoomed, blurred) encoded with `encoder`. `size` is the
/// picture's size, which zooms need.
pub fn export_args(
    input: &Path,
    output: &Path,
    edit: &Edit,
    tracks: &Tracks,
    size: (u32, u32),
    encoder: &[&str],
) -> Result<Vec<String>> {
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

    let zooms = visible_zooms(edit);
    if !edit.removed.is_empty() {
        let zoom = zoom_filter(&zooms, size, 0);
        return Ok(cut_args(input, output, edit, &graph, zoom.as_deref(), encoder));
    }
    // `-ss` before `-i` makes the trim start `t = 0`, so the zooms are shifted by it.
    let zoom = zoom_filter(&zooms, size, start.unwrap_or(0));
    let reencode = reencodes(edit);
    let mut args: Vec<String> = ["-hide_banner", "-nostdin", "-y"].map(String::from).into();
    if let Some(start) = start {
        args.extend(["-ss".into(), seconds(start)]);
    }
    args.extend(["-i".into(), input.to_string_lossy().into_owned()]);
    if let Some(end) = edit.trim_end_ms {
        args.extend(["-to".into(), seconds(end - start.unwrap_or(0))]);
    }
    // The picture's filters in order: zoom, then blur (so the blur looks the same zoomed or not).
    let blur = edit.blur_video.then(|| format!("scale=-2:'min({BLUR_MAX_HEIGHT},ih)',gblur=sigma={BLUR_SIGMA}"));
    let video: Vec<String> = zoom.into_iter().chain(blur).collect();
    let graph = if video.is_empty() { graph } else { format!("[0:v:0]{}[vout];{graph}", video.join(",")) };
    args.extend(["-filter_complex".into(), graph]);
    if reencode {
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

/// The arguments for an export with removed parts: every kept part is cut out exactly (`trim`,
/// `atrim` on the mix), the parts are joined with `concat` and the video is encoded with
/// `encoder`. `mix` is the audio graph ending in `[aout]`. `zoom` (in the file's time) runs
/// before the parts are cut out, so a zoom across a removed part plays right on both sides.
fn cut_args(input: &Path, output: &Path, edit: &Edit, mix: &str, zoom: Option<&str>, encoder: &[&str]) -> Vec<String> {
    // No end for a trim running to the file's end: `trim` then stops where the file does.
    let kept = kept_ranges(edit, u64::MAX);
    let n = kept.len();
    let range = |(s, e): (u64, u64)| {
        if e == u64::MAX { format!("start={}", seconds(s)) } else { format!("start={}:end={}", seconds(s), seconds(e)) }
    };
    let mut graph = vec![mix.replace("[aout]", "[amix]")];
    graph.push(format!("[amix]asplit={n}{}", (0..n).map(|i| format!("[am{i}]")).collect::<String>()));
    // Each trim needs its own input: the zoomed picture is split once per kept part.
    let source = |i: usize| if zoom.is_some() { format!("[vz{i}]") } else { "[0:v:0]".to_string() };
    if let Some(zoom) = zoom {
        graph.push(format!("[0:v:0]{zoom},split={n}{}", (0..n).map(|i| format!("[vz{i}]")).collect::<String>()));
    }
    let mut pairs = String::new();
    for (i, k) in kept.iter().enumerate() {
        graph.push(format!("{}trim={},setpts=PTS-STARTPTS[v{i}]", source(i), range(*k)));
        graph.push(format!("[am{i}]atrim={},asetpts=PTS-STARTPTS[a{i}]", range(*k)));
        pairs.push_str(&format!("[v{i}][a{i}]"));
    }
    graph.push(format!("{pairs}concat=n={n}:v=1:a=1[vcat][aout]"));
    graph.push(if edit.blur_video {
        format!("[vcat]scale=-2:'min({BLUR_MAX_HEIGHT},ih)',gblur=sigma={BLUR_SIGMA}[vout]")
    } else {
        "[vcat]null[vout]".to_string()
    });
    let mut args: Vec<String> = ["-hide_banner", "-nostdin", "-y", "-i"].map(String::from).into();
    args.push(input.to_string_lossy().into_owned());
    args.extend(["-filter_complex".into(), graph.join(";"), "-map".into(), "[vout]".into()]);
    args.extend(encoder.iter().map(|s| s.to_string()));
    // Like imported copies: a keyframe each second keeps trimming and seeking exact.
    args.extend(["-pix_fmt", "yuv420p", "-force_key_frames", "expr:gte(t,n_forced*1)"].map(String::from));
    args.extend(
        ["-map", "[aout]", "-c:a", "aac", "-b:a", "160k", "-movflags", "+faststart", "-f", "mp4", "-progress", "pipe:1", "-nostats"]
            .map(String::from),
    );
    args.push(output.to_string_lossy().into_owned());
    args
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

    let duration = info.duration_ms.max(1);
    let kept_ms: u64 = kept_ranges(edit, duration).iter().map(|(s, e)| e.min(&duration).saturating_sub(*s)).sum();
    let expected_us = (kept_ms.max(1) as f64) * 1000.0;

    let dir = cache_dir.join("exports");
    fs::create_dir_all(&dir)?;
    let out = dir.join(format!("{clip_id}.mp4"));
    let temp = dir.join(format!("{clip_id}.tmp.mp4"));

    let encoder = if reencodes(edit) {
        let found = crate::import::detect_encoder(tools)
            .ok_or_else(|| AppError::ToolFailed { tool: "ffmpeg", detail: "no H.264 encoder to re-encode the video with".into() })?;
        found.args
    } else {
        Vec::new()
    };
    let args = export_args(input, &temp, edit, &tracks, (info.width, info.height), &encoder)?;
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
    const SIZE: (u32, u32) = (1920, 1080);

    #[test]
    fn args_with_mic_and_voice_mix_three_tracks() {
        let tracks = Tracks { mic: Some(1), voice: Some(2), extra: vec![] };
        let joined = export_args(Path::new("i"), Path::new("o"), &edit(None, None), &tracks, SIZE, &[]).unwrap().join(" ");
        assert!(joined.contains(
            "[0:a:0]volume=1[a0];[0:a:1]volume=0.5[a1];[0:a:2]volume=0.25[a2];[a0][a1][a2]amix=inputs=3:normalize=0[aout]"
        ), "{joined}");
        let voice_only = Tracks { mic: None, voice: Some(1), extra: vec![] };
        let joined = export_args(Path::new("i"), Path::new("o"), &edit(None, None), &voice_only, SIZE, &[]).unwrap().join(" ");
        assert!(joined.contains("[0:a:1]volume=0.25[a1];[a0][a1]amix=inputs=2"), "{joined}");
    }

    #[test]
    fn master_volume_scales_the_mix() {
        let e = Edit { master_volume: 0.5, ..edit(None, None) };
        let joined = export_args(Path::new("i"), Path::new("o"), &e, &MIC, SIZE, &[]).unwrap().join(" ");
        assert!(joined.contains("amix=inputs=2:normalize=0,volume=0.5[aout]"), "{joined}");
        let joined = export_args(Path::new("i"), Path::new("o"), &e, &Tracks::default(), SIZE, &[]).unwrap().join(" ");
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
        let joined = export_args(Path::new("i"), Path::new("o"), &e, &tracks, SIZE, &[]).unwrap().join(" ");
        assert!(
            joined.contains("[0:a:0]volume=1[a0];[0:a:1]volume=1[a1];[0:a:2]volume=0.3[a2];[a0][a1][a2]amix=inputs=3"),
            "{joined}"
        );
    }

    #[test]
    fn blurred_exports_encode_a_blurred_picture() {
        let e = Edit { blur_video: true, ..edit(None, None) };
        let joined = export_args(Path::new("i"), Path::new("o"), &e, &MIC, SIZE, &["-c:v", "libx264"]).unwrap().join(" ");
        assert!(joined.contains("[0:v:0]scale=-2:'min(720,ih)',gblur=sigma=8[vout];[0:a:0]volume=1[a0]"), "{joined}");
        assert!(joined.contains("-map [vout] -c:v libx264 -pix_fmt yuv420p -force_key_frames"), "{joined}");
        assert!(!joined.contains("-c:v copy"), "{joined}");
        // Not blurred: the picture is copied as it is, no encoder needed.
        let joined = export_args(Path::new("i"), Path::new("o"), &edit(None, None), &MIC, SIZE, &[]).unwrap().join(" ");
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
        Edit { trim_start_ms: start, trim_end_ms: end, game_volume: 1.0, mic_volume: 0.5, voice_volume: 0.25, master_volume: 1.0, extra_volumes: BTreeMap::new(), blur_video: false, removed: Vec::new(), zooms: Vec::new(), subtitles: Vec::new() }
    }

    #[test]
    fn args_with_a_mic() {
        let args = export_args(Path::new("in.mp4"), Path::new("out.mp4"), &edit(Some(2000), Some(4000)), &MIC, SIZE, &[]).unwrap();
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
        let args = export_args(Path::new("in.mp4"), Path::new("out.mp4"), &edit(None, None), &Tracks::default(), SIZE, &[]).unwrap();
        assert_eq!(
            args.join(" "),
            "-hide_banner -nostdin -y -i in.mp4 -filter_complex [0:a:0]volume=1[aout] \
             -map 0:v:0 -c:v copy -map [aout] -c:a aac -b:a 160k -movflags +faststart -f mp4 -progress pipe:1 -nostats out.mp4"
        );
    }

    #[test]
    fn volumes_are_clamped_and_bad_ranges_rejected() {
        let e = Edit { trim_start_ms: None, trim_end_ms: Some(1500), game_volume: 5.0, mic_volume: -1.0, voice_volume: 1.0, master_volume: 1.0, extra_volumes: BTreeMap::new(), blur_video: false, removed: Vec::new(), zooms: Vec::new(), subtitles: Vec::new() };
        let joined = export_args(Path::new("i"), Path::new("o"), &e, &MIC, SIZE, &[]).unwrap().join(" ");
        assert!(joined.contains("volume=2[a0]") && joined.contains("volume=0[a1]") && joined.contains("-to 1.500"));
        for (s, e) in [(Some(4000), Some(2000)), (Some(2000), Some(2000)), (None, Some(0))] {
            assert!(matches!(
                export_args(Path::new("i"), Path::new("o"), &edit(s, e), &MIC, SIZE, &[]),
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

    fn cut(start: u64, end: u64) -> crate::library::CutRange {
        crate::library::CutRange { start_ms: start, end_ms: end }
    }

    #[test]
    fn kept_ranges_subtract_the_removed_parts() {
        let e = Edit { removed: vec![cut(1000, 2000), cut(4000, 5000)], ..edit(Some(1000), Some(9000)) };
        assert_eq!(kept_ranges(&e, 10_000), vec![(2000, 4000), (5000, 9000)]);
        assert_eq!(kept_ranges(&edit(None, None), 10_000), vec![(0, 10_000)]);
    }

    #[test]
    fn cut_exports_trim_and_concat() {
        let e = Edit { removed: vec![cut(2000, 3000)], ..edit(Some(1000), Some(5000)) };
        let args = export_args(Path::new("i"), Path::new("o"), &e, &MIC, SIZE, &["-c:v", "libx264"]).unwrap();
        let joined = args.join(" ");
        for part in [
            "[0:v:0]trim=start=1.000:end=2.000,setpts=PTS-STARTPTS[v0]",
            "[0:v:0]trim=start=3.000:end=5.000,setpts=PTS-STARTPTS[v1]",
            "atrim=start=1.000:end=2.000,asetpts=PTS-STARTPTS[a0]",
            "concat=n=2:v=1:a=1",
            "-c:v libx264",
            "-force_key_frames",
        ] {
            assert!(joined.contains(part), "{part} missing: {joined}");
        }
        assert!(!args.iter().any(|a| a == "-ss" || a == "-to") && !joined.contains("-c:v copy"), "{joined}");
    }

    #[test]
    fn cut_and_blur_blurs_the_joined_video() {
        let e = Edit { removed: vec![cut(2000, 3000)], blur_video: true, ..edit(Some(1000), Some(5000)) };
        let joined = export_args(Path::new("i"), Path::new("o"), &e, &MIC, SIZE, &["-c:v", "libx264"]).unwrap().join(" ");
        let concat = joined.find("concat=").unwrap();
        assert!(joined.find("gblur=sigma=8").is_some_and(|g| g > concat), "{joined}");
    }

    fn zoom(start_ms: u64, end_ms: u64, zoom: f32, x: f32, y: f32) -> crate::library::ZoomRange {
        crate::library::ZoomRange { start_ms, end_ms, zoom, x, y }
    }

    #[test]
    fn zoom_filter_is_none_without_zooms() {
        assert_eq!(zoom_filter(&[], SIZE, 0), None);
    }

    #[test]
    fn zoom_filter_scales_per_frame_and_crops_by_time() {
        let f = zoom_filter(&[zoom(1000, 3000, 2.0, 0.5, 0.5)], SIZE, 0).unwrap();
        assert!(f.starts_with("scale=") && f.contains("eval=frame") && f.contains("crop=1920:1080:"), "{f}");
        let crop = &f[f.find("crop=").unwrap()..];
        assert!(!crop.contains("iw") && !crop.contains("ih"), "crop reads iw/ih from the first frame only: {crop}");
        assert!(f.contains("clip((t-1.000)/0.400,0,1)*clip((3.000-t)/0.400,0,1)"), "{f}");
        // A 600 ms zoom eases over half its length.
        let f = zoom_filter(&[zoom(1000, 1600, 2.0, 0.5, 0.5)], SIZE, 0).unwrap();
        assert!(f.contains("/0.300,"), "{f}");
    }

    #[test]
    fn zoom_filter_offsets_time_by_the_trim_start() {
        let f = zoom_filter(&[zoom(5000, 8000, 2.0, 0.5, 0.5)], SIZE, 4000).unwrap();
        assert!(f.contains("clip(((t+4.000)-5.000)/0.400,0,1)"), "{f}");
        assert!(!f.contains("(t-"), "{f}");
    }

    #[test]
    fn odd_and_wide_sizes_stay_even_and_inside() {
        for (w, h) in [(1366, 768), (2560, 1080)] {
            let f = zoom_filter(&[zoom(0, 2000, 3.0, 1.0, 1.0)], (w, h), 0).unwrap();
            assert!(f.contains(&format!("w='trunc({w}*")) && f.contains(&format!("h='trunc({h}*")), "{f}");
            assert!(f.contains("/2+0.5)*2"), "{f}");
            assert!(f.contains(&format!("crop={w}:{h}:x='max(0,min(")), "{f}");
        }
    }

    #[test]
    fn exporting_a_zoom_reencodes() {
        let enc = ["-c:v", "libx264"];
        let e = Edit { zooms: vec![zoom(1000, 3000, 2.0, 0.5, 0.5)], ..edit(None, None) };
        let joined = export_args(Path::new("i"), Path::new("o"), &e, &MIC, SIZE, &enc).unwrap().join(" ");
        assert!(joined.contains("[0:v:0]scale=w=") && joined.contains("-map [vout] -c:v libx264"), "{joined}");
        assert!(!joined.contains("-c:v copy"), "{joined}");
        // With cuts: zoomed first (in the file's time), then split into the kept parts.
        let cut_e = Edit { removed: vec![cut(1500, 2000)], ..e.clone() };
        let joined = export_args(Path::new("i"), Path::new("o"), &cut_e, &MIC, SIZE, &enc).unwrap().join(" ");
        assert!(joined.contains("eval=frame") && joined.contains(",split=2[vz0][vz1]"), "{joined}");
        assert!(joined.contains("[vz0]trim=start=0.000:end=1.500") && joined.contains("[vz1]trim=start=2.000"), "{joined}");
        assert!(joined.contains("clip((t-1.000)"), "no trim offset in the cut path: {joined}");
        assert!(!joined.contains("-c:v copy"), "{joined}");
        // With blur: the zoom comes first.
        for blurred in [Edit { blur_video: true, ..e.clone() }, Edit { blur_video: true, ..cut_e }] {
            let joined = export_args(Path::new("i"), Path::new("o"), &blurred, &MIC, SIZE, &enc).unwrap().join(" ");
            assert!(joined.find("eval=frame").unwrap() < joined.find("gblur").unwrap(), "{joined}");
        }
        // A zoom outside the trim doesn't show, so the picture is still copied.
        let outside = Edit { zooms: vec![zoom(5000, 6000, 2.0, 0.5, 0.5)], ..edit(Some(1000), Some(4000)) };
        let joined = export_args(Path::new("i"), Path::new("o"), &outside, &MIC, SIZE, &[]).unwrap().join(" ");
        assert!(joined.contains("-c:v copy") && !joined.contains("eval=frame"), "{joined}");
    }

    /// A `w`×`h` test pattern with moving parts and one audio track, a keyframe each second.
    fn pattern_clip(dir: &Path, name: &str, (w, h): (u32, u32), seconds: u32) -> PathBuf {
        let out = dir.join(name);
        let status = Command::new(&tools().ffmpeg)
            .args(["-nostdin", "-loglevel", "error", "-y", "-f", "lavfi", "-i"])
            .arg(format!("testsrc2=s={w}x{h}:r=30:d={seconds}"))
            .args(["-f", "lavfi", "-i"])
            .arg(format!("sine=d={seconds}"))
            .args(["-pix_fmt", "yuv420p", "-g", "30", "-shortest"])
            .arg(&out)
            .status()
            .unwrap();
        assert!(status.success());
        out
    }

    /// The frame of `path` at `at` seconds as small grey pixels, after `vf` (on the full frame).
    fn frame(path: &Path, at: f32, vf: &str) -> Vec<u8> {
        let vf = if vf.is_empty() { String::new() } else { format!("{vf},") };
        let out = Command::new(&tools().ffmpeg)
            .args(["-nostdin", "-loglevel", "error", "-ss", &format!("{at:.3}"), "-i"])
            .arg(path)
            .args(["-frames:v", "1", "-vf", &format!("{vf}scale=160:90,format=gray"), "-f", "rawvideo", "-"])
            .output()
            .unwrap();
        assert!(out.status.success() && out.stdout.len() == 160 * 90, "{}", String::from_utf8_lossy(&out.stderr));
        out.stdout
    }

    fn mean_diff(a: &[u8], b: &[u8]) -> f64 {
        a.iter().zip(b).map(|(x, y)| (*x as f64 - *y as f64).abs()).sum::<f64>() / a.len() as f64
    }

    // Review Focus 1: with a trim start (`-ss`), the zoom lands at the same moment of the clip.
    #[test]
    fn a_zoomed_export_zooms_the_right_moment() {
        let dir = tempfile::tempdir().unwrap();
        let src = pattern_clip(dir.path(), "src.mp4", (640, 360), 4);
        // Top-right quarter, 2×, from 2.0 s to 3.5 s; the trim starts at 1 s.
        let e = Edit { zooms: vec![zoom(2000, 3500, 2.0, 1.0, 0.0)], ..edit(Some(1000), None) };
        let out = export(&tools(), &src, dir.path(), "zoom", &e, Tracks::default(), |_| {}).unwrap();
        assert_eq!((probe(&tools(), &out).unwrap().width, probe(&tools(), &out).unwrap().height), (640, 360));

        let before = mean_diff(&frame(&out, 0.3, ""), &frame(&src, 1.3, ""));
        assert!(before < 6.0, "before the zoom the picture is untouched: {before}");
        let zoomed = frame(&out, 1.8, "");
        let plain = mean_diff(&zoomed, &frame(&src, 2.8, ""));
        let corner = mean_diff(&zoomed, &frame(&src, 2.8, "crop=320:180:320:0"));
        assert!(plain > 15.0, "zoomed at 2.8 s of the file: {plain}");
        assert!(corner < 8.0, "it shows the top-right quarter: {corner}");
    }

    // Review Focus 2: a zoom across a removed part plays right on both sides of the cut.
    #[test]
    fn a_zoom_across_a_cut_plays_on_both_sides() {
        let dir = tempfile::tempdir().unwrap();
        let src = pattern_clip(dir.path(), "src.mp4", (640, 360), 4);
        // Bottom-left quarter, 2×, 0.5–3.0 s; 1.0–2.0 s is removed.
        let e = Edit { removed: vec![cut(1000, 2000)], zooms: vec![zoom(500, 3000, 2.0, 0.0, 1.0)], ..edit(None, None) };
        let out = export(&tools(), &src, dir.path(), "zoomcut", &e, Tracks::default(), |_| {}).unwrap();
        let corner = "crop=320:180:0:180";
        let before_cut = mean_diff(&frame(&out, 0.95, ""), &frame(&src, 0.95, corner));
        let after_cut = mean_diff(&frame(&out, 1.5, ""), &frame(&src, 2.5, corner));
        let after_zoom = mean_diff(&frame(&out, 2.5, ""), &frame(&src, 3.5, ""));
        assert!(before_cut < 8.0 && after_cut < 8.0, "{before_cut} {after_cut}");
        assert!(after_zoom < 6.0, "{after_zoom}");
    }

    // Review Focus 3: a 1366×768 import exports zoomed at its own size.
    #[test]
    fn a_zoomed_odd_sized_clip_exports() {
        let dir = tempfile::tempdir().unwrap();
        let src = pattern_clip(dir.path(), "src.mp4", (1366, 768), 2);
        let e = Edit { zooms: vec![zoom(200, 1800, 3.3, 0.7, 0.9)], ..edit(None, None) };
        let out = export(&tools(), &src, dir.path(), "odd", &e, Tracks::default(), |_| {}).unwrap();
        let p = probe(&tools(), &out).unwrap();
        assert_eq!((p.width, p.height), (1366, 768));
    }

    #[test]
    fn exporting_with_cuts_keeps_only_the_kept_time() {
        let cache = tempfile::tempdir().unwrap();
        let e = Edit { removed: vec![cut(1500, 3500)], ..edit(Some(500), Some(5500)) };
        let mut reports = vec![];
        let out = export(&tools(), fixture(), cache.path(), "cuts", &e, MIC, |p| reports.push(p)).unwrap();
        let p = probe(&tools(), &out).unwrap();
        assert!((2_800..=3_200).contains(&p.duration_ms), "{}", p.duration_ms);
        assert_eq!(p.audio_tracks, 1);
        let info = crate::ffmpeg::inspect(&tools(), &out).unwrap();
        assert_eq!(info.video_codec.as_deref(), Some("h264"));
        assert_eq!(reports.last(), Some(&1.0));
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
