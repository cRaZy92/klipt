//! Black-box tests of the klipt-recorder binary.
//!
//! The first group needs no OBS. The second group runs only when `KLIPT_TEST_OBS_ROOT` points at
//! an OBS runtime (Windows: the folder holding bin/64bit/obs.dll, core/ and data/; Linux: /usr).

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn recorder() -> Command {
    Command::new(env!("CARGO_BIN_EXE_klipt-recorder"))
}

fn json(line: &str) -> serde_json::Value {
    serde_json::from_str(line).unwrap_or_else(|e| panic!("stdout line is not JSON ({e}): {line}"))
}

#[test]
fn help_prints_usage_and_exits_zero() {
    let out = recorder().arg("--help").output().unwrap();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("--selftest"));
    assert!(text.contains("--hotkey"));
}

#[test]
fn unknown_arguments_exit_64() {
    let out = recorder().arg("--frobnicate").output().unwrap();
    assert_eq!(out.status.code(), Some(64));
    assert!(out.stdout.is_empty(), "nothing may be printed on stdout");
}

#[test]
fn missing_obs_gives_one_error_event_on_stdout() {
    let empty = std::env::temp_dir().join(format!("klipt-no-obs-{}", std::process::id()));
    std::fs::create_dir_all(&empty).unwrap();
    let out = recorder().arg("--obs-root").arg(&empty).stdin(Stdio::null()).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stdout = String::from_utf8(out.stdout).unwrap();
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 1, "stdout: {stdout}");
    let ev = json(lines[0]);
    assert_eq!(ev["event"], "error");
    assert_eq!(ev["code"], "obs_load_failed");
    assert!(ev["message"].as_str().unwrap().len() > 5);
}

// ---------------------------------------------------------------------------------------------

fn obs_root() -> Option<String> {
    std::env::var("KLIPT_TEST_OBS_ROOT").ok().filter(|s| !s.is_empty())
}

/// Reads stdout lines until `count` events arrived or the timeout passes.
fn read_events(reader: &mut impl BufRead, count: usize, timeout: Duration) -> Vec<serde_json::Value> {
    let start = Instant::now();
    let mut events = vec![];
    let mut line = String::new();
    while events.len() < count && start.elapsed() < timeout {
        line.clear();
        if reader.read_line(&mut line).unwrap() == 0 {
            break;
        }
        events.push(json(line.trim_end()));
    }
    events
}

#[test]
fn real_obs_answers_commands_and_reports_errors() {
    let Some(root) = obs_root() else {
        eprintln!("skipped: set KLIPT_TEST_OBS_ROOT to run");
        return;
    };
    let mut child = recorder()
        .args(["--obs-root", &root])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    let ready = read_events(&mut stdout, 1, Duration::from_secs(30));
    assert_eq!(ready[0]["event"], "ready", "{ready:?}");
    assert!(ready[0]["encoders"].as_array().unwrap().iter().any(|e| e == "obs_x264"));

    writeln!(stdin, r#"{{"cmd":"listDisplays"}}"#).unwrap();
    writeln!(stdin, r#"{{"cmd":"listAudioDevices"}}"#).unwrap();
    writeln!(stdin, r#"{{"cmd":"explode"}}"#).unwrap();
    writeln!(stdin, "not json").unwrap();
    writeln!(stdin, r#"{{"cmd":"save"}}"#).unwrap();
    writeln!(stdin, r#"{{"cmd":"shutdown"}}"#).unwrap();
    let events = read_events(&mut stdout, 5, Duration::from_secs(30));
    assert_eq!(events.len(), 5, "{events:?}");

    assert_eq!(events[0]["event"], "displays");
    let displays = events[0]["items"].as_array().unwrap();
    if cfg!(windows) {
        assert!(!displays.is_empty());
        assert_eq!(displays.iter().filter(|d| d["primary"] == true).count(), 1);
        assert!(displays.iter().all(|d| d["width"].as_u64().unwrap() > 0));
    }
    assert_eq!(events[1]["event"], "audioDevices");
    assert!(events[1]["outputs"].as_array().unwrap().iter().any(|d| d["id"] == "default"));
    assert_eq!((&events[2]["event"], &events[2]["code"]), (&"error".into(), &"bad_command".into()));
    assert_eq!((&events[3]["event"], &events[3]["code"]), (&"error".into(), &"bad_command".into()));
    assert_eq!((&events[4]["event"], &events[4]["code"]), (&"error".into(), &"not_recording".into()));

    let status = child.wait().unwrap();
    assert!(status.success(), "{status:?}");
}

#[test]
fn real_obs_exits_when_stdin_closes() {
    let Some(root) = obs_root() else {
        eprintln!("skipped: set KLIPT_TEST_OBS_ROOT to run");
        return;
    };
    let mut child = recorder()
        .args(["--obs-root", &root])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    assert_eq!(read_events(&mut stdout, 1, Duration::from_secs(30))[0]["event"], "ready");
    drop(child.stdin.take()); // the app died
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(15), "recorder did not exit after stdin closed");
        std::thread::sleep(Duration::from_millis(100));
    }
}
