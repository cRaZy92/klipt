# In-app clip player rework

Date: 2026-11-26. Approved by the owner.

## Why

The owner finds the in-app clip player (`desktop/src/components/ClipPlayer.svelte`, opened from the
Feed and from uploaded clips in My clips) annoying to use: the bottom bar (WebKitGTK's built-in video
controls) looks terrible, a clip doesn't start when the player opens (you click Play on the card,
then Play again), clips take long to start, there is no fullscreen and no favorite button. Separately,
My clips cards don't show whether an uploaded clip is in the Feed or link-only.

## Goals

- Klipt's own bottom bar over the video (option A from the mockups), in place of the native controls.
- Fullscreen.
- The clip starts playing as soon as the player opens.
- Uploaded clips that are on this PC start at once.
- A favorite star in the player.
- A "Link only" label on My clips cards.

## Non-goals

- Clips that aren't uploaded keep opening the editor when clicked (owner: "No 2. is a no"). The
  player is not opened for local-only clips.
- No autoplaying the next clip (turned down earlier; "Up next" stays as it is).
- No video player library; no changes to the website player.

## Approaches

- **Rework `ClipPlayer.svelte` in place (chosen).** It already handles navigation, views, "Seen by",
  albums and "Up next"; only its controls, header and source change.
- A separate fullscreen player page: duplicates all of that for no gain.
- A player library (Plyr, Video.js): weight and styling to fight, and WebKitGTK quirks stay.

## Design

### Bottom bar (mockup A)

- Over a dark fade at the bottom of the video: a seek bar (played part in `--accent`, buffered part
  lighter, a knob on hover/drag, click or drag to seek), then a row: play/pause, volume icon (click =
  mute) with a slider, `0:11 / 0:29`, spacer, playback speed (0.5×, 1×, 1.5×, 2×; click cycles or a
  small menu), fullscreen.
- Hides with the cursor 2 s after the mouse last moved over the video, while playing; always shown
  while paused, while seeking, or while a control has keyboard focus.
- Volume and mute are remembered between clips and launches (per-viewer convenience).
- Clicking the video toggles play/pause; double-click toggles fullscreen.
- Screenshots (`kind === "screenshot"`) keep showing the image with no bar.

### Fullscreen

- Button, F key or double-click. The Klipt window goes fullscreen (Tauri window `setFullscreen`) and
  the player fills it: only the video and the bar, no header. Esc or F leaves; leaving restores the
  window's previous state. Closing the player while fullscreen leaves fullscreen first.

### Keys

Space play/pause · M mute · J / L back / forward 5 s · F fullscreen · ← / → previous / next clip (as
now) · Esc leaves fullscreen, else closes the player. Keys wait while "Add to album" is open, as now.

### Header

Avatar, title, meta, "With …" and "Seen by …" stay. Tools: ★ favorite (filled when favorited; uses
the same server favorite as My clips and the Feed, and the card's star follows), Copy link, Album,
⋯ (Open on web), close. Prev/next arrows stay when navigation is available.

### Starts on open

- Find the cause first, on the owner's Linux machine: whether WebKitGTK refuses autoplay with sound
  (the `autoplay` attribute is already set), or the source is too slow, or something else.
- Fix the real cause: allow media playback without a user gesture for Klipt's own webview
  (WebKitGTK setting; WebView2 already allows it for apps) and/or call `play()` once the video can
  play. A refused `play()` must show the paused bar, not a stuck player.

### Starts fast

- The player keeps streaming the server copy (`clip.videoUrl`) for every clip (owner chose this,
  option C). Playing the local file was dropped: it is the raw recording, not the edited upload.
  Keeping the uploaded copy on the PC was offered and not picked. No loading on hover (owner).
- **Diagnosis (2026-11-26, owner's Linux machine, dev-build trace + throwaway proxies):** the server
  is fine (TTFB ~0.12 s, ranges work, `moov` at the front, curl 10–16 MB/s). WebKitGTK's media
  source reads in ~1 MB range requests one after another, dropping each connection, ~0.33 s per
  round trip → ~3 MB/s into the player; GStreamer then waits for 5–9 s of video (6–20 MB here)
  before playing → 2–8 s starts. `preload="metadata"` changed nothing; a plain local pass-through
  proxy was the same or worse.
- **Fix: read-ahead cache in the app's media server.** Server clips play through the desktop's
  local media server: on the first request for a clip it downloads the whole file from the server
  in one stream (keep-alive, no per-chunk round trips) into a local cache, and answers WebKit's
  range requests from it as it fills. A throwaway proxy doing exactly this took starts from
  1.6–8.2 s down to 0.75–1.9 s (e.g. 3fwbb 4.7 → 1.9 s, h7rw 8.2 → 1.1 s); re-opening a cached
  clip is instant. The cache has a size limit (e.g. 1 GB in the app's cache dir), drops the oldest
  clips first, and is keyed by the media URL (versioned, so a replaced video is a new entry). If
  the cache path fails, the player falls back to `clip.videoUrl`. Nothing is fetched before a clip
  is opened (no preloading on hover or on list load).

### My clips: Feed or link-only

- Uploaded cards (local cards with `serverClipId`, and online cards) whose visibility is `link` show
  a small "Link only" label on the thumbnail. Feed clips show nothing (the usual case).
- Clicking the label switches the clip to the Feed (same action as ⋯ "Show in feed"), with the
  existing toast.

## Left to the builder

Icons, exact sizes and fade timing; whether speed is a cycle button or a small menu; where volume is
remembered (localStorage is fine); WebKitGTK API details for the autoplay setting.
