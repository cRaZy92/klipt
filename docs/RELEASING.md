# Releasing Klipt (desktop)

Windows installs update themselves from GitHub Releases. The Linux package (`packaging/aur`) can't:
it records the commit it was built from (`KLIPT_COMMIT`), checks GitHub every 6 h for a newer
release, and then shows a command to copy that runs `packaging/aur/update.sh` from `main`: it asks for
the password once, clones, builds and installs (`makepkg -si --noconfirm`).
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

## Versions

Klipt uses semantic versions, `MAJOR.MINOR.PATCH` (1.0.0 was the first; releases before it were
`0.1.<commit count>`):

- **patch** (1.2.0 → 1.2.1): only fixes.
- **minor** (1.2.1 → 1.3.0): new features or visible changes; nothing old breaks.
- **major** (1.3.0 → 2.0.0): breaking changes, e.g. the app needs a newer server, or settings /
  the clip library can't be read by an older version anymore.

The version lives in `desktop/src-tauri/tauri.conf.json` (also `desktop/package.json`,
`desktop/src-tauri/Cargo.toml` and `desktop/Cargo.lock`, kept in sync by the release script).
Settings → Account → About shows it.

## How releases happen

Pushing to `main` does **not** release; the `desktop-windows` workflow only builds the installer as
a CI artifact. A release is a `vX.Y.Z` tag:

```sh
pnpm release patch   # or minor, major, or an exact X.Y.Z
git push --atomic origin main vX.Y.Z   # or: pnpm release patch --push
```

`scripts/release.mjs` (run on a clean `main`) bumps the version in the files above, commits
`release: vX.Y.Z` and creates an annotated tag listing the commits since the previous release. It
refuses a version that isn't higher than every existing release tag (the updater only moves up).

When CI sees the tag it:

1. Checks the tag matches `tauri.conf.json`'s version.
2. Builds the installer signed with the updater key.
3. Writes `latest.json` (with the commit list as notes) and publishes both as GitHub Release
   `vX.Y.Z` (marked latest, same notes).

Installed copies check `releases/latest/download/latest.json` at start and every 6 hours, then show
"Update available — Restart to install" at the top of the window.

The Linux package (`packaging/aur/PKGBUILD`) takes its version from the newest tag
(`git describe`): building the tag gives `X.Y.Z`, building later commits of `main` gives
`X.Y.Z.r<commits>.g<commit>` (the app shows `X.Y.Z+r<commits>.g<commit>`). `update.sh`, which the
Linux update bar runs, builds the newest release tag, not `main`.

## Testing an update

Install the latest release's `*-setup.exe` on Windows, cut a release (`pnpm release patch --push`), wait for the
workflow, then start Klipt: within a few seconds the update bar appears; "Restart to install"
installs the new build and restarts.
