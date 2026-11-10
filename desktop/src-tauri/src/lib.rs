//! Klipt desktop app core. All logic lives here; the Svelte UI talks to it through Tauri commands.

pub mod errors;
pub mod ffmpeg;
pub mod games;
pub mod library;
pub mod recorder;
pub mod settings;
pub mod state;

use tauri::{Manager, RunEvent};

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let state = state::AppState::init(data_dir.clone())?;
            let settings = state.settings.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone();
            app.manage(state);
            app.manage(recorder::spawn_for_app(app.handle(), data_dir, &settings));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![recorder::save_clip])
        .build(tauri::generate_context!())
        .expect("error while building Klipt")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                // Stop the recorder helper cleanly instead of leaving it to die with us.
                if let Some(recorder) = app.try_state::<recorder::SupervisorHandle>() {
                    recorder.shutdown();
                }
            }
        });
}
