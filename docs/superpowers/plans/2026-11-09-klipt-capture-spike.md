# Klipt Recording Spike Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. This is a **spike**. The code is throwaway, not test-driven, and it lives in `spikes/`. What it produces is *evidence* and a decision record. Several steps need the owner at the machine (KDE permission prompts, playing a game, pressing hotkeys). Stop and ask them at those steps. Never fake a result. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Decide how Klipt records, and prove on the owner's real machine that it can. The decision record is what the desktop app plan gets written from.

**Architecture under test (leading candidate):** a separate **recorder sidecar process** (`klipt-recorder`) that embeds **libobs**. Tauri talks to it as JSON lines over stdin/stdout. libobs provides display capture (the KDE portal + PipeWire on Linux, DXGI on Windows), system and mic audio, and GPU encoders (NVENC/AMF/VAAPI/x264). Klipt keeps its **own replay buffer** fed by a custom encoded-packet output, so the same code path can buffer in RAM or on disk. Running it as a separate process means a capture crash can't take down the UI.

**Fallbacks**, only if libobs fails a criterion on a platform: Windows, FFmpeg `ddagrab` + hardware encoder with WASAPI loopback captured in Rust. Linux, the portal + PipeWire in Rust (`ashpd`, `pipewire` crates) feeding FFmpeg.

**Tech Stack:** Rust (install with rustup), `bindgen` over `/usr/include/obs` (system OBS 33 on CachyOS), `ashpd` (portals), ffmpeg/ffprobe CLI (muxing and checks), MangoHud (FPS measurement), Tauri v2 (WebKitGTK playback check).

**Spec:** `docs/superpowers/specs/2026-11-09-medal-alternative-design.md`

## Pass criteria (from the spec)

| # | Criterion | Pass if |
|---|-----------|---------|
| C1 | Display capture on CachyOS KDE Wayland + RTX 3070 Ti | The chosen display is captured at 1080p60 through the portal. A second run reuses the restore token **without a prompt**. |
| C2 | GPU encode on Linux | NVENC H.264 works. `nvidia-smi` shows the encoder in use and CPU stays under 5% of one core for capture+encode. |
| C3 | Audio | The saved clip has **1 video + 2 audio tracks** (track 1 system, track 2 mic). With the mic disabled, only 1 audio track. A/V stays in sync within 1 frame over 5 min. |
| C4 | Replay buffer | `save` writes the last N s (±1 s) without re-encoding, in under 2 s, and the file passes `ffprobe`. It works with N = 30 s and N = 300 s, in **RAM mode and in Disk mode**. Killing the process mid-save does not corrupt earlier clips. |
| C5 | Game impact | In a demanding game, average FPS with the buffer on is within **5%** of buffer off (MangoHud log, same scene, 60 s each). |
| C6 | Global hotkey on KDE Wayland | A shortcut registered through the **GlobalShortcuts portal** fires while a fullscreen game has focus. |
| C7 | Linux webview playback | In a Tauri window on WebKitGTK, the saved H.264 clip plays and seeks, and two separately-volumed audio sources stay in sync with a muted `<video>`. |
| C8 | Bundling | The recorder runs against a **copied** libobs + plugins + data directory (not the system install). This proves it can ship inside an AppImage or Flatpak. |
| C9 | Windows | The same recorder builds for Windows, and on an NVIDIA RTX 40xx and an AMD machine it passes C2–C5 equivalents (DXGI capture, WASAPI loopback + mic, NVENC / AMF). |

---

### Task 1: Toolchain and spike workspace

**Files:** `spikes/capture/Cargo.toml`, `spikes/capture/build.rs` (bindgen over `obs.h`, links `obs`), `spikes/capture/src/main.rs`

- [ ] Install Rust with rustup (stable). Check that `cargo --version` works.
- [ ] Generate libobs bindings with bindgen, allow-listing `obs_.*`, `gs_.*`, `video_.*`, `audio_.*`, and `calldata_.*`.
- [ ] A smoke binary calls `obs_startup`, `obs_reset_video` (1920×1080@60, NV12), and `obs_reset_audio` (48 kHz stereo), then loads all modules (`obs_add_module_path` + `obs_load_all_modules`) and prints the source and encoder ids it found. **Check:** the list includes the PipeWire screen-capture source, the Pulse input and output captures, and an NVENC H.264 encoder id. Record the exact ids in the decision record.
- [ ] Commit with `spike: libobs bindings and module discovery`.

### Task 2: Capture + encode + RAM replay buffer (C1–C4 RAM)

**Files:** `spikes/capture/src/{recorder.rs, buffer.rs, protocol.rs}`

- [ ] Wire up the scene: the display source → video. Pulse output → mixer track 1, mic → mixer track 2.
- [ ] Register a custom libobs output (`OBS_OUTPUT_ENCODED | OBS_OUTPUT_MULTI_TRACK`) whose `encoded_packet` callback pushes packets into a `RingBuffer` that keeps whole GOPs. Use a 1 s keyframe interval and trim on keyframe boundaries to `max_seconds`.
- [ ] `save <path>`: snapshot the packets from the first keyframe ≥ (now − N), then write MKV *and* MP4 through `ffmpeg -f` with piped elementary streams, or with libobs's own muxer if that turns out simpler. Record which approach was used. Copy only, no re-encode.
- [ ] Protocol over stdin/stdout, one JSON per line: `{"cmd":"start", ...settings}`, `{"cmd":"save","path":...}`, `{"cmd":"stop"}`, then `{"event":"saved","path":...,"durationMs":...}` and `{"event":"error","message":...}`. Record the final shape in the decision record.
- [ ] **Owner step:** first run, approve the KDE prompt. Second run: no prompt (C1). Store the restore token in a file.
- [ ] Check C3/C4 with `ffprobe -show_streams clip.mp4`: 3 streams, duration ≈ N. Run with N=30 and N=300, and with the mic off. Measure save time. `kill -9` during a save, then check that the earlier clips still probe cleanly.
- [ ] Commit with `spike: replay buffer in RAM`.

### Task 3: Disk buffer mode (C4 Disk)

- [ ] Add `buffer: "disk", dir` to the protocol. Packets are appended to rolling segment files of about 10 s each, starting on a keyframe, with an index of `(pts, offset)`. Segments older than N + 10 s are deleted. On start and exit the directory is wiped.
- [ ] Check: N=300 keeps disk use bounded (`du -sh` stays steady over 10 minutes), `save` output matches what RAM mode produces, and RSS stays under 150 MB.
- [ ] Commit with `spike: disk replay buffer`.

### Task 4: Game impact (C2, C5) — owner step

- [ ] The owner chooses a demanding game and a repeatable 60 s scene. MangoHud logs with the recorder stopped and then running (1080p60 NVENC, preset P4 or similar, 20 Mbps CBR). Run each twice.
- [ ] Record average FPS and 1% lows, `nvidia-smi dmon` encoder %, and recorder CPU% from `pidstat`. Check them against C2/C5.
- [ ] If C5 fails, try the NVENC preset, lower bitrate, and 30 fps, and record which settings pass.

### Task 5: Global hotkey via portal (C6) — owner step

**Files:** `spikes/hotkey/` (Rust, `ashpd` GlobalShortcuts)

- [ ] Bind a `save-clip` shortcut with preferred trigger `ALT+F9`. KDE shows its binding dialog. Print a line on each `Activated` signal.
- [ ] The owner presses it in a fullscreen game → the line prints (C6). Restart the binary: the binding persists and no dialog appears.
- [ ] Note what happens in the Windows equivalent: `RegisterHotKey` vs a low-level keyboard hook, tested in Task 7.

### Task 6: WebKitGTK playback + bundling (C7, C8)

- [ ] A minimal Tauri v2 app (in `spikes/webview/`) loads a clip from Task 2 through Tauri's asset protocol: a muted `<video>` plus two `<audio>` elements fed by `ffmpeg -map 0:a:0 -c copy game.m4a` / `-map 0:a:1 mic.m4a`. Sync on `play`/`pause`/`seeked` and correct drift above 50 ms. Volume sliders on each. **Check C7** on the owner's machine. Also check that an AppImage build of it plays the video (GStreamer libav plugin bundled). Record the result.
- [ ] C8: copy `libobs.so*`, `libobs-opengl.so*`, the required plugin `.so` files, and `/usr/share/obs/libobs` + plugin data into `spikes/bundle/`. Run the recorder with `LD_LIBRARY_PATH` and the module paths pointing there, and with the system OBS hidden (`env -i` plus an explicit path). It works → pass.

### Task 7: Windows (C9) — needs a Windows machine

- [ ] Cross-check the build on a GitHub Actions `windows-latest` runner: bindgen against the OBS 33 release SDK headers, linking `obs.lib`, with the OBS runtime DLLs/plugins taken from the official release zip.
- [ ] **Owner step:** ask whether a friend with an RTX 40xx and a friend with an AMD card can run a test zip. Each runs the recorder with the same checks as Task 2 (`monitor_capture` / WGC, WASAPI output + input, NVENC / AMF), plus a 60 s FPS comparison in a game they own, and sends back `ffprobe` output and the numbers.
- [ ] Hotkey: `RegisterHotKey`, tested while a fullscreen exclusive game has focus. If it fails, try a `WH_KEYBOARD_LL` hook. Record which one works.
- [ ] Without a GPU (a VM), the x264 fallback must still work.

### Task 8: Decision record

**Files:** `docs/superpowers/specs/2026-11-09-klipt-capture-decision.md`

- [ ] Write the decision record:
  - the C1–C9 table with pass/fail and the evidence (numbers, ffprobe excerpts)
  - the chosen engine per platform
  - the exact libobs source and encoder ids
  - the final sidecar protocol
  - the clip container (MP4 or MKV) and why
  - recommended default encoder settings per vendor
  - the hotkey mechanism per platform
  - how libobs gets bundled per platform (AppImage or Flatpak, and the Windows file layout)
  - open issues
- [ ] Show it to the owner. The desktop app plan (`docs/superpowers/plans/…-klipt-desktop.md`) is written from it.
