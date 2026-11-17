#!/bin/sh
# Downloads MinUI's UI font into .minui-res/ (git-ignored) for the desktop preview.
# On the Mini the paks use the copy MinUI already has on the SD card
# (.system/res/BPreplayBold-unhinted.otf), so the font is never bundled.
set -e
cd "$(dirname "$0")/.."
mkdir -p .minui-res
curl -fsSL -o .minui-res/BPreplayBold-unhinted.otf \
	https://raw.githubusercontent.com/shauninman/MinUI/main/skeleton/SYSTEM/res/BPreplayBold-unhinted.otf
echo "Saved .minui-res/BPreplayBold-unhinted.otf"
