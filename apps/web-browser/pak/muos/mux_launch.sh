#!/bin/sh
# HELP: Text web browser and file downloader
# ICON: app
# GRID: Browser

. /opt/muos/script/var/func.sh

SETUP_APP "web_browser" ""

APP_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$APP_DIR" || exit 1

# Shared pocketkit settings and logs live outside the app folder, so
# reinstalling the app keeps them.
export POCKETKIT_DATA="${XDG_CONFIG_HOME:-$HOME/.config}/pocketkit"
mkdir -p "$POCKETKIT_DATA/web-browser"
export POCKETKIT_FONT="$APP_DIR/font.ttf"
# Draw in the muOS style (header/footer bars, mustard highlight).
export POCKETKIT_STYLE=muos
export BROWSER_BOOKMARKS="$APP_DIR/bookmarks.txt"
export BROWSER_DOWNLOAD_DIR="/mnt/mmc/Downloads"

./web_browser > "$POCKETKIT_DATA/web-browser/log.txt" 2>&1
