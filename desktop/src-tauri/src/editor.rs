//! The clip editor's file work: the per-track audio it plays, saving an edit (which makes any
//! cached upload export stale), and exporting a copy. The Tauri commands are in `commands`.
//!
//! The editor never touches the clip's video file: an edit is only the trim and volumes stored in
//! the library, applied when a clip is exported or uploaded.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::commands::Uploads;
use crate::errors::{AppError, Result};
use crate::export::{Edit, Tracks};
use crate::ffmpeg::{self, Tools};
use crate::library::{Clip, Library, Trim, Volumes};

/// Where the editor's copies of the game and mic audio of clip `id` are cached:
/// `<cache>/edit/<id>-game.m4a` and `<cache>/edit/<id>-mic.m4a`.
pub fn edit_audio_paths(cache_dir: &Path, id: &str) -> (PathBuf, PathBuf) {
    let dir = cache_dir.join("edit");
    (dir.join(format!("{id}-game.m4a")), dir.join(format!("{id}-mic.m4a")))
}

/// The editor's copy of the voice chat audio of clip `id`: `<cache>/edit/<id>-voice.m4a`.
pub fn voice_audio_path(cache_dir: &Path, id: &str) -> PathBuf {
    cache_dir.join("edit").join(format!("{id}-voice.m4a"))
}

/// The upload's export of clip `id` (what an interrupted upload resumes from).
pub fn upload_export_path(cache_dir: &Path, id: &str) -> PathBuf {
    cache_dir.join("exports").join(format!("{id}.mp4"))
}

/// The staging name for "Export copy…", kept apart from the upload's export so exporting a copy
/// never disturbs an upload that can resume.
pub fn copy_export_id(id: &str) -> String {
    format!("{id}-copy")
}

/// True when `cached` exists, isn't empty and was written no earlier than `source` was last
/// modified, so it still matches the source.
pub fn is_fresh(cached: &Path, source: &Path) -> bool {
    let (Ok(cached), Ok(source)) = (fs::metadata(cached), fs::metadata(source)) else {
        return false;
    };
    match (cached.modified(), source.modified()) {
        (Ok(c), Ok(s)) => cached.is_file() && cached.len() > 0 && c >= s,
        _ => false,
    }
}

/// Demuxes audio track `track` of `input` into `out` unless a fresh copy is already there.
/// Returns whether it demuxed.
pub fn demux_cached(tools: &Tools, input: &Path, track: u32, out: &Path) -> Result<bool> {
    if is_fresh(out, input) {
        return Ok(false);
    }
    ffmpeg::demux_audio(tools, input, track, out)?;
    Ok(true)
}

/// The game audio and (when the clip has them) mic and voice chat audio of a clip, as separate
/// cached files the editor plays. A mic or voice track that can't be read is treated as missing
/// rather than an error.
pub fn prepare_audio(
    tools: &Tools,
    input: &Path,
    tracks: Tracks,
    cache_dir: &Path,
    id: &str,
) -> Result<(PathBuf, Option<PathBuf>, Option<PathBuf>)> {
    let (game, mic) = edit_audio_paths(cache_dir, id);
    demux_cached(tools, input, 0, &game)?;
    let optional = |track: Option<u32>, out: PathBuf, what: &str| {
        let track = track?;
        match demux_cached(tools, input, track, &out) {
            Ok(_) => Some(out),
            Err(e) => {
                eprintln!("klipt: clip {id} has no readable {what} track, editing without it: {e}");
                None
            }
        }
    };
    let mic = optional(tracks.mic, mic, "mic");
    let voice = optional(tracks.voice, voice_audio_path(cache_dir, id), "voice chat");
    Ok((game, mic, voice))
}

/// The edit stored for a clip.
pub fn current_edit(clip: &Clip) -> Edit {
    Edit {
        trim_start_ms: clip.trim_start_ms,
        trim_end_ms: clip.trim_end_ms,
        game_volume: clip.game_volume,
        mic_volume: clip.mic_volume,
        voice_volume: clip.voice_volume,
        master_volume: clip.master_volume,
    }
}

/// Saves the edit of clip `id`. Refused while the clip is uploading or exporting. A changed edit
/// also drops the cached upload export and the upload's resume point, so an interrupted upload
/// can't resume with the old cut. Returns whether anything changed.
pub fn save_edit(library: &Library, uploads: &Uploads, cache_dir: &Path, id: &str, edit: Edit) -> Result<bool> {
    let _busy = uploads.try_begin(id).map_err(|_| AppError::ClipBusy)?;
    let clip = library.get(id)?;
    if current_edit(&clip) == edit {
        return Ok(false);
    }
    // Drop the old cut's upload export first: if that fails, the edit isn't saved either, so a
    // retry still sees a changed edit and cleans up (instead of resuming the old cut's upload).
    let (trim, volumes) =
        (Trim { start_ms: edit.trim_start_ms, end_ms: edit.trim_end_ms }, Volumes { game: edit.game_volume, mic: edit.mic_volume, voice: edit.voice_volume, master: edit.master_volume });
    crate::library::validate_edit(trim, volumes)?;
    forget_upload_export(library, cache_dir, id)?;
    library.update_edit(
        id,
        Trim { start_ms: edit.trim_start_ms, end_ms: edit.trim_end_ms },
        Volumes { game: edit.game_volume, mic: edit.mic_volume, voice: edit.voice_volume, master: edit.master_volume },
    )?;
    Ok(true)
}

/// Deletes the cached upload export of clip `id` and forgets where its upload stopped.
pub fn forget_upload_export(library: &Library, cache_dir: &Path, id: &str) -> Result<()> {
    remove_if_present(&upload_export_path(cache_dir, id))?;
    library.set_upload_progress(id, None)
}

/// Best-effort cleanup of the editor's cached audio of a clip that was removed.
pub fn remove_edit_cache(cache_dir: &Path, id: &str) {
    let (game, mic) = edit_audio_paths(cache_dir, id);
    for file in [game, mic, voice_audio_path(cache_dir, id)] {
        if let Err(e) = remove_if_present(&file) {
            eprintln!("klipt: can't delete {}: {e}", file.display());
        }
    }
}

fn remove_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Whether `a` and `b` name the same file (`b` may not exist yet).
pub fn same_file(a: &Path, b: &Path) -> bool {
    fn canonical(p: &Path) -> PathBuf {
        p.canonicalize()
            .ok()
            .or_else(|| {
                let parent = p.parent()?.canonicalize().ok()?;
                Some(parent.join(p.file_name()?))
            })
            .unwrap_or_else(|| p.to_path_buf())
    }
    a == b || canonical(a) == canonical(b)
}

/// Moves `from` to `to`, replacing `to`. Falls back to copy + delete when a rename can't work
/// (e.g. `to` is on another drive). `from` is gone afterwards either way.
pub fn move_file(from: &Path, to: &Path) -> Result<()> {
    if fs::rename(from, to).is_ok() {
        return Ok(());
    }
    let copied = fs::copy(from, to);
    let _ = fs::remove_file(from);
    if let Err(e) = copied {
        let _ = fs::remove_file(to);
        return Err(e.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffmpeg::tests::{fixture, tools};
    use crate::library::NewClip;
    use std::time::{Duration, SystemTime};

    fn set_mtime(path: &Path, time: SystemTime) {
        fs::File::options().write(true).open(path).unwrap().set_modified(time).unwrap();
    }

    #[test]
    fn edit_files_are_named_per_clip() {
        let (game, mic) = edit_audio_paths(Path::new("/cache"), "abc");
        assert_eq!(game, Path::new("/cache/edit/abc-game.m4a"));
        assert_eq!(mic, Path::new("/cache/edit/abc-mic.m4a"));
        assert_eq!(upload_export_path(Path::new("/cache"), "abc"), Path::new("/cache/exports/abc.mp4"));
        assert_ne!(copy_export_id("abc"), "abc", "a copy never overwrites the upload's export");
    }

    #[test]
    fn a_cache_is_fresh_only_if_written_after_the_source() {
        let dir = tempfile::tempdir().unwrap();
        let (source, cached) = (dir.path().join("clip.mp4"), dir.path().join("game.m4a"));
        fs::write(&source, b"video").unwrap();
        assert!(!is_fresh(&cached, &source), "missing");
        fs::write(&cached, b"").unwrap();
        assert!(!is_fresh(&cached, &source), "empty");
        fs::write(&cached, b"audio").unwrap();
        let now = SystemTime::now();
        set_mtime(&source, now - Duration::from_secs(60));
        set_mtime(&cached, now);
        assert!(is_fresh(&cached, &source));
        set_mtime(&source, now + Duration::from_secs(60));
        assert!(!is_fresh(&cached, &source), "the clip changed after the cache was written");
        assert!(!is_fresh(&cached, &dir.path().join("gone.mp4")), "no source");
    }

    #[test]
    fn demux_is_cached_until_the_clip_changes() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("clip.mp4");
        fs::copy(fixture(), &input).unwrap();
        set_mtime(&input, SystemTime::now() - Duration::from_secs(60));
        let (game, _) = edit_audio_paths(dir.path(), "c");
        assert!(demux_cached(&tools(), &input, 0, &game).unwrap(), "first open demuxes");
        assert!(!demux_cached(&tools(), &input, 0, &game).unwrap(), "second open reuses the file");
        set_mtime(&input, SystemTime::now() + Duration::from_secs(60));
        assert!(demux_cached(&tools(), &input, 0, &game).unwrap(), "a changed clip is demuxed again");
    }

    #[test]
    fn audio_for_clips_with_and_without_a_mic() {
        let dir = tempfile::tempdir().unwrap();
        let (game, mic, _) = prepare_audio(&tools(), fixture(), Tracks { mic: Some(1), voice: None }, dir.path(), "two").unwrap();
        assert_eq!(ffmpeg::probe(&tools(), &game).unwrap().audio_tracks, 1);
        assert_eq!(ffmpeg::probe(&tools(), mic.as_ref().unwrap()).unwrap().audio_tracks, 1);

        // Review Focus 5: mic off means no mic file, so the editor hides the Mic slider.
        let (_, mic, _) = prepare_audio(&tools(), fixture(), Tracks::default(), dir.path(), "off").unwrap();
        assert_eq!(mic, None);
        assert!(!edit_audio_paths(dir.path(), "off").1.exists());

        // A clip that claims a mic it doesn't have still opens, without one.
        let one = dir.path().join("one.mp4");
        let status = std::process::Command::new(&tools().ffmpeg)
            .args(["-nostdin", "-loglevel", "error", "-y", "-i"])
            .arg(fixture())
            .args(["-map", "0:v:0", "-map", "0:a:0", "-c", "copy"])
            .arg(&one)
            .status()
            .unwrap();
        assert!(status.success());
        let (game, mic, _) = prepare_audio(&tools(), &one, Tracks { mic: Some(1), voice: None }, dir.path(), "one").unwrap();
        assert!(game.is_file());
        assert_eq!(mic, None);
    }

    // Review Focus 4: a missing clip file is an error, not a panic.
    #[test]
    fn a_missing_clip_has_no_audio() {
        let dir = tempfile::tempdir().unwrap();
        let r = prepare_audio(&tools(), &dir.path().join("gone.mp4"), Tracks { mic: Some(1), voice: None }, dir.path(), "x");
        assert!(matches!(r, Err(AppError::ToolFailed { .. })));
    }

    struct Fixture {
        dir: tempfile::TempDir,
        lib: Library,
        clip: Clip,
    }

    impl Fixture {
        fn new() -> Fixture {
            let dir = tempfile::tempdir().unwrap();
            let lib = Library::open(&dir.path().join("library.db")).unwrap();
            let clip = lib
                .insert(NewClip {
                    path: dir.path().join("clip.mp4"),
                    created_at: 1,
                    duration_ms: 10_000,
                    width: 1920,
                    height: 1080,
                    has_mic: true,
                    has_voice: false,
                    kind: crate::library::ClipKind::Clip,
                    game_name: None,
                    steam_app_id: None,
                    exe: None,
                    title: "clip".into(),
                })
                .unwrap();
            Fixture { dir, lib, clip }
        }

        fn cache(&self) -> PathBuf {
            self.dir.path().join("cache")
        }

        /// A half-finished upload: its export on disk and its resume point in the library.
        fn interrupted_upload(&self) -> PathBuf {
            let export = upload_export_path(&self.cache(), &self.clip.id);
            fs::create_dir_all(export.parent().unwrap()).unwrap();
            fs::write(&export, b"old cut").unwrap();
            self.lib.set_upload_progress(&self.clip.id, Some(("https://a.com/api/uploads/u", 1024))).unwrap();
            export
        }
    }

    fn cut(start: u64, end: u64) -> Edit {
        Edit { trim_start_ms: Some(start), trim_end_ms: Some(end), game_volume: 0.8, mic_volume: 0.3, voice_volume: 1.0, master_volume: 1.0 }
    }

    #[test]
    fn saving_an_edit_drops_the_stale_upload_export() {
        let f = Fixture::new();
        let export = f.interrupted_upload();
        assert!(save_edit(&f.lib, &Uploads::default(), &f.cache(), &f.clip.id, cut(1_000, 4_000)).unwrap());

        let got = f.lib.get(&f.clip.id).unwrap();
        assert_eq!(current_edit(&got), cut(1_000, 4_000));
        assert!(!export.exists(), "the old cut's export is deleted");
        assert_eq!((got.upload_id, got.upload_offset), (None, None), "the upload starts over");
    }

    #[test]
    fn saving_the_same_edit_keeps_a_resumable_upload() {
        let f = Fixture::new();
        let export = f.interrupted_upload();
        let unchanged = current_edit(&f.clip);
        assert!(!save_edit(&f.lib, &Uploads::default(), &f.cache(), &f.clip.id, unchanged).unwrap());
        assert!(export.exists());
        assert!(f.lib.get(&f.clip.id).unwrap().upload_id.is_some());
    }

    #[test]
    fn saving_a_full_clip_edit_without_any_cache_works() {
        let f = Fixture::new();
        let edit = Edit { trim_start_ms: None, trim_end_ms: None, game_volume: 0.5, mic_volume: 1.0, voice_volume: 0.7, master_volume: 0.9 };
        assert!(save_edit(&f.lib, &Uploads::default(), &f.cache(), &f.clip.id, edit).unwrap());
        assert_eq!(current_edit(&f.lib.get(&f.clip.id).unwrap()), edit);
    }

    #[test]
    fn an_edit_is_refused_while_the_clip_uploads() {
        let f = Fixture::new();
        let export = f.interrupted_upload();
        let uploads = Uploads::default();
        let upload = uploads.try_begin(&f.clip.id).unwrap();
        let r = save_edit(&f.lib, &uploads, &f.cache(), &f.clip.id, cut(1_000, 4_000));
        assert!(matches!(r, Err(AppError::ClipBusy)));
        assert!(!r.unwrap_err().user_message().contains("clip_busy"));
        // Nothing changed: the running upload keeps its cut and its resume point.
        assert_eq!(current_edit(&f.lib.get(&f.clip.id).unwrap()), current_edit(&f.clip));
        assert!(export.exists());
        drop(upload);
        assert!(save_edit(&f.lib, &uploads, &f.cache(), &f.clip.id, cut(1_000, 4_000)).unwrap());
        assert!(!uploads.is_active(&f.clip.id), "saving doesn't leave the clip marked busy");
    }

    #[test]
    fn invalid_edits_change_nothing() {
        let f = Fixture::new();
        let export = f.interrupted_upload();
        let r = save_edit(&f.lib, &Uploads::default(), &f.cache(), &f.clip.id, cut(4_000, 1_000));
        assert!(matches!(r, Err(AppError::InvalidInput(_))));
        assert!(export.exists());
        assert!(matches!(
            save_edit(&f.lib, &Uploads::default(), &f.cache(), "nope", cut(0, 1)),
            Err(AppError::ClipNotFound(_))
        ));
    }

    #[test]
    fn edit_cache_cleanup_ignores_missing_files() {
        let dir = tempfile::tempdir().unwrap();
        let (game, mic) = edit_audio_paths(dir.path(), "c");
        fs::create_dir_all(game.parent().unwrap()).unwrap();
        fs::write(&game, b"a").unwrap();
        remove_edit_cache(dir.path(), "c");
        assert!(!game.exists() && !mic.exists());
    }

    #[test]
    fn same_file_sees_through_relative_parts() {
        let dir = tempfile::tempdir().unwrap();
        let clip = dir.path().join("clip.mp4");
        fs::write(&clip, b"v").unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        assert!(same_file(&dir.path().join("sub").join("..").join("clip.mp4"), &clip));
        assert!(!same_file(&dir.path().join("copy.mp4"), &clip));
    }

    #[test]
    fn move_file_replaces_the_destination() {
        let dir = tempfile::tempdir().unwrap();
        let (from, to) = (dir.path().join("staged.mp4"), dir.path().join("out.mp4"));
        fs::write(&from, b"new").unwrap();
        fs::write(&to, b"old").unwrap();
        move_file(&from, &to).unwrap();
        assert_eq!(fs::read(&to).unwrap(), b"new");
        assert!(!from.exists());
        // A destination folder that doesn't exist fails cleanly, leaving no staged file behind.
        fs::write(&from, b"new").unwrap();
        assert!(move_file(&from, &dir.path().join("nope").join("out.mp4")).is_err());
        assert!(!from.exists());
    }
}
