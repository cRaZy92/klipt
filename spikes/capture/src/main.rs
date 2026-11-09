mod obs;
mod record;
use std::ffi::{c_void, CStr, CString};

extern "C" { fn wl_display_connect(name: *const i8) -> *mut c_void; }

pub fn cs(s: &str) -> CString { CString::new(s).unwrap() }
pub unsafe fn s(p: *const i8) -> String { if p.is_null() { String::new() } else { CStr::from_ptr(p).to_string_lossy().into() } }

unsafe fn init() -> anyhow::Result<()> {
    let disp = wl_display_connect(std::ptr::null());
    anyhow::ensure!(!disp.is_null(), "wayland connect failed");
    obs::obs_set_nix_platform(obs::obs_nix_platform_type_OBS_NIX_PLATFORM_WAYLAND);
    obs::obs_set_nix_platform_display(disp);
    let loc = cs("en-US");
    anyhow::ensure!(obs::obs_startup(loc.as_ptr(), std::ptr::null(), std::ptr::null_mut()), "obs_startup failed");
    let bin = cs(&std::env::var("KLIPT_OBS_BIN").unwrap_or("/usr/lib/obs-modules/core/".into()));
    let data = cs(&std::env::var("KLIPT_OBS_DATA").unwrap_or("/usr/share/obs/obs-modules/core/%module%".into()));
    obs::obs_add_module_path(bin.as_ptr(), data.as_ptr());
    let gm = cs("libobs-opengl");
    let mut ovi = obs::obs_video_info {
        graphics_module: gm.as_ptr(), fps_num: std::env::var("KLIPT_FPS").ok().and_then(|v| v.parse().ok()).unwrap_or(60), fps_den: 1,
        base_width: 1920, base_height: 1080, output_width: 1920, output_height: 1080,
        output_format: obs::video_format_VIDEO_FORMAT_NV12, adapter: 0, gpu_conversion: true,
        colorspace: obs::video_colorspace_VIDEO_CS_709, range: obs::video_range_type_VIDEO_RANGE_PARTIAL,
        scale_type: obs::obs_scale_type_OBS_SCALE_BICUBIC,
    };
    let r = obs::obs_reset_video(&mut ovi);
    anyhow::ensure!(r == 0, "obs_reset_video failed: {r}");
    let oai = obs::obs_audio_info { samples_per_sec: 48000, speakers: obs::speaker_layout_SPEAKERS_STEREO };
    anyhow::ensure!(obs::obs_reset_audio(&oai), "obs_reset_audio failed");
    let bin_dir = std::env::var("KLIPT_OBS_BIN").unwrap_or("/usr/lib/obs-modules/core".into());
    let data_dir = std::env::var("KLIPT_OBS_DATA").unwrap_or("/usr/share/obs/obs-modules/core".into());
    for m in ["linux-pipewire", "linux-pulseaudio", "obs-ffmpeg", "obs-nvenc", "obs-x264", "obs-outputs"] {
        let path = cs(&format!("{bin_dir}/{m}.so"));
        let data = cs(&format!("{data_dir}/{m}"));
        let mut module: *mut obs::obs_module_t = std::ptr::null_mut();
        let r = obs::obs_open_module(&mut module, path.as_ptr(), data.as_ptr());
        let ok = r == 0 && obs::obs_init_module(module);
        eprintln!("module {m}: open={r} init={ok}");
    }
    obs::obs_post_load_modules();
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    unsafe {
        init()?;
        if args.get(1).map(|s| s.as_str()) == Some("record") {
            let get = |k: &str, d: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned().unwrap_or(d.into());
            record::run(record::Opts {
                exit_after: args.iter().position(|a| a == "--exit-after").map(|i| args[i + 1].parse().unwrap()),
                save_at: get("--save-at", "").split(',').filter(|x| !x.is_empty()).map(|x| x.parse().unwrap()).collect(),
                seconds: get("--seconds", "30").parse()?,
                out_dir: get("--out", "/tmp/klipt-spike"),
                mic: args.iter().any(|a| a == "--mic"),
                encoder: get("--encoder", "obs_nvenc_h264_tex"),
                bitrate: get("--bitrate", "20000").parse()?,
            })?;
        } else {
            let mut id: *const i8 = std::ptr::null();
            let (mut i, mut v) = (0, vec![]);
            while obs::obs_enum_input_types(i, &mut id) { v.push(s(id)); i += 1; }
            println!("INPUTS: {}", v.join(" "));
            let (mut i, mut v) = (0, vec![]);
            while obs::obs_enum_encoder_types(i, &mut id) { v.push(s(id)); i += 1; }
            println!("ENCODERS: {}", v.join(" "));
            let (mut i, mut v) = (0, vec![]);
            while obs::obs_enum_output_types(i, &mut id) { v.push(s(id)); i += 1; }
            println!("OUTPUTS: {}", v.join(" "));
        }
        obs::obs_shutdown();
    }
    Ok(())
}
