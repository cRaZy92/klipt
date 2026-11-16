# Editor layout rework (desktop)

## Why

Step 3 of the desktop rework (after instant navigation and the player). The owner approved a
mockup of the editor (`.superpowers/brainstorm/…/editor-v4.html`, "looks really good, start with
the implementation"). Earlier whole-app restyles were rejected as generic; this change keeps
Klipt's current colours and components and changes layout and workflow only.

## Goals (the six approved changes)

1. **Saves by itself.** Every edit (trim, cuts, volumes, mute, blur, thumbnail) is saved shortly
   after it's made. No "Save edit" button, no "Unsaved changes" label, no "Discard changes?"
   prompt for the editor. Header shows "Saving…" / "✓ Saved" / "Couldn't save — Retry".
   **Ctrl+Z** undoes and **Ctrl+Shift+Z / Ctrl+Y** redoes edits made since the editor opened.
2. **Header:** back, title, game (as now); on the right the save status, **Export…**, **Upload**
   (primary), and a **⋯** menu with **Show in folder** and **Delete** (moved out of the header).
3. **Transport centred under the video:** step one frame back, play/pause, step one frame
   forward; time as `m:ss.t / m:ss`; **"Kept m:ss"** (length after trim and removed parts);
   **Split (S)**.
4. **Track lanes:** the timeline becomes rows with a label column on the left:
   - a time ruler;
   - **Video**: the filmstrip track with trim handles, split lines, removed-part shading,
     playhead and thumbnail marker (today's TrimBar), with a **Blur** toggle in its label;
   - one **audio lane per track** (Game / Track 1, Mic, Voice chat, Track N): its waveform, and in
     its label a mute button, a volume slider and the value; trims dim and removed parts shade
     every lane at the same positions; the playhead crosses all lanes;
   - **Master**: label with the master volume (no waveform).
   Clicking or dragging on any lane moves the playhead like the video lane does.
5. **Shortcut hints** under the lanes: Space play · S split · ←/→ one frame · Shift+←/→ one
   second · Ctrl+Z undo. ←/→ and Shift+←/→ are new and move the playhead (paused or playing).
6. **Thumbnail marker** stays on the video lane (yellow), draggable as today; while hovered or
   dragged, a small preview of its frame (from the filmstrip, if ready) with its time shows
   above it.

## Non-goals / anti-goals

- No new visual style, colours or fonts; reuse app.css tokens and existing components.
- No change to how playback, filmstrip, uploads or exports work.
- Undo must never undo a delete, an upload, or a game change (those aren't edits).
- Autosave must never lose an edit: leaving the editor (back, another tab, logout, closing the
  window) saves any pending change first.

## Constraints

- WebKitGTK (Linux) and WebView2 (Windows).
- Waveforms come from the audio already decoded by the editor's `AudioEngine` (no new backend
  work, no extra files).
- Frame step uses the clip's frame rate: `open_editor` reports `fps` (probe's
  `avg_frame_rate`; 60 when unknown).
- Saving is `api.saveEdit` as today (instructions only; the clip file is never touched).

## Approaches

- Waveforms: **from the decoded AudioBuffers** (picked: already in memory, instant once audio is
  ready) vs backend ffmpeg peak files (extra process and files for data the editor has).
- Autosave: **debounced save (~600 ms after the last change) with a flush on leave** (picked)
  vs save on every change (a drag would save dozens of times) vs save on leave only (a crash
  loses work).
- Undo: **snapshot stack of the edit state, coalescing rapid changes** (a drag is one step)
  (picked) vs per-action command objects (more code, nothing gained here).

## Left to the builder

Exact debounce/coalescing times, waveform bucket count and drawing (canvas), lane heights, and
the ⋯ menu's component (reuse the existing menu pattern in App.svelte / ClipCard).
