//! What a problem report says about the PC: app version, OS, CPU, RAM, graphics cards and how
//! Klipt is set to record. Also what the app tells the server (`POST /api/me/device`) so the
//! admin page can show who runs which version on what.

use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};

use crate::settings::Settings;

/// One line of system info, as label and value (kept in order).
pub type Entry = (String, String);

/// The OS with its version, e.g. "Windows 11 Pro (26100)" or "CachyOS Linux (6.17.2-cachyos)".
pub fn os_name() -> String {
    let long = System::long_os_version().ok().filter(|s| !s.trim().is_empty());
    let kernel = System::kernel_version().ok().filter(|s| !s.trim().is_empty());
    match (long, kernel) {
        (Some(os), Some(k)) if !os.contains(&k) => format!("{os} ({k})"),
        (Some(os), _) => os,
        (None, Some(k)) => format!("{} ({k})", std::env::consts::OS),
        (None, None) => std::env::consts::OS.to_string(),
    }
}

/// The graphics cards' names (with the driver version where known).
pub fn gpus() -> Vec<String> {
    platform::gpus()
}

fn gib(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
}

/// Everything a problem report includes about this PC and the app.
pub fn system_info(app_version: &str, settings: &Settings) -> Vec<Entry> {
    let mut out: Vec<Entry> = Vec::new();
    let mut add = |k: &str, v: String| {
        if !v.trim().is_empty() {
            out.push((k.to_string(), v));
        }
    };
    let commit = option_env!("KLIPT_COMMIT").filter(|c| !c.is_empty()).map(|c| c.chars().take(7).collect::<String>());
    add(
        "App",
        format!(
            "Klipt {app_version}{}{}",
            commit.map(|c| format!(" ({c})")).unwrap_or_default(),
            if cfg!(debug_assertions) { " dev build" } else { "" }
        ),
    );
    add("OS", os_name());
    if let Ok(system) = System::new_with_specifics(
        RefreshKind::nothing().with_cpu(CpuRefreshKind::nothing()).with_memory(MemoryRefreshKind::nothing().with_ram()),
    ) {
        let cpus = system.cpus();
        if let Some(cpu) = cpus.first() {
            let cores = System::physical_core_count().map(|c| format!(", {c} cores")).unwrap_or_default();
            add("CPU", format!("{}{cores}, {} threads", cpu.brand().trim(), cpus.len()));
        }
        add("RAM", gib(system.total_memory()));
    }
    for (i, gpu) in gpus().into_iter().enumerate() {
        add(if i == 0 { "GPU" } else { "GPU 2+" }, gpu);
    }
    #[cfg(target_os = "linux")]
    {
        let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
        let session = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();
        add("Desktop", format!("{desktop} {session}").trim().to_string());
    }
    add("Encoder", crate::recorder::current_encoder().unwrap_or_else(|| "not recording".into()));
    add(
        "Recording",
        format!(
            "{:?}, {} s buffer in {:?}, {:?}, {} fps, quality {:?}, mic {}, voice apps {}",
            settings.recording_mode,
            settings.buffer_seconds,
            settings.buffer_location,
            settings.resolution,
            settings.fps,
            settings.quality,
            if settings.mic_enabled { "on" } else { "off" },
            if settings.voice_apps.is_empty() { "none".to_string() } else { settings.voice_apps.join(", ") },
        ),
    );
    add(
        "Shortcuts",
        format!(
            "save {}, screenshot {}, record {}",
            settings.hotkey, settings.screenshot_hotkey, settings.record_hotkey
        ),
    );
    out
}

/// The system info as text, for the top of a report's log.
pub fn as_text(entries: &[Entry]) -> String {
    entries.iter().map(|(k, v)| format!("{k}: {v}\n")).collect()
}

#[cfg(target_os = "linux")]
mod platform {
    use std::process::Command;

    /// `lspci -mm` lines for display controllers, plus the NVIDIA driver version when loaded.
    pub fn gpus() -> Vec<String> {
        let mut out: Vec<String> = Command::new("lspci")
            .arg("-mm")
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).lines().filter_map(parse_lspci_line).collect())
            .unwrap_or_default();
        if out.is_empty() {
            out = sysfs_gpus();
        }
        if let Some(version) = nvidia_driver() {
            for gpu in out.iter_mut().filter(|g| g.to_ascii_lowercase().contains("nvidia")) {
                gpu.push_str(&format!(", driver {version}"));
            }
        }
        out
    }

    /// `00:02.0 "VGA compatible controller" "Intel Corporation" "Device name" …` → "Intel Corporation Device name".
    pub fn parse_lspci_line(line: &str) -> Option<String> {
        let fields: Vec<&str> = line.split('"').collect();
        // [slot, class, " ", vendor, " ", device, …]
        let class = fields.get(1)?;
        if !(class.contains("VGA") || class.contains("3D controller") || class.contains("Display controller")) {
            return None;
        }
        let vendor = fields.get(3)?.trim();
        let device = fields.get(5)?.trim();
        Some(format!("{vendor} {device}").trim().to_string())
    }

    /// When lspci is missing: the PCI ids and kernel driver of each DRM card.
    fn sysfs_gpus() -> Vec<String> {
        let Ok(entries) = std::fs::read_dir("/sys/class/drm") else { return Vec::new() };
        let mut out = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with("card") || name.contains('-') {
                continue;
            }
            let dev = entry.path().join("device");
            let read = |f: &str| std::fs::read_to_string(dev.join(f)).map(|s| s.trim().to_string()).unwrap_or_default();
            let driver = std::fs::read_link(dev.join("driver"))
                .ok()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                .unwrap_or_default();
            out.push(format!("PCI {}:{} ({driver})", read("vendor"), read("device")));
        }
        out
    }

    fn nvidia_driver() -> Option<String> {
        let text = std::fs::read_to_string("/proc/driver/nvidia/version").ok()?;
        // "NVRM version: NVIDIA UNIX Open Kernel Module for x86_64  580.95.05  Release Build …"
        text.lines()
            .next()?
            .split_whitespace()
            .find(|w| w.chars().next().is_some_and(|c| c.is_ascii_digit()) && w.contains('.'))
            .map(String::from)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn reads_display_controllers_from_lspci() {
            let line = r#"01:00.0 "VGA compatible controller" "NVIDIA Corporation" "GA104 [GeForce RTX 3070 Ti]" -ra1 "ASUSTeK" "Device 8851""#;
            assert_eq!(parse_lspci_line(line).as_deref(), Some("NVIDIA Corporation GA104 [GeForce RTX 3070 Ti]"));
            assert_eq!(parse_lspci_line(r#"00:1f.3 "Audio device" "Intel" "Sound""#), None);
        }
    }
}

#[cfg(windows)]
mod platform {
    use windows::core::{w, HSTRING};
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};

    /// The display adapters Windows knows (its "Display adapters" device class), with driver versions.
    pub fn gpus() -> Vec<String> {
        let mut out = Vec::new();
        for i in 0..16 {
            let key = HSTRING::from(format!(
                "SYSTEM\\CurrentControlSet\\Control\\Class\\{{4d36e968-e325-11ce-bfc1-08002be10318}}\\{i:04}"
            ));
            let Some(name) = read_string(&key, w!("DriverDesc")) else { continue };
            if name.contains("Basic Display") || name.contains("Basic Render") || name.contains("Remote Display") {
                continue;
            }
            let entry = match read_string(&key, w!("DriverVersion")) {
                Some(version) => format!("{name}, driver {version}"),
                None => name,
            };
            if !out.contains(&entry) {
                out.push(entry);
            }
        }
        out
    }

    fn read_string(key: &HSTRING, value: windows::core::PCWSTR) -> Option<String> {
        unsafe {
            let mut buf = [0u16; 512];
            let mut bytes = (buf.len() * 2) as u32;
            let status = RegGetValueW(
                HKEY_LOCAL_MACHINE,
                key,
                value,
                RRF_RT_REG_SZ,
                None,
                Some(buf.as_mut_ptr().cast()),
                Some(&mut bytes),
            );
            if status.is_err() {
                return None;
            }
            // `bytes` includes the terminating NUL.
            let chars = (bytes as usize / 2).saturating_sub(1).min(buf.len());
            let text = String::from_utf16_lossy(&buf[..chars]).trim().to_string();
            (!text.is_empty()).then_some(text)
        }
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod platform {
    pub fn gpus() -> Vec<String> {
        Vec::new()
    }
}
