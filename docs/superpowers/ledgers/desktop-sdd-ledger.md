# SDD ledger — plan: docs/superpowers/plans/2026-11-09-klipt-desktop.md
Branch: feat/desktop. Session 2026-11-09 on the owner's Windows dual boot (RTX 3070 Ti, 2× 1920x1080: AOC 24G1WG4 primary, Samsung SMBX2335).

## Windows machine setup (done)
- winget: Rustup (rustc 1.99.1), Gyan.FFmpeg 9.0.2, **OpenJS.NodeJS.22** (the winget "LTS" id is now Node 26, which has no corepack), LLVM 23 (not on PATH; LIBCLANG_PATH set), GitHub CLI, VS 2022 Build Tools (VCTools).
- `corepack enable` fails with EPERM under Program Files → used `corepack enable --install-directory %APPDATA%\npm`.
- `scripts/fetch-obs-windows.ps1 -Dest C:\klipt-deps` → OBS 33.0.1 runtime + headers + SIMDe. User env: OBS_INCLUDE_DIR=C:\klipt-deps\include\obs, SIMDE_INCLUDE_DIR=C:\klipt-deps\include\simde.
- Repo-local git identity set (matches earlier commits). `gh` is installed but not authenticated (`gh auth login` is an owner step).

## Task 1: klipt-recorder — implemented, commit 8ac05fe
Rulings / deviations from the plan text:
- OBS 33 Windows zip has no `obs-plugins/64bit`: plugins are `core/<name>/<name>.dll`, data `core/<name>/data`. Task 2's artifact layout must mirror this.
- libobs loads graphics/plugins with LOAD_LIBRARY_SEARCH_DEFAULT_DIRS → `AddDllDirectory(bin/64bit)` (PATH/cwd not searched). cwd = bin/64bit (libobs data is `../../data/libobs` relative to cwd).
- libobs default log handler prints to **stdout** → custom handler to stderr (vsnprintf via legacy_stdio_definitions on MSVC).
- obs-nvenc (OBS 30.2+) reads `preset`, not `preset2`. **The Linux spike's preset comparison was invalid: NVENC ran at default p5 in every libobs run** (tune did apply).
- calldata strings are inline → `calldata_get_string`, not `calldata_get_data`.
- `obsconfig.h` is CMake-generated → stub in `recorder-obs/include-stub/` (last on the include path).
- Linux soname is `libobs.so.33` (SOVERSION = major), not `.so.30`; loader tries .33→.30→.0.
- Output size = display scaled to fit inside width×height keeping aspect (protocol box). The app (Task 6/12) should send a box matching the display aspect for ultrawides.
- Extra error codes beyond the plan: not_recording, already_recording, save_failed (60 s timeout; libobs emits no signal on mux failure), recording_stopped (then exit 2), obs_load_failed, bad_command. A leading UTF-8 BOM on stdin is tolerated (a .NET writer sent one).
- Extra CLI: `--list`, `--fps/--width/--height/--bitrate/--encoder/--mic-device`; selftest accepts `s`/`q` + Enter.
Evidence (Windows, release build in C:\klipt-deps\obs\bin\64bit):
- 30 unit + 5 CLI tests pass (CLI tests use KLIPT_TEST_OBS_ROOT=C:\klipt-deps\obs).
- selftest --mic: NVENC CBR 20000, keyint 60 (keyframes at exactly 1.000 s), p4, 0 B-frames; ffprobe h264 High 1920x1080@60 + 2× AAC stereo; durationMs 6017 vs file 6.06 s.
- protocol mode, mic off: double save → busy then saved (150 ms); start while recording → already_recording; stop → stopped, exit 0; 1 video + 1 audio track.
- x264 fallback: works, 49% of one core, 627 MB peak. Non-primary monitor (SMBX2335) captured via DXGI.
- Idle-desktop recorder CPU ≈ 16–17% of one core over 6–9 s runs (includes session start + one save); not yet measured in a game.
Not done:
- Independent review of 8ac05fe: done, see "Task 1 review" below.
- Linux code paths are compile-unverified (no Linux here): Step 5 on CachyOS must `cargo build` and run the printf check.
- C9 owner test in a fullscreen game (FPS off/on, Alt+F9 hotkey, recorder CPU) — see HANDOFF.md.

## Task 2: Windows CI build — implemented
- `scripts/package-recorder-windows.ps1`: OBS layout subset (bin/64bit minus Qt/UI/scripting/OpenGL/pdb; core/{win-capture,win-wasapi,obs-ffmpeg,obs-outputs,obs-x264,obs-nvenc,obs-qsv11} minus pdb and game-capture inject/hook helpers; data/libobs) + TEST-ON-WINDOWS.md. 57 MB. Verified locally: selftest from the packaged folder → NVENC, 1 video + 2 AAC; only log noise is win-capture's missing get-graphics-offsets (game capture, unused).
- `.github/workflows/windows-recorder.yml`: windows-latest, fetch → build → unit tests → package → `--list` smoke (non-blocking: runners have no GPU) → artifact `klipt-recorder-windows`. Actions pinned to current majors (checkout/upload-artifact v7, rust-cache v2).
- Ruling: `server-ci.yml` goes on `feat/server`, not here — the server code doesn't exist on `feat/desktop` (it branched from spike/capture), and the CI should run on the feat/server → main PR.

## Task 1 review (independent reviewer, 8ac05fe) → fix wave
Claims checked against libobs 33.0.1 source before fixing. All 9 addressed:
1. Critical — two saves in the same second overwrote each other (libobs names to the second with overwrite=true). Fix: `ClipNamer` pushes a fresh `format` via `obs_output_update` before each save; saves < 2 s apart get " (2)", " (3)"… Verified: burst of 3 saves 0.4 s apart → 3 files, two in the same second.
2. Important — stop/shutdown/EOF during a save lost the `saved` event. Fix: `Controller::end_session` waits for the pending save first. Verified: stop sent right after the 3rd save → all 3 `saved` before `stopped`.
3. Important — a failed mux (libobs signals `saved` only on success) left `busy` for 60 s. Fix: `MuxWatch` sees muxing end via `get_last_replay`; no `saved` within 1 s → `save_failed`. Verified with obs-ffmpeg-mux.exe removed: `save_failed` ~1 s after the save.
4. Important — `eprintln!` panics on a closed stderr, aborting inside the libobs log callback. Fix: `log!` macro ignoring write errors, used everywhere.
5. Minor — relative paths resolved after the chdir. Fix: `--obs-root` and `--out` made absolute while parsing; protocol `outputDir` resolved against the launch dir. Verified.
6. Minor — invalid UTF-8 on stdin looked like EOF. Fix: `read_until` + lossy decode.
7. Minor — release could hang on obs_output_destroy. Fix: wait for `obs_output_active() == false` after the stop signal; `arm_exit_watchdog` exits with the intended code after 10 s if cleanup hangs.
8. Minor (Linux) — obs-ffmpeg-mux inherits fd 1 and may print to it; helpers are looked up next to /proc/self/exe. Fix: events go to a private dup of fd 1 and fd 1 → stderr (Linux only; on Windows libobs gives helpers no stdout handle, checked in pipe-windows.c); missing-helper warning with the `ln -s` command.
9. Minor (Linux) — `improper_ctypes` on the tuple Rusage, dead code warning. Fixed.
Notes for Task 6 (supervisor): keep draining the recorder's stderr (a full pipe blocks libobs threads), and if `stopped` doesn't arrive within ~15 s of `stop`, kill the child by PID.
Still unverified: the Linux paths compile only by reading; Step 5 on CachyOS needs obs-ffmpeg-mux/obs-nvenc-test symlinked next to target/debug/klipt-recorder.

## C9 prep (2026-11-10): Alt+F9 is taken on NVIDIA PCs
- The owner's first C9 run got `RegisterHotKey` error 1409 for ALT+F9. A probe showed exactly NVIDIA App's default set taken: Alt+F9, Alt+F10, Alt+Shift+F10, Alt+F1, Alt+Z, Alt+R. Alt+F8, Alt+F11, Alt+F12, F8, F9, Ctrl+Alt+F9, Alt+Shift+F9 and Ctrl+Shift+F9 were free. NVIDIA's registry (`HKCU\Software\NVIDIA Corporation\Global\ShadowPlay\NVSPCAPS`) shows ShadowPlay enabled with a 300 s buffer at 60 fps.
- The C9 test now uses `--hotkey ALT+F8`, and TEST-ON-WINDOWS.md says to turn off other background recorders during the FPS test.
- For Task 4: the app's default hotkey is Alt+Backquote, the key under Esc, not Alt+F9 as the plan says. The owner chose it on 2026-11-10. It types ` on US and ; on Slovak, but it is VK_OEM_3 on both and free on the owner's PC; on KDE it showed as Alt+;. Store it by key position (W3C code `Backquote`, as Tauri does), and show the label the OS gives (GetKeyNameText; on Slovak it is ";"). Task 4/6 must also handle a taken shortcut: show "already used by another app" and offer Change, not just fail.
