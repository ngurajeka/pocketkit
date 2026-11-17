# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Repository

MinUI paks for the Miyoo Mini / Mini+, all in Rust. A pak on the device is a folder `<Name>.pak/` containing a `launch.sh` that MinUI runs; it goes in `Tools/miyoomini/` on the SD card. Each project's `package.sh` assembles it under `<project>/dist/`.

- `pocket-pet/` → `Pocket Pet.pak`: virtual pet game. Standalone (own 320x240 renderer, 2x-scaled device backend).
- `web-browser/` → `Web Browser.pak`: text-mode browser + downloader (HTTPS via rustls/ring).
- `wifi/` → `Wifi.pak`: Wi-Fi network picker (replaced an earlier shell-script Wifi.pak, see git history).
- `music/` → `Music.pak`: music player styled like a classic late-90s desktop player (original art drawn in `skin.rs`). The only pak built for **glibc** (`armv7-unknown-linux-gnueabihf`), because audio goes through the vendor `MI_AO` library (`src/ao_shim.c`); needs the Miyoo Mini toolchain (see Environment).
- `settings/` → `Settings.pak`: screen-off time for our paks, MinUI brightness/RTC/560p, Wi-Fi/ADB at startup (managed block in `$USERDATA_PATH/auto.sh`, MinUI's boot hook, backed up once to `auto.sh.before-settings-pak`), music folder, browser search engine.
- `common/` (crate `minui_common`): shared by web-browser, wifi, music and settings: 640x480 `Canvas` + 8x8 font (`font8x8`), `Button`, on-screen `Keyboard`, `Platform` backends (incl. backlight), `minui` (MinUI's shared settings: volume, brightness), `settings` (our `key=value` file `$USERDATA_PATH/minui-paks.txt`), and `run()` (the 30 fps loop; MENU quits; screen off after the `screen_off` setting or with POWER, first press only wakes) driving anything that implements `minui_common::App`.

Every project is its own crate with its own `Cargo.lock`/`target` (no workspace).

## Environment

The root `flake.nix` + `.envrc` (`use flake`, via direnv) provide the toolchain: stable Rust with the `armv7-unknown-linux-musleabihf` target, and the X11/Wayland libs that `minifb` dlopens for the desktop preview (via `LD_LIBRARY_PATH`).

Cross-compiling uses plain `cargo` + `rust-lld` against Rust's bundled self-contained musl; the dev shell sets `BUILD_CMD="cargo build"` and the linker, which `package.sh` picks up instead of its `cross` default. For crates with small libc-free C parts (ring, used by rustls), the shell also sets `CC/CFLAGS/AR_armv7_unknown_linux_musleabihf` to unwrapped clang + llvm-ar. There are no musl headers, so C code that includes libc headers (SDL etc.) won't build without adding a real musl cross toolchain. (`cargo-zigbuild` from nixpkgs fails for this target: it writes into zig's read-only store path.)

The music pak needs the Miyoo Mini toolchain MinUI builds with (Linaro GCC 8.3 + glibc 2.28 sysroot containing `libmi_ao`/`libmi_sys`/`libmi_common` and `mi_ao.h`): <https://github.com/shauninman/miyoomini-toolchain-buildroot/releases/tag/v0.0.3> (sha256 `e158b6eb...4f2e`). The flake fetches it as `miyooToolchain` (x86_64-linux only; host tools autoPatchelf'd, the bundled gdb's missing deps ignored, broken-symlink check off because the sysroot mirrors a device filesystem) and sets `MIYOO_TOOLCHAIN`, `CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_LINKER`, `CC_/AR_armv7_unknown_linux_gnueabihf`; the Rust toolchain includes the `armv7-unknown-linux-gnueabihf` target. `music/build.rs` links `libmi_ao/mi_sys/mi_common` statically from the sysroot, so the binary only needs the device's glibc (≤ 2.28). Run glibc ARM builds under qemu with `qemu-arm -L $MIYOO_TOOLCHAIN/arm-linux-gnueabihf/libc <bin>`. The root `package.sh` keeps going when one pak fails and lists failures at the end.

ARM builds can be smoke-tested with `qemu-arm` (`nix build nixpkgs#qemu-user`), e.g. web-browser's `fetch` example built for the target.

## Commands

From the repo root, `./package.sh` builds every project that has a `package.sh` (or `./package.sh wifi web-browser` for some) and collects the paks in the root `dist/`, ready to copy to `Tools/miyoomini/`.

`./deploy.sh [project...]` builds and copies paks to the Mini over Wi-Fi with adb (from the dev shell): on the Mini, connect with Wifi then open MinUI's ADBD.pak (adbd on port 5555, no auth). The Mini's IP is set once with `--ip` and kept in the untracked `.miyoo-ip`; `--no-build` skips building. It always replaces binaries and `launch.sh` but never overwrites `*.txt` already on the Mini (user-edited config). adb's background server inherits stdout, so redirect it (`adb start-server >/dev/null 2>&1`) or piped commands hang. With adbd running you can also read device logs, e.g. `adb shell cat /mnt/SDCARD/.userdata/miyoomini/music/log.txt`.

Inside the dev shell, from a project directory:

```sh
./package.sh                     # device build -> dist/<Name>.pak
cargo run --release              # desktop preview window
cargo test                       # cargo test <name> for a single test
cargo clippy --all-targets
cargo run --example snapshot     # snapshots/*.ppm of the screens (pocket-pet, web-browser, wifi)
```

Project-specific:
- pocket-pet: `PET_SPEED=60 cargo run` (needs tick faster); saves to `pet_save.txt` (`PET_SAVE_PATH`).
- web-browser: `cargo run --example fetch -- <url|search>` prints a page as laid out on screen; `--save <url>` downloads into `./downloads`. Env: `BROWSER_BOOKMARKS` (default `pak/bookmarks.txt`), `BROWSER_DOWNLOAD_DIR` (default `./downloads`), `BROWSER_SEARCH_URL` (default Brave Search; DuckDuckGo is unreachable from this dev machine, Mojeek/Startpage serve JS captchas).
- music: `MUSIC_DIR=~/Music cargo run --release` (desktop audio is piped to `pw-play --raw`/`paplay`/`aplay`); tests generate WAV tones (`decoder::write_test_wav`) and play them through `audio::NullSink`. Env: `MUSIC_DIR`, `MUSIC_STATE`.
- wifi: the desktop build uses a simulated backend (`backend::Fake`, correct password `password123`); `tests/flow.rs` drives the whole app through it.

Desktop keys (common): arrows = D-pad, Z/Space = A, X = B, C = X, V = Y, Q/W = L1/R1, E/R = L2/R2, P = Power, Enter = Start, Tab = Select, Esc = Menu. Device key codes: L2 = 15, R2 = 14, Power = 116 (MinUI's miyoomini layout).

## Architecture

- Cargo features select the backend: `desktop` (default, minifb window) vs `device` (`package.sh` builds `--no-default-features --features device`). The `Platform` trait (`poll()` buttons in, `present()` a canvas out) is the only screen/input boundary; logic is pure and unit-tested on the host.
- Device backends write straight to `/dev/fb0` and read evdev `/dev/input/event0` (common's is native 640x480 with software key repeat for D-pad/L1/R1/B and a 180-degree flip, `FB_ROTATE=0` disables it). Both backends rotate every frame 180 degrees because the Mini's panel is mounted upside down (confirmed on hardware). The paks run on a Miyoo Mini+; `input_event` is 16 bytes on 32-bit ARM vs 24 on 64-bit hosts.
- web-browser: `html.rs` turns HTML into `Doc` (text + link nodes) and word-wraps it (`layout`), `net.rs` fetches on a background thread (ureq 2 + rustls; non-page content types are streamed to a `.part` file then renamed; a custom cert verifier uses the build time as "now" when the device clock is behind it), `app.rs` holds screens/history/link navigation.
- wifi: `worker.rs` runs a `Backend` on a thread (Cmd/Ev channels). `device.rs` brings Wi-Fi up like MinUI's ADBD.pak (`8188fu.ko`, `/customer/app/axp_test wifion`, `wpa_supplicant -B -D nl80211`, `udhcpc`) and does everything else via wpa_supplicant's control socket (`wpa.rs`, `/var/run/wpa_supplicant/wlan0`), so `wpa_cli`/`iw` aren't required. Networks persist in `/appconfigs/wpa_supplicant.conf` (`WIFI_CONF`); a new network is removed again if it fails to connect.
- MinUI facts that shaped things: its auto-sleep (30 s) and sleep→power-off (2 min) are compile-time constants (`SLEEP_DELAY` in `workspace/all/common/api.c`), not settings; its options are flag files in `$USERDATA_PATH` (`enable-rtc`, `enable-560p`; `enable-simple-mode` in the shared userdata hides Tools, so we don't offer it); it runs `$USERDATA_PATH/auto.sh` at boot with `SDCARD_PATH`/`PLATFORM` exported. On the Mini+ it also runs the firmware `audioserver` and exports `LD_PRELOAD=/customer/lib/libpadsp.so`, so audio must go through `/dev/dsp` (see `music/src/ao_shim.c` for that library's crash workarounds). `wifi --up` brings Wi-Fi up headless (used by the auto.sh block).
- Each pak's `launch.sh` redirects stdout/stderr to a `log.txt` (pocket-pet/web-browser under `$USERDATA_PATH/<name>/`, wifi in the pak folder); check it first when something fails on device.
- Release profiles are size-optimized (`opt-level="s"`, LTO, `panic="abort"`, strip).
