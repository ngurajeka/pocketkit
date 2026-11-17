#!/bin/sh
# MinUI launches this when you pick "Wifi" from Tools.
cd "$(dirname "$0")" || exit 1

# Saved networks live here (the stock firmware's file). Override if yours differs.
export WIFI_CONF=/appconfigs/wpa_supplicant.conf
# Frames are rotated 180 degrees for the Mini's upside-down panel; uncomment to turn that off.
# export FB_ROTATE=0

# Every command the pak runs (and its errors) goes to log.txt in this folder.
./wifi > log.txt 2>&1
