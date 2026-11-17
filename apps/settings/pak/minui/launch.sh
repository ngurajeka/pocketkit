#!/bin/sh
# MinUI launches this when you pick "Settings" from Tools.
cd "$(dirname "$0")" || exit 1
DATA_DIR="${USERDATA_PATH:-.}/settings"
mkdir -p "$DATA_DIR"
./settings > "$DATA_DIR/log.txt" 2>&1
