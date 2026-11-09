# Klipt Recording Spike — Decision Record

Date: 2026-11-09
Status: Linux decided. Windows still to be confirmed with test builds sent to friends (C9).
Plan: `docs/superpowers/plans/2026-11-09-klipt-capture-spike.md`
Code (throwaway): `spikes/capture` (libobs recorder in Rust), `spikes/hotkey` (GlobalShortcuts portal)

## Decision

| Platform | Recording engine | Capture | Encoder | Hotkey |
|---|---|---|---|---|
| **Linux (CachyOS KDE Wayland)** | **gpu-screen-recorder 6.x** as a helper process | **Direct KMS capture (`-w <monitor>`)** by default, with the **portal** (`-w portal` + restore token) as fallback | NVENC / VAAPI, picked by gsr | **XDG GlobalShortcuts portal** |
| **Windows 10/11** | **libobs** as a helper process (still to be confirmed: C9) | DXGI / Windows Graphics Capture (`monitor_capture`) | NVENC (`obs_nvenc_h264_tex`) / AMF / x264 fallback | `RegisterHotKey` (still to be confirmed) |

The Tauri app talks to one **recorder interface** (start, save, stop, and events). Each platform has its own backend behind it. The UI does not know which engine is running.

### Why gpu-screen-recorder on Linux and not libobs

Measured in Active Matter, at a fixed spot, with FPS uncapped and the GPU at 99% (the worst case). The 3070 Ti drives a 144 Hz DP-1 display.

| Engine / mode | Avg FPS | 1% low | FPS cost | Recorder CPU (share of one core) |
|---|---|---|---|---|
| None (2 runs) | 140.6 / 140.3 | 100.2 / 127.7 | – | – |
| libobs, portal, 60 fps, NVENC p4/hq | 125.4 | 85.5 | **−11%** | 28% |
| libobs, portal, 60 fps, NVENC p1/ll | 126.0 | 112.0 | **−10%** | 29% |
| libobs, portal, 30 fps | 130.6 | 112.2 | −7% | – |
| gsr, portal, 60 fps | 133.6 | 119.9 | −5% | 5.1% |
| **gsr, direct KMS (DP-1), 60 fps** | **135.7** | **122.0** | **−3.4%** ✅ | 5.4% |

- The NVENC preset made no difference. The libobs cost comes from capturing and compositing the screen on the GPU, and from the portal delivering up to 144 frames per second on this monitor.
- gsr direct capture is the only setup that meets C5 (under 5%).

## Pass criteria

| # | Criterion | Result | Evidence |
|---|---|---|---|
| C1 | Display capture without a repeat prompt | ✅ | libobs: second run reused RestoreToken and started in 135 ms. gsr: `-restore-portal-session` saved its token. gsr KMS needs no portal at all. |
| C2 | GPU encode, low CPU | ✅ gsr (≈5% of one core) / ❌ libobs (28%) | NVENC confirmed. CPU measured with getrusage and /proc. |
| C3 | 1 video + 2 audio tracks, separate | ✅ | ffprobe: h264 + 2× AAC stereo (both engines). The owner listened to each track: system audio only and mic only. |
| C4 | Replay buffer, RAM | ✅ | libobs 30 s / 140 s and gsr 159 s clips saved correctly. RSS about 770–840 MB with a roughly 150 s buffer at 20 Mbps. |
| C4 | Replay buffer, Disk | ❌ (gsr) / not built (libobs) | gsr `-replay-storage disk` with `-r 20` grew to 240 MB in 100 s and saved all 99.7 s, so it never trims. See Open issues. |
| C5 | Under 5% FPS cost | ✅ gsr direct (−3.4%) / ❌ libobs (−7% to −11%) | MangoHud logs, 60 s each. |
| C6 | Global hotkey on Wayland, fullscreen game | ✅ | GlobalShortcuts portal: 16 of 16 presses received in fullscreen Active Matter. KDE bound the key as `Alt+;`, and rebinding happened instantly on a second run. |
| C7 | WebKitGTK playback and the per-track volume editor | ✅ | 1080p H.264 plays in WebKitGTK 2.54 / GStreamer 1.28. Muted `<video>` plus two `<audio>` elements: independent volume and sync confirmed by the owner, with 6 ms drift at the end. |
| C8 | Packaging | ⚠️ | gsr-kms-server needs `cap_sys_admin=ep`, which can't be granted from an AppImage or Flatpak. See Packaging. libobs bundling was not tested (not needed on Linux). |
| C9 | Windows | ⏳ | Needs test builds run by friends (RTX 40xx + AMD). |

## Linux backend details (gpu-screen-recorder)

- **Command** (the one tested):
  ```
  gpu-screen-recorder -w DP-1 -f 60 -k h264 -bm cbr -q 20000 -ac aac \
    -a default_output -a default_input -c mp4 -r <seconds> -replay-storage ram -o <clips dir>
  ```
  - Each `-a` becomes a separate audio track. Track 1 is system audio and track 2 is the mic.
  - If the mic is off, leave out the second `-a`.
  - Portal fallback: `-w portal -restore-portal-session yes -portal-session-token-filepath <app data>/portal-token`.
- **Control:**
  - **SIGUSR1** saves the replay. gsr prints the saved file path on stdout, which is the "saved" event.
  - **SIGINT** stops it.
  - The app keeps the child PID and only ever signals that PID.
- **Monitor list:** `gpu-screen-recorder --list-monitors` returns lines such as `DP-1|1920x1080`. Use it for the display picker.
- **Night Light:** gsr removes KDE's Night Light tint from the capture automatically.
- **License:** GPL-3.0. Klipt is open source, so this is fine.

## Packaging (Linux)

- Direct capture needs the system package's `gsr-kms-server` with `cap_sys_admin`. Arch, CachyOS and SteamOS ship gsr in their repos, and the package sets that capability.
- **Recommendation:** ship Klipt for Linux as an **Arch/AUR package that depends on `gpu-screen-recorder`**. Optionally also ship an AppImage that uses the system `gpu-screen-recorder` when it's present. Without it, the AppImage tells the user to install the package.
- If `gsr-kms-server` lacks the capability, fall back to `-w portal` (measured at −5%).

## Windows backend details (libobs) — what we know so far

- Proven on Linux with libobs 33: explicit module loading (`obs_open_module` + `obs_init_module`) and the replay-buffer output with multiple audio tracks.
- Helper executables must sit **next to the recorder executable**: `obs-ffmpeg-mux` (replay muxing) and `obs-nvenc-test` (NVENC detection). Both apply to Windows too (`.exe`).
- Encoder settings that worked: `rate_control=CBR`, `bitrate=20000`, `keyint_sec=1`, `preset2=p4`, `tune=hq`, `multipass=disabled`, `lookahead=false`, `bf=0`.
- Still to verify (C9): `monitor_capture` (DXGI/WGC), `wasapi_output_capture` / `wasapi_input_capture`, AMF on an AMD card, FPS cost, and `RegisterHotKey` in fullscreen-exclusive games. That needs friends' machines.

## Open issues

1. **Disk buffer mode on Linux.** gsr's disk mode doesn't trim, so it is unusable as is. Options: (a) offer RAM only on Linux at first; (b) Klipt runs its own rolling segments, e.g. restarting gsr recordings in short chunks and stitching them on save; (c) report it upstream. **Recommended: (a) for v1, (c) in parallel.**
2. **CPU at 5.1–5.4% of one core** sits right at the C2 threshold. Acceptable.
3. **Windows (C9)** is entirely unverified on real hardware.
4. **Editor sync:** pause the `<audio>` elements during a seek and resync after `seeked`, so the drift spikes seen in the test don't happen.
5. **Hotkey key name:** KDE reported `Alt+;` for the requested `ALT+F9`, probably because of the Slovak keyboard layout. The app should show the binding KDE reports, not the one it requested.
