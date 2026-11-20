# Full Recording Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A shortcut, a My clips button and a tray item start and stop a normal recording of any length while the replay buffer keeps running. The finished file lands in My clips like a clip, marked as a recording.

**Architecture:** The same helper process records both. On Linux, gsr gets `-ro <clips>/Recordings` and SIGRTMIN starts or stops the recording; on Windows, `klipt-recorder` gets a libobs `ffmpeg_muxer` output that shares the replay buffer's encoders. The supervisor owns the recording state and makes sure nothing it does (settings change, pause, game exit, quit) silently loses a running recording. The saved file goes through the existing save pipeline with `is_recording = true`.

**Tech Stack:** Rust (Tauri 2 app in `desktop/src-tauri`, `desktop/recorder-obs` sidecar), Svelte 5 + TypeScript UI (`desktop/src`), SQLite (rusqlite).

**Spec:** No separate design doc. The owner approved the roadmap entry (`docs/ROADMAP.md`, "Planned: recording and multi-clip editing", item 1) and said "go for it". The decisions below are Claude's calls, made from the code and a gsr test run on 2026-11-20.

## Decisions (Claude's calls; the owner can overturn any)

- **Naming in code:** "full recording" (`full_recording`, `FullRecording…`), because "recording" already means the replay buffer running (`is_recording`, `RecordingMode`, `recording-started`). In the UI: "Record" / "Stop recording" / "Recording saved".
- **Where files go:** `<output_dir>/Recordings/`. gsr names them `Video_YYYY-MM-DD_HH-MM-SS.mp4`; klipt-recorder uses `Klipt Recording %CCYY-%MM-%DD %hh-%mm-%ss`.
- **Library:** a new local column `is_recording INTEGER NOT NULL DEFAULT 0`. `kind` stays `clip`, so uploads, the server and the editor need no changes. Cards show a "Recording" badge.
- **Storage cleanup:** recordings follow the same rule as clips (same days; favorites and uploads kept). No separate setting.
- **Game tag:** the game detected when the recording **starts**.
- **Default shortcut:** `Alt+F9` (setting `record_hotkey`; empty = no shortcut), bound beside the save and screenshot shortcuts.
- **Length and size:** no limit and no disk check (YAGNI; the owner rejected quotas).
- **Feedback:** the start and stop notifications respect `save_notification`; the saved sound plays on stop when `save_sound` is on. The tray tooltip shows "Klipt — recording video".

### Behaviour while a full recording runs (the supervisor's rules)

| Situation | What happens |
| --- | --- |
| Toggle while the replay buffer isn't running (WhileGameRunning, no game) and not paused | Start the backend, then start the full recording once the backend reports `Started`. The backend stays up until the recording stops, then the mode is applied again |
| Toggle while paused | Error `recording_paused` (existing message) |
| Toggle while `Failed` / `RestartPending` | Error `not_recording` |
| Settings saved (`Configure`) | Kept as `pending_config`; applied right after the recording's file arrives (or the recording is lost) |
| Pause from the tray | Stop the full recording first (the file is kept), then pause |
| Game exits in WhileGameRunning mode | Don't stop the backend while a full recording runs; the next poll after it ends stops it |
| Helper exits unexpectedly mid-recording | Emit error `full_recording_lost` ("The recording stopped unexpectedly. What was recorded so far may be in the Recordings folder."), clear the state, normal restart logic |
| App quit (`Shutdown`) | Ask the backend to stop the recording, wait up to `FULL_RECORDING_STOP_TIMEOUT = 15 s` for its file, process it, then shut down |
| Replay save (`Save`) during a full recording | Works as today |

gsr facts (tested 2026-11-20 with gsr 6.1.7):
- SIGRTMIN starts the recording and SIGRTMIN again stops it.
- stderr prints `gsr info: Started recording` and `gsr info: Stopped recording`.
- stdout prints the recording's absolute path when it stops.
- SIGINT during a recording finishes the file cleanly and prints its path.
- Replay saves still work during and after a recording.

## Global Constraints

- Never signal or kill a process the app didn't spawn; signal only by the child's own PID or handle (`feedback-safe-commands`).
- Never run prettier (no config in the repo; it reformats whole files).
- Linux target: KDE Plasma on Wayland. Windows code paths can't run here: they must compile (`cargo check` for the app; the sidecar's protocol tests run on Linux), and their real-machine checks go into `desktop/recorder-obs/TEST-ON-WINDOWS.md`.
- One commit per task, conventional style (`feat(desktop): …`), ending with the session's `Claude-Session:` line.
- User-facing copy is plain English sentences, like `friendly_message` today.

## Review Focus

1. **Settings saved mid-recording.** The recording must not be cut, and the new settings must still apply once it ends. Tested in Task 4 (`configure_waits_for_the_full_recording`).
2. **Quitting Klipt mid-recording.** The file must reach My clips, not sit orphaned in the folder. Tested in Task 4 (`shutdown_finishes_the_full_recording_first`).
3. **The game closes mid-recording in "while a game runs" mode.** Recording continues. Tested in Task 4 (`game_exit_keeps_a_full_recording_running`).
4. **A replay saved during a full recording** must be routed as a clip, and the recording's file as a recording, by folder rather than by order. Tested in Task 2 (`recordings_folder_lines_are_full_recordings`).
5. **The window is re-created** (tray release after 5 min) mid-recording. The button must show the running timer again. Tested in Task 6 via `get_full_recording` returning `startedAtMs`, which the UI reads on mount.

---

### Task 1: Library marks recordings

**Files:**
- Modify: `desktop/src-tauri/src/library/db.rs` (append a migration; `COLUMNS`, `row_to_clip`, `insert`)
- Modify: `desktop/src-tauri/src/library/mod.rs` (`Clip.is_recording`, `NewClip.is_recording`)
- Modify: every `NewClip { … }` literal (`recorder/mod.rs`, `import.rs`, `screenshot.rs`, tests). Add `is_recording: false`
- Modify: `desktop/src/lib/api.ts` (`ClipView.isRecording: boolean`)

**Interfaces:**
- Produces: `Clip.is_recording: bool` (serialized `isRecording`) and `NewClip.is_recording: bool`.

- [ ] **Step 1: Write the failing test** `inserts_and_reads_back_is_recording` in `db.rs` tests: insert a `NewClip { is_recording: true, .. }` and assert `library.get(&id)?.is_recording`. Insert another with `false` and assert false. A DB created by the previous migration list (open an old DB fixture, or open before adding the row) reads `is_recording == false`.
- [ ] **Step 2: Run** `cd desktop/src-tauri && cargo test inserts_and_reads_back_is_recording`. Expected: FAIL (no field).
- [ ] **Step 3: Implement.** New migration string `ALTER TABLE clips ADD COLUMN is_recording INTEGER NOT NULL DEFAULT 0;` appended last; add the column to `COLUMNS`, `row_to_clip` (next index) and the `INSERT`.
- [ ] **Step 4: Run** `cargo test` (whole crate) and `cd .. && pnpm check`. Expected: all pass.
- [ ] **Step 5: Commit** `feat(desktop): the library can mark a clip as a full recording`

### Task 2: Backend trait and gsr support

**Files:**
- Modify: `desktop/src-tauri/src/recorder/mod.rs` (trait, `RecorderEvent`, `RecordConfig::recordings_dir`)
- Modify: `desktop/src-tauri/src/recorder/gsr.rs`
- Modify: `desktop/src-tauri/src/recorder/obs.rs` (stub methods only, returning `Err(AppError::RecorderFailed("not supported yet".into()))`. Task 3 fills them in)
- Modify: `FakeBackend` in `supervisor.rs` tests (record calls)

**Interfaces:**
- Produces:
  - `RecorderBackend::start_full_recording(&mut self) -> Result<()>` and `stop_full_recording(&mut self) -> Result<()>`. Both return at once; results arrive as events.
  - `RecorderEvent::FullRecordingStarted` and `RecorderEvent::FullRecordingSaved { path: PathBuf, duration_ms: Option<u64> }`.
  - `RecordConfig::recordings_dir(&self) -> PathBuf` = `output_dir.join("Recordings")`.
  - `gsr::RECORDINGS_DIR: &str = "Recordings"`.

- [ ] **Step 1: Write failing tests** in `gsr.rs`:
  - `build_args` tests now expect `-ro /home/u/Videos/Klipt/Recordings` right after `-o <dir>`. Update the four expected strings.
  - `recordings_folder_lines_are_full_recordings`: `classify_stdout_line(line, recordings_dir) -> Option<RecorderEvent>`. An existing `.mp4` in `recordings_dir` → `FullRecordingSaved { path, duration_ms: None }`; one in the replay dir → `Saved`; other text → `None`.
  - `stderr_started_line_is_full_recording_started`: `is_full_recording_started("gsr info: Started recording")` is true; `"gsr info: Stopped recording"` is false.
  - `real_child_records_on_sigrtmin` with a bash fake gsr (`#!/bin/bash`, `trap '…' RTMIN`). The first RTMIN prints `gsr info: Started recording` to stderr; the second writes `$ro/Video_x.mp4` and prints its path. Expect `FullRecordingStarted`, then `FullRecordingSaved { path: <recordings>/Video_x.mp4, .. }`. The fake reads `-ro`'s value from its args.
- [ ] **Step 2: Run** `cargo test --lib recorder::gsr`. Expected: FAIL.
- [ ] **Step 3: Implement.**
  - `build_args` appends `-ro <recordings_dir>`.
  - `start` creates the recordings dir.
  - `spawn_child` takes the recordings dir and uses `classify_stdout_line`; the stderr reader sends `FullRecordingStarted` on the started line.
  - Both trait methods send `SIGRTMIN` (`Signal::SIGRTMIN` isn't in nix's `Signal` enum: use `nix::libc::kill(pid, nix::libc::SIGRTMIN())`) to the child in the slot. No child → `AppError::NotRecording`; restarting → `AppError::RecorderStarting`.
  - gsr only toggles, so start/stop correctness is the supervisor's job (Task 4).
- [ ] **Step 4: Run** `cargo test`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(desktop): gsr records a full video alongside the replay buffer (-ro, SIGRTMIN)`

### Task 3: klipt-recorder (Windows) records alongside the replay buffer

**Files:**
- Modify: `desktop/recorder-obs/src/protocol.rs` (commands, events, error codes, `KNOWN_COMMANDS`)
- Modify: `desktop/recorder-obs/src/session.rs` (recording output)
- Modify: `desktop/recorder-obs/src/main.rs` (`Controller`)
- Modify: `desktop/src-tauri/src/recorder/obs.rs` (send the commands, parse the events)
- Modify: `desktop/recorder-obs/TEST-ON-WINDOWS.md`

**Interfaces:**
- Consumes: the Task 2 trait methods and events.
- Produces (protocol, camelCase like the rest):
  - commands `{"cmd":"startRecording"}` and `{"cmd":"stopRecording"}`
  - events `{"event":"recordingStarted"}` and `{"event":"recordingSaved","path":…,"durationMs":…}`
  - error codes `recording_failed` (output couldn't start or the file failed), `already_full_recording`, `not_full_recording`

- [ ] **Step 1: Write failing tests.**
  - `protocol.rs`: the two commands parse; both events serialize to the lines above; the new error codes serialize snake_case.
  - `recorder/obs.rs`: `parse_event_line` maps both events to new `HelperEvent` variants. The backend's event loop turns them into `RecorderEvent::FullRecordingStarted` and `FullRecordingSaved { duration_ms: Some(..) }`. The error codes pass through as `RecorderEvent::Error`.
- [ ] **Step 2: Run** `cd desktop/recorder-obs && cargo test --lib protocol` and `cd ../src-tauri && cargo test recorder::obs`. Expected: FAIL.
- [ ] **Step 3: Implement.**
  - `Session::start_recording(&mut self) -> Result<(), Event>`:
    - Create an `ffmpeg_muxer` output named `klipt recording`. Its `path` is `<output_dir>\Recordings\Klipt Recording <local time>.mp4`; create the folder.
    - Set the **same** video encoder and audio encoders (each track index) as the replay buffer, then `obs_output_start`.
    - Connect its `stop` signal to a new `Signal::RecordingStopped { code }` (a second `SignalContext`).
    - Remember `recording_started_at`.
  - `Session::stop_recording(&mut self)` calls `obs_output_stop` on the recording output. On `RecordingStopped`, code 0 emits `recordingSaved` with the path and `recording_started_at.elapsed()`; any other code emits `recording_failed`. Then release the output.
  - `Session::stop` stops a running recording first and waits up to 5 s for its stop signal (like the replay buffer's wait), so `Stop`/`Shutdown` still emit `recordingSaved`.
  - Unexpected `OutputStopped` of the replay buffer while recording: also release the recording output (its file is what it is).
- [ ] **Step 4: Run** both crates' tests, plus `cargo check --target x86_64-pc-windows-gnu` if that target is installed (else rely on CI `desktop-windows.yml`). Expected: PASS.
- [ ] **Step 5: Add to `TEST-ON-WINDOWS.md`:**
  - Start/stop a recording during a game; it plays with sound.
  - A replay save during the recording works.
  - Quitting mid-recording leaves a playable file in My clips.
  - GPU encoder load doesn't double (one shared encoder).
- [ ] **Step 6: Commit** `feat(recorder-obs): a recording output shares the replay buffer's encoders`

### Task 4: Supervisor owns the full recording

**Files:**
- Modify: `desktop/src-tauri/src/recorder/supervisor.rs`
- Modify: `desktop/src-tauri/src/recorder/mod.rs` (`SavedClip.is_recording`, `process_saved_clip` passes it to `NewClip`)

**Interfaces:**
- Consumes: the Task 2 trait methods and events; `NewClip.is_recording` from Task 1.
- Produces:
  - `Input::ToggleFullRecording`.
  - `SupervisorEvent::FullRecordingStarted { started_at_ms: i64 }` (wall clock, for the UI timer) and `SupervisorEvent::FullRecordingStopped` (sent when the state ends for any reason, before `ClipSaved` of the recording).
  - `SavedClip.is_recording: bool`.
  - `Supervisor::full_recording_started_at_ms(&self) -> Option<i64>`.
  - `pub const FULL_RECORDING_STOP_TIMEOUT: Duration = Duration::from_secs(15)`.
  - `friendly_message("full_recording_lost")` with the copy from Decisions.
  - `Deps.wall_clock_ms: Box<dyn Fn() -> i64 + Send>` (tests pass a fixed value).

State: `full: Option<FullRecording>`, where `FullRecording { phase: Starting | Running | Stopping { deadline }, game: Detected, started_at_ms: i64 }`. Also `start_full_after_backend: bool`, `pending_config: Option<(RecordConfig, RecordingMode)>`, `shutting_down: bool`, `pause_after_full: bool`.

- [ ] **Step 1: Write failing tests** (with `FakeBackend` counting `full_starts`/`full_stops`, and the existing harness):
  - `toggle_starts_then_stops`: Running + `ToggleFullRecording` → `full_starts == 1`. The backend sends `FullRecordingStarted` → sink got `FullRecordingStarted { started_at_ms }`. Toggle again → `full_stops == 1`. `FullRecordingSaved` → `process_clip` got `SavedClip { is_recording: true, game: <game at start> }`, then the sink got `FullRecordingStopped` and `ClipSaved`.
  - `toggle_in_game_mode_without_a_game_starts_the_backend_first`: WhileGameRunning, no game, toggle → `starts == 1`, `full_starts == 0`. After `Started` → `full_starts == 1`. After the recording is saved and with no game running, the next tick stops the backend.
  - `configure_waits_for_the_full_recording`: while recording, `Configure(new)` → `stops == 0`. After `FullRecordingSaved` → stop+start with the new config.
  - `game_exit_keeps_a_full_recording_running`: game mode, recording runs, game gone, tick past `GAME_POLL_INTERVAL` → `stops == 0`.
  - `pause_stops_the_full_recording_first`: `Pause` → `full_stops == 1`, `stops == 0`. After saved → `stops == 1`, `is_paused()`.
  - `shutdown_finishes_the_full_recording_first`: `handle(Shutdown)` returns true with `full_stops == 1`. After saved → the next `handle` returns false and the file was processed. Without a file, a tick past `FULL_RECORDING_STOP_TIMEOUT` ends it (returns false).
  - `helper_exit_loses_the_full_recording`: `Exited` while recording → sink got `Error { code: "full_recording_lost" }` and `FullRecordingStopped`; the normal restart is scheduled.
  - `toggle_while_paused_or_failed_errors`: `recording_paused` and `not_recording` respectively.
  - `a_stop_that_never_produces_a_file_times_out`: Stopping, tick past `FULL_RECORDING_STOP_TIMEOUT` → `Error { code: "save_failed" }`, `FullRecordingStopped`, pending config applied.
- [ ] **Step 2: Run** `cargo test recorder::supervisor`. Expected: FAIL.
- [ ] **Step 3: Implement** the rules in the Decisions table.
  - `next_deadline` includes the Stopping deadline.
  - `stop_backend` must not be reached while `full.is_some()` except via Shutdown/Exited paths. A `FullRecordingSaved` from an older generation (after the SIGINT-on-stop path) is still accepted. Exempt that one event kind from the generation filter in `handle`.
  - `Shutdown` returns false only once `full` is `None`; the run loop already keeps calling `handle`.
- [ ] **Step 4: Run** `cargo test`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(desktop): the supervisor runs a full recording beside the replay buffer without losing it`

### Task 5: Shortcut

**Files:**
- Modify: `desktop/src-tauri/src/settings.rs` (`record_hotkey: String`, `DEFAULT_RECORD_HOTKEY = "Alt+F9"`, validation like `screenshot_hotkey`, reset key `"recordHotkey"`, rebind when it changes)
- Modify: `desktop/src-tauri/src/hotkey/mod.rs` (`Action::ToggleFullRecording`, `Keys.record: Option<String>`, `BoundKey.record` / `record_error`, `HotkeyStatus.record_display` / `record_error`, the forwarder calls `SupervisorHandle::toggle_full_recording()`)
- Modify: `desktop/src-tauri/src/hotkey/portal.rs` (`RECORD_ID = "full-recording"`, description `"Klipt: start/stop recording"`)
- Modify: `desktop/src-tauri/src/hotkey/windows.rs`
- Modify: `desktop/src/views/Settings.svelte` and `desktop/src/lib/settings.ts` (a "Start/stop recording" shortcut row under the screenshot one, same component)

**Interfaces:**
- Consumes: `SupervisorHandle::toggle_full_recording(&self)` (added in this task: sends `Input::ToggleFullRecording`).
- Produces: `Settings.record_hotkey` (`recordHotkey`); the `hotkey-bound` payload gains `recordDisplay` and `recordError`.

- [ ] **Step 1: Write failing tests.**
  - settings: default `Alt+F9`; an invalid `recordHotkey` is rejected like the screenshot one; empty means none; the same key as save or screenshot is rejected.
  - `Keys::from_settings` filters empty.
  - `settings.test.ts`: the new field's default.
- [ ] **Step 2: Run** `cargo test settings hotkey` and `pnpm test`. Expected: FAIL.
- [ ] **Step 3: Implement.** A record shortcut that can't be bound is reported like the screenshot one (notification; saving clips still works).
- [ ] **Step 4: Run** `cargo test && pnpm test && pnpm check`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(desktop): a shortcut starts and stops a full recording (Alt+F9)`

### Task 6: App wiring, tray, button and badge

**Files:**
- Modify: `desktop/src-tauri/src/recorder/mod.rs` (`events::FULL_RECORDING_STARTED = "full-recording-started"` with payload `{ startedAtMs }`, `FULL_RECORDING_STOPPED = "full-recording-stopped"`; `spawn_for_app` passes `wall_clock_ms`)
- Modify: `desktop/src-tauri/src/recorder/supervisor.rs` (`SupervisorHandle::full_recording(&self) -> Option<i64>`, answered through `Input::FullRecordingStatus(Sender<Option<i64>>)`)
- Modify: `desktop/src-tauri/src/commands.rs` + `lib.rs` handler list (`toggle_full_recording`, `get_full_recording -> Option<i64>`)
- Modify: `desktop/src-tauri/src/feedback.rs`:
  - notify "Recording started" on start and "Recording saved" on the recording's `ClipSaved` (decide by `view.clip.is_recording`); both respect `save_notification`, and the sound respects `save_sound`
  - `full_recording_lost` always notifies, like a failed save
- Modify: `desktop/src-tauri/src/tray.rs`:
  - an item `ID_RECORD` labelled "Start recording" or "Stop recording", between "Save clip now" and Pause
  - tooltip "Klipt — recording video" while a full recording runs
  - reuse `tray-recording.png`
- Modify: `desktop/src/lib/api.ts`:
  - `toggleFullRecording()` and `getFullRecording(): Promise<number | null>`
  - event names
- Modify: `desktop/src/views/Library.svelte` (header buttons)
- Modify: `desktop/src/components/ClipCard.svelte` (a "Recording" badge when `clip.isRecording`)
- Create: `desktop/src/lib/elapsed.ts` + `elapsed.test.ts` (`formatElapsed(ms: number): string` → `"0:07"`, `"12:34"`, `"1:02:03"`)

**Interfaces:**
- Consumes: Tasks 1, 4, 5.
- Produces: the UI contract above.

- [ ] **Step 1: Write failing tests.**
  - `elapsed.test.ts`: `formatElapsed(7_000) === "0:07"`, `formatElapsed(754_000) === "12:34"`, `formatElapsed(3_723_000) === "1:02:03"`, `formatElapsed(-5) === "0:00"`.
  - tray: the label is "Stop recording" while a full recording runs and "Start recording" after `FullRecordingStopped` (unit test on the status/label function, like `pause_label`).
  - feedback: `saved_text` for a recording says "Recording saved".
- [ ] **Step 2: Run** `pnpm test elapsed` and `cargo test tray feedback`. Expected: FAIL.
- [ ] **Step 3: Implement.**
  - Library header: a `btn` "Record" (icon `video` if it exists in `Icon.svelte`, else the rec dot) sits left of "Save clip now".
  - While running it reads "Stop · 12:34" (a 1 s interval from `startedAtMs`) and gets the `btn-rec` style.
  - On mount, call `getFullRecording()`, so a re-created window shows the timer. Listen to both events.
- [ ] **Step 4: Run** `cargo test && pnpm test && pnpm check`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(desktop): Record button, tray item and Recording badge for full recordings`

### Task 7: Roadmap

**Files:**
- Modify: `docs/ROADMAP.md` (item 1 of "Planned: recording and multi-clip editing")

- [ ] **Step 1:**
  - Mark item 1 built (date and commit range).
  - Replace its "Open:" sentence with what was decided: gsr `-ro` + SIGRTMIN on one process; libobs shares the encoders; recordings follow the normal cleanup rule; shortcut Alt+F9.
  - No "test in the app" todo lines (owner preference).
- [ ] **Step 2: Commit** `docs: roadmap: full recording built`
