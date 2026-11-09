//! JSON-lines protocol between the Klipt app and `klipt-recorder`.
//!
//! - stdin: one [`Command`] per line
//! - stdout: one [`Event`] per line, flushed after every line
//! - stderr: libobs and recorder logs (never parsed)

use serde::{Deserialize, Serialize};

/// A request from the app.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "camelCase")]
pub enum Command {
    ListDisplays,
    ListAudioDevices,
    Start(StartParams),
    Save,
    Stop,
    Shutdown,
}

const KNOWN_COMMANDS: &[&str] = &["listDisplays", "listAudioDevices", "start", "save", "stop", "shutdown"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartParams {
    /// A display id from `listDisplays`; `null` means the primary display.
    pub display_id: Option<String>,
    pub fps: u32,
    /// The output is the display scaled to fit inside `width`×`height`, keeping its aspect ratio.
    pub width: u32,
    pub height: u32,
    pub bitrate_kbps: u32,
    pub buffer_seconds: u32,
    pub mic: bool,
    /// An input id from `listAudioDevices`; `null` means the system default microphone.
    pub mic_device_id: Option<String>,
    pub output_dir: String,
    /// `"auto"` or a specific libobs encoder id.
    pub encoder: String,
}

impl StartParams {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=240).contains(&self.fps) {
            return Err(format!("fps must be 1–240, got {}", self.fps));
        }
        if self.width < 16 || self.height < 16 || self.width > 7680 || self.height > 4320 {
            return Err(format!("unsupported size {}x{}", self.width, self.height));
        }
        if !(500..=200_000).contains(&self.bitrate_kbps) {
            return Err(format!("bitrateKbps must be 500–200000, got {}", self.bitrate_kbps));
        }
        if !(1..=3600).contains(&self.buffer_seconds) {
            return Err(format!("bufferSeconds must be 1–3600, got {}", self.buffer_seconds));
        }
        if self.output_dir.trim().is_empty() {
            return Err("outputDir must not be empty".into());
        }
        if self.encoder.trim().is_empty() {
            return Err("encoder must be \"auto\" or an encoder id".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProtocolError {
    /// Not JSON, or not an object with a string `cmd`.
    BadJson(String),
    UnknownCommand(String),
    /// A known command with missing or invalid fields.
    InvalidArgs(String),
}

impl ProtocolError {
    pub fn to_event(&self) -> Event {
        let message = match self {
            ProtocolError::BadJson(e) => format!("Not a valid command line: {e}"),
            ProtocolError::UnknownCommand(c) => format!("Unknown command \"{c}\""),
            ProtocolError::InvalidArgs(e) => format!("Invalid arguments: {e}"),
        };
        Event::error(ErrorCode::BadCommand, message)
    }
}

pub fn parse_command(line: &str) -> Result<Command, ProtocolError> {
    // A UTF-8 BOM (written by some Windows stream writers before the first line) is not JSON.
    let line = line.trim().trim_start_matches('\u{feff}');
    let value: serde_json::Value = serde_json::from_str(line).map_err(|e| ProtocolError::BadJson(e.to_string()))?;
    let name = value
        .get("cmd")
        .and_then(|c| c.as_str())
        .ok_or_else(|| ProtocolError::BadJson("expected an object with a string \"cmd\"".into()))?;
    if !KNOWN_COMMANDS.contains(&name) {
        return Err(ProtocolError::UnknownCommand(name.to_string()));
    }
    let cmd: Command = serde_json::from_value(value).map_err(|e| ProtocolError::InvalidArgs(e.to_string()))?;
    if let Command::Start(params) = &cmd {
        params.validate().map_err(ProtocolError::InvalidArgs)?;
    }
    Ok(cmd)
}

/// A notification to the app.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "event", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Event {
    Ready { version: String, platform: String, encoders: Vec<String> },
    Displays { items: Vec<DisplayInfo> },
    AudioDevices { outputs: Vec<AudioDevice>, inputs: Vec<AudioDevice> },
    Started { encoder: String, width: u32, height: u32 },
    Saved { path: String, duration_ms: u64 },
    Stopped,
    Error { code: ErrorCode, message: String },
    Stats { cpu_seconds: f64, wall_seconds: f64, avg_cpu_percent_of_one_core: f64, max_rss_mb: u64 },
}

impl Event {
    pub fn error(code: ErrorCode, message: impl Into<String>) -> Self {
        Event::Error { code, message: message.into() }
    }

    pub fn stats(cpu_seconds: f64, wall_seconds: f64, max_rss_bytes: u64) -> Self {
        let round = |v: f64, places: i32| {
            let f = 10f64.powi(places);
            (v * f).round() / f
        };
        let avg = if wall_seconds > 0.0 { 100.0 * cpu_seconds / wall_seconds } else { 0.0 };
        Event::Stats {
            cpu_seconds: round(cpu_seconds, 2),
            wall_seconds: round(wall_seconds, 1),
            avg_cpu_percent_of_one_core: round(avg, 1),
            max_rss_mb: max_rss_bytes / (1024 * 1024),
        }
    }

    /// One JSON line without the trailing newline. serde_json escapes control characters,
    /// so the line never contains a raw newline.
    pub fn to_line(&self) -> String {
        serde_json::to_string(self).expect("events always serialize")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DisplayInfo {
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub primary: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// Recording could not start (source, encoder or output failure).
    StartFailed,
    /// No usable H.264 encoder could be created.
    NoEncoder,
    /// The requested display doesn't exist or delivers no frames.
    NoDisplay,
    /// `save` while a save is still running (the request is ignored).
    Busy,
    /// `save` before a successful `start`.
    NotRecording,
    /// `start` while already recording.
    AlreadyRecording,
    /// The replay buffer didn't produce a file.
    SaveFailed,
    /// Recording stopped on its own (encoder error, display lost). The process exits after this.
    RecordingStopped,
    /// libobs or a required module could not be loaded. The process exits after this.
    ObsLoadFailed,
    /// The command line could not be understood.
    BadCommand,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn start_params() -> StartParams {
        StartParams {
            display_id: None,
            fps: 60,
            width: 1920,
            height: 1080,
            bitrate_kbps: 20000,
            buffer_seconds: 30,
            mic: true,
            mic_device_id: None,
            output_dir: "C:\\Users\\a\\Videos\\Klipt".into(),
            encoder: "auto".into(),
        }
    }

    #[test]
    fn parses_simple_commands() {
        assert_eq!(parse_command(r#"{"cmd":"listDisplays"}"#), Ok(Command::ListDisplays));
        assert_eq!(parse_command(r#"{"cmd":"listAudioDevices"}"#), Ok(Command::ListAudioDevices));
        assert_eq!(parse_command(r#"{"cmd":"save"}"#), Ok(Command::Save));
        assert_eq!(parse_command(r#"{"cmd":"stop"}"#), Ok(Command::Stop));
        assert_eq!(parse_command(r#"{"cmd":"shutdown"}"#), Ok(Command::Shutdown));
        // surrounding whitespace / CRLF from Windows pipes is tolerated
        assert_eq!(parse_command("  {\"cmd\":\"save\"}\r\n"), Ok(Command::Save));
        // a UTF-8 byte order mark before the first command is tolerated
        assert_eq!(parse_command("\u{feff}{\"cmd\":\"save\"}"), Ok(Command::Save));
    }

    #[test]
    fn parses_start_with_all_fields() {
        let line = r#"{"cmd":"start","displayId":"\\\\?\\DISPLAY#GSM5B7F#1","fps":60,"width":1920,"height":1080,"bitrateKbps":20000,"bufferSeconds":30,"mic":true,"micDeviceId":"{0.0.1.00000000}.{abc}","outputDir":"C:\\clips","encoder":"obs_x264"}"#;
        assert_eq!(
            parse_command(line),
            Ok(Command::Start(StartParams {
                display_id: Some("\\\\?\\DISPLAY#GSM5B7F#1".into()),
                mic_device_id: Some("{0.0.1.00000000}.{abc}".into()),
                output_dir: "C:\\clips".into(),
                encoder: "obs_x264".into(),
                ..start_params()
            }))
        );
    }

    #[test]
    fn start_accepts_null_or_missing_optional_ids() {
        let with_nulls = r#"{"cmd":"start","displayId":null,"fps":60,"width":1920,"height":1080,"bitrateKbps":20000,"bufferSeconds":30,"mic":true,"micDeviceId":null,"outputDir":"C:\\Users\\a\\Videos\\Klipt","encoder":"auto"}"#;
        let without = r#"{"cmd":"start","fps":60,"width":1920,"height":1080,"bitrateKbps":20000,"bufferSeconds":30,"mic":true,"outputDir":"C:\\Users\\a\\Videos\\Klipt","encoder":"auto"}"#;
        assert_eq!(parse_command(with_nulls), Ok(Command::Start(start_params())));
        assert_eq!(parse_command(without), Ok(Command::Start(start_params())));
    }

    #[test]
    fn every_command_round_trips() {
        let all = [
            Command::ListDisplays,
            Command::ListAudioDevices,
            Command::Start(start_params()),
            Command::Start(StartParams { display_id: Some("d1".into()), mic: false, ..start_params() }),
            Command::Save,
            Command::Stop,
            Command::Shutdown,
        ];
        for cmd in all {
            let line = serde_json::to_string(&cmd).unwrap();
            assert_eq!(parse_command(&line), Ok(cmd), "line: {line}");
        }
    }

    #[test]
    fn unknown_command_is_reported_by_name() {
        assert_eq!(parse_command(r#"{"cmd":"explode"}"#), Err(ProtocolError::UnknownCommand("explode".into())));
    }

    #[test]
    fn malformed_json_is_bad_json() {
        assert!(matches!(parse_command("{\"cmd\":"), Err(ProtocolError::BadJson(_))));
        assert!(matches!(parse_command("save"), Err(ProtocolError::BadJson(_))));
        assert!(matches!(parse_command(r#"["save"]"#), Err(ProtocolError::BadJson(_))));
        assert!(matches!(parse_command(r#"{"command":"save"}"#), Err(ProtocolError::BadJson(_))));
        assert!(matches!(parse_command(r#"{"cmd":7}"#), Err(ProtocolError::BadJson(_))));
    }

    #[test]
    fn start_with_missing_or_invalid_fields_is_invalid_args() {
        assert!(matches!(parse_command(r#"{"cmd":"start","fps":60}"#), Err(ProtocolError::InvalidArgs(_))));
        let zero_fps = r#"{"cmd":"start","fps":0,"width":1920,"height":1080,"bitrateKbps":20000,"bufferSeconds":30,"mic":false,"outputDir":"C:\\x","encoder":"auto"}"#;
        assert!(matches!(parse_command(zero_fps), Err(ProtocolError::InvalidArgs(_))));
        let empty_dir = r#"{"cmd":"start","fps":60,"width":1920,"height":1080,"bitrateKbps":20000,"bufferSeconds":30,"mic":false,"outputDir":"","encoder":"auto"}"#;
        assert!(matches!(parse_command(empty_dir), Err(ProtocolError::InvalidArgs(_))));
    }

    #[test]
    fn protocol_errors_map_to_bad_command_events() {
        let ev = ProtocolError::UnknownCommand("explode".into()).to_event();
        assert_eq!(ev.to_line(), r#"{"event":"error","code":"bad_command","message":"Unknown command \"explode\""}"#);
    }

    #[test]
    fn ready_event_shape() {
        let ev = Event::Ready {
            version: "0.1.0".into(),
            platform: "windows".into(),
            encoders: vec!["obs_nvenc_h264_tex".into(), "obs_x264".into()],
        };
        assert_eq!(
            ev.to_line(),
            r#"{"event":"ready","version":"0.1.0","platform":"windows","encoders":["obs_nvenc_h264_tex","obs_x264"]}"#
        );
    }

    #[test]
    fn displays_event_shape() {
        let ev = Event::Displays {
            items: vec![DisplayInfo {
                id: r"\\?\DISPLAY#GSM5B7F#1".into(),
                name: "LG ULTRAGEAR".into(),
                width: 2560,
                height: 1440,
                primary: true,
            }],
        };
        assert_eq!(
            ev.to_line(),
            r#"{"event":"displays","items":[{"id":"\\\\?\\DISPLAY#GSM5B7F#1","name":"LG ULTRAGEAR","width":2560,"height":1440,"primary":true}]}"#
        );
    }

    #[test]
    fn audio_devices_event_shape() {
        let ev = Event::AudioDevices {
            outputs: vec![AudioDevice { id: "default".into(), name: "Default".into() }],
            inputs: vec![],
        };
        assert_eq!(
            ev.to_line(),
            r#"{"event":"audioDevices","outputs":[{"id":"default","name":"Default"}],"inputs":[]}"#
        );
    }

    #[test]
    fn started_saved_stopped_event_shapes() {
        assert_eq!(
            Event::Started { encoder: "obs_nvenc_h264_tex".into(), width: 1920, height: 1080 }.to_line(),
            r#"{"event":"started","encoder":"obs_nvenc_h264_tex","width":1920,"height":1080}"#
        );
        assert_eq!(
            Event::Saved { path: r"C:\clips\Klipt 2026-11-09 22-00-00.mp4".into(), duration_ms: 30000 }.to_line(),
            r#"{"event":"saved","path":"C:\\clips\\Klipt 2026-11-09 22-00-00.mp4","durationMs":30000}"#
        );
        assert_eq!(Event::Stopped.to_line(), r#"{"event":"stopped"}"#);
    }

    #[test]
    fn error_event_shapes() {
        assert_eq!(
            Event::error(ErrorCode::Busy, "Already saving").to_line(),
            r#"{"event":"error","code":"busy","message":"Already saving"}"#
        );
        for (code, text) in [
            (ErrorCode::StartFailed, "start_failed"),
            (ErrorCode::NoEncoder, "no_encoder"),
            (ErrorCode::NoDisplay, "no_display"),
            (ErrorCode::NotRecording, "not_recording"),
            (ErrorCode::AlreadyRecording, "already_recording"),
            (ErrorCode::SaveFailed, "save_failed"),
            (ErrorCode::RecordingStopped, "recording_stopped"),
            (ErrorCode::ObsLoadFailed, "obs_load_failed"),
        ] {
            let line = Event::error(code, "x").to_line();
            assert_eq!(line, format!(r#"{{"event":"error","code":"{text}","message":"x"}}"#));
        }
    }

    #[test]
    fn stats_event_shape_rounds_numbers() {
        let ev = Event::stats(12.3456, 180.04, 2_150 * 1024 * 1024 / 10);
        assert_eq!(
            ev.to_line(),
            r#"{"event":"stats","cpuSeconds":12.35,"wallSeconds":180.0,"avgCpuPercentOfOneCore":6.9,"maxRssMb":215}"#
        );
    }

    #[test]
    fn event_lines_never_contain_newlines() {
        let ev = Event::error(ErrorCode::StartFailed, "line one\nline two");
        assert!(!ev.to_line().contains('\n'));
    }
}
