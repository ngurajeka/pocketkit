#!/bin/sh
# MinUI launches this when you pick "Web Browser" from Tools.
cd "$(dirname "$0")" || exit 1

# MinUI exports $USERDATA_PATH and $SDCARD_PATH (verify on your install).
DATA_DIR="${USERDATA_PATH:-.}/web-browser"
mkdir -p "$DATA_DIR"

# Bookmarks live next to this script so they're easy to edit from a PC.
export BROWSER_BOOKMARKS="$PWD/bookmarks.txt"
# Downloads go to /Downloads at the root of the SD card.
export BROWSER_DOWNLOAD_DIR="${SDCARD_PATH:-/mnt/SDCARD}/Downloads"
# Search engine; the query is appended. Uncomment to change it.
# export BROWSER_SEARCH_URL="https://lite.duckduckgo.com/lite/?q="
# Frames are rotated 180 degrees for the Mini's upside-down panel; uncomment to turn that off.
# export FB_ROTATE=0

# Everything the browser prints (including errors) goes to a log you can read on the PC.
./web_browser > "$DATA_DIR/log.txt" 2>&1
