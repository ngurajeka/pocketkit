#!/bin/sh
# MinUI launches this when you pick "Music" from Tools.
cd "$(dirname "$0")" || exit 1

DATA_DIR="${USERDATA_PATH:-.}/music"
mkdir -p "$DATA_DIR"

# Music is read from /Music at the root of the SD card (subfolders are fine),
# or the folder chosen in Settings.pak.
# Volume, shuffle, repeat and the last track are remembered here.
export MUSIC_STATE="$DATA_DIR/state.txt"
# Frames are rotated 180 degrees for the Mini's upside-down panel; uncomment to turn that off.
# export FB_ROTATE=0

./music > "$DATA_DIR/log.txt" 2>&1
