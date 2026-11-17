#!/bin/sh
# Builds paks and copies them to the Miyoo Mini over Wi-Fi with ADB.
#
#   ./deploy.sh                    # build + copy every pak
#   ./deploy.sh music wifi         # only these projects
#   ./deploy.sh --ip 192.168.1.3   # set the Mini's address (remembered in .miyoo-ip)
#   ./deploy.sh --no-build music   # copy what's already in dist/
#
# On the Mini first: connect with Wifi, then open ADBD (Tools). Quit the pak
# you're updating before deploying it, or its binary can't be replaced.
#
# Binaries and launch.sh are always replaced. Other files in a pak (like Web
# Browser's bookmarks.txt) are only copied if they don't exist on the Mini yet,
# so your edits there are kept.
set -e
cd "$(dirname "$0")"
ROOT=$PWD
IP_FILE="$ROOT/.miyoo-ip"
DEST=/mnt/SDCARD/Tools/miyoomini
BUILD=1
PROJECTS=""

while [ $# -gt 0 ]; do
	case "$1" in
	--ip) MIYOO_IP=$2; echo "$2" > "$IP_FILE"; shift 2 ;;
	--no-build) BUILD=; shift ;;
	-h|--help) sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
	*) PROJECTS="$PROJECTS ${1%/}"; shift ;;
	esac
done

MIYOO_IP=${MIYOO_IP:-$(cat "$IP_FILE" 2>/dev/null || true)}
if [ -z "$MIYOO_IP" ]; then
	echo "Which address? Run: ./deploy.sh --ip <the IP the Wifi pak shows>" >&2
	exit 1
fi
case "$MIYOO_IP" in *:*) DEV=$MIYOO_IP ;; *) DEV=$MIYOO_IP:5555 ;; esac

if [ -z "$PROJECTS" ]; then
	PROJECTS=$(for f in */package.sh; do dirname "$f"; done)
fi

if [ -n "$BUILD" ]; then
	# shellcheck disable=SC2086
	./package.sh $PROJECTS
fi

command -v adb >/dev/null || { echo "adb not found (it's in the Nix dev shell)" >&2; exit 1; }
# Keep adb's background server off our stdout so pipes/CI don't hang on it.
adb start-server >/dev/null 2>&1 </dev/null

# The Mini's Wi-Fi may be dozing: the first try can time out.
connected=
for try in 1 2 3; do
	if timeout 15 adb connect "$DEV" </dev/null 2>&1 | grep -q "connected"; then
		connected=1
		break
	fi
	echo "connect $DEV: attempt $try failed, retrying..."
	sleep 2
done
if [ -z "$connected" ] || ! timeout 10 adb -s "$DEV" shell true </dev/null >/dev/null 2>&1; then
	echo "Can't reach the Mini at $DEV. Is Wi-Fi on and ADBD running? (./deploy.sh --ip <ip> to change it)" >&2
	exit 1
fi

on_mini() { timeout 30 adb -s "$DEV" shell "$1" </dev/null; }

for p in $PROJECTS; do
	for pak in "$ROOT/$p"/dist/*.pak; do
		[ -d "$pak" ] || continue
		name=$(basename "$pak")
		target="$DEST/$name"
		echo "== $name -> $target"
		on_mini "mkdir -p '$target'"
		for f in "$pak"/*; do
			file=$(basename "$f")
			case "$file" in
			*.txt)
				if on_mini "test -e '$target/$file'"; then
					echo "   keep $file (already on the Mini)"
					continue
				fi ;;
			esac
			if ! out=$(timeout 120 adb -s "$DEV" push "$f" "$target/$file" </dev/null 2>&1); then
				echo "$out" >&2
				echo "Couldn't copy $file. If $name is running on the Mini, quit it and deploy again." >&2
				exit 1
			fi
			echo "   copied $file"
		done
	done
done
on_mini sync
echo "Done. Paks are in Tools on the Mini."
