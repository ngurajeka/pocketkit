#!/bin/sh
# Builds the Miyoo Mini+ binary and assembles dist/Wifi.pak.
# Run inside the repo's Nix dev shell (direnv), which sets BUILD_CMD and the linker.
set -e
cd "$(dirname "$0")"
TARGET="${TARGET:-armv7-unknown-linux-musleabihf}"
BUILD_CMD="${BUILD_CMD:-cross build}"

$BUILD_CMD --release --no-default-features --features device --target "$TARGET"

PAK="dist/Wifi.pak"
rm -rf "$PAK"
mkdir -p "$PAK"
cp "target/$TARGET/release/wifi" "$PAK/wifi"
cp pak/launch.sh "$PAK/launch.sh"
chmod +x "$PAK/wifi" "$PAK/launch.sh"
echo "Built: $PAK"
echo "Copy it to the SD card, e.g. <SD>/Tools/miyoomini/ (replacing the old Wifi.pak)"
