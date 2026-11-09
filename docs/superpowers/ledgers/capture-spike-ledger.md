# SDD ledger — plan: docs/superpowers/plans/2026-11-09-klipt-capture-spike.md
Spike (throwaway, evidence-driven; TDD not applicable per plan header). Branch spike/capture in main checkout.
Pre-flight: tasks share spikes/capture recorder (T2→T3,T4,T6,T7) — consistent.
Task 1: complete — libobs 33 via bindgen (needs SIMDe headers, third_party/). Modules must be loaded explicitly with obs_open_module+obs_init_module (obs_load_all_modules2/obs_load_core_modules find nothing in OBS 33 outside the obs binary layout). Ids: pipewire-screen-capture-source, pulse_output_capture, pulse_input_capture, obs_nvenc_h264_tex, ffmpeg_aac, replay_buffer. NVENC needs helper `obs-nvenc-test` next to the executable.
Task 2 (partial): libobs built-in replay_buffer works. Needs helper `obs-ffmpeg-mux` next to the executable. Portal prompt approved once, RestoreToken saved. Clip: h264 1920x1080@60 + 2x AAC stereo, 29.9s, 75MB @20Mbps. Track1 mean -38.3dB (system), track2 -33.9dB (mic). PipeWire logs "buffer contains corrupted data" (debug) — frame visually clean.
Task 2: C1 PASS — 2nd run with saved RestoreToken started in 135ms, no prompt. C3 PASS — owner confirmed track1 = system only, track2 = mic only.
Task 4 (C5) run1 OFF: Active Matter 60s avg 140.6 fps, 1% low 100.2, GPU 99%. run2 ON (p4/hq, no lookahead, 20Mbps CBR, 1080p60, mic): avg 125.4, 1% low 85.5 → −10.8% avg (FAIL vs 5%, pending noise check)
Bench (desktop idle, release): NVENC util p4/hq ~60%, p1/hq ~60%, p1/ll 27%; recorder CPU 14–17% of one core incl. startup; RSS ~450MB max (CUDA/GL mapped libs), anon ~120MB at start
C5 runs (Active Matter, same spot, uncapped, GPU 99%): off 140.6/100.2, off 140.3/127.7; libobs p4/hq 60fps 125.4/85.5 (−11%); p1/ll 60fps 126.0/112.0 (−10%); p1/ll 30fps 130.6/112.2 (−7%). Preset irrelevant → cost is capture/compose GPU work. Monitor DP-1 is 144Hz (PipeWire delivers up to 144 fps). Recorder CPU while gaming 28–29% of one core (C2 FAIL). C5 FAIL for libobs.
C4 RAM: 140s save = 353MB, works. RSS 766MB with 140s buffer.
Next: compare gpu-screen-recorder 6.1.7 (extra repo) replay mode on same scene.
C5 gsr 6.1.7 portal 60fps h264 cbr20M aac 2 tracks replay 300s RAM: avg 133.6 / 1% low 119.9 (−5%). Portal prompt once, restore token saved by gsr itself.
gsr stats: CPU 5.1% of one core avg over 180s (libobs 28–29%), max RSS 844MB (~160s RAM buffer). Saved clip: h264 1920x1080 + 2x AAC stereo, 159s, 402MB. → Linux engine candidate: gpu-screen-recorder sidecar.
C5 gsr direct KMS (-w DP-1, gsr-kms-server has caps → no password prompt; auto-removes Night Light tint): avg 135.7 / 1% low 122.0 (−3.4%) → C5 PASS. NVENC util 14%.
gsr direct: CPU 5.4% of one core, VmHWM 827MB; saved clip ok
C6 PASS: GlobalShortcuts portal bound 'save-clip' (KDE assigned Alt+;), 16/16 activations received while Active Matter fullscreen; rerun re-bound instantly (dialog persistence: asking owner).
C4 disk (gsr -replay-storage disk, -r 20): after 40s disk grew linearly to 96MB, saved clip 39.7s (not 20s) → disk buffer not trimmed within 40s; needs longer run
C4 disk gsr 100s run with -r 20: disk grows linearly to 240MB, saved clip 99.7s → gsr disk mode does NOT trim (unbounded). RAM mode trims correctly (earlier: -r 300 saved 140–159s as expected). FAIL for gsr disk mode as-is.
C7 PASS: WebKitGTK 2.54 MiniBrowser plays 1080p H.264 clip (GStreamer 1.28, gst-libav), muted <video> + 2 <audio> with independent volume confirmed by owner, end drift 6ms; max-drift metric polluted by seek (editor must pause audio during seek). AppImage-bundled GStreamer not tested.
C8 gsr: binaries gpu-screen-recorder + gsr-kms-server (needs cap_sys_admin=ep — cannot be granted from AppImage/Flatpak user install) ; libs ffmpeg 8 (avcodec 63), pipewire, pulse, libva, wayland, dbus. gsr prints saved replay path on stdout.
