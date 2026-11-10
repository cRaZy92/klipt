# Klipt — Session Handoff

Read this first if you're a fresh Claude Code session picking up Klipt. Then read the files it links to.

## Latest: Windows session (2026-11-09/10)

On the owner's Windows dual boot (same PC: RTX 3070 Ti, two 1920×1080 monitors). Details and evidence: `docs/superpowers/ledgers/desktop-sdd-ledger.md`.

- The machine is set up per `docs/WINDOWS-DEV.md` (corrected: Node 22 package id, corepack install dir, OBS 33 layout). OBS runtime + headers are in `C:\klipt-deps`.
- **Desktop Task 1 is built** (`desktop/recorder-obs`, `klipt-recorder`) and verified on Windows: NVENC p4 CBR 1080p60, 2 audio tracks, busy on double save, x264 fallback, second monitor. Not compiled on Linux yet.
- **Task 2:** `scripts/fetch-obs-windows.ps1`, `scripts/package-recorder-windows.ps1` (57 MB test folder) and `.github/workflows/windows-recorder.yml`.
- Plans, decision record and WINDOWS-DEV were corrected for OBS 33 facts (plugin layout `core/<name>/`, DLL search, stdout logging, NVENC `preset` not `preset2` — the Linux spike's preset comparison never applied the preset).
- **Next:** the owner runs the C9 game test (`desktop/recorder-obs/TEST-ON-WINDOWS.md`, or straight from `C:\klipt-deps\obs\bin\64bit`), results go in the decision record's C9 row; then desktop Task 4. On Linux: `cargo build -p klipt-recorder` and Task 1 Step 5.

The rest of this file is the handoff written at the end of the Linux session.

---

## What Klipt is

A self-hosted alternative to Medal.tv for a small friend group. You press a hotkey, the last N seconds of gameplay are saved, and you can trim the clip, upload it to the owner's server and share a link. Friends browse a shared feed. It's open source (GPL is fine) and has no paid code signing.

- Design spec: `docs/superpowers/specs/2026-11-09-medal-alternative-design.md`
- Recording decision (binding): `docs/superpowers/specs/2026-11-09-klipt-capture-decision.md`
- Plans:
  - `docs/superpowers/plans/2026-11-09-klipt-server.md` (done)
  - `docs/superpowers/plans/2026-11-09-klipt-capture-spike.md` (done for Linux)
  - `docs/superpowers/plans/2026-11-09-klipt-desktop.md` (**next**)
- Progress ledgers (rulings, deferred minors, measurements): `docs/superpowers/ledgers/`

## The owner and how they want to work

- **Linux:** CachyOS, KDE Plasma on Wayland, RTX 3070 Ti, main monitor DP-1 at 1920×1080@144 Hz, Slovak keyboard layout. **Dual-boots Windows** and tests Windows builds personally.
- Friends use Windows with RTX 40xx and AMD GPUs.
- **Server:** Proxmox with Dokploy. Deploys as one Docker Compose stack. Repo: `github.com/cRaZy92/klipt`.
- The owner approved pushing **feature branches** and using GitHub Actions. Never force-push `main`. Since 2026-11-10 the owner wants no PRs: once a branch is finished and its CI is green, fast-forward `main` to it (`git push origin <sha>:refs/heads/main`). `feat/server` was merged this way at `ad512b7`.
- **Safety:** never kill processes by name or pattern. Stop only processes you started, by exact PID. The owner rejected a `pkill` command earlier.
- Chosen process: superpowers skills, with **subagent-driven development** (implementer plus reviewer per task) for building, and evidence-based spikes with the owner at the machine.

## State of each part

### Server — branch `feat/server`: built, reviewed, final fixes landing

- Fastify 5, TypeScript, Drizzle/Postgres, tus uploads, a server-rendered website, Discord share page, Docker Compose for Dokploy. See `README.md` on that branch for deployment.
- 70+ tests. All 10 plan tasks passed task review. The final whole-branch review found 4 Important issues, and all 4 are fixed and committed:
  - corrupt video now gives 422 `corrupt_video`
  - finalize is retryable and returns the existing clip with 200
  - admin and owner can delete from the share page
  - `.env` is in `.gitignore`
- The hardening batch (items 5–10 in `docs/superpowers/ledgers/server-final-review-findings.md`) is also done. **`feat/server` is complete at `4db02fc`**: 83/83 tests pass and the scoped re-review is clean. The remaining low-priority minors are at the end of `docs/superpowers/ledgers/server-sdd-ledger.md`.
- Still to do: open a PR `feat/server` → `main` (no `gh` CLI was available on Linux), and run the manual browser check of the website (plan Task 8, Step 5), which was never done.
- API facts the desktop app depends on:
  - `POST /api/auth/login {username,password,client:'desktop',deviceName}` → `{token, me}`, then Bearer auth.
  - tus at `/api/uploads/`, then `POST /api/clips {uploadId,title,gameName|null,steamAppId|null,visibility}` → 201 `Clip`. A repeat finalize → 200 with the same clip; 409 `upload_finalizing` → retry.
  - Send `null`, never `""`, for an unknown game.

### Recording spike — branch `spike/capture`: Linux done, Windows not started

- Measured in Active Matter, GPU-bound at 99%, uncapped FPS:
  - libobs through the KDE portal: −7% to −11% FPS and 28% CPU. **Rejected for Linux.**
  - gpu-screen-recorder with direct KMS capture: **−3.4% FPS** and about 5% of one core. **Chosen for Linux.**
- Global hotkey on Linux: XDG GlobalShortcuts portal, verified in a fullscreen game.
- WebKitGTK playback and the two-track volume editor approach: verified.
- gsr's disk replay mode doesn't trim, so **RAM-only buffer on Linux in v1**.
- **Windows is unverified (C9).** The plan is libobs (DXGI/WGC capture, NVENC/AMF) in our own helper `klipt-recorder`.

### Desktop app — branch `feat/desktop`: plan written, nothing built yet

- The plan is ordered so **Windows recording gets tested first**:
  - **Task 1:** `desktop/recorder-obs`, a Rust binary that loads libobs at runtime (libloading plus bindgen), speaks a JSON-lines protocol, and has a `--selftest` mode with a Windows `RegisterHotKey` hotkey and CPU stats.
  - **Task 2:** Windows CI workflow plus `scripts/fetch-obs-windows.ps1`. On Windows this is optional, since you can build locally.
  - **Task 3:** docs. Partly done: this file and `docs/WINDOWS-DEV.md`.
- The spike's working libobs code is in `spikes/capture/src/{main.rs,record.rs}`. Port from it: explicit module loading, replay buffer, encoder settings, save via the `save` proc handler and `get_last_replay`.

## What to do next on Windows

1. Set up the machine with `docs/WINDOWS-DEV.md`, clone, and `git switch feat/desktop`.
2. Run **desktop plan Task 1** with subagent-driven development, building and running on Windows. Windows specifics:
   - graphics module `libobs-d3d11`
   - sources `monitor_capture` (`monitor_id`, `method=0`), `wasapi_output_capture`, `wasapi_input_capture`
   - modules `win-capture, win-wasapi, obs-ffmpeg, obs-nvenc, obs-x264, obs-outputs`
   - the recorder exe must sit in OBS's `bin/64bit` next to `obs.dll`, `obs-ffmpeg-mux.exe` and `obs-nvenc-test.exe`
3. **Windows C9 test with the owner.** Use `klipt-recorder --selftest --mic --hotkey ALT+F9 --exit-after 180`. Check:
   - the clip has 1 video and 2 audio tracks
   - the hotkey works in a fullscreen game
   - FPS cost is under 5%, measured 60 s off vs 60 s on in the same spot with FPS uncapped (Active Matter worked well on Linux)
   - recorder CPU
   
   Record the results in the C9 row of the decision record.
4. If Windows passes, continue with desktop plan **Task 4** onward. Linux-only parts (gsr backend, portal hotkey) are `#[cfg(target_os = "linux")]` and have to be checked manually on Linux later.
5. Open the `feat/server` PR when convenient.

## Gotchas found so far

- OBS 33: `obs_load_all_modules2` / `obs_load_core_modules` load nothing outside OBS's own layout. Use `obs_open_module` plus `obs_init_module` per module.
- `obs-ffmpeg-mux` and `obs-nvenc-test` must sit next to the executable that hosts libobs.
- bindgen needs SIMDe headers for `obs.h`. Allow-list `bfree` explicitly, because a regex like `bfree_.*` doesn't match it.
- With a 144 Hz monitor, portal capture receives up to 144 frames per second even when recording at 60, which is wasted CPU and GPU. KMS capture avoids it.
- KDE reported the bound shortcut as `Alt+;` when `ALT+F9` was requested, so always display the binding the OS reports.
- In the server tests, `app.inject` can't drive @tus/server PATCH, so the tus tests use real HTTP (`listenOrigin`).
