# Thumbnail from an image — design (small change)

## Intent
Let a clip's thumbnail be an image file the owner picks, instead of a frame of the video. Set from the
desktop app only (owner: "enough that thumbnail can be set from app, not web").

## Approaches (owner picked B)
- A. Editor only: image stored locally, sent with the upload. Online-only clips can't get one.
- **B. Editor + online clips (picked):** editor sets it locally and sends it after each upload; the ⋯ menu of an
  online clip card sets it on the server directly. One new server call serves both.
- C. Online clips only: least work, but the local card keeps the frame and a re-upload overwrites the image.

## Design
**Server**
- `PUT /api/clips/:id/thumbnail`, owner only, raw image body (`image/jpeg|png|webp`, ≤ 10 MB), video clips only
  (screenshots: 400). Validated with `probeImage`, scaled to 640 wide JPEG like other thumbnails, written over
  `<id>.jpg` via a temp file. Answers the updated clip.
- New column `thumb_version` (migration 0014), bumped on each set. Thumbnail URLs become
  `?v=<mediaVersion>-<thumbVersion>` when thumbVersion > 0 (else today's query), so the immutable cache,
  the Feed, the website and Discord pick up the new picture. Applied in `toClip`, album cover, and the two web
  pages that use a bare `/media/<id>.jpg` (components.tsx, me.tsx).
- A video replace (re-upload to the same link) still redraws the frame; the app then sends the image again.

**Desktop**
- Library column `thumbnail_image` (0/1). The picked image is center-cropped to the clip's aspect ratio, at most
  1280 wide, and kept as `<thumbs>/<id>.image.jpg`; the card thumbnail `<id>.jpg` is made from it.
- Editor ⋯ menu: "Thumbnail image…" (file picker: png, jpg, jpeg, webp) and, when one is set, "Use a frame again".
  Applies at once (not part of undo/autosave). While an image is set, the trim bar's thumbnail marker is hidden
  and saving an edit doesn't redraw the thumbnail from a frame.
- Upload (new or replace, not screenshots): after finalize, if the clip has an image, it is PUT to the server.
  A failure there is logged and the upload still counts as done (the server keeps the frame).
- Online clip card ⋯ menu: "Change thumbnail…" → file picker → cropped to the online clip's aspect → PUT →
  the card shows the new thumbnail. If a local clip is linked to that online clip, it gets the image too, so a later
  re-upload keeps it.

## Non-goals
Web UI for it; an image for screenshots; "New clip from part"/combine carrying the image over; reverting an
online-only clip to a frame from the menu.

## Left to the builder
Exact menu icons and wording polish; how the picker is filtered.
