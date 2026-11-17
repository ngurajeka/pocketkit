#!/bin/sh
# Builds the Miyoo Mini+ binary and assembles dist/Pocket Pet.pak
#
# Requires ONE of:
#   cross        (cargo install cross; needs Docker)   -> BUILD_CMD="cross build"   (default)
#   cargo-zigbuild (cargo install cargo-zigbuild + zig) -> BUILD_CMD="cargo zigbuild"
set -e
# cross needs rustup-managed rustc; prefer it over other rustc on PATH (e.g. Nix)
if [ -x "$HOME/.cargo/bin/rustup" ]; then
  PATH="$HOME/.cargo/bin:$PATH"
  export PATH
fi
# cross images are amd64-only; on Apple Silicon run them under emulation.
# Also needs: rustup toolchain add stable-x86_64-unknown-linux-gnu --profile minimal --force-non-host
# Use podman when docker is missing or its daemon is down (override with CROSS_CONTAINER_ENGINE)
if [ -z "$CROSS_CONTAINER_ENGINE" ] && ! docker info >/dev/null 2>&1 && command -v podman >/dev/null 2>&1; then
  CROSS_CONTAINER_ENGINE=podman
  export CROSS_CONTAINER_ENGINE
fi
if [ "$(uname -s)" = Darwin ] && [ "$(uname -m)" = arm64 ]; then
  CROSS_CONTAINER_OPTS="${CROSS_CONTAINER_OPTS:---platform linux/amd64}"
  export CROSS_CONTAINER_OPTS
fi
TARGET="${TARGET:-armv7-unknown-linux-musleabihf}"
BUILD_CMD="${BUILD_CMD:-cross build}"

$BUILD_CMD --release --no-default-features --features device --target "$TARGET"

PAK="dist/Pocket Pet.pak"
rm -rf "$PAK"
mkdir -p "$PAK"
cp "target/$TARGET/release/pocket_pet" "$PAK/pocket_pet"
cp pak/launch.sh "$PAK/launch.sh"
chmod +x "$PAK/pocket_pet" "$PAK/launch.sh"
echo "Built: $PAK"
echo "Copy it to the SD card, e.g. <SD>/Tools/miyoomini/"
