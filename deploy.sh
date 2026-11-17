#!/bin/sh
# Builds pocketkit apps and copies them to a device over Wi-Fi.
#
#   ./deploy.sh                          # MinUI (Miyoo, adb): every app
#   ./deploy.sh music wifi               # only these apps
#   ./deploy.sh --muos music             # muOS (RG35XX-SP, ssh/scp)
#   ./deploy.sh --ip 192.168.1.3         # set the address (remembered per platform:
#                                        #   .miyoo-ip / .muos-ip, both git-ignored)
#   ./deploy.sh --no-build music         # copy what's already in dist/
#
# MinUI: on the Mini, connect with Wifi and open ADBD (or turn on "ADB at
# startup" in Settings). muOS: enable SSH in its web services; log in as root
# (muOS's default password is "root"; run `ssh-copy-id root@<ip>` once, or set
# SSHPASS to use sshpass). Quit an app on the device before updating it.
#
# Binaries and launchers are always replaced. Other *.txt files in an app (like
# Web Browser's bookmarks.txt) are only copied if they aren't on the device yet,
# so your edits there are kept.
set -e
cd "$(dirname "$0")"
ROOT=$PWD
PLATFORM=minui
BUILD=1
APPS=""
NEW_IP=""

while [ $# -gt 0 ]; do
	case "$1" in
	--muos) PLATFORM=muos; shift ;;
	--minui) PLATFORM=minui; shift ;;
	--ip) NEW_IP=$2; shift 2 ;;
	--no-build) BUILD=; shift ;;
	-h | --help) sed -n '2,18p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
	*) APPS="$APPS ${1%/}"; shift ;;
	esac
done

case "$PLATFORM" in
minui) IP_FILE="$ROOT/.miyoo-ip"; DEST=/mnt/SDCARD/Tools/miyoomini ;;
muos) IP_FILE="$ROOT/.muos-ip"; DEST=/run/muos/storage/application ;;
esac
[ -n "$NEW_IP" ] && echo "$NEW_IP" > "$IP_FILE"
IP=${NEW_IP:-$(cat "$IP_FILE" 2>/dev/null || true)}
if [ -z "$IP" ]; then
	echo "Which address? Run: ./deploy.sh $([ $PLATFORM = muos ] && echo '--muos ')--ip <device IP>" >&2
	exit 1
fi

# Apps for this platform: the table in package.sh (dir|name|binary|platforms).
table() { sed -n '/^APPS="/,/^"/p' package.sh | grep '|'; }
[ -n "$APPS" ] || APPS=$(table | grep " *$PLATFORM" | cut -d'|' -f1 | tr '\n' ' ')

if [ -n "$BUILD" ]; then
	# shellcheck disable=SC2086
	./package.sh "$PLATFORM" $APPS
fi

# ------------------------------------------------------------ transport
if [ "$PLATFORM" = minui ]; then
	case "$IP" in *:*) DEV=$IP ;; *) DEV=$IP:5555 ;; esac
	command -v adb >/dev/null || { echo "adb not found (it's in the Nix dev shell)" >&2; exit 1; }
	# Keep adb's background server off our stdout so pipes don't hang on it.
	adb start-server >/dev/null 2>&1 </dev/null
	connected=
	for try in 1 2 3; do # the Mini's Wi-Fi may be dozing
		if timeout 15 adb connect "$DEV" </dev/null 2>&1 | grep -q "connected"; then connected=1; break; fi
		echo "connect $DEV: attempt $try failed, retrying..."
		sleep 2
	done
	remote() { timeout 30 adb -s "$DEV" shell "$1" </dev/null; }
	push() { timeout 120 adb -s "$DEV" push "$1" "$2" </dev/null 2>&1; }
	if [ -z "$connected" ] || ! remote true >/dev/null 2>&1; then
		echo "Can't reach the Mini at $DEV. Is Wi-Fi on and ADBD running?" >&2
		exit 1
	fi
else
	SSH_OPTS="-o ConnectTimeout=8 -o StrictHostKeyChecking=accept-new"
	WRAP=""
	if [ -n "$SSHPASS" ] && command -v sshpass >/dev/null; then
		# sshpass only answers the plain password prompt, not keyboard-interactive.
		WRAP="sshpass -e"
		SSH_OPTS="$SSH_OPTS -o PreferredAuthentications=password -o NumberOfPasswordPrompts=1"
	fi
	# shellcheck disable=SC2086
	remote() { timeout 30 $WRAP ssh $SSH_OPTS "root@$IP" "$1" </dev/null; }
	# shellcheck disable=SC2086
	push() { timeout 120 $WRAP scp -q $SSH_OPTS "$1" "root@$IP:$2" </dev/null 2>&1; }
	remote true >/dev/null 2>&1 || {
		echo "Can't reach muOS at $IP over ssh (enable SSH in muOS; see ./deploy.sh --help)" >&2
		exit 1
	}
fi

# ---------------------------------------------------------------- copy
for app in $APPS; do
	name=$(table | grep "^$app|" | cut -d'|' -f2)
	case "$PLATFORM" in
	minui) src="dist/minui/$name.pak"; target="$DEST/$name.pak" ;;
	muos) src="dist/muos/$name"; target="$DEST/$name" ;;
	esac
	[ -d "$src" ] || { echo "== $app: nothing built for $PLATFORM, skipped"; continue; }
	echo "== $name -> $target"
	remote "mkdir -p '$target'"
	for f in "$src"/*; do
		file=$(basename "$f")
		case "$file" in
		*.txt)
			if remote "test -e '$target/$file'"; then
				echo "   keep $file (already on the device)"
				continue
			fi ;;
		esac
		if ! out=$(push "$f" "$target/$file"); then
			echo "$out" >&2
			echo "Couldn't copy $file. If $name is running on the device, quit it and deploy again." >&2
			exit 1
		fi
		echo "   copied $file"
	done
done
remote sync
echo "Done."
