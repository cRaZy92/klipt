# Editor Subtitles Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Typed subtitles in the editor. Timed text blocks go on a Subtitles lane, preview over the
video, and are burned into exports and uploads.

**Architecture:** A `SubtitleRange` list is stored with the clip's edit, the same way
`zoom_ranges` is. The export writes a temporary ASS file (one fixed style, with times mapped into
the export's timeline) and adds ffmpeg's `subtitles` filter after zoom and blur, using a font
embedded in the binary. The editor adds a Subtitles lane in `TrackLanes`, a T key, and a text
overlay that sits on the stage outside the zoomed element.

**Tech Stack:** Tauri 2 (Rust, rusqlite, ffmpeg with libass), Svelte 5 runes, TypeScript, vitest.

**Spec:** `docs/superpowers/specs/2026-11-20-editor-subtitles-design.md`

## Global Constraints

- `SubtitleRange { start_ms, end_ms, text }`, with times in ms into the video file (the same
  frame of reference as `ZoomRange`/`CutRange`). The list is sorted, never overlapping, and
  touching blocks are allowed.
- `MIN_SUBTITLE_MS = 300`, `DEFAULT_SUBTITLE_MS = 2000`, `MAX_SUBTITLE_CHARS = 200`. Text is
  stored trimmed, on one line (newlines are replaced with spaces), and is never empty.
- Font: **Noto Sans Bold** (OFL), a static TTF at `desktop/src/assets/fonts/NotoSans-Bold.ttf`
  with `OFL.txt` beside it. The preview loads it with `@font-face` (family `"Klipt Subtitles"`).
  Rust embeds it with `include_bytes!` and writes it to `<cache_dir>/fonts/` before an export.
- Style, as fractions of the picture height H: font size `0.055·H`, outline `max(2, 0.0035·H)`,
  bottom margin `0.06·H`, side margins `0.05·W`. White fill, black outline, no shadow, bold,
  bottom centre, smart wrapping. The preview uses the same fractions of the shown video's height.
- Any subtitle overlapping the trim forces a re-encode. Filter order: zoom → (cuts trim/concat) →
  blur → subtitles. Text is never zoomed or blurred.
- Never pattern-`pkill`. Stop only your own processes, by PID.

## Review Focus

1. **Trim and cuts move the text:** a subtitle at 5–7 s with the trim starting at 4 s must show at
   output 1–3 s. A subtitle spanning a removed part shows in the kept parts on both sides, and
   one that's entirely inside a removed part doesn't show at all. Tests in Task 2.
2. **Windows paths in the filter:** `C:\Users\Ann's PC\AppData\…` must reach ffmpeg's
   `subtitles=` and `fontsdir=` options intact (the drive colon, backslashes and the apostrophe
   escaped). Test in Task 2.
3. **Text that looks like ASS markup:** `{\b0}lol`, `C:\new`, `a\Nb` must show exactly as typed.
   They must not be read as override tags or line breaks. Test in Task 2 (string) and the real
   export test.
4. **Slovak/Czech letters and emoji:** `čo to bolo ľudia 💀` exports without an error. The letters
   render with the bundled font; a missing emoji glyph may show as a box, but it must not fail
   the export. Covered by the real-ffmpeg test in Task 2.
5. **Old clips and an old database:** a clip saved before the migration loads with no subtitles,
   and its save still round-trips. Test in Task 1.

---

### Task 1: Store subtitles with the edit (Rust)

**Files:**
- Modify: `desktop/src-tauri/src/library/mod.rs` (`SubtitleRange`, `Clip.subtitles`, constants)
- Modify: `desktop/src-tauri/src/library/db.rs` (migration, `COLUMNS`, row read at index 30,
  `set_subtitles`, `validate_subtitles`)
- Modify: `desktop/src-tauri/src/export.rs` (`Edit.subtitles`)
- Modify: `desktop/src-tauri/src/editor.rs` (`current_edit`, `save_edit`, test fixtures)
- Modify: `desktop/src-tauri/src/commands.rs` (`edit_from` and its callers take
  `subtitles: Option<Vec<SubtitleRange>>`; the upload command around line 1138 validates them next
  to `validate_zooms`; test fixtures)
- Modify: `desktop/src-tauri/src/recorder/supervisor.rs` (struct literal, if it builds a `Clip`)

**Interfaces:**
- Produces: `#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)] #[serde(rename_all = "camelCase")] pub struct SubtitleRange { pub start_ms: u64, pub end_ms: u64, pub text: String }`
  in `crate::library`. `Library::set_subtitles(&self, id: &str, subs: &[SubtitleRange]) -> Result<()>`.
  `pub fn validate_subtitles(subs: &[SubtitleRange], duration_ms: u64) -> Result<()>`.
  `Edit.subtitles: Vec<SubtitleRange>`. The Tauri command argument is `subtitles` (camelCase
  `subtitles` on the JS side).

- [ ] **Step 1: Write failing tests in `db.rs`**
  - `subtitles_round_trip`: a new clip has empty `subtitles`. After
    `set_subtitles(&id, &[s(1000, 3000, "čo to bolo")])`, `get` returns exactly that.
    `set_subtitles("nope", &[])` returns `Err(AppError::ClipNotFound(_))`.
  - `invalid_subtitles_are_refused`: on duration 10_000, `Err(InvalidInput)` for each of:
    start ≥ end; length 299; overlapping; out of order; end 10_001; text `""`; text `"   "`;
    text with `\n`; 201 chars (count `chars()`, not bytes). `Ok` for `[]`, for touching
    `[0..300 "a", 300..10_000 "b"]`, and for 200 × `"ľ"`.
- [ ] **Step 2: Run** `cd desktop/src-tauri && cargo test subtitle`. Expected: FAIL (missing items).
- [ ] **Step 3: Implement.** Migration appended to the list:
  `-- Subtitles typed in the editor: JSON list of {"startMs","endMs","text"} (ms into the file).`
  `ALTER TABLE clips ADD COLUMN subtitles TEXT NOT NULL DEFAULT '[]';`. Read it with
  `unwrap_or_default()` like `zoom_ranges`. `save_edit` validates and stores it next to zooms,
  `current_edit` copies it, and `edit_from` defaults to empty.
- [ ] **Step 4: Editor test** `saving_subtitles_drops_the_stale_upload_export`, modeled on
  `saving_zooms_drops_the_stale_upload_export`.
- [ ] **Step 5: Run** `cargo test`. Expected: all pass.
- [ ] **Step 6: Commit** `feat(desktop): subtitles are saved with a clip's edit`

### Task 2: Subtitles in the export (Rust)

**Files:**
- Create: `desktop/src/assets/fonts/NotoSans-Bold.ttf`, `desktop/src/assets/fonts/OFL.txt`
  (from the googlefonts/noto-fonts release: the static `NotoSans-Bold.ttf`, hinted, not the
  variable font)
- Create: `desktop/src-tauri/src/subtitles.rs` (ASS building, the font file, path escaping),
  registered in `lib.rs`
- Modify: `desktop/src-tauri/src/export.rs`

**Interfaces:**
- Consumes: `Edit.subtitles`, `SubtitleRange` (Task 1); `kept_ranges` (export.rs).
- Produces, in `subtitles.rs`:
  - `pub fn events(subs: &[SubtitleRange], kept: &[(u64, u64)]) -> Vec<(u64, u64, String)>`:
    each subtitle intersected with each kept part, mapped to output time (the sum of the earlier
    kept lengths plus the offset into the part). Empty intersections are dropped.
  - `pub fn ass_text(text: &str) -> String`: escapes `{` → `\{` and `}` → `\}`, and a backslash
    that comes before `N`, `n` or `h` gets U+2060 inserted after it.
  - `pub fn ass_document(events: &[(u64, u64, String)], size: (u32, u32)) -> String`: the
    `[Script Info]` (PlayResX/Y = size, `WrapStyle: 0`, `ScaledBorderAndShadow: yes`),
    `[V4+ Styles]` with one `Default` style using the Global Constraints values (font name
    `Noto Sans`, Bold -1, BorderStyle 1, Shadow 0, Alignment 2), and `[Events]` with one
    `Dialogue` per event. Times use the `H:MM:SS.cc` format.
  - `pub fn filter_path(p: &Path) -> String`: backslashes become `/`, `:` becomes `\:`, and the
    result is wrapped in single quotes, with each `'` written as `'\''`.
  - `pub fn write_font(cache_dir: &Path) -> Result<PathBuf>`: writes the embedded TTF to
    `<cache_dir>/fonts/NotoSans-Bold.ttf` if it's missing or a different size, and returns the
    directory.
- `export_args` gains `subtitles: Option<&str>` (the ready-made `subtitles=…:fontsdir=…` filter),
  and `export()` builds it. The ASS file goes to `<cache_dir>/exports/<clip_id>.ass` and is
  removed afterwards, whether the export succeeded or not.
- `reencodes(edit)` is also true when a subtitle overlaps the trim.

- [ ] **Step 1: Write failing tests** (unit tests in `subtitles.rs`, the rest in `export.rs`)
  - `events_follow_the_trim`: kept `[(4000, 20000)]`, sub 5000–7000 → `(1000, 3000)`.
  - `events_split_across_a_cut`: kept `[(0,4000),(6000,10000)]`, sub 3000–7000 →
    `[(3000,4000),(4000,5000)]`. A sub at 4500–5500 → none.
  - `ass_text_shows_markup_literally`: `"{\\b0}lol"` → `"\\{\\b0\\}lol"`. `"a\\Nb"` contains no
    `\N` sequence. `"C:\\new"` keeps its letters.
  - `ass_document_uses_the_fixed_style`: at (1920,1080) it contains `PlayResY: 1080`,
    `Noto Sans`, font size `59`, and `Dialogue: 0,0:00:01.00,0:00:03.00,Default,,0,0,0,,hi`.
  - `filter_path_escapes_windows_paths`: `C:\Users\Ann's PC\x.ass` →
    `'C\:/Users/Ann'\''s PC/x.ass'`.
  - `exporting_subtitles_reencodes`: the `export_args` string contains `subtitles=` after
    `gblur` when blurred, the encoder, and no `-c:v copy`. In the cut path, it comes after
    `[vcat]`.
  - `a_subtitled_export_draws_text` (real ffmpeg, like `a_zoomed_export_zooms_the_right_moment`):
    make a 4 s 640×360 black `color` clip, trim start 1000, subtitle 2000–3000
    `"{\\b0}čo to bolo ľudia 💀"`. Export. The output frame at 1.5 s has bright pixels in its
    bottom third, and the frame at 0.3 s has none. ffmpeg exits 0.
- [ ] **Step 2: Run** `cargo test subtitle export`. Expected: FAIL.
- [ ] **Step 3: Implement** per the Interfaces. Plain path: append the filter after zoom/blur in
  the `video` chain, and use `kept_ranges(edit, u64::MAX)` for `events`, since `-ss` makes the trim
  start `t = 0`. Cut path: replace `[vcat]null[vout]` / the blur line so `subtitles` runs last
  on `[vcat]`. If the bundled ffmpeg lacks libass (`ffmpeg -filters` has no `subtitles`), fail
  with `ToolFailed { detail: "this ffmpeg can't draw subtitles" }`. Check that the Windows
  FFmpeg-Builds GPL build lists it, by reading `scripts/fetch-ffmpeg-windows.ps1` for the variant.
- [ ] **Step 4: Run** `cargo test`. Expected: all pass.
- [ ] **Step 5: Commit** `feat(desktop): exports and uploads include the editor's subtitles`

### Task 3: Subtitle helpers for the editor (TypeScript)

**Files:**
- Create: `desktop/src/lib/subtitles.ts`, `desktop/src/lib/subtitles.test.ts`
- Modify: `desktop/src/lib/api.ts` (`SubtitleRange`; `EditInput.subtitles`; clip data `subtitles?`)

**Interfaces:**
- Produces (pure, returning new arrays, or `null` when refused):
  - `interface SubtitleRange { startMs: number; endMs: number; text: string }` (in `api.ts`)
  - `MIN_SUBTITLE_MS = 300`, `DEFAULT_SUBTITLE_MS = 2000`, `MAX_SUBTITLE_CHARS = 200`
  - `subtitleIndexAt(subs, ms): number` (−1 if none; `start ≤ ms < end`)
  - `addSubtitleAt(subs, ms, trim: TrimRange): SubtitleRange[] | null`: a new block with
    `text: ""`, 2000 ms long, cut short to fit before the next block or the trim end, and `null`
    when there's less than 300 ms of room or the playhead is inside a block
  - `moveSubtitle(subs, i, startMs, trim)`, `resizeSubtitle(subs, i, edge, ms, trim)`: these
    behave like `moveZoom`/`resizeZoom`, with the 300 ms minimum
  - `cleanText(text: string): string`: newlines become spaces, the text is trimmed and cut to
    200 characters (`Array.from`, so emoji aren't split)
  - `setSubtitleText(subs, i, text): SubtitleRange[]`: stores `cleanText`, and removes the block
    when the result is empty
  - `removeSubtitle(subs, i)`, `sameSubtitles(a, b): boolean`
  - `savedSubtitles(subs): SubtitleRange[]`: drops blocks whose text is still empty (one being
    typed), so a save never sends what Rust would refuse

- [ ] **Step 1: Write failing tests in `subtitles.test.ts`**: one per function. Include: adding at
  5000 → `5000..7000`; with a block at 6000, the new one is cut to `5000..6000`; at 5800 → `null`;
  inside a block → `null`. `cleanText("  a\nb  ")` → `"a b"`. 250 × `"💀"` → 200 code points.
  `setSubtitleText(…, "   ")` removes the block. `savedSubtitles` drops `""`.
- [ ] **Step 2: Run** `cd desktop && pnpm test subtitles`. Expected: FAIL.
- [ ] **Step 3: Implement.** If the move/resize clamping can be shared with `zoom.ts` through a
  generic `{startMs, endMs}` helper without changing zoom behaviour, do that. Otherwise mirror it.
- [ ] **Step 4: Run** `pnpm test`. Expected: all pass.
- [ ] **Step 5: Commit** `feat(desktop): subtitle helpers for the editor`

### Task 4: Editor state, save, T key and preview

**Files:**
- Modify: `desktop/src/views/Editor.svelte`, `desktop/src/app.css` (`@font-face`)
- Modify: `desktop/src/lib/editorKeys.ts`, `desktop/src/lib/editorKeys.test.ts`
- Modify: `desktop/src/components/Icon.svelte` (a `subtitles` icon)

**Interfaces:**
- Consumes: `subtitles.ts` (Task 3); the Rust argument `subtitles` (Task 1).
- Produces: in `Editor.svelte`, `let subtitles = $state<SubtitleRange[]>([])`,
  `let selectedSubtitle = $state<number | null>(null)`, and
  `let editingSubtitle = $state<number | null>(null)` (the block whose text box is open). The new
  key action is `{ kind: "subtitle" }`. Picking a subtitle clears `selectedZoom`, and picking a
  zoom clears `selectedSubtitle`. `delete` removes whichever one is picked.

- [ ] **Step 1: Failing tests in `editorKeys.test.ts`**: `t` with no modifiers →
  `{kind:"subtitle"}`. Ctrl+T → not `subtitle`. `t` in a text field → null.
- [ ] **Step 2: Implement state.** Wire `subtitles` through `EditState`, `editState`,
  `applyState`, the stored-edit comparison (`sameSubtitles`), the save payload
  (`subtitles: savedSubtitles(subtitles)`), the load (`d.clip.subtitles ?? []`) and the copy
  payload near line 948. Add `"subtitle"` to the `Gestures` kinds. The `subtitle` key action and a
  "Subtitle" button after Zoom (same disabled rule, title "Add a subtitle at the playhead (T)")
  pause playback, call `addSubtitleAt`, record history, select the new block and set
  `editingSubtitle`. On failure, show the toast "Can't add a subtitle here: inside another one, too
  close to the next one, or outside the trim". Add `<kbd>T</kbd> subtitle` to the hints.
- [ ] **Step 3: Preview overlay.** Inside `.stage`, positioned over `videoRect` but **outside** the
  element with `--zoom`, show `subtitles[subtitleIndexAt(subtitles, position)]?.text` in a
  bottom-centred box. Use font `"Klipt Subtitles"`, size `0.055 × videoRect.height` px, white,
  `-webkit-text-stroke: <2 × outline>px black` with `paint-order: stroke fill`, the bottom margin
  and side margins from Global Constraints, `pointer-events: none`, and wrapping. It hides while
  that block's text box is open.
- [ ] **Step 4: Run** `pnpm test && pnpm check`. Expected: pass, with no new svelte-check errors.
- [ ] **Step 5: Commit** `feat(desktop): T adds a subtitle; the editor previews subtitles`

### Task 5: Subtitles lane and text editing

**Files:**
- Modify: `desktop/src/components/TrackLanes.svelte` (a Subtitles lane under the Zoom lane)
- Modify: `desktop/src/views/Editor.svelte` (wire the props; the text box)

**Interfaces:**
- Consumes: Task 3 helpers, Task 4 state.
- Produces: `TrackLanes` props `subtitles: SubtitleRange[]`, `selectedSubtitle: number | null`,
  `onsubtitleselect(i: number | null)`, `onsubtitlechange(subs)`,
  `onsubtitlegesture(active: boolean)`, `onsubtitleremove(i)`, `onsubtitleedit(i)`.

- [ ] **Step 1: Lane.** One block per subtitle, showing its text truncated with an ellipsis. Drag
  the body to move it, drag an edge (8 px) to resize it, and click to select it and seek to its
  start. Double-click calls `onsubtitleedit`. Gestures are wrapped so one drag is one undo step.
  With no subtitles, the lane shows the hint "Press T to add a subtitle", like the Zoom lane.
- [ ] **Step 2: Text box.** When `editingSubtitle !== null`, show an `<input maxlength=200>` at the
  subtitle's place over the video, focused and filled with the current text. Enter or blur
  commits through `setSubtitleText`, which is one history step and removes the block when the
  text is empty. Esc cancels, and a new block that never had text is removed. Keys typed in it
  must not trigger editor shortcuts (check that `editorKeys` treats it as a text field).
- [ ] **Step 3: Run** `pnpm test && pnpm check`. Expected: pass.
- [ ] **Step 4: Manual check** in `pnpm tauri dev` on a real clip: add three subtitles with T,
  including Slovak letters and `{lol}`. Retime them, undo/redo, add a zoom over one (the text must
  not zoom), trim past the first, cut through the middle of one, export a copy and watch it in a
  player. The text must appear at the same moments and look like the preview.
- [ ] **Step 5: Commit** `feat(desktop): a subtitles lane to place and edit each subtitle`
