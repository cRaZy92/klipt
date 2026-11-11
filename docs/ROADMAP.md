# Klipt roadmap

As of 2026-11-11. Update this file when an item is done or a plan changes.

All of v1 is built for Windows and Linux, and every desktop change pushed to `main` ships itself as a
signed Windows release. What's missing is mostly proof on real machines and with real friends, not code.

## Where Klipt is today

| Date       | Milestone                | What it means                                                                                         |
| ---------- | ------------------------ | ----------------------------------------------------------------------------------------------------- |
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

Small bugs that reviews deferred. None blocks v1. They're ordered by how likely a friend is to hit
them. The final whole-branch review parked 9 more minors in
`.superpowers/sdd/2026-11-09-klipt-desktop/final-review.md`, which exists only on the Linux machine.
Copy them here or into a ledger.

| Area    | Problem                                                                                     | Source        |
| ------- | ------------------------------------------------------------------------------------------- | ------------- |
| Desktop | Pressing the hotkey while recording is paused or stopped, or when a save fails, gives no feedback in game | Task 7        |
| Desktop | Windows counts any `steamapps\common` exe as a game, so Wallpaper Engine keeps "record while a game runs" always on | Task 5        |
| Desktop | A corrupt `library.db` crashes the app at startup; it should back the file up and say so      | Task 4        |
| Desktop | A corrupt game-mappings file is silently overwritten, losing every mapping                  | Task 5        |
| Desktop | Linux game mappings key on the Proton wrapper exe, so one mapping applies to every Proton game | Task 5        |
| Desktop | "Record while a game runs" warns "recorder missing" again at every game start               | Task 6        |
| Desktop | A recorder start failure shows two toasts                                                   | Task 6        |
| Desktop | Logging out during an upload shows "session expired" and resumes the upload under the next login | Task 10       |
| Desktop | Editor: a short audio blip when playing right after a seek; logging out skips the unsaved-changes prompt | Task 11       |
| Desktop | Modals don't trap focus; the clip menus can't be used from the keyboard                     | Task 10       |
| Server  | A server-side ffmpeg failure (disk full) is treated as a corrupt video and the upload is deleted | Final review  |
| Server  | Server logs include link-only share ids and invite codes                                    | Final review  |
| Website | Raw error codes, no empty states, unclear visibility button label                           | Server Task 8 |

Sources: `docs/superpowers/ledgers/desktop-sdd-progress-2026-11-10-linux.md`,
`docs/superpowers/ledgers/server-sdd-ledger.md`.

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
