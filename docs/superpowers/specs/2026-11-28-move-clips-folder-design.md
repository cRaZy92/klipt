# Move clips when the clips folder changes

## Why

Changing the clips folder in Settings today only points new clips at the new folder; existing
clips stay behind in the old one (the library keeps their full paths). Moving them by hand in a
file manager breaks them: the library still points at the old paths and shows them as missing.
The owner wants Klipt to offer to bring the clips along.

## What it does

After a save that changed the clips folder succeeds, Klipt asks: "Move your N clips (X GB) to
the new folder?" with **Move** and **Leave them** buttons. Leave them = today's behavior.
No dialog when the old folder has nothing to move.

Move opens a progress window that covers Klipt until it's done (files done / total, bytes, a
Cancel button). Then a result: "Moved all N clips", or "Moved 120 of 214. The rest are still in
the old folder and still work." with the reason (disk full, file in use…).

## What moves

- Library rows (clips, screenshots, full recordings, combined/parts/downloads) whose file is
  inside the old folder, keeping their relative path (`Recordings/…`, `Imported/…` stay in
  those subfolders).
- Not: clips imported in place from elsewhere (their file is outside the old folder), files
  Klipt doesn't track, thumbnails (they live in the app data folder).
- A name already taken in the new folder: the moved file gets a free name, as saving does.
- Emptied Klipt subfolders (`Recordings`, `Imported`) in the old folder are removed; the old
  folder itself is removed only if it ends up empty. *(my call)*

## How (approach A, picked)

One file at a time: rename when on the same drive; otherwise copy, check the size matches,
then delete the original. Right after each file lands, its library row's `path` is updated.
So at any point every row points at a file that exists; stopping (error or Cancel, which drops
a copy in progress) leaves a valid split, never a missing clip. A half-copied file is deleted.

While moving (`is_moving`): uploads (and the background resume of unfinished ones), exports,
Combine, new clips from parts, imports and the cleanup wait or are refused; they carry on after.
Move is refused (the dialog says why and offers Try again) while an upload, export, import,
Combine or download runs. *(Changed while building:)* the recorder isn't stopped. Saving the new
folder already restarted it into the new folder, so new clips never land in the old one.

The move is a job in the Jobs list (kind `move`); the dialog shows that job's bar, bytes, speed
and time left, then its result. Library lists reload on `clips-moved`.

## Approaches considered

- **A. Blocking progress window (picked)**: nothing touches a file mid-move; simple and
  reliable. Owner: "A is more reliable."
- **B. Background move while the app stays usable**: every part touching clips would have to
  handle a file mid-move. Too much code and risk for a rare action.
- **C. Move on next restart**: clips split across two folders until then; confusing.

## Left to the builder

Exact wording, dialog look (match existing dialogs like `DeleteClipDialog`), how progress
events reach the UI, where the move code lives.
