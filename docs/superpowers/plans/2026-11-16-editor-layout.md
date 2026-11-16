# Editor Layout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The editor saves by itself with undo, has Export/Upload/⋯ in the header, a centred transport with frame steps and "Kept", track lanes with waveforms and per-lane volume/mute, shortcut hints, and a thumbnail marker with a preview.

**Architecture:** Pure helpers in `desktop/src/lib` (waveform peaks, edit history, time formatting) with Vitest tests; `Editor.svelte` wires autosave/undo/header/transport; a new `TrackLanes.svelte` lays out the ruler, the video lane (existing `TrimBar`) and audio lanes; the backend only adds `fps` to `EditorData`.

**Tech Stack:** Svelte 5 runes, TypeScript, Vitest; Tauri 2 / Rust for `fps`.

**Spec:** `docs/superpowers/specs/2026-11-16-editor-layout-design.md`

## Global Constraints

- Keep Klipt's look: app.css tokens (`--surface*`, `--border`, `--text*`, `--accent*`, `--radius*`), existing `.btn` classes and `Icon`. No new fonts or colours except the thumbnail marker's yellow (`--warning` `#ffb547` is fine).
- WebKitGTK (Linux) and WebView2 (Windows).
- Autosave debounce ~600 ms; leaving the editor flushes a pending save first. Undo/redo cover edits only (trim, cuts, volumes, mute, blur, thumbnail), never delete/upload/game.
- Shortcuts: Space play/pause, S split, ←/→ one frame, Shift+←/→ one second, Ctrl+Z undo, Ctrl+Shift+Z and Ctrl+Y redo — ignored while typing in an input or with a dialog open (same guard as today's Space/S).
- Frame step = 1000 / fps ms (fps from `EditorData.fps`, 60 when 0/unknown).
- Time format in the transport: `m:ss.t / m:ss`; "Kept m:ss".
- Verification per task: from `desktop/` `npm test` and `npm run check` (0 errors, 0 warnings); Rust tasks also `cargo test` from `desktop/src-tauri` (the two export nvenc tests may fail under parallel load — pre-existing). Do not launch the app.

## Review Focus

1. Leaving the editor (back, rail tab, logout, window close) right after a change: the change is saved, not lost.
2. Undo after a drag: one Ctrl+Z restores the state before the whole drag, not one pixel back.
3. A clip with only a game track, and an imported clip with extra tracks: lanes match the tracks that exist; failed-to-load tracks show a lane without waveform, not a crash.
4. Shortcuts while typing in the "Set game" dialog or the upload dialog's title field: no seeking, no undo of edits.
5. Save failing (e.g. the clip is uploading → busy): status shows the failure with Retry; nothing is silently dropped.

---

### Task 1: `fps` in EditorData

**Files:** Modify `desktop/src-tauri/src/ffmpeg.rs` (`Probe` gains `fps: f64`, filled like the existing `avg_frame_rate` parsing near L113–170), `desktop/src-tauri/src/commands.rs` (`EditorData.fps: f64`), `desktop/src/lib/api.ts` (`EditorData.fps: number`).
**Interfaces:** Produces `EditorData.fps` (0 when unknown).

- [ ] Step 1: failing Rust test `probe_reports_fps`: probing `crate::ffmpeg::tests::fixture()` gives `fps` within 0.01 of the fixture's rate (read it from the fixture's ffmpeg command in `ffmpeg.rs` tests).
- [ ] Step 2: run `cargo test probe_reports_fps` → FAIL. Step 3: implement. Step 4: `cargo test`, `npm run check` → pass.
- [ ] Step 5: commit `feat(desktop): the editor knows the clip's frame rate`

### Task 2: Waveform peaks

**Files:** Create `desktop/src/lib/waveform.ts`, `desktop/src/lib/waveform.test.ts`; modify `desktop/src/lib/audioEngine.ts`.
**Interfaces:**
- `peaks(channels: Float32Array[], buckets: number): Float32Array` — per bucket the max absolute sample over all channels (0–1); `buckets` ≤ 0 or no samples → empty array.
- `AudioEngine.peaks(key: string, buckets: number): Float32Array | null` — null for a key not loaded; computed from the track's `AudioBuffer` channels and cached per (key, buckets).

- [ ] Step 1: failing tests: `peaks([new Float32Array([0, .5, -1, .25])], 2)` → `[.5, 1]`; two channels take the max of both; `peaks([], 4)` and `peaks([x], 0)` → length 0; buckets > samples still returns `buckets` values (empty buckets 0). Engine: `peaks("game", 2)` from a fake buffer with `getChannelData`; unknown key → null.
- [ ] Step 2: RED. Step 3: implement. Step 4: GREEN + `npm test`, `npm run check`.
- [ ] Step 5: commit `feat(desktop): waveform peaks from the decoded audio`

### Task 3: Edit history (undo/redo)

**Files:** Create `desktop/src/lib/editHistory.ts`, `desktop/src/lib/editHistory.test.ts`.
**Interfaces:** `class EditHistory<T>` with `constructor(initial: T, coalesceMs = 400, now = () => performance.now())`; `record(state: T)` (pushes; a record within `coalesceMs` of the previous one replaces the newest step instead of adding one); `undo(): T | null`; `redo(): T | null`; `canUndo`, `canRedo` getters; recording after an undo drops the redo steps; equal consecutive states (deep-equal via JSON) aren't recorded.

- [ ] Step 1: failing tests for: undo returns the previous state; a burst of records within 400 ms is one undo step; redo after undo; record after undo clears redo; equal state not recorded; undo at the start returns null.
- [ ] Step 2: RED. Step 3: implement. Step 4: GREEN.
- [ ] Step 5: commit `feat(desktop): undo history for editor changes`

### Task 4: Autosave, undo and the header

**Files:** Modify `desktop/src/views/Editor.svelte`, `desktop/src/App.svelte` (the editor no longer binds `dirty`; Settings keeps its guard).
**Interfaces:** Consumes `EditHistory` (Task 3); existing `api.saveEdit`, `api.showInFolder`.

- [ ] Step 1: Autosave: when the edit (incl. thumbnail) differs from the last saved one, save ~600 ms after the last change; status in the header: "Saving…", "✓ Saved", or "Couldn't save" + Retry button (toast too). On destroy, a pending or failed change is saved immediately (fire-and-forget is fine; log failure). Remove the "Save edit" button, the "Unsaved changes" label and the editor's `dirty` binding (App's "Discard changes?" stays for Settings only).
- [ ] Step 2: Undo/redo: record the edit state on every change into `EditHistory`; Ctrl+Z / Ctrl+Shift+Z / Ctrl+Y apply it (restoring trim, pieces, volumes, blur, thumbnail), guarded like today's Space/S. The applied state autosaves like any change.
- [ ] Step 3: Header right side: save status, **Export…** (existing export action), **Upload** (primary, existing action), **⋯** menu (pattern of App.svelte's account menu with `use:menu`) with **Show in folder** (`api.showInFolder(clip.id)`) and **Delete** (existing delete flow). The old Delete button and the bottom actions row go.
- [ ] Step 4: `npm test`, `npm run check` → pass. Commit `feat(desktop): the editor saves by itself, with undo; actions move to the header`

### Task 5: Transport and shortcuts

**Files:** Create `desktop/src/lib/timecode.ts` + test; modify `desktop/src/views/Editor.svelte`.
**Interfaces:** `formatTenths(ms: number): string` → `m:ss.t` (`formatTenths(12_440) === "0:12.4"`, `formatTenths(61_000) === "1:01.0"`, negative → `0:00.0`); `stepMs(fps: number): number` → `1000 / (fps > 0 ? fps : 60)`.

- [ ] Step 1: failing tests for both helpers. Step 2: RED. Step 3: implement.
- [ ] Step 4: Transport row centred under the video: frame back, play/pause (as today), frame forward, `formatTenths(position) / formatClock(duration)`, "Kept " + `formatClock(keptLength)`, Split (S). Frame/second steps move the playhead via `seek(position ± step)` (clamped to 0–duration); while playing they seek and keep playing.
- [ ] Step 5: Keyboard: ←/→ = one frame, Shift+←/→ = one second, same guards as Space/S. Hints row under the lanes (Task 6 places it; here add the markup at the end of the timeline section): `Space play · S split · ←/→ one frame · Shift+←/→ one second · Ctrl+Z undo` with `<kbd>`.
- [ ] Step 6: `npm test`, `npm run check`. Commit `feat(desktop): centred transport with frame steps, kept length and shortcuts`

### Task 6: Track lanes

**Files:** Create `desktop/src/components/TrackLanes.svelte`; modify `desktop/src/views/Editor.svelte`, `desktop/src/components/TrimBar.svelte` (only what's needed to sit inside a lane).
**Interfaces:** `TrackLanes` props: `duration`, `trim`, `pieces`, `position`, `tracks: { key: string; name: string; peaks: Float32Array | null; volume: number; muted: boolean }[]`, `master: number`, `blur: boolean`, callbacks `onvolume(key, v)`, `onmute(key)`, `onmaster(v)`, `onblur(b)`, `onseek(ms)`, `onscrubstart()`, `onscrubend()`, and a `video` snippet slot rendering the existing `<TrimBar>`.

- [ ] Step 1: Layout per spec §4: a 170 px label column + lanes column; a ruler (labels every 10 s, or 5 s under 30 s, 30 s over 3 min); Video lane = the TrimBar slot with a Blur toggle in its label; one audio lane per track with a canvas waveform (peaks from `engine.peaks(key, width)` once audio is ready; redraw on resize), label with mute button, volume slider (the existing `.range` style) and value; Master row with the master slider. Trim dimming and removed-part shading drawn on every lane at the same positions as the video lane; playhead line across all lanes.
- [ ] Step 2: Mute = volume 0 that remembers the previous level (unmute restores it; a track at 0 shows as muted). It's saved as the volume like today.
- [ ] Step 3: Pointer down/drag on an audio lane seeks and scrubs exactly like the video lane (same `onscrubstart`/`onscrubend`, pointer capture, `preventDefault`).
- [ ] Step 4: The old mixer panel and blur toggle below the timeline go.
- [ ] Step 5: `npm test`, `npm run check`. Commit `feat(desktop): the editor's timeline shows a lane per track with its waveform`

### Task 7: Thumbnail marker preview

**Files:** Modify `desktop/src/components/TrimBar.svelte`.

- [ ] Step 1: The thumbnail marker is yellow (`--warning`). While it's hovered or dragged, a small preview (≈120 px wide) of the filmstrip picture at its time (`previewIndex`/`frameUrl` from `lib/filmstrip.ts`; skipped if not ready) with "Thumbnail · m:ss" shows above it, kept inside the timeline's width.
- [ ] Step 2: `npm test`, `npm run check`. Commit `feat(desktop): the thumbnail marker shows a preview of its frame`

### Task 8: Check in the app (owner)

- [ ] With the owner: open clips (one with mic + voice, one imported with extra tracks), edit, leave and come back (changes kept), undo after a drag, use every shortcut, check waveforms line up with the sound.
