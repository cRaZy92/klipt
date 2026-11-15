# Editor cuts: remove parts of a clip

Date: 2026-11-14 · Status: approved design, awaiting plan

## Intent

Today an edit is one trim (start/end). The owner wants to remove any parts *inside* a clip and
have only the kept parts play in the editor and go into uploads/exports, joined into one video.

Use cases the owner named:

- Something important happens at the start of the clip and the payoff at the end — drop the middle.
- The whole clip is funny but has long silent or empty stretches — cut those out.

The second case needs cuts that land exactly where they were put (a cut that drifts by up to 2 s
chops off someone's sentence or leaves the silence in), which drove the approach below.

## Goals

- Mark any number of removed parts between the trim start and end.
- The editor preview plays only the kept parts, back to back, and loops.
- Upload and "Export copy…" produce one video of the kept parts, frame-exact.
- Works together with "Blur video" and every audio track / volume setting.

## Non-goals

- Undo/redo (a removed piece can just be switched back on).
- Reordering pieces, transitions, crossfades.
- Any change on the server or share page: they play whatever file is uploaded.

## Anti-goals

- Making clips **without** cuts slower to upload: they keep today's stream-copy export.
- More seek churn in the player: there is an open "editor stuck after many seeks" bug; playback
  jumps over a removed part with exactly one seek.

## Approaches considered

| | Approach | Trade-off |
|---|---|---|
| **A (picked)** | Re-encode the video (GPU encoder, as "Blur video" does) only when the clip has removed parts; cuts are frame-exact | A cut clip takes a few extra seconds to prepare; small quality loss (the server compresses anyway) |
| B | Stream-copy kept pieces, snapped to keyframes | Instant, but every cut can be up to 2 s off (gsr keyframe interval) — bad for trimming silences |
| C | Smart render: re-encode only the GOP around each cut, copy the rest | Fast and exact, but splicing requires matching the recorder's encoder output exactly — fragile across NVIDIA/AMD/Intel and Windows |

Owner picked A.

## How it works (decided with the owner)

### Marking cuts — "split, then switch pieces off" (mockup option C)

```
 ▌░░│███████│▒▒▒▒▒▒▒▒▒│██████│▒▒▒│█████████▐░░
 start   ┆  kept  ┆  removed  ┆  kept  ┆rem┆   kept   end
         S        S           S        S   (S = split line, draggable)
                 Clip length 0:15.6  (kept time only)
```

- **S** key or a **Split** button (next to the play button) adds a split line at the playhead.
- Split lines are draggable; hovering one shows a small ✕ that deletes it.
- The pieces between split lines (and the trim handles) are either kept or removed. Removed
  pieces are drawn red-hatched (`--danger`), like the mockup.
- A plain click on the bar still seeks, as today. Switching a piece kept ↔ removed is a small
  toggle button that appears when hovering the piece (not double-click: too easy to trigger).
- Deleting a split line merges its two pieces; the merged piece is removed only if **both** were
  removed, otherwise kept.
- No piece can be shorter than 0.5 s (`MIN_TRIM_MS`, same as the trim); splits closer than that
  to another split or a trim handle are refused.
- The start/end trim handles keep working exactly as today. "Clip length" shows kept time only.
- Dragging a trim handle over split lines or removed pieces **hides** them (owner's choice); they
  come back if the handle is dragged back. The editor keeps all splits/ranges in its state; only
  the saved ranges are clipped to the trim (so ranges outside the trim are gone after save + reopen).
- The uncommitted player-trace code in `Editor.svelte` (the "stuck after many seeks"
  investigation) stays uncommitted: the owner is still testing with it. Build around it and never
  commit it with this feature.
- The thumbnail marker can't sit on a removed piece (clamped to the nearest kept time).
- All of this is keyboard-reachable (split lines and piece toggles are focusable, like the
  existing handles).

### Preview playback

- The existing rAF loop, which already loops inside the trim, also jumps from the start of a
  removed piece to the end of it with one `seek()`. Playing from inside a removed piece starts at
  the next kept time. After the last kept piece it loops to the first kept time.

### Storage

- New library migration: a column holding the removed ranges as JSON
  (`[{"startMs":…,"endMs":…}]`, sorted, non-overlapping, inside the trim). Empty / NULL = no cuts.
  The split lines themselves are UI state; a split line that ends up between two kept pieces is
  not stored (it has no effect).
- `Edit` / `current_edit` / `save_edit` / `validate_edit` carry the ranges, so changing them drops
  the stale upload export exactly like a trim change does today.
- `api.ts` clip type and the `save_edit` command gain the field.

### Export

- No removed ranges → today's `export_args`, untouched (stream copy unless blurred).
- With removed ranges → the kept intervals (trim minus removed ranges) are cut out with `trim` /
  `atrim` + `setpts`/`asetpts` per kept interval and joined with `concat`, on the video and on the
  already-mixed audio, then the video is encoded with `crate::import::detect_encoder` (as blur
  does) with a keyframe per second. Blur, when on, is applied to the concatenated video.
- Progress expects the kept duration, not trim end − trim start.
- `upload_thumbnail_ms` maps the chosen frame into the uploaded video by subtracting the removed
  time before it (and clamps into a kept piece).

## Left to the builder

- Exact ffmpeg graph (e.g. `select`/`aselect` vs. `trim`+`concat`) as long as cuts are frame-exact
  and A/V stays in sync — verify with a test export that has several cuts.
- Encoder quality settings for the non-blurred re-encode: it must look like the original at a
  glance (use a high-quality rate control; blur's settings were tuned for a blurry picture).
- Whether the "hover toggle" is a small icon button centred on the piece or at its top edge.
- Where the split/piece math lives: pure functions in `desktop/src/lib/` (like `trim.ts`) with tests.
