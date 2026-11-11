# Klipt roadmap

As of 2026-11-11. Update this file when an item is done or a plan changes.

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

The owner picked these from a list of suggestions. None has a spec yet.

| Feature                    | What it means                                                                                      |
| -------------------------- | -------------------------------------------------------------------------------------------------- |
| New clips from friends     | A "new" count on the Feed tab since you last looked; optional desktop notification when a friend uploads |
| Local storage cleanup      | Settings shows the clips folder's size; optional "delete local clips older than N days" that keeps uploaded and favorite clips |
| Leave voice chat out       | Game audio and voice/chat apps on separate tracks, so a clip can be shared without friends' voices |
| Favorites and search       | Star clips in the library; search by title, game or friend in the library, Feed and website      |
| Views and "seen by"        | A view count on each clip, and which friends watched it                                            |
| Your storage on the server | Show each user how much server storage their clips use. No quotas                                  |

Turned down: a save-and-share hotkey (clips are always trimmed before sharing), choosing the clip
length at save time, moment markers. Not now: automatic Discord posts, a weekly highlights page.

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
| Editor beyond trim and volume         | Cuts in the middle, text, merging clips: a non-goal for v1                   |
| Likes, comments, notifications        | A non-goal for v1; the Feed is enough for a small group                      |
| Several qualities per clip on the server | Server-side transcoding: a non-goal for v1                                |

## Open questions

- **When is it 1.0?** Releases are `0.1.<run number>`. Going to 1.0 means changing `version` in
  `desktop/src-tauri/tauri.conf.json` to `1.0.0`.
- **Should every push to `main` reach friends?** Today any desktop change on `main` becomes a release
  within ~40 minutes. Batching changes would need a manual trigger or tags.
- **Is the AUR package enough for Linux,** or does a friend on another distro need an AppImage?
