//! The local clip library: one SQLite row per saved clip, plus generated thumbnails.

mod db;
pub use db::{validate_cuts, validate_edit, validate_subtitles, validate_zooms};

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub use db::{Library, ServerFields};

/// A video clip or a screenshot (a PNG): both live in the library.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClipKind {
    #[default]
    Clip,
    Screenshot,
}

impl ClipKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ClipKind::Clip => "clip",
            ClipKind::Screenshot => "screenshot",
        }
    }
}

/// What the save flow knows about a fresh clip.
#[derive(Debug, Clone, PartialEq)]
pub struct NewClip {
    pub path: PathBuf,
    /// Unix time in milliseconds.
    pub created_at: i64,
    pub duration_ms: u64,
    pub width: u32,
    pub height: u32,
    pub has_mic: bool,
    pub game_name: Option<String>,
    pub steam_app_id: Option<u32>,
    pub exe: Option<String>,
    pub title: String,
    /// Voice chat apps (Discord…) were recorded on their own track (Linux).
    pub has_voice: bool,
    pub kind: ClipKind,
    /// A full recording (started and stopped by the user), not a replay-buffer clip.
    pub is_recording: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Clip {
    pub id: String,
    pub path: PathBuf,
    /// Unix time in milliseconds.
    pub created_at: i64,
    pub duration_ms: u64,
    pub width: u32,
    pub height: u32,
    pub has_mic: bool,
    pub game_name: Option<String>,
    pub steam_app_id: Option<u32>,
    pub exe: Option<String>,
    pub title: String,
    pub trim_start_ms: Option<u64>,
    pub trim_end_ms: Option<u64>,
    pub game_volume: f32,
    pub mic_volume: f32,
    pub server_clip_id: Option<String>,
    pub share_url: Option<String>,
    pub visibility: Option<String>,
    pub upload_id: Option<String>,
    pub upload_offset: Option<u64>,
    /// Starred: shown under Favorites and never deleted by the automatic cleanup.
    pub favorite: bool,
    /// The thumbnail's frame (ms into the video file); `None` = automatic.
    pub thumbnail_ms: Option<u64>,
    /// Voice chat has its own audio track, after the game and mic tracks.
    pub has_voice: bool,
    pub voice_volume: f32,
    /// The whole clip's volume, applied after the tracks are mixed.
    pub master_volume: f32,
    pub kind: ClipKind,
    /// A full recording (started and stopped by the user), not a replay-buffer clip.
    pub is_recording: bool,
    /// Volumes of the other audio tracks (imported clips with more tracks), by track index.
    pub extra_volumes: BTreeMap<u32, f32>,
    /// Uploads and exports blur the picture: a clip where only the sound matters.
    pub blur_video: bool,
    /// Parts removed in the editor (ms into the video file), sorted, inside the trim.
    pub removed_ranges: Vec<CutRange>,
    /// Zooms set in the editor (ms into the video file), sorted, never overlapping.
    pub zoom_ranges: Vec<ZoomRange>,
    /// Subtitles typed in the editor (ms into the video file), sorted, never overlapping.
    pub subtitles: Vec<SubtitleRange>,
}

/// A part of a clip removed in the editor, `start_ms..end_ms` into the video file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CutRange {
    pub start_ms: u64,
    pub end_ms: u64,
}

/// The least a zoom magnifies.
pub const MIN_ZOOM: f32 = 1.25;
/// The most a zoom magnifies: past this, 1080p footage turns to mush.
pub const MAX_ZOOM: f32 = 4.0;
/// The shortest a zoom can be.
pub const MIN_ZOOM_MS: u64 = 500;

/// A zoom set in the editor: during `start_ms..end_ms` (into the video file) the picture eases
/// into `zoom`× and back out. `x` and `y` (0–1) place the zoomed area within the slack: its left
/// edge is at `x · (1 − 1/zoom)` of the frame's width, and the same for `y` and the height.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoomRange {
    pub start_ms: u64,
    pub end_ms: u64,
    pub zoom: f32,
    pub x: f32,
    pub y: f32,
}

/// The shortest a subtitle can be.
pub const MIN_SUBTITLE_MS: u64 = 300;
/// The longest a subtitle's text can be, in characters.
pub const MAX_SUBTITLE_CHARS: usize = 200;

/// A subtitle typed in the editor: `text` (one line) shows during `start_ms..end_ms` (into the
/// video file), drawn onto the picture in exports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleRange {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

impl Clip {
    /// The audio track index of the mic, if the clip has one (the game is always track 0).
    pub fn mic_track(&self) -> Option<u32> {
        self.has_mic.then_some(1)
    }

    /// The audio track index of voice chat, if the clip has one: right after the game and mic.
    pub fn voice_track(&self) -> Option<u32> {
        self.has_voice.then_some(if self.has_mic { 2 } else { 1 })
    }
}

/// A clip as the library screen shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipView {
    #[serde(flatten)]
    pub clip: Clip,
    /// The video file is gone (deleted in Explorer or Dolphin, or its drive is unplugged).
    pub file_missing: bool,
    /// Set only when the thumbnail has been generated.
    pub thumb_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Sort {
    #[default]
    NewestFirst,
    OldestFirst,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ListFilter {
    /// Only clips tagged with this game.
    pub game: Option<String>,
    pub sort: Sort,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameCount {
    pub name: String,
    pub steam_app_id: Option<u32>,
    pub count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trim {
    pub start_ms: Option<u64>,
    pub end_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Volumes {
    pub game: f32,
    pub mic: f32,
    pub voice: f32,
    /// The whole mix.
    pub master: f32,
    /// The other audio tracks, by track index (missing = 1).
    pub extra: BTreeMap<u32, f32>,
}

impl Default for Volumes {
    fn default() -> Self {
        Volumes { game: 1.0, mic: 1.0, voice: 1.0, master: 1.0, extra: BTreeMap::new() }
    }
}
