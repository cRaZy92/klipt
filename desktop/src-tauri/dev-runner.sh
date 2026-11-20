#!/bin/sh
# Cargo runner for `tauri dev` / `tauri build` on Linux (tauri.linux.conf.json).
#
# `tauri dev` runs Klipt inside the terminal's systemd unit, so KDE's GlobalShortcuts portal files
# its shortcuts under the terminal (e.g. "Alacritty"), apart from the installed app's. Running it
# in a scope named like a launched app (app-<app id>-<random>.scope) makes the portal see
# app.klipt.Klipt, so dev and installed builds share one set of shortcuts.
if [ "$1" = run ] && command -v systemd-run >/dev/null 2>&1 && [ -n "$XDG_RUNTIME_DIR" ]; then
  exec systemd-run --user --scope --quiet --unit="app-app.klipt.Klipt-dev$$" -- cargo "$@"
fi
exec cargo "$@"
