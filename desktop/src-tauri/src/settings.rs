//! User settings, stored as `<app data>/settings.json` (camelCase JSON).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::errors::Result;

pub const FILE_NAME: &str = "settings.json";
const BACKUP_NAME: &str = "settings.json.bak";
const TEMP_NAME: &str = "settings.json.tmp";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RecordingMode {
    #[default]
    Always,
    WhileGameRunning,
}

/// v1 records to RAM only; Disk is shown in the UI but disabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BufferLocation {
    #[default]
    Ram,
    Disk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Resolution {
    P720,
    #[default]
    P1080,
    P1440,
    Native,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Quality {
    Low,
    Medium,
    #[default]
    High,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub recording_mode: RecordingMode,
    pub buffer_location: BufferLocation,
    /// None = the primary display.
    pub display_id: Option<String>,
    pub resolution: Resolution,
    pub fps: u32,
    pub quality: Quality,
    pub custom_bitrate_kbps: u32,
    pub buffer_seconds: u32,
    pub mic_enabled: bool,
    /// None = the default microphone.
    pub mic_device_id: Option<String>,
    /// Tauri accelerator syntax. The default is Alt plus the key under Esc (W3C code "Backquote"):
    /// NVIDIA App already takes Alt+F9/Alt+F10 on most NVIDIA PCs.
    pub hotkey: String,
    pub output_dir: PathBuf,
    pub start_with_os: bool,
    pub save_sound: bool,
    pub save_notification: bool,
    pub server_url: Option<String>,
    /// Delete clips older than `cleanup_days` from this PC (uploaded and favorite clips are kept).
    pub cleanup_enabled: bool,
    pub cleanup_days: u32,
    /// A system notification when a friend uploads a clip.
    pub friend_notifications: bool,
    /// Voice chat apps (as PipeWire names them) recorded on their own track, so a clip can be
    /// shared without friends' voices. Linux only; empty = no separate track.
    pub voice_apps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldError {
    pub field: &'static str,
    pub message: String,
}

pub const DEFAULT_HOTKEY: &str = "Alt+Backquote";

pub const BUFFER_SECONDS_RANGE: std::ops::RangeInclusive<u32> = 10..=600;
pub const BITRATE_KBPS_RANGE: std::ops::RangeInclusive<u32> = 2_000..=80_000;
pub const FPS_CHOICES: [u32; 2] = [30, 60];
pub const MAX_VOICE_APPS: usize = 10;
pub const CLEANUP_DAYS_RANGE: std::ops::RangeInclusive<u32> = 1..=365;

/// `<Videos>/Klipt`.
pub fn default_output_dir() -> PathBuf {
    dirs::video_dir()
        .or_else(|| dirs::home_dir().map(|home| home.join("Videos")))
        .unwrap_or_else(|| PathBuf::from("Videos"))
        .join("Klipt")
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            recording_mode: RecordingMode::Always,
            buffer_location: BufferLocation::Ram,
            display_id: None,
            resolution: Resolution::P1080,
            fps: 60,
            quality: Quality::High,
            custom_bitrate_kbps: 20_000,
            buffer_seconds: 30,
            mic_enabled: false,
            mic_device_id: None,
            hotkey: DEFAULT_HOTKEY.into(),
            output_dir: default_output_dir(),
            start_with_os: true,
            save_sound: true,
            save_notification: true,
            server_url: None,
            cleanup_enabled: false,
            cleanup_days: 30,
            friend_notifications: true,
            voice_apps: vec!["discord".into(), "vesktop".into()],
        }
    }
}

impl Quality {
    /// The bitrate a preset stands for; None for Custom.
    pub fn preset_kbps(self) -> Option<u32> {
        match self {
            Quality::Low => Some(8_000),
            Quality::Medium => Some(14_000),
            Quality::High => Some(20_000),
            Quality::Custom => None,
        }
    }
}

impl Resolution {
    /// Output size; None for Native (the display's own size).
    pub fn size(self) -> Option<(u32, u32)> {
        match self {
            Resolution::P720 => Some((1280, 720)),
            Resolution::P1080 => Some((1920, 1080)),
            Resolution::P1440 => Some((2560, 1440)),
            Resolution::Native => None,
        }
    }
}

impl Settings {
    /// Reads `dir/settings.json`. A missing file gives defaults. A corrupt file is moved to
    /// `settings.json.bak` and gives defaults. Out-of-range values are reset to their defaults.
    pub fn load(dir: &Path) -> Settings {
        let path = dir.join(FILE_NAME);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    eprintln!("klipt: can't read {}: {e}; using default settings", path.display());
                }
                return Settings::default();
            }
        };
        match serde_json::from_slice::<Settings>(&bytes) {
            Ok(settings) => settings.with_invalid_fields_reset(),
            Err(e) => {
                eprintln!("klipt: {} is corrupt ({e}); moved to {BACKUP_NAME}, using default settings", path.display());
                if let Err(e) = fs::rename(&path, dir.join(BACKUP_NAME)) {
                    eprintln!("klipt: can't back up {}: {e}", path.display());
                }
                Settings::default()
            }
        }
    }

    /// Writes atomically: a temp file in the same folder, then rename over the old file.
    pub fn save(&self, dir: &Path) -> Result<()> {
        fs::create_dir_all(dir)?;
        let temp = dir.join(TEMP_NAME);
        {
            let mut file = fs::File::create(&temp)?;
            file.write_all(&serde_json::to_vec_pretty(self)?)?;
            file.sync_all()?;
        }
        fs::rename(&temp, dir.join(FILE_NAME))?;
        Ok(())
    }

    pub fn validate(&self) -> Vec<FieldError> {
        let mut errors = vec![];
        let mut fail = |field, message: &str| errors.push(FieldError { field, message: message.into() });
        if self.buffer_location == BufferLocation::Disk {
            fail("bufferLocation", "Recording to disk isn't supported yet.");
        }
        if !FPS_CHOICES.contains(&self.fps) {
            fail("fps", "Frame rate must be 30 or 60.");
        }
        // Only Custom uses (and shows) the bitrate; a stale value there must not block saving.
        if self.quality == Quality::Custom && !BITRATE_KBPS_RANGE.contains(&self.custom_bitrate_kbps) {
            fail("customBitrateKbps", "Bitrate must be between 2,000 and 80,000 kbps.");
        }
        if !BUFFER_SECONDS_RANGE.contains(&self.buffer_seconds) {
            fail("bufferSeconds", "Clip length must be between 10 seconds and 10 minutes.");
        }
        if self.hotkey.trim().is_empty() {
            fail("hotkey", "Choose a shortcut for saving clips.");
        }
        if self.output_dir.as_os_str().is_empty() {
            fail("outputDir", "Choose a folder for your clips.");
        }
        let bad_app = |a: &String| a.trim().is_empty() || a.len() > 64 || a.contains(['|', ':']) || a.trim() != a;
        if self.voice_apps.len() > MAX_VOICE_APPS || self.voice_apps.iter().any(bad_app) {
            fail("voiceApps", "List up to 10 app names, without | or : characters.");
        }
        if !CLEANUP_DAYS_RANGE.contains(&self.cleanup_days) {
            fail("cleanupDays", "Keep clips for 1 to 365 days.");
        }
        errors
    }

    /// The video bitrate to record with.
    pub fn bitrate_kbps(&self) -> u32 {
        self.quality.preset_kbps().unwrap_or(self.custom_bitrate_kbps)
    }

    /// Hand-edited or older files can hold values the app would reject; reset just those fields.
    fn with_invalid_fields_reset(mut self) -> Settings {
        let defaults = Settings::default();
        for error in self.validate() {
            match error.field {
                "bufferLocation" => self.buffer_location = defaults.buffer_location,
                "fps" => self.fps = defaults.fps,
                "customBitrateKbps" => self.custom_bitrate_kbps = defaults.custom_bitrate_kbps,
                "bufferSeconds" => self.buffer_seconds = defaults.buffer_seconds,
                "hotkey" => self.hotkey = defaults.hotkey.clone(),
                "outputDir" => self.output_dir = defaults.output_dir.clone(),
                "cleanupDays" => self.cleanup_days = defaults.cleanup_days,
                "voiceApps" => self.voice_apps = defaults.voice_apps.clone(),
                other => debug_assert!(false, "validate() returned a field this function doesn't reset: {other}"),
            }
        }
        self
    }
}

/// What saving new settings must do besides writing the file. Save sound and notification need
/// nothing: the save feedback reads them live.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Effects {
    /// Something the recorder uses changed: restart it (which clears the replay buffer).
    pub restart_recorder: bool,
    /// The save-clip shortcut changed: bind the new one.
    pub rebind_hotkey: bool,
    /// "Start with the OS" changed: turn autostart on or off now.
    pub autostart: bool,
}

impl Effects {
    pub fn between(old: &Settings, new: &Settings) -> Effects {
        let restart_recorder = old.recording_mode != new.recording_mode
            || old.display_id != new.display_id
            || old.resolution != new.resolution
            || old.fps != new.fps
            || old.bitrate_kbps() != new.bitrate_kbps()
            || old.buffer_seconds != new.buffer_seconds
            || old.mic_enabled != new.mic_enabled
            || old.mic_device_id != new.mic_device_id
            || old.output_dir != new.output_dir
            || old.voice_apps != new.voice_apps;
        let canonical = |accel: &str| crate::hotkey::normalize(accel).unwrap_or_else(|_| accel.trim().to_string());
        Effects {
            restart_recorder,
            rebind_hotkey: canonical(&old.hotkey) != canonical(&new.hotkey),
            autostart: old.start_with_os != new.start_with_os,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn defaults_match_the_global_constraints() {
        let s = Settings::default();
        assert_eq!(s.recording_mode, RecordingMode::Always);
        assert_eq!(s.buffer_location, BufferLocation::Ram);
        assert_eq!(s.resolution, Resolution::P1080);
        assert_eq!(s.fps, 60);
        assert_eq!(s.bitrate_kbps(), 20_000);
        assert_eq!(s.buffer_seconds, 30);
        assert!(!s.mic_enabled);
        assert_eq!(s.display_id, None);
        assert_eq!(s.hotkey, "Alt+Backquote");
        assert!(s.output_dir.ends_with("Klipt"), "{}", s.output_dir.display());
        assert!(s.start_with_os && s.save_sound && s.save_notification);
        assert_eq!(s.server_url, None);
        assert!(s.validate().is_empty(), "{:?}", s.validate());
    }

    #[test]
    fn quality_presets_map_to_kbps() {
        let mut s = Settings::default();
        for (q, kbps) in [(Quality::Low, 8_000), (Quality::Medium, 14_000), (Quality::High, 20_000)] {
            s.quality = q;
            assert_eq!(s.bitrate_kbps(), kbps);
        }
        s.quality = Quality::Custom;
        s.custom_bitrate_kbps = 35_000;
        assert_eq!(s.bitrate_kbps(), 35_000);
        assert_eq!(Resolution::P1440.size(), Some((2560, 1440)));
        assert_eq!(Resolution::Native.size(), None);
    }

    #[test]
    fn json_is_camel_case() {
        let json = serde_json::to_value(Settings::default()).unwrap();
        assert_eq!(json["bufferSeconds"], 30);
        assert_eq!(json["recordingMode"], "always");
        assert_eq!(json["resolution"], "p1080");
        assert_eq!(json["bufferLocation"], "ram");
    }

    #[test]
    fn missing_file_gives_defaults_without_a_backup() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Settings::load(dir.path()), Settings::default());
        assert!(!dir.path().join(BACKUP_NAME).exists());
    }

    #[test]
    fn corrupt_file_gives_defaults_and_is_backed_up() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(FILE_NAME), b"{ not json").unwrap();
        assert_eq!(Settings::load(dir.path()), Settings::default());
        assert_eq!(fs::read(dir.path().join(BACKUP_NAME)).unwrap(), b"{ not json");
        assert!(!dir.path().join(FILE_NAME).exists());
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let settings_dir = dir.path().join("nested");
        let s = Settings {
            fps: 30,
            quality: Quality::Custom,
            custom_bitrate_kbps: 12_345,
            mic_enabled: true,
            mic_device_id: Some("mic-1".into()),
            server_url: Some("https://clips.example.com".into()),
            ..Settings::default()
        };
        s.save(&settings_dir).unwrap();
        assert_eq!(Settings::load(&settings_dir), s);
        assert!(!settings_dir.join(TEMP_NAME).exists());
        // Saving again replaces the file.
        let s2 = Settings { buffer_seconds: 120, ..s };
        s2.save(&settings_dir).unwrap();
        assert_eq!(Settings::load(&settings_dir).buffer_seconds, 120);
    }

    #[test]
    fn partial_file_fills_in_defaults() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(FILE_NAME), br#"{"fps":30,"someFutureSetting":true}"#).unwrap();
        let s = Settings::load(dir.path());
        assert_eq!(s.fps, 30);
        assert_eq!(s.buffer_seconds, 30);
    }

    #[test]
    fn out_of_range_values_in_the_file_are_reset() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(FILE_NAME), br#"{"fps":45,"bufferSeconds":5,"micEnabled":true}"#).unwrap();
        let s = Settings::load(dir.path());
        assert_eq!(s.fps, 60);
        assert_eq!(s.buffer_seconds, 30);
        assert!(s.mic_enabled, "valid values are kept");
    }

    #[test]
    fn no_change_has_no_side_effects() {
        let s = Settings::default();
        assert_eq!(Effects::between(&s, &s.clone()), Effects::default());
    }

    #[test]
    fn recording_fields_restart_the_recorder() {
        let old = Settings::default();
        let changes: Vec<Settings> = vec![
            Settings { recording_mode: RecordingMode::WhileGameRunning, ..old.clone() },
            Settings { display_id: Some("DP-1".into()), ..old.clone() },
            Settings { resolution: Resolution::P720, ..old.clone() },
            Settings { fps: 30, ..old.clone() },
            Settings { quality: Quality::Low, ..old.clone() },
            Settings { quality: Quality::Custom, custom_bitrate_kbps: 30_000, ..old.clone() },
            Settings { buffer_seconds: 60, ..old.clone() },
            Settings { mic_enabled: true, ..old.clone() },
            Settings { mic_device_id: Some("alsa_input.usb".into()), ..old.clone() },
            Settings { output_dir: "/elsewhere".into(), ..old.clone() },
        ];
        for new in changes {
            let effects = Effects::between(&old, &new);
            assert_eq!(effects, Effects { restart_recorder: true, ..Effects::default() }, "{new:?}");
        }
    }

    #[test]
    fn same_effective_bitrate_does_not_restart() {
        let old = Settings::default(); // High = 20 000 kbps
        // The hidden custom value changes, but a preset is still selected.
        assert_eq!(Effects::between(&old, &Settings { custom_bitrate_kbps: 50_000, ..old.clone() }), Effects::default());
        // Custom at the preset's bitrate records exactly the same.
        let custom = Settings { quality: Quality::Custom, custom_bitrate_kbps: 20_000, ..old.clone() };
        assert!(!Effects::between(&old, &custom).restart_recorder);
    }

    #[test]
    fn hotkey_autostart_and_feedback_changes() {
        let old = Settings::default();
        let rebind = Effects::between(&old, &Settings { hotkey: "Ctrl+Shift+S".into(), ..old.clone() });
        assert_eq!(rebind, Effects { rebind_hotkey: true, ..Effects::default() });
        // Spelling differences of the same shortcut don't rebind.
        let same = Effects::between(&old, &Settings { hotkey: "alt+`".into(), ..old.clone() });
        assert_eq!(same, Effects::default());
        let autostart = Effects::between(&old, &Settings { start_with_os: false, ..old.clone() });
        assert_eq!(autostart, Effects { autostart: true, ..Effects::default() });
        // Read live by the save feedback: nothing to apply.
        let feedback = Settings { save_sound: false, save_notification: false, ..old.clone() };
        assert_eq!(Effects::between(&old, &feedback), Effects::default());
        let all = Settings { fps: 30, hotkey: "Alt+F8".into(), start_with_os: false, ..old.clone() };
        assert_eq!(
            Effects::between(&old, &all),
            Effects { restart_recorder: true, rebind_hotkey: true, autostart: true }
        );
    }

    #[test]
    fn validate_rejects_out_of_range_values() {
        let fields = |s: Settings| s.validate().into_iter().map(|e| e.field).collect::<Vec<_>>();
        assert_eq!(fields(Settings { buffer_seconds: 5, ..Settings::default() }), ["bufferSeconds"]);
        assert_eq!(fields(Settings { buffer_seconds: 601, ..Settings::default() }), ["bufferSeconds"]);
        assert!(fields(Settings { buffer_seconds: 600, ..Settings::default() }).is_empty());
        assert_eq!(fields(Settings { fps: 45, ..Settings::default() }), ["fps"]);
        let custom = |kbps| Settings { quality: Quality::Custom, custom_bitrate_kbps: kbps, ..Settings::default() };
        assert_eq!(fields(custom(1_999)), ["customBitrateKbps"]);
        assert_eq!(fields(custom(80_001)), ["customBitrateKbps"]);
        // A preset quality ignores the (hidden) custom bitrate.
        assert!(fields(Settings { custom_bitrate_kbps: 1, ..Settings::default() }).is_empty());
        assert_eq!(
            fields(Settings { buffer_location: BufferLocation::Disk, ..Settings::default() }),
            ["bufferLocation"]
        );
        assert_eq!(fields(Settings { hotkey: " ".into(), ..Settings::default() }), ["hotkey"]);
        assert_eq!(fields(Settings { output_dir: PathBuf::new(), ..Settings::default() }), ["outputDir"]);
        let messages = Settings { fps: 45, ..Settings::default() }.validate();
        assert!(messages[0].message.contains("30 or 60"), "{}", messages[0].message);
    }

    #[test]
    fn cleanup_is_off_by_default_and_days_must_be_in_range() {
        let defaults = Settings::default();
        assert!(!defaults.cleanup_enabled);
        assert_eq!(defaults.cleanup_days, 30);
        for (days, ok) in [(0, false), (1, true), (365, true), (366, false)] {
            let s = Settings { cleanup_days: days, ..Settings::default() };
            assert_eq!(s.validate().iter().all(|e| e.field != "cleanupDays"), ok, "{days} days");
        }
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(FILE_NAME), r#"{"cleanupEnabled": true, "cleanupDays": 0}"#).unwrap();
        let loaded = Settings::load(dir.path());
        assert!(loaded.cleanup_enabled);
        assert_eq!(loaded.cleanup_days, 30);
    }

    #[test]
    fn voice_apps_default_to_discord_and_are_checked() {
        assert_eq!(Settings::default().voice_apps, vec!["discord".to_string(), "vesktop".to_string()]);
        let with = |apps: &[&str]| Settings { voice_apps: apps.iter().map(|a| a.to_string()).collect(), ..Settings::default() };
        let ok = |s: Settings| s.validate().iter().all(|e| e.field != "voiceApps");
        assert!(ok(with(&[])));
        assert!(ok(with(&["TeamSpeak", "mumble"])));
        assert!(!ok(with(&["a|b"])));
        assert!(!ok(with(&["app:x"])));
        assert!(!ok(with(&[" "])));
        assert!(!ok(with(&["x"; 11])));
        let old = Settings::default();
        assert!(Effects::between(&old, &with(&["discord"])).restart_recorder);
    }
}
