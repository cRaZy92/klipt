// Spike recorder: display + system audio (track 1) + mic (track 2) -> NVENC/AAC -> libobs replay_buffer.
use crate::{cs, obs, s};
use std::ffi::c_void;
use std::io::BufRead;
use std::sync::mpsc;

#[repr(C)] pub struct libc_rusage { utime: (i64, i64), stime: (i64, i64), maxrss: i64, _rest: [i64; 13] }
extern "C" { fn getrusage(who: i32, usage: *mut libc_rusage) -> i32; }
extern "C" { fn g_main_context_iteration(ctx: *mut c_void, may_block: i32) -> i32; }

pub struct Opts { pub exit_after: Option<u64>, pub save_at: Vec<u64>, pub seconds: i64, pub out_dir: String, pub mic: bool, pub encoder: String, pub bitrate: i64 }

fn token_path() -> std::path::PathBuf {
    let d = std::path::PathBuf::from(std::env::var("HOME").unwrap()).join(".local/share/klipt-spike");
    std::fs::create_dir_all(&d).ok();
    d.join("restore_token")
}

unsafe extern "C" fn on_saved(_p: *mut c_void, _cd: *mut obs::calldata_t) {
    println!("{{\"event\":\"saved_signal\"}}");
}

unsafe fn last_replay(out: *mut obs::obs_output_t) -> String {
    let mut cd: obs::calldata_t = std::mem::zeroed();
    let ph = obs::obs_output_get_proc_handler(out);
    obs::proc_handler_call(ph, cs("get_last_replay").as_ptr(), &mut cd);
    let mut p: *const i8 = std::ptr::null();
    obs::calldata_get_data(&cd, cs("path").as_ptr(), &mut p as *mut _ as *mut c_void, std::mem::size_of::<*const i8>());
    let r = s(p);
    obs::bfree(cd.stack as *mut c_void);
    r
}

pub unsafe fn run(o: Opts) -> anyhow::Result<()> {
    // --- display source (portal + PipeWire) ---
    let st = obs::obs_data_create();
    obs::obs_data_set_bool(st, cs("ShowCursor").as_ptr(), true);
    if let Ok(tok) = std::fs::read_to_string(token_path()) {
        obs::obs_data_set_string(st, cs("RestoreToken").as_ptr(), cs(tok.trim()).as_ptr());
        eprintln!("using saved restore token");
    }
    let display = obs::obs_source_create(cs("pipewire-screen-capture-source").as_ptr(), cs("display").as_ptr(), st, std::ptr::null_mut());
    obs::obs_data_release(st);
    anyhow::ensure!(!display.is_null(), "display source create failed");
    let scene = obs::obs_scene_create(cs("scene").as_ptr());
    obs::obs_scene_add(scene, display);
    obs::obs_set_output_source(0, obs::obs_scene_get_source(scene));

    // --- audio: desktop -> mixer 1 (track 1), mic -> mixer 2 (track 2) ---
    let ast = obs::obs_data_create();
    obs::obs_data_set_string(ast, cs("device_id").as_ptr(), cs("default").as_ptr());
    let desk = obs::obs_source_create(cs("pulse_output_capture").as_ptr(), cs("desktop").as_ptr(), ast, std::ptr::null_mut());
    obs::obs_source_set_audio_mixers(desk, 1 << 0);
    obs::obs_set_output_source(1, desk);
    if o.mic {
        let mic = obs::obs_source_create(cs("pulse_input_capture").as_ptr(), cs("mic").as_ptr(), ast, std::ptr::null_mut());
        obs::obs_source_set_audio_mixers(mic, 1 << 1);
        obs::obs_set_output_source(2, mic);
    }
    obs::obs_data_release(ast);

    // --- encoders ---
    let vs = obs::obs_data_create();
    obs::obs_data_set_string(vs, cs("rate_control").as_ptr(), cs("CBR").as_ptr());
    obs::obs_data_set_int(vs, cs("bitrate").as_ptr(), o.bitrate);
    obs::obs_data_set_int(vs, cs("keyint_sec").as_ptr(), 1);
    let preset = std::env::var("KLIPT_PRESET").unwrap_or("p4".into());
    obs::obs_data_set_string(vs, cs("preset2").as_ptr(), cs(&preset).as_ptr());
    let tune = std::env::var("KLIPT_TUNE").unwrap_or("hq".into());
    obs::obs_data_set_string(vs, cs("tune").as_ptr(), cs(&tune).as_ptr());
    obs::obs_data_set_string(vs, cs("multipass").as_ptr(), cs(&std::env::var("KLIPT_MULTIPASS").unwrap_or("disabled".into())).as_ptr());
    obs::obs_data_set_bool(vs, cs("lookahead").as_ptr(), std::env::var("KLIPT_LOOKAHEAD").is_ok());
    obs::obs_data_set_int(vs, cs("bf").as_ptr(), std::env::var("KLIPT_BF").ok().and_then(|v| v.parse().ok()).unwrap_or(0));
    obs::obs_data_set_string(vs, cs("profile").as_ptr(), cs("high").as_ptr());
    let venc = obs::obs_video_encoder_create(cs(&o.encoder).as_ptr(), cs("venc").as_ptr(), vs, std::ptr::null_mut());
    obs::obs_data_release(vs);
    anyhow::ensure!(!venc.is_null(), "video encoder {} create failed", o.encoder);
    obs::obs_encoder_set_video(venc, obs::obs_get_video());

    let tracks = if o.mic { 2 } else { 1 };
    let mut aencs = vec![];
    for t in 0..tracks {
        let a = obs::obs_data_create();
        obs::obs_data_set_int(a, cs("bitrate").as_ptr(), 160);
        let e = obs::obs_audio_encoder_create(cs("ffmpeg_aac").as_ptr(), cs(&format!("aac{t}")).as_ptr(), a, t, std::ptr::null_mut());
        obs::obs_data_release(a);
        anyhow::ensure!(!e.is_null(), "aac encoder create failed");
        obs::obs_encoder_set_audio(e, obs::obs_get_audio());
        aencs.push(e);
    }

    // --- replay buffer output ---
    let rs = obs::obs_data_create();
    obs::obs_data_set_int(rs, cs("max_time_sec").as_ptr(), o.seconds);
    obs::obs_data_set_int(rs, cs("max_size_mb").as_ptr(), 4096);
    obs::obs_data_set_string(rs, cs("directory").as_ptr(), cs(&o.out_dir).as_ptr());
    obs::obs_data_set_string(rs, cs("format").as_ptr(), cs("Klipt %CCYY-%MM-%DD %hh-%mm-%ss").as_ptr());
    obs::obs_data_set_string(rs, cs("extension").as_ptr(), cs("mp4").as_ptr());
    obs::obs_data_set_bool(rs, cs("allow_spaces").as_ptr(), true);
    let out = obs::obs_output_create(cs("replay_buffer").as_ptr(), cs("rb").as_ptr(), rs, std::ptr::null_mut());
    obs::obs_data_release(rs);
    anyhow::ensure!(!out.is_null(), "replay_buffer create failed");
    obs::obs_output_set_video_encoder(out, venc);
    for (i, e) in aencs.iter().enumerate() { obs::obs_output_set_audio_encoder(out, *e, i); }
    obs::signal_handler_connect(obs::obs_output_get_signal_handler(out), cs("saved").as_ptr(), Some(on_saved), std::ptr::null_mut());

    // stdin commands on a thread; GLib loop (portal D-Bus) on main thread
    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || { for l in std::io::stdin().lock().lines().map_while(Result::ok) { if tx.send(l).is_err() { break; } } });

    let mut started = false;
    let mut start_at: Option<std::time::Instant> = None;
    let mut pending_saves = o.save_at.clone();
    let mut token_saved = false;
    let t0 = std::time::Instant::now();
    loop {
        g_main_context_iteration(std::ptr::null_mut(), 0);
        // start once the display source has real frames
        if !started && obs::obs_source_get_width(display) > 0 {
            anyhow::ensure!(obs::obs_output_start(out), "output start failed: {}", s(obs::obs_output_get_last_error(out)));
            started = true;
            start_at = Some(std::time::Instant::now());
            println!("{{\"event\":\"started\",\"width\":{},\"height\":{},\"afterMs\":{}}}", obs::obs_source_get_width(display), obs::obs_source_get_height(display), t0.elapsed().as_millis());
        }
        if started && !token_saved {
            let cur = obs::obs_source_get_settings(display);
            let tok = s(obs::obs_data_get_string(cur, cs("RestoreToken").as_ptr()));
            obs::obs_data_release(cur);
            if !tok.is_empty() { std::fs::write(token_path(), &tok)?; token_saved = true; eprintln!("restore token saved"); }
        }
        let mut auto: Option<String> = None;
        if let Some(t) = start_at {
            let el = t.elapsed().as_secs();
            if let Some(i) = pending_saves.iter().position(|&s| el >= s) { pending_saves.remove(i); auto = Some("save".into()); }
            if o.exit_after.map_or(false, |x| el >= x) { auto = Some("quit".into()); }
        }
        let next = match auto { Some(c) => Ok(c), None => rx.try_recv() };
        match next {
            Ok(cmd) if cmd.trim() == "save" => {
                let t = std::time::Instant::now();
                let ph = obs::obs_output_get_proc_handler(out);
                let mut cd: obs::calldata_t = std::mem::zeroed();
                obs::proc_handler_call(ph, cs("save").as_ptr(), &mut cd);
                obs::bfree(cd.stack as *mut c_void);
                // wait for the file to be written
                let before = last_replay(out);
                for _ in 0..500 {
                    g_main_context_iteration(std::ptr::null_mut(), 0);
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    let p = last_replay(out);
                    if !p.is_empty() && p != before { println!("{{\"event\":\"saved\",\"path\":{:?},\"saveMs\":{}}}", p, t.elapsed().as_millis()); break; }
                }
            }
            Ok(cmd) if cmd.trim() == "quit" => break,
            Ok(_) | Err(mpsc::TryRecvError::Empty) => std::thread::sleep(std::time::Duration::from_millis(5)),
            Err(_) => break,
        }
    }
    if let Some(t) = start_at {
        let mut ru: libc_rusage = std::mem::zeroed();
        getrusage(0, &mut ru);
        let cpu = ru.utime.0 as f64 + ru.utime.1 as f64 / 1e6 + ru.stime.0 as f64 + ru.stime.1 as f64 / 1e6;
        let wall = t0.elapsed().as_secs_f64();
        println!("{{\"event\":\"stats\",\"cpuSeconds\":{:.2},\"wallSeconds\":{:.1},\"avgCpuPercentOfOneCore\":{:.1},\"maxRssMb\":{}}}", cpu, wall, 100.0 * cpu / wall, ru.maxrss / 1024);
        let _ = t;
    }
    obs::obs_output_stop(out);
    std::thread::sleep(std::time::Duration::from_millis(500));
    Ok(())
}
