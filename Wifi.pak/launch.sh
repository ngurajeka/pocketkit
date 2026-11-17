#!/bin/sh
# Wifi.pak: toggles Wi-Fi on the Miyoo Mini Plus and imports networks from networks.txt.
# Run it once to turn Wi-Fi on (shows the IP address), run it again to turn it off.

PAK_DIR=$(cd "$(dirname "$0")" && pwd)
cd "$PAK_DIR" || exit 1
exec >"$PAK_DIR/log.txt" 2>&1
echo "== Wifi.pak $(date)"

. "$PAK_DIR/lib.sh"

NOTE=""
RESTART=""

# 1. Import networks.txt if there is one.
if [ -f networks.txt ]; then
	if n=$(build_conf networks.txt "$CONF" "$PAK_DIR/backup/wpa_supplicant.conf.bak"); then
		rm -f networks.txt
		NOTE="Saved $n network(s)."
		RESTART=1
	else
		say "No valid networks in networks.txt" "Format:  SSID | PASSWORD" "(see networks.example.txt and log.txt)"
		exit 0
	fi
fi

# 2. Already on and nothing new to load: turn it off.
if wifi_is_on && [ -z "$RESTART" ]; then
	wifi_down
	say "Wi-Fi turned OFF"
	exit 0
fi

# Restarting to pick up freshly saved networks.
if wifi_is_on; then
	wifi_down
	sleep 1
fi

if ! has_saved_network; then
	say "No networks saved yet." "Copy networks.example.txt to" "networks.txt, edit it, run again."
	exit 0
fi

if ! load_driver; then
	say "Wi-Fi driver not found." "Install ADBD.pak (MinUI Extras)" "or copy 8188fu.ko into Wifi.pak."
	exit 0
fi

# 3. Turn it on and wait for an address.
wifi_up
if IP=$(wait_for_ip 20); then
	SSID=$(current_ssid)
	say "$NOTE" "Wi-Fi is ON" "${SSID:+Network: $SSID}" "IP: $IP"
else
	say "$NOTE" "Not connected after 20s." "Check the name and password." "Run Wifi again to turn it off."
fi
