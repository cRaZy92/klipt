# Releasing Klipt (desktop)

Windows installs update themselves from GitHub Releases. The Linux package (`packaging/aur`) can't:
it records the commit it was built from (`KLIPT_COMMIT`), checks GitHub every 6 h for a newer
release, and then shows a command to copy (fresh clone + `makepkg -si`) that rebuilds and installs it.
Dev builds (`pnpm tauri dev`) have no commit recorded and never check.

## One-time setup (owner)

The updater only installs builds signed with Klipt's updater key.

- The public key is in `desktop/src-tauri/tauri.windows.conf.json` (`plugins.updater.pubkey`).
- The private key is **never committed**. It lives at `~/.tauri/klipt.key` on the owner's machine
  and in the GitHub secret **`TAURI_SIGNING_PRIVATE_KEY`** (Settings → Secrets and variables →
  Actions → New repository secret; paste the whole file content).
- The key has no password, so `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` isn't needed.
- Losing the private key means installed copies can't auto-update to builds signed with a new
  key: they'd need a manual reinstall. Keep a backup of `~/.tauri/klipt.key`.

## How releases happen

There's nothing to do by hand: **every push to `main` that changes the desktop app is a release.**
The `desktop-windows` workflow:

1. Versions the build `MAJOR.MINOR.<run number>` (MAJOR.MINOR from `desktop/src-tauri/tauri.conf.json`,
   currently 0.1), so each build is newer than the last.
2. Builds the installer signed with the updater key.
3. Writes `latest.json` and publishes both as GitHub Release `v<version>` (marked latest).

Installed copies check `releases/latest/download/latest.json` at start and every 6 hours, then show
"Update available — Restart to install" at the top of the window.

Pushes to `feat/desktop` (or anything other than `main`) only build the installer as a CI artifact.
To start a new series (e.g. 0.2.x), change the version in `tauri.conf.json`.

## Testing an update

Install the latest release's `*-setup.exe` on Windows, push any app change to `main`, wait for the
workflow, then start Klipt: within a few seconds the update bar appears; "Restart to install"
installs the new build and restarts.
