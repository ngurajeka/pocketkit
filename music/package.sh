#!/bin/sh
# Builds the Miyoo Mini+ binary and assembles dist/Music.pak.
#
# Unlike the other paks this one links against the device's glibc and the
# vendor MI_AO audio library, so it needs the Miyoo Mini toolchain (the one
# MinUI builds with). The repo's Nix dev shell provides it via MIYOO_TOOLCHAIN.
set -e
cd "$(dirname "$0")"
TARGET=armv7-unknown-linux-gnueabihf

if [ -z "$MIYOO_TOOLCHAIN" ]; then
	echo "music: MIYOO_TOOLCHAIN is not set; the Miyoo Mini toolchain is needed for audio (see README.md)" >&2
	exit 1
fi

cargo build --release --no-default-features --features device --target "$TARGET"

PAK="dist/Music.pak"
rm -rf "$PAK"
mkdir -p "$PAK"
cp "target/$TARGET/release/music" "$PAK/music"
cp pak/launch.sh "$PAK/launch.sh"
chmod +x "$PAK/music" "$PAK/launch.sh"
echo "Built: $PAK"
echo "Copy it to the SD card, e.g. <SD>/Tools/miyoomini/"
