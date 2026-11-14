# Uploaded clips stay in sync with the server

**Why:** renaming an uploaded clip in the app didn't rename it on the server (and website edits
never came back). An uploaded clip should look the same everywhere; only the video file may be
missing locally.

**Synced fields:** title, game (name + Steam id), favorite (your star), visibility (server → app only;
the app only changes it through the server already).

**Approaches considered**
1. *Pending app edit wins* (picked): the app remembers which fields of an uploaded clip still
   have to reach the server and keeps retrying; server values flow back into every field that
   isn't waiting. No server change. Rare edge: an offline app edit beats a later website edit
   of the same field.
2. *True most-recent-wins*: per-field edit timestamps on the server and in the app. Needs a server
   schema/API change and trusts the PC clock, for a rare case. Rejected.

**Design**
- New column `clips.sync_pending` (comma list of `title`, `game`, `favorite`).
- Renaming, re-tagging the game, or starring a clip that has a server copy saves locally, marks the
  field pending and pushes in the background (`PATCH /api/clips/:id`, `PUT|DELETE …/favorite`).
  A field is cleared from the list only once the server took the value it still has.
- Every uploads refresh (`my_uploads`, every 15 s while My clips is open) first pushes what's pending,
  then copies the server's title/game/favorite/visibility into linked local clips, skipping pending
  fields. A 404 on push unlinks the clip like the existing deleted-upload check.
- Edits made from the uploads view (rename, star) update the linked local clip immediately.
- Upload: a clip starred before uploading marks `favorite` pending so the server gets the star.
- Migration: existing uploaded clips mark `title` and `game` pending (the app's values win once,
  which fixes renames done before this change) and `favorite` where starred locally (so a star in
  either place survives, as the list showed before).
