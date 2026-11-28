# Uploads survive a restart, Copy link on finish, recording safety — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Unfinished uploads resume after a restart; a finished upload offers a Copy link button instead of overwriting the clipboard; a full recording shows its size and stops cleanly before the disk fills.

**Architecture:** The Rust core owns all three. A `pending_uploads` table in `library.db` is the durable queue; the core resumes it at launch and after login. The core decides how a finished upload is announced (in-app toast vs system notification with a Copy button). The recorder supervisor checks disk and file size every 5 s during a full recording through a new `Deps` hook.

**Tech Stack:** Tauri 2, Rust (rusqlite, notify-rust on Linux, tauri-winrt-notification on Windows, sysinfo), Svelte 5 + TypeScript (vitest).

**Spec:** `docs/superpowers/specs/2026-11-22-uploads-and-recording-safety-design.md`

## Global Constraints

- All work in `desktop/`. Run Rust tests with `cargo test --manifest-path src-tauri/Cargo.toml <filter>`, UI tests with `pnpm vitest run <file>`, types with `pnpm check`.
- Never run prettier (no config in repo; it reformats whole files).
- Migrations are only ever appended to `MIGRATIONS` in `src-tauri/src/library/db.rs`.
- New setting `copy_link_on_upload: bool` (TS `copyLinkOnUpload`), **default false**, label "Copy the link by itself when an upload finishes", Settings → General.
- Disk thresholds: warn under **5 GB** free (once per recording), stop under **1 GB** free; check every **5 s**. Fixed constants, not settings. GB = 1_000_000_000 bytes.
- Copy: notification title "Clip uploaded" (or "Video replaced" when replaced), body = clip title, button "Copy link"; bulk: "N clips uploaded", no button; auto-copy on → toast/notification says "Link copied".
- Disk messages: warning "Disk almost full: {free} left. The recording stops at 1 GB." Stop: "Recording stopped: the disk is almost full".
- Windows code paths can't be run here; they must compile in CI (`desktop-windows.yml`). Keep them behind `#[cfg(windows)]`.
- One commit per task, conventional-commit style as in `git log` (`feat(desktop): …`).

## Review Focus

1. Quitting Klipt **during the export step** of an upload — next launch must re-export (the half-written export must not be resumed or uploaded). Test in Task 3.
2. A pending upload whose **clip was deleted** (or whose file is gone) at launch — the row must be dropped, not retried forever on every launch. Test in Task 3.
3. Launch with pending rows but **no login** — nothing starts, nothing is dropped, and logging in starts them. Test in Task 3.
4. **Several uploads finishing while the window is unfocused** — exactly one "N clips uploaded" notification, not N single ones. Test in Task 4.
5. **Free space unknown** (sysinfo finds no matching disk, e.g. a network share) — no warning and no stop, size still shown. Test in Task 5.

---

### Task 1: `pending_uploads` table and library methods

**Files:**
- Modify: `src-tauri/src/library/db.rs` (append migration; methods; tests)
- Modify: `src-tauri/src/library/mod.rs` (type `PendingUpload`, re-export as the other types are)

**Interfaces:**
- Produces:
  - `pub struct PendingUpload { pub clip_id: String, pub title: String, pub visibility: Visibility, pub tagged: Vec<String>, pub replace: bool, pub queued_at_ms: i64 }` (Serialize, camelCase)
  - `Library::add_pending_upload(&self, p: &PendingUpload) -> Result<()>` — upsert by `clip_id` (a retry overwrites settings but keeps the original `queued_at_ms`).
  - `Library::remove_pending_upload(&self, clip_id: &str) -> Result<()>`
  - `Library::clear_pending_uploads(&self) -> Result<()>`
  - `Library::pending_uploads(&self) -> Result<Vec<PendingUpload>>` — oldest `queued_at_ms` first.
  - Deleting a clip through the library's existing delete method also removes its pending row.

- [ ] **Step 1: Write failing tests** in `db.rs` tests:
  - `pending_uploads_round_trip_oldest_first`: add B (queued 200) then A (queued 100) → list is `[A, B]` with tagged `["sam"]`, replace `true` kept.
  - `pending_upload_retry_keeps_queue_place`: add A at 100, add A again at 300 with title "New" → one row, title "New", `queued_at_ms == 100`.
  - `deleting_a_clip_drops_its_pending_upload`.
  - `clear_pending_uploads_empties_the_table`.
- [ ] **Step 2:** `cargo test --manifest-path src-tauri/Cargo.toml pending_upload` → FAIL (no such method).
- [ ] **Step 3:** Append migration `CREATE TABLE pending_uploads (clip_id TEXT PRIMARY KEY, title TEXT NOT NULL, visibility TEXT NOT NULL, tagged TEXT NOT NULL DEFAULT '[]', replace INTEGER NOT NULL DEFAULT 0, queued_at INTEGER NOT NULL);` and implement the methods (upsert = `INSERT … ON CONFLICT(clip_id) DO UPDATE SET title, visibility, tagged, replace`).
- [ ] **Step 4:** Same command → PASS; full `cargo test` for `library` still passes (the migration-count test, if any, updated).
- [ ] **Step 5:** Commit `feat(desktop): remember unfinished uploads in the library`.

### Task 2: Upload jobs write and clear their row

**Files:**
- Modify: `src-tauri/src/commands.rs` (`start_upload`, its spawned body, `logout`)
- Modify: `src-tauri/src/errors.rs` (classification + tests)

**Interfaces:**
- Consumes: Task 1 methods.
- Produces:
  - `impl AppError { pub fn keeps_upload_queued(&self) -> bool }` — true for `Network`, `Unauthorized`, `Io` (connection/quit), `ServerError` whose status is 0, 408, 429 or ≥ 500; false for everything else (`ServerError` 4xx, `InvalidVideo`, `FileMissing`, `ToolFailed`, `ToolsMissing`, `ClipNotFound`, `Refused`, `InvalidTitle`, …). `ServerError` keeps its text as `"{status} {code}"`; parse the leading status.
  - `async fn run_job(app: AppHandle, job: Job, guard: UploadGuard) -> UploadEnd` extracted from the task `start_upload` spawns today (same behaviour: panic-safe inner task, guard dropped before emitting, cleanup on success). `pub enum UploadEnd { Done { share_url: String, replaced: bool, title: String }, Failed }`. `start_upload` keeps returning once started; it spawns `run_job`. (Use the existing guard type name from `Uploads::try_begin`.)
  - `fn prepare_upload(app: &AppHandle, id, title, visibility, tagged, replace) -> Result<(Job, UploadGuard)>` — the checks `start_upload` does today, split out so Task 3 can await `run_job` directly.

- [ ] **Step 1: Failing tests** in `errors.rs`: `upload_failures_that_wait_for_next_launch` asserts `keeps_upload_queued` true for `Network("x")`, `Unauthorized`, `ServerError("503 busy")`, `ServerError("0 bad_location")`, `ServerError("429 slow")`; false for `ServerError("413 too_big")`, `InvalidVideo("x")`, `FileMissing`, `ToolFailed{tool:"ffmpeg",…}`.
- [ ] **Step 2:** Run `cargo test … upload_failures_that_wait` → FAIL.
- [ ] **Step 3:** Implement `keeps_upload_queued`. In `prepare_upload`, after `try_begin` succeeds, `add_pending_upload` (now-ms, title cleaned, tagged, `replace.unwrap_or(false)`); a failure to write is logged, not fatal. In `run_job`: on success `remove_pending_upload`; on failure remove it unless `e.keeps_upload_queued()`. `logout` calls `clear_pending_uploads` (logout means "don't carry on").
- [ ] **Step 4:** `cargo test --manifest-path src-tauri/Cargo.toml` → PASS.
- [ ] **Step 5:** Commit `feat(desktop): keep the upload queue across restarts`.

### Task 3: Resume pending uploads at launch and after login

**Files:**
- Create: `src-tauri/src/upload_resume.rs`
- Modify: `src-tauri/src/lib.rs` (`mod`, call in `setup` after `feed_watch::start`, register commands)
- Modify: `src-tauri/src/commands.rs` (`run_upload`: export restart rule)
- Modify: `src/lib/api.ts`, `src/lib/uploads.svelte.ts`, `src/App.svelte`
- Test: `src-tauri/src/upload_resume.rs` (unit), `src/lib/uploads.test.ts` (create if absent)

**Interfaces:**
- Consumes: `prepare_upload`, `run_job`, `UploadEnd` (Task 2); `pending_uploads` (Task 1).
- Produces:
  - `pub fn start(app: &AppHandle)` — spawns one resume pass if a login exists; no-op otherwise.
  - `#[tauri::command] pub async fn resume_pending_uploads(app) -> Result<()>` — the same pass; called by the UI after login. A pass already running → returns Ok without a second one (an `AtomicBool` in managed state).
  - `#[tauri::command] pub fn pending_uploads(app) -> Result<Vec<PendingUpload>>`.
  - Pure: `fn resume_action(row: &PendingUpload, clip: Option<&Clip>, logged_in: bool) -> ResumeAction` with `enum ResumeAction { Start, Drop, Wait }`: no login → `Wait`; clip missing from library or its file gone → `Drop`; else `Start`.
  - TS: `api.pendingUploads(): Promise<PendingUpload[]>`, `api.resumePendingUploads(): Promise<void>`; `uploads.svelte.ts` exports `loadPendingUploads(): Promise<void>` that fills `requested` and shows each as `status: "queued"` cards.
- Pass behaviour: rows in order; `Drop` → `remove_pending_upload` + log; `Wait` → stop the pass; `Start` → `prepare_upload` then **await** `run_job` before the next row; `AlreadyUploading` from `prepare_upload` → skip the row (the window is already on it).
- Export rule (Review Focus 1): in `run_upload`, resume only if `upload_id` is set (tus started); when there is no saved `upload_id`, delete any existing export file before exporting so a half-written one from a quit mid-export is never reused. Check today's `resume_point(...).filter(|_| exported.is_file())` already requires `upload_id`; add the delete.

- [ ] **Step 1: Failing Rust tests**: `resume_waits_without_login`, `resume_drops_a_deleted_clip`, `resume_drops_a_clip_whose_file_is_gone` (tempdir path that doesn't exist), `resume_starts_a_present_clip`. Plus in `commands.rs` tests (or `editor` tests where `upload_export_path` lives) `stale_export_is_removed_without_an_upload_id` if the delete is put in a small helper `fn discard_stale_export(path: &Path, upload_id: Option<&str>)`.
- [ ] **Step 2:** Run them → FAIL.
- [ ] **Step 3:** Implement `upload_resume.rs`, the helper, wire into `lib.rs` (manage the running flag; `upload_resume::start(app.handle())` in setup; register both commands).
- [ ] **Step 4: Failing TS test** `loadPendingUploads shows saved uploads as queued cards` (mock `api.pendingUploads` → one row) asserting `uploads[id].status === "queued"` and its title.
- [ ] **Step 5:** Implement; in `App.svelte` call `loadPendingUploads()` at startup next to `watchUploads`, and in `resumeAfterLogin` also call `api.resumePendingUploads()`.
- [ ] **Step 6:** `cargo test …` and `pnpm vitest run src/lib/uploads.test.ts` and `pnpm check` → PASS.
- [ ] **Step 7:** Commit `feat(desktop): unfinished uploads carry on after a restart`.

### Task 4: Copy link on a button when an upload finishes

**Files:**
- Modify: `src-tauri/src/settings.rs` (field, default false), `src/lib/api.ts` (`copyLinkOnUpload`), `src/views/Settings.svelte` (switch in General, map to `"general"` like `friendNotifications`)
- Create: `src-tauri/src/upload_notice.rs` (pure decision + batch counter)
- Modify: `src-tauri/src/feedback.rs` (`notify_with_copy`), `src-tauri/src/commands.rs` (`run_job` end), `src-tauri/Cargo.toml` (`tauri-winrt-notification = "0.8.1"` under `cfg(windows)`)
- Modify: `src/lib/toasts.svelte.ts` (action), `src/components/Toast.svelte` (button), `src/lib/uploads.svelte.ts` + `src/App.svelte` (`upload-done` handling), `src/views/Library.svelte` (bulk summary text)
- Test: `upload_notice.rs` unit tests, `src/lib/toasts.test.ts`

**Interfaces:**
- Produces:
  - `pub enum Notice { Toast { title: String, link: String, copied: bool }, System { title: String, body: String, link: Option<String>, copied: bool }, Nothing }`
  - `pub fn finish_notice(finished: &[Finished], still_queued: usize, focused: bool, auto_copy: bool) -> Notice` where `pub struct Finished { pub title: String, pub link: String, pub replaced: bool }` is every upload done since the queue was last empty (including this one). Rules: `still_queued > 0` → `Nothing`; one finished → title "Clip uploaded"/"Video replaced", body = clip title, `link: Some(link)` unless `auto_copy` (then `None`, `copied: true`); several → `System`/`Toast` title "N clips uploaded", `link: None`; `focused` → `Toast` variant, else `System`. (For several + focused, `Toast` with empty `link` and the UI shows no button.)
  - `pub struct UploadBatch(Mutex<Vec<Finished>>)` managed state: `push`, `take_if_idle`.
  - `pub fn feedback::notify_with_copy(app: &AppHandle, title: &str, body: &str, link: Option<String>)` — Linux: `linux_notification(...)` + `.action("copy", "Copy link")` when `link` is Some, then on its own thread `show()?.wait_for_action(|a| if a == "copy" { copy })`. Windows: `tauri_winrt_notification::Toast::new(&app.config().identifier).title(title).text1(body).add_button("Copy link", "copy").on_activated(move |action| …)`, falling back to `notify` on error. Copy = the existing `copy_text(app, link)`.
  - `upload-done` payload gains `toast: { title: string; link: string; copied: boolean } | null`; the window shows a toast only when it's non-null (replacing today's "Link copied!" text logic in `App.svelte`). `copied` is true only when the core actually copied.
  - TS: `toast(message, kind?, action?: { label: string; run: () => void })`; toasts with an action stay 8000 ms.
- End of `run_job` on success: if `copy_link_on_upload` → copy now; push `Finished`; `still_queued` = pending rows left (Task 1) + uploads running in `Uploads`; `focused` = main window exists and `is_focused()`; act on `finish_notice`. On failure with nothing left queued and a non-empty batch, flush it the same way (so a failed last item doesn't swallow the summary). `uploadInTurn` stops relying on `inBulk` for the toast; keep its returned count for Library's existing summary but remove any duplicate "uploaded" toast there.

- [ ] **Step 1: Failing Rust tests** in `upload_notice.rs`:
  - `one_upload_unfocused_gets_a_notification_with_copy` → `System{title:"Clip uploaded", body:"Ace", link:Some(..), copied:false}`.
  - `one_upload_focused_gets_a_toast_with_copy`.
  - `auto_copy_drops_the_button` → `copied: true`, `link: None`.
  - `replaced_says_video_replaced`.
  - `nothing_while_more_are_queued` (still_queued 2).
  - `several_finished_get_one_summary` → title "3 clips uploaded", `link: None` (Review Focus 4).
  - `UploadBatch` `take_if_idle` empties itself.
- [ ] **Step 2:** Run → FAIL.
- [ ] **Step 3:** Implement `upload_notice.rs`, the setting (Rust default false; `#[serde(default)]` already on the struct), `notify_with_copy`, wiring in `run_job`.
- [ ] **Step 4: Failing TS test** `toast with an action keeps it and stays longer` in `toasts.test.ts` (fake timers: still there at 5000 ms, `action.label === "Copy link"`).
- [ ] **Step 5:** Implement toast action + button (copy via existing `api.copyTextToClipboard(link)` then `toast("Link copied!", "success")`), `upload-done` handling, Settings switch.
- [ ] **Step 6:** `cargo test …`, `pnpm vitest run`, `pnpm check` → PASS; `cargo check --target x86_64-pc-windows-gnu` only if that target is installed, else rely on CI.
- [ ] **Step 7:** Commit `feat(desktop): Copy link button when an upload finishes`.

### Task 5: Supervisor watches disk and size during a full recording

**Files:**
- Create: `src-tauri/src/disk.rs`
- Modify: `src-tauri/Cargo.toml` (sysinfo feature `"disk"` on both entries)
- Modify: `src-tauri/src/recorder/supervisor.rs` (Deps hook, deadline, tick, events, messages, tests)
- Modify: `src-tauri/src/recorder/mod.rs` (event to UI), `src-tauri/src/feedback.rs` (`is_recording_problem` includes the two new codes so they notify)

**Interfaces:**
- Produces:
  - `pub struct DiskReading { pub recording_bytes: Option<u64>, pub free_bytes: Option<u64> }`
  - `pub fn disk::reading(clips_dir: &Path, started: SystemTime) -> DiskReading` — size of the newest file in `clips_dir/Recordings` modified at or after `started` (none → `None`); free bytes of the disk with the longest mount point that `clips_dir` (canonicalized) starts with (none → `None`).
  - `Deps.disk: Box<dyn FnMut(&Path, SystemTime) -> DiskReading + Send>`; `spawn_for_app` passes `disk::reading`; tests pass a fake.
  - `FullRecording` gains `next_disk_check: Option<Instant>`, `warned_low: bool`; set on `FullRecordingStarted` to now + `DISK_CHECK_INTERVAL` (5 s); included in `next_deadline`.
  - `pub const DISK_CHECK_INTERVAL: Duration = 5 s; DISK_WARN_BYTES: u64 = 5_000_000_000; DISK_STOP_BYTES: u64 = 1_000_000_000;`
  - `SupervisorEvent::FullRecordingStatus { size_bytes: Option<u64>, free_bytes: Option<u64> }` → UI event `full-recording-status` `{ sizeBytes, freeBytes }`.
  - Error codes `disk_low` (message built with the free size: "Disk almost full: 4.8 GB left. The recording stops at 1 GB.") and `disk_full_stopped` ("Recording stopped: the disk is almost full"). `emit_error` takes the code only, so add `emit_error_with(code, message)` for the formatted one.
  - Pure: `fn disk_action(free: Option<u64>, warned: bool) -> DiskAction` with `enum DiskAction { Nothing, Warn, Stop }`; `None` → `Nothing` (Review Focus 5).
- Tick: when `next_disk_check <= now` and the full recording is running: read, emit status, act (`Warn` once → `warned_low = true`; `Stop` → `emit_error_with("disk_full_stopped", …)` then the same stop path the toggle uses), reschedule.

- [ ] **Step 1: Failing tests**: `disk_action` table (`None`→Nothing, 6 GB→Nothing, 4 GB→Warn, 4 GB warned→Nothing, 0.9 GB→Stop, 0.9 GB warned→Stop); supervisor tests with the fake clock + fake disk: `full_recording_reports_size_every_5s`, `low_disk_warns_once`, `full_disk_stops_the_recording_cleanly` (asserts the backend got `stop_full_recording` and the `disk_full_stopped` error was emitted), `unknown_free_space_never_stops`.
- [ ] **Step 2:** Run `cargo test … supervisor` → FAIL.
- [ ] **Step 3:** Implement `disk.rs`, the hook and tick logic; a `disk.rs` test with a tempdir: two files in `Recordings/`, only the newer one after `started` is reported.
- [ ] **Step 4:** `cargo test --manifest-path src-tauri/Cargo.toml` → PASS.
- [ ] **Step 5:** Commit `feat(desktop): stop a recording before the disk fills`.

### Task 6: Record button shows the size; roadmap

**Files:**
- Modify: `src/lib/api.ts` (event type `full-recording-status`), `src/views/Library.svelte` (button label), `src/lib/format.ts` (label helper) + `src/lib/format.test.ts`
- Modify: `docs/ROADMAP.md`

**Interfaces:**
- Consumes: `full-recording-status { sizeBytes: number | null; freeBytes: number | null }` (Task 5).
- Produces: `export function recordingLabel(elapsedMs: number, sizeBytes: number | null): string` → `"12:04 · 3.1 GB"`, or just the elapsed part when size is null (uses `formatElapsed`/`formatSize` as the button does now).

- [ ] **Step 1: Failing test** `recordingLabel adds the size when known`: `(724_000, 3_100_000_000)` → contains `"12:04"` and `"3.1 GB"`; `(5_000, null)` has no `"·"`.
- [ ] **Step 2:** `pnpm vitest run src/lib/format.test.ts` → FAIL.
- [ ] **Step 3:** Implement; Library listens to `full-recording-status` (reset size on started/stopped) and renders `Stop · {recordingLabel(...)}`. The disk warnings arrive as `recorder-error` events, already shown as toasts.
- [ ] **Step 4:** `pnpm vitest run` and `pnpm check` → PASS.
- [ ] **Step 5:** Roadmap: add a "Where Klipt is today" row for 2026-11-22 covering all three (untested in the app and on Windows); strike items 1, 3, 4 in "Planned: safety and editor additions" as done 2026-11-22, and update its intro line.
- [ ] **Step 6:** Commit `feat(desktop): show a recording's size` and `docs: roadmap: uploads survive a restart, Copy link, recording safety` (two commits).
