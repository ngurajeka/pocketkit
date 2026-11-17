#!/bin/sh
# Builds the Miyoo Mini+ binary and assembles "dist/Web Browser.pak".
# Run inside the repo's Nix dev shell (direnv), which sets BUILD_CMD, the
# rust-lld linker and the clang used to compile ring (TLS).
set -e
cd "$(dirname "$0")"
TARGET="${TARGET:-armv7-unknown-linux-musleabihf}"
BUILD_CMD="${BUILD_CMD:-cross build}"

$BUILD_CMD --release --no-default-features --features device --target "$TARGET"

PAK="dist/Web Browser.pak"
rm -rf "$PAK"
mkdir -p "$PAK"
cp "target/$TARGET/release/web_browser" "$PAK/web_browser"
cp pak/launch.sh pak/bookmarks.txt "$PAK/"
chmod +x "$PAK/web_browser" "$PAK/launch.sh"
echo "Built: $PAK"
echo "Copy it to the SD card, e.g. <SD>/Tools/miyoomini/"
