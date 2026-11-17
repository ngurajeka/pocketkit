#!/bin/sh
# MinUI launches this when you pick "Pocket Pet" from Tools.
cd "$(dirname "$0")" || exit 1

# MinUI exports $USERDATA_PATH (verify on your install); fall back to this folder.
SAVE_DIR="${USERDATA_PATH:-.}/pocket-pet"
mkdir -p "$SAVE_DIR"

export PET_SAVE_PATH="$SAVE_DIR/save.txt"

# Everything the game prints (including errors) goes to a log you can read on the PC.
./pocket_pet > "$SAVE_DIR/log.txt" 2>&1
