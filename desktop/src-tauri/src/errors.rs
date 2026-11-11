//! One error type for the app core. Commands return it to the UI serialized as
//! `{ code, message }`: `code` is stable for the UI to branch on, `message` is friendly
//! English for the user. Raw details (paths, ffmpeg stderr, SQL errors) only go to the log.

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

use crate::settings::FieldError;
use crate::upload::{ApiError, UploadError};

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
    /// Between attempts to start the recorder (e.g. waiting for a busy GPU encoder).
    #[error("the recorder is still starting")]
    RecorderStarting,
    #[error("the shortcut is taken by another app")]
    HotkeyTaken,
    #[error("can't set up the global shortcut: {0}")]
    HotkeyUnavailable(String),
    #[error("the global shortcut stopped working")]
    HotkeyLost,
    #[error("not logged in to the server")]
    Unauthorized,
    #[error("wrong username or password")]
    WrongCredentials,
    #[error("invalid server address")]
    InvalidServerAddress,
    #[error("no Klipt server at that address: {0}")]
    ServerNotFound(String),
    #[error("invalid clip title")]
    InvalidTitle,
    #[error("the clip is already uploading")]
    AlreadyUploading,
    #[error("the clip's video file is missing")]
    FileMissing,
    #[error("the clip is busy uploading or exporting")]
    ClipBusy,
    #[error("the export would overwrite the original clip")]
    OverwriteOriginal,
    #[error("internal error: {0}")]
    Internal(String),
    #[error("can't reach the server: {0}")]
    Network(String),
    #[error("server error: {0}")]
    ServerError(String),
    #[error("the server rejected the video: {0}")]
    InvalidVideo(String),
    #[error("i/o: {0}")]
    Io(#[from] std::io::Error),
    #[error("database: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

impl From<ApiError> for AppError {
    fn from(e: ApiError) -> Self {
        match e {
            ApiError::Unauthorized => AppError::Unauthorized,
            ApiError::Network(d) => AppError::Network(d),
            ApiError::Server { status, code } => AppError::ServerError(format!("{status} {code}")),
            ApiError::InvalidVideo(d) => AppError::InvalidVideo(d),
        }
    }
}

impl From<UploadError> for AppError {
    fn from(e: UploadError) -> Self {
        match e {
            UploadError::Api(e) => e.into(),
            UploadError::Io(e) => AppError::Io(e),
        }
    }
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
            AppError::RecorderStarting => "recorder_starting",
            AppError::HotkeyTaken => "hotkey_taken",
            AppError::HotkeyUnavailable(_) | AppError::HotkeyLost => "hotkey_unavailable",
            AppError::Unauthorized => "unauthorized",
            AppError::WrongCredentials => "wrong_credentials",
            AppError::InvalidServerAddress => "invalid_server_address",
            AppError::ServerNotFound(_) => "server_not_found",
            AppError::InvalidTitle => "invalid_title",
            AppError::AlreadyUploading => "already_uploading",
            AppError::FileMissing => "file_missing",
            AppError::ClipBusy => "clip_busy",
            AppError::OverwriteOriginal => "overwrite_original",
            AppError::Internal(_) => "internal",
            AppError::Network(_) => "network",
            AppError::ServerError(_) => "server_error",
            AppError::InvalidVideo(_) => "invalid_video",
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
            AppError::RecorderStarting => "Recording is still starting. Try again in a few seconds.".into(),
            AppError::HotkeyTaken => {
                "That shortcut is already used by another app. Pick a different one in Settings.".into()
            }
            AppError::HotkeyUnavailable(_) => {
                "Klipt couldn't set up the save-clip shortcut. You can still save clips from the tray icon.".into()
            }
            AppError::HotkeyLost => "The save shortcut stopped working. Use the tray menu to save, or restart Klipt.".into(),
            AppError::Unauthorized => "Your session has expired. Please log in again.".into(),
            AppError::WrongCredentials => "Wrong username or password.".into(),
            AppError::InvalidServerAddress => {
                "That doesn't look like a server address. It should look like clips.example.com.".into()
            }
            AppError::ServerNotFound(_) => {
                "Klipt couldn't find a Klipt server at that address. Check the address and your connection.".into()
            }
            AppError::InvalidTitle => "Give the clip a title (up to 100 characters).".into(),
            AppError::AlreadyUploading => "This clip is already uploading.".into(),
            AppError::FileMissing => "This clip's video file is missing. It may have been moved or deleted.".into(),
            AppError::ClipBusy => {
                "This clip is uploading or exporting right now. Try again when that's done.".into()
            }
            AppError::OverwriteOriginal => {
                "That's the original clip. Choose a different name so it isn't overwritten.".into()
            }
            AppError::Internal(_) => "Something went wrong. Please try again.".into(),
            AppError::Network(_) => "Can't reach the Klipt server. Check your connection and try again.".into(),
            AppError::ServerError(_) => "The server had a problem. Try again in a moment.".into(),
            AppError::InvalidVideo(_) => "The server couldn't read this video. It may be damaged.".into(),
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
