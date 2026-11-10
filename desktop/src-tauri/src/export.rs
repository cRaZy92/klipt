//! Exporting a clip for upload: the video is stream-copied (trimmed), and the game and mic audio
//! tracks are mixed into one AAC track at the user's chosen volumes.

use std::ffi::OsString;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::errors::{AppError, Result};
use crate::ffmpeg::{probe, Tools};

/// What the user chose in the editor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Edit {
    pub trim_start_ms: Option<u64>,
    pub trim_end_ms: Option<u64>,
    pub game_volume: f32,
    pub mic_volume: f32,
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
pub fn export_args(input: &Path, output: &Path, edit: &Edit, has_mic: bool) -> Result<Vec<String>> {
    let start = edit.trim_start_ms;
    if let Some(end) = edit.trim_end_ms {
        if end <= start.unwrap_or(0) {
            return Err(AppError::InvalidInput("the trim end must be after the trim start".into()));
        }
    }
    let game = volume(edit.game_volume);
    let graph = if has_mic {
        let mic = volume(edit.mic_volume);
        format!("[0:a:0]volume={game}[a0];[0:a:1]volume={mic}[a1];[a0][a1]amix=inputs=2:normalize=0[aout]")
    } else {
        format!("[0:a:0]volume={game}[aout]")
    };

    let mut args: Vec<String> = ["-hide_banner", "-nostdin", "-y"].map(String::from).into();
    if let Some(start) = start {
        args.extend(["-ss".into(), seconds(start)]);
    }
    args.extend(["-i".into(), input.to_string_lossy().into_owned()]);
    if let Some(end) = edit.trim_end_ms {
        args.extend(["-to".into(), seconds(end - start.unwrap_or(0))]);
    }
    args.extend(
        ["-map", "0:v:0", "-c:v", "copy", "-filter_complex"].map(String::from),
    );
    args.push(graph);
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
    has_mic: bool,
    mut progress: impl FnMut(f32),
) -> Result<PathBuf> {
    let info = probe(tools, input)?;
    // Never fail on a missing mic track; never mix a track that isn't there.
    let has_mic = has_mic && info.audio_tracks >= 2;

    let start = edit.trim_start_ms.unwrap_or(0);
    let end = edit.trim_end_ms.unwrap_or(info.duration_ms).min(info.duration_ms.max(1));
    let expected_us = (end.saturating_sub(start).max(1) as f64) * 1000.0;

    let dir = cache_dir.join("exports");
    fs::create_dir_all(&dir)?;
    let out = dir.join(format!("{clip_id}.mp4"));
    let temp = dir.join(format!("{clip_id}.tmp.mp4"));

    let args = export_args(input, &temp, edit, has_mic)?;
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

    fn edit(start: Option<u64>, end: Option<u64>) -> Edit {
        Edit { trim_start_ms: start, trim_end_ms: end, game_volume: 1.0, mic_volume: 0.5 }
    }

    #[test]
    fn args_with_a_mic() {
        let args = export_args(Path::new("in.mp4"), Path::new("out.mp4"), &edit(Some(2000), Some(4000)), true).unwrap();
        assert_eq!(
            args.join(" "),
            "-hide_banner -nostdin -y -ss 2.000 -i in.mp4 -to 2.000 -map 0:v:0 -c:v copy -filter_complex \
             [0:a:0]volume=1[a0];[0:a:1]volume=0.5[a1];[a0][a1]amix=inputs=2:normalize=0[aout] \
             -map [aout] -c:a aac -b:a 160k -movflags +faststart -f mp4 -progress pipe:1 -nostats out.mp4"
        );
    }

    // Review Focus 5: no mic track, the mix comes from one track.
    #[test]
    fn args_without_a_mic_or_trim() {
        let args = export_args(Path::new("in.mp4"), Path::new("out.mp4"), &edit(None, None), false).unwrap();
        assert_eq!(
            args.join(" "),
            "-hide_banner -nostdin -y -i in.mp4 -map 0:v:0 -c:v copy -filter_complex [0:a:0]volume=1[aout] \
             -map [aout] -c:a aac -b:a 160k -movflags +faststart -f mp4 -progress pipe:1 -nostats out.mp4"
        );
    }

    #[test]
    fn volumes_are_clamped_and_bad_ranges_rejected() {
        let e = Edit { trim_start_ms: None, trim_end_ms: Some(1500), game_volume: 5.0, mic_volume: -1.0 };
        let joined = export_args(Path::new("i"), Path::new("o"), &e, true).unwrap().join(" ");
        assert!(joined.contains("volume=2[a0]") && joined.contains("volume=0[a1]") && joined.contains("-to 1.500"));
        for (s, e) in [(Some(4000), Some(2000)), (Some(2000), Some(2000)), (None, Some(0))] {
            assert!(matches!(
                export_args(Path::new("i"), Path::new("o"), &edit(s, e), true),
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
        let out = export(&tools(), fixture(), cache.path(), "clip-1", &edit(Some(2000), Some(4000)), true, |p| {
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
        let out = export(&tools(), &one, cache.path(), "c", &edit(None, None), true, |_| {}).unwrap();
        let p = probe(&tools(), &out).unwrap();
        assert_eq!(p.audio_tracks, 1);
        assert!((5_900..=6_200).contains(&p.duration_ms), "{}", p.duration_ms);
    }

    // Review Focus 4: a missing input is an error and leaves nothing behind.
    #[test]
    fn missing_input_is_an_error() {
        let cache = tempfile::tempdir().unwrap();
        let r = export(&tools(), &cache.path().join("nope.mp4"), cache.path(), "x", &edit(None, None), true, |_| {});
        assert!(matches!(r, Err(AppError::ToolFailed { .. })));
        assert!(!cache.path().join("exports").join("x.mp4").exists());
    }
}
