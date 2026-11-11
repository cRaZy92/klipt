# Releasing Klipt (desktop)

Windows installs update themselves from GitHub Releases. The Linux package (`packaging/aur`) updates
through pacman (`makepkg -si` again), not through the in-app updater.

## One-time setup (owner)

The updater only installs builds signed with Klipt's updater key.

- The public key is in `desktop/src-tauri/tauri.windows.conf.json` (`plugins.updater.pubkey`).
- The private key is **never committed**. It lives at `~/.tauri/klipt.key` on the owner's machine
  and in the GitHub secret **`TAURI_SIGNING_PRIVATE_KEY`** (Settings → Secrets and variables →
  Actions → New repository secret; paste the whole file content).
- The key has no password, so `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` isn't needed.
- Losing the private key means installed copies can't auto-update to builds signed with a new
  key: they'd need a manual reinstall. Keep a backup of `~/.tauri/klipt.key`.

## Cutting a release

1. Bump the version in all three places (they must match):
   - `desktop/src-tauri/tauri.conf.json` → `"version"`
   - `desktop/src-tauri/Cargo.toml` → `version`
   - `desktop/package.json` → `"version"`
2. Commit, push to `main`, and wait for CI to go green.
3. Tag and push: `git tag v0.1.1 && git push origin v0.1.1`.
4. The `desktop-windows` workflow checks the tag matches the version, builds the signed installer,
   writes `latest.json`, and attaches both to the GitHub Release for the tag.

Installed copies check `releases/latest/download/latest.json` at start and every 6 hours, then show
"Update available — Restart to install" at the top of the window.

## Testing an update

Install `v0.1.0` from its release, then publish `v0.1.1`. Start the installed 0.1.0: within a few
seconds the update bar should appear; "Restart to install" installs 0.1.1 and restarts.
