# Editor Cuts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the user split a clip in the editor, switch pieces off, preview only the kept pieces, and upload/export them joined with frame-exact cuts.

**Architecture:** The library stores the removed ranges (JSON column) next to the trim. Export keeps today's stream-copy path when there are no removed ranges; with removed ranges it trims each kept interval with `trim`/`atrim`, joins them with `concat` and re-encodes the video with the detected encoder. The editor models the timeline as contiguous pieces (pure functions in `src/lib/cuts.ts`) that `TrimBar` draws and `Editor` uses for playback skipping.

**Tech Stack:** Rust (Tauri 2, rusqlite, ffmpeg CLI), Svelte 5 runes, TypeScript, vitest.

**Spec:** `docs/superpowers/specs/2026-11-14-editor-cuts-design.md`

## Global Constraints

- Clips **without** removed ranges export exactly as today (`-c:v copy`); existing `export_args` tests must stay green unchanged.
- Minimum piece length: `MIN_TRIM_MS` = 500 ms (from `src/lib/trim.ts`).
- Removed pieces drawn red-hatched with `--danger` (`#ff5d6c`).
- Playback jumps over a removed piece with exactly one `seek()`.
- Dragging a trim handle over splits/removed pieces hides them; they come back when dragged back. Only the saved ranges are clipped to the trim.
- **Never commit `desktop/src/views/Editor.svelte`'s uncommitted player-trace code** (`trace(`, `copyTrace`, `traceMedia`, `rejected(`). Commit Editor.svelte with `git add -p`, leaving those hunks unstaged.
- Commits end without extra attribution lines (none required for commits).

## Review Focus

1. Everything inside the trim switched off → Save/Export/Upload must refuse ("Keep at least part of the clip"), not produce an empty file. Tested in Task 1 (`validate_edit`) and Task 3 (`canRemove`).
2. A removed range touching the trim start or end (e.g. first piece off) → export starts at the first kept frame, no black lead-in; thumbnail time maps correctly. Tested in Task 2.
3. Saved clip reopened → pieces rebuilt from ranges show the same removed parts; split lines between two kept pieces are not restored (accepted). Tested in Task 3 (`piecesFromRanges`).
4. Playback started from inside a removed piece, or the playhead dragged into one → plays from the next kept time; after the last kept piece loops to the first. Tested in Task 3 (`playFrom`).
5. Thumbnail picked earlier sits in a piece later switched off → clamped to the nearest kept time on save and upload. Tested in Task 2 and Task 3 (`clampToKept`).

---

### Task 1: Store removed ranges with the edit

**Files:**
- Modify: `desktop/src-tauri/src/library/mod.rs` (new `CutRange`, `Clip.removed_ranges`)
- Modify: `desktop/src-tauri/src/library/db.rs` (migration, COLUMNS, row mapping, `update_edit`, `validate_edit`, `pending_syncs` column index)
- Modify: `desktop/src-tauri/src/export.rs` (`Edit.removed`), `editor.rs` (`current_edit`, `save_edit`), `commands.rs` (`edit_from`, `save_edit` param), `recorder/supervisor.rs` (any `Clip` literal)
- Modify: `desktop/src/lib/api.ts` (`ClipView.removedRanges`, `EditInput.removedRanges`)

**Interfaces:**
- Produces (Rust): `#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)] #[serde(rename_all = "camelCase")] pub struct CutRange { pub start_ms: u64, pub end_ms: u64 }` in `library/mod.rs`; `Clip.removed_ranges: Vec<CutRange>`; `Edit.removed: Vec<CutRange>`; `Library::update_edit(&self, id, trim: Trim, volumes: &Volumes, removed: &[CutRange])`; `validate_edit(trim: Trim, volumes: &Volumes, removed: &[CutRange], duration_ms: u64) -> Result<()>`.
- Produces (TS): `export interface CutRange { startMs: number; endMs: number }`; `ClipView.removedRanges: CutRange[]`; `EditInput.removedRanges: CutRange[]`. `api.saveEdit` passes it through unchanged (spread already does).
- Tauri command `save_edit` gains `removed_ranges: Option<Vec<CutRange>>` (missing = none).

- [ ] **Step 1: Failing tests in `library/db.rs` tests**
  - `removed_ranges_round_trip`: `update_edit(id, trim(1000, 9000), &Volumes::default(), &[r(2000,3000), r(5000,6000)])` then `get(id).removed_ranges == [r(2000,3000), r(5000,6000)]`; a fresh clip has `removed_ranges == []`.
  - `validate_edit_rejects_bad_cuts`: each is `Err(InvalidInput)` with duration 10_000, trim 1000..9000: unsorted `[r(5000,6000), r(2000,3000)]`; overlapping `[r(2000,4000), r(3000,5000)]`; empty `r(3000,3000)`; outside trim `r(500,2000)` and `r(8000,9500)`; covering everything `[r(1000,9000)]`. And `Ok` for `[r(1000,2000), r(8000,9000)]` (touching the edges) and for no trim `(None, None)` with `[r(0, 500)]`.
- [ ] **Step 2:** `cargo test -p klipt-desktop removed_ranges validate_edit_rejects` → fails to compile / FAIL.
- [ ] **Step 3: Implement.** Append migration `ALTER TABLE clips ADD COLUMN removed_ranges TEXT NOT NULL DEFAULT '[]';` to `MIGRATIONS`. Append `removed_ranges` to `COLUMNS` (after `blur_video`), parse with `serde_json` (bad JSON → empty vec, like `extra_volumes`). **Shift `pending_syncs`'s `row.get::<_, String>(28)` to the new index of `sync_pending`.** `validate_edit` takes `duration_ms` (the clip's) so a `None` trim end means `duration_ms`; ranges must be sorted, non-overlapping, `start < end`, inside `[trim start, trim end]`, and leave > 0 kept ms. `update_edit` fetches the clip's duration for that, writes the JSON. Thread `removed` through `Edit`, `current_edit`, `editor::save_edit` (compare + save; `Edit` equality already drops the stale export), `edit_from`, the command, and fix every `Edit { .. }` / `Clip { .. }` literal (≈15, mostly tests: add `removed: vec![]`).
- [ ] **Step 4:** `cd desktop/src-tauri && cargo test` → all pass. `cd desktop && pnpm check` → 0 errors (Editor.svelte's `edit` must include `removedRanges: []` for now).
- [ ] **Step 5: Commit** `feat(desktop): the library keeps the parts of a clip the editor removed` (stage `git add -p desktop/src/views/Editor.svelte` without trace hunks).

### Task 2: Export joins the kept parts; thumbnail time follows

**Files:**
- Modify: `desktop/src-tauri/src/export.rs`
- Modify: `desktop/src-tauri/src/commands.rs` (`upload_thumbnail_ms`)

**Interfaces:**
- Consumes: `Edit.removed`, `CutRange` (Task 1).
- Produces: `pub fn kept_ranges(edit: &Edit, duration_ms: u64) -> Vec<(u64, u64)>` (trim minus removed, in order); `export_args(input, output, edit, tracks, encoder, duration_ms: u64)` (new last param, used only when `edit.removed` is non-empty); `upload_thumbnail_ms(clip) -> Option<u64>` now maps into the joined video.

- [ ] **Step 1: Failing tests**
  - `kept_ranges_subtract_the_removed_parts`: trim 1000..9000, removed `[1000..2000, 4000..5000]`, duration 10_000 → `[(2000,4000),(5000,9000)]`; no trim + no removed → `[(0,10_000)]`.
  - `cut_exports_trim_and_concat`: `edit(Some(1000), Some(5000))` + removed `[2000..3000]`, MIC tracks, encoder `["-c:v","libx264"]`, duration 6000 → args contain `trim=start=1.000:end=2.000,setpts=PTS-STARTPTS`, `trim=start=3.000:end=5.000,setpts=PTS-STARTPTS`, `atrim=start=1.000:end=2.000,asetpts=PTS-STARTPTS`, `concat=n=2:v=1:a=1`, `-c:v libx264`, `-force_key_frames`; do **not** contain `-ss`, `-to` or `-c:v copy`.
  - `cut_and_blur_blurs_the_joined_video`: same + `blur_video: true` → `gblur=sigma=8` appears after `concat=` in the graph.
  - `exporting_with_cuts_keeps_only_the_kept_time` (real ffmpeg, fixture 6 s): `edit(Some(500), Some(5500))` + removed `[1500..3500]` → `probe` duration in 2_800..=3_200, 1 audio track, h264; progress ends at 1.0.
  - `thumbnail_time_skips_removed_parts` (commands.rs tests or wherever `upload_thumbnail_ms` is tested): trim 1000..9000, removed `[2000..4000]`, thumbnail 5000 → `Some(2000)` (5000 − 1000 − 2000); thumbnail 3000 (inside removed) → `Some(1000)` (clamped to next kept time 4000 → 4000 − 1000 − 2000); no removed → old behaviour.
- [ ] **Step 2:** `cargo test export:: thumbnail_time` → FAIL.
- [ ] **Step 3: Implement.** No removed ranges → current code path untouched. With removed: no input `-ss`/`-to`; graph = the existing audio mix ending in `[amix]` (rename the current `[aout]` label only on this path) + `[amix]asplit=N[am0]…`; per kept `(s,e)`: `[0:v:0]trim=start=S:end=E,setpts=PTS-STARTPTS[v{i}]` and `[am{i}]atrim=start=S:end=E,asetpts=PTS-STARTPTS[a{i}]`; then `[v0][a0][v1][a1]…concat=n=N:v=1:a=1[vcat][aout]`; blur (if on) as `[vcat]scale…,gblur…[vout]` else `[vcat]null[vout]`; map `[vout]` with the encoder args + `-pix_fmt yuv420p -force_key_frames expr:gte(t,n_forced*1)` (same as blur). `export()` gets the encoder when `blur_video || !removed.is_empty()`, passes `info.duration_ms`, and sets `expected_us` from the sum of kept ranges. For N=1 the asplit is `asplit=1` (valid). `upload_thumbnail_ms`: clamp the thumbnail into the kept ranges (inside a removed range → start of the next kept range, past the last → last kept ms), then subtract `trim start` + removed ms before it.
- [ ] **Step 4:** `cargo test` → all pass (old export tests untouched).
- [ ] **Step 5: Commit** `feat(desktop): uploads and exports join the kept parts with exact cuts`.

### Task 3: Timeline pieces math (`src/lib/cuts.ts`)

**Files:**
- Create: `desktop/src/lib/cuts.ts`, `desktop/src/lib/cuts.test.ts`

**Interfaces:**
- Consumes: `MIN_TRIM_MS`, `TrimRange` from `./trim`; `CutRange` from `./api`.
- Produces:
  - `interface Piece { start: number; end: number; removed: boolean }` — pieces always cover `0..duration` contiguously.
  - `wholeClip(duration): Piece[]`
  - `piecesFromRanges(ranges: CutRange[], duration): Piece[]` — boundaries at each range edge.
  - `splitAt(pieces, ms, trim): Piece[] | null` — null if `ms` is outside `trim` or either half < `MIN_TRIM_MS`.
  - `moveSplit(pieces, i, ms): Piece[]` — boundary between `i` and `i+1`, clamped so both stay ≥ `MIN_TRIM_MS`.
  - `deleteSplit(pieces, i): Piece[]` — merge `i`,`i+1`; removed only if both were.
  - `toggle(pieces, i): Piece[]`; `canRemove(pieces, i, trim): boolean` — false if switching it off leaves no kept time inside `trim`.
  - `removedRanges(pieces, trim): CutRange[]` — removed pieces clipped to trim, adjacent merged, empty dropped (what gets saved).
  - `keptRanges(pieces, trim): TrimRange[]`; `keptLength(pieces, trim): number`.
  - `playFrom(ms, kept: TrimRange[]): number` — `ms` if inside a kept range, else start of the next kept range, else `kept[0].start` (loop).
  - `clampToKept(ms, kept): number` — inside a kept range: unchanged; else the next kept start, or the last kept end − 1.
  - `sameRanges(a: CutRange[], b: CutRange[]): boolean`.

- [ ] **Step 1: Failing tests** in `cuts.test.ts` (duration 10_000, trim 1000..9000 unless noted):
  - `splitAt` at 4000 on `wholeClip` → two pieces `0..4000, 4000..10000`; at 4200 next to a split at 4000 → `null`; at 500 (outside trim) → `null`.
  - `toggle` then `removedRanges` → `[{startMs:4000,endMs:6000}]` for pieces split at 4000/6000 with the middle off.
  - Hidden by trim: middle piece `4000..6000` off, trim 5000..9000 → `removedRanges == [{5000,6000}]`; trim 7000..9000 → `[]`; pieces unchanged in both.
  - `deleteSplit`: kept+removed → kept; removed+removed → removed.
  - `moveSplit` clamps at `MIN_TRIM_MS` from neighbours.
  - `canRemove` false for the only kept piece inside the trim (Review Focus 1).
  - `piecesFromRanges([{2000,3000}], 10000)` → `0..2000 kept, 2000..3000 removed, 3000..10000 kept`, and round-trips through `removedRanges` (Review Focus 3).
  - `playFrom(2500, [{1000,2000},{3000,9000}]) == 3000`; `playFrom(9500, …) == 1000`; `playFrom(1500, …) == 1500` (Review Focus 4).
  - `clampToKept(2500, …) == 3000`; `clampToKept(9500, …) == 8999` (Review Focus 5).
  - `keptLength` of trim 1000..9000 with 4000..6000 off == 6000.
- [ ] **Step 2:** `cd desktop && pnpm vitest run src/lib/cuts.test.ts` → FAIL.
- [ ] **Step 3:** Implement as pure functions returning new arrays.
- [ ] **Step 4:** same command → PASS.
- [ ] **Step 5: Commit** `feat(desktop): timeline piece math for removing parts of a clip`.

### Task 4: TrimBar draws pieces, split lines and toggles

**Files:**
- Modify: `desktop/src/components/TrimBar.svelte`

**Interfaces:**
- Consumes: Task 3 functions.
- Produces: new props `pieces = $bindable<Piece[]>()` and `kept: number` (kept length for the label). Exported method not needed: the editor adds splits by assigning `pieces = splitAt(...)`.

- [ ] **Step 1: Implement.**
  - Removed pieces: absolutely positioned `.cut` divs, red hatch (`repeating-linear-gradient(135deg, rgba(255,93,108,.28) 0 6px, rgba(255,93,108,.08) 6px 12px)`), clipped to the trim (pieces/split lines outside `start..end` are not drawn — that's the "hidden").
  - Split lines (boundaries strictly inside `start..end`): `role="slider"`, `tabindex="0"`, `aria-label="Split"`, draggable via the existing `grabHandle`/`move` machinery (extend `Handle` with `{ split: i }`), arrow keys move by 100/1000 ms, `Delete`/`Backspace` deletes; on hover/focus a small ✕ button (`aria-label="Remove split"`) calls `deleteSplit`.
  - Each piece inside the trim shows on hover/focus-within a small toggle button centred at the top of the piece (`aria-label` "Remove this part" / "Keep this part", `Icon` scissors-or-similar existing icon, else text ✕/↺). Disabled with `title="Keep at least part of the clip"` when `!canRemove`. `pointerdown` on the button stops propagation so it doesn't seek.
  - Label: `Clip length <strong>{formatClock(kept)}</strong>`.
  - `thumb` clamping: `thumbAt` additionally uses `clampToKept` over `keptRanges(pieces, {start,end})`.
- [ ] **Step 2:** `cd desktop && pnpm check` → 0 errors; `pnpm test` → all pass.
- [ ] **Step 3: Commit** `feat(desktop): split lines and removable parts on the editor timeline`.

### Task 5: Editor wiring — split key/button, playback skipping, saving

**Files:**
- Modify: `desktop/src/views/Editor.svelte` (commit with `git add -p`, trace hunks excluded)

**Interfaces:**
- Consumes: Tasks 1, 3, 4.

- [ ] **Step 1: Implement.**
  - State `let pieces = $state<Piece[]>([])`; in `load()`: `pieces = piecesFromRanges(d.clip.removedRanges ?? [], d.durationMs)`.
  - `const kept = $derived(keptRanges(pieces, trim))`.
  - `edit` gains `removedRanges: removedRanges(pieces, trim)`; `changed` also checks `!sameRanges(edit.removedRanges, stored.removedRanges)`; `saveEdit` copies `removedRanges` into `data.clip`.
  - Split: a `Split` button next to the play button (`title="Split at the playhead (S)"`) and `S` key in `onkeydown` (same target guard as Space): `const next = splitAt(pieces, position, trim); if (next) pieces = next;`.
  - rAF loop: replace the trim loop condition with: while playing, if `ms >= trim.end` or `ms < trim.start - 50` or `ms` is inside a removed piece of the trim → `seek(playFrom(ms, kept))` (one seek). `togglePlay`: start at `playFrom(ms, kept)` when not inside a kept range (and from `kept[0].start` when at/after the last kept end − 50). `onended`: `seek(kept[0].start)`.
  - Save/Export/Upload buttons disabled when `kept.length === 0` (can only happen by dragging a trim handle so only removed pieces remain inside), with `title="Keep at least part of the clip"`.
  - Pass `bind:pieces` and `kept={keptLength(pieces, trim)}` to `TrimBar`; thumbnail sent on save is `clampToKept(thumb, kept)` when `thumb !== null`.
- [ ] **Step 2:** `pnpm check` → 0 errors; `pnpm test` and `cargo test` → pass.
- [ ] **Step 3: Commit** `feat(desktop): remove parts of a clip in the editor` (verify with `git diff --cached | grep -c trace` → 0).

### Task 6: Real-app check

- [ ] Run `pnpm tauri dev` from `desktop/`; on a real recording: split twice, switch the middle off, play (jumps once, loops), drag the start handle past a split and back (split hides/returns), save, reopen (cuts restored), Export copy… → open the file: only the kept parts, in sync, cut where marked. Report results to the owner; Windows check is the owner's.
