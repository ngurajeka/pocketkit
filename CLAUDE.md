# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Repository

MinUI paks for the Miyoo Mini / Mini+, all in Rust. A pak on the device is a folder `<Name>.pak/` containing a `launch.sh` that MinUI runs; it goes in `Tools/miyoomini/` on the SD card. Each project's `package.sh` assembles it under `<project>/dist/`.

- `pocket-pet/` → `Pocket Pet.pak`: virtual pet game. Standalone (own 320x240 renderer, 2x-scaled device backend).
- `web-browser/` → `Web Browser.pak`: text-mode browser + downloader (HTTPS via rustls/ring).
- `wifi/` → `Wifi.pak`: Wi-Fi network picker (replaced an earlier shell-script Wifi.pak, see git history).
- `common/` (crate `minui_common`): shared by web-browser and wifi: 640x480 `Canvas` + 8x8 font (`font8x8`), `Button`, on-screen `Keyboard`, `Platform` backends, and `run()` (the 30 fps loop; MENU quits) driving anything that implements `minui_common::App`.

Every project is its own crate with its own `Cargo.lock`/`target` (no workspace).

## Environment

The root `flake.nix` + `.envrc` (`use flake`, via direnv) provide the toolchain: stable Rust with the `armv7-unknown-linux-musleabihf` target, and the X11/Wayland libs that `minifb` dlopens for the desktop preview (via `LD_LIBRARY_PATH`).

Cross-compiling uses plain `cargo` + `rust-lld` against Rust's bundled self-contained musl; the dev shell sets `BUILD_CMD="cargo build"` and the linker, which `package.sh` picks up instead of its `cross` default. For crates with small libc-free C parts (ring, used by rustls), the shell also sets `CC/CFLAGS/AR_armv7_unknown_linux_musleabihf` to unwrapped clang + llvm-ar. There are no musl headers, so C code that includes libc headers (SDL etc.) won't build without adding a real musl cross toolchain. (`cargo-zigbuild` from nixpkgs fails for this target: it writes into zig's read-only store path.)

ARM builds can be smoke-tested with `qemu-arm` (`nix build nixpkgs#qemu-user`), e.g. web-browser's `fetch` example built for the target.

## Commands

From the repo root, `./package.sh` builds every project that has a `package.sh` (or `./package.sh wifi web-browser` for some) and collects the paks in the root `dist/`, ready to copy to `Tools/miyoomini/`.

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
- wifi: the desktop build uses a simulated backend (`backend::Fake`, correct password `password123`); `tests/flow.rs` drives the whole app through it.

Desktop keys (common): arrows = D-pad, Z/Space = A, X = B, C = X, V = Y, Q/W = L1/R1, Enter = Start, Tab = Select, Esc = Menu.

## Architecture

- Cargo features select the backend: `desktop` (default, minifb window) vs `device` (`package.sh` builds `--no-default-features --features device`). The `Platform` trait (`poll()` buttons in, `present()` a canvas out) is the only screen/input boundary; logic is pure and unit-tested on the host.
- Device backends write straight to `/dev/fb0` and read evdev `/dev/input/event0` (common's is native 640x480 with software key repeat for D-pad/L1/R1/B and an `FB_ROTATE=180` env switch). **None of this is verified on hardware yet**: fb format/stride and key codes are best guesses; `input_event` is 16 bytes on 32-bit ARM vs 24 on 64-bit hosts.
- web-browser: `html.rs` turns HTML into `Doc` (text + link nodes) and word-wraps it (`layout`), `net.rs` fetches on a background thread (ureq 2 + rustls; non-page content types are streamed to a `.part` file then renamed; a custom cert verifier uses the build time as "now" when the device clock is behind it), `app.rs` holds screens/history/link navigation.
- wifi: `worker.rs` runs a `Backend` on a thread (Cmd/Ev channels). `device.rs` brings Wi-Fi up like MinUI's ADBD.pak (`8188fu.ko`, `/customer/app/axp_test wifion`, `wpa_supplicant -B -D nl80211`, `udhcpc`) and does everything else via wpa_supplicant's control socket (`wpa.rs`, `/var/run/wpa_supplicant/wlan0`), so `wpa_cli`/`iw` aren't required. Networks persist in `/appconfigs/wpa_supplicant.conf` (`WIFI_CONF`); a new network is removed again if it fails to connect.
- Each pak's `launch.sh` redirects stdout/stderr to a `log.txt` (pocket-pet/web-browser under `$USERDATA_PATH/<name>/`, wifi in the pak folder); check it first when something fails on device.
- Release profiles are size-optimized (`opt-level="s"`, LTO, `panic="abort"`, strip).
