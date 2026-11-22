# Uploads that survive a restart, Copy link on finish, recording safety

2026-11-22. Three items from "Planned: safety and editor additions" (#1, #3, #4) in `docs/ROADMAP.md`.

## Why

v1 is done when a friend on Windows saves, uploads and shares a clip with no help. An upload lost to
a quit, crash or sleep, or a full disk eating a long recording, are the failures that would stop
them. The link-copy change is the owner's taste: nothing should overwrite the clipboard by surprise.

## 1. Uploads survive a restart

**Today.** tus already resumes: `upload_id` / `upload_offset` are saved per clip after every chunk
(`set_upload_progress`), and `run_upload` resumes when the export file in the cache is still there.
But the queue (title, visibility, tagged friends, replace) only lives in the window
(`uploads.svelte.ts`), so after a restart nothing starts the upload again.

**Design.**
- A new `pending_uploads` table in `library.db` (migration): `clip_id` (key), `title`, `visibility`,
  `tagged` (JSON), `replace`, `queued_at`. A row is written when `start_upload` accepts a job and
  deleted on `upload-done`, on a failure that isn't an interruption (see below), and when the clip is
  deleted.
- On launch (after the library opens and a login exists), the core resumes every pending row, one
  after another, oldest first, using the saved settings. The window sees ordinary `upload-*` events;
  `jobFor` already builds a card for uploads it didn't start, and it gets title/visibility from a new
  `pending_uploads` command at startup so cards read right.
- Not logged in at launch: rows stay; they start after login (same path as `resumeAfterLogin`).
- **Which failures keep the row:** network errors, timeouts, the app quitting, `unauthorized`.
  **Which drop it:** the server refusing the upload (4xx other than 401/408/429), the clip file
  missing, an export (ffmpeg) failure. A dropped one shows as failed with "Try again", as today.
- Uploads stopped by logging out drop their rows (the logout already means "don't carry on").
- Quitting during the export step: the export restarts from scratch next launch (its file may be
  half-written); during the upload step it resumes from the saved offset.

## 2. Copy link when the upload is done (owner chose: on a button, not automatic)

**Today.** Every finished upload copies the share link to the clipboard (`copy_text` in
`start_upload`) and the window shows "Link copied!".

**Design.**
- By default the link is **not** copied. When an upload finishes:
  - Klipt's window is focused → an in-app toast "Uploaded" with a **Copy link** button.
  - Otherwise (window hidden, released, or another app in front) → a system notification
    "Clip uploaded" / `<title>` with a **Copy link** button. Clicking it copies the link.
- New setting, Settings → General: **"Copy the link by itself when an upload finishes"**, off by
  default. On → today's behaviour (copied right away; the toast/notification says "Link copied").
- Bulk uploads (`uploadInTurn`, and the restart resumes): one notification at the end, "3 clips
  uploaded", no Copy button (several links); the links are in My clips as today.
- Linux: `notify-rust` action `copy` (`wait_for_action` on its own thread, the same notification
  builder as `feedback::linux_notification`). Windows: `tauri-winrt-notification` `Toast` with
  `add_button("Copy link", "copy")` and `on_activated`, under Klipt's app id. Works while Klipt runs
  (it lives in the tray); if the button can't be wired on some system, the notification still shows
  and the link is in My clips.
- Replaced uploads say "Video replaced" instead of "Clip uploaded"; same button.

## 3. Recording safety

**Today.** The Record button shows how long a full recording has run (`full-recording-started`,
`startedAtMs`). Nothing watches the disk.

**Design.**
- While a full recording runs, the core checks every 5 s:
  - the size of the newest file in `<clips>/Recordings/` (both gsr `-ro` and the libobs
    `ffmpeg_muxer` write there), sent to the window as `full-recording-status { sizeBytes, freeBytes }`;
  - free space on the drive holding the clips folder (`sysinfo` disks, longest matching mount point;
    unknown → no warnings).
- The Record button shows length and size: "● 12:04 · 3.1 GB".
- **Under 5 GB free:** one warning (notification + in-app toast): "Disk almost full: 4.8 GB left.
  The recording stops at 1 GB." Once per recording.
- **Under 1 GB free:** the recording is stopped the normal way (same as pressing the shortcut, so the
  file is finished and playable) with a notification "Recording stopped: the disk is almost full".
- Thresholds are fixed (5 GB / 1 GB), not settings.
- The replay buffer isn't touched (RAM on Linux; on Windows it writes only on save).

## Approaches considered

- **Restart queue in the window's local storage** instead of the database: rejected — the window is
  destroyed after 5 min hidden and may not exist at startup; the core owns uploads.
- **Copy-link only when the editor/window is closed** (keep auto-copy otherwise): the owner picked
  always-on-a-button, with a setting for the old behaviour.
- **Watching the disk from the recorder backends** (gsr/libobs each): rejected — one check in the
  supervisor covers both and the file size is read the same way.

## Left to the builder

Exact copy of messages, where the migration number lands, how the size label fits the Record button
at narrow widths, tests per piece (pure helpers for failure classification, thresholds and the label).
Windows paths are untested by the builder; the owner checks them.
