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
# The newest release (vX.Y.Z tag), not whatever is on main right now.
tag=$(git ls-remote --tags --refs --sort=-version:refname https://github.com/cRaZy92/klipt 'v*.*.*' |
  sed -n '1s|.*refs/tags/||p')
[ -n "$tag" ] || { echo "Klipt update: couldn't find the newest release" >&2; exit 1; }
echo "Klipt update: building $tag"
git clone --depth 1 --branch "$tag" https://github.com/cRaZy92/klipt /tmp/klipt-update
cd /tmp/klipt-update/packaging/aur
export KLIPT_REF="tag=$tag"
# makepkg's own -s/-i run `sudo -k pacman`, which forgets the password and asks again. So this
# installs the build dependencies and the finished package itself, with the password from above.
deps=$(makepkg --printsrcinfo | sed -n 's/^[[:space:]]*\(make\)\{0,1\}depends = //p')
# Only what's missing: pacman -T knows that e.g. rustup already provides rust and cargo.
# shellcheck disable=SC2086 # one package name per word
missing=$(pacman -T $deps || true)
if [ -n "$missing" ]; then
  # shellcheck disable=SC2086
  sudo pacman -S --needed --noconfirm $missing
fi
makepkg --noconfirm
# shellcheck disable=SC2046 # the package file paths
sudo pacman -U --noconfirm $(makepkg --packagelist)

echo
echo "Klipt is updated. Quit Klipt from the tray and start it again to use the new version."
