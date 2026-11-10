//! `--selftest`: record without the app. Prints the protocol events plus a final `stats` event
//! with the recorder's own CPU and memory use, for the Windows hardware test (criterion C9).

use crate::protocol::{parse_command, Command, Event, StartParams};
use crate::{emit, hotkey_win, obs, Controller, Flow, Input};
use std::path::PathBuf;
use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq)]
pub struct SelftestArgs {
    pub seconds: u32,
    pub display: Option<String>,
    pub mic: bool,
    pub mic_device: Option<String>,
    pub out: PathBuf,
    pub save_at: Vec<u64>,
    pub exit_after: Option<u64>,
    pub hotkey: Option<String>,
    pub fps: u32,
    pub width: u32,
    pub height: u32,
    pub bitrate: u32,
    pub encoder: String,
}

impl SelftestArgs {
    pub fn parse(args: &[String]) -> Result<Self, String> {
        let mut s = SelftestArgs {
            seconds: 30,
            display: None,
            mic: false,
            mic_device: None,
            out: default_out_dir(),
            save_at: vec![],
            exit_after: None,
            hotkey: None,
            fps: 60,
            width: 1920,
            height: 1080,
            bitrate: 20000,
            encoder: "auto".into(),
        };
        let mut it = args.iter();
        while let Some(a) = it.next() {
            let mut value = |name: &str| it.next().cloned().ok_or_else(|| format!("{name} needs a value"));
            let num = |name: &str, v: String| v.parse::<u64>().map_err(|_| format!("{name}: \"{v}\" is not a number"));
            match a.as_str() {
                "--selftest" => {}
                "--mic" => s.mic = true,
                "--seconds" => s.seconds = num(a, value(a)?)? as u32,
                "--display" => s.display = Some(value(a)?),
                "--mic-device" => {
                    s.mic_device = Some(value(a)?);
                    s.mic = true;
                }
                // Made absolute now: the working directory changes when libobs starts.
                "--out" => {
                    let v = value(a)?;
                    s.out = std::path::absolute(&v).map_err(|e| format!("--out {v}: {e}"))?;
                }
                "--save-at" => {
                    for part in value(a)?.split(',').filter(|p| !p.trim().is_empty()) {
                        s.save_at.push(num(a, part.trim().to_string())?);
                    }
                    s.save_at.sort_unstable();
                }
                "--exit-after" => s.exit_after = Some(num(a, value(a)?)?),
                "--hotkey" => {
                    let h = value(a)?;
                    hotkey_win::parse_accelerator(&h)?;
                    s.hotkey = Some(h);
                }
                "--fps" => s.fps = num(a, value(a)?)? as u32,
                "--width" => s.width = num(a, value(a)?)? as u32,
                "--height" => s.height = num(a, value(a)?)? as u32,
                "--bitrate" => s.bitrate = num(a, value(a)?)? as u32,
                "--encoder" => s.encoder = value(a)?,
                other => return Err(format!("unknown selftest option {other} (see --help)")),
            }
        }
        Ok(s)
    }

    pub fn start_params(&self) -> StartParams {
        let out = &self.out;
        StartParams {
            display_id: self.display.clone(),
            fps: self.fps,
            width: self.width,
            height: self.height,
            bitrate_kbps: self.bitrate,
            buffer_seconds: self.seconds,
            mic: self.mic,
            mic_device_id: self.mic_device.clone(),
            output_dir: out.to_string_lossy().into_owned(),
            encoder: self.encoder.clone(),
        }
    }
}

fn default_out_dir() -> PathBuf {
    let home =
        std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from).unwrap_or_default();
    home.join("Videos").join("Klipt-test")
}

pub fn run(ctl: &mut Controller, args: SelftestArgs) -> i32 {
    let params = args.start_params();
    if let Err(e) = params.validate() {
        log!("klipt-recorder: {e}");
        return 64;
    }
    ctl.command(Command::ListDisplays);
    ctl.command(Command::ListAudioDevices);
    ctl.command(Command::Start(params.clone()));
    if ctl.session.is_none() {
        return 1;
    }
    let started = Instant::now();
    let cpu_at_start = process_cpu_seconds();

    let mut how_to_save = vec!["type s + Enter".to_string()];
    if let Some(h) = &args.hotkey {
        let accel = hotkey_win::parse_accelerator(h).expect("validated in parse");
        let hotkey_tx = ctl.tx.clone();
        match hotkey_win::listen(accel, move || {
            let _ = hotkey_tx.send(Input::Hotkey);
        }) {
            Ok(()) => how_to_save.insert(0, format!("press {h}")),
            Err(e) => log!("hotkey {h} not available: {e}"),
        }
    }
    log!(
        "Recording to {} (buffer {} s). To save a clip: {}. To stop: type q + Enter{}.",
        params.output_dir,
        args.seconds,
        how_to_save.join(" or "),
        args.exit_after.map(|n| format!(" or wait {n} s")).unwrap_or_default()
    );

    let mut save_at = args.save_at.clone();
    loop {
        let input = ctl.rx.recv_timeout(Duration::from_millis(50));
        match input {
            Ok(Input::Line(line)) => match line.trim().to_ascii_lowercase().as_str() {
                "" => {}
                "s" | "save" => ctl.save(),
                "q" | "quit" | "stop" => break,
                text if text.starts_with('{') => match parse_command(&line) {
                    Ok(Command::Stop) | Ok(Command::Shutdown) => break,
                    Ok(cmd) => {
                        if let Flow::Exit(code) = ctl.command(cmd) {
                            return code;
                        }
                    }
                    Err(e) => emit(&e.to_event()),
                },
                other => log!("unknown input \"{other}\" (s = save, q = stop)"),
            },
            // stdin may be closed or redirected from NUL; the hotkey and timers still work.
            Ok(Input::StdinClosed) => {}
            Ok(Input::Obs(signal)) => {
                if let Flow::Exit(code) = ctl.signal(signal) {
                    return code;
                }
            }
            Ok(Input::Hotkey) => ctl.save(),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        let elapsed = started.elapsed().as_secs();
        while save_at.first().is_some_and(|&t| t <= elapsed) {
            save_at.remove(0);
            ctl.save();
        }
        if args.exit_after.is_some_and(|n| elapsed >= n) {
            break;
        }
        obs::pump_events();
        ctl.tick();
    }

    // A save still being written is reported before the stats, so they cover it.
    ctl.wait_for_pending_save();
    let cpu = process_cpu_seconds() - cpu_at_start;
    emit(&Event::stats(cpu, started.elapsed().as_secs_f64(), peak_memory_bytes()));
    match ctl.command(Command::Stop) {
        Flow::Exit(code) => code,
        Flow::Continue => 0,
    }
}

/// User + kernel CPU time of this process, in seconds.
#[cfg(windows)]
fn process_cpu_seconds() -> f64 {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
    let ticks = |t: FILETIME| ((t.dwHighDateTime as u64) << 32 | t.dwLowDateTime as u64) as f64 / 1e7;
    unsafe {
        let zero = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
        let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
        if GetProcessTimes(GetCurrentProcess(), &mut created, &mut exited, &mut kernel, &mut user) == 0 {
            return 0.0;
        }
        ticks(kernel) + ticks(user)
    }
}

/// Peak working set (Windows) / max RSS (Linux) of this process, in bytes.
#[cfg(windows)]
fn peak_memory_bytes() -> u64 {
    use windows_sys::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    unsafe {
        let mut counters: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
        counters.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        if GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) == 0 {
            return 0;
        }
        counters.PeakWorkingSetSize as u64
    }
}

#[cfg(not(windows))]
#[repr(C)]
struct Timeval {
    sec: i64,
    usec: i64,
}

/// `struct rusage` on x86_64 Linux: two timevals, then 14 longs starting with ru_maxrss.
#[cfg(not(windows))]
#[repr(C)]
struct Rusage {
    utime: Timeval,
    stime: Timeval,
    maxrss: i64,
    _rest: [i64; 13],
}

#[cfg(not(windows))]
extern "C" {
    fn getrusage(who: i32, usage: *mut Rusage) -> i32;
}

#[cfg(not(windows))]
fn rusage() -> Rusage {
    unsafe {
        let mut r: Rusage = std::mem::zeroed();
        getrusage(0, &mut r); // RUSAGE_SELF
        r
    }
}

#[cfg(not(windows))]
fn process_cpu_seconds() -> f64 {
    let r = rusage();
    let secs = |t: &Timeval| t.sec as f64 + t.usec as f64 / 1e6;
    secs(&r.utime) + secs(&r.stime)
}

#[cfg(not(windows))]
fn peak_memory_bytes() -> u64 {
    rusage().maxrss as u64 * 1024 // ru_maxrss is in KiB on Linux
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn defaults_match_the_app_defaults() {
        let s = SelftestArgs::parse(&args(&["--selftest"])).unwrap();
        assert_eq!((s.seconds, s.fps, s.width, s.height, s.bitrate), (30, 60, 1920, 1080, 20000));
        assert_eq!(s.encoder, "auto");
        assert!(!s.mic);
        assert!(s.out.ends_with("Videos/Klipt-test") || s.out.ends_with("Videos\\Klipt-test"));
    }

    #[test]
    fn parses_the_windows_test_command() {
        let s = SelftestArgs::parse(&args(&[
            "--selftest",
            "--mic",
            "--hotkey",
            "ALT+F9",
            "--exit-after",
            "180",
            "--out",
            "C:\\clips",
            "--save-at",
            "40,5",
        ]))
        .unwrap();
        assert!(s.mic);
        assert_eq!(s.hotkey.as_deref(), Some("ALT+F9"));
        assert_eq!(s.exit_after, Some(180));
        assert_eq!(s.out, PathBuf::from("C:\\clips"));
        assert_eq!(s.save_at, vec![5, 40]);
        let p = s.start_params();
        assert!(p.mic);
        assert_eq!(p.buffer_seconds, 30);
        assert!(p.validate().is_ok());
    }

    #[test]
    fn mic_device_implies_mic() {
        let s = SelftestArgs::parse(&args(&["--selftest", "--mic-device", "{abc}"])).unwrap();
        assert!(s.mic);
        assert_eq!(s.start_params().mic_device_id.as_deref(), Some("{abc}"));
    }

    #[test]
    fn rejects_bad_options() {
        assert!(SelftestArgs::parse(&args(&["--selftest", "--seconds"])).is_err());
        assert!(SelftestArgs::parse(&args(&["--selftest", "--seconds", "abc"])).is_err());
        assert!(SelftestArgs::parse(&args(&["--selftest", "--hotkey", "Hyper+X"])).is_err());
        assert!(SelftestArgs::parse(&args(&["--selftest", "--frobnicate"])).is_err());
    }
}
