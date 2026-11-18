# Editor Zoom Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Zoom segments in the editor. The user sets a time range and a box over the picture, the
zoom eases in and out, it previews live, and exports and uploads include it.

**Architecture:** A `ZoomRange` list is stored with the clip's edit, the same way the cuts'
`removed_ranges` is. One easing formula drives two outputs: a CSS `transform: scale()` on the
preview, and an ffmpeg `scale=eval=frame` + `crop` filter in the export. The UI adds a Zoom lane
in `TrackLanes`, a box overlay on the stage, a Z key and a Zoom button.

**Tech Stack:** Tauri 2 (Rust, rusqlite, ffmpeg sidecar), Svelte 5 runes, TypeScript, vitest.

**Spec:** `docs/superpowers/specs/2026-11-18-editor-zoom-design.md`

## Global Constraints

- Zoom factor range: 1.25–4 (`MIN_ZOOM = 1.25`, `MAX_ZOOM = 4`).
- Minimum zoom length: 500 ms (`MIN_ZOOM_MS = 500`). Default new zoom: 3000 ms at 2×,
  `x = y = 0.5`.
- Ramp: `r = min(400, (end − start) / 2)` ms. `E = clip((t−start)/r,0,1) · clip((end−t)/r,0,1)`,
  then smoothstep `E²(3−2E)`. `Z = 1 + (zoom−1)·E`.
- Box: `x`, `y` in 0–1 are the position within the slack. Box left = `x·(1−1/zoom)` of the width.
  The preview uses `transform-origin: x·100% y·100%`.
- Times are in ms into the video file (the same frame of reference as `CutRange`). Zooms are
  sorted and never overlap.
- ffmpeg crop `x`/`y` must be expressions of `t`, never `iw`/`ih`, because crop reads those from
  the first frame only (seen in the 2026-11-18 spike).
- Zooms force a re-encode. Order: zoom → (cuts trim/concat) → blur.
- Never pattern-`pkill`. Stop only your own processes, by PID.

## Review Focus

1. **Trimmed export with `-ss`:** a zoom at 5–8 s in a clip trimmed to start at 4 s must zoom at
   output 1–4 s. If it lands at 5–8 s, the `(t+trimStart)` offset is missing. Test in Task 2.
2. **Zoom across a cut:** a zoom that spans a removed part must still play correctly in the kept
   parts on either side, which works because zoom is applied before `trim`. Test in Task 2.
3. **Odd sizes and non-16:9 clips** (e.g. 1366×768 imports, 2560×1080): the scaled size has to
   stay even and the crop has to stay inside the frame at every `Z`. Test in Task 2.
4. **Adding a zoom where there's no room** (the playhead inside an existing zoom, or under 500 ms
   before the next one or the trim end): nothing happens and no overlap is created. Test in Task 3.
5. **Old clips and an old database:** a clip saved before the migration loads with no zooms, and
   its save still round-trips. Test in Task 1.

---

### Task 1: Store zooms with the edit (Rust)

**Files:**
- Modify: `desktop/src-tauri/src/library/mod.rs` (add `ZoomRange`, `Clip.zoom_ranges`)
- Modify: `desktop/src-tauri/src/library/db.rs` (migration, `COLUMNS`, row read,
  `set_zoom_ranges`, `validate_zooms`)
- Modify: `desktop/src-tauri/src/export.rs` (`Edit.zooms`)
- Modify: `desktop/src-tauri/src/editor.rs` (`current_edit`, `save_edit`)
- Modify: `desktop/src-tauri/src/commands.rs` (`edit_from` and the three commands that call it
  take `zoom_ranges: Option<Vec<ZoomRange>>`; the test fixtures near lines 1237/1279)

**Interfaces:**
- Produces: `#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)] #[serde(rename_all = "camelCase")] pub struct ZoomRange { pub start_ms: u64, pub end_ms: u64, pub zoom: f32, pub x: f32, pub y: f32 }`
  in `crate::library`. `Library::set_zoom_ranges(&self, id: &str, zooms: &[ZoomRange]) -> Result<()>`.
  `pub fn validate_zooms(zooms: &[ZoomRange], duration_ms: u64) -> Result<()>`.
  `Edit.zooms: Vec<ZoomRange>`. Constants `MIN_ZOOM`, `MAX_ZOOM`, `MIN_ZOOM_MS` live in
  `library/mod.rs`.

- [ ] **Step 1: Write failing tests in `db.rs`**
  - `zoom_ranges_round_trip`: a new clip has `zoom_ranges` empty. After
    `set_zoom_ranges(&id, &[z(1000,3000,2.0,0.5,0.5)])`, `get` returns exactly that list.
    `set_zoom_ranges("nope", &[])` returns `Err(AppError::ClipNotFound(_))`.
  - `invalid_zooms_are_refused`: `validate_zooms` on duration 10_000 is `Err(InvalidInput)` for
    each of: start ≥ end; length 499; overlapping `[1000..3000, 2500..4000]`; out of order; end
    10_001; zoom 1.2; zoom 4.1; x −0.1; y 1.1; a NaN zoom. It's `Ok` for `[]` and for
    `[0..500 @1.25, 500..10_000 @4 x=1 y=0]` (touching zooms are allowed).
- [ ] **Step 2: Run** `cd desktop/src-tauri && cargo test zoom`. Expected: FAIL (missing items).
- [ ] **Step 3: Implement.** Add a migration to the end of the migrations list in `db.rs`:
  `-- Zooms set in the editor: JSON list of {"startMs","endMs","zoom","x","y"} (ms into the file).`
  `ALTER TABLE clips ADD COLUMN zoom_ranges TEXT NOT NULL DEFAULT '[]';`. Append the column to
  `COLUMNS` and read it as index 29 with `unwrap_or_default()`, like `removed_ranges`. In
  `save_edit`, call `validate_zooms` next to `validate_cuts` and then `set_zoom_ranges`. Add
  `zooms: clip.zoom_ranges.clone()` to `current_edit`. In `edit_from`, `zoom_ranges` defaults to
  empty.
- [ ] **Step 4: Add the editor test** to `editor.rs` tests: `saving_zooms_drops_the_stale_upload_export`,
  modeled on `saving_an_edit_drops_the_stale_upload_export`, where changing only `zooms` counts
  as a change (returns `true`, and the export file is gone).
- [ ] **Step 5: Run** `cargo test`. Expected: all pass, and the existing tests are untouched apart
  from the added struct fields.
- [ ] **Step 6: Commit** `feat(desktop): zoom ranges are saved with a clip's edit`

### Task 2: Zoom in the export (Rust)

**Files:**
- Modify: `desktop/src-tauri/src/export.rs`

**Interfaces:**
- Consumes: `Edit.zooms`, `ZoomRange` (Task 1).
- Produces: `fn zoom_filter(zooms: &[ZoomRange], size: (u32, u32), offset_ms: u64) -> Option<String>`,
  which returns a filter chain fragment (no labels) or `None` when the list is empty.
  `export_args(input, output, edit, tracks, size: (u32, u32), encoder)` gains the `size`
  parameter, and `export()` passes `(info.width, info.height)` from `probe`. Re-encode when
  `blur_video || !removed.is_empty() || !zooms.is_empty()`.

- [ ] **Step 1: Write failing tests**
  - `zoom_filter_is_none_without_zooms`.
  - `zoom_filter_scales_per_frame_and_crops_by_time`: for one zoom on (1920,1080), the string
    contains `eval=frame`, `crop=1920:1080`, and no `iw` or `ih` inside the crop's `x=`/`y=`.
  - `zoom_filter_offsets_time_by_the_trim_start`: with `offset_ms = 4000`, the expressions use
    `(t+4.000)`.
  - `exporting_a_zoom_reencodes`: the `export_args` joined string contains `-map [vout]` and the
    encoder, and does not contain `-c:v copy`. Do this with and without cuts. With blur also on,
    the zoom filter comes before `gblur` in the graph.
  - `a_zoomed_export_zooms_the_right_moment` (real ffmpeg, like
    `exporting_a_blurred_clip_makes_a_playable_h264_file`): make a 4 s 640×360 `testsrc2` clip,
    trim start 1000, one zoom 2000–3500 @2× x=1 y=0. Export, then grab output frames at 0.3 s and
    1.8 s. The 0.3 s frame equals the source frame at 1.3 s (mean abs diff below a small
    threshold, compared via ffmpeg `-f rawvideo` bytes). The 1.8 s frame differs from the source
    at 2.8 s. The output is 640×360.
  - `odd_and_wide_sizes_stay_even`: `zoom_filter` for (1366,768) and (2560,1080) rounds the scaled
    w/h with `trunc(.../2)*2`, and a real 1366×768 export succeeds.
- [ ] **Step 2: Run** `cargo test export`. Expected: FAIL.
- [ ] **Step 3: Implement `zoom_filter`.** For each zoom k, with times in seconds and
  `T = (t+offset)`: `Ek = clip((T−sk)/rk,0,1)*clip((ek−T)/rk,0,1)`, then smoothstep. Since zooms
  never overlap, `Z = 1 + Σ (zk−1)·Ek` and the crop position is
  `x = Σ (W·(1+(zk−1)·Ek) − W)·xk`. Emit
  `scale=w='trunc(W*Z/2)*2':h='trunc(H*Z/2)*2':eval=frame:flags=bicubic,crop=W:H:x='X':y='Y'`.
  Clamp X to `[0, scaledW−W]` with `min/max` so rounding never pushes the crop outside the frame.
  Plain path: offset = trim start, filter on `[0:v:0]`. Cut path: offset 0, filter on `[0:v:0]`
  before the `split` into the trims. Note that `trim` needs one input per kept part, so insert
  `[0:v:0]<zoom>,split=n[...]` and feed each trim from a split output.
- [ ] **Step 4: Run** `cargo test`. Expected: all pass.
- [ ] **Step 5: Commit** `feat(desktop): exports and uploads include the editor's zooms`

### Task 3: Zoom math for the editor (TypeScript)

**Files:**
- Create: `desktop/src/lib/zoom.ts`, `desktop/src/lib/zoom.test.ts`
- Modify: `desktop/src/lib/api.ts` (`ZoomRange` type; `EditInput.zoomRanges`; clip data `zoomRanges?`)

**Interfaces:**
- Produces (all pure, returning new arrays, or `null` when refused):
  - `interface ZoomRange { startMs: number; endMs: number; zoom: number; x: number; y: number }` (in `api.ts`)
  - `MIN_ZOOM = 1.25`, `MAX_ZOOM = 4`, `MIN_ZOOM_MS = 500`, `DEFAULT_ZOOM_MS = 3000`
  - `zoomAt(zooms, ms): { scale: number; x: number; y: number }`, which is `{1, 0.5, 0.5}` outside every zoom
  - `zoomIndexAt(zooms, ms): number` (−1 if none)
  - `addZoomAt(zooms, ms, trim: TrimRange): ZoomRange[] | null`
  - `moveZoom(zooms, i, startMs, trim): ZoomRange[]`, which keeps the length and clamps between neighbors and the trim
  - `resizeZoom(zooms, i, edge: "start" | "end", ms, trim): ZoomRange[]`, which keeps ≥ MIN_ZOOM_MS and clamps to neighbors
  - `setZoomBox(zooms, i, box: { zoom: number; x: number; y: number }): ZoomRange[]`, which clamps zoom and x/y
  - `removeZoom(zooms, i): ZoomRange[]`
  - `boxRect(z: ZoomRange): { left: number; top: number; width: number; height: number }` (fractions of the frame)
  - `boxFromRect(rect): { zoom; x; y }` (the inverse; width = 1/zoom)

- [ ] **Step 1: Write failing tests in `zoom.test.ts`**
  - `zoomAt` for `{1000..3000, 2×}`: scale is 1 at 999 and at 3000, 2 at 2000, strictly between 1
    and 2 at 1200, and equal at 1200 and 2800 (symmetric). For a 600 ms zoom the ramp is 300 ms,
    so the scale reaches 2 exactly at its midpoint.
  - `addZoomAt(…, 5000, trim 0..20000)` → `[{5000..8000, 2, .5, .5}]`. With a zoom at 6000–9000,
    the new one is cut to 5000–6000. At 5800 (only 200 ms of room) → `null`. Inside an existing
    zoom → `null`. With 1000 ms left before the trim end → cut to fit.
  - `moveZoom` can't pass a neighbor or the trim. `resizeZoom` refuses to go below 500 ms.
  - `setZoomBox` clamps zoom 5 → 4 and 1 → 1.25, and x 1.3 → 1.
  - `boxRect`/`boxFromRect` round-trip, and `boxRect({zoom:2,x:1,y:0})` is `{left:.5, top:0, width:.5, height:.5}`.
- [ ] **Step 2: Run** `cd desktop && pnpm test zoom`. Expected: FAIL.
- [ ] **Step 3: Implement `zoom.ts`** using the easing in Global Constraints. Use `Math.round` for
  ms, as `cuts.ts` does.
- [ ] **Step 4: Run** `pnpm test`. Expected: all pass.
- [ ] **Step 5: Commit** `feat(desktop): zoom math for the editor`

### Task 4: Editor state, save and preview

**Files:**
- Modify: `desktop/src/views/Editor.svelte`
- Modify: `desktop/src/lib/editorKeys.ts`, `desktop/src/lib/editorKeys.test.ts`

**Interfaces:**
- Consumes: `zoom.ts` (Task 3); the Rust command arg `zoomRanges` (Task 1).
- Produces: in `Editor.svelte`, `let zooms = $state<ZoomRange[]>([])` and
  `let selectedZoom = $state<number | null>(null)`. A new key action: `{ kind: "zoom" }`.

- [ ] **Step 1: Write failing tests in `editorKeys.test.ts`.** `z` with no modifiers → `{kind:"zoom"}`.
  Ctrl+Z is still undo. `z` in a text field → null.
- [ ] **Step 2: Implement.** Add `zooms` to `EditState`, `editState`, `applyState`, the stored-edit
  comparison (near line 193, compared field by field like `sameRanges`), the save payload
  (`zoomRanges: zooms`) and the load (`d.clip.zoomRanges ?? []`). Add `"zoom"` to the
  `Gestures` kinds. The `"zoom"` key action calls `addZoomAt(zooms, position, trim)` and, on
  success, records history and selects the new zoom. Add a "Zoom" button after Split with the
  same disabled rule and the title "Zoom in at the playhead (Z), then drag the box on the video".
  Add `<kbd>Z</kbd> zoom` to the hints.
- [ ] **Step 3: Preview.** Compute `const view = $derived(zoomAt(zooms, position))` and apply
  `style:transform="scale({view.scale})"` and
  `style:transform-origin="{view.x*100}% {view.y*100}%"` to the `<video>`, `.freeze` canvas and
  `.scrub` img. Set `.stage { overflow: hidden }`. If `position` updates too slowly during
  playback for a smooth ramp, drive it from the existing playback rAF loop.
- [ ] **Step 4: Run** `pnpm test && pnpm check`. Expected: pass, with no new svelte-check errors.
- [ ] **Step 5: Commit** `feat(desktop): Z adds a zoom; the editor previews zooms`

### Task 5: Zoom lane and box overlay

**Files:**
- Modify: `desktop/src/components/TrackLanes.svelte` (a Zoom lane under the video lane)
- Create: `desktop/src/components/ZoomBox.svelte` (the box over the stage)
- Modify: `desktop/src/views/Editor.svelte` (wire both)

**Interfaces:**
- Consumes: `zoom.ts` (Task 3), the `zooms`/`selectedZoom` state (Task 4), `msToX`/`xToMs` from `lib/trim`.
- Produces: `TrackLanes` props `zooms: ZoomRange[]`, `selectedZoom: number | null`,
  `onzoomselect(i: number | null)`, `onzoomchange(zooms: ZoomRange[])`, `onzoomgesture(active: boolean)`.
  `ZoomBox` props `zoom: ZoomRange`, `videoRect: DOMRect-like {left,top,width,height}` (the
  letterboxed video area in stage px), `onchange(box)`, `ongesture(active)`.

- [ ] **Step 1: Zoom lane.** Show one block per zoom across `duration`, styled like the lanes, with
  the selected block highlighted. A pointer-drag on the body calls `moveZoom`, and a drag on an
  edge (8 px hit area) calls `resizeZoom`. A click selects the zoom and seeks to its start + ramp.
  Delete/Backspace on the focused block calls `removeZoom`. Wrap gestures in
  `onzoomgesture(true/false)` so one drag is one undo step. Hide the lane when there are no zooms
  and show the hint "Press Z to zoom" in its place.
- [ ] **Step 2: ZoomBox.** It's visible only when paused and `zoomIndexAt(zooms, position) === selectedZoom`.
  While it's visible, the preview shows scale 1 so the box can be placed over the whole frame.
  It's a rectangle at `boxRect` mapped into `videoRect`, with a dimmed outside. A drag inside
  moves it, and a corner drag resizes it with the aspect ratio locked. Every change goes through
  `boxFromRect` → `setZoomBox`.
- [ ] **Step 3: Run** `pnpm test && pnpm check`. Expected: pass.
- [ ] **Step 4: Manual check** in `pnpm tauri dev` on a real clip: add two zooms, place the boxes,
  play through (smooth in and out), undo/redo a box drag, trim the start past the first zoom,
  export a copy and watch it in a player, and confirm the zoom lands at the same moments as in the
  preview.
- [ ] **Step 5: Commit** `feat(desktop): a zoom lane and a box on the video to place each zoom`
