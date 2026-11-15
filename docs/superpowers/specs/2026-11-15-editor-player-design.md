# Editor player rework (desktop)

## Why

The owner says the editor "feels fragile overall". The cause is the player's design: a muted
`<video>` plus one `<audio>` element per track (game, mic, voice, an imported clip's extra
tracks), each with its own clock and its own system audio stream, held together by workarounds in
`Editor.svelte` and `lib/audioSync.ts` (a 250 ms drift check, "settling" tracks, a learned restart
lead for WebKitGTK, queued seeks). A concrete failure seen on 2026-11-15: on Linux every editor
audio element's PipeWire stream sat at 0% volume (the clip's saved volumes were all 1.0), playback
reset a stream set to 100% back to 0%, and KDE's per-app slider then also unmuted the muted video,
so the clip's own audio could play on top of the separate tracks. WebKitGTK maps each media
element's volume onto its own system stream, which the app doesn't control. A log of the streams
during playback showed WirePlumber's remembered volume for Klipt at 100% the whole time while
pressing play pushed the streams to 0%: the 0 came from the webview applying the elements' own
volume, not from PipeWire's stream restore. Web Audio doesn't use per-element volume, so the
single output stream isn't subject to it.

The owner also wants scrubbing to feel fast and predictable: the frame under the pointer should
show quickly while dragging, and holding anything on the timeline must not let the clip play.

This is step 2 of 3 (1: instant navigation, done; 3: visual design).

## Goals

- Audio tracks never drift apart or from the picture by an audible/visible amount.
- One audio output for the editor, whose volume the app alone sets: no per-element system streams
  stuck at 0%, no muted video getting unmuted by the desktop mixer.
- Volume sliders (per track and master) act instantly while playing.
- Playback over removed parts skips them without an audible gap or blip.
- While the pointer is down on the timeline (playhead, trim handles, split lines, thumbnail
  marker), playback is paused, moving or not. On release it resumes if it was playing before,
  from the playhead.
- While dragging, the picture follows the pointer instantly, at any drag speed.
- The timeline track shows a filmstrip of the clip's pictures.

## Non-goals

- Changing what uploads/exports produce (they keep using ffmpeg with the saved volumes).
- Changing how removed parts, trims, splits or the thumbnail are edited (only how they play and
  how dragging previews them).
- A visual redesign of the editor (step 3), beyond the filmstrip on the track.

## Anti-goals (a failure even if it "works")

- Opening a clip becomes noticeably slower before the picture shows. The picture must show as
  soon as the video loads, as today; audio and the filmstrip may arrive after.
- Dragging gets worse while the filmstrip is still being made (it must behave at least as today).
- The cache grows without bound (the filmstrip lives with and is cleaned like the existing
  per-clip editor audio files).

## Constraints

- Runs in WebKitGTK (Linux, owner's main platform, 2.54) and WebView2 (Windows).
- Clips: 340 in the owner's library, average ~67 s, longest ~8 min (487 s). Klipt recordings have
  a keyframe every 1 s; imported clips may have one every ~4 s.
- Editor audio tracks already exist as AAC files in the cache (`<cache>/edit/<id>-game.m4a`,
  `-mic.m4a`, `-voice.m4a`, `-track<n>.m4a`), served to the webview by the loopback media server
  (`media_server.rs`), because WebKitGTK can't stream media from `asset://`.
- CSP: `img-src` allows `http:` (loopback), `media-src` allows `http://127.0.0.1:*`;
  `connect-src` falls back to `default-src 'self'`, so `fetch` of loopback URLs needs a CSP change
  (or the data arrives another way).

## Approaches

Audio (picked: A):
- **A. Web Audio buffers.** Decode each track into memory; play all from one `AudioContext` clock
  through per-track gain nodes into a master gain. Removes drift by construction, exact skips over
  removed parts, instant volume, one output stream. Costs decode time on open (well under 1 s for a
  typical clip, a few seconds for 8 min) and memory (~25 MB per track per minute; worst case
  ~550 MB for an 8-minute, three-track clip while open).
- B. Route the existing `<audio>` elements through Web Audio for volume only. Fixes the 0% bug but
  keeps every sync workaround: the editor stays fragile.
- C. Premix tracks in the backend and play one file. No sync at all, but every slider change means
  a remix and a source swap: delayed feedback and a playback hiccup.

Scrubbing preview (picked: 1):
- **1. Filmstrip of preview pictures.** On first open the backend extracts ~10 pictures per second
  (~640 px wide) into the cache in the background. While dragging, the preview shows the nearest
  picture (instant); the exact frame replaces it when the pointer rests or is released. Softer
  picture and 0.1 s granularity while dragging; some seconds of extraction on first open;
  ~25 MB disk per minute of clip. The same pictures tile the timeline track (owner's addition).
- 2. A low-res all-keyframe proxy video used while dragging: faster than now but still goes
  through WebKitGTK's seek pipeline, so not instant; slower to build.
- 3. Snap to keyframes while dragging: free and fast, but jumps in 1 s (or 4 s) steps.

## Design

### Audio engine

- The `<video>` stays and stays muted; it only shows pictures. All sound comes from Web Audio.
- The video must not have an audio stream the desktop mixer can unmute. On Linux its audio tracks
  are disabled (`video.audioTracks[i].enabled = false`), not just muted. If WebKitGTK doesn't
  support that, the loopback media server serves the editor's video without its audio streams
  (remuxed on the fly or a cached video-only copy, builder's call, as long as the picture still
  shows as soon as it would today). Windows (WebView2) needs nothing beyond `muted`.
- On open, each track's file is fetched and decoded (`decodeAudioData`) into an `AudioBuffer`.
  The picture shows as soon as the video loads; Play is available once the audio is decoded
  (until then the play button shows that audio is loading). A track that fails to decode is
  reported (toast) and the others still play.
- Graph: per track `AudioBufferSourceNode → GainNode(track) → GainNode(master) → destination`.
  The sliders set gain values directly (same 0–100% scale as today; master multiplies every track).
- Playing schedules, on the `AudioContext` clock, one source per track per kept range from the
  current position to the end of the trim, back to back, so removed parts are skipped
  sample-exactly. Pause/seek stops all sources and reschedules from the new position. Looping at
  the trim's end restarts from the first kept part, as today.
- The audio clock is the master. The playhead position comes from it. The video follows: at each
  kept-range boundary the video seeks to the next range's start (as today); within a range, if the
  video drifts more than ~80 ms from the audio clock it is corrected (small `playbackRate`
  nudges preferred; a seek only when far off, e.g. > 0.5 s). Exact thresholds are the builder's.
- What's removed: the `<audio>` elements, `lib/audioSync.ts` (and its test), the drift interval,
  `syncAudio`/`playAudio`/`pauseAudio`. The freeze-frame overlay and queued seeks for the video stay.

### Scrubbing

- Pointer down on the playhead, a trim handle, a split line, the thumbnail marker, or the track
  itself: if playing, pause and remember that it was playing. Pointer up (or pointer cancel):
  resume from the playhead if it was playing. Keyboard nudges of handles don't pause.
- While the pointer is down and moving, the picture shown is the filmstrip picture nearest the
  pointer's time, drawn over the video (instant, no seek per move). When the pointer has rested
  for a short moment (~150 ms) or is released, the video seeks to the exact frame and the
  filmstrip picture is removed once that frame is painted (same "unfreeze after paint" logic as
  today). If the filmstrip isn't ready yet, dragging seeks the video as today.

### Filmstrip

- Backend: on opening the editor, if the clip's filmstrip isn't complete, extract (the rest of) it
  in the background with ffmpeg: ~10 pictures per second of video, ~640 px wide, JPEG, into the
  editor cache next to the audio files (e.g. `<cache>/edit/<id>-strip/`). Opening the editor must
  not wait for it. Progress is reported to the UI (an event) so the strip fills in as it's made and
  scrubbing switches to it when ready.
- Extraction must not hurt a running game or recording (owner-approved):
  - low priority (lowest CPU priority the OS offers), CPU decode only (no hardware decode, the GPU
    belongs to the game and the recorder);
  - it runs only while that clip's editor is open: closing the editor or opening another clip
    stops it;
  - a partly made strip is kept; the next open continues from where it stopped.
- Disk cap (owner-approved): all filmstrips together are kept under a cap, a new setting
  ("Editor preview cache", in Settings → Clips folder, in GB; default 2 GB; range 0.5–100 GB).
  Over the cap, the strips of the clips opened longest ago are deleted (whole strips; never the
  strip of the clip being edited). A clip whose strip was deleted builds it again on its next
  open. The editor copes with its strip disappearing (falls back to seeking as today).
- After a clip is uploaded successfully, its editor leftovers are deleted: the filmstrip and the
  editor's audio copies (`<cache>/edit/<id>-*`). The clip's original file is never touched (it
  stays unless the user deletes it). They're made again if the clip is opened in the editor later.
- Deleting a clip removes its strip with the other editor cache files, as today.
- Pictures are served to the webview the same way as the audio (loopback media server URL), or
  via the asset protocol if that's simpler for images; builder's call.
- Timeline track: the pictures tile the track's background as a filmstrip, one tile per track
  segment of the tile's width, each showing the picture nearest that segment's middle time. The
  trim dimming, removed-part shading, split lines and playhead draw over it as today. Tiles not
  yet extracted show the track's current background.

## Acceptance on the owner's PC

- The editor plays at the clip's saved volumes on the owner's Linux PC (KDE, PipeWire), and KDE's
  per-app slider for Klipt doesn't produce a second copy of the clip's sound.

## Left to the builder

- Exact drift thresholds and correction method details; the pointer-rest delay.
- How decoded audio is fetched given the CSP (`connect-src` for the loopback media server, or
  another way), as long as it stays restricted to the app's own loopback server.
- Filmstrip file layout, JPEG quality, how progress is reported, and whether pictures are served by
  the media server or the asset protocol.
- How the filmstrip tiles are rendered (images vs. a canvas).
