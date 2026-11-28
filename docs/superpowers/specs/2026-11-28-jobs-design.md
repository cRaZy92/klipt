# Jobs screen

## Why

Long work runs in the background (uploads, imports, Combine, exports, downloads) and each shows
its progress only in its own place: the upload dialog and cards, the % on the Clips icon, the
Combine dialog, the editor. The owner wants one place that lists all of it with progress, start
time and estimates.

## What the owner decided

- A **Jobs** item in the nav rail, with a count badge while something runs.
- A percentage on every job. Time left only where it's meaningful or easy to get.
- Finished jobs stay until **Clear finished** or the next start of Klipt.
- An upload is one row moving through its steps, including the server's compression after it
  (a real percentage from the server).
- Approach A: the list lives in the app core (not the window), so a window released while hidden
  and opened again still has every job.

## Approaches considered

- **A. A job list in the core (picked).** Each feature adds a job, updates it, ends it; the core
  sends the whole list as `jobs-changed`. Existing progress displays stay as they are.
- **B. Window-only list built from existing events.** Breaks "keep until restart": the window is
  destroyed while hidden and misses events.
- **C. One job runner all work goes through.** A rewrite of working upload/import/export code for
  nothing visible.

## Design

Core (`desktop/src-tauri/src/jobs.rs`): `JobList` (managed state) of `JobView { id, kind, title,
clipId, state, step, progress, etaSecs, amount, message, startedAtMs, finishedAtMs }`. Work holds a
`Job` (dropping it while running marks it failed, so nothing stays "running" after a panic) and
clones `Reporter`s into its callbacks. A step fills a slice of the bar; an unmeasured step
(finishing up, getting the link ready, queued on the server) holds the bar at its start and gets
no time left. Time left = elapsed / moved × remaining for measured steps (after 2 s and 2 %), or
the work's own estimate (imports). Progress sends are throttled to 4/s per job. At most 200
finished jobs are kept.

Upload slices *(my call)*: preparing video 0–40 %, uploading 40–90 % (0–90 % without an export),
finishing up 90–95 %, link 95–100 %, all scaled into the first 75 % when the server will compress;
compression fills the rest. After the link is ready the core asks `GET /api/clips/:id` every 4 s
(up to 3 h); the server now adds `processingProgress` (0..1 while running) for the owner.

Jobs covered: uploads (also resumed ones), imports (folder and drop), Combine, editor "Export
copy", downloads of uploads, and the clips-folder move. Not covered *(my call)*: uploads still
queued in the window's bulk queue, "New clip from part" (seconds, no progress), the editor's
preparing of a clip, full recordings (no end).

UI: `views/Jobs.svelte` — running jobs first (oldest first), then finished (newest first); each
row: kind, title, %, bar, step + sizes/speed/time left, "Started 14:02 · 1:35 so far" or
"14:02 → 14:05 · took 3:12", the error or result line. No retry/cancel *(not asked)*.
