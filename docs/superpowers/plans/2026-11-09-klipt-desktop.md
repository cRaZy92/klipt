# Klipt Desktop App Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The Klipt desktop app for Windows and Linux. It runs a background replay buffer, saves clips on a hotkey, tags each clip with the game, keeps a local library, has a trim and per-track volume editor, uploads to the Klipt server, and copies the share link.

**Architecture:**
- Tauri v2 app with a Svelte 5 + TypeScript UI. All logic lives in the Rust core: settings, library database, game detection, ffmpeg export, HTTP/tus upload, tray, hotkeys.
- Recording runs in a **separate helper process** behind one `Recorder` interface:
  - **Linux:** `gpu-screen-recorder`, using direct KMS capture with a portal fallback.
  - **Windows:** `klipt-recorder`, our own Rust binary that loads **libobs** at runtime and talks to the app with JSON lines over stdin/stdout.
- **Tasks 1–3 come first** so the owner can test Windows recording on their dual boot before the UI exists.

**Tech Stack:**
- Rust (stable) with these crates: tauri 2, tauri-plugin-{global-shortcut, notification, autostart, dialog, opener, updater}, rusqlite (bundled), reqwest (rustls), serde/serde_json, tokio, ashpd (Linux portals), nix (Linux signals), windows (Win32), libloading + bindgen (libobs), rodio (save sound), wiremock (tests).
- Frontend: Svelte 5, Vite, vitest.
- Media tools: ffmpeg/ffprobe. The system copy on Linux, bundled on Windows.
- Build: GitHub Actions on Windows for the Windows build.

**Spec:** `docs/superpowers/specs/2026-11-09-medal-alternative-design.md`
**Recording decision (binding):** `docs/superpowers/specs/2026-11-09-klipt-capture-decision.md`
**Server API (already built, branch `feat/server`):** `packages/shared/src/api.ts`, `server/src/**/routes.ts`

## Global Constraints

- **Platforms:** Windows 10/11 x64, and CachyOS KDE Plasma Wayland. No macOS, no SteamOS in v1.
- **Recording engines (decision record):**
  - Linux uses `gpu-screen-recorder` with `-w <monitor>` (KMS) and falls back to `-w portal -restore-portal-session yes`.
  - Windows uses libobs 33 through `klipt-recorder`.
  - The app only ever signals or kills **child PIDs it spawned itself**.
- **Clip file:** MP4 with H.264 video, **track 1 = system audio**, **track 2 = mic** (present only when the mic is on), AAC audio.
- **Defaults:** 1080p, 60 fps, 20 000 kbps CBR, keyframe every 1 s, 30 s buffer (adjustable 10–600 s), mic off, recording mode **Always**, output folder `<Videos>/Klipt`.
- **Buffer location setting (RAM/Disk):** v1 supports **RAM only**. The setting is shown with Disk disabled and a tooltip "Not supported yet" (decision record, open issue 1).
- **Upload output:** H.264 copied from the source, one AAC track mixed from game and mic at the chosen volumes, `+faststart` MP4. A trim start snaps to the keyframe at or before the requested time, at most 1 s earlier.
- **Uploads to the server:**
  - tus 1.0 at `{server}/api/uploads/` with an `Authorization: Bearer <token>` header, in 8 MiB chunks, resumed with HEAD.
  - Then `POST /api/clips {uploadId, title, gameName|null, steamAppId|null, visibility}`. Never send `""` for gameName; send `null`.
  - A repeat finalize returns the existing clip with 200. A 409 `upload_finalizing` means retry after 3 s.
- **Login:** `POST /api/auth/login {username, password, client:'desktop', deviceName}` returns `{token, me}`. The token is stored in `<app data>/auth.json` with file mode 0600 on Linux.
- **Never** show raw error codes to users. Map them to friendly messages.
- Every user-visible string is in English. The UI is dark, modern and obvious to non-technical users.

## Review Focus

1. **Hotkey pressed before the buffer is full, or twice quickly:** the clip is saved with whatever is in the buffer, and a second press while a save is still running is ignored with a toast "Already saving…". It must not create a corrupt file or crash the recorder. Test in Task 1 (protocol) and Task 7.
2. **Recorder helper crashes or exits unexpectedly** (driver reset, display unplugged): the app notices within 2 s, shows "Recording stopped — restarting…", restarts it up to 3 times with backoff, then shows a persistent error in the tray. Test in Task 6.
3. **Server unreachable or token revoked during upload:** a network error pauses the upload and resumes from the HEAD offset on retry. A 401 sends the user to login without losing the clip or the edit. Test in Task 10.
4. **Clip file missing on disk** (deleted in Explorer or Dolphin): the library shows the row as "File missing", offers "Remove from library", and never crashes on thumbnail or export. Test in Task 4.
5. **Clip has no mic track** (mic off): the editor hides the Mic slider and export still mixes correctly from one track. Test in Task 9.

---

## File Structure

```
pnpm-workspace.yaml                      # add: desktop
desktop/package.json, vite.config.ts, svelte.config.js, tsconfig.json, index.html
desktop/src/main.ts, App.svelte
desktop/src/lib/api.ts                   # typed wrappers over Tauri invoke/events
desktop/src/lib/format.ts                # durations, dates, sizes (+ format.test.ts)
desktop/src/views/{Login,Library,Editor,Settings}.svelte
desktop/src/components/{ClipCard,GameFilter,UploadDialog,Toast,TrimBar}.svelte
desktop/src-tauri/Cargo.toml, build.rs, tauri.conf.json, capabilities/default.json
desktop/src-tauri/src/main.rs, lib.rs, state.rs, commands.rs, tray.rs, sound.rs, errors.rs
desktop/src-tauri/src/settings.rs
desktop/src-tauri/src/library/{mod.rs, db.rs}
desktop/src-tauri/src/ffmpeg.rs          # locate binaries, probe, thumbnail, demux audio
desktop/src-tauri/src/export.rs          # trim+mix command builder + runner
desktop/src-tauri/src/games/{mod.rs, steam.rs, known.rs, known_games.json, mappings.rs, proc_linux.rs, proc_windows.rs}
desktop/src-tauri/src/recorder/{mod.rs, gsr.rs, obs.rs, supervisor.rs}
desktop/src-tauri/src/hotkey/{mod.rs, portal.rs, windows.rs}
desktop/src-tauri/src/upload/{mod.rs, api.rs, tus.rs}
desktop/recorder-obs/Cargo.toml, build.rs, wrapper.h
desktop/recorder-obs/src/{main.rs, obs.rs, session.rs, protocol.rs, selftest.rs, hotkey_win.rs}
.github/workflows/server-ci.yml, windows-recorder.yml, desktop-windows.yml
scripts/fetch-obs-windows.ps1            # downloads OBS runtime + headers for local Windows builds
docs/WINDOWS-DEV.md, docs/HANDOFF.md
packaging/aur/PKGBUILD
```

---

### Task 1: `klipt-recorder`, a libobs helper (Windows first, also runs on Linux)

**Files:**
- Create: `desktop/recorder-obs/{Cargo.toml, build.rs, wrapper.h}` and `desktop/recorder-obs/src/{main.rs, obs.rs, session.rs, protocol.rs, selftest.rs, hotkey_win.rs}`.
- Port the proven parts of `spikes/capture`: module loading, the replay buffer, and the encoder settings.
- Test: `desktop/recorder-obs/src/protocol.rs` (unit tests) and `desktop/recorder-obs/tests/protocol_cli.rs`.

**Interfaces:**
- **Loading libobs:**
  - Load libobs at runtime with `libloading`, using bindgen's `dynamic_library_name("Obs")`. No import library is needed: `obs.dll` on Windows, `libobs.so.33` on Linux (the soname follows OBS's major version).
  - Headers come from the env var `OBS_INCLUDE_DIR` (Windows CI/local: the obs-studio source `libobs/` at tag `33.0.1` plus SIMDe), defaulting to `/usr/include/obs` plus `third_party/simde` on Linux.
- **Runtime layout:**
  - Windows: `klipt-recorder.exe` sits in OBS's `bin/64bit/` next to `obs.dll`, `obs-ffmpeg-mux.exe`, `obs-nvenc-test.exe` and `obs-amf-test.exe`. Plugins are found at `../../core/<name>/<name>.dll` with data at `../../core/<name>/data/` (OBS 33's layout), and libobs data at `../../data/libobs/` relative to the working directory.
  - Linux: the system layout from the spike (`/usr/lib/obs-modules/core`, `/usr/share/obs/obs-modules/core`). Both roots can be overridden with `--obs-root`.
- **Modules loaded explicitly:**
  - Windows: `win-capture, win-wasapi, obs-ffmpeg, obs-nvenc, obs-x264, obs-outputs`, plus `obs-qsv11` if present.
  - Linux: `linux-pipewire, linux-pulseaudio, obs-ffmpeg, obs-nvenc, obs-x264, obs-outputs`.
- **Graphics:** `libobs-d3d11` on Windows, `libobs-opengl` with the Wayland display on Linux.
- **Sources:**
  - Windows: `monitor_capture` (settings `monitor_id` = the chosen display id, `method` = 0 for auto), `wasapi_output_capture` and `wasapi_input_capture` with `device_id` = `"default"` or the chosen device.
  - Linux: `pipewire-screen-capture-source` with RestoreToken, `pulse_output_capture` and `pulse_input_capture`.
  - System audio goes to mixer bit 0 (track 1) and the mic to mixer bit 1 (track 2).
- **Encoder choice when `encoder:"auto"`:** the first one that creates **and** starts from `obs_nvenc_h264_tex`, `h264_texture_amf`, `obs_qsv11_v2`, `obs_x264`. Settings: `rate_control=CBR, bitrate, keyint_sec=1, preset=p4, tune=hq, multipass=disabled, lookahead=false, bf=0, profile=high` (OBS 30.2+'s NVENC reads `preset`; `preset2` is ignored) (for AMF and x264, map them to their keys: `rate_control=CBR`, `bitrate`, `keyint_sec=1`).
- **Output:** `replay_buffer` with `max_time_sec`, `max_size_mb=8192`, `directory`, `format="Klipt %CCYY-%MM-%DD %hh-%mm-%ss"` and `extension=mp4`.
- **Protocol (JSON Lines).** stdin carries commands, stdout carries events, stderr carries libobs logs.
  - Commands:
    - `{"cmd":"listDisplays"}` → `{"event":"displays","items":[{"id":String,"name":String,"width":u32,"height":u32,"primary":bool}]}`
    - `{"cmd":"listAudioDevices"}` → `{"event":"audioDevices","outputs":[{"id","name"}],"inputs":[{"id","name"}]}`
    - `{"cmd":"start","displayId":String|null,"fps":u32,"width":u32,"height":u32,"bitrateKbps":u32,"bufferSeconds":u32,"mic":bool,"micDeviceId":String|null,"outputDir":String,"encoder":"auto"|String}` → `{"event":"started","encoder":String,"width":u32,"height":u32}` or `{"event":"error","code":"start_failed"|"no_encoder"|"no_display","message":String}`
    - `{"cmd":"save"}` → `{"event":"saved","path":String,"durationMs":u64}`. If a save is already running: `{"event":"error","code":"busy","message":"Already saving"}`.
    - `{"cmd":"stop"}` → `{"event":"stopped"}`, then the process exits 0.
    - `{"cmd":"shutdown"}` → the process exits 0.
  - On startup it emits `{"event":"ready","version":String,"platform":"windows"|"linux","encoders":[String]}`.
  - On EOF of stdin it behaves like `shutdown`, so an orphaned helper exits when the app dies.
- **Selftest CLI**, for the owner's Windows test with no app:
  ```
  klipt-recorder --selftest [--seconds 30] [--display <id>] [--mic] [--out <dir>] [--exit-after N] [--save-at N] [--hotkey ALT+F9]
  ```
  - It starts recording and prints events plus a final `{"event":"stats","cpuSeconds","wallSeconds","avgCpuPercentOfOneCore","maxRssMb"}`.
  - On Windows, `--hotkey` registers a `RegisterHotKey` hotkey that triggers `save` (in `hotkey_win.rs`).

- [ ] **Step 1: Write failing tests** for `protocol.rs`. `parse_command(line: &str) -> Result<Command, ProtocolError>` round-trips each command above. Unknown `cmd` → `ProtocolError::UnknownCommand`. Malformed JSON → `ProtocolError::BadJson`. `Event` serializes to exactly the JSON shapes above (snapshot-style assertions on the strings).
- [ ] **Step 2:** Run `cargo test -p klipt-recorder protocol`. Expected: FAIL (missing module).
- [ ] **Step 3:** Implement `protocol.rs`, `obs.rs` (dynamic bindings, module loading per OS), `session.rs` (scene, sources, encoders, replay buffer, save/busy state machine) and `main.rs` (stdin thread, GLib iteration on Linux and a message loop on Windows, an event writer that flushes per line).
- [ ] **Step 4:** Run `cargo test -p klipt-recorder`. Expected: PASS.
- [ ] **Step 5: Linux integration check (manual, owner present).** First `cargo build -p klipt-recorder`, then symlink libobs's helpers next to the binary (libobs looks for them beside `/proc/self/exe`): `ln -s /usr/bin/obs-ffmpeg-mux /usr/bin/obs-nvenc-test desktop/target/debug/`.
  ```
  printf '{"cmd":"listDisplays"}\n{"cmd":"start","displayId":null,"fps":60,"width":1920,"height":1080,"bitrateKbps":20000,"bufferSeconds":30,"mic":true,"micDeviceId":null,"outputDir":"/tmp/klipt-test","encoder":"auto"}\n' | cargo run -p klipt-recorder
  ```
  then send `{"cmd":"save"}` twice quickly.
  - Expected: `ready` → `displays` → `started` (encoder `obs_nvenc_h264_tex`) → `saved` → `error busy`, or two `saved` events if the first had already finished.
  - `ffprobe` on the saved file shows 1 video stream and 2 audio streams.
- [ ] **Step 6:** Commit with `feat(recorder): libobs helper process with JSON-lines protocol and selftest`.

### Task 2: Windows CI build of `klipt-recorder` with the OBS runtime

**Files:**
- Create: `scripts/fetch-obs-windows.ps1`
- Create: `.github/workflows/windows-recorder.yml`
- Create: `.github/workflows/server-ci.yml`, which runs `pnpm --filter @klipt/server test` on ubuntu-latest with ffmpeg installed.

**Interfaces:**
- `fetch-obs-windows.ps1 -Version 33.0.1 -Dest <dir>`:
  - Downloads the OBS Windows x64 release zip. The asset name is resolved through `https://api.github.com/repos/obsproject/obs-studio/releases/tags/<ver>` by matching `Windows.*x64.*\.zip`.
  - Extracts to `<dest>/obs`.
  - Shallow-clones `obsproject/obs-studio` at the tag, keeping `libobs/` headers only, into `<dest>/include/obs`.
  - Clones SIMDe `v0.8.2` into `<dest>/include/simde`.
  - Prints the `OBS_INCLUDE_DIR` and OBS runtime path to use.
- `windows-recorder.yml`:
  - Runs on `windows-latest`, on push to `feat/desktop` and on workflow_dispatch.
  - Steps: Rust stable → LLVM (for bindgen; it's preinstalled on the runner, so set `LIBCLANG_PATH`) → run the fetch script → `cargo build -p klipt-recorder --release`.
  - Assembles an artifact `klipt-recorder-windows`, a zip that mirrors the OBS layout: `bin/64bit/` (obs.dll and its dependency DLLs, `klipt-recorder.exe`, the helper exes), `core/<module>/` (only the modules listed in Task 1, with their `data/`), and `data/libobs/`, plus `TEST-ON-WINDOWS.md`. Assembled by `scripts/package-recorder-windows.ps1`.
- `TEST-ON-WINDOWS.md`: step-by-step for the owner.
  - Run `bin\64bit\klipt-recorder.exe --selftest --mic --hotkey ALT+F9 --exit-after 180 --out %USERPROFILE%\Videos\Klipt-test` from a terminal.
  - Play a game, press Alt+F9, and check the clip with the bundled instructions.
  - FPS test with the in-game FPS counter or CapFrameX: run 60 s off, then 60 s on.

- [ ] **Step 1:** Write the script and the workflow.
- [ ] **Step 2:** Push the branch (the owner approved pushing feature branches). The workflow run must go green and the artifact must exist and contain `bin/64bit/klipt-recorder.exe` and `obs.dll`. If it fails, fix it and push again. Never force-push `main`.
- [ ] **Step 3:** Commit with `ci: windows build of klipt-recorder with OBS runtime; server CI`.

### Task 3: Handoff and Windows dev setup docs

**Files:** create `docs/WINDOWS-DEV.md` and `docs/HANDOFF.md`.

- `WINDOWS-DEV.md`, steps for an exact local setup:
  - Install Git, Node 22 LTS with corepack (pnpm), Rust stable with `rustup` (x86_64-pc-windows-msvc), Visual Studio 2022 Build Tools ("Desktop development with C++"), LLVM (for bindgen) with `LIBCLANG_PATH`, FFmpeg (gyan.dev full build) on PATH, and WebView2 (preinstalled on Windows 11).
  - Run `scripts/fetch-obs-windows.ps1`.
  - Build and run `klipt-recorder --selftest`.
  - Note: clone from GitHub, don't copy the Linux folder.
- `HANDOFF.md`, for a fresh Claude Code session on Windows:
  - Project status and branch map (`main`, `feat/server`, `spike/capture`, `feat/desktop`).
  - Where the spec, decision record and plans are.
  - What was verified on Linux, with the numbers.
  - What to do next: Task 1 Step 5 on Windows, record the results in the decision record's C9 row, then continue at Task 4.
  - The owner's preferences: safe commands only, stop only your own PIDs, no pattern `pkill`/`taskkill /IM`.

- [ ] **Step 1:** Write both docs.
- [ ] **Step 2:** Commit and push with `docs: Windows dev setup and session handoff`.

### Task 4: Desktop app scaffold, settings, library database

**Files:**
- `desktop/` scaffold: Tauri 2 + Svelte 5 + Vite. Add `desktop` to `pnpm-workspace.yaml`. Make `desktop/src-tauri` and `desktop/recorder-obs` members of one Cargo workspace at `desktop/Cargo.toml`.
- Create `desktop/src-tauri/src/{settings.rs, library/mod.rs, library/db.rs, ffmpeg.rs, errors.rs, state.rs}`.

**Interfaces:**
- **Settings** (serde camelCase, defaults from Global Constraints): `recordingMode: Always|WhileGameRunning`, `bufferLocation: Ram|Disk`, `displayId: Option<String>`, `resolution: P720|P1080|P1440|Native`, `fps: u32`, `quality: Low|Medium|High|Custom`, `customBitrateKbps: u32`, `bufferSeconds: u32`, `micEnabled: bool`, `micDeviceId: Option<String>`, `hotkey: String` (default `"Alt+Backquote"`, the key under Esc; NVIDIA App holds Alt+F9 — owner decision 2026-11-10), `outputDir: PathBuf`, `startWithOs: bool` (default true), `saveSound: bool`, `saveNotification: bool`, `serverUrl: Option<String>`.
  - Quality presets map to kbps: Low 8000, Medium 14000, High 20000.
  - Functions: `Settings::load(dir) -> Settings` (a missing or corrupt file gives defaults, and the corrupt file is backed up as `.bak`), `save(&self, dir) -> Result<()>` (writes atomically via temp + rename), `validate(&self) -> Vec<FieldError>` (buffer 10–600, fps ∈ {30, 60}, bitrate 2000–80000).
- **Library DB** (SQLite at `<app data>/library.db`):
  ```
  clips(id TEXT PK, path TEXT UNIQUE, created_at INTEGER, duration_ms INTEGER, width INTEGER, height INTEGER,
        has_mic INTEGER, game_name TEXT NULL, steam_app_id INTEGER NULL, exe TEXT NULL, title TEXT,
        trim_start_ms INTEGER NULL, trim_end_ms INTEGER NULL, game_volume REAL DEFAULT 1, mic_volume REAL DEFAULT 1,
        server_clip_id TEXT NULL, share_url TEXT NULL, visibility TEXT NULL, upload_id TEXT NULL, upload_offset INTEGER NULL)
  ```
  - Functions: `Library::open(path)`, `insert(NewClip) -> Clip`, `list(filter: { game: Option<String>, sort: NewestFirst|OldestFirst }) -> Vec<ClipView>` (`ClipView` adds `file_missing: bool` and `thumb_path`), `games() -> Vec<(String, Option<u32>, u32)>`, `update_edit(id, trim, vols)`, `set_game(id, name, steam_app_id)`, `rename(id, title)`, `mark_uploaded(id, server_clip_id, share_url, visibility)`, `remove(id, delete_file: bool)`.
- **ffmpeg** (`ffmpeg.rs`):
  - `locate() -> Tools {ffmpeg, ffprobe}`: Windows uses the bundled sidecar next to the exe, then PATH. Linux uses PATH.
  - `probe(path) -> Probe {duration_ms, width, height, audio_tracks}`
  - `thumbnail(path, out, at_ms)`
  - `demux_audio(path, track_index, out)`, which writes a `.m4a` with `-c copy`.

- [ ] **Step 1: Write failing tests.**
  - Settings: defaults match the constraints; a corrupt file gives defaults plus a `.bak`; `validate` rejects buffer 5 and fps 45.
  - Library: insert, list sort and filter, games counts.
  - **Review Focus 4:** a clip whose file was deleted lists with `file_missing=true`, and `remove(id, false)` works.
  - ffmpeg: on a generated 2-audio-track fixture, `probe` reports `audio_tracks=2`; `thumbnail` writes a JPEG; `demux_audio(…,1,…)` writes a playable m4a.
- [ ] **Step 2:** Run `cargo test -p klipt-desktop`. Expected: FAIL.
- [ ] **Step 3:** Implement. `pnpm --filter klipt-desktop tauri dev` opens an empty window with "Klipt".
- [ ] **Step 4:** Run `cargo test -p klipt-desktop`. Expected: PASS.
- [ ] **Step 5:** Commit with `feat(desktop): scaffold, settings, library db, ffmpeg tools`.

### Task 5: Game detection

**Files:** create `desktop/src-tauri/src/games/{mod.rs, steam.rs, known.rs, known_games.json, mappings.rs, proc_linux.rs, proc_windows.rs}`, with test fixtures under `desktop/src-tauri/tests/fixtures/steam/`.

**Interfaces:**
- `detect_now() -> Detected { game_name: Option<String>, steam_app_id: Option<u32>, exe: Option<String> }`, checked in this order:
  1. Linux: scan `/proc/*/environ` for `SteamAppId` or `SteamGameId` (non-zero), skipping our own process tree.
  2. Windows: foreground window process (`GetForegroundWindow` → `GetWindowThreadProcessId` → `QueryFullProcessImageNameW`). If the path contains `\steamapps\common\<dir>\`, look `<dir>` up in the appmanifests.
  3. `known_games.json` ({exe_lowercase: {name, steamAppId?}}), for example `valorant-win64-shipping.exe`, `overwatch.exe`, `league of legends.exe`, `wow.exe`, `diablo iv.exe`, `cod.exe`, `hearthstone.exe`, `r5apex.exe`, `cs2.exe`, `fortnitepc-win64-shipping.exe`.
  4. Manual mapping (`mappings.json`, exe → name), saved when the user sets a game for an Unknown clip.
- `steam::libraries(steam_root) -> Vec<PathBuf>` parses `steamapps/libraryfolders.vdf`.
- `steam::manifests(lib) -> Vec<AppManifest{appid, name, installdir}>` parses `appmanifest_*.acf`.
- The Steam root is `~/.local/share/Steam` on Linux and `HKCU\Software\Valve\Steam\SteamPath` on Windows.
- `steam::header_art_url(appid) -> String` returns `https://cdn.cloudflare.steamstatic.com/steam/apps/<appid>/header.jpg`.
- `is_game_running() -> bool`, for WhileGameRunning mode: the same checks without the foreground requirement (Linux: any SteamAppId process; Windows: any process matching manifests, known or mapped).

- [ ] **Step 1: Write failing tests.**
  - The VDF and ACF parsers on fixtures copied from the owner's real files (`libraryfolders.vdf` with two library paths; an appmanifest for 2887580 "Active Matter").
  - `known` lookup is case-insensitive.
  - A mapping round-trip.
  - Linux environ parsing: `parse_environ(bytes) -> Option<u32>` returns 2887580 for `...\0SteamAppId=2887580\0...` and None for `SteamAppId=0`.
- [ ] **Step 2:** Run `cargo test -p klipt-desktop games`. Expected: FAIL.
- [ ] **Step 3:** Implement. Windows code is behind `#[cfg(windows)]` and compiled in CI.
- [ ] **Step 4:** Run `cargo test -p klipt-desktop games`. Expected: PASS.
- [ ] **Step 5:** Commit with `feat(desktop): game detection (steam, known exes, manual mapping)`.

### Task 6: Recorder interface, backends, supervisor

**Files:** create `desktop/src-tauri/src/recorder/{mod.rs, gsr.rs, obs.rs, supervisor.rs}`.

**Interfaces:**
- `trait RecorderBackend { fn start(&mut self, cfg: &RecordConfig) -> Result<()>; fn save(&mut self) -> Result<()>; fn stop(&mut self) -> Result<()>; fn events(&self) -> Receiver<RecorderEvent>; fn list_displays(&mut self) -> Result<Vec<Display>>; }`
- `RecorderEvent = Started{encoder} | Saved{path, duration_ms} | Busy | Error{code, message} | Exited{code: Option<i32>}`
- `gsr.rs` (Linux):
  - `build_args(cfg) -> Vec<String>` produces exactly the decision-record command: `-w <display or portal>`, `-f`, `-k h264 -bm cbr -q <kbps>`, `-ac aac`, `-a default_output` and `-a <mic|default_input>` only if the mic is on, `-c mp4 -r <seconds> -replay-storage ram -o <dir>`, plus the portal flags `-restore-portal-session yes -portal-session-token-filepath <app data>/portal-token`.
  - Save = `nix::sys::signal::kill(child_pid, SIGUSR1)`. A stdout line that is an existing `.mp4` path becomes `Saved`.
  - Stop = SIGINT to the child PID, then wait up to 5 s, then kill **that PID**.
  - Fallback: if KMS mode exits within 3 s with a non-zero code, restart once with `-w portal`.
  - `list_displays` parses `gpu-screen-recorder --list-monitors` (`NAME|WxH`).
- `obs.rs` (Windows, and Linux for debugging): spawns `klipt-recorder` with piped stdio and speaks the Task 1 protocol.
- `supervisor.rs`:
  - Owns the backend. It applies `recordingMode` (in WhileGameRunning it polls `games::is_game_running()` every 3 s and starts or stops).
  - On `Exited` without a requested stop, it restarts with backoff of 1 s, 3 s, then 10 s, up to 3 times, then reports `Error{code:"recorder_crashed"}`.
  - It ignores `save` while a save is pending and emits `Busy` instead (**Review Focus 1**).
  - On `Saved` it runs game detection (captured **at save time**), probe and thumbnail, inserts into the library, and emits the `clip-saved` Tauri event.

- [ ] **Step 1: Write failing tests.**
  - `gsr::build_args`: KMS with mic, portal without mic, custom bitrate.
  - `--list-monitors` parsing.
  - Saved-line parsing ignores non-path lines.
  - Supervisor with a **fake backend** (a test double implementing the trait):
    - A double save gives `Busy`.
    - An unexpected `Exited` gives 3 restarts and then `recorder_crashed` (**Review Focus 2**, with a fake clock so delays aren't real).
    - WhileGameRunning starts when the detector returns true and stops when it returns false.
- [ ] **Step 2:** Run `cargo test -p klipt-desktop recorder`. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run `cargo test -p klipt-desktop recorder`. Expected: PASS.
- [ ] **Step 5:** Manual Linux check with the owner: `tauri dev`, recording starts with gsr on DP-1, and an invoke `save_clip` puts a clip in the library DB.
- [ ] **Step 6:** Commit with `feat(desktop): recorder backends (gsr, libobs) and supervisor`.

### Task 7: Hotkeys, tray, save feedback

**Files:** create `desktop/src-tauri/src/hotkey/{mod.rs, portal.rs, windows.rs}`, `tray.rs` and `sound.rs`, plus `assets/saved.wav`.

**Interfaces:**
- `trait HotkeyService { fn bind(&mut self, accel: &str) -> Result<BoundKey { display: String }>; fn events(&self) -> Receiver<()>; }`
  - Linux uses the ashpd GlobalShortcuts portal with id `save-clip`. It shows the key **KDE reports**, not the requested one (decision record, open issue 5).
  - Windows uses tauri-plugin-global-shortcut.
- Tray:
  - The icon shows the state: recording, paused or error.
  - Menu: "Save clip now", "Pause recording" / "Resume recording", "Open Klipt", "Quit".
  - Closing the window hides it to the tray. `startWithOs` uses tauri-plugin-autostart.
- On save: play `saved.wav` with rodio if `saveSound` is on, and show a notification "Clip saved — <game or 'Unknown game'>" if `saveNotification` is on. On `Busy`: show the toast "Already saving…".

- [ ] **Step 1: Write failing tests** for an accelerator parser/normalizer: `"alt+f9"` → `"Alt+F9"`, and invalid input → error.
- [ ] **Step 2:** Run the tests. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Manual check with the owner in a fullscreen game on Linux: press the key → sound plus notification → the clip appears in the library.
- [ ] **Step 6:** Commit with `feat(desktop): hotkeys, tray, save feedback`.

### Task 8: Server API client and tus upload

**Files:** create `desktop/src-tauri/src/upload/{mod.rs, api.rs, tus.rs}`.

**Interfaces:**
- `ApiClient::new(base_url)`, with methods:
  - `login(username, password, device_name) -> Result<Session{token, me}>`
  - `me(token)`
  - `logout(token)`
  - `finalize(token, FinalizeRequest) -> Result<ServerClip>`
- `ApiError = Unauthorized | Network(String) | Server{status, code} | InvalidVideo(String)`. A 422 becomes `InvalidVideo` carrying the server's message.
- `tus::upload(client, token, file, resume: Option<(upload_url, offset)>, progress: impl Fn(u64, u64)) -> Result<UploadOutcome{upload_id}>`:
  - POST create with `Upload-Length` and `Upload-Metadata filename`.
  - PATCH in 8 MiB chunks with `Tus-Resumable: 1.0.0`.
  - On a network error it retries with HEAD to get the offset and continues, retrying 5 times with exponential backoff, then fails with `Network`.
  - It persists `(upload_id, offset)` through a callback so an app restart can resume.
- `finalize` with retry: 409 `upload_finalizing` → wait 3 s and retry (up to 10 min), 200 or 201 → done.
- `auth.json` holds `{serverUrl, token, username}`. On Linux it is written with mode 0600.

- [ ] **Step 1: Write failing tests** with wiremock:
  - The happy path (create, 2 PATCHes, finalize 201).
  - **Review Focus 3:** the connection drops after chunk 1 → HEAD gives the offset → resume → done.
  - A 401 on a PATCH gives `Unauthorized`, with no retries.
  - A finalize 409 then 200 gives success.
  - A 422 gives `InvalidVideo` with the message.
  - A login 401 gives `Unauthorized`.
- [ ] **Step 2:** Run `cargo test -p klipt-desktop upload`. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run `cargo test -p klipt-desktop upload`. Expected: PASS.
- [ ] **Step 5:** Commit with `feat(desktop): server api client and resumable tus upload`.

### Task 9: Export (trim and mix)

**Files:** create `desktop/src-tauri/src/export.rs`.

**Interfaces:**
- `export_args(input, output, edit: Edit{trim_start_ms, trim_end_ms, game_volume, mic_volume}, has_mic: bool) -> Vec<String>`:
  - `-ss <start>` placed before `-i` (keyframe snap), `-to` relative to it, `-map 0:v:0 -c:v copy`.
  - With a mic: `-filter_complex "[0:a:0]volume=G[a0];[0:a:1]volume=M[a1];[a0][a1]amix=inputs=2:normalize=0[aout]"` and `-map "[aout]"`.
  - Without a mic: `[0:a:0]volume=G[aout]`.
  - Then `-c:a aac -b:a 160k -movflags +faststart -f mp4`.
- `export(…) -> Result<PathBuf>` runs ffmpeg, writing to `<app cache>/exports/<clip id>.mp4`, and reports progress from `-progress pipe:1`.

- [ ] **Step 1: Write failing tests.**
  - Argument snapshots for the with-mic and no-mic cases (**Review Focus 5**).
  - An integration test on a generated fixture (two audio tracks, 6 s) exporting the range 2000–4000 ms: the output has 1 video and 1 audio stream, duration ≈2–3 s (keyframe snap), and the moov atom comes before mdat.
- [ ] **Step 2:** Run `cargo test -p klipt-desktop export`. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run `cargo test -p klipt-desktop export`. Expected: PASS.
- [ ] **Step 5:** Commit with `feat(desktop): trim + mix export`.

### Task 10: UI — login, library, upload flow

**Files:** `desktop/src/views/{Login,Library}.svelte`, `desktop/src/components/{ClipCard,GameFilter,UploadDialog,Toast}.svelte`, `desktop/src/lib/{api.ts, format.ts}`, `desktop/src-tauri/src/commands.rs`.

**Interfaces (Tauri commands):** `get_settings`, `set_settings`, `login`, `logout`, `list_clips(filter)`, `list_games`, `rename_clip`, `set_clip_game`, `delete_clip(id, deleteFile)`, `show_in_folder`, `upload_clip(id, title, visibility) -> emits upload-progress {id, sent, total, phase: 'exporting'|'uploading'|'finalizing'}`, `copy_share_link(id)`, `save_clip_now`.

**Behaviour:**
- First run: a server address field, then a login form. "Don't have an account? Ask your admin for an invite link."
- Library: a grid of cards with the thumbnail, game, relative date, duration, an "Uploaded" badge plus a copy-link button, and a "File missing" state. There's a game filter row and a sort toggle.
- Upload dialog:
  - The title defaults to `<Game> — <date>`.
  - Visibility is "Feed" (default) or "Link only".
  - Progress goes through export → upload → finalize.
  - On success the link is copied automatically and the toast says "Link copied!".
  - On a 401 the user is sent to login, and the upload restarts after login (**Review Focus 3**).

- [ ] **Step 1: Write failing vitest tests** for `format.ts`: `formatDuration(65000)` → `"1:05"`, `relativeDate`, and `defaultTitle("Active Matter", date)` → `"Active Matter — 9 Nov 2026"`.
- [ ] **Step 2:** Run the tests. Expected: FAIL.
- [ ] **Step 3:** Implement the views and commands.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Manual check with the owner against a local server (`docker compose` from feat/server): log in, save a clip, upload it, and confirm the link opens the share page.
- [ ] **Step 6:** Commit with `feat(desktop): login, library, upload flow`.

### Task 11: UI — editor

**Files:** `desktop/src/views/Editor.svelte`, `desktop/src/components/TrimBar.svelte`.

**Behaviour:**
- On open, the clip's audio tracks are demuxed to `<app cache>/edit/<id>-{game,mic}.m4a` (cached).
- A muted `<video>` plus two `<audio>` elements, all loaded through Tauri's asset protocol.
- Sync: on play, set the audio currentTime to the video's. **On seeking, pause the audio and resync on `seeked`.** Correct drift above 50 ms every 250 ms.
- TrimBar has two handles and a playhead. Playback loops inside the trim range.
- Game and Mic volume sliders from 0–100%. The Mic slider is hidden when there's no mic track.
- Buttons: "Save edit" (persists to the DB, non-destructive), "Export copy…" (save dialog), and "Upload" (opens the UploadDialog).

- [ ] **Step 1: Write failing vitest tests** for pure helpers in `TrimBar` logic (`clampTrim`, `msToX`/`xToMs`).
- [ ] **Step 2:** Run the tests. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Manual check with the owner: trim, change volumes, upload, and the shared clip plays with the expected cut and mix.
- [ ] **Step 6:** Commit with `feat(desktop): editor`.

### Task 12: UI — settings

**Files:** `desktop/src/views/Settings.svelte`.

**Behaviour:**
- Sections:
  - **Recording:** mode, display (from `list_displays`), resolution, FPS, quality, and buffer length with an estimated RAM use of `bitrate × seconds / 8`.
  - **Buffer location:** RAM, plus Disk disabled with the tooltip.
  - **Audio:** mic on/off and device.
  - **Hotkey:** shows the bound key, with a "Change" button.
  - **Clips folder.**
  - **General:** start with the OS, save sound, notification.
  - **Account:** server, user, log out.
- Changing any recording setting restarts the recorder, and the UI shows "Recording restarted with new settings — the buffer was cleared."

- [ ] **Step 1: Write failing vitest tests**: `estimateRamMb(20000, 30)` → 75, and form validation mirrors `Settings::validate`.
- [ ] **Step 2:** Run the tests. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Commit with `feat(desktop): settings view`.

### Task 13: Packaging and release builds

**Files:** `.github/workflows/desktop-windows.yml`, `packaging/aur/PKGBUILD`, and the bundle section of `tauri.conf.json`.

- **Windows:**
  - An NSIS installer containing `klipt-desktop.exe`, `ffmpeg.exe` and `ffprobe.exe` (BtbN win64 GPL build, pinned release), and the Task 2 OBS runtime subset under `resources/obs/`.
  - The app finds `klipt-recorder.exe` at `resources/obs/bin/64bit/`.
  - The workflow uploads the installer as an artifact, and attaches it to a GitHub Release on `v*` tags.
- **Linux:** the PKGBUILD builds from the repo and depends on `gpu-screen-recorder ffmpeg webkit2gtk-4.1 libayatana-appindicator`. `makepkg -si` installs it on the owner's machine.
- `TEST-ON-WINDOWS.md` is updated for the full app.

- [ ] **Step 1:** Write the workflow and the PKGBUILD.
- [ ] **Step 2:** Push. The Windows workflow must go green and produce an installer artifact. On Linux, `makepkg -si` must install and launch Klipt from the app menu (manual, with the owner).
- [ ] **Step 3:** Commit with `build: windows installer CI, AUR PKGBUILD`.

### Task 14: Auto-update

**Files:** the updater config in `tauri.conf.json`, the `desktop-windows.yml` release job, and `docs/RELEASING.md`.

- tauri-plugin-updater reads `https://github.com/cRaZy92/klipt/releases/latest/download/latest.json`.
- The updater signing key is generated with `pnpm tauri signer generate`. The **private key is stored by the owner** as the GitHub secret `TAURI_SIGNING_PRIVATE_KEY`. This is an owner step, documented in RELEASING.md and never committed.
- The app checks for updates on start and every 6 h, then shows "Update available — restart to install".

- [ ] **Step 1:** Configure the updater, and have the owner add the secret.
- [ ] **Step 2:** Tag `v0.1.0`, then `v0.1.1`. An installed 0.1.0 must offer and install 0.1.1 (manual, on Windows).
- [ ] **Step 3:** Commit with `build: auto-update via GitHub Releases`.
