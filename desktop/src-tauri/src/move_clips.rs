//! Moving the library's clips along when the clips folder changes in Settings.
//!
//! What moves: every library file inside the old folder, keeping its place under it
//! (`Recordings/…`, `Imported/…`). Clips imported in place from elsewhere, files Klipt doesn't
//! track and thumbnails (in the app data folder) stay where they are.
//!
//! One file at a time: a rename on the same drive, else a copy (checked by size) and then the
//! original deleted. Each clip's row points at the new file right after it lands, so stopping
//! anywhere (an error, Cancel) leaves every clip with a file: some in the old folder, some in the
//! new one, none missing. While it runs, uploads, exports, Combine, imports and the cleanup wait
//! (`is_moving`), and the window covers itself with the move's progress.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::{lock, Uploads};
use crate::errors::{AppError, Result};
use crate::jobs::{Job, JobKind, JobList, JobState};
use crate::library::Library;
use crate::state::AppState;

/// Payload: null. The move ended: clips have new paths, so lists load again.
pub const CLIPS_MOVED: &str = "clips-moved";

static MOVING: AtomicBool = AtomicBool::new(false);
static CANCEL: AtomicBool = AtomicBool::new(false);
/// Copy in pieces this big, so Cancel and the bar respond during a big file.
const CHUNK: usize = 8 * 1024 * 1024;
/// Klipt's own folders inside the clips folder, removed from the old one once empty.
const OWN_DIRS: &[&str] = &["Recordings", "Imported"];

/// A move is running: work that reads or writes clip files waits.
pub fn is_moving() -> bool {
    MOVING.load(Ordering::SeqCst)
}

/// Refuses work that touches clip files while a move runs.
pub fn check_not_moving() -> Result<()> {
    if is_moving() {
        return Err(AppError::Refused("Klipt is moving your clips to the new folder. Try again when that's done.".into()));
    }
    Ok(())
}

struct MovingGuard;

impl Drop for MovingGuard {
    fn drop(&mut self) {
        MOVING.store(false, Ordering::SeqCst);
    }
}

/// What the move would take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MovePreview {
    pub count: usize,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq)]
struct Planned {
    id: String,
    from: PathBuf,
    /// Where it sits under the old folder.
    rel: PathBuf,
    bytes: u64,
}

/// `path` relative to `root`, comparing them as written and, failing that, as resolved on disk.
fn relative(path: &Path, root: &Path) -> Option<PathBuf> {
    if let Ok(rel) = path.strip_prefix(root) {
        return Some(rel.to_path_buf());
    }
    let path = dunce::canonicalize(path).ok()?;
    let root = dunce::canonicalize(root).ok()?;
    path.strip_prefix(root).ok().map(Path::to_path_buf)
}

/// The library files to move from `old` to `new`: existing files inside `old` that aren't
/// already inside `new` (the new folder may be inside the old one).
fn plan(rows: &[(String, PathBuf)], old: &Path, new: &Path) -> Vec<Planned> {
    rows.iter()
        .filter_map(|(id, path)| {
            let rel = relative(path, old)?;
            if rel.as_os_str().is_empty() || relative(path, new).is_some() {
                return None;
            }
            let meta = fs::metadata(path).ok().filter(|m| m.is_file())?;
            Some(Planned { id: id.clone(), from: path.clone(), rel, bytes: meta.len() })
        })
        .collect()
}

/// `want`, or `name 2.ext`, `name 3.ext`… when that's taken.
fn free_dest(want: &Path) -> PathBuf {
    if !want.exists() {
        return want.to_path_buf();
    }
    let dir = want.parent().unwrap_or(Path::new(""));
    let stem = want.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = want.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    (2..).map(|n| dir.join(format!("{stem} {n}{ext}"))).find(|p| !p.exists()).unwrap_or_else(|| want.to_path_buf())
}

#[derive(Debug, PartialEq)]
enum Moved {
    Done,
    Cancelled,
}

/// Moves one file: a rename, or (another drive) a copy. `on_bytes` gets the bytes copied so far.
fn move_file(from: &Path, to: &Path, cancel: &AtomicBool, on_bytes: impl FnMut(u64)) -> io::Result<Moved> {
    if fs::rename(from, to).is_ok() {
        return Ok(Moved::Done);
    }
    copy_then_remove(from, to, cancel, on_bytes)
}

/// Copies `from` to `to` (checked by size), then deletes `from`. A half-made copy never stays,
/// and when the original can't be deleted (in use) the copy goes instead.
fn copy_then_remove(from: &Path, to: &Path, cancel: &AtomicBool, mut on_bytes: impl FnMut(u64)) -> io::Result<Moved> {
    let part = to.with_file_name(format!("{}.part", to.file_name().map(|n| n.to_string_lossy()).unwrap_or_default()));
    let copied = (|| {
        let mut src = fs::File::open(from)?;
        let mut dst = fs::File::create(&part)?;
        let mut buf = vec![0u8; CHUNK];
        let mut done = 0u64;
        loop {
            if cancel.load(Ordering::SeqCst) {
                return Ok(Moved::Cancelled);
            }
            let n = src.read(&mut buf)?;
            if n == 0 {
                break;
            }
            dst.write_all(&buf[..n])?;
            done += n as u64;
            on_bytes(done);
        }
        dst.sync_all()?;
        if fs::metadata(&part)?.len() != fs::metadata(from)?.len() {
            return Err(io::Error::other("the copy came out a different size"));
        }
        Ok(Moved::Done)
    })();
    match copied {
        Ok(Moved::Done) => {}
        other => {
            let _ = fs::remove_file(&part);
            return other;
        }
    }
    fs::rename(&part, to).inspect_err(|_| {
        let _ = fs::remove_file(&part);
    })?;
    if let Err(e) = fs::remove_file(from) {
        let _ = fs::remove_file(to);
        return Err(e);
    }
    Ok(Moved::Done)
}

#[derive(Debug, Default, PartialEq)]
struct Outcome {
    moved: usize,
    total: usize,
    cancelled: bool,
    error: Option<String>,
}

impl Outcome {
    /// The line the Jobs list and the move window show.
    fn message(&self) -> String {
        let clips = |n: usize| if n == 1 { "1 clip".to_string() } else { format!("{n} clips") };
        if self.moved == self.total {
            return format!("Moved {}.", clips(self.total));
        }
        let mut text = format!("Moved {} of {}. The rest are still in the old folder and still work.", self.moved, self.total);
        if let Some(e) = &self.error {
            text.push_str(&format!(" ({e})"));
        }
        text
    }
}

fn run(library: &Mutex<Library>, files: &[Planned], new_root: &Path, cancel: &AtomicBool, mut on_bytes: impl FnMut(u64, usize)) -> Outcome {
    let mut out = Outcome { total: files.len(), ..Outcome::default() };
    let mut done_bytes = 0u64;
    for file in files {
        if cancel.load(Ordering::SeqCst) {
            out.cancelled = true;
            break;
        }
        let dest = free_dest(&new_root.join(&file.rel));
        let moved = dest
            .parent()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| move_file(&file.from, &dest, cancel, |b| on_bytes(done_bytes + b, out.moved)));
        match moved {
            Ok(Moved::Done) => {}
            Ok(Moved::Cancelled) => {
                out.cancelled = true;
                break;
            }
            // Deleted meanwhile (by hand): nothing to move, the clip shows as missing as before.
            Err(_) if !file.from.exists() && !dest.exists() => {
                done_bytes += file.bytes;
                out.total -= 1;
                continue;
            }
            Err(e) => {
                crate::elog!("klipt: can't move {} to {}: {e}", file.from.display(), dest.display());
                out.error = Some(reason(&e));
                break;
            }
        }
        if let Err(e) = lock(library).set_path(&file.id, &file.from, &dest) {
            crate::elog!("klipt: can't save the new place of clip {}: {e}", file.id);
            // Put it back, so the library and the disk agree.
            if let Err(e) = fs::rename(&dest, &file.from) {
                crate::elog!("klipt: can't put {} back: {e}", file.from.display());
            }
            out.error = Some("Klipt couldn't update its library".into());
            break;
        }
        out.moved += 1;
        done_bytes += file.bytes;
        on_bytes(done_bytes, out.moved);
    }
    out
}

/// A short reason for the user.
fn reason(e: &io::Error) -> String {
    match e.kind() {
        io::ErrorKind::StorageFull => "the new folder's drive is full".into(),
        io::ErrorKind::PermissionDenied => "Klipt isn't allowed to write there".into(),
        io::ErrorKind::ResourceBusy => "a clip is in use by another program".into(),
        _ if e.raw_os_error() == Some(32) => "a clip is in use by another program".into(),
        _ => e.to_string(),
    }
}

/// Removes Klipt's emptied folders from the old clips folder, then the folder itself if it's
/// empty. Anything still in them keeps them.
fn remove_leftovers(old: &Path) {
    for dir in OWN_DIRS {
        let _ = fs::remove_dir(old.join(dir));
    }
    let _ = fs::remove_dir(old);
}

fn files_to_move(app: &AppHandle, from: &Path) -> Result<(Vec<Planned>, PathBuf)> {
    let state = app.state::<AppState>();
    let new_root = lock(&state.settings).output_dir.clone();
    let rows = lock(&state.library).paths()?;
    Ok((plan(&rows, from, &new_root), new_root))
}

/// How many clips (and bytes) would move from `from` (the old clips folder) to the current one.
#[tauri::command]
pub async fn clips_to_move(app: AppHandle, from: PathBuf) -> Result<MovePreview> {
    crate::commands::off_main(move || {
        let (files, _) = files_to_move(&app, &from)?;
        Ok(MovePreview { count: files.len(), bytes: files.iter().map(|f| f.bytes).sum() })
    })
    .await?
}

/// Starts moving the clips in `from` to the current clips folder; returns the move's job id
/// (its progress and result are in the Jobs list). Refused while clip files are in use.
#[tauri::command]
pub async fn move_clips(app: AppHandle, from: PathBuf) -> Result<u64> {
    crate::commands::off_main(move || start(app, from)).await?
}

#[tauri::command]
pub fn cancel_move_clips() {
    CANCEL.store(true, Ordering::SeqCst);
}

fn busy_reason(app: &AppHandle) -> Option<&'static str> {
    if app.state::<Uploads>().count() > 0 {
        return Some("A clip is uploading or exporting. Try again once that's done.");
    }
    if crate::import::is_running() {
        return Some("Clips are being imported. Try again once that's done.");
    }
    let busy = app
        .state::<JobList>()
        .views()
        .into_iter()
        .any(|j| j.state == JobState::Running && matches!(j.kind, JobKind::Combine | JobKind::Download));
    busy.then_some("Clips are being combined or downloaded. Try again once that's done.")
}

fn start(app: AppHandle, from: PathBuf) -> Result<u64> {
    if MOVING.swap(true, Ordering::SeqCst) {
        return Err(AppError::Refused("Your clips are already being moved.".into()));
    }
    let guard = MovingGuard;
    if let Some(reason) = busy_reason(&app) {
        return Err(AppError::Refused(reason.into()));
    }
    let (files, new_root) = files_to_move(&app, &from)?;
    CANCEL.store(false, Ordering::SeqCst);
    let title = new_root.to_string_lossy().into_owned();
    let job = Job::start(&app, JobKind::Move, &title, None);
    let id = job.id();
    let total_bytes: u64 = files.iter().map(|f| f.bytes).sum();
    job.step("Moving clips", 0.0, 1.0, true);
    job.bytes(0, total_bytes);
    let worker = app.clone();
    std::thread::Builder::new()
        .name("klipt-move-clips".into())
        .spawn(move || {
            let _moving = guard;
            let state = worker.state::<AppState>();
            let out = run(&state.library, &files, &new_root, &CANCEL, |done, _| job.bytes(done, total_bytes));
            remove_leftovers(&from);
            let message = Some(out.message());
            if out.error.is_some() {
                job.failed(message.unwrap_or_default());
            } else if out.cancelled {
                job.cancelled(message);
            } else {
                job.done(message);
            }
            if let Err(e) = worker.emit(CLIPS_MOVED, ()) {
                crate::elog!("klipt: can't send {CLIPS_MOVED} to the window: {e}");
            }
            // Uploads that waited for the move carry on.
            crate::upload_resume::start(&worker);
        })
        .map_err(|e| AppError::Internal(format!("can't start moving the clips: {e}")))?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library(dir: &Path) -> Mutex<Library> {
        Mutex::new(Library::open(&dir.join("library.db")).unwrap())
    }

    fn add(lib: &Mutex<Library>, path: &Path) -> String {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"video bytes").unwrap();
        let clip = crate::library::NewClip {
            path: path.to_path_buf(),
            created_at: 0,
            duration_ms: 1000,
            width: 1,
            height: 1,
            has_mic: false,
            game_name: None,
            steam_app_id: None,
            exe: None,
            title: "t".into(),
            has_voice: false,
            kind: crate::library::ClipKind::Clip,
            is_recording: false,
        };
        lock(lib).insert(clip).unwrap().id
    }

    #[test]
    fn moves_library_files_keeping_their_subfolders_and_leaves_the_rest() {
        let tmp = tempfile::tempdir().unwrap();
        let (old, new, elsewhere) = (tmp.path().join("old"), tmp.path().join("new"), tmp.path().join("elsewhere"));
        let lib = library(tmp.path());
        let a = add(&lib, &old.join("a.mp4"));
        let rec = add(&lib, &old.join("Recordings").join("r.mp4"));
        let outside = add(&lib, &elsewhere.join("x.mp4"));
        fs::write(old.join("mine.txt"), b"not Klipt's").unwrap();

        let files = plan(&lock(&lib).paths().unwrap(), &old, &new);
        assert_eq!(files.len(), 2);
        let out = run(&lib, &files, &new, &AtomicBool::new(false), |_, _| {});
        assert_eq!(out, Outcome { moved: 2, total: 2, cancelled: false, error: None });
        remove_leftovers(&old);

        let get = |id: &str| lock(&lib).get(id).unwrap().path;
        assert_eq!(get(&a), new.join("a.mp4"));
        assert_eq!(get(&rec), new.join("Recordings").join("r.mp4"));
        assert_eq!(get(&outside), elsewhere.join("x.mp4"));
        assert!(new.join("a.mp4").is_file() && !old.join("a.mp4").exists());
        assert!(!old.join("Recordings").exists(), "emptied Klipt folder removed");
        assert!(old.join("mine.txt").is_file(), "untracked files stay, and so does the folder");
    }

    #[test]
    fn a_taken_name_gets_a_free_one() {
        let tmp = tempfile::tempdir().unwrap();
        let (old, new) = (tmp.path().join("old"), tmp.path().join("new"));
        let lib = library(tmp.path());
        let a = add(&lib, &old.join("a.mp4"));
        fs::create_dir_all(&new).unwrap();
        fs::write(new.join("a.mp4"), b"someone else's").unwrap();
        let files = plan(&lock(&lib).paths().unwrap(), &old, &new);
        run(&lib, &files, &new, &AtomicBool::new(false), |_, _| {});
        assert_eq!(lock(&lib).get(&a).unwrap().path, new.join("a 2.mp4"));
        assert_eq!(fs::read(new.join("a.mp4")).unwrap(), b"someone else's");
        remove_leftovers(&old);
        assert!(!old.exists(), "the emptied old folder goes");
    }

    #[test]
    fn a_new_folder_inside_the_old_one_isnt_moved_into_itself() {
        let tmp = tempfile::tempdir().unwrap();
        let old = tmp.path().join("old");
        let new = old.join("sub");
        let lib = library(tmp.path());
        add(&lib, &old.join("a.mp4"));
        add(&lib, &new.join("b.mp4"));
        let files = plan(&lock(&lib).paths().unwrap(), &old, &new);
        assert_eq!(files.iter().map(|f| f.rel.clone()).collect::<Vec<_>>(), [PathBuf::from("a.mp4")]);
    }

    #[test]
    fn cancel_stops_before_the_next_file_and_every_clip_keeps_a_file() {
        let tmp = tempfile::tempdir().unwrap();
        let (old, new) = (tmp.path().join("old"), tmp.path().join("new"));
        let lib = library(tmp.path());
        let ids = [add(&lib, &old.join("a.mp4")), add(&lib, &old.join("b.mp4"))];
        let files = plan(&lock(&lib).paths().unwrap(), &old, &new);
        let cancel = AtomicBool::new(false);
        let out = run(&lib, &files, &new, &cancel, |_, moved| {
            if moved == 1 {
                cancel.store(true, Ordering::SeqCst);
            }
        });
        assert_eq!((out.moved, out.cancelled), (1, true));
        for id in &ids {
            assert!(lock(&lib).get(id).unwrap().path.is_file());
        }
        assert!(out.message().starts_with("Moved 1 of 2."));
    }

    #[test]
    fn a_copy_replaces_the_original_and_a_cancelled_one_leaves_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let from = tmp.path().join("a.mp4");
        fs::write(&from, vec![7u8; 1000]).unwrap();
        let to = tmp.path().join("b.mp4");

        assert_eq!(copy_then_remove(&from, &to, &AtomicBool::new(true), |_| {}).unwrap(), Moved::Cancelled);
        assert!(from.is_file() && !to.exists() && !tmp.path().join("b.mp4.part").exists());

        let mut seen = 0;
        assert_eq!(copy_then_remove(&from, &to, &AtomicBool::new(false), |b| seen = b).unwrap(), Moved::Done);
        assert_eq!((seen, fs::read(&to).unwrap()), (1000, vec![7u8; 1000]));
        assert!(!from.exists());

        // Nowhere to write: the original stays.
        assert!(move_file(&to, &tmp.path().join("missing").join("c.mp4"), &AtomicBool::new(false), |_| {}).is_err());
        assert!(to.is_file());
    }

    #[test]
    fn the_message_says_what_happened() {
        assert_eq!(Outcome { moved: 3, total: 3, ..Outcome::default() }.message(), "Moved 3 clips.");
        let failed = Outcome { moved: 120, total: 214, error: Some("the new folder's drive is full".into()), ..Outcome::default() };
        assert_eq!(
            failed.message(),
            "Moved 120 of 214. The rest are still in the old folder and still work. (the new folder's drive is full)"
        );
    }
}
