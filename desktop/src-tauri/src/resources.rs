//! Settings → Resources: what Klipt's own processes use right now. The UI asks about once a
//! second while that page is open; nothing is measured otherwise.
//!
//! Klipt is a tree of processes: the app, the web view that draws its window, the recorder
//! helper and short ffmpeg jobs. Each process under ours is put in one [`Part`] by its name, or
//! the part of the process that started it. RAM and CPU come from `sysinfo` on every OS. GPU load
//! and VRAM come from NVML on Linux (NVIDIA only) and from Windows' own GPU counters on Windows.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde::Serialize;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

use crate::commands::lock;
use crate::errors::{AppError, Result};

/// A part of Klipt, as the Resources page lists it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Part {
    /// The app itself.
    App,
    /// The web view that draws the window (WebKitGTK helpers on Linux, WebView2 on Windows).
    Window,
    /// The recorder helper: gpu-screen-recorder on Linux, klipt-recorder (OBS) on Windows.
    Recorder,
    /// ffmpeg jobs: exports, imports, editor pictures, screenshots.
    VideoJobs,
}

const PARTS: [Part; 4] = [Part::App, Part::Window, Part::Recorder, Part::VideoJobs];

/// The part a process belongs to by its name alone, if the name says. Linux names are cut to 15
/// characters (`gpu-screen-reco`, `WebKitWebProces`), so these are prefixes.
pub fn part_by_name(name: &str) -> Option<Part> {
    let lower = name.to_ascii_lowercase();
    let n = lower.strip_suffix(".exe").unwrap_or(&lower);
    if n.starts_with("webkit") || n.starts_with("msedgewebview2") || n == "bwrap" || n == "xdg-dbus-proxy" {
        Some(Part::Window)
    } else if n.starts_with("gpu-screen-rec") || n.starts_with("gsr-") || n.starts_with("klipt-recorder") {
        Some(Part::Recorder)
    } else if n.starts_with("ffmpeg") || n.starts_with("ffprobe") {
        Some(Part::VideoJobs)
    } else {
        None
    }
}

/// One running process, as far as sorting it into parts goes.
pub struct Proc<'a> {
    pub pid: u32,
    pub parent: Option<u32>,
    pub name: &'a str,
}

/// `root` and every process started under it, each with its part. `root` is the app; others take
/// the part their name says, else their parent's (e.g. `obs-ffmpeg-mux` under the recorder).
pub fn assign_parts(root: u32, procs: &[Proc]) -> HashMap<u32, Part> {
    let mut children: HashMap<u32, Vec<&Proc>> = HashMap::new();
    for p in procs {
        if let Some(parent) = p.parent.filter(|&parent| parent != p.pid) {
            children.entry(parent).or_default().push(p);
        }
    }
    let mut parts = HashMap::from([(root, Part::App)]);
    let mut queue = vec![(root, Part::App)];
    while let Some((pid, part)) = queue.pop() {
        for child in children.get(&pid).into_iter().flatten() {
            if parts.contains_key(&child.pid) {
                continue;
            }
            let child_part = part_by_name(child.name).unwrap_or(part);
            parts.insert(child.pid, child_part);
            queue.push((child.pid, child_part));
        }
    }
    parts
}

/// What one process uses. `cpu_percent` is of the whole CPU (all threads), 0–100.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ProcStat {
    pub ram_bytes: u64,
    pub cpu_percent: f32,
    pub gpu_percent: Option<f32>,
    pub vram_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PartUsage {
    pub part: Part,
    /// How many processes of this part are running.
    pub processes: u32,
    pub ram_bytes: u64,
    pub cpu_percent: f32,
    /// None when no GPU numbers could be read.
    pub gpu_percent: Option<f32>,
    pub vram_bytes: Option<u64>,
}

/// Adds up processes by part; every part is listed, in a fixed order, running or not. With
/// `gpu` false, the GPU fields stay None; with it true, a process with no reading counts as 0.
pub fn sum_parts(parts: &HashMap<u32, Part>, stats: &HashMap<u32, ProcStat>, gpu: bool) -> Vec<PartUsage> {
    PARTS
        .iter()
        .map(|&part| {
            let mut usage = PartUsage {
                part,
                processes: 0,
                ram_bytes: 0,
                cpu_percent: 0.0,
                gpu_percent: gpu.then_some(0.0),
                vram_bytes: gpu.then_some(0),
            };
            for (pid, _) in parts.iter().filter(|(_, p)| **p == part) {
                let Some(s) = stats.get(pid) else { continue };
                usage.processes += 1;
                usage.ram_bytes += s.ram_bytes;
                usage.cpu_percent += s.cpu_percent;
                if gpu {
                    usage.gpu_percent = Some(usage.gpu_percent.unwrap_or(0.0) + s.gpu_percent.unwrap_or(0.0));
                    usage.vram_bytes = Some(usage.vram_bytes.unwrap_or(0) + s.vram_bytes.unwrap_or(0));
                }
            }
            if let Some(g) = usage.gpu_percent.as_mut() {
                *g = g.min(100.0);
            }
            usage
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub parts: Vec<PartUsage>,
    pub total_ram_bytes: u64,
    pub cpu_threads: u32,
    /// Whether GPU numbers could be read at all.
    pub gpu_available: bool,
    /// Why not, in words for the page, when they couldn't.
    pub gpu_note: Option<String>,
    pub gpu_name: Option<String>,
    pub total_vram_bytes: Option<u64>,
}

struct Sampler {
    system: System,
    #[cfg(target_os = "linux")]
    nvidia: Option<nvidia::Gpu>,
}

impl Sampler {
    fn new() -> Result<Sampler> {
        let mut system = System::new().map_err(|e| AppError::Internal(format!("can't read processes: {e}")))?;
        system.refresh_memory();
        Ok(Sampler {
            system,
            #[cfg(target_os = "linux")]
            nvidia: nvidia::Gpu::open(),
        })
    }

    fn sample(&mut self) -> Usage {
        // CPU use is measured between two refreshes, so the very first sample reads 0. Without
        // tasks: Linux would list every thread as a process too, each with the whole RAM again.
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().with_memory().with_cpu().without_tasks(),
        );
        let procs: Vec<Proc> = self
            .system
            .processes()
            .values()
            .filter(|p| p.thread_kind().is_none())
            .map(|p| Proc { pid: p.pid().as_u32(), parent: p.parent().map(Pid::as_u32), name: p.name().to_str().unwrap_or("") })
            .collect();
        let parts = assign_parts(std::process::id(), &procs);
        drop(procs);

        let threads = std::thread::available_parallelism().map_or(1, |n| n.get()) as f32;
        let mut stats: HashMap<u32, ProcStat> = parts
            .keys()
            .filter_map(|&pid| {
                let p = self.system.process(Pid::from_u32(pid))?;
                let ram_bytes = own_memory(pid).unwrap_or_else(|| p.memory());
                Some((pid, ProcStat { ram_bytes, cpu_percent: p.cpu_usage() / threads, ..Default::default() }))
            })
            .collect();

        let gpu = self.read_gpu(&parts, &mut stats);
        Usage {
            parts: sum_parts(&parts, &stats, gpu.available),
            total_ram_bytes: self.system.total_memory(),
            cpu_threads: threads as u32,
            gpu_available: gpu.available,
            gpu_note: gpu.note,
            gpu_name: gpu.name,
            total_vram_bytes: gpu.total_vram_bytes,
        }
    }

    #[cfg(target_os = "linux")]
    fn read_gpu(&mut self, parts: &HashMap<u32, Part>, stats: &mut HashMap<u32, ProcStat>) -> GpuSummary {
        let Some(gpu) = self.nvidia.as_mut() else {
            return GpuSummary::none("No NVIDIA graphics card found, so GPU and VRAM can't be shown.");
        };
        let Some(reading) = gpu.read() else {
            return GpuSummary::none("The NVIDIA driver didn't answer, so GPU and VRAM can't be shown.");
        };
        for (pid, (percent, vram)) in reading.per_pid {
            if parts.contains_key(&pid) {
                if let Some(s) = stats.get_mut(&pid) {
                    s.gpu_percent = Some(percent);
                    s.vram_bytes = Some(vram);
                }
            }
        }
        GpuSummary { available: true, note: None, name: reading.name, total_vram_bytes: reading.total_vram_bytes }
    }

    #[cfg(windows)]
    fn read_gpu(&mut self, parts: &HashMap<u32, Part>, stats: &mut HashMap<u32, ProcStat>) -> GpuSummary {
        let pids: Vec<Pid> = parts.keys().map(|&p| Pid::from_u32(p)).collect();
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&pids),
            false,
            ProcessRefreshKind::nothing().with_gpu_usage().with_gpu_memory(),
        );
        let mut any = false;
        for pid in &pids {
            let (Some(p), Some(s)) = (self.system.process(*pid), stats.get_mut(&pid.as_u32())) else { continue };
            s.gpu_percent = p.gpu_usage();
            s.vram_bytes = p.gpu_memory();
            any |= s.gpu_percent.is_some() || s.vram_bytes.is_some();
        }
        if any {
            GpuSummary { available: true, note: None, name: None, total_vram_bytes: None }
        } else {
            GpuSummary::none("Windows isn't reporting GPU use yet.")
        }
    }

    #[cfg(not(any(target_os = "linux", windows)))]
    fn read_gpu(&mut self, _: &HashMap<u32, Part>, _: &mut HashMap<u32, ProcStat>) -> GpuSummary {
        GpuSummary::none("GPU use isn't available on this system.")
    }
}

/// Linux: the process's proportional set size. Memory shared with other processes (GTK, WebKit
/// and NVIDIA libraries) counts as a fair share instead of in full for each, which is what
/// plain RSS (`sysinfo`'s number) does. None elsewhere or when it can't be read.
fn own_memory(pid: u32) -> Option<u64> {
    #[cfg(target_os = "linux")]
    return std::fs::read_to_string(format!("/proc/{pid}/smaps_rollup")).ok().and_then(|s| pss_bytes(&s));
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        None
    }
}

/// The `Pss:` line of `/proc/<pid>/smaps_rollup`, in bytes.
#[allow(dead_code)] // Only used on Linux.
fn pss_bytes(smaps_rollup: &str) -> Option<u64> {
    let line = smaps_rollup.lines().find_map(|l| l.strip_prefix("Pss:"))?;
    let kb: u64 = line.trim().strip_suffix("kB")?.trim().parse().ok()?;
    Some(kb * 1024)
}

struct GpuSummary {
    available: bool,
    note: Option<String>,
    name: Option<String>,
    total_vram_bytes: Option<u64>,
}

impl GpuSummary {
    fn none(note: &str) -> GpuSummary {
        GpuSummary { available: false, note: Some(note.into()), name: None, total_vram_bytes: None }
    }
}

#[cfg(target_os = "linux")]
mod nvidia {
    use std::collections::HashMap;

    use nvml_wrapper::enums::device::UsedGpuMemory;
    use nvml_wrapper::Nvml;

    pub struct Gpu {
        nvml: Nvml,
        /// NVML's timestamp (µs) of the newest load sample already read, so each reading covers
        /// only the time since the one before.
        last_seen: Option<u64>,
    }

    pub struct Reading {
        pub name: Option<String>,
        pub total_vram_bytes: Option<u64>,
        /// Per process: load in % of the GPU (its busiest engine: 3D, encoder or decoder) and
        /// VRAM in bytes.
        pub per_pid: HashMap<u32, (f32, u64)>,
    }

    impl Gpu {
        /// None without the NVIDIA driver's `libnvidia-ml` or without an NVIDIA card.
        pub fn open() -> Option<Gpu> {
            let nvml = Nvml::init().ok()?;
            (nvml.device_count().ok()? > 0).then_some(Gpu { nvml, last_seen: None })
        }

        pub fn read(&mut self) -> Option<Reading> {
            let count = self.nvml.device_count().ok()?;
            let mut reading = Reading { name: None, total_vram_bytes: None, per_pid: HashMap::new() };
            let mut newest = self.last_seen;
            for index in 0..count {
                let Ok(device) = self.nvml.device_by_index(index) else { continue };
                if reading.name.is_none() {
                    reading.name = device.name().ok();
                }
                if let Ok(memory) = device.memory_info() {
                    *reading.total_vram_bytes.get_or_insert(0) += memory.total;
                }
                let processes = device.running_graphics_processes().unwrap_or_default().into_iter();
                // A process doing 3D and CUDA work is in both lists with the same memory.
                let mut vram: HashMap<u32, u64> = HashMap::new();
                for p in processes.chain(device.running_compute_processes().unwrap_or_default()) {
                    if let UsedGpuMemory::Used(bytes) = p.used_gpu_memory {
                        let e = vram.entry(p.pid).or_default();
                        *e = (*e).max(bytes);
                    }
                }
                for (pid, bytes) in vram {
                    reading.per_pid.entry(pid).or_default().1 += bytes;
                }
                // Load: average of the samples since the last reading. NVML errors with
                // "not found" when there are none, which just means idle.
                let samples = device.process_utilization_stats(self.last_seen).unwrap_or_default();
                let mut loads: HashMap<u32, (u32, u32)> = HashMap::new();
                for s in samples {
                    newest = Some(newest.map_or(s.timestamp, |n| n.max(s.timestamp)));
                    let e = loads.entry(s.pid).or_default();
                    e.0 += s.sm_util.max(s.enc_util).max(s.dec_util);
                    e.1 += 1;
                }
                for (pid, (sum, n)) in loads {
                    reading.per_pid.entry(pid).or_default().0 += sum as f32 / n as f32;
                }
            }
            self.last_seen = newest;
            Some(reading)
        }
    }
}

static SAMPLER: OnceLock<Mutex<Option<Sampler>>> = OnceLock::new();

/// What Klipt's processes use right now. Made for polling about once a second.
#[tauri::command]
pub async fn resource_usage() -> Result<Usage> {
    tauri::async_runtime::spawn_blocking(|| {
        let mut sampler = lock(SAMPLER.get_or_init(|| Mutex::new(None)));
        if sampler.is_none() {
            *sampler = Some(Sampler::new()?);
        }
        Ok(sampler.as_mut().expect("just set").sample())
    })
    .await
    .map_err(|e| AppError::Internal(format!("resource sampling failed: {e}")))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(pid: u32, parent: u32, name: &str) -> Proc<'_> {
        Proc { pid, parent: Some(parent), name }
    }

    #[test]
    fn reads_pss_from_smaps_rollup() {
        let rollup = "55d0-7ffd ---p 00000000 00:00 0  [rollup]\nRss:              279196 kB\nPss:              154694 kB\nPss_Dirty:         87618 kB\n";
        assert_eq!(pss_bytes(rollup), Some(154694 * 1024));
        assert_eq!(pss_bytes("Rss: 1 kB\n"), None);
    }

    #[test]
    fn names_say_the_part() {
        assert_eq!(part_by_name("WebKitWebProces"), Some(Part::Window));
        assert_eq!(part_by_name("msedgewebview2.exe"), Some(Part::Window));
        assert_eq!(part_by_name("gpu-screen-reco"), Some(Part::Recorder));
        assert_eq!(part_by_name("gsr-kms-server"), Some(Part::Recorder));
        assert_eq!(part_by_name("klipt-recorder.exe"), Some(Part::Recorder));
        assert_eq!(part_by_name("ffmpeg"), Some(Part::VideoJobs));
        assert_eq!(part_by_name("FFPROBE.EXE"), Some(Part::VideoJobs));
        assert_eq!(part_by_name("obs-ffmpeg-mux.exe"), None);
        assert_eq!(part_by_name("klipt-desktop"), None);
    }

    #[test]
    fn processes_under_the_app_get_their_part() {
        let procs = [
            proc(1, 0, "systemd"),
            proc(10, 1, "klipt-desktop"),
            proc(11, 10, "bwrap"),
            proc(12, 11, "WebKitWebProces"),
            proc(13, 10, "klipt-recorder.exe"),
            proc(14, 13, "obs-ffmpeg-mux.exe"),
            proc(15, 10, "ffmpeg"),
            proc(16, 10, "sh"),
            proc(20, 1, "ffmpeg"), // someone else's
        ];
        let parts = assign_parts(10, &procs);
        assert_eq!(parts.len(), 7);
        assert_eq!(parts[&10], Part::App);
        assert_eq!(parts[&11], Part::Window);
        assert_eq!(parts[&12], Part::Window);
        assert_eq!(parts[&13], Part::Recorder);
        assert_eq!(parts[&14], Part::Recorder);
        assert_eq!(parts[&15], Part::VideoJobs);
        assert_eq!(parts[&16], Part::App);
        assert!(!parts.contains_key(&20));
    }

    #[test]
    fn a_loop_in_the_tree_ends() {
        let procs = [proc(10, 0, "app"), proc(11, 12, "a"), proc(12, 11, "b"), proc(13, 13, "c")];
        assert_eq!(assign_parts(10, &procs).len(), 1);
    }

    #[test]
    fn sums_by_part_and_lists_every_part() {
        let parts = HashMap::from([(10, Part::App), (11, Part::Window), (12, Part::Window)]);
        let stats = HashMap::from([
            (10, ProcStat { ram_bytes: 100, cpu_percent: 1.0, gpu_percent: None, vram_bytes: None }),
            (11, ProcStat { ram_bytes: 200, cpu_percent: 2.0, gpu_percent: Some(70.0), vram_bytes: Some(5) }),
            (12, ProcStat { ram_bytes: 300, cpu_percent: 3.0, gpu_percent: Some(60.0), vram_bytes: Some(7) }),
        ]);
        let usage = sum_parts(&parts, &stats, true);
        assert_eq!(usage.iter().map(|u| u.part).collect::<Vec<_>>(), PARTS);
        assert_eq!(usage[0], PartUsage { part: Part::App, processes: 1, ram_bytes: 100, cpu_percent: 1.0, gpu_percent: Some(0.0), vram_bytes: Some(0) });
        assert_eq!(usage[1].processes, 2);
        assert_eq!(usage[1].ram_bytes, 500);
        assert_eq!(usage[1].gpu_percent, Some(100.0)); // capped
        assert_eq!(usage[1].vram_bytes, Some(12));
        assert_eq!(usage[2].processes, 0);

        let no_gpu = sum_parts(&parts, &stats, false);
        assert!(no_gpu.iter().all(|u| u.gpu_percent.is_none() && u.vram_bytes.is_none()));
    }
}
