//! App-wide state, created once at startup and shared with commands through Tauri.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::ErrorCode;

use crate::errors::{AppError, Result};
use crate::ffmpeg::{self, Tools};
use crate::library::Library;
use crate::settings::Settings;

pub const LIBRARY_FILE: &str = "library.db";

pub struct AppState {
    /// `<app data>`: settings.json, library.db, thumbs/.
    pub data_dir: PathBuf,
    pub settings: Mutex<Settings>,
    pub library: Mutex<Library>,
    /// None when ffmpeg/ffprobe can't be found; features that need them report `ToolsMissing`.
    pub tools: Option<Tools>,
    /// Something the user must hear about at startup (the library was damaged and replaced).
    pub startup_notice: Option<String>,
}

impl AppState {
    pub fn init(data_dir: PathBuf) -> Result<AppState> {
        std::fs::create_dir_all(&data_dir)?;
        let settings = Settings::load(&data_dir);
        let library_path = data_dir.join(LIBRARY_FILE);
        let (library, startup_notice) = match Library::open(&library_path) {
            Ok(library) => (library, None),
            Err(e) if is_corrupt(&e) => {
                eprintln!("klipt: {LIBRARY_FILE} is damaged ({e}); starting a new library");
                let kept = set_aside(&library_path)?;
                let notice = format!(
                    "Klipt's clip library was damaged, so Klipt started a new, empty one. Your video files \
                     weren't touched. The old library was kept as {}.",
                    kept.display()
                );
                (Library::open(&library_path)?, Some(notice))
            }
            Err(e) => return Err(e),
        };
        let tools = match ffmpeg::locate() {
            Ok(tools) => Some(tools),
            Err(e) => {
                eprintln!("klipt: {e}");
                None
            }
        };
        Ok(AppState { data_dir, settings: Mutex::new(settings), library: Mutex::new(library), tools, startup_notice })
    }
}

/// True when the database file is damaged or isn't a database at all.
fn is_corrupt(e: &AppError) -> bool {
    matches!(e, AppError::Db(rusqlite::Error::SqliteFailure(err, _))
        if matches!(err.code, ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase))
}

/// Moves a damaged database (and its WAL files) to `<name>.corrupt-<unix time>`; returns where.
fn set_aside(path: &Path) -> Result<PathBuf> {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let kept = path.with_file_name(format!("{name}.corrupt-{secs}"));
    std::fs::rename(path, &kept)?;
    for suffix in ["-wal", "-shm"] {
        let side = path.with_file_name(format!("{name}{suffix}"));
        if side.exists() {
            let _ = std::fs::rename(&side, kept.with_file_name(format!("{name}.corrupt-{secs}{suffix}")));
        }
    }
    Ok(kept)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::ListFilter;

    #[test]
    fn init_creates_the_data_dir_and_an_empty_library() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("app.klipt.desktop");
        let state = AppState::init(data_dir.clone()).unwrap();
        assert!(data_dir.join(LIBRARY_FILE).exists());
        assert_eq!(*state.settings.lock().unwrap(), Settings::default());
        assert!(state.library.lock().unwrap().list(&ListFilter::default()).unwrap().is_empty());
    }

    #[test]
    fn a_damaged_library_is_set_aside_and_replaced_with_a_notice() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("app.klipt.desktop");
        std::fs::create_dir_all(&data_dir).unwrap();
        std::fs::write(data_dir.join(LIBRARY_FILE), vec![0x5a; 8192]).unwrap();

        let state = AppState::init(data_dir.clone()).unwrap();
        assert!(state.library.lock().unwrap().list(&ListFilter::default()).unwrap().is_empty());
        let notice = state.startup_notice.expect("the user is told");
        assert!(notice.contains("library.db.corrupt-"), "{notice}");
        let kept: Vec<_> = std::fs::read_dir(&data_dir)
            .unwrap()
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter(|n| n.starts_with("library.db.corrupt-"))
            .collect();
        assert_eq!(kept.len(), 1, "{kept:?}");
        assert_eq!(std::fs::read(data_dir.join(&kept[0])).unwrap(), vec![0x5a; 8192], "old bytes kept");
    }

    #[test]
    fn a_healthy_library_gives_no_notice() {
        let dir = tempfile::tempdir().unwrap();
        AppState::init(dir.path().to_path_buf()).unwrap();
        let again = AppState::init(dir.path().to_path_buf()).unwrap();
        assert_eq!(again.startup_notice, None);
    }
}
