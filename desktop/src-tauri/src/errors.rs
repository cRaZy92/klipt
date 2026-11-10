//! One error type for the app core. Commands return it to the UI serialized as
//! `{ code, message }`: `code` is stable for the UI to branch on, `message` is friendly
//! English for the user. Raw details (paths, ffmpeg stderr, SQL errors) only go to the log.

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

use crate::settings::FieldError;

pub type Result<T, E = AppError> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("clip {0} not found in the library")]
    ClipNotFound(String),
    #[error("ffmpeg/ffprobe not found")]
    ToolsMissing,
    #[error("{tool} failed: {detail}")]
    ToolFailed { tool: &'static str, detail: String },
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("invalid settings: {0:?}")]
    InvalidSettings(Vec<FieldError>),
    #[error("recorder helper not found: {0}")]
    RecorderMissing(String),
    #[error("recorder failed: {0}")]
    RecorderFailed(String),
    #[error("not recording")]
    NotRecording,
    #[error("the shortcut is taken by another app")]
    HotkeyTaken,
    #[error("can't set up the global shortcut: {0}")]
    HotkeyUnavailable(String),
    #[error("i/o: {0}")]
    Io(#[from] std::io::Error),
    #[error("database: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            AppError::ClipNotFound(_) => "clip_not_found",
            AppError::ToolsMissing => "tools_missing",
            AppError::ToolFailed { .. } => "tool_failed",
            AppError::InvalidInput(_) => "invalid_input",
            AppError::InvalidSettings(_) => "invalid_settings",
            AppError::RecorderMissing(_) => "recorder_missing",
            AppError::RecorderFailed(_) => "recorder_failed",
            AppError::NotRecording => "not_recording",
            AppError::HotkeyTaken => "hotkey_taken",
            AppError::HotkeyUnavailable(_) => "hotkey_unavailable",
            AppError::Io(_) => "io",
            AppError::Db(_) => "database",
            AppError::Json(_) => "json",
        }
    }

    /// What the user sees. Never contains raw codes or internal details.
    pub fn user_message(&self) -> String {
        match self {
            AppError::ClipNotFound(_) => "That clip isn't in your library anymore.".into(),
            AppError::ToolsMissing => "Klipt can't find FFmpeg, which it needs to read and export clips.".into(),
            AppError::ToolFailed { .. } => {
                "Klipt couldn't process this clip. The file may be damaged or missing.".into()
            }
            AppError::InvalidInput(_) => "That change isn't valid.".into(),
            AppError::InvalidSettings(_) => "Some settings need fixing before they can be saved.".into(),
            AppError::RecorderMissing(_) if cfg!(target_os = "linux") => {
                "Klipt needs GPU Screen Recorder to record. Install it from your distribution's packages.".into()
            }
            AppError::RecorderMissing(_) => "Klipt's recorder is missing. Try reinstalling Klipt.".into(),
            AppError::RecorderFailed(_) => "Recording couldn't start. Check your recording settings.".into(),
            AppError::NotRecording => "Klipt isn't recording right now.".into(),
            AppError::HotkeyTaken => {
                "That shortcut is already used by another app. Pick a different one in Settings.".into()
            }
            AppError::HotkeyUnavailable(_) => {
                "Klipt couldn't set up the save-clip shortcut. You can still save clips from the tray icon.".into()
            }
            AppError::Io(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                "Klipt doesn't have permission to use that file or folder.".into()
            }
            AppError::Io(_) => "Something went wrong reading or writing a file.".into(),
            AppError::Db(_) | AppError::Json(_) => {
                "Something went wrong with Klipt's data. Try restarting the app.".into()
            }
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let fields = match self {
            AppError::InvalidSettings(fields) => Some(fields),
            _ => None,
        };
        let mut s = serializer.serialize_struct("AppError", if fields.is_some() { 3 } else { 2 })?;
        s.serialize_field("code", self.code())?;
        s.serialize_field("message", &self.user_message())?;
        if let Some(fields) = fields {
            s.serialize_field("fields", fields)?;
        }
        s.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_code_and_friendly_message_without_internal_details() {
        let e = AppError::ToolFailed { tool: "ffprobe", detail: "C:\\secret\\path: moov atom not found".into() };
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(json["code"], "tool_failed");
        let text = json.to_string();
        assert!(!text.contains("moov") && !text.contains("secret"), "{text}");
    }

    #[test]
    fn invalid_settings_carry_the_field_errors() {
        let e = AppError::InvalidSettings(vec![FieldError {
            field: "fps",
            message: "Frame rate must be 30 or 60.".into(),
        }]);
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(json["fields"][0]["field"], "fps");
    }
}
