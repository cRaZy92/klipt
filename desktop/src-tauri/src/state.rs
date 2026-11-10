//! App-wide state, created once at startup and shared with commands through Tauri.

use std::path::PathBuf;
use std::sync::Mutex;

use crate::errors::Result;
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
}

impl AppState {
    pub fn init(data_dir: PathBuf) -> Result<AppState> {
        std::fs::create_dir_all(&data_dir)?;
        let settings = Settings::load(&data_dir);
        let library = Library::open(&data_dir.join(LIBRARY_FILE))?;
        let tools = match ffmpeg::locate() {
            Ok(tools) => Some(tools),
            Err(e) => {
                eprintln!("klipt: {e}");
                None
            }
        };
        Ok(AppState { data_dir, settings: Mutex::new(settings), library: Mutex::new(library), tools })
    }
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
}
