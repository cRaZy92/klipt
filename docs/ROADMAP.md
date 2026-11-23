# Klipt roadmap

As of 2026-11-23. Update this file when an item is done or a plan changes.

All of v1 is built for Windows and Linux, and every desktop change pushed to `main` ships itself as a
signed Windows release. What's missing is mostly proof on real machines and with real friends, not code.

## Where Klipt is today

| Date       | Milestone                | What it means                                                                                         |
| ---------- | ------------------------ | ----------------------------------------------------------------------------------------------------- |
| 2026-11-23 | Uploads and recording safety | Uploads cut off by quitting, a crash or sleep carry on at the next launch (or after logging in), one after another; ones the server refused or whose clip is gone don't. A finished upload no longer overwrites the clipboard: the toast (window in front) or system notification has a **Copy link** button, several in a row get one "N clips uploaded"; Settings → General "Copy the link by itself when an upload finishes" (off) brings back the old way. A full recording shows its size on the Record button; under 5 GB free it warns once, under 1 GB it stops cleanly. Untested in the app and on Windows |
| 2026-11-22 | Albums                   | Gather clips (yours or friends', link-only ones too) into a named album with one link, `/a/<id>`, that anyone with it can watch, like a clip's share link. Website: an Albums page, "Add to album" on a clip's page (tick albums, or make one), and the album page plays the clip picked with the whole list beside it; its owner renames, deletes, reorders and removes clips. App: "Add to album" in the clip player and the ⋯ menus of uploaded clips, and "Album" in My clips' selection bar. Up to 200 clips; a deleted clip leaves its albums. Untested in the app; server needs redeploy |
| 2026-11-22 | Mute part of a lane      | Shift+drag on a sound lane (or its new button, from the playhead) mutes that part of the track; its level button sets it to 25% or 50% instead. The parts move, resize and undo like subtitles, play that way in the editor, and uploads, exports, new clips from parts and combined clips turn the track down at the same moments (30 ms fades; the video isn't cut). Untested in the app and on Windows |
| 2026-11-22 | Combine clips            | "Combine" in My clips' selection bar joins 2–50 clips (each with its trim, cuts, zooms, subtitles and volumes) in a chosen order into one new clip, then opens its upload ("Just combine" keeps it local). Each clip is exported as for an upload, then joined and encoded once more, scaled to the biggest clip's size (others centred on black) at the highest frame rate; the game is the one most clips have; the sources stay. Untested in the app and on Windows |
| 2026-11-22 | Clips from parts         | Editor "New clip" makes the trimmed part (cuts, zooms, subtitles, volumes, thumbnail inside it) its own clip in the clips folder, game kept, source untouched; a lossless stream copy from the keyframe before the part, so it takes seconds on long recordings. I / O set the trim start / end at the playhead. Untested in the app and on Windows |
| 2026-11-22 | Timeline zoom            | Zoom the editor timeline (Ctrl+scroll, the slider in the ruler's corner, + / − / 0) down to 1 s; scrollbar, Shift+scroll, follows the playhead; finer ruler labels and waveforms when zoomed (read straight from the samples on long recordings); drags held at an edge scroll along. Untested in the app and on Windows |
| 2026-11-21 | Ready for a friend       | Log file on every PC; Report a problem / Suggest a feature (with system info and log) to the admin page; display names; change password (app and website) and admin password links; admin page shows last seen, app version, OS and GPU; hotkey hint for overlays that take shortcuts. Untested on Windows; server needs redeploy |
| 2026-11-20 | Replace an online clip   | Re-uploading an uploaded clip can swap its video at the same link (`85ebbfa`). Untested in the app; server needs redeploy |
| 2026-11-20 | Editor subtitles         | Typed subtitles (T key, own lane), burned into exports and uploads with libass. Untested in the app and on Windows |
| 2026-11-20 | Website redesign         | The website matches the desktop app; admin charts (uploads per week, top games, compression, storage per user). Needs redeploy |
| 2026-11-20 | Upload progress          | Size, speed and time left while uploading; cards show the edited length                               |
| 2026-11-18 | Editor zoom              | Drag a box on the video to zoom part of a clip (Z key, eased, 1.25–4×), kept in exports and uploads    |
| 2026-11-16 | Editor layout rework     | Saves by itself with undo; a lane per audio track with its waveform; centred transport with frame steps |
| 2026-11-15 | Editor cuts and player   | Remove parts from the middle of a clip (S key); filmstrip timeline; all audio on one Web Audio clock    |
| 2026-11-15 | Discord embeds fixed     | A small embed copy (≤720p, 30 fps, ~8 MB) so Discord's crawler doesn't give up on big clips            |
| 2026-11-11 | Compression and import   | Server H.264 compression queue, favorites kept on the server, desktop folder import                    |
| 2026-11-11 | Review bugs fixed        | All 13 bugs deferred by reviews and the final review's 9 minors fixed (see "Next")                    |
| 2026-11-11 | Linux update notice      | The Linux package checks GitHub and shows the command that installs a newer release; autostart follows the installed copy |
| 2026-11-11 | v0.1.10                  | In-app logo matches the app icon; login hint removed                                                   |
| 2026-11-11 | v0.1.8, first release    | Signed Windows installer on GitHub Releases; installed copies update themselves                        |
| 2026-11-11 | Desktop v1 complete      | Feed tab, installer CI, AUR package, auto-update; final whole-branch review: Ready                     |
| 2026-11-10 | Desktop app built        | Game detection, recorders (libobs on Windows, gpu-screen-recorder on Linux), hotkeys, tray, upload, library, editor, settings |
| 2026-11-10 | Server merged to `main`  | Fastify + Postgres, tus uploads, website, Discord share page, 83 tests                                 |
| 2026-11-09 | Linux capture spike      | gpu-screen-recorder costs 3.4% FPS; libobs through the KDE portal (7–11%) rejected                     |

## Now: prove v1 on real machines

v1 is done when a friend on Windows installs Klipt, saves a clip in a game, uploads it and shares the
link, with no help.

- [x] **Update path.** Checked by the owner on Windows: the installed copy offers the new version and
      "Restart to install" works.
- [x] **Game detection on Windows.** A clip saved in WARDOGS got the right game.
- [x] **Linux manual checks.** The owner uses the Linux app every day (gsr save, hotkey, tray, upload).
- [x] **Install instructions for friends.** `/get-app` (2026-11-16): new accounts land there, and
      it's in the top bar. Download button (newest installer via the release's `latest.json`), the
      SmartScreen step ("More info → Run anyway"), the server address to log in with, the save
      shortcut, and the Arch command. Still missing: a screenshot of the SmartScreen window.
- [ ] **C9 in a real game (owner's PC), later.** FPS cost under 5% (60 s off vs 60 s on, FPS uncapped);
      the owner expects it is. Results go in the C9 row of
      `docs/superpowers/specs/2026-11-09-klipt-capture-decision.md`.
- [ ] **Windows checks, later.** Screenshots (exclusive-fullscreen games may come out black; DPI
      scaling), subtitles with a non-ASCII profile path, folder import, full recording (the "Full
      recording" list in `desktop/recorder-obs/TEST-ON-WINDOWS.md`).
- [ ] **Check the 2026-11-21 additions on Windows.** The log file under `%APPDATA%\app.klipt.Klipt\logs`,
      GPU names from the registry in a report, the overlay hint with NVIDIA/AMD software running.
- [ ] **Friends join on their own.** The server deploys itself; nobody is pushed to install. When a
      friend with an RTX 40xx or an AMD card (the AMF encoder is still unverified) starts using it,
      note anything they get stuck on.

## Next: fixes found in review

The 13 small bugs that reviews deferred were all fixed on 2026-11-11 (commits `4eeba85`…`804e089`):
in-game notice for a save that didn't happen, one message per recorder failure, the missing-recorder
warning shown once, more non-game Steam tools ignored, the real game exe found behind Proton/Steam
Runtime wrappers, a corrupt `mappings.json` or `library.db` kept aside instead of lost or crashing,
logout stopping uploads cleanly, the editor's play blip, keyboard menus and dialog focus, disk-full
ffmpeg failures keeping the upload, share ids and invite codes kept out of server logs, and the
website's error messages, visibility button and empty states.

The 9 minors the final whole-branch review parked (`.superpowers/sdd/2026-11-09-klipt-desktop/final-review.md`,
M-1 to M-9) were fixed the same day: website links at a server subpath, media server limits and
CORS, a dead login cleared on a feed 401, "still starting" during the gsr encoder retry, gsr
listings with a deadline, no restart toast while paused, login saving under the settings lock, feed
media kept on the server, and an unwritable clips folder refused in Settings.

## Planned features (chosen 2026-11-11)

The owner picked these from two lists of suggestions. None has a spec yet.

| Feature                    | What it means                                                                                      |
| -------------------------- | -------------------------------------------------------------------------------------------------- |
| New clips from friends     | **Done.** The app checks the feed every 2 min; the Feed tab shows how many friends' clips are new since you last opened it; "Notify me when a friend shares a clip" (Settings → General, on by default) |
| Local storage cleanup      | **Done.** Settings → Clips folder shows its size; "Delete old clips from this PC" (off by default, 30 days) runs at startup, every 6 h and on save, keeping uploaded, favorite and uploading clips |
| Leave voice chat out       | **Done (Linux).** Voice chat apps (Settings → Audio, default discord, vesktop) are recorded on their own track and left out of the game track; the editor has a Voice chat slider. Windows keeps voices in the game sound (OBS can't record "all but these apps") |
| Favorites and search       | **Done.** Star clips in My clips (Favorites filter); search by title and game in My clips, by title, game or friend in the Feed and on the website (`?q=`) |
| Views and "seen by"        | **Done.** Each friend counts once (never the owner), plus every logged-out play; counts on the website and in the app; the owner sees "Seen by …" on the share page and in My clips |
| Your storage on the server | **Done.** "N clips · 1.5 GB on the server" on the website's My clips and in Settings → Account; admins see it per user. No quotas |
| Thumbnail handle           | **Done.** A thumbnail marker above the editor's trim bar (kept inside the cut); saving redraws the local thumbnail, and an upload sends the frame for the Feed, website and Discord thumbnail |
| Tag friends in a clip      | **Done.** Pick "Who's in it" when uploading (or "Tag friends" on the share page); tagged feed clips show on their profiles, "With …" on the share page and player, and an "I'm in it" filter in the Feed and on the website |
| Next clip suggestion       | **Done (desktop Feed).** When a clip ends, "Up next" offers the next one (Play next / Replay, no autoplay); ← → and header arrows move between clips |
| Download on the share page | **Done.** A Download button saves the video as "<title>.mp4" (`/media/<id>.mp4?download=1`)       |
| Screenshots                | **Done (needs a Windows check).** A screenshot shortcut (default Alt+Shift+`, Settings → Hotkey) or the Screenshot button saves a PNG next to the clips; Clips / Screenshots filters in My clips, the Feed and on the website; screenshots upload and get a share page. Windows grabs the desktop with ffmpeg gdigrab: exclusive-fullscreen games may come out black |
| Sign up from a share link  | **Done (website).** Logged-out visitors see a "Create an account" form beside the clip; the owner is recorded as the inviter (an invite row used on the spot). `SHARE_SIGNUP=off` turns it off; a disabled owner's links can't sign anyone up |
| Profile stats              | **Done (website).** Profiles show clips, total views and the most-watched clip (over what the viewer may see); clips per game are the game chips' counts |

Turned down: a save-and-share hotkey (clips are always trimmed before sharing), choosing the clip
length at save time, moment markers, autoplaying the next clip. Not now: automatic Discord posts, a
weekly highlights page. Not picked (yet): keyboard trimming, re-trim after upload, loudness
evening, links to a moment, per-game recording settings, recording the game's monitor.

## Planned: My clips and Feed polish (chosen 2026-11-13)

Picked by the owner after the sticky page header and the "+N more" game chips (`d057028`). All but
slow motion built on 2026-11-20, one commit each; item 10 was dropped.

| # | Item                               | What it means                                                                                      |
| - | ---------------------------------- | -------------------------------------------------------------------------------------------------- |
| 1 | Keep filters and scroll position   | **Done.** My clips and the Feed stay alive while hidden, so filters, scroll and loaded clips are kept for the session |
| 2 | Sticky day headings                | **Done.** "Today", "Yesterday"… stay pinned under the page header while their clips scroll |
| 3 | Feed loads older clips by itself   | **Done.** The next page loads as you near the end; "Load older clips" only comes back after a failed page |
| 4 | Compact header when scrolled       | **Done.** Once scrolled, the subtitle goes and the title shrinks |
| 5 | "Clear filters" on empty results   | **Done.** In My clips and the Feed |
| 6 | Same filter style on both pages    | **Done.** The Feed uses segmented groups too: All / I'm in it / Favorites, and Everything / Clips / Screenshots |
| 7 | Game counts follow every filter    | **Done.** My clips' game chips count what the place, media and search filters let through; a picked game keeps its chip at 0 |
| 8 | `/` focuses search                 | **Done.** In My clips and the Feed; the field shows the key while empty |
| 9 | Select several clips               | **Done.** A check box on each card (or Ctrl+click; Shift+click selects a run, Ctrl+A all, Esc clears); a bar to star, set game, upload (one after another) or delete them together |
| 11 | Right-click a clip card           | **Done.** Opens the ⋯ menu, in My clips |
| 12 | Hover preview                      | **Done for clips on this PC.** Resting on the picture plays the kept part muted, served with no audio track (a muted stream once made PipeWire start Klipt muted). Feed and upload-only cards show just the thumbnail: a sound-free preview copy on the server was turned down (2026-11-20) |
| 13 | "New" dot on Feed cards            | **Done.** Friends' clips shared since you last opened the Feed get a dot by the title |
| 14 | Slow motion in the editor          | **Not now.** Frame-by-frame stepping is done (2026-11-16); setting parts of a clip to 0.25×/0.5× waits |

Not now: sorting by size, length or views; moving between cards with the arrow keys. Dropped: undo
instead of "Are you sure?" (item 10).

## Planned: recording and multi-clip editing (chosen 2026-11-20)

Asked for by the owner. Item 1 is built and merged to `main` (plan: `docs/superpowers/plans/2026-11-20-full-recording.md`); items 2 and 3 are built (2026-11-22, no spec).

| # | Feature                          | What it means                                                                                      |
| - | -------------------------------- | -------------------------------------------------------------------------------------------------- |
| 1 | Full recording (start/stop) — **built 2026-11-20, on `main`; needs a Windows check** | Besides the replay buffer, a shortcut (default Ctrl+Alt+Backquote), a Record button in My clips and a tray item start and stop a normal recording of any length, saved to `<clips>/Recordings/` and into the library with a recording marker. Linux: gsr `-ro` + SIGRTMIN in the same process (one encoder). Windows: a libobs `ffmpeg_muxer` output sharing the replay buffer's encoders. Recordings follow the normal cleanup rule. Settings, pause, game exit and quit never cut a running recording |
| 2 | Several clips from one recording — **built 2026-11-22; untested in the app** | One part at a time: set the trim (drag, or I / O at the playhead), press "New clip", name it, repeat. In the editor, mark several parts of one clip or recording and export each as its own clip (own title, game kept, own thumbnail), without touching the source |
| 3 | Combine clips into one upload — **built 2026-11-22; untested in the app** | Pick several clips (e.g. from the multi-select bar in My clips) and join them in order into one video that uploads as a single clip; each part keeps its own trim. Moved here from "Merging clips in the editor" under Later |

## Planned: safety and editor additions (chosen 2026-11-20)

Picked by the owner from a list of suggestions (item 8 added 2026-11-22). Items 1–5 and 8 are built
(see "Where Klipt is today"; spec `docs/superpowers/specs/2026-11-22-uploads-and-recording-safety-design.md`);
the rest have no spec yet.

| # | Feature                          | What it means                                                                                      |
| - | -------------------------------- | -------------------------------------------------------------------------------------------------- |
| 1 | ~~Recording safety~~ (done 2026-11-23) | While a full recording runs, show its length and size; warn when the disk runs low, and stop the recording cleanly before the disk fills so the file isn't lost |
| 2 | ~~Mute or duck part of a lane~~ (done 2026-11-22) | In the editor, drag over part of one audio lane (game, mic, voice chat) to silence it or turn it down, without cutting the video; kept in exports and uploads |
| 3 | ~~Uploads survive a restart~~ (done 2026-11-23) | An upload cut off by quitting Klipt, a crash or sleep resumes on the next launch (tus already resumes; the desktop keeps the queue) |
| 4 | ~~Copy link when the upload is done~~ (done 2026-11-23: on a button, owner's choice) | Closing the editor while an upload runs still copies the share link to the clipboard when it finishes, with a notification |
| 5 | ~~Albums~~ (done 2026-11-22)     | Gather clips (your own or friends') into a named album with one share link, e.g. "Best of the tournament" |
| 6 | Draw on the clip                 | Arrows, circles and boxes placed on the video for part of a clip, on their own editor lane like subtitles; burned into exports and uploads |
| 7 | Download all my clips            | A website button that downloads every clip you uploaded as one zip, as a backup or to leave |
| 8 | ~~Zoom the editor timeline~~ (done 2026-11-22) | Zoom in on the timeline (Ctrl+scroll or a slider) to place cuts, subtitles and markers precisely on long clips and recordings; scrolls sideways and follows the playhead. Added by the owner 2026-11-22 |

## Later: after v1

Ideas the spec left out of v1 on purpose, or that came up while building. Nothing here is committed.

| Idea                                  | Why it waits                                                                 |
| ------------------------------------- | ---------------------------------------------------------------------------- |
| Linux AppImage or Flatpak, built in CI | Only the owner uses Linux today; the AUR package covers that                 |
| Disk replay buffer on Linux           | gsr's disk replay mode doesn't trim, so Linux keeps the buffer in RAM in v1  |
| HDR tone-mapping                      | v1 records 8-bit SDR; HDR desktops may give washed-out clips                 |
| Steam Deck / SteamOS Gaming Mode      | Needs gamescope capture; nobody in the group has asked                       |
| GNOME and X11 on Linux                | v1 targets KDE Plasma on Wayland only                                        |
| Code signing                          | Costs money; it would remove the SmartScreen warning and make a Program Files install practical |
| Slow motion in the editor             | Picked on 2026-11-13, then put off by the owner                              |
| Speech-to-text subtitles              | Subtitles are typed by hand for now; the owner deferred automatic ones          |
| Likes, comments, notifications        | A non-goal for v1; the Feed is enough for a small group                      |
| Several qualities per clip on the server | The server compresses every clip to H.264 and makes a small Discord copy; picking a quality in the player is still out |

## Open questions

- **When is it 1.0?** Releases are `0.1.<run number>`. Going to 1.0 means changing `version` in
  `desktop/src-tauri/tauri.conf.json` to `1.0.0`.
- **Should every push to `main` reach friends?** Today any desktop change on `main` becomes a release
  within ~40 minutes. Batching changes would need a manual trigger or tags.
- **Is the AUR package enough for Linux,** or does a friend on another distro need an AppImage?
