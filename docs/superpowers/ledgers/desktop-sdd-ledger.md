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
- **Independent review of 8ac05fe** (started, stopped when the usage limit hit) — rerun before relying on it.
- Linux code paths are compile-unverified (no Linux here): Step 5 on CachyOS must `cargo build` and run the printf check.
- C9 owner test in a fullscreen game (FPS off/on, Alt+F9 hotkey, recorder CPU) — see HANDOFF.md.
