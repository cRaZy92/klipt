//! "New clip from this part": the editor's trim of a clip (a long recording, usually) becomes a
//! clip of its own, with its own file, title and thumbnail. The source clip and its file are left
//! as they are.
//!
//! The new file is a stream copy (no re-encoding, so it takes seconds even for a long part),
//! starting at the keyframe at or before the trim. The new clip's trim then starts the part
//! exactly where it was set, and the cuts, zooms and subtitles inside the part move along with it.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::errors::{AppError, Result};
use crate::export::Edit;
use crate::ffmpeg::{self, Tools};
use crate::library::{
    AudioRange, BlurRange, Clip, ClipKind, ClipView, CutRange, Library, NewClip, SubtitleRange, Trim, Volumes, ZoomRange, MIN_SUBTITLE_MS,
    MIN_AUDIO_RANGE_MS, MIN_BLUR_MS, MIN_ZOOM_MS,
};

/// The shortest part that can become a clip.
pub const MIN_PART_MS: u64 = 500;

/// The part of the source an edit's trim marks: `start..end` ms into the file.
pub fn part_range(edit: &Edit, duration_ms: u64) -> Result<(u64, u64)> {
    let start = edit.trim_start_ms.unwrap_or(0);
    let end = edit.trim_end_ms.unwrap_or(duration_ms).min(duration_ms.max(start));
    if end < start + MIN_PART_MS {
        return Err(AppError::InvalidInput(format!("the part {start}..{end} ms is too short")));
    }
    Ok((start, end))
}

/// The edit of the new clip: the source's edit for `start..end`, in the new file's time (which
/// starts at `offset_ms` of the source and is `duration_ms` long). Cuts, zooms and subtitles are
/// clipped to the part; a zoom or subtitle left too short is dropped.
pub fn part_edit(edit: &Edit, start: u64, end: u64, offset_ms: u64, duration_ms: u64) -> Edit {
    let shift = |ms: u64| ms.saturating_sub(offset_ms).min(duration_ms);
    let (trim_start, trim_end) = (shift(start), shift(end));
    let clipped = |s: u64, e: u64| (shift(s.max(start)), shift(e.min(end)));
    let removed = edit
        .removed
        .iter()
        .map(|r| clipped(r.start_ms, r.end_ms))
        .filter(|(s, e)| s < e)
        .map(|(start_ms, end_ms)| CutRange { start_ms, end_ms })
        .collect::<Vec<_>>();
    // Nothing left to keep (it can only happen when the clipping moved a cut): keep it all instead.
    let removed_ms: u64 = removed.iter().map(|r| r.end_ms - r.start_ms).sum();
    let removed = if removed_ms >= trim_end - trim_start { Vec::new() } else { removed };
    let zooms = edit
        .zooms
        .iter()
        .map(|z| (z, clipped(z.start_ms, z.end_ms)))
        .filter(|(_, (s, e))| *e >= s + MIN_ZOOM_MS)
        .map(|(z, (start_ms, end_ms))| ZoomRange { start_ms, end_ms, ..*z })
        .collect();
    let blurs = edit
        .blurs
        .iter()
        .map(|b| clipped(b.start_ms, b.end_ms))
        .filter(|(s, e)| *e >= s + MIN_BLUR_MS)
        .map(|(start_ms, end_ms)| BlurRange { start_ms, end_ms })
        .collect();
    let subtitles = edit
        .subtitles
        .iter()
        .map(|t| (t, clipped(t.start_ms, t.end_ms)))
        .filter(|(_, (s, e))| *e >= s + MIN_SUBTITLE_MS)
        .map(|(t, (start_ms, end_ms))| SubtitleRange { start_ms, end_ms, text: t.text.clone() })
        .collect();
    let audio = edit
        .audio
        .iter()
        .map(|r| (r, clipped(r.start_ms, r.end_ms)))
        .filter(|(_, (s, e))| *e >= s + MIN_AUDIO_RANGE_MS)
        .map(|(r, (start_ms, end_ms))| AudioRange { start_ms, end_ms, ..*r })
        .collect();
    Edit {
        trim_start_ms: (trim_start > 0).then_some(trim_start),
        trim_end_ms: (trim_end < duration_ms).then_some(trim_end),
        removed,
        zooms,
        blurs,
        subtitles,
        audio,
        ..edit.clone()
    }
}

/// The new clip's thumbnail frame (ms into its file): the source's when it is inside the kept
/// part, otherwise a third into the part, but no more than 1 s.
pub fn part_thumbnail(source_thumb: Option<u64>, edit: &Edit, start: u64, end: u64, offset_ms: u64) -> u64 {
    let removed = |ms: u64| edit.removed.iter().any(|r| (r.start_ms..r.end_ms).contains(&ms));
    match source_thumb {
        Some(ms) if (start..end).contains(&ms) && !removed(ms) => ms - offset_ms,
        _ => start - offset_ms + crate::recorder::thumbnail_at_ms(end - start),
    }
}

/// A file name (without extension) for `title`: the characters files can't have become spaces.
pub fn file_stem(title: &str) -> String {
    let unsafe_char = |c: char| matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control();
    let cleaned: String = title.chars().map(|c| if unsafe_char(c) { ' ' } else { c }).collect();
    let joined = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let short: String = joined.chars().take(100).collect();
    // Windows drops a trailing dot or space from a file name.
    let stem = short.trim_end_matches(['.', ' ']);
    if stem.is_empty() {
        "Clip".into()
    } else {
        stem.into()
    }
}

/// `<stem>.mp4` in `dir`, or `<stem> (2).mp4`, `<stem> (3).mp4`… when taken.
pub fn free_path(dir: &Path, stem: &str) -> PathBuf {
    let first = dir.join(format!("{stem}.mp4"));
    if !first.exists() {
        return first;
    }
    (2..).map(|n| dir.join(format!("{stem} ({n}).mp4"))).find(|p| !p.exists()).expect("some free name")
}

/// Makes a new clip titled `title` in `dir` from the part of `source` that `edit` trims to, with
/// the edit's cuts, zooms, subtitles and volumes inside it. `thumbnail_ms` is the source's
/// thumbnail frame as the editor has it. The source clip isn't changed.
pub fn make_part(
    tools: &Tools,
    library: &Mutex<Library>,
    source: &Clip,
    edit: &Edit,
    thumbnail_ms: Option<u64>,
    title: &str,
    dir: &Path,
) -> Result<ClipView> {
    let (start, end) = part_range(edit, source.duration_ms)?;
    let offset = ffmpeg::keyframe_at_or_before(tools, &source.path, start)?;
    let path = free_path(dir, &file_stem(title));
    ffmpeg::copy_range(tools, &source.path, offset, end, &path)?;
    let result = (|| {
        let probe = ffmpeg::probe(tools, &path)?;
        // The copy can come out a few ms shorter or longer than asked; the part is what's in it.
        let duration_ms = if probe.duration_ms > 0 { probe.duration_ms } else { end - offset };
        let new_edit = part_edit(edit, start, end, offset, duration_ms);
        let thumb_ms = part_thumbnail(thumbnail_ms, edit, start, end, offset).min(duration_ms.saturating_sub(1));
        let new = NewClip {
            path: path.clone(),
            created_at: SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0),
            duration_ms,
            width: if probe.width > 0 { probe.width } else { source.width },
            height: if probe.height > 0 { probe.height } else { source.height },
            has_mic: source.has_mic,
            has_voice: source.has_voice,
            kind: ClipKind::Clip,
            is_recording: false,
            game_name: source.game_name.clone(),
            steam_app_id: source.steam_app_id,
            exe: source.exe.clone(),
            title: title.to_string(),
        };
        let trim = Trim { start_ms: new_edit.trim_start_ms, end_ms: new_edit.trim_end_ms };
        let volumes = Volumes {
            game: new_edit.game_volume,
            mic: new_edit.mic_volume,
            voice: new_edit.voice_volume,
            master: new_edit.master_volume,
            extra: new_edit.extra_volumes.clone(),
        };
        crate::library::validate_edit(trim, &volumes)?;
        crate::library::validate_cuts(trim, &new_edit.removed, duration_ms)?;
        crate::library::validate_zooms(&new_edit.zooms, duration_ms)?;
        crate::library::validate_blurs(&new_edit.blurs, duration_ms)?;
        crate::library::validate_subtitles(&new_edit.subtitles, duration_ms)?;
        crate::library::validate_audio_ranges(&new_edit.audio, duration_ms)?;
        let (clip, thumb) = {
            let library = library.lock().unwrap_or_else(PoisonError::into_inner);
            let clip = library.insert(new)?;
            let saved = (|| {
                library.update_edit(&clip.id, trim, &volumes)?;
                library.set_blur_video(&clip.id, new_edit.blur_video)?;
                library.set_removed_ranges(&clip.id, &new_edit.removed)?;
                library.set_zoom_ranges(&clip.id, &new_edit.zooms)?;
                library.set_blur_ranges(&clip.id, &new_edit.blurs)?;
                library.set_subtitles(&clip.id, &new_edit.subtitles)?;
                library.set_audio_ranges(&clip.id, &new_edit.audio)?;
                library.set_thumbnail(&clip.id, Some(thumb_ms))
            })();
            if let Err(e) = saved {
                let _ = library.remove(&clip.id, false);
                return Err(e);
            }
            (library.get(&clip.id)?, library.thumb_path(&clip.id))
        };
        if let Err(e) = ffmpeg::thumbnail(tools, &path, &thumb, thumb_ms) {
            crate::elog!("klipt: can't make a thumbnail for {}: {e}", path.display());
        }
        Ok(ClipView { file_missing: false, thumb_path: thumb.is_file().then_some(thumb), clip })
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&path);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffmpeg::tests::{fixture, tools};
    use std::collections::BTreeMap;

    fn edit(start: Option<u64>, end: Option<u64>) -> Edit {
        Edit {
            trim_start_ms: start,
            trim_end_ms: end,
            game_volume: 0.8,
            mic_volume: 0.5,
            voice_volume: 1.0,
            master_volume: 1.2,
            extra_volumes: BTreeMap::from([(3, 0.4)]),
            blur_video: true,
            removed: Vec::new(),
            zooms: Vec::new(),
            blurs: Vec::new(),
            subtitles: Vec::new(),
            audio: Vec::new(),
        }
    }

    fn zoom(start_ms: u64, end_ms: u64) -> ZoomRange {
        ZoomRange { start_ms, end_ms, zoom: 2.0, x: 0.25, y: 0.75 }
    }

    fn sub(start_ms: u64, end_ms: u64, text: &str) -> SubtitleRange {
        SubtitleRange { start_ms, end_ms, text: text.into() }
    }

    #[test]
    fn the_part_is_the_trim() {
        assert_eq!(part_range(&edit(Some(2_000), Some(5_000)), 10_000).unwrap(), (2_000, 5_000));
        assert_eq!(part_range(&edit(None, None), 10_000).unwrap(), (0, 10_000));
        assert_eq!(part_range(&edit(Some(2_000), Some(20_000)), 10_000).unwrap(), (2_000, 10_000));
        assert!(part_range(&edit(Some(2_000), Some(2_100)), 10_000).is_err(), "too short");
        assert!(part_range(&edit(Some(12_000), None), 10_000).is_err(), "past the end");
    }

    #[test]
    fn the_edit_moves_with_the_part() {
        let source = Edit {
            removed: vec![CutRange { start_ms: 3_000, end_ms: 3_500 }],
            zooms: vec![zoom(500, 1_500), zoom(4_000, 6_000), zoom(7_800, 9_000)],
            blurs: vec![BlurRange { start_ms: 2_000, end_ms: 2_400 }, BlurRange { start_ms: 7_000, end_ms: 9_000 }],
            subtitles: vec![
                sub(1_000, 1_900, "before"),
                sub(2_000, 3_000, "across the start"),
                sub(4_500, 5_000, "in"),
            ],
            ..edit(Some(2_200), Some(8_000))
        };
        // The copy starts at the keyframe at 2 s and is 6.02 s long.
        let got = part_edit(&source, 2_200, 8_000, 2_000, 6_020);
        assert_eq!((got.trim_start_ms, got.trim_end_ms), (Some(200), Some(6_000)));
        assert_eq!(got.removed, vec![CutRange { start_ms: 1_000, end_ms: 1_500 }]);
        // The zoom at 7.8 s keeps only 200 ms: too short, dropped.
        assert_eq!(got.zooms, vec![zoom(2_000, 4_000)]);
        // The blurred part across the start keeps 200 ms: too short, dropped; the last is clipped.
        assert_eq!(got.blurs, vec![BlurRange { start_ms: 5_000, end_ms: 6_000 }]);
        assert_eq!(got.subtitles, vec![sub(200, 1_000, "across the start"), sub(2_500, 3_000, "in")]);
        assert_eq!((got.game_volume, got.master_volume, got.blur_video), (0.8, 1.2, true));
        assert_eq!(got.extra_volumes, BTreeMap::from([(3, 0.4)]));
    }

    #[test]
    fn muted_parts_move_with_the_part() {
        let part = |track, start_ms, end_ms| AudioRange { track, start_ms, end_ms, level: 0.25 };
        let source = Edit {
            audio: vec![part(0, 1_000, 2_000), part(0, 2_100, 3_000), part(1, 7_900, 9_000), part(1, 5_000, 6_000)],
            ..edit(Some(2_200), Some(8_000))
        };
        let got = part_edit(&source, 2_200, 8_000, 2_000, 6_020);
        // Before the part: gone; across its start: clipped; 100 ms left at the end: too short, gone.
        assert_eq!(got.audio, vec![part(0, 200, 1_000), part(1, 3_000, 4_000)]);
    }

    #[test]
    fn a_part_reaching_the_end_has_no_trim_end() {
        let got = part_edit(&edit(Some(1_000), None), 1_000, 6_000, 1_000, 5_000);
        assert_eq!((got.trim_start_ms, got.trim_end_ms), (None, None));
    }

    #[test]
    fn the_thumbnail_follows_the_source_when_it_is_in_the_part() {
        let source =
            Edit { removed: vec![CutRange { start_ms: 3_000, end_ms: 3_500 }], ..edit(Some(2_200), Some(8_000)) };
        assert_eq!(part_thumbnail(Some(5_000), &source, 2_200, 8_000, 2_000), 3_000);
        // Outside the part, or in a removed piece: a second into the part.
        assert_eq!(part_thumbnail(Some(1_000), &source, 2_200, 8_000, 2_000), 1_200);
        assert_eq!(part_thumbnail(Some(3_200), &source, 2_200, 8_000, 2_000), 1_200);
        assert_eq!(part_thumbnail(None, &source, 2_200, 2_900, 2_000), 200 + 233);
    }

    #[test]
    fn file_names_drop_what_files_cant_have() {
        assert_eq!(file_stem("Ace: 1v5 / clutch?"), "Ace 1v5 clutch");
        assert_eq!(file_stem("  ...  "), "Clip");
        assert_eq!(file_stem("gg."), "gg");
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(free_path(dir.path(), "a"), dir.path().join("a.mp4"));
        std::fs::write(dir.path().join("a.mp4"), b"x").unwrap();
        assert_eq!(free_path(dir.path(), "a"), dir.path().join("a (2).mp4"));
    }

    #[test]
    fn keyframes_are_read_from_packet_flags() {
        let csv = "1.000000,K__\n1.033333,___\n2.000000,K__\n2.033333,___\n";
        assert_eq!(ffmpeg::parse_keyframe_before(csv, 0.0, 2_500), 2_000);
        assert_eq!(ffmpeg::parse_keyframe_before(csv, 0.0, 1_999), 1_000);
        assert_eq!(ffmpeg::parse_keyframe_before(csv, 1_000.0, 1_500), 1_000, "relative to the file's start");
        assert_eq!(ffmpeg::parse_keyframe_before("", 0.0, 1_500), 0);
    }

    #[test]
    fn a_part_becomes_its_own_clip_and_the_source_stays() {
        let dir = tempfile::tempdir().unwrap();
        let src_path = dir.path().join("Recordings").join("rec.mp4");
        std::fs::create_dir_all(src_path.parent().unwrap()).unwrap();
        std::fs::copy(fixture(), &src_path).unwrap();
        let before = std::fs::read(&src_path).unwrap();
        let library = Mutex::new(Library::open(&dir.path().join("library.db")).unwrap());
        let source = library
            .lock()
            .unwrap()
            .insert(NewClip {
                path: src_path.clone(),
                created_at: 1,
                duration_ms: 6_000,
                width: 320,
                height: 240,
                has_mic: true,
                has_voice: false,
                kind: ClipKind::Clip,
                is_recording: true,
                game_name: Some("WARDOGS".into()),
                steam_app_id: Some(42),
                exe: Some("wardogs.exe".into()),
                title: "Recording".into(),
            })
            .unwrap();
        let cut = Edit { subtitles: vec![sub(3_000, 3_600, "gg")], ..edit(Some(2_500), Some(4_500)) };

        let view = make_part(&tools(), &library, &source, &cut, None, "Clutch: round 3", dir.path()).unwrap();
        let clip = view.clip;
        assert_eq!(clip.path, dir.path().join("Clutch round 3.mp4"));
        assert_eq!(
            (clip.title.as_str(), clip.game_name.as_deref(), clip.steam_app_id),
            ("Clutch: round 3", Some("WARDOGS"), Some(42))
        );
        assert!(!clip.is_recording && clip.has_mic && clip.blur_video);
        // The fixture has a keyframe every second: the copy starts at 2 s, about 2.5 s long.
        assert!((2_400..=2_600).contains(&clip.duration_ms), "{}", clip.duration_ms);
        assert_eq!(clip.trim_start_ms, Some(500));
        assert_eq!(clip.subtitles, vec![sub(1_000, 1_600, "gg")]);
        assert_eq!(ffmpeg::probe(&tools(), &clip.path).unwrap().audio_tracks, 2, "every track is kept");
        assert_eq!(clip.thumbnail_ms, Some(500 + 666));
        assert!(view.thumb_path.is_some_and(|t| t.is_file()));

        // The source is as it was.
        assert_eq!(std::fs::read(&src_path).unwrap(), before);
        assert_eq!(library.lock().unwrap().get(&source.id).unwrap(), source);

        // A second part with the same title gets its own file.
        let again = make_part(&tools(), &library, &source, &cut, None, "Clutch: round 3", dir.path()).unwrap();
        assert_eq!(again.clip.path, dir.path().join("Clutch round 3 (2).mp4"));
    }

    #[test]
    fn a_failed_copy_leaves_nothing_behind() {
        let dir = tempfile::tempdir().unwrap();
        let library = Mutex::new(Library::open(&dir.path().join("library.db")).unwrap());
        let source = library
            .lock()
            .unwrap()
            .insert(NewClip {
                path: dir.path().join("gone.mp4"),
                created_at: 1,
                duration_ms: 6_000,
                width: 320,
                height: 240,
                has_mic: false,
                has_voice: false,
                kind: ClipKind::Clip,
                is_recording: true,
                game_name: None,
                steam_app_id: None,
                exe: None,
                title: "Recording".into(),
            })
            .unwrap();
        let out = dir.path().join("out");
        assert!(make_part(&tools(), &library, &source, &edit(Some(1_000), Some(3_000)), None, "x", &out).is_err());
        assert_eq!(library.lock().unwrap().list(&Default::default()).unwrap().len(), 1);
        assert!(!out.join("x.mp4").exists());
    }
}
