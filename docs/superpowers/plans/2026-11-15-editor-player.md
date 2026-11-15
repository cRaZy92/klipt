# Editor Player Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The editor plays all audio through Web Audio on one clock, pauses while the timeline is held, previews frames instantly from a filmstrip while dragging, and shows that filmstrip on the track.

**Architecture:** Backend (Rust): a `filmstrip` module extracts ~10 JPEGs/s per clip into the editor cache at low priority while the clip's editor is open, keeps all strips under a configurable cap (least recently opened go first), and editor leftovers are deleted after an upload. Frontend (Svelte 5): pure scheduling/follow/filmstrip helpers with tests, a thin `AudioEngine` over `AudioContext`, and `Editor.svelte`/`TrimBar.svelte` rewired to them; the `<audio>` elements and `audioSync.ts` go.

**Tech Stack:** Svelte 5 runes, TypeScript, Vitest; Tauri 2, Rust (rusqlite, tempfile tests), ffmpeg; Web Audio API.

**Spec:** `docs/superpowers/specs/2026-11-15-editor-player-design.md`

## Global Constraints

- Runs on WebKitGTK 2.54 (Linux) and WebView2 (Windows).
- The picture shows as soon as the video loads, as today; audio decoding and the filmstrip never delay it.
- Filmstrip: 10 pictures per second (`FRAMES_PER_SECOND = 10`, one every 100 ms), 640 px wide, JPEG, in `<cache>/edit/<id>-strip/`.
- Extraction: lowest CPU priority, CPU decode only (no `-hwaccel`), runs only while that clip's editor is open, resumes a partial strip.
- Setting `editorCacheGb` (Rust `editor_cache_gb: f32`): default `2.0`, valid `0.5..=100.0`; error copy: "Keep 0.5 to 100 GB of editor previews."; shown in Settings → Clips folder as "Editor preview cache".
- Eviction: whole strips, least recently opened first, never the strip of the clip being edited.
- After a successful upload: delete `<cache>/edit/<id>-*` (audio copies and strip). Never the clip's original file.
- The original clip file is never modified.
- Pointer-rest delay before an exact seek while dragging: 150 ms.
- Commands from `desktop/`: `npm test`, `npm run check`; from `desktop/src-tauri/`: `cargo test`.

## Review Focus

1. Opening another clip (or closing the editor) while a strip is being extracted: the old ffmpeg stops, nothing is left half-written as "complete", and the next open resumes. → Task 2 test `extract_resumes_after_stop`, Task 4 `close_editor`.
2. The strip of the clip being uploaded or evicted disappears while its editor is open: scrubbing falls back to seeking, no broken images. → Task 7 Step 4 (`onerror` → treat as not ready).
3. A track fails to decode (corrupt/missing m4a): the others still play, a toast says which. → Task 5 test `a track that fails to load is skipped`.
4. Pointer released outside the window / pointer cancel during a drag: playback resumes as if released. → Task 7 Step 1 (`onpointercancel` → release).
5. Play pressed before audio is decoded, or a clip with removed parts that ends in a removed part: no sound from removed parts, loop restarts at the first kept part. → Task 5 tests `schedule skips removed parts` / `loops from the first kept part`.

---

### Task 1: Filmstrip storage and eviction (Rust)

**Files:**
- Create: `desktop/src-tauri/src/filmstrip.rs` (add `pub mod filmstrip;` in `lib.rs`)
- Modify: `desktop/src-tauri/src/editor.rs` (`remove_edit_cache` also removes the strip dir)

**Interfaces:**
- Produces:
  - `pub const FRAMES_PER_SECOND: u32 = 10;` `pub const WIDTH: u32 = 640;`
  - `pub fn strip_dir(cache_dir: &Path, id: &str) -> PathBuf` → `<cache>/edit/<id>-strip`
  - `pub fn frame_path(dir: &Path, index: u32) -> PathBuf` → `<dir>/<index padded to 6>.jpg` (index 0 = 0 ms)
  - `pub fn expected_frames(duration_ms: u64) -> u32` → `ceil(duration_ms / 100)`
  - `pub fn ready_frames(dir: &Path) -> u32` — count of consecutive frames from 0; if no `complete` marker file exists, the highest one is not counted (it may be half-written)
  - `pub fn is_complete(dir: &Path) -> bool` — `<dir>/complete` exists
  - `pub fn mark_opened(dir: &Path)` — creates/touches `<dir>/opened` (its mtime = last open)
  - `pub struct StripInfo { pub id: String, pub bytes: u64, pub opened: SystemTime }`
  - `pub fn choose_evictions(strips: &[StripInfo], cap_bytes: u64, keep_id: &str) -> Vec<String>` — pure
  - `pub fn evict(cache_dir: &Path, cap_bytes: u64, keep_id: &str)` — lists strips, calls `choose_evictions`, deletes their dirs (best effort, logs errors)

- [ ] **Step 1: Write failing tests** (in `filmstrip.rs` `#[cfg(test)] mod tests`):
  - `expected_frames_rounds_up`: `expected_frames(0) == 0`, `(1) == 1`, `(100) == 1`, `(101) == 2`, `(67_000) == 670`.
  - `ready_frames_skips_the_last_until_complete`: frames {0,1,2,3,4}, no marker → `4`; with `complete` → `5`; frames {0,1,3}, no marker → `1` (consecutive run 0–1, its last not counted).
  - `choose_evictions_drops_least_recently_opened_until_under_cap`: strips a(100, t1) b(100, t3) c(100, t2), cap 150, keep "" → `["a", "c"]`.
  - `choose_evictions_never_drops_the_kept_strip`: a(300, t1) kept, b(100, t2), cap 150 → `["b"]`.
  - `choose_evictions_none_under_cap`: total 200, cap 200 → `[]`.
  - `remove_edit_cache_removes_the_strip` (in `editor.rs` tests): create strip dir with a frame → after `remove_edit_cache` it's gone.
- [ ] **Step 2: Run** `cargo test filmstrip remove_edit_cache` — expected: FAIL (module missing).
- [ ] **Step 3: Implement** the interfaces above. A strip is any dir in `<cache>/edit/` named `<id>-strip`; `bytes` sums its files; `opened` is `opened`'s mtime (fallback: dir mtime).
- [ ] **Step 4: Run** `cargo test filmstrip remove_edit_cache` — expected: all pass.
- [ ] **Step 5: Commit** `feat(desktop): filmstrip storage and least-recently-opened eviction`

### Task 2: Filmstrip extraction (Rust)

**Files:**
- Modify: `desktop/src-tauri/src/filmstrip.rs`, `desktop/src-tauri/Cargo.toml` (`[target.'cfg(unix)'.dependencies] libc = "0.2"`)

**Interfaces:**
- Consumes: Task 1 paths/counters; `crate::ffmpeg::Tools`; test helpers `crate::ffmpeg::tests::{tools, fixture}` (6 s clip).
- Produces: `pub fn extract(tools: &Tools, input: &Path, dir: &Path, duration_ms: u64, stop: &AtomicBool, progress: impl FnMut(u32)) -> Result<()>`
  - starts at `ready_frames(dir)` (deleting a not-counted trailing frame first): ffmpeg `-ss <start s> -i <input> -an -vf fps=10,scale=640:-2 -q:v 5 -start_number <start> -threads 2 -progress pipe:1 <dir>/%06d.jpg`, no `-hwaccel`;
  - lowest priority: Unix `pre_exec` → `libc::setpriority(PRIO_PROCESS, 0, 19)`; Windows creation flags `IDLE_PRIORITY_CLASS (0x40) | CREATE_NO_WINDOW`;
  - reports `progress(ready count)` from `out_time_us`; checks `stop` per progress line and kills ffmpeg when set (returns `Err(ToolFailed{ detail: "stopped" })` like `write_mp4_progress`);
  - on success writes `complete`.

- [ ] **Step 1: Write failing tests:**
  - `extract_writes_ten_frames_per_second`: fixture (6 s) → `ready_frames == expected_frames(6000) == 60`, `is_complete`, `frame_path(dir, 0)` is a JPEG 640 px wide (check with `ffmpeg::probe` or image header width).
  - `extract_resumes_after_stop`: set `stop` from the progress callback once count ≥ 10 → returns Err, not complete, `ready_frames` ≥ 10 and < 60; run again without stop → complete with 60 frames, and frame 0's mtime unchanged (not re-extracted).
- [ ] **Step 2: Run** `cargo test filmstrip::tests::extract` — expected: FAIL.
- [ ] **Step 3: Implement** `extract`.
- [ ] **Step 4: Run** `cargo test filmstrip` — expected: pass.
- [ ] **Step 5: Commit** `feat(desktop): extract filmstrips at low priority, resuming partial ones`

### Task 3: "Editor preview cache" setting

**Files:**
- Modify: `desktop/src-tauri/src/settings.rs` (field, default, `validate`, `with_invalid_fields_reset`), `desktop/src/lib/api.ts` (`Settings.editorCacheGb: number`), `desktop/src/lib/settings.ts` (`EDITOR_CACHE_GB_MIN = 0.5`, `EDITOR_CACHE_GB_MAX = 100`, validation), `desktop/src/views/Settings.svelte` (Clips folder tab: number input, step 0.5, label "Editor preview cache", help "Pictures for scrubbing in the editor. The ones you opened longest ago are deleted first.", suffix "GB")
- Test: `settings.rs` tests, `desktop/src/lib/settings.test.ts`

**Interfaces:**
- Produces: `Settings.editor_cache_gb: f32` (serde `editorCacheGb`), `pub fn editor_cache_bytes(&self) -> u64`.

- [ ] **Step 1: Write failing tests:** Rust `editor_cache_defaults_and_range` (default 2.0; 0.4 and 100.5 rejected with field `editorCacheGb`; an old settings file without the field loads 2.0; a file with 0.1 loads reset to 2.0); TS `fields({ editorCacheGb: 0.4 })` and `(100.5)` contain `"editorCacheGb"`, `(0.5)` and `(100)` don't. Add `editorCacheGb: 2` to the test `defaults`.
- [ ] **Step 2: Run** `cargo test settings` and `npx vitest run src/lib/settings.test.ts` — expected: FAIL.
- [ ] **Step 3: Implement** field, validation, UI.
- [ ] **Step 4: Run** both test commands and `npm run check` — expected: pass, 0 errors.
- [ ] **Step 5: Commit** `feat(desktop): editor preview cache setting`

### Task 4: Editor commands: strip job, cleanup after upload, CSP

**Files:**
- Modify: `desktop/src-tauri/src/commands.rs` (`open_editor`, new `close_editor`, `EditorData`, upload success branch ~L610, events), `desktop/src-tauri/src/filmstrip.rs` (job registry), `desktop/src-tauri/src/lib.rs` (manage `filmstrip::Jobs`, register `close_editor`), `desktop/src-tauri/tauri.conf.json` (CSP add `connect-src 'self' http://127.0.0.1:*`), `desktop/src/lib/api.ts`

**Interfaces:**
- Consumes: Tasks 1–3.
- Produces:
  - `filmstrip::Jobs` (managed state): `pub fn start(&self, id: String, run: impl FnOnce(&AtomicBool) + Send + 'static)` — stops any running job first, runs `run` on a blocking thread; `pub fn stop(&self, id: &str)`; `pub fn stop_all(&self)`.
  - `EditorData.filmstrip: FilmstripInfo { dir: PathBuf, frames: u32, total: u32 }` (TS: `filmstrip: { dir: string; frames: number; total: number }`).
  - Event `filmstrip-progress` `{ id: string, frames: number }` (at most every 250 ms, and at the end).
  - Command `close_editor(id: String)` → stops that clip's job. TS `api.closeEditor(id)`.

- [ ] **Step 1: Write failing test** `choose_after_open_keeps_current` is covered by Task 1; here add Rust test `upload_cleanup_keeps_the_original` in `editor.rs`: a clip file inside a temp "clips" dir plus `<cache>/edit/<id>-game.m4a` and a strip → `editor::remove_edit_cache` (the function the upload branch calls) leaves the clip file and removes the rest.
- [ ] **Step 2: Run** `cargo test upload_cleanup` — expected: FAIL until the strip removal from Task 1 + this wiring exist (if it passes already, the test documents it; note it in the ledger).
- [ ] **Step 3: Implement:**
  - `open_editor`: `mark_opened(dir)`; `evict(cache, settings.editor_cache_bytes(), &id)`; allow `dir` in the asset scope (`allow_directory(dir, false)`); return `FilmstripInfo { frames: ready_frames, total: expected_frames(duration_ms) }`; if not complete, `Jobs::start` an `extract` that emits `filmstrip-progress` and, when done, runs `evict` again.
  - `close_editor`: `Jobs::stop(id)`.
  - Upload success branch: `Jobs::stop(&id)` then `editor::remove_edit_cache(&cache_dir, &id)`.
  - CSP: `connect-src 'self' http://127.0.0.1:*`.
- [ ] **Step 4: Run** `cargo test` and `npm run check` — expected: pass.
- [ ] **Step 5: Commit** `feat(desktop): filmstrips are made while a clip is open and cleaned after upload`

### Task 5: Playback scheduling and the audio engine (frontend)

**Files:**
- Create: `desktop/src/lib/playSchedule.ts`, `desktop/src/lib/playSchedule.test.ts`, `desktop/src/lib/audioEngine.ts`, `desktop/src/lib/audioEngine.test.ts`

**Interfaces:**
- Consumes: `CutRange`-like `{ start: number; end: number }` (ms) from `lib/cuts.ts` `keptRanges`.
- Produces:
  - `type Segment = { at: number; from: number; to: number }` — `at` seconds after play starts, `from`/`to` ms in the clip.
  - `schedule(fromMs: number, kept: Range[]): Segment[]` — from `fromMs` to the end of the last kept range, back to back (`at` accumulates kept durations); a `fromMs` inside a removed part starts at the next kept range; at/after the last kept end starts at `kept[0].start`.
  - `clipMsAt(elapsedS: number, segments: Segment[]): number | null` — null after the last segment (the caller loops).
  - `videoCorrection(videoMs: number, audioMs: number): { rate: number } | { seekMs: number } | null` — null within 80 ms; `rate` 1.03/0.97 between 80 and 500 ms (video behind → faster); `seekMs: audioMs` beyond 500 ms.
  - `class AudioEngine` (constructor takes a context factory, default `() => new AudioContext()`): `load(tracks: { key: string; url: string }[]): Promise<string[]>` (returns keys that failed), `setGain(key: string, gain: number)`, `setMaster(gain: number)`, `play(fromMs: number, kept: Range[])`, `stop()`, `positionMs(): number | null`, `readonly playing: boolean`, `close()`.

- [ ] **Step 1: Write failing tests:**
  - `schedule plays straight through without cuts`: `schedule(1000, [{start:0,end:5000}])` → `[{at:0, from:1000, to:5000}]`.
  - `schedule skips removed parts`: kept `[{0,2000},{3000,5000}]`, from 1500 → `[{at:0,from:1500,to:2000},{at:0.5,from:3000,to:5000}]`.
  - `schedule starts at the next kept part from inside a removed one`: from 2500 → first segment `{at:0, from:3000, to:5000}`.
  - `loops from the first kept part`: from 5000 → first segment `from: 0`.
  - `clipMsAt maps elapsed time across a skip`: with the skip schedule, `clipMsAt(0.75, s) === 3250`, `clipMsAt(3, s) === null`.
  - `videoCorrection`: `(1000, 1050)` → null; `(1000, 1200)` → `{rate:1.03}`; `(1200, 1000)` → `{rate:0.97}`; `(0, 1000)` → `{seekMs:1000}`.
  - `AudioEngine` with a fake context (records `createBufferSource().start(when, offset, duration)` calls and gain values; `decodeAudioData` resolves a fake buffer, or rejects for url "bad"): `a track that fails to load is skipped` (`load` returns `["mic"]`, play starts only game's sources); `play schedules one source per track per segment` (2 tracks × 2 segments = 4 starts with offsets 1.5/3.0 s); `setGain and setMaster set gain values` (track gain 0.5, master 0.8); `stop stops every source`.
- [ ] **Step 2: Run** `npx vitest run src/lib/playSchedule.test.ts src/lib/audioEngine.test.ts` — expected: FAIL.
- [ ] **Step 3: Implement.** `load` fetches each URL (`fetch(url).then(r => r.arrayBuffer())`) and `decodeAudioData`s them in parallel. `play` resumes the context, creates sources per track per segment at `ctx.currentTime + 0.05 + seg.at`; `positionMs` = `clipMsAt(ctx.currentTime - startedAt, segments)`.
- [ ] **Step 4: Run** the same command — expected: pass.
- [ ] **Step 5: Commit** `feat(desktop): Web Audio playback engine with exact skips over removed parts`

### Task 6: Editor plays through the engine

**Files:**
- Modify: `desktop/src/views/Editor.svelte`
- Delete: `desktop/src/lib/audioSync.ts`, `desktop/src/lib/audioSync.test.ts`

**Interfaces:**
- Consumes: `AudioEngine`, `schedule`, `videoCorrection` (Task 5); `api.closeEditor` (Task 4).

- [ ] **Step 1:** Replace the `<audio>` elements, `audioSync`, the drift interval, `syncAudio`/`playAudio`/`pauseAudio` with one `AudioEngine`:
  - on load: `engine.load([game, mic?, voice?, extras…])`; a toast "Couldn't load the <name> track" per failed key; `audioReady` state; the play button is disabled with title "Loading audio…" until ready.
  - sliders: `$effect`s call `engine.setGain(key, v/100)` and `engine.setMaster(master/100)`.
  - play: `engine.play(position, kept)` and `video.play()`; pause/seek: `engine.stop()` (and restart from the new position if playing).
  - the rAF loop: while playing, `position = engine.positionMs()`; null → loop (`engine.play(kept[0].start, kept)` and seek the video); otherwise apply `videoCorrection(video.currentTime*1000, position)` (`playbackRate` or `seek`), and seek the video when the engine crosses into a new segment (segment `from` ≠ previous segment's `to`).
  - `onloadedmetadata`: disable the video's audio tracks if `video.audioTracks` exists (`for (const t of video.audioTracks) t.enabled = false`); keep `muted`.
  - on destroy: `engine.close()`, `api.closeEditor(clip.id)`.
  - also: keep the freeze-frame overlay and queued seeks for the video.
- [ ] **Step 2: Verify** `npm test` (audioSync tests gone, rest pass) and `npm run check` — 0 errors; `grep -rn "audioSync\|<audio" src` — no matches.
- [ ] **Step 3: Commit** `feat(desktop): the editor plays all audio through one Web Audio clock`

### Task 7: Scrubbing: hold pauses, filmstrip preview while dragging

**Files:**
- Create: `desktop/src/lib/filmstrip.ts`, `desktop/src/lib/filmstrip.test.ts`
- Modify: `desktop/src/components/TrimBar.svelte`, `desktop/src/views/Editor.svelte`

**Interfaces:**
- Consumes: `EditorData.filmstrip`, event `filmstrip-progress` (Task 4); `fileUrl` (api.ts).
- Produces:
  - `frameIndex(ms: number): number` → `Math.round(ms / 100)` clamped ≥ 0.
  - `frameUrl(dir: string, index: number): string` → `fileUrl(`${dir}/${String(index).padStart(6, "0")}.jpg`)` (path join with the platform separator the dir already uses).
  - `previewIndex(ms: number, ready: number): number | null` → `frameIndex(ms)` if `< ready`, else null.
  - TrimBar props `onscrubstart?: () => void`, `onscrubend?: () => void` — called when any pointer grab starts (track, handle, split) and on `release`/`pointercancel`.

- [ ] **Step 1: Write failing tests** `filmstrip.test.ts`: `frameIndex(0)=0, (149)=1, (150)=2` (round), `(-5)=0`; `previewIndex(1000, 11) === 10`, `previewIndex(1000, 10) === null`; `frameUrl("/c/edit/a-strip", 7)` ends with `000007.jpg`.
- [ ] **Step 2: Run** `npx vitest run src/lib/filmstrip.test.ts` — expected: FAIL.
- [ ] **Step 3: Implement** `filmstrip.ts`; TrimBar calls `onscrubstart` in `grabTrack`/`grabHandle`/`grabSplit` and `onscrubend` in `release` and a new `onpointercancel={release}` on the same elements. Editor:
  - `onscrubstart`: `wasPlaying = playing`; pause (engine + video).
  - while scrubbing, a move sets `previewAt = ms`; if `previewIndex(ms, stripFrames)` is a number, show that picture in an `<img class="scrub">` over the video (no video seek); else seek as today. After 150 ms without a move, and on `onscrubend`, seek the video to the exact frame; the `<img>` hides once that frame is painted (reuse `unfreezeAfterPaint`).
  - `onscrubend`: if `wasPlaying`, play from `position`.
  - `stripFrames` starts at `data.filmstrip.frames`, updated by `filmstrip-progress` for this clip; an `<img>` `onerror` sets `stripFrames = 0` (strip deleted).
- [ ] **Step 4: Run** `npm test` and `npm run check` — expected: pass, 0 errors.
- [ ] **Step 5: Commit** `feat(desktop): holding the timeline pauses; dragging previews from the filmstrip`

### Task 8: Filmstrip on the track

**Files:**
- Modify: `desktop/src/lib/filmstrip.ts`, `desktop/src/lib/filmstrip.test.ts`, `desktop/src/components/TrimBar.svelte`

**Interfaces:**
- Produces: `tiles(trackWidthPx: number, tileWidthPx: number, durationMs: number): { leftPct: number; widthPct: number; ms: number }[]` — `ceil(trackWidth / tileWidth)` tiles, each `ms` = its middle's time. TrimBar prop `strip?: { dir: string; ready: number } | null`.

- [ ] **Step 1: Write failing test** `tiles`: `tiles(1000, 100, 10_000)` → 10 tiles, first `{leftPct:0, widthPct:10, ms:500}`, last `ms: 9500`; `tiles(0, 100, 10_000)` → `[]`.
- [ ] **Step 2: Run** `npx vitest run src/lib/filmstrip.test.ts` — expected: FAIL.
- [ ] **Step 3: Implement** `tiles`; TrimBar measures the track (`bind:clientWidth`), tile width = track height × 16/9, renders `<img>`s for tiles whose `previewIndex(ms, ready)` is a number, behind `.dim`, `.selection`, cuts, splits and playhead (no pointer events, `loading="lazy"`, `decoding="async"`).
- [ ] **Step 4: Run** `npm test` and `npm run check` — expected: pass.
- [ ] **Step 5: Commit** `feat(desktop): the editor's track shows the clip as a filmstrip`

### Task 9: Check on the owner's PC

- [ ] **Step 1:** With `pnpm tauri dev` running, ask the owner to open a clip, play, move sliders, drag the playhead and hold it still, then close. Meanwhile log Klipt's PipeWire streams (`pactl list sink-inputs`, as on 2026-11-15). Pass: one Klipt playback stream at 100% while playing (no muted video stream), sliders change loudness, holding pauses, the strip fills in on the track.
- [ ] **Step 2:** If a stream for the video still exists: implement the spec's fallback (media server serves the editor's video without audio) as its own commit with a Rust test in `media_server.rs`, then repeat Step 1.
