//! Finding and running ffmpeg/ffprobe: probing clips, thumbnails, and pulling out one audio track.
//! Windows ships them as sidecars next to the app's exe; Linux uses the system copies on PATH.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde::{Deserialize, Serialize};

use crate::errors::{AppError, Result};

/// Width of library thumbnails; the height follows the aspect ratio.
const THUMB_WIDTH: u32 = 480;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tools {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Probe {
    pub duration_ms: u64,
    pub width: u32,
    pub height: u32,
    pub audio_tracks: u32,
}

/// Finds ffmpeg and ffprobe for this install.
pub fn locate() -> Result<Tools> {
    // Tauri puts bundled sidecars next to the app's exe (Windows only; Linux uses the system ffmpeg).
    let exe_dir = if cfg!(windows) {
        std::env::current_exe().ok().and_then(|exe| exe.parent().map(Path::to_path_buf))
    } else {
        None
    };
    locate_in(exe_dir.as_deref(), std::env::var_os("PATH").as_deref()).ok_or(AppError::ToolsMissing)
}

/// `exe_dir` (the bundled sidecars) is searched before `path_var` (the PATH value).
pub fn locate_in(exe_dir: Option<&Path>, path_var: Option<&OsStr>) -> Option<Tools> {
    let dirs: Vec<PathBuf> = exe_dir
        .map(Path::to_path_buf)
        .into_iter()
        .chain(path_var.into_iter().flat_map(std::env::split_paths))
        .collect();
    let find = |name: &str| {
        let file = if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
        dirs.iter().map(|dir| dir.join(&file)).find(|candidate| candidate.is_file())
    };
    Some(Tools { ffmpeg: find("ffmpeg")?, ffprobe: find("ffprobe")? })
}

pub fn probe(tools: &Tools, input: &Path) -> Result<Probe> {
    #[derive(Deserialize)]
    struct Output {
        #[serde(default)]
        streams: Vec<Stream>,
        format: Option<Format>,
    }
    #[derive(Deserialize)]
    struct Stream {
        codec_type: Option<String>,
        width: Option<u32>,
        height: Option<u32>,
        duration: Option<String>,
    }
    #[derive(Deserialize)]
    struct Format {
        duration: Option<String>,
    }
    fn seconds_to_ms(s: &Option<String>) -> Option<u64> {
        let seconds: f64 = s.as_deref()?.parse().ok()?;
        (seconds.is_finite() && seconds >= 0.0).then(|| (seconds * 1000.0).round() as u64)
    }

    let mut args = os_args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams"]);
    args.push(input.into());
    let output = run("ffprobe", &tools.ffprobe, &args)?;
    let parsed: Output = serde_json::from_slice(&output.stdout)
        .map_err(|e| AppError::ToolFailed { tool: "ffprobe", detail: format!("unexpected output: {e}") })?;

    let video = parsed.streams.iter().find(|s| s.codec_type.as_deref() == Some("video"));
    let duration_ms = parsed
        .format
        .as_ref()
        .and_then(|f| seconds_to_ms(&f.duration))
        .or_else(|| parsed.streams.iter().filter_map(|s| seconds_to_ms(&s.duration)).max())
        .unwrap_or(0);
    Ok(Probe {
        duration_ms,
        width: video.and_then(|v| v.width).unwrap_or(0),
        height: video.and_then(|v| v.height).unwrap_or(0),
        audio_tracks: parsed.streams.iter().filter(|s| s.codec_type.as_deref() == Some("audio")).count() as u32,
    })
}

/// What an import needs to know about a file: whether it's a picture or a video, and its codecs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MediaInfo {
    /// ffprobe's container name, e.g. `mov,mp4,m4a,3gp,3g2,mj2`, `matroska,webm`, `png_pipe`.
    pub format_name: String,
    pub video_codec: Option<String>,
    pub pix_fmt: Option<String>,
    /// Every audio track's codec, in order.
    pub audio_codecs: Vec<String>,
    pub duration_ms: u64,
    pub width: u32,
    pub height: u32,
    /// Frames per second (0 when unknown).
    pub fps: f64,
}

impl MediaInfo {
    /// A still picture: ffmpeg reads those with `image2` or a `<codec>_pipe` demuxer.
    pub fn is_image(&self) -> bool {
        self.format_name == "image2" || self.format_name.ends_with("_pipe")
    }
}

/// Probes `input` for importing. Fails when ffprobe can't read it (not a media file).
pub fn inspect(tools: &Tools, input: &Path) -> Result<MediaInfo> {
    #[derive(Deserialize)]
    struct Output {
        #[serde(default)]
        streams: Vec<Stream>,
        format: Option<Format>,
    }
    #[derive(Deserialize)]
    struct Stream {
        codec_type: Option<String>,
        codec_name: Option<String>,
        pix_fmt: Option<String>,
        width: Option<u32>,
        height: Option<u32>,
        avg_frame_rate: Option<String>,
    }
    #[derive(Deserialize)]
    struct Format {
        format_name: Option<String>,
        duration: Option<String>,
    }
    let mut args = os_args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams"]);
    args.push(input.into());
    let output = run("ffprobe", &tools.ffprobe, &args)?;
    let parsed: Output = serde_json::from_slice(&output.stdout)
        .map_err(|e| AppError::ToolFailed { tool: "ffprobe", detail: format!("unexpected output: {e}") })?;
    let video = parsed.streams.iter().find(|s| s.codec_type.as_deref() == Some("video"));
    let format = parsed.format.as_ref();
    let duration_ms = format
        .and_then(|f| f.duration.as_deref())
        .and_then(|d| d.parse::<f64>().ok())
        .filter(|d| d.is_finite() && *d >= 0.0)
        .map_or(0, |d| (d * 1000.0).round() as u64);
    Ok(MediaInfo {
        format_name: format.and_then(|f| f.format_name.clone()).unwrap_or_default(),
        video_codec: video.and_then(|v| v.codec_name.clone()),
        pix_fmt: video.and_then(|v| v.pix_fmt.clone()),
        audio_codecs: parsed
            .streams
            .iter()
            .filter(|s| s.codec_type.as_deref() == Some("audio"))
            .map(|s| s.codec_name.clone().unwrap_or_default())
            .collect(),
        duration_ms,
        width: video.and_then(|v| v.width).unwrap_or(0),
        height: video.and_then(|v| v.height).unwrap_or(0),
        fps: video.and_then(|v| v.avg_frame_rate.as_deref()).map_or(0.0, parse_rate),
    })
}

/// `60/1`, `30000/1001` → frames per second; 0 for `0/0` or junk.
pub fn parse_rate(rate: &str) -> f64 {
    let (n, d) = rate.split_once('/').unwrap_or((rate, "1"));
    match (n.trim().parse::<f64>(), d.trim().parse::<f64>()) {
        (Ok(n), Ok(d)) if d > 0.0 && n.is_finite() && n > 0.0 => n / d,
        _ => 0.0,
    }
}

/// Like `write_mp4`, but reports 0..=1 through `progress` as ffmpeg works through
/// `duration_ms` of input, and stops (with an error, leaving nothing behind) once `stop` is set.
pub fn write_mp4_progress(
    tools: &Tools,
    args: &[String],
    out: &Path,
    duration_ms: u64,
    stop: &std::sync::atomic::AtomicBool,
    mut progress: impl FnMut(f64),
) -> Result<()> {
    use std::io::{BufRead, BufReader, Read};
    use std::sync::atomic::Ordering;
    if let Some(dir) = out.parent() {
        fs::create_dir_all(dir)?;
    }
    let temp = out.with_extension("tmp.mp4");
    let mut command = Command::new(&tools.ffmpeg);
    command.args(args).args(["-progress", "pipe:1", "-nostats"]).arg(&temp);
    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command
        .spawn()
        .map_err(|e| AppError::ToolFailed { tool: "ffmpeg", detail: format!("can't start {}: {e}", tools.ffmpeg.display()) })?;
    // stderr on its own thread, so a chatty ffmpeg can't block on a full pipe.
    let mut stderr = child.stderr.take().expect("piped");
    let stderr_reader = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text);
        text
    });
    let mut stopped = false;
    for line in BufReader::new(child.stdout.take().expect("piped")).lines().map_while(std::result::Result::ok) {
        if stop.load(Ordering::SeqCst) {
            stopped = true;
            let _ = child.kill();
            break;
        }
        if let Some(us) = line.strip_prefix("out_time_us=").and_then(|v| v.trim().parse::<u64>().ok()) {
            if duration_ms > 0 {
                progress((us as f64 / 1000.0 / duration_ms as f64).min(1.0));
            }
        }
    }
    let status = child.wait()?;
    let stderr = stderr_reader.join().unwrap_or_default();
    let written = fs::metadata(&temp).map(|m| m.len() > 0).unwrap_or(false);
    if stopped || stop.load(Ordering::SeqCst) {
        let _ = fs::remove_file(&temp);
        return Err(AppError::ToolFailed { tool: "ffmpeg", detail: "stopped".into() });
    }
    if !status.success() || !written {
        let _ = fs::remove_file(&temp);
        let tail: Vec<&str> = stderr.lines().rev().take(5).collect();
        return Err(AppError::ToolFailed {
            tool: "ffmpeg",
            detail: format!("exited with {status}: {}", tail.into_iter().rev().collect::<Vec<_>>().join(" | ")),
        });
    }
    fs::rename(&temp, out)?;
    progress(1.0);
    Ok(())
}

/// Whether ffmpeg can encode with `encoder_args` here (e.g. an NVIDIA encoder without an NVIDIA
/// GPU fails): a fraction of a second of test picture, thrown away.
pub fn can_encode(tools: &Tools, encoder_args: &[&str]) -> bool {
    let mut args = os_args(["-nostdin", "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", "testsrc2=size=640x360:rate=30", "-t", "0.3"]);
    args.extend(os_args(encoder_args.iter().copied()));
    args.extend(os_args(["-pix_fmt", "yuv420p", "-f", "null", "-"]));
    run("ffmpeg", &tools.ffmpeg, &args).is_ok()
}

/// Runs ffmpeg with `args` (everything but the output path) writing `out` atomically: a failed
/// run leaves nothing behind. Used for import conversions.
pub fn write_mp4(tools: &Tools, args: &[String], out: &Path) -> Result<()> {
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    write_via_temp(tools, args, out, "tmp.mp4", "ffmpeg wrote nothing")
}

/// Writes a JPEG frame from `at_ms` into `out` (480 px wide), replacing it atomically.
pub fn thumbnail(tools: &Tools, input: &Path, out: &Path, at_ms: u64) -> Result<()> {
    let mut args = os_args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y", "-ss"]);
    args.push(format!("{:.3}", at_ms as f64 / 1000.0).into());
    args.push("-i".into());
    args.push(input.into());
    args.extend(os_args(["-frames:v", "1", "-vf", &format!("scale={THUMB_WIDTH}:-2"), "-q:v", "4", "-update", "1"]));
    write_via_temp(tools, args, out, "tmp.jpg", &format!("no video frame at {at_ms} ms"))
}

/// Copies audio track `track_index` (0 = game, 1 = mic) into an `.m4a` without re-encoding.
pub fn demux_audio(tools: &Tools, input: &Path, track_index: u32, out: &Path) -> Result<()> {
    let mut args = os_args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y", "-i"]);
    args.push(input.into());
    args.extend(os_args(["-map", &format!("0:a:{track_index}"), "-vn", "-c", "copy", "-f", "ipod"]));
    write_via_temp(tools, args, out, "tmp.m4a", &format!("no audio track {track_index}"))
}

/// Runs ffmpeg with `args` plus a temp output path next to `out`, then renames the temp file over
/// `out`, so a failed or interrupted run never leaves a half-written file behind.
fn write_via_temp(
    tools: &Tools,
    mut args: Vec<OsString>,
    out: &Path,
    temp_ext: &str,
    empty_detail: &str,
) -> Result<()> {
    if let Some(dir) = out.parent() {
        fs::create_dir_all(dir)?;
    }
    let temp = out.with_extension(temp_ext);
    args.push(temp.clone().into());
    let result = run("ffmpeg", &tools.ffmpeg, &args).and_then(|_| {
        // ffmpeg exits 0 when it had nothing to write (e.g. a seek past the end).
        match fs::metadata(&temp) {
            Ok(meta) if meta.len() > 0 => Ok(fs::rename(&temp, out)?),
            _ => Err(AppError::ToolFailed { tool: "ffmpeg", detail: empty_detail.to_string() }),
        }
    });
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn os_args<'a>(args: impl IntoIterator<Item = &'a str>) -> Vec<OsString> {
    args.into_iter().map(OsString::from).collect()
}

/// Runs a tool without a console window and returns its output if it exited successfully.
fn run(tool: &'static str, program: &Path, args: &[OsString]) -> Result<Output> {
    let mut command = Command::new(program);
    command.args(args).stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command
        .output()
        .map_err(|e| AppError::ToolFailed { tool, detail: format!("can't start {}: {e}", program.display()) })?;
    if output.status.success() {
        return Ok(output);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let tail: Vec<&str> = stderr.lines().rev().take(5).collect();
    Err(AppError::ToolFailed {
        tool,
        detail: format!("exited with {}: {}", output.status, tail.into_iter().rev().collect::<Vec<_>>().join(" | ")),
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::fs;
    use std::sync::OnceLock;

    fn exe(name: &str) -> String {
        if cfg!(windows) {
            format!("{name}.exe")
        } else {
            name.to_string()
        }
    }

    pub(crate) fn tools() -> Tools {
        locate().expect("ffmpeg and ffprobe must be installed to run these tests (see docs/WINDOWS-DEV.md)")
    }

    /// A 6 s clip shaped like the recorder's (keyframe every 1 s): H.264 video, track 1 = game audio, track 2 = mic.
    pub(crate) fn fixture() -> &'static Path {
        static FIXTURE: OnceLock<(tempfile::TempDir, PathBuf)> = OnceLock::new();
        &FIXTURE
            .get_or_init(|| {
                let dir = tempfile::tempdir().unwrap();
                let path = dir.path().join("two-tracks.mp4");
                let status = Command::new(&tools().ffmpeg)
                    .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y"])
                    .args(["-f", "lavfi", "-i", "testsrc2=size=320x240:rate=30"])
                    .args(["-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000"])
                    .args(["-f", "lavfi", "-i", "sine=frequency=880:sample_rate=48000"])
                    .args(["-t", "6", "-map", "0:v", "-map", "1:a", "-map", "2:a"])
                    .args(["-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-g", "30"])
                    .args(["-c:a", "aac", "-b:a", "96k"])
                    .arg(&path)
                    .status()
                    .unwrap();
                assert!(status.success(), "could not generate the test clip");
                (dir, path)
            })
            .1
    }

    #[test]
    fn locate_prefers_the_exe_dir_then_path() {
        let bundled = tempfile::tempdir().unwrap();
        let on_path = tempfile::tempdir().unwrap();
        for dir in [bundled.path(), on_path.path()] {
            fs::write(dir.join(exe("ffmpeg")), b"").unwrap();
            fs::write(dir.join(exe("ffprobe")), b"").unwrap();
        }
        let path_var = std::env::join_paths([on_path.path()]).unwrap();

        let found = locate_in(Some(bundled.path()), Some(&path_var)).unwrap();
        assert_eq!(found.ffmpeg, bundled.path().join(exe("ffmpeg")));
        assert_eq!(found.ffprobe, bundled.path().join(exe("ffprobe")));

        let empty = tempfile::tempdir().unwrap();
        let found = locate_in(Some(empty.path()), Some(&path_var)).unwrap();
        assert_eq!(found.ffmpeg, on_path.path().join(exe("ffmpeg")));

        assert_eq!(locate_in(Some(empty.path()), None), None);
        // Both tools are required.
        fs::remove_file(on_path.path().join(exe("ffprobe"))).unwrap();
        assert_eq!(locate_in(None, Some(&path_var)), None);
    }

    #[test]
    fn probe_reports_size_duration_and_both_audio_tracks() {
        let p = probe(&tools(), fixture()).unwrap();
        assert_eq!((p.width, p.height), (320, 240));
        assert_eq!(p.audio_tracks, 2);
        assert!((5_900..=6_100).contains(&p.duration_ms), "{}", p.duration_ms);
    }

    #[test]
    fn thumbnail_writes_a_jpeg() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("thumbs").join("clip.jpg");
        thumbnail(&tools(), fixture(), &out, 1_000).unwrap();
        let bytes = fs::read(&out).unwrap();
        assert_eq!(&bytes[..3], &[0xFF, 0xD8, 0xFF], "JPEG magic");
        // Replacing an existing thumbnail works.
        thumbnail(&tools(), fixture(), &out, 0).unwrap();
        assert_eq!(fs::read_dir(out.parent().unwrap()).unwrap().count(), 1, "no temp files left");
    }

    #[test]
    fn demux_audio_writes_a_playable_m4a_of_one_track() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("mic.m4a");
        demux_audio(&tools(), fixture(), 1, &out).unwrap();
        let p = probe(&tools(), &out).unwrap();
        assert_eq!(p.audio_tracks, 1);
        assert_eq!((p.width, p.height), (0, 0), "no video");
        assert!((5_900..=6_100).contains(&p.duration_ms), "{}", p.duration_ms);
    }

    // Review Focus 4: a missing file or track is an error, never a panic.
    #[test]
    fn missing_files_and_tracks_are_errors() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing.mp4");
        assert!(matches!(probe(&tools(), &missing), Err(AppError::ToolFailed { .. })));
        assert!(matches!(
            thumbnail(&tools(), &missing, &dir.path().join("t.jpg"), 0),
            Err(AppError::ToolFailed { .. })
        ));
        assert!(matches!(
            demux_audio(&tools(), fixture(), 5, &dir.path().join("x.m4a")),
            Err(AppError::ToolFailed { .. })
        ));
        // A frame past the end of the clip.
        assert!(matches!(
            thumbnail(&tools(), fixture(), &dir.path().join("late.jpg"), 60_000),
            Err(AppError::ToolFailed { .. })
        ));
    }
}
