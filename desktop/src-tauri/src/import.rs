//! Importing clips that are already on disk (other recorders' folders: ShadowPlay, OBS, Medal…).
//!
//! The user picks a folder; every video and picture under it joins the library. A file the app
//! can play and upload as it is (H.264 + AAC in an MP4) is imported in place: the library points
//! at it and nothing is copied. Anything else gets an H.264/AAC copy in `<clips folder>/Imported`
//! (a quick remux when only the container or audio differs), because the editor's webview can't
//! reliably play H.265/AV1 and the server only takes H.264. Originals are never changed, imported
//! clips are never removed by the automatic cleanup, and a file is never imported twice.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::UNIX_EPOCH;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::errors::{AppError, Result};
use crate::ffmpeg::{MediaInfo, Tools};
use crate::library::{ClipKind, ClipView, Library, NewClip};
use crate::state::AppState;

pub mod events {
    pub const CHECK_PROGRESS: &str = "import-check-progress";
    pub const PROGRESS: &str = "import-progress";
    pub const CLIP_IMPORTED: &str = "clip-imported";
    pub const FINISHED: &str = "import-finished";
}

const VIDEO_EXTS: &[&str] = &["mp4", "m4v", "mov", "mkv", "webm", "avi", "flv", "ts", "wmv"];
const PICTURE_EXTS: &[&str] = &["png", "jpg", "jpeg", "webp"];
/// Played and uploaded as they are (when the codecs are right too).
const IN_PLACE_VIDEO_EXTS: &[&str] = &["mp4", "m4v"];
/// Folder depth searched under the picked folder (recorders nest at most a couple of levels).
const MAX_DEPTH: usize = 8;
/// A bound on one import; a folder with more is most likely the wrong folder.
const MAX_FILES: usize = 5_000;
const MAX_TITLE_CHARS: usize = 100;
/// Folder names that say nothing about the game, so a clip in one gets no game from it.
const GENERIC_DIRS: &[&str] =
    &["clips", "replays", "replay", "videos", "video", "captures", "recordings", "highlights", "screenshots", "imported", "klipt"];

static RUNNING: AtomicBool = AtomicBool::new(false);
static CANCEL: AtomicBool = AtomicBool::new(false);

/// What happens to one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    /// Not importable; the reason is shown to the user.
    Skip(&'static str),
    /// The library points at the file itself.
    InPlace,
    /// An H.264/AAC MP4 copy is made.
    Convert { reencode_video: bool, audio: AudioPlan },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioPlan {
    Copy,
    Encode,
    /// No audio at all: a silent track, since editing and uploading mix track 0.
    AddSilent,
}

fn ext_of(path: &Path) -> String {
    path.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase()
}

/// Decides how to import a file from its extension and what ffprobe saw in it.
pub fn plan(info: &MediaInfo, ext: &str) -> Plan {
    if info.is_image() {
        return if PICTURE_EXTS.contains(&ext) { Plan::InPlace } else { Plan::Skip("not a picture Klipt can show") };
    }
    let Some(video) = info.video_codec.as_deref() else { return Plan::Skip("no video in it") };
    if info.duration_ms == 0 || info.width == 0 {
        return Plan::Skip("empty or damaged video");
    }
    // 10-bit or 4:4:4 H.264 doesn't play in browsers either.
    let playable = video == "h264" && matches!(info.pix_fmt.as_deref(), Some("yuv420p" | "yuvj420p"));
    let audio = if info.audio_codecs.is_empty() {
        AudioPlan::AddSilent
    } else if info.audio_codecs.iter().all(|c| c == "aac") {
        AudioPlan::Copy
    } else {
        AudioPlan::Encode
    };
    if playable && audio == AudioPlan::Copy && IN_PLACE_VIDEO_EXTS.contains(&ext) {
        Plan::InPlace
    } else {
        Plan::Convert { reencode_video: !playable, audio }
    }
}

/// An H.264 encoder a re-encode can use.
#[derive(Debug, Clone, PartialEq)]
pub struct Encoder {
    pub args: Vec<&'static str>,
    /// For the progress line: "NVIDIA GPU", "CPU"…
    pub label: &'static str,
    pub gpu: bool,
}

/// H.264 encoders to try for a re-encode, best first: the GPU's when there is one, else x264.
/// Every one keeps a keyframe each second, like Klipt's own clips, so trimming and seeking stay exact.
pub fn video_encoders() -> Vec<Encoder> {
    // NVENC only honours forced keyframes as IDR frames with `-forced-idr`.
    let mut list = vec![Encoder {
        args: vec!["-c:v", "h264_nvenc", "-preset", "p5", "-rc", "vbr", "-cq", "21", "-b:v", "0", "-forced-idr", "1"],
        label: "NVIDIA GPU",
        gpu: true,
    }];
    if cfg!(windows) {
        list.push(Encoder {
            args: vec!["-c:v", "h264_amf", "-quality", "quality", "-rc", "cqp", "-qp_i", "20", "-qp_p", "22"],
            label: "AMD GPU",
            gpu: true,
        });
        list.push(Encoder { args: vec!["-c:v", "h264_qsv", "-global_quality", "21"], label: "Intel GPU", gpu: true });
    }
    list.push(Encoder { args: vec!["-c:v", "libx264", "-preset", "veryfast", "-crf", "20"], label: "CPU", gpu: false });
    list
}

/// The best encoder that works on this PC (tested once per run of the app).
pub fn detect_encoder(tools: &Tools) -> Option<Encoder> {
    static DETECTED: OnceLock<Option<Encoder>> = OnceLock::new();
    DETECTED.get_or_init(|| video_encoders().into_iter().find(|e| crate::ffmpeg::can_encode(tools, &e.args))).clone()
}

/// ffmpeg arguments (without the output path) for making the H.264/AAC copy of `input`.
pub fn convert_args(input: &Path, reencode_video: bool, audio: AudioPlan, encoder: &[&str]) -> Vec<String> {
    let mut args: Vec<String> = ["-nostdin", "-hide_banner", "-loglevel", "error", "-y", "-i"].map(String::from).into();
    args.push(input.to_string_lossy().into_owned());
    if audio == AudioPlan::AddSilent {
        args.extend(["-f", "lavfi", "-i", "anullsrc=r=48000:cl=stereo"].map(String::from));
    }
    args.extend(["-map", "0:v:0"].map(String::from));
    args.extend(match audio {
        AudioPlan::AddSilent => ["-map", "1:a:0", "-shortest"].map(String::from).to_vec(),
        _ => ["-map", "0:a"].map(String::from).to_vec(),
    });
    if reencode_video {
        args.extend(encoder.iter().map(|s| s.to_string()));
        args.extend(["-pix_fmt", "yuv420p", "-force_key_frames", "expr:gte(t,n_forced*1)"].map(String::from));
    } else {
        args.extend(["-c:v", "copy"].map(String::from));
    }
    args.extend(match audio {
        AudioPlan::Copy => ["-c:a", "copy"].map(String::from).to_vec(),
        _ => ["-c:a", "aac", "-b:a", "160k"].map(String::from).to_vec(),
    });
    args.extend(["-movflags", "+faststart", "-f", "mp4"].map(String::from));
    args
}

/// The game a clip is from, by the folder it's in (ShadowPlay and Medal keep a folder per game).
/// None for files right in the picked folder or in a generic folder ("Replays", "Clips"…).
pub fn guess_game(file: &Path, root: &Path) -> Option<String> {
    let parent = file.parent()?;
    if parent == root {
        return None;
    }
    let name = parent.file_name()?.to_str()?.trim();
    if name.is_empty() || GENERIC_DIRS.contains(&name.to_ascii_lowercase().as_str()) {
        return None;
    }
    Some(name.chars().take(MAX_TITLE_CHARS).collect())
}

/// A title from the file name: `Counter-strike 2 2026.11.10 - 22.55.49.02.DVR.mp4` →
/// `Counter-strike 2 2026.11.10 - 22.55.49.02.DVR`, underscores as spaces.
pub fn title_from(file: &Path) -> String {
    let stem = file.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
    let title: String = stem.replace('_', " ").split_whitespace().collect::<Vec<_>>().join(" ");
    let title: String = title.chars().take(MAX_TITLE_CHARS).collect();
    if title.is_empty() {
        "Imported clip".into()
    } else {
        title
    }
}

/// Videos and pictures under `root` (hidden folders and symlinked folders skipped), oldest first.
pub fn scan(root: &Path) -> Vec<PathBuf> {
    let mut found: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with('.') {
                continue;
            }
            let Ok(kind) = entry.file_type() else { continue };
            let path = entry.path();
            if kind.is_dir() {
                if depth + 1 < MAX_DEPTH {
                    stack.push((path, depth + 1));
                }
            } else if kind.is_file() {
                let ext = ext_of(&path);
                if VIDEO_EXTS.contains(&ext.as_str()) || PICTURE_EXTS.contains(&ext.as_str()) {
                    let modified = entry.metadata().and_then(|m| m.modified()).unwrap_or(UNIX_EPOCH);
                    found.push((modified, path));
                    if found.len() >= MAX_FILES {
                        stack.clear();
                        break;
                    }
                }
            }
        }
    }
    found.sort();
    found.into_iter().map(|(_, p)| p).collect()
}

/// `name` in `dir`, or `stem_2.ext`, `stem_3.ext`… when taken.
fn free_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let first = dir.join(format!("{stem}.{ext}"));
    if !first.exists() {
        return first;
    }
    (2..).map(|n| dir.join(format!("{stem}_{n}.{ext}"))).find(|p| !p.exists()).expect("some free name")
}

// ---------------------------------------------------------------------------------------------
// Checking a folder first: what each file needs, and how long it all will take.

/// A file found in the folder and what importing it takes.
#[derive(Debug, Clone, PartialEq)]
pub struct Planned {
    /// The real path (symlinks resolved), as stored in the library.
    pub path: PathBuf,
    pub info: Option<MediaInfo>,
    pub plan: Plan,
    pub bytes: u64,
    /// Already in the library.
    pub already: bool,
}

/// The kind of work a file needs, for counting and estimating.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Work {
    /// Imported as it is: just a thumbnail.
    Instant,
    /// A new container (and maybe audio): about as fast as the disk.
    Remux,
    /// The video is encoded again: the slow part.
    Reencode,
    /// Nothing to do (already there, or can't be imported).
    None,
}

impl Planned {
    pub fn work(&self) -> Work {
        if self.already {
            return Work::None;
        }
        match self.plan {
            Plan::Skip(_) => Work::None,
            Plan::InPlace => Work::Instant,
            Plan::Convert { reencode_video: false, .. } => Work::Remux,
            Plan::Convert { reencode_video: true, .. } => Work::Reencode,
        }
    }
}

/// How hard a video is to encode, in seconds of 1080p60: a 4K30 minute counts as two minutes.
pub fn load_secs(info: &MediaInfo) -> f64 {
    let fps = if info.fps > 0.0 { info.fps.min(240.0) } else { 30.0 };
    let pixels = f64::from(info.width.max(1)) * f64::from(info.height.max(1)) * fps;
    info.duration_ms as f64 / 1000.0 * pixels / (1920.0 * 1080.0 * 60.0)
}

/// Rates the estimate uses: starting guesses, replaced by what this import measures as it goes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rates {
    /// Seconds for a file imported as it is (probing done, the thumbnail left).
    pub instant_secs: f64,
    pub remux_bytes_per_sec: f64,
    /// Seconds of 1080p60 video encoded per second.
    pub reencode_speed: f64,
}

impl Rates {
    pub fn guess(gpu: bool) -> Rates {
        Rates { instant_secs: 0.3, remux_bytes_per_sec: 150e6, reencode_speed: if gpu { 4.0 } else { 1.0 } }
    }
}

/// Seconds `file` is expected to take at `rates`.
pub fn file_secs(file: &Planned, rates: &Rates) -> f64 {
    let load = file.info.as_ref().map_or(0.0, load_secs);
    match file.work() {
        Work::Instant => rates.instant_secs,
        Work::Remux => rates.instant_secs + file.bytes as f64 / rates.remux_bytes_per_sec,
        Work::Reencode => rates.instant_secs + load / rates.reencode_speed,
        Work::None => 0.0,
    }
}

/// Learns the real rates while importing. Each kind starts from the guess, weighted like a few
/// seconds of work, so one odd file can't swing the estimate wildly.
#[derive(Debug, Clone)]
pub struct Meter {
    guess: Rates,
    instant: (f64, f64),
    remux: (f64, f64),
    reencode: (f64, f64),
}

const PRIOR_SECS: f64 = 5.0;

impl Meter {
    pub fn new(guess: Rates) -> Meter {
        Meter { guess, instant: (0.0, 0.0), remux: (0.0, 0.0), reencode: (0.0, 0.0) }
    }

    /// `file` took `secs`.
    pub fn record(&mut self, file: &Planned, secs: f64) {
        let secs = secs.max(0.001);
        match file.work() {
            Work::Instant => {
                self.instant.0 += 1.0;
                self.instant.1 += secs;
            }
            Work::Remux => {
                self.remux.0 += file.bytes as f64;
                self.remux.1 += secs;
            }
            Work::Reencode => {
                self.reencode.0 += file.info.as_ref().map_or(0.0, load_secs);
                self.reencode.1 += secs;
            }
            Work::None => {}
        }
    }

    pub fn rates(&self) -> Rates {
        let blend = |(units, secs): (f64, f64), guess_rate: f64| (units + guess_rate * PRIOR_SECS) / (secs + PRIOR_SECS);
        // Instant files: a time per file, so blend the per-file seconds instead.
        let instant = if self.instant.0 > 0.0 {
            (self.instant.1 + self.guess.instant_secs * 3.0) / (self.instant.0 + 3.0)
        } else {
            self.guess.instant_secs
        };
        Rates {
            instant_secs: instant,
            remux_bytes_per_sec: blend(self.remux, self.guess.remux_bytes_per_sec),
            reencode_speed: blend(self.reencode, self.guess.reencode_speed),
        }
    }
}

/// What checking a folder found, for the confirmation dialog.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub folder: String,
    pub files: usize,
    pub in_place: usize,
    pub remux: usize,
    pub reencode: usize,
    pub already_there: usize,
    pub unsupported: usize,
    /// Length of the videos that get encoded again.
    pub reencode_video_ms: u64,
    /// Roughly how much disk the H.264 copies take.
    pub copy_bytes: u64,
    /// Where those copies go.
    pub copies_dir: String,
    pub estimate_secs: f64,
    /// The encoder re-encodes use ("NVIDIA GPU", "CPU"); None when nothing needs one.
    pub encoder: Option<String>,
    /// No encoder works at all: those files will be skipped.
    pub no_encoder: bool,
}

pub fn preview_of(folder: &Path, copies_dir: &Path, files: &[Planned], encoder: Option<&Encoder>) -> Preview {
    let rates = Rates::guess(encoder.is_some_and(|e| e.gpu));
    let mut p = Preview {
        folder: folder.to_string_lossy().into_owned(),
        copies_dir: copies_dir.to_string_lossy().into_owned(),
        files: files.len(),
        ..Preview::default()
    };
    for f in files {
        match f.work() {
            Work::Instant => p.in_place += 1,
            Work::Remux => {
                p.remux += 1;
                p.copy_bytes += f.bytes;
            }
            Work::Reencode => {
                p.reencode += 1;
                p.reencode_video_ms += f.info.as_ref().map_or(0, |i| i.duration_ms);
                // H.265/AV1 sources are smaller than their H.264 copies.
                let grows = f.info.as_ref().and_then(|i| i.video_codec.as_deref()).is_some_and(|c| c == "hevc" || c == "av1");
                p.copy_bytes += if grows { f.bytes * 3 / 2 } else { f.bytes };
            }
            Work::None if f.already => p.already_there += 1,
            Work::None => p.unsupported += 1,
        }
        p.estimate_secs += file_secs(f, &rates);
    }
    if p.reencode > 0 {
        p.encoder = encoder.map(|e| e.label.to_string());
        p.no_encoder = encoder.is_none();
    }
    p
}

/// Looks at every file under `root`: what it is and what importing it takes. `checked` hears
/// (checked, total) as it goes; returns None when `stop` is set.
pub fn check_folder(
    tools: &Tools,
    library: &Mutex<Library>,
    root: &Path,
    stop: &AtomicBool,
    mut checked: impl FnMut(usize, usize),
) -> Option<Vec<Planned>> {
    let found = scan(root);
    let total = found.len();
    let mut files = Vec::with_capacity(total);
    for (i, file) in found.into_iter().enumerate() {
        if stop.load(Ordering::SeqCst) {
            return None;
        }
        checked(i, total);
        let path = dunce::canonicalize(&file).unwrap_or(file);
        let bytes = std::fs::metadata(&path).map_or(0, |m| m.len());
        let already = crate::commands::lock(library).contains_file(&path).unwrap_or(false);
        let (info, plan) = if already {
            (None, Plan::Skip("already in Klipt"))
        } else {
            match crate::ffmpeg::inspect(tools, &path) {
                Ok(info) => {
                    let plan = plan(&info, &ext_of(&path));
                    (Some(info), plan)
                }
                Err(_) => (None, Plan::Skip("not a video or picture ffmpeg can read")),
            }
        };
        files.push(Planned { path, info, plan, bytes, already });
    }
    checked(total, total);
    Some(files)
}

// ---------------------------------------------------------------------------------------------
// Importing.

/// How one file went.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Imported { view: Box<ClipView>, converted: bool },
    AlreadyThere,
    Skipped(String),
    /// The user stopped the import during this file; nothing of it was kept.
    Stopped,
}

/// Imports one checked file. `copies_dir` is where H.264 copies go; `progress` hears 0..=1 while
/// a copy is written; `stop` interrupts a copy.
pub fn import_planned(
    tools: &Tools,
    library: &Mutex<Library>,
    file: &Planned,
    root: &Path,
    copies_dir: &Path,
    encoder: Option<&Encoder>,
    stop: &AtomicBool,
    mut progress: impl FnMut(f64),
) -> Result<Outcome> {
    if file.already || crate::commands::lock(library).contains_file(&file.path)? {
        return Ok(Outcome::AlreadyThere);
    }
    let (Some(info), plan) = (file.info.as_ref(), &file.plan) else {
        let reason = if let Plan::Skip(r) = file.plan { r } else { "can't be read" };
        return Ok(Outcome::Skipped(reason.into()));
    };
    let source = &file.path;
    let (path, converted) = match *plan {
        Plan::Skip(reason) => return Ok(Outcome::Skipped(reason.into())),
        Plan::InPlace => (source.clone(), false),
        Plan::Convert { reencode_video, audio } => {
            let encoder_args: &[&str] = if reencode_video {
                match encoder {
                    Some(e) => &e.args,
                    None => return Ok(Outcome::Skipped("no H.264 encoder works on this PC".into())),
                }
            } else {
                &[]
            };
            std::fs::create_dir_all(copies_dir)?;
            let stem = source.file_stem().and_then(|s| s.to_str()).unwrap_or("clip");
            let out = free_path(copies_dir, stem, "mp4");
            let args = convert_args(source, reencode_video, audio, encoder_args);
            if let Err(e) = crate::ffmpeg::write_mp4_progress(tools, &args, &out, info.duration_ms, stop, &mut progress) {
                if stop.load(Ordering::SeqCst) {
                    return Ok(Outcome::Stopped);
                }
                return Ok(Outcome::Skipped(format!("couldn't convert it: {e}")));
            }
            (out, true)
        }
    };

    let created_at = std::fs::metadata(source)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_millis() as i64);
    let kind = if info.is_image() { ClipKind::Screenshot } else { ClipKind::Clip };
    let folder_game = guess_game(source, root);
    let (clip, thumb) = {
        let library = crate::commands::lock(library);
        // A game Klipt already knows (any capitalization) keeps its name and Steam art.
        let known = folder_game.as_deref().and_then(|g| {
            library.games().ok()?.into_iter().find(|k| k.name.eq_ignore_ascii_case(g))
        });
        let new = NewClip {
            path: path.clone(),
            created_at,
            duration_ms: if kind == ClipKind::Screenshot { 0 } else { info.duration_ms },
            width: info.width,
            height: info.height,
            has_mic: false,
            game_name: known.as_ref().map(|k| k.name.clone()).or(folder_game),
            steam_app_id: known.and_then(|k| k.steam_app_id),
            exe: None,
            title: title_from(source),
            has_voice: false,
            kind,
        };
        let clip = match library.insert(new) {
            Ok(clip) => clip,
            Err(e) => {
                if converted {
                    let _ = std::fs::remove_file(&path);
                }
                return Err(e);
            }
        };
        library.set_source(&clip.id, source)?;
        let thumb = library.thumb_path(&clip.id);
        (library.get(&clip.id)?, thumb)
    };
    let at_ms = if kind == ClipKind::Screenshot { 0 } else { (clip.duration_ms / 2).min(1_000) };
    if let Err(e) = crate::ffmpeg::thumbnail(tools, &clip.path, &thumb, at_ms) {
        eprintln!("klipt: can't make a thumbnail for {}: {e}", clip.path.display());
    }
    let view = ClipView { file_missing: false, thumb_path: thumb.is_file().then_some(thumb), clip };
    Ok(Outcome::Imported { view: Box::new(view), converted })
}

/// Checks and imports one file (tests, and anything that imports a single file).
pub fn import_one(tools: &Tools, library: &Mutex<Library>, file: &Path, root: &Path, copies_dir: &Path) -> Result<Outcome> {
    let path = dunce::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
    let already = crate::commands::lock(library).contains_file(&path)?;
    let info = if already { None } else { crate::ffmpeg::inspect(tools, &path).ok() };
    let plan = match &info {
        Some(info) => plan(info, &ext_of(&path)),
        None if already => Plan::Skip("already in Klipt"),
        None => Plan::Skip("not a video or picture ffmpeg can read"),
    };
    let bytes = std::fs::metadata(&path).map_or(0, |m| m.len());
    let planned = Planned { path, info, plan, bytes, already };
    let encoder = detect_encoder(tools);
    import_planned(tools, library, &planned, root, copies_dir, encoder.as_ref(), &AtomicBool::new(false), |_| {})
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    /// Files finished.
    pub done: usize,
    pub total: usize,
    /// The file being worked on, relative to the picked folder.
    pub current: Option<String>,
    /// "importing", "remuxing" or "converting".
    pub action: Option<&'static str>,
    /// 0..=1 through the current file's copy.
    pub file_fraction: Option<f64>,
    /// How many times real time the current conversion runs.
    pub speed: Option<f64>,
    pub encoder: Option<String>,
    pub imported: usize,
    pub converted: usize,
    pub skipped: usize,
    pub already_there: usize,
    pub elapsed_secs: f64,
    /// Seconds left, by the measured rates.
    pub eta_secs: f64,
    /// 0..=1 of the whole import, by time.
    pub fraction: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedFile {
    pub file: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub imported: usize,
    /// Of `imported`, how many got an H.264 copy.
    pub converted: usize,
    pub already_there: usize,
    pub skipped: Vec<SkippedFile>,
    pub cancelled: bool,
    pub elapsed_secs: f64,
}

/// A checked folder, waiting for the user to confirm.
struct Checked {
    root: PathBuf,
    files: Vec<Planned>,
    encoder: Option<Encoder>,
}

static CHECKED: Mutex<Option<Checked>> = Mutex::new(None);

/// Clears the running flag when dropped, so a panic can't leave imports blocked for good.
struct RunningGuard;

impl Drop for RunningGuard {
    fn drop(&mut self) {
        RUNNING.store(false, Ordering::SeqCst);
    }
}

fn copies_dir(app: &AppHandle) -> PathBuf {
    let state = app.state::<AppState>();
    let dir = crate::commands::lock(&state.settings).output_dir.join("Imported");
    dir
}

/// Checks every file under `folder` (`import-check-progress` as it goes) and returns what an
/// import would do, for the user to confirm. None when stopped, or when an import is running.
#[tauri::command]
pub async fn check_import(app: AppHandle, folder: String) -> Result<Option<Preview>> {
    let root = PathBuf::from(&folder);
    if !root.is_dir() {
        return Err(AppError::InvalidInput("not a folder".into()));
    }
    let tools = app.state::<AppState>().tools.clone().ok_or(AppError::ToolsMissing)?;
    if RUNNING.swap(true, Ordering::SeqCst) {
        return Ok(None);
    }
    CANCEL.store(false, Ordering::SeqCst);
    let root = dunce::canonicalize(&root).unwrap_or(root);
    let copies = copies_dir(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let _running = RunningGuard;
        let state = app.state::<AppState>();
        let mut last = std::time::Instant::now();
        let files = check_folder(&tools, &state.library, &root, &CANCEL, |checked, total| {
            if checked == 0 || checked == total || last.elapsed().as_millis() >= 150 {
                last = std::time::Instant::now();
                let _ = app.emit(events::CHECK_PROGRESS, CheckProgress { checked, total });
            }
        })?;
        let needs_encoder = files.iter().any(|f| f.work() == Work::Reencode);
        let encoder = if needs_encoder { detect_encoder(&tools) } else { None };
        let preview = preview_of(&root, &copies, &files, encoder.as_ref());
        *crate::commands::lock(&CHECKED) = Some(Checked { root, files, encoder });
        Some(preview)
    })
    .await
    .map_err(|e| AppError::Internal(format!("checking the folder failed: {e}")))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckProgress {
    pub checked: usize,
    pub total: usize,
}

/// Imports the folder `check_import` just looked at, in the background (`import-progress`,
/// `clip-imported`, `import-finished`). False when an import is already running or the folder
/// wasn't checked.
#[tauri::command]
pub fn import_folder(app: AppHandle, folder: String) -> Result<bool> {
    let tools = app.state::<AppState>().tools.clone().ok_or(AppError::ToolsMissing)?;
    let root = PathBuf::from(&folder);
    let root = dunce::canonicalize(&root).unwrap_or(root);
    let Some(checked) = crate::commands::lock(&CHECKED).take().filter(|c| c.root == root) else {
        return Ok(false);
    };
    if RUNNING.swap(true, Ordering::SeqCst) {
        return Ok(false);
    }
    CANCEL.store(false, Ordering::SeqCst);
    let copies = copies_dir(&app);
    let spawned = std::thread::Builder::new().name("klipt-import".into()).spawn(move || {
        let _running = RunningGuard;
        let summary = run(&app, &tools, &checked, &copies);
        let _ = app.emit(events::FINISHED, summary);
    });
    if let Err(e) = spawned {
        RUNNING.store(false, Ordering::SeqCst);
        return Err(AppError::Internal(format!("can't start the import: {e}")));
    }
    Ok(true)
}

/// Stops checking, or stops the import (the file being converted is dropped).
#[tauri::command]
pub fn cancel_import() {
    CANCEL.store(true, Ordering::SeqCst);
}

/// Seconds left: the current file's remainder plus every file after it, at `rates`.
pub fn eta_secs(files: &[Planned], current: usize, current_fraction: f64, rates: &Rates) -> f64 {
    let Some(now) = files.get(current) else { return 0.0 };
    let rest: f64 = files[current + 1..].iter().map(|f| file_secs(f, rates)).sum();
    file_secs(now, rates) * (1.0 - current_fraction.clamp(0.0, 1.0)) + rest
}

fn run(app: &AppHandle, tools: &Tools, checked: &Checked, copies_dir: &Path) -> Summary {
    let state = app.state::<AppState>();
    let started = std::time::Instant::now();
    let files = &checked.files;
    let encoder = checked.encoder.as_ref();
    let mut meter = Meter::new(Rates::guess(encoder.is_some_and(|e| e.gpu)));
    let mut summary = Summary::default();
    let mut p = Progress { total: files.len(), encoder: encoder.map(|e| e.label.to_string()), ..Progress::default() };
    let mut last_emit = std::time::Instant::now() - std::time::Duration::from_secs(1);

    for (i, file) in files.iter().enumerate() {
        if CANCEL.load(Ordering::SeqCst) {
            summary.cancelled = true;
            break;
        }
        let shown = file.path.strip_prefix(&checked.root).unwrap_or(&file.path).to_string_lossy().into_owned();
        let work = file.work();
        p.current = Some(shown.clone());
        p.action = Some(match work {
            Work::Remux => "remuxing",
            Work::Reencode => "converting",
            _ => "importing",
        });
        p.file_fraction = matches!(work, Work::Remux | Work::Reencode).then_some(0.0);
        p.speed = None;
        let file_started = std::time::Instant::now();
        let duration_s = file.info.as_ref().map_or(0.0, |info| info.duration_ms as f64 / 1000.0);

        let mut emit = |p: &mut Progress, fraction: f64, force: bool| {
            if !force && last_emit.elapsed().as_millis() < 250 {
                return;
            }
            last_emit = std::time::Instant::now();
            p.elapsed_secs = started.elapsed().as_secs_f64();
            p.eta_secs = eta_secs(files, i, fraction, &meter.rates());
            p.fraction = (p.elapsed_secs / (p.elapsed_secs + p.eta_secs)).clamp(0.0, 1.0);
            let _ = app.emit(events::PROGRESS, p.clone());
        };
        emit(&mut p, 0.0, work != Work::Instant && work != Work::None);

        let outcome = import_planned(tools, &state.library, file, &checked.root, copies_dir, encoder, &CANCEL, |fraction| {
            p.file_fraction = Some(fraction);
            let secs = file_started.elapsed().as_secs_f64();
            if work == Work::Reencode && secs > 0.5 && duration_s > 0.0 {
                p.speed = Some(fraction * duration_s / secs);
            }
            emit(&mut p, fraction, false);
        });
        meter.record(file, file_started.elapsed().as_secs_f64());
        match outcome {
            Ok(Outcome::Imported { view, converted }) => {
                summary.imported += 1;
                summary.converted += usize::from(converted);
                let _ = app.emit(events::CLIP_IMPORTED, *view);
            }
            Ok(Outcome::AlreadyThere) => summary.already_there += 1,
            Ok(Outcome::Skipped(reason)) => summary.skipped.push(SkippedFile { file: shown, reason }),
            Ok(Outcome::Stopped) => {
                summary.cancelled = true;
                break;
            }
            Err(e) => summary.skipped.push(SkippedFile { file: shown, reason: e.to_string() }),
        }
        p.done = i + 1;
        p.imported = summary.imported;
        p.converted = summary.converted;
        p.skipped = summary.skipped.len();
        p.already_there = summary.already_there;
    }
    summary.elapsed_secs = started.elapsed().as_secs_f64();
    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffmpeg::tests::tools;
    use std::fs;
    use std::process::Command;

    fn info(video: Option<&str>, pix: &str, audio: &[&str]) -> MediaInfo {
        MediaInfo {
            format_name: "mov,mp4,m4a,3gp,3g2,mj2".into(),
            video_codec: video.map(String::from),
            pix_fmt: Some(pix.into()),
            audio_codecs: audio.iter().map(|s| s.to_string()).collect(),
            duration_ms: 5_000,
            width: 1920,
            height: 1080,
            fps: 60.0,
        }
    }

    fn planned(info: MediaInfo, ext: &str, bytes: u64) -> Planned {
        let plan = plan(&info, ext);
        Planned { path: PathBuf::from(format!("/v/x.{ext}")), info: Some(info), plan, bytes, already: false }
    }

    #[test]
    fn load_counts_resolution_and_frame_rate() {
        let base = info(Some("hevc"), "yuv420p", &["aac"]);
        assert!((load_secs(&base) - 5.0).abs() < 1e-9, "5 s of 1080p60");
        let uhd30 = MediaInfo { width: 3840, height: 2160, fps: 30.0, ..base.clone() };
        assert!((load_secs(&uhd30) - 10.0).abs() < 1e-9, "4K30 is twice the work");
        let unknown_fps = MediaInfo { fps: 0.0, ..base };
        assert!((load_secs(&unknown_fps) - 2.5).abs() < 1e-9, "assumed 30 fps");
    }

    #[test]
    fn preview_counts_kinds_space_and_time() {
        let files = vec![
            planned(info(Some("h264"), "yuv420p", &["aac"]), "mp4", 100),
            planned(info(Some("h264"), "yuv420p", &["aac"]), "mkv", 1_000_000_000),
            planned(info(Some("hevc"), "yuv420p", &["aac"]), "mp4", 200),
            Planned { already: true, ..planned(info(Some("h264"), "yuv420p", &["aac"]), "mp4", 5) },
            planned(info(None, "", &[]), "mp4", 5),
        ];
        let gpu = Encoder { args: vec![], label: "NVIDIA GPU", gpu: true };
        let p = preview_of(Path::new("/v"), Path::new("/c/Imported"), &files, Some(&gpu));
        assert_eq!((p.files, p.in_place, p.remux, p.reencode, p.already_there, p.unsupported), (5, 1, 1, 1, 1, 1));
        assert_eq!(p.reencode_video_ms, 5_000);
        assert_eq!(p.copy_bytes, 1_000_000_000 + 300, "HEVC copies grow by half");
        assert_eq!(p.encoder.as_deref(), Some("NVIDIA GPU"));
        // 3 × 0.3 s + 1 GB at 150 MB/s + 5 s of 1080p60 at 4× real time.
        assert!((p.estimate_secs - (0.9 + 1e9 / 150e6 + 5.0 / 4.0)).abs() < 1e-6, "{}", p.estimate_secs);
        let none = preview_of(Path::new("/v"), Path::new("/c"), &files, None);
        assert!(none.no_encoder);
    }

    #[test]
    fn the_meter_moves_from_the_guess_toward_measured_speed() {
        let mut meter = Meter::new(Rates::guess(true));
        assert_eq!(meter.rates().reencode_speed, 4.0);
        // A long 1080p60 file encoded at 2× real time: 600 s of video in 300 s.
        let long = planned(MediaInfo { duration_ms: 600_000, ..info(Some("hevc"), "yuv420p", &["aac"]) }, "mp4", 1);
        meter.record(&long, 300.0);
        let speed = meter.rates().reencode_speed;
        assert!((2.0..2.1).contains(&speed), "{speed}");
    }

    #[test]
    fn eta_counts_the_current_files_rest_and_what_follows() {
        let rates = Rates { instant_secs: 1.0, remux_bytes_per_sec: 100.0, reencode_speed: 1.0 };
        let files = vec![
            planned(info(Some("h264"), "yuv420p", &["aac"]), "mp4", 0),
            planned(info(Some("hevc"), "yuv420p", &["aac"]), "mp4", 0),
            planned(info(Some("h264"), "yuv420p", &["aac"]), "mkv", 200),
        ];
        // Half through the 5 s re-encode (+1 s overhead): 3 s, then the remux: 1 + 2 s.
        assert!((eta_secs(&files, 1, 0.5, &rates) - 6.0).abs() < 1e-9);
        assert_eq!(eta_secs(&files, 3, 0.0, &rates), 0.0);
    }

    #[test]
    fn plans_in_place_only_for_h264_aac_mp4() {
        assert_eq!(plan(&info(Some("h264"), "yuv420p", &["aac"]), "mp4"), Plan::InPlace);
        assert_eq!(plan(&info(Some("h264"), "yuv420p", &["aac", "aac"]), "m4v"), Plan::InPlace);
        // Same codecs, other container: a quick remux.
        assert_eq!(
            plan(&info(Some("h264"), "yuv420p", &["aac"]), "mkv"),
            Plan::Convert { reencode_video: false, audio: AudioPlan::Copy }
        );
        // H.265 / AV1 / 10-bit H.264: re-encoded.
        for (codec, pix) in [("hevc", "yuv420p"), ("av1", "yuv420p"), ("h264", "yuv420p10le")] {
            assert_eq!(
                plan(&info(Some(codec), pix, &["aac"]), "mp4"),
                Plan::Convert { reencode_video: true, audio: AudioPlan::Copy },
                "{codec} {pix}"
            );
        }
        // Opus audio is encoded; no audio gets a silent track.
        assert_eq!(
            plan(&info(Some("h264"), "yuv420p", &["opus"]), "mp4"),
            Plan::Convert { reencode_video: false, audio: AudioPlan::Encode }
        );
        assert_eq!(
            plan(&info(Some("h264"), "yuv420p", &[]), "mp4"),
            Plan::Convert { reencode_video: false, audio: AudioPlan::AddSilent }
        );
    }

    #[test]
    fn plans_pictures_and_skips_the_rest() {
        let png = MediaInfo { format_name: "png_pipe".into(), video_codec: Some("png".into()), width: 10, height: 10, ..Default::default() };
        assert_eq!(plan(&png, "png"), Plan::InPlace);
        assert!(matches!(plan(&png, "bmp"), Plan::Skip(_)));
        assert!(matches!(plan(&info(None, "", &["aac"]), "mp4"), Plan::Skip(_)));
        let empty = MediaInfo { duration_ms: 0, ..info(Some("h264"), "yuv420p", &["aac"]) };
        assert!(matches!(plan(&empty, "mp4"), Plan::Skip(_)));
    }

    #[test]
    fn convert_args_map_audio_and_choose_codecs() {
        let silent = convert_args(Path::new("/in.mkv"), true, AudioPlan::AddSilent, &["-c:v", "libx264"]).join(" ");
        assert!(silent.contains("-f lavfi -i anullsrc=r=48000:cl=stereo"));
        assert!(silent.contains("-map 0:v:0 -map 1:a:0 -shortest"));
        assert!(silent.contains("-c:v libx264 -pix_fmt yuv420p -force_key_frames"));
        assert!(silent.contains("-c:a aac"));
        let remux = convert_args(Path::new("/in.mkv"), false, AudioPlan::Copy, &[]).join(" ");
        assert!(remux.contains("-map 0:a -c:v copy -c:a copy -movflags +faststart -f mp4"));
        assert!(!remux.contains("lavfi"));
    }

    #[test]
    fn games_come_from_game_folders_only() {
        let root = Path::new("/videos");
        assert_eq!(guess_game(Path::new("/videos/Counter-strike 2/a.mp4"), root).as_deref(), Some("Counter-strike 2"));
        assert_eq!(guess_game(Path::new("/videos/a.mp4"), root), None);
        assert_eq!(guess_game(Path::new("/videos/Deadlock/Replays/a.mp4"), root), None);
        assert_eq!(title_from(Path::new("/v/Replay_2026-11-10_22-55-49.mp4")), "Replay 2026-11-10 22-55-49");
        assert_eq!(title_from(Path::new("/v/___.mp4")), "Imported clip");
    }

    #[test]
    fn scan_finds_media_in_subfolders_but_not_hidden_ones() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("Game/Deeper")).unwrap();
        fs::create_dir_all(root.join(".hidden")).unwrap();
        for f in ["a.mp4", "Game/b.MKV", "Game/Deeper/c.png", ".hidden/d.mp4", "notes.txt", "Game/e.jpg.part"] {
            fs::write(root.join(f), b"x").unwrap();
        }
        let mut found: Vec<String> =
            scan(root).iter().map(|p| p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/")).collect();
        found.sort();
        assert_eq!(found, vec!["Game/Deeper/c.png", "Game/b.MKV", "a.mp4"]);
    }

    fn make(out: &Path, args: &[&str]) {
        let status = Command::new(&tools().ffmpeg)
            .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y"])
            .args(args)
            .arg(out)
            .status()
            .unwrap();
        assert!(status.success(), "can't make {}", out.display());
    }

    struct World {
        dir: tempfile::TempDir,
        library: Mutex<Library>,
    }

    impl World {
        fn new() -> World {
            let dir = tempfile::tempdir().unwrap();
            let library = Mutex::new(Library::open(&dir.path().join("data/library.db")).unwrap());
            fs::create_dir_all(dir.path().join("src/Deadlock")).unwrap();
            World { dir, library }
        }
        fn root(&self) -> PathBuf {
            dunce::canonicalize(self.dir.path().join("src")).unwrap()
        }
        fn import(&self, file: &Path) -> Outcome {
            let copies = self.dir.path().join("clips/Imported");
            import_one(&tools(), &self.library, file, &self.root(), &copies).unwrap()
        }
    }

    const SRC: [&str; 4] = ["-f", "lavfi", "-i", "testsrc2=size=320x240:rate=30"];
    const TONE: [&str; 4] = ["-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000"];

    #[test]
    fn imports_an_h264_mp4_in_place_once_and_keeps_it_from_the_cleanup() {
        let w = World::new();
        let file = w.root().join("Deadlock/Replay_1.mp4");
        let args: Vec<&str> = [&SRC[..], &TONE[..], &["-t", "2", "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac"]].concat();
        make(&file, &args);
        let Outcome::Imported { view, converted } = w.import(&file) else { panic!("not imported") };
        assert!(!converted);
        assert_eq!(view.clip.path, file);
        assert_eq!(view.clip.game_name.as_deref(), Some("Deadlock"));
        assert_eq!(view.clip.title, "Replay 1");
        assert!((1_900..=2_100).contains(&view.clip.duration_ms));
        assert!(view.thumb_path.is_some());
        assert_eq!(w.import(&file), Outcome::AlreadyThere);
        assert!(crate::commands::lock(&w.library).cleanup_candidates(i64::MAX).unwrap().is_empty());
    }

    #[test]
    fn converts_other_codecs_and_silent_videos_into_h264_aac_copies() {
        let w = World::new();
        // MPEG-4 Part 2 video (not H.264) in an MKV, with no audio at all.
        let file = w.root().join("odd.mkv");
        let args: Vec<&str> = [&SRC[..], &["-t", "2", "-c:v", "mpeg4"]].concat();
        make(&file, &args);
        let Outcome::Imported { view, converted } = w.import(&file) else { panic!("not imported") };
        assert!(converted);
        assert_ne!(view.clip.path, file);
        assert!(view.clip.path.starts_with(w.dir.path().join("clips/Imported")));
        assert_eq!(view.clip.game_name, None);
        let out = crate::ffmpeg::inspect(&tools(), &view.clip.path).unwrap();
        assert_eq!(out.video_codec.as_deref(), Some("h264"));
        assert_eq!(out.pix_fmt.as_deref(), Some("yuv420p"));
        assert_eq!(out.audio_codecs, vec!["aac"]);
        assert!(file.is_file(), "the original stays");
        // A keyframe every second, whichever encoder ran, so trims stay exact.
        let keys = Command::new(&tools().ffprobe)
            .args(["-v", "error", "-skip_frame", "nokey", "-select_streams", "v", "-show_entries", "frame=pts_time", "-of", "csv=p=0"])
            .arg(&view.clip.path)
            .output()
            .unwrap();
        let times: Vec<f64> = String::from_utf8_lossy(&keys.stdout).lines().filter_map(|l| l.trim_end_matches(',').parse().ok()).collect();
        assert!(times.len() >= 2 && times.windows(2).all(|w| (w[1] - w[0] - 1.0).abs() < 0.1), "{times:?}");
        assert_eq!(w.import(&file), Outcome::AlreadyThere, "the original isn't imported twice");
    }

    #[test]
    fn checking_a_folder_plans_every_file_and_stopping_drops_the_half_written_copy() {
        let w = World::new();
        let root = w.root();
        let mp4 = root.join("Deadlock/ok.mp4");
        let args: Vec<&str> = [&SRC[..], &TONE[..], &["-t", "1", "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac"]].concat();
        make(&mp4, &args);
        let odd = root.join("long.mkv");
        let args: Vec<&str> = [&SRC[..], &["-t", "20", "-c:v", "mpeg4"]].concat();
        make(&odd, &args);
        fs::write(root.join("junk.mp4"), b"x").unwrap();

        let mut reports = Vec::new();
        let files = check_folder(&tools(), &w.library, &root, &AtomicBool::new(false), |c, t| reports.push((c, t))).unwrap();
        assert_eq!(reports.last(), Some(&(3, 3)));
        let work: Vec<Work> = files.iter().map(Planned::work).collect();
        assert_eq!(work.iter().filter(|w| **w == Work::Instant).count(), 1);
        assert_eq!(work.iter().filter(|w| **w == Work::Reencode).count(), 1);
        assert_eq!(work.iter().filter(|w| **w == Work::None).count(), 1);

        // Stop as soon as the conversion reports progress: nothing is kept.
        let long = files.iter().find(|f| f.work() == Work::Reencode).unwrap();
        let copies = w.dir.path().join("clips/Imported");
        let stop = AtomicBool::new(false);
        let encoder = detect_encoder(&tools());
        let outcome = import_planned(&tools(), &w.library, long, &root, &copies, encoder.as_ref(), &stop, |_| {
            stop.store(true, Ordering::SeqCst)
        })
        .unwrap();
        assert_eq!(outcome, Outcome::Stopped);
        let left: Vec<_> = fs::read_dir(&copies).map(|d| d.flatten().collect()).unwrap_or_default();
        assert!(left.is_empty(), "{left:?}");
        assert!(!crate::commands::lock(&w.library).contains_file(&long.path).unwrap());
    }

    #[test]
    fn imports_pictures_and_skips_junk() {
        let w = World::new();
        let png = w.root().join("shot.png");
        make(&png, &["-f", "lavfi", "-i", "testsrc2=size=64x48", "-frames:v", "1"]);
        let Outcome::Imported { view, .. } = w.import(&png) else { panic!("not imported") };
        assert_eq!(view.clip.kind, ClipKind::Screenshot);
        assert_eq!((view.clip.width, view.clip.height), (64, 48));
        let junk = w.root().join("junk.mp4");
        fs::write(&junk, b"not a video").unwrap();
        assert!(matches!(w.import(&junk), Outcome::Skipped(_)));
    }
}
