#!/bin/sh
# Rebuilds and installs the newest Klipt (Linux package). The app's update bar shows the command
# that runs this; it asks for your password once, at the start, and answers pacman's questions.
set -eu

echo "Klipt update: your password is needed once, to install the package at the end."
sudo -v
# The build can take longer than sudo remembers the password: keep it fresh until this ends.
( while sleep 50; do sudo -n -v 2>/dev/null || exit 0; done ) &
keepalive=$!
trap 'kill "$keepalive" 2>/dev/null || true' EXIT

# Not from inside the folder this removes (an earlier run may have left the shell there).
cd "$HOME"
rm -rf /tmp/klipt-update
git clone --depth 1 https://github.com/cRaZy92/klipt /tmp/klipt-update
cd /tmp/klipt-update/packaging/aur
makepkg -si --noconfirm

echo
echo "Klipt is updated. Quit Klipt from the tray and start it again to use the new version."
