#!/bin/sh
# Builds pocketkit apps and packages them per platform:
#   dist/minui/<Name>.pak/     Miyoo Mini+ under MinUI  (copy into Tools/miyoomini/)
#   dist/muos/<Name>/          RG35XX-SP under muOS      (the installed app folder)
#   dist/muos/<Name>.muxapp    the same, for muOS's Archive Manager (put it in ARCHIVE/)
#
#   ./package.sh                       # every app, every platform it supports
#   ./package.sh muos                  # every muOS app
#   ./package.sh minui music wifi      # these apps for MinUI
#
# Run inside the Nix dev shell (direnv): it provides the cross toolchains.
set -e
cd "$(dirname "$0")"
ROOT=$PWD

# app dir | display name | binary | platforms
APPS="
web-browser|Web Browser|web_browser|minui muos
music|Music|music|minui muos
wifi|Wifi|wifi|minui
settings|Settings|settings|minui
pocket-pet|Pocket Pet|pocket_pet|minui
"

PLATFORMS="minui muos"
case "$1" in
minui | muos) PLATFORMS=$1; shift ;;
all) shift ;;
esac
WANT="$*"

# Target triple + cargo features for an app on a platform.
build_args() { # app platform
	case "$2" in
	minui)
		case "$1" in
		music) echo "armv7-unknown-linux-gnueabihf --features minui" ;; # vendor audio needs glibc
		pocket-pet) echo "armv7-unknown-linux-musleabihf --features device" ;;
		*) echo "armv7-unknown-linux-musleabihf --features minui" ;;
		esac ;;
	muos) echo "aarch64-unknown-linux-musl --features muos" ;;
	esac
}

echo "$APPS" | while IFS='|' read -r dir name bin platforms; do
	[ -n "$dir" ] || continue
	if [ -n "$WANT" ] && ! echo " $WANT " | grep -q " $dir "; then
		continue
	fi
	for plat in $PLATFORMS; do
		echo " $platforms " | grep -q " $plat " || continue
		set -- $(build_args "$dir" "$plat")
		target=$1
		shift
		echo "== $name ($plat, $target)"
		if [ "$dir" = music ] && [ "$plat" = minui ] && [ -z "$MIYOO_TOOLCHAIN" ]; then
			echo "   skipped: needs MIYOO_TOOLCHAIN (Nix dev shell on x86_64 Linux)" >&2
			echo "$dir/$plat" >> "$ROOT/target/.package-failed"
			continue
		fi
		if ! cargo build --release -p "$bin" --no-default-features "$@" --target "$target"; then
			echo "$dir/$plat" >> "$ROOT/target/.package-failed"
			continue
		fi
		src="apps/$dir/pak/$plat"
		case "$plat" in
		minui)
			out="dist/minui/$name.pak"
			rm -rf "$out" && mkdir -p "$out"
			cp -R "$src"/. "$out"/
			;;
		muos)
			out="dist/muos/$name"
			rm -rf "$out" && mkdir -p "$out"
			cp -R "$src"/. "$out"/
			cp packaging/muos/font/AtkinsonHyperlegible-Bold.ttf "$out/font.ttf"
			cp packaging/muos/font/OFL.txt "$out/font-OFL.txt"
			;;
		esac
		cp "target/$target/release/$bin" "$out/$bin"
		chmod +x "$out/$bin" "$out"/*.sh
		if [ "$plat" = muos ]; then
			# A .muxapp is a zip whose top level is the app folder; muOS unzips it
			# into its application directory.
			rm -f "dist/muos/$name.muxapp"
			(cd dist/muos && zip -qr -X "$name.muxapp" "$name")
			echo "   built $out and dist/muos/$name.muxapp"
		else
			echo "   built $out"
		fi
	done
done

echo
for plat in $PLATFORMS; do
	[ -d "dist/$plat" ] && { echo "dist/$plat:"; ls -1 "dist/$plat" | sed 's/^/  /'; }
done
if [ -s target/.package-failed ]; then
	echo "Failed: $(tr '\n' ' ' < target/.package-failed)" >&2
	rm -f target/.package-failed
	exit 1
fi
