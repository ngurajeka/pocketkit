#!/bin/sh
# Builds paks for the Miyoo Mini+ and collects them in ./dist, ready to copy
# into Tools/miyoomini/ on the SD card.
#
#   ./package.sh                  # every project that has a package.sh
#   ./package.sh wifi web-browser # only these
#
# Run inside the Nix dev shell (direnv), which provides the cross toolchain.
set -e
cd "$(dirname "$0")"
ROOT=$PWD

if [ $# -gt 0 ]; then
	PROJECTS="$*"
else
	PROJECTS=$(for f in */package.sh; do dirname "$f"; done)
fi

mkdir -p dist
for p in $PROJECTS; do
	p=${p%/}
	if [ ! -x "$p/package.sh" ]; then
		echo "No $p/package.sh" >&2
		exit 1
	fi
	echo "== $p"
	(cd "$ROOT/$p" && ./package.sh)
	for pak in "$ROOT/$p"/dist/*.pak; do
		name=$(basename "$pak")
		rm -rf "dist/$name"
		cp -R "$pak" "dist/$name"
	done
done

echo
echo "Paks in $ROOT/dist:"
ls -1 dist
echo "Copy them into Tools/miyoomini/ on the SD card."
