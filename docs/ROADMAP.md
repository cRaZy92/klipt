# Klipt roadmap

As of 2026-11-13. Update this file when an item is done or a plan changes.

All of v1 is built for Windows and Linux, and every desktop change pushed to `main` ships itself as a
signed Windows release. What's missing is mostly proof on real machines and with real friends, not code.

## Where Klipt is today

| Date       | Milestone                | What it means                                                                                         |
| ---------- | ------------------------ | ----------------------------------------------------------------------------------------------------- |
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

- [ ] **Update path.** Install v0.1.8 on Windows, start it, and check that it offers v0.1.10 and that
      "Restart to install" works.
- [ ] **C9 in a real game (owner's PC).** FPS cost under 5% (60 s off vs 60 s on, FPS uncapped), the
      hotkey works in a fullscreen game, recorder CPU noted. Results go in the C9 row of
      `docs/superpowers/specs/2026-11-09-klipt-capture-decision.md`.
- [ ] **Game detection on Windows.** A clip saved in a Steam game gets the right game name and shows
      under that game in the Feed.
- [ ] **Linux manual checks.** gsr save, hotkey and tray, upload to a local server. The 2026-11-10
      ledger lists these as pending, and nothing since records them as done.
- [ ] **Server live for friends.** The Docker Compose stack runs on Dokploy, with real accounts and
      invite links.
- [ ] **Install instructions for friends.** The spec asks for a page showing the SmartScreen step
      ("More info → Run anyway") with a screenshot. It doesn't exist yet.
- [ ] **First friend test.** One friend with an RTX 40xx and one with an AMD card (the AMF encoder
      is still unverified). Note anything they get stuck on.

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

Picked by the owner after the sticky page header and the "+N more" game chips (`d057028`). None has
a spec yet. 1 and 7 are bugs you hit in normal use, so they go first.

| # | Item                               | What it means                                                                                      |
| - | ---------------------------------- | -------------------------------------------------------------------------------------------------- |
| 1 | Keep filters and scroll position   | **Done for My clips.** Filters, sort, search, the clips shown and the scroll position are kept for the session (forgotten without a login). Feed still starts over |
| 2 | Sticky day headings                | "Today", "Yesterday"… stay pinned under the sticky header while their clips scroll, so you know which day you're in |
| 3 | Feed loads older clips by itself   | Replace "Load older clips" with loading the next page as you near the end of the list            |
| 4 | Compact header when scrolled       | The pinned block is ~200 px; once scrolled, drop the subtitle and shrink the title so small windows keep room for clips |
| 5 | "Clear filters" on empty results   | Empty states like "No clips match …" get one button that resets every filter                       |
| 6 | Same filter style on both pages    | My clips uses segmented groups; Feed uses loose toggle chips ("I'm in it", "Favorites", "Clips", "Screenshots"). Make them match |
| 7 | Game counts follow every filter    | My clips' game chip counts only follow the place filter: with Screenshots picked, "WARDOGS 5" can count 5 clips and then show nothing. Counts should follow media and search too |
| 8 | `/` focuses search                 | Press `/` anywhere in My clips or Feed to jump to the search field                                 |
| 9 | Select several clips               | **Done.** A check box on each card (or Ctrl+click; Shift+click selects a run, Ctrl+A all, Esc clears); a bar to star, set game, upload (one after another) or delete them together |
| 10 | Undo instead of "Are you sure?"   | "Remove from library" happens at once, with Undo in its toast. Deleting the file itself keeps its confirmation |
| 11 | Right-click a clip card            | Opens the same menu as ⋯ (Rename, Set game, Show in folder, Remove)                                |
| 12 | Hover preview                      | Hovering a thumbnail in My clips or Feed plays the clip muted, to find "the one where…" without opening it |
| 13 | "New" dot on Feed cards            | Cards shared since you last opened the Feed are marked; the app already keeps that time for the new-clips count |
| 14 | Slow motion in the editor          | Pick parts of a clip and set their speed (e.g. 0.25×/0.5×); the saved and uploaded clip plays them slowed. The editor's preview gets speed and frame-by-frame stepping to find the moment |

Not now: sorting by size, length or views; moving between cards with the arrow keys.

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
| Editor beyond trim, volume and speed  | Cuts in the middle, text, merging clips: a non-goal for v1 (slow motion is planned, item 14 above) |
| Likes, comments, notifications        | A non-goal for v1; the Feed is enough for a small group                      |
| Several qualities per clip on the server | Server-side transcoding: a non-goal for v1                                |

## Open questions

- **When is it 1.0?** Releases are `0.1.<run number>`. Going to 1.0 means changing `version` in
  `desktop/src-tauri/tauri.conf.json` to `1.0.0`.
- **Should every push to `main` reach friends?** Today any desktop change on `main` becomes a release
  within ~40 minutes. Batching changes would need a manual trigger or tags.
- **Is the AUR package enough for Linux,** or does a friend on another distro need an AppImage?
