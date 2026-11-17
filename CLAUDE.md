# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Repository

**pocketkit**: Rust apps for retro handhelds, one Cargo workspace (single `Cargo.lock`, `target/` and release profile at the root). Platforms:

- **MinUI** on the Miyoo Mini / Mini+ (ARMv7): an app ships as a pak folder `<Name>.pak/` with a `launch.sh`, installed in `Tools/miyoomini/` on the SD card.
- **muOS** on the Anbernic RG35XX-SP (Allwinner H700, aarch64): an app is a folder in `/run/muos/storage/application/<Name>/` with `mux_launch.sh` (sources `/opt/muos/script/var/func.sh`, calls `SETUP_APP "<binary>" ""`; `# HELP/# ICON/# GRID` header lines are read by the frontend) and `mux_launch.ini`; distributed as `<Name>.muxapp`, a zip whose top level is the app folder (muOS's `script/mux/extract.sh` unzips `.muxapp` into its application dir).

Layout:
- `crates/core` (`pocketkit_core`): device-independent: 640x480 `Canvas` + 8x8 fallback font, `Button` (incl. `Power`, and `Other` for unmapped keys so they count as activity), on-screen `Keyboard`, `ui` (font via `fontdue`; pills/rows/button hints/toasts/keyboard with MinUI's sizes from its `defines.h`/`api.c` at 2x; two `Style`s sharing that layout: `MinUI` and `Muos` (header/footer bars, mustard highlight), picked by `POCKETKIT_STYLE`, which muOS launchers set; apps draw their own extras with `ui.pal`), `settings` (shared `key=value` file), the `Platform` trait, and `run_with()` (30 fps loop; MENU quits, `App::quit` lets B quit; screen off after the `screen_off` setting or with POWER, first press only wakes).
- `crates/platform` (`pocketkit_platform`): Cargo features `desktop` (minifb window), `minui`, `muos`; `open()`/`run(title, app)` pick by feature. `fb.rs` (framebuffer writer, optional 180° flip; with `page_flip` (muOS) it draws into the hidden page of the 640x960 framebuffer and pans to it via `/sys/class/graphics/fb0/pan` to avoid tearing, falling back to the visible page if panning is refused, and restores the start page on drop), `evdev.rs` (key tables per device, ABS_HAT0 D-pad, software key repeat), `minui.rs`/`muos.rs` (device wiring), `minui_settings.rs` (MinUI's shared volume/brightness), `volume.rs` (system volume: MinUI file or PipeWire `wpctl`).
- `apps/*`, each with `pak/minui/` and, where ported, `pak/muos/` packaging files:
  - `web-browser` (MinUI, muOS): text-mode browser + downloader (HTTPS via rustls/ring).
  - `music` (MinUI, muOS): retro desktop-player skin (original art in `skin.rs`), 5 colour themes in `theme.rs` (L2/R2, saved in the state file; a test enforces text contrast). On MinUI it's the only **glibc** build (`armv7-unknown-linux-gnueabihf`, Miyoo toolchain) because audio must go through the firmware audioserver (`src/ao_shim.c`); elsewhere `PipeSink` pipes PCM to `pw-play --raw`/`paplay`/`aplay`, skipping players that exit immediately.
  - `wifi`, `settings` (MinUI only; muOS has its own network/settings menus). Settings manages a marked block in MinUI's boot hook `$USERDATA_PATH/auto.sh` (markers `# >>> minui-paks` / `# <<< minui-paks`, kept for existing installs; backed up once to `auto.sh.before-settings-pak`).
  - `pocket-pet` (MinUI only): still on its own 320x240 renderer and Miyoo backend (`--features device`), not yet on core/platform.
- `packaging/muos/font/`: Atkinson Hyperlegible Bold + `OFL.txt`, copied into every muOS app as `font.ttf` (muOS has no MinUI font). On MinUI the UI font is MinUI's own `$SDCARD_PATH/.system/res/BPreplayBold-unhinted.otf`, never bundled; desktop previews/tests need `tools/fetch-minui-res.sh` once (downloads it to git-ignored `.minui-res/`), else everything falls back to the bitmap font.

## Environment

`flake.nix` + `.envrc` (direnv) provide: stable Rust with `armv7-unknown-linux-musleabihf`, `armv7-unknown-linux-gnueabihf` and `aarch64-unknown-linux-musl`; X11/Wayland libs for the minifb preview; `adb`, `sshpass`, `zip`.

- Static musl builds link with `rust-lld` against Rust's self-contained musl; ring's libc-free C is compiled with unwrapped clang (`CC/CFLAGS/AR_<target>` for both musl targets). No musl headers, so C that includes libc headers won't build. (`cargo-zigbuild` from nixpkgs fails: it writes into zig's read-only store path.)
- MinUI Music needs the Miyoo Mini toolchain MinUI builds with (Linaro GCC 8.3, glibc 2.28 sysroot with `libmi_*`): fetched by the flake as `miyooToolchain` (x86_64-linux only; host tools autoPatchelf'd, bundled gdb's missing deps ignored, broken-symlink check off because the sysroot mirrors a device filesystem); sets `MIYOO_TOOLCHAIN` and the gnueabihf linker/CC/AR. `apps/music/build.rs` compiles `ao_shim.c` only with feature `minui`.
- Smoke-test ARM builds with `qemu-arm`/`qemu-aarch64` (`nix build nixpkgs#qemu-user`); glibc builds need `-L $MIYOO_TOOLCHAIN/arm-linux-gnueabihf/libc`.

## Commands

From the repo root, in the dev shell:

```sh
./package.sh [minui|muos] [app...]   # -> dist/minui/<Name>.pak, dist/muos/<Name>/ + <Name>.muxapp
./deploy.sh [app...]                 # build + copy to the Mini (adb); --no-build, --ip <ip>
./deploy.sh --muos [app...]          # build + copy to muOS (ssh/scp as root)
cargo test --workspace               # cargo test -p <crate> [name] for one
cargo clippy --workspace --all-targets -- -D warnings
cargo run --release -p web_browser   # desktop preview (default feature `desktop`)
cargo run -p music --example music_snapshot   # snapshots/*.ppm (also browser_, wifi_, settings_, pet_snapshot)
```

Publishing: MIT `LICENSE`; root `README.md` (apps, install, build, credits). `docs/screenshots/*.png` are regenerated by `tools/screenshots.sh` (runs each `*_snapshot` example in a temp dir with MinUI's font, converts with `tools/ppm2png.py`). GitHub Actions: `.github/workflows/ci.yml` (clippy `-D warnings`, tests, full `./package.sh`, all inside `nix develop`) and `release.yml` (on a `v*` tag: builds, zips each `dist/minui/*.pak` as `<Name>.pak.zip`, adds the `.muxapp`s and `SHA256SUMS` to a GitHub Release). Bundled font licensing is explained in `packaging/muos/font/README.md`.

`package.sh` holds the app table (dir | display name | binary | platforms) and the target/feature per platform; it keeps going when one build fails (MinUI Music without `MIYOO_TOOLCHAIN` is skipped) and lists failures at the end. `deploy.sh` reads the same table, always replaces binaries/launchers and never overwrites a `*.txt` already on the device. Device IPs live in git-ignored `.miyoo-ip` / `.muos-ip`.

- MinUI deploy: on the Mini, Wifi then ADBD.pak (adbd on 5555, no auth; or Settings → ADB at startup). adb's background server inherits stdout, so start it with `adb start-server >/dev/null 2>&1` or piped commands hang. Logs: `adb shell cat /mnt/SDCARD/.userdata/miyoomini/<app>/log.txt`.
- muOS deploy: enable SSH in muOS; root's default password is `root`. With `SSHPASS=root`, deploy.sh uses sshpass and forces `PreferredAuthentications=password` (sshpass can't answer keyboard-interactive and hangs). Logs: `/root/.config/pocketkit/<app>/log.txt`. Give ssh `</dev/null` in scripts or it can hang.

Project-specific:
- web-browser: `cargo run -p web_browser --example fetch -- <url|search>` prints a page as laid out (char grid via `html::layout_cols`; the app wraps by pixel width with the real font via `html::layout`). Env: `BROWSER_BOOKMARKS`, `BROWSER_DOWNLOAD_DIR`, `BROWSER_SEARCH_URL` (default Brave; DuckDuckGo is unreachable from this dev machine, Mojeek/Startpage serve JS captchas).
- music: `MUSIC_DIR=~/Music cargo run --release -p music`; tests generate WAV tones (`decoder::write_test_wav`) and play them through `audio::NullSink`. Env: `MUSIC_DIR`, `MUSIC_STATE`, `MINUI_SETTINGS`.
- wifi: the desktop build uses `backend::Fake` (correct password `password123`); `tests/flow.rs` drives the whole app through it. `wifi --up` brings Wi-Fi up headless (used by the auto.sh block).
- pocket-pet: `PET_SPEED=60 cargo run -p pocket_pet`.

Desktop keys: arrows = D-pad, Z/Space = A, X = B, C = X, V = Y, Q/W = L1/R1, E/R = L2/R2, P = Power, Enter = Start, Tab = Select, Esc = Menu.

## Device facts (verified on hardware)

- **Miyoo Mini+ / MinUI**: `/dev/fb0` 640x480 32bpp; panel mounted upside down, so frames are flipped 180° (`FB_ROTATE=0` disables). Keys on `event0` as keyboard codes (A=57, B=29, X=42, Y=56, L1=18, R1=20, L2=15, R2=14, Start=28, Select=97, Menu=1, Power=116). Backlight via MinUI's GPIO4/PWM sequence. MinUI's auto-sleep (30 s) and sleep→power-off (2 min) are compile-time constants (`SLEEP_DELAY` in its `api.c`), not settings. Options are flag files in `$USERDATA_PATH` (`enable-rtc`, `enable-560p`; `enable-simple-mode` hides Tools, so it isn't offered). MinUI runs `$USERDATA_PATH/auto.sh` at boot with `SDCARD_PATH`/`PLATFORM` exported. Shared volume/brightness are in `/tmp/SharedSettings` (no /dev/shm on the Mini). On the Mini+ the firmware `audioserver` owns the audio hardware and MinUI exports `LD_PRELOAD=/customer/lib/libpadsp.so`: audio must go through `/dev/dsp`, opened after a warm-up `open()` (libpadsp jumps to NULL if /dev/dsp is the first file opened; `SNDCTL_DSP_RESET` also crashes it) — see `ao_shim.c`.
- **RG35XX-SP / muOS**: `/dev/fb0` 640x480 32bpp, 640x960 virtual, not rotated; no `/sys/class/backlight` (screen off via `/sys/class/graphics/fb0/blank`). Gamepad `muOS-Keys` (event1): A=304, B=305, X=307, Y=306, L1=308, R1=309, L2=314, R2=315, Select=310, Start=311, Menu=312, D-pad = ABS_HAT0X/Y, VOL±=115/114; power `axp2202-pek` (event0, 116). muOS grabs the gamepad while its menu is up and releases it for launched apps. Audio: PipeWire; its old `pw-play` has no `--raw`, but ALSA's default PCM goes through PipeWire (`/etc/asound.conf`), so `aplay` follows the system volume; `wpctl` needs `XDG_RUNTIME_DIR=/run` (set for launched apps). Storage: apps `/run/muos/storage/application`, music `/run/muos/storage/music`.

## Architecture notes

- The `Platform` trait (`poll` buttons, `present` canvas, `set_backlight`) is the only device boundary; app logic is pure and unit-tested on the host. `input_event` is 16 bytes on 32-bit ARM, 24 on 64-bit (handled in `evdev.rs`).
- Shared settings file: `PAKS_SETTINGS`, else `$POCKETKIT_DATA/pocketkit.txt` (muOS launchers set `POCKETKIT_DATA=~/.config/pocketkit`), else `$USERDATA_PATH/minui-paks.txt` (MinUI; old name kept), else `./pocketkit.txt`. Keys: `screen_off`, `music_dir`, `search_url`.
- web-browser: `html.rs` turns HTML into `Doc` (text + link nodes) and wraps it; `net.rs` fetches on a background thread (ureq 2 + rustls; non-page content types are streamed to a `.part` file then renamed; a custom cert verifier uses the build time as "now" when the device clock is behind it).
- wifi: `worker.rs` runs a `Backend` on a thread; `device.rs` brings Wi-Fi up like MinUI's ADBD.pak and drives wpa_supplicant through its control socket (`wpa.rs`), so `wpa_cli`/`iw` aren't required. Networks persist in `/appconfigs/wpa_supplicant.conf`; a new network is removed again if it fails to connect.
- Every launcher redirects stdout/stderr to a `log.txt`; check it first when something fails on a device.
