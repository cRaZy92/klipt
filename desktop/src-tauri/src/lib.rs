//! Klipt desktop app core. All logic lives here; the Svelte UI talks to it through Tauri commands.

pub mod errors;
pub mod ffmpeg;
pub mod library;
pub mod settings;
pub mod state;

use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            app.manage(state::AppState::init(data_dir)?);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Klipt");
}
