//! Runtime-loaded libobs: paths, startup, module loading, logging and small safe helpers.

#![allow(non_upper_case_globals, non_camel_case_types, non_snake_case, dead_code, improper_ctypes, clippy::all)]

mod sys {
    #![allow(
        non_upper_case_globals,
        non_camel_case_types,
        non_snake_case,
        dead_code,
        improper_ctypes,
        unsafe_op_in_unsafe_fn,
        clippy::all
    )]
    include!(concat!(env!("OUT_DIR"), "/obs_sys.rs"));
}

pub use sys::*;

use anyhow::{anyhow, bail, Context, Result};
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static API: OnceLock<Obs> = OnceLock::new();

/// The loaded libobs. Panics if called before [`init`] succeeded.
pub fn api() -> &'static Obs {
    API.get().expect("libobs is not loaded")
}

pub fn cs(s: &str) -> CString {
    CString::new(s.replace('\0', "")).unwrap()
}

/// Copies a C string owned by libobs. A null pointer becomes "".
pub unsafe fn from_c(p: *const c_char) -> String {
    if p.is_null() {
        String::new()
    } else {
        CStr::from_ptr(p).to_string_lossy().into_owned()
    }
}

// ---------------------------------------------------------------------------------------------
// Paths

/// Where the libobs library, its plugins and their data live.
#[derive(Debug, Clone)]
pub struct ObsLayout {
    /// Candidate paths for the libobs shared library, tried in order.
    pub library: Vec<PathBuf>,
    /// Directory that must be the working directory (libobs resolves `../../data/libobs/`
    /// relative to it on Windows) and that holds the runtime DLLs.
    pub bin_dir: PathBuf,
    /// Binary path of module `name`.
    module_bin: fn(&Path, &str) -> PathBuf,
    /// Data directory of module `name`.
    module_data: fn(&Path, &str) -> PathBuf,
    root: PathBuf,
}

impl ObsLayout {
    /// Windows: the OBS release layout. `root` holds `bin/64bit/obs.dll`, `core/<module>/<module>.dll`,
    /// `core/<module>/data/` and `data/libobs/`. By default `root` is two levels above this exe,
    /// because klipt-recorder.exe sits in `bin/64bit` next to obs.dll and OBS's helper executables.
    #[cfg(windows)]
    pub fn detect(root_override: Option<&Path>) -> Result<Self> {
        let root = match root_override {
            Some(r) => r.to_path_buf(),
            None => {
                let exe = std::env::current_exe().context("cannot find own executable")?;
                exe.parent()
                    .and_then(Path::parent)
                    .and_then(Path::parent)
                    .map(Path::to_path_buf)
                    .ok_or_else(|| anyhow!("cannot derive OBS root from {}", exe.display()))?
            }
        };
        let bin_dir = root.join("bin").join("64bit");
        Ok(Self {
            library: vec![bin_dir.join("obs.dll")],
            bin_dir,
            module_bin: |root, name| root.join("core").join(name).join(format!("{name}.dll")),
            module_data: |root, name| root.join("core").join(name).join("data"),
            root,
        })
    }

    /// Linux: an install prefix (default `/usr`) with `lib/libobs.so.<major>`,
    /// `lib/obs-modules/core/<module>.so` and `share/obs/obs-modules/core/<module>/`.
    #[cfg(not(windows))]
    pub fn detect(root_override: Option<&Path>) -> Result<Self> {
        let root = root_override.map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("/usr"));
        let lib = root.join("lib");
        Ok(Self {
            library: ["libobs.so.33", "libobs.so.32", "libobs.so.31", "libobs.so.30", "libobs.so.0", "libobs.so"]
                .iter()
                .map(|n| lib.join(n))
                .collect(),
            bin_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            module_bin: |root, name| root.join("lib/obs-modules/core").join(format!("{name}.so")),
            module_data: |root, name| root.join("share/obs/obs-modules/core").join(name),
            root,
        })
    }

    pub fn module_bin(&self, name: &str) -> PathBuf {
        (self.module_bin)(&self.root, name)
    }

    pub fn module_data(&self, name: &str) -> PathBuf {
        (self.module_data)(&self.root, name)
    }
}

#[cfg(windows)]
pub const GRAPHICS_MODULE: &str = "libobs-d3d11";
#[cfg(not(windows))]
pub const GRAPHICS_MODULE: &str = "libobs-opengl";

/// Modules loaded explicitly. OBS 33's automatic discovery finds nothing outside OBS's own
/// executable layout, so every module is opened by path.
#[cfg(windows)]
pub const MODULES: &[(&str, bool)] = &[
    // (name, required)
    ("win-capture", true),
    ("win-wasapi", true),
    ("obs-ffmpeg", true),
    ("obs-outputs", true),
    ("obs-x264", true),
    ("obs-nvenc", false),
    ("obs-qsv11", false),
];
#[cfg(not(windows))]
pub const MODULES: &[(&str, bool)] = &[
    ("linux-pipewire", true),
    ("linux-pulseaudio", true),
    ("obs-ffmpeg", true),
    ("obs-outputs", true),
    ("obs-x264", true),
    ("obs-nvenc", false),
];

/// Hardware encoders first, x264 last. `auto` tries them in this order.
pub const H264_ENCODERS: &[&str] = &["obs_nvenc_h264_tex", "h264_texture_amf", "obs_qsv11_v2", "obs_x264"];

// ---------------------------------------------------------------------------------------------
// Logging: libobs's default handler prints info/warnings to stdout, which would corrupt the
// event stream. Everything goes to stderr instead.

#[cfg(windows)]
type VaListArg = *mut c_char;
#[cfg(not(windows))]
type VaListArg = *mut __va_list_tag;

extern "C" {
    fn vsnprintf(buf: *mut c_char, size: usize, format: *const c_char, args: VaListArg) -> c_int;
}

unsafe extern "C" fn log_to_stderr(level: c_int, format: *const c_char, args: VaListArg, _param: *mut c_void) {
    let level = level as i64;
    if level >= LOG_DEBUG as i64 && std::env::var_os("KLIPT_RECORDER_DEBUG").is_none() {
        return;
    }
    let mut buf = [0 as c_char; 8192];
    vsnprintf(buf.as_mut_ptr(), buf.len(), format, args);
    let msg = CStr::from_ptr(buf.as_ptr()).to_string_lossy();
    let tag = match level {
        l if l <= LOG_ERROR as i64 => "error",
        l if l <= LOG_WARNING as i64 => "warning",
        l if l <= LOG_INFO as i64 => "info",
        _ => "debug",
    };
    log!("[obs {tag}] {msg}");
}

// ---------------------------------------------------------------------------------------------
// Startup

pub struct InitInfo {
    pub version: String,
    /// H.264 encoder ids registered by the loaded modules, in preference order.
    pub encoders: Vec<String>,
}

/// Loads libobs, starts it, initializes graphics/audio and loads the capture/encode modules.
pub fn init(layout: &ObsLayout) -> Result<InitInfo> {
    prepare_process(layout)?;
    let lib = layout.library.iter().find_map(|p| unsafe { open_library(p).ok() }).ok_or_else(|| {
        let tried: Vec<_> = layout.library.iter().map(|p| p.display().to_string()).collect();
        anyhow!("could not load libobs (tried {})", tried.join(", "))
    })?;
    if API.set(lib).is_err() {
        bail!("libobs already initialized");
    }
    let o = api();
    unsafe {
        o.base_set_log_handler(Some(log_to_stderr as _), std::ptr::null_mut());
        platform_pre_startup()?;
        let locale = cs("en-US");
        if !o.obs_startup(locale.as_ptr(), std::ptr::null(), std::ptr::null_mut()) {
            bail!("obs_startup failed");
        }
        // Modules such as win-capture query the graphics device while loading, so video
        // must be up before they are opened. The real canvas is set on `start`.
        reset_video(60, 1920, 1080)?;
        reset_audio()?;
        for (name, required) in MODULES {
            match load_module(layout, name) {
                Ok(()) => log!("module {name}: loaded"),
                Err(e) if *required => return Err(e),
                Err(e) => log!("module {name}: skipped ({e})"),
            }
        }
        o.obs_post_load_modules();
    }
    let available = registered_encoders();
    let encoders =
        H264_ENCODERS.iter().filter(|id| available.iter().any(|a| a == *id)).map(|s| s.to_string()).collect();
    let version = unsafe { from_c(o.obs_get_version_string()) };
    Ok(InitInfo { version, encoders })
}

#[cfg(windows)]
unsafe fn open_library(path: &Path) -> std::result::Result<Obs, libloading::Error> {
    use libloading::os::windows::{Library, LOAD_WITH_ALTERED_SEARCH_PATH};
    // Resolve obs.dll's own dependencies from its directory, wherever this exe lives.
    let lib = Library::load_with_flags(path, LOAD_WITH_ALTERED_SEARCH_PATH)?;
    Obs::from_library(lib)
}

#[cfg(not(windows))]
unsafe fn open_library(path: &Path) -> std::result::Result<Obs, libloading::Error> {
    Obs::new(path)
}

#[cfg(windows)]
fn prepare_process(layout: &ObsLayout) -> Result<()> {
    use windows_sys::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
    // Physical pixel sizes for monitors, like OBS itself.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    if !layout.bin_dir.join("obs.dll").exists() {
        bail!("obs.dll not found in {}", layout.bin_dir.display());
    }
    // libobs finds its effects at ../../data/libobs relative to the working directory.
    std::env::set_current_dir(&layout.bin_dir).with_context(|| format!("cannot enter {}", layout.bin_dir.display()))?;
    // libobs loads the graphics module and plugins with LOAD_LIBRARY_SEARCH_DEFAULT_DIRS: only the
    // exe's folder, System32 and AddDllDirectory folders are searched (not PATH or the working
    // directory). When the exe isn't in bin/64bit (e.g. tests with --obs-root), register it.
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::System::LibraryLoader::AddDllDirectory;
        let wide: Vec<u16> = layout.bin_dir.as_os_str().encode_wide().chain(Some(0)).collect();
        if unsafe { AddDllDirectory(wide.as_ptr()) }.is_null() {
            bail!("AddDllDirectory({}) failed: {}", layout.bin_dir.display(), std::io::Error::last_os_error());
        }
    }
    for helper in ["obs-ffmpeg-mux.exe", "obs-nvenc-test.exe", "obs-amf-test.exe"] {
        let next_to_exe = std::env::current_exe()?.with_file_name(helper);
        if !next_to_exe.exists() {
            log!(
                "warning: {helper} is not next to klipt-recorder.exe ({}); {}",
                next_to_exe.display(),
                if helper == "obs-ffmpeg-mux.exe" {
                    "saving clips will fail"
                } else {
                    "that encoder will be unavailable"
                }
            );
        }
    }
    Ok(())
}

/// Linux: libobs also looks for its helpers next to this executable (`/proc/self/exe`), not in
/// /usr/bin. For `cargo run`, symlink them into the target directory.
#[cfg(not(windows))]
fn prepare_process(layout: &ObsLayout) -> Result<()> {
    for helper in ["obs-ffmpeg-mux", "obs-nvenc-test"] {
        let next_to_exe = std::env::current_exe()?.with_file_name(helper);
        if !next_to_exe.exists() {
            log!(
                "warning: {helper} is not next to klipt-recorder ({}); {}. Fix: ln -s {} {}",
                next_to_exe.display(),
                if helper == "obs-ffmpeg-mux" { "saving clips will fail" } else { "NVENC will be unavailable" },
                layout.root.join("bin").join(helper).display(),
                next_to_exe.display()
            );
        }
    }
    Ok(())
}

#[cfg(windows)]
unsafe fn platform_pre_startup() -> Result<()> {
    Ok(())
}

#[cfg(target_os = "linux")]
unsafe fn platform_pre_startup() -> Result<()> {
    extern "C" {
        fn wl_display_connect(name: *const c_char) -> *mut c_void;
    }
    let display = wl_display_connect(std::ptr::null());
    if display.is_null() {
        bail!("cannot connect to the Wayland display");
    }
    let o = api();
    o.obs_set_nix_platform(obs_nix_platform_type_OBS_NIX_PLATFORM_WAYLAND);
    o.obs_set_nix_platform_display(display);
    Ok(())
}

unsafe fn load_module(layout: &ObsLayout, name: &str) -> Result<()> {
    let bin = layout.module_bin(name);
    if !bin.exists() {
        bail!("{} does not exist", bin.display());
    }
    let data = layout.module_data(name);
    let bin_c = cs(&bin.to_string_lossy());
    let data_c = cs(&data.to_string_lossy());
    let o = api();
    let mut module: *mut obs_module_t = std::ptr::null_mut();
    let code = o.obs_open_module(&mut module, bin_c.as_ptr(), data_c.as_ptr());
    if code != MODULE_SUCCESS as c_int {
        bail!("obs_open_module({}) returned {code}", bin.display());
    }
    if !o.obs_init_module(module) {
        bail!("obs_init_module({name}) failed");
    }
    Ok(())
}

/// Resets the video pipeline: canvas == output size (scaling happens in the scene), NV12, BT.709 partial.
pub fn reset_video(fps: u32, width: u32, height: u32) -> Result<()> {
    let graphics = cs(GRAPHICS_MODULE);
    let mut ovi = obs_video_info {
        graphics_module: graphics.as_ptr(),
        fps_num: fps,
        fps_den: 1,
        base_width: width,
        base_height: height,
        output_width: width,
        output_height: height,
        output_format: video_format_VIDEO_FORMAT_NV12,
        adapter: 0,
        gpu_conversion: true,
        colorspace: video_colorspace_VIDEO_CS_709,
        range: video_range_type_VIDEO_RANGE_PARTIAL,
        scale_type: obs_scale_type_OBS_SCALE_BICUBIC,
    };
    let code = unsafe { api().obs_reset_video(&mut ovi) };
    if code != OBS_VIDEO_SUCCESS as c_int {
        bail!("obs_reset_video({width}x{height}@{fps}) returned {code}");
    }
    Ok(())
}

fn reset_audio() -> Result<()> {
    let oai = obs_audio_info { samples_per_sec: 48000, speakers: speaker_layout_SPEAKERS_STEREO };
    if unsafe { !api().obs_reset_audio(&oai) } {
        bail!("obs_reset_audio failed");
    }
    Ok(())
}

pub fn registered_encoders() -> Vec<String> {
    let mut out = vec![];
    let mut id: *const c_char = std::ptr::null();
    let mut i = 0;
    unsafe {
        while api().obs_enum_encoder_types(i, &mut id) {
            out.push(from_c(id));
            i += 1;
        }
    }
    out
}

/// Runs pending GLib callbacks. On Linux the screen-cast portal talks D-Bus through the default
/// GLib main context, which nothing else iterates in this process.
#[cfg(target_os = "linux")]
pub fn pump_events() {
    extern "C" {
        fn g_main_context_iteration(context: *mut c_void, may_block: c_int) -> c_int;
    }
    unsafe { while g_main_context_iteration(std::ptr::null_mut(), 0) != 0 {} }
}

#[cfg(not(target_os = "linux"))]
pub fn pump_events() {}

pub fn shutdown() {
    if let Some(o) = API.get() {
        unsafe { o.obs_shutdown() };
    }
}

// ---------------------------------------------------------------------------------------------
// Helpers

/// Owned `obs_data_t` (settings object). Released on drop.
pub struct Data(pub *mut obs_data_t);

impl Data {
    pub fn new() -> Self {
        Self(unsafe { api().obs_data_create() })
    }
    pub fn str(self, key: &str, value: &str) -> Self {
        unsafe { api().obs_data_set_string(self.0, cs(key).as_ptr(), cs(value).as_ptr()) };
        self
    }
    pub fn int(self, key: &str, value: i64) -> Self {
        unsafe { api().obs_data_set_int(self.0, cs(key).as_ptr(), value) };
        self
    }
    pub fn bool(self, key: &str, value: bool) -> Self {
        unsafe { api().obs_data_set_bool(self.0, cs(key).as_ptr(), value) };
        self
    }
}

impl Drop for Data {
    fn drop(&mut self) {
        unsafe { api().obs_data_release(self.0) };
    }
}

/// The (label, value) items of a string list property, e.g. a source's `device_id` or `monitor_id`.
pub fn list_property_items(source_id: &str, property: &str) -> Vec<(String, String)> {
    let o = api();
    let mut out = vec![];
    unsafe {
        let props = o.obs_get_source_properties(cs(source_id).as_ptr());
        if props.is_null() {
            return out;
        }
        let prop = o.obs_properties_get(props, cs(property).as_ptr());
        if !prop.is_null() {
            for i in 0..o.obs_property_list_item_count(prop) {
                out.push((
                    from_c(o.obs_property_list_item_name(prop, i)),
                    from_c(o.obs_property_list_item_string(prop, i)),
                ));
            }
        }
        o.obs_properties_destroy(props);
    }
    out
}

/// Reads an integer argument from signal/proc call data.
pub unsafe fn calldata_int(cd: *const calldata_t, name: &str) -> i64 {
    let mut v: i64 = 0;
    api().calldata_get_data(cd, cs(name).as_ptr(), &mut v as *mut i64 as *mut c_void, std::mem::size_of::<i64>());
    v
}

/// Reads a string argument from signal/proc call data. Strings are stored inline in the call
/// data (not as pointers), so `calldata_get_data` with a pointer size would never match.
pub unsafe fn calldata_string(cd: *const calldata_t, name: &str) -> String {
    let mut p: *const c_char = std::ptr::null();
    if !api().calldata_get_string(cd, cs(name).as_ptr(), &mut p) {
        return String::new();
    }
    from_c(p)
}

/// Calls a proc on an output (e.g. `save`, `get_last_replay`) and returns the call data's string `out`.
pub unsafe fn call_output_proc(output: *mut obs_output_t, proc_name: &str, out_key: Option<&str>) -> Option<String> {
    let o = api();
    let mut cd: calldata_t = std::mem::zeroed();
    let ph = o.obs_output_get_proc_handler(output);
    let ok = o.proc_handler_call(ph, cs(proc_name).as_ptr(), &mut cd);
    let result = if ok { out_key.map(|k| calldata_string(&cd, k)) } else { None };
    o.bfree(cd.stack as *mut c_void);
    result
}
