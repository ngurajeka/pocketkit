# Wifi

Pick a Wi-Fi network on screen and connect to it, on the Miyoo Mini+ (MinUI). Written in
Rust; replaces the old shell-script Wifi.pak that only read `networks.txt`.

Opening it turns Wi-Fi on and scans. Networks are listed strongest first with signal bars;
saved ones are marked `saved`, the current one `connected`. Wi-Fi stays on when you quit,
so other paks (like Web Browser) can use it.

## Controls

| Mini+ | Desktop preview | Action |
|---|---|---|
| Up / Down (L1 / R1 page) | arrows (Q / W) | choose a network |
| A | Z or Space | connect (asks for the password the first time) |
| SELECT | Tab | type a new password for the selected network |
| Y | V | forget the selected saved network |
| X | C | scan again |
| START | Enter | turn Wi-Fi off / on |
| MENU | Esc | quit (Wi-Fi stays as it is) |

On the password keyboard: A types, B deletes, Y space, X switches abc / ABC / symbols,
L1 clears, START connects, SELECT goes back.

Open and WPA/WPA2 password networks work. WEP and WPA-Enterprise (802.1X) are listed but
not supported.

## Setup

1. Driver: install ADBD.pak from the MinUI Extras (it contains `8188fu.ko`), or copy
   `8188fu.ko` into `Wifi.pak/`.
2. Copy `dist/Wifi.pak` (see below) to `Tools/miyoomini/Wifi.pak` on the SD card.

Networks are saved in `/appconfigs/wpa_supplicant.conf`, the stock firmware's file. The
first run adds `ctrl_interface` / `update_config` to it if missing, keeping a copy of the
original in `Wifi.pak/backup/`.

## Develop on your computer

```sh
cargo run --release           # preview window with simulated networks; "password123" works
cargo test                    # parsers, control socket, screens, full flow on the simulator
cargo run --example snapshot  # snapshots/*.ppm of both screens
./package.sh                  # device build -> dist/Wifi.pak (inside the Nix dev shell)
```

## How it works

Bring-up follows MinUI's ADBD.pak: load `8188fu.ko`, `/customer/app/axp_test wifion`,
`ifconfig wlan0 up`, `wpa_supplicant -B -D nl80211 -i wlan0 -c <conf>`, `udhcpc`. Scanning,
connecting and saving go straight to wpa_supplicant's control socket
(`/var/run/wpa_supplicant/wlan0`), so `wpa_cli` and `iw` aren't needed. A new network is
only saved if it actually connects.

Works on a Miyoo Mini+. Every command it runs and its errors go to `Wifi.pak/log.txt`;
check that first if something fails.
