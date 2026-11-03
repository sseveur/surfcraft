#!/bin/sh
# Install or update SurfCraft on macOS:
#
#   curl -fsSL https://raw.githubusercontent.com/sseveur/surfcraft/main/install.sh | sh
#
# Downloads the latest release's dmg from GitHub, copies SurfCraft.app to /Applications and opens it.
set -eu

REPO=sseveur/surfcraft
APP=/Applications/SurfCraft.app

say() { printf '%s\n' "$*"; }
fail() { say "SurfCraft install: $*" >&2; exit 1; }

[ "$(uname -s)" = Darwin ] || fail "this script is for macOS. Other systems: https://github.com/$REPO#download"

say "Looking for the latest SurfCraft release..."
URL=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" |
    sed -n 's/.*"browser_download_url": *"\([^"]*-macos\.dmg\)".*/\1/p' | head -n 1)
[ -n "$URL" ] || fail "no macOS download found in the latest release (https://github.com/$REPO/releases)."

WORK=$(mktemp -d -t surfcraft-install)
MOUNT="$WORK/mount"
cleanup() {
    hdiutil detach -quiet "$MOUNT" 2>/dev/null || true
    rm -rf "${WORK:?}"
}
trap cleanup EXIT INT TERM

say "Downloading $(basename "$URL")..."
curl -fL --progress-bar -o "$WORK/SurfCraft.dmg" "$URL"

mkdir "$MOUNT"
hdiutil attach -quiet -nobrowse -readonly -mountpoint "$MOUNT" "$WORK/SurfCraft.dmg"
[ -d "$MOUNT/SurfCraft.app" ] || fail "SurfCraft.app not found in the dmg."

# Quit a running copy so it can be replaced.
osascript -e 'tell application id "org.surfcraft.SurfCraft" to quit' >/dev/null 2>&1 || true

SUDO=
if [ ! -w /Applications ] || { [ -e "$APP" ] && [ ! -w "$APP" ]; }; then
    say "Your password is needed to write to /Applications."
    SUDO=sudo
fi

say "Installing to $APP..."
$SUDO rm -rf /Applications/SurfCraft.app
$SUDO ditto "$MOUNT/SurfCraft.app" "$APP"
# Files downloaded with curl are not quarantined; clear the flag anyway in case an older copy had it.
$SUDO xattr -dr com.apple.quarantine "$APP" 2>/dev/null || true

say "SurfCraft is installed. Opening it..."
open "$APP"
