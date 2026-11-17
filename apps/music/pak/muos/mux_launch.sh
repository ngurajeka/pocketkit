#!/bin/sh
# HELP: Music player with a classic desktop-player look
# ICON: music
# GRID: Music

. /opt/muos/script/var/func.sh

SETUP_APP "music" ""

APP_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$APP_DIR" || exit 1

export POCKETKIT_DATA="${XDG_CONFIG_HOME:-$HOME/.config}/pocketkit"
mkdir -p "$POCKETKIT_DATA/music"
export POCKETKIT_FONT="$APP_DIR/font.ttf"
# muOS's own music folder.
export MUSIC_DIR="/run/muos/storage/music"
export MUSIC_STATE="$POCKETKIT_DATA/music/state.txt"

./music > "$POCKETKIT_DATA/music/log.txt" 2>&1
