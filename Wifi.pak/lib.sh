# lib.sh - helpers for Wifi.pak. Plain POSIX sh so it runs on the device's busybox.
# The bring-up commands follow MinUI's own ADBD.pak for the Miyoo Mini Plus.

IFACE=wlan0
CONF="${WPA_CONF:-/appconfigs/wpa_supplicant.conf}"

# say LINE...  Show a message on screen (MinUI's say.elf) and print it to the log.
say() {
	msg=""
	for l in "$@"; do
		[ -n "$l" ] || continue
		if [ -n "$msg" ]; then
			msg="$msg
$l"
		else
			msg="$l"
		fi
	done
	echo "say: $msg"
	if command -v say.elf >/dev/null 2>&1; then
		say.elf "$msg"
	fi
}

trim() {
	printf '%s' "$1" | sed 's/^[[:space:]]*//; s/[[:space:]]*$//'
}

# Prefers `ip`; falls back to `ifconfig` (busybox may lack `ip`).
current_ip() {
	if command -v ip >/dev/null 2>&1; then
		ip -4 addr show dev "$IFACE" 2>/dev/null | awk '/inet / {split($2, a, "/"); print a[1]; exit}'
	else
		ifconfig "$IFACE" 2>/dev/null | sed -n 's/.*inet \(addr:\)\{0,1\}\([0-9.]*\).*/\2/p' | head -n 1
	fi
}

# Prefers wpa_cli (talks to our wpa_supplicant); falls back to iw. Prints nothing if unknown.
current_ssid() {
	if command -v wpa_cli >/dev/null 2>&1; then
		wpa_cli -i "$IFACE" status 2>/dev/null | sed -n 's/^ssid=//p' | head -n 1
	elif command -v iw >/dev/null 2>&1; then
		iw dev "$IFACE" link 2>/dev/null | awk -F': ' '/SSID/ {print $2; exit}'
	fi
}

wifi_is_on() {
	pidof wpa_supplicant >/dev/null 2>&1
}

# Load the Realtek driver. MinUI doesn't load it by default; ADBD.pak ships a copy.
load_driver() {
	[ -d /sys/module/8188fu ] && return 0
	for ko in "$PAK_DIR/8188fu.ko" "$PAK_DIR/../ADBD.pak/8188fu.ko"; do
		if [ -f "$ko" ]; then
			insmod "$ko" && return 0
		fi
	done
	return 1
}

wifi_up() {
	/customer/app/axp_test wifion
	sleep 2
	ifconfig "$IFACE" up
	wpa_supplicant -B -D nl80211 -i"$IFACE" -c "$CONF"
	# Detach from log.txt: udhcpc outlives the pak and would keep writing to it on every renewal.
	udhcpc -i "$IFACE" -s /etc/init.d/udhcpc.script >/dev/null 2>&1 &
	if command -v iw >/dev/null 2>&1; then
		iw dev "$IFACE" set power_save off
	fi
}

wifi_down() {
	pkill -9 wpa_supplicant
	pkill -9 udhcpc
	ifconfig "$IFACE" down
	/customer/app/axp_test wifioff
}

# wait_for_ip SECONDS: print the IP as soon as DHCP hands one out.
wait_for_ip() {
	i=0
	while [ "$i" -lt "$1" ]; do
		ip=$(current_ip)
		if [ -n "$ip" ]; then
			echo "$ip"
			return 0
		fi
		sleep 1
		i=$((i + 1))
	done
	return 1
}

has_saved_network() {
	[ -f "$CONF" ] && grep -q '^[[:space:]]*network[[:space:]]*=' "$CONF"
}

# Everything before the first network={ block (ctrl_interface and friends).
header_of() {
	if [ -f "$1" ]; then
		sed '/^[[:space:]]*network[[:space:]]*=/,$d' "$1"
	fi
}

# build_conf INPUT OUTPUT BACKUP
#
# Turns a networks.txt into a wpa_supplicant.conf, keeping OUTPUT's existing
# header lines and saving a copy of the old OUTPUT to BACKUP.
# Input format, one network per line:   SSID | PASSWORD
#   - leave the password empty (or omit the "|") for an open network
#   - blank lines and lines starting with # are ignored
# Prints the number of networks written. Returns 1 (and touches nothing) if
# no line was valid. Problems are reported on stderr.
build_conf() {
	in="$1"
	out="$2"
	backup="$3"
	tmp="$out.new"

	hdr=$(header_of "$out")
	if [ -z "$hdr" ]; then
		hdr="ctrl_interface=/var/run/wpa_supplicant
update_config=1"
	fi
	printf '%s\n' "$hdr" >"$tmp" || return 1

	count=0
	lineno=0
	while IFS= read -r line || [ -n "$line" ]; do
		lineno=$((lineno + 1))
		line=$(printf '%s' "$line" | tr -d '\r')
		case "$line" in
		'' | '#'*) continue ;;
		esac
		case "$line" in
		*'|'*)
			ssid=$(trim "${line%%|*}")
			pass=$(trim "${line#*|}")
			;;
		*)
			ssid=$(trim "$line")
			pass=""
			;;
		esac
		[ -n "$ssid" ] || continue
		if [ -n "$pass" ] && { [ "${#pass}" -lt 8 ] || [ "${#pass}" -gt 63 ]; }; then
			echo "networks.txt line $lineno: password for \"$ssid\" must be 8-63 characters, skipped" >&2
			continue
		fi
		{
			printf '\nnetwork={\n\tssid="%s"\n' "$ssid"
			if [ -n "$pass" ]; then
				printf '\tpsk="%s"\n\tkey_mgmt=WPA-PSK\n' "$pass"
			else
				printf '\tkey_mgmt=NONE\n'
			fi
			printf '\tscan_ssid=1\n}\n'
		} >>"$tmp"
		count=$((count + 1))
	done <"$in"

	if [ "$count" -eq 0 ]; then
		rm -f "$tmp"
		return 1
	fi

	if [ -f "$out" ]; then
		mkdir -p "$(dirname "$backup")"
		cp "$out" "$backup"
	fi
	mv "$tmp" "$out" || return 1
	chmod 600 "$out" 2>/dev/null
	echo "$count"
}
