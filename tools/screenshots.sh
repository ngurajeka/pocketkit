#!/bin/sh
# Regenerates docs/screenshots/*.png from the apps' snapshot examples (no device,
# no network). Uses MinUI's font so they look like the real thing; it's fetched
# into the git-ignored .minui-res/ if missing.
set -e
cd "$(dirname "$0")/.."
ROOT=$PWD
OUT="$ROOT/docs/screenshots"
[ -f .minui-res/BPreplayBold-unhinted.otf ] || ./tools/fetch-minui-res.sh
export MINUI_FONT="$ROOT/.minui-res/BPreplayBold-unhinted.otf"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$OUT"

shot() { # package example ppm-name png-name
	(cd "$WORK" && cargo run -q --manifest-path "$ROOT/Cargo.toml" -p "$1" --example "$2" >/dev/null)
	python3 "$ROOT/tools/ppm2png.py" "$OUT/$4.png" "$WORK/snapshots/$3.ppm"
}

# The muOS style, with the font muOS builds bundle.
muos_shot() { # package example ppm-name png-name
	(cd "$WORK" && POCKETKIT_STYLE=muos POCKETKIT_FONT="$ROOT/packaging/muos/font/AtkinsonHyperlegible-Bold.ttf" \
		cargo run -q --manifest-path "$ROOT/Cargo.toml" -p "$1" --example "$2" >/dev/null)
	python3 "$ROOT/tools/ppm2png.py" "$OUT/$4.png" "$WORK/snapshots/$3.ppm"
}

shot web_browser browser_snapshot home browser-home
shot web_browser browser_snapshot page browser-page
shot web_browser browser_snapshot keyboard browser-keyboard
shot music music_snapshot playing music
shot music music_snapshot theme-sakura music-sakura
shot music music_snapshot theme-midnight music-midnight
muos_shot web_browser browser_snapshot home browser-home-muos
muos_shot web_browser browser_snapshot page browser-page-muos
shot wifi wifi_snapshot list wifi
shot settings settings_snapshot settings settings
shot pocket_pet pet_snapshot happy pocket-pet
ls -1 "$OUT"
