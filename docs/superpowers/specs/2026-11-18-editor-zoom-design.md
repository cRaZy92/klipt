# Editor zoom — design

**Date:** 2026-11-18 · **Status:** approved in conversation

## Intent

In the editor, the user can zoom into a chosen area of the picture for a chosen stretch of the
clip, then back out. A clip can have several zooms. The reason is to show something small in a
gameplay clip, like a far-off enemy, the killfeed or the scoreboard, which is otherwise easy to
miss, especially in a phone-sized Discord embed.

## Goals

- Any number of zoom segments per clip, each with its own start and end time and its own area.
- Each zoom eases in and back out smoothly instead of cutting.
- The preview in the editor plays the zoom live, with no export needed.
- Exports and uploads contain the zoom.
- Zooms are part of the saved edit: autosave, undo and redo, and dirty tracking cover them.

## Non-goals and anti-goals

- No zoom that follows a moving target (the owner ruled it out). Each zoom's area stays fixed.
- Zooms can't overlap, and there are no zoom-to-zoom transitions: each one goes out to 1× before
  the next starts.
- No zoom past 4×, because 1080p footage turns to mush. A zoom that looks blurry has failed even if
  it technically works.
- The server doesn't change. It receives the already-zoomed file.

## Approaches

1. **Drag a box on the video (picked).** The user drags a rectangle with the video's aspect ratio
   over the preview, and the box sets both where to zoom and how far, in one move. It's precise
   for small corners like the killfeed. The owner left the choice to me, and I picked this one.
2. **Click a spot, then set the zoom with a slider.** It gives the same result in two steps and is
   less precise. Rejected.
3. **A zoom that follows a target over time (keyframed area).** The owner explicitly ruled it out.

## Design

- **Data:** `ZoomRange { startMs, endMs, zoom, x, y }`, with times in ms into the video file (the
  same frame of reference as cuts). `zoom` is in 1.25–4. `x` and `y` are in 0–1 and give the box's
  position within the slack: the box's left edge is at `x · (1 − 1/zoom)` of the frame width, and
  the same holds for `y` and the height. Zooms are sorted, don't overlap, and each is at least
  500 ms long. They're stored as JSON in a new `zoom_ranges` column, migrated the same way
  `removed_ranges` was.
- **Easing:** for a segment of length L, the ramp is `r = min(400 ms, L/2)`. The ease value is
  `E(t) = clip((t−start)/r, 0, 1) · clip((end−t)/r, 0, 1)`, smoothed with smoothstep
  (`E²(3−2E)`). The current zoom is `Z(t) = 1 + (zoom−1)·E`.
- **Same math in both places:** for the preview, the `<video>`, the freeze canvas and the scrub
  image get `transform: scale(Z)` with `transform-origin: x·100% y·100%`. In the export, ffmpeg
  scales the frame by `Z(t)` (`scale … eval=frame`) and then crops back to the source size at
  `((W·Z−W)·x, (H·Z−H)·y)`. A spike on 2026-11-18 (ffmpeg n9.0.2) proved this works. The crop's
  x and y have to be written in terms of `t`, not `iw` and `ih`, because crop reads `iw` from the
  first frame only.
- **Export timing:** zoom expressions use source time. In the cut path, zoom is applied to
  `[0:v:0]` before `trim`. In the plain path, `-ss` before `-i` resets `t` to 0, so the
  expressions use `(t+trimStart)`. Zoom forces a re-encode, the same as blur or cuts. When blur is
  also on, zoom runs before blur.
- **UI:** the Z key, or a "Zoom" button next to Split, adds a 3 s zoom at the playhead (2×,
  centered), cut short to fit before the next zoom or the trim end. It's added only where there's
  room for 500 ms. A Zoom lane under the video lane in `TrackLanes` shows the segments: drag the
  body to move one, drag its edges to resize it, click to select it, and Delete or Backspace
  removes it. While the playhead is inside the selected zoom and playback is paused, the stage
  shows that zoom's box at full zoom as a frame over the un-zoomed video. Drag inside the box to
  move it, and drag a corner to resize it (aspect ratio locked, clamped to 1.25–4×). During
  playback, the preview shows the eased zoom instead.
- **Trim and cuts:** zooms outside the kept time are kept but have no effect. Validation only
  checks bounds, order, overlap and value ranges.

## Left to the builder

The visual style of the Zoom lane and the box handles (follow the existing TrimBar and lane look),
plus the exact key-hint copy.
