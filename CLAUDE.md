# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Repository

A collection of MinUI paks for the Miyoo Mini / Mini+. Each top-level directory is one self-contained pak project (currently only `pocket-pet/`, a Rust virtual-pet game). A pak on the device is a folder `<Name>.pak/` containing a `launch.sh` that MinUI runs; it goes in `Tools/miyoomini/` (or `Emus/`) on the SD card.

## Environment

The root `flake.nix` + `.envrc` (`use flake`, via direnv) provide the toolchain for all paks: stable Rust with the `armv7-unknown-linux-musleabihf` target, and the X11/Wayland libs that `minifb` dlopens for the desktop preview (exported via `LD_LIBRARY_PATH`).

Cross-compiling uses plain `cargo` + `rust-lld` linking against Rust's bundled self-contained musl. The dev shell sets `BUILD_CMD="cargo build"` and `CARGO_TARGET_ARMV7_UNKNOWN_LINUX_MUSLEABIHF_LINKER=rust-lld`, which `package.sh` picks up instead of its `cross` default. This only works while a pak has no C dependencies; a pak that needs C/SDL would require adding a musl cross C compiler to the flake. (`cargo-zigbuild` from nixpkgs fails for this target: it tries to write into zig's read-only store path.)

## Commands (pocket-pet)

Run from `pocket-pet/` inside the dev shell:

```sh
./package.sh                     # device build -> dist/Pocket Pet.pak (static armv7 ELF + launch.sh)
cargo run --release              # 640x480 desktop preview window
cargo test                       # game logic, save format, renderer smoke tests
cargo test <name>                # single test
cargo clippy
cargo run --example snapshot     # writes snapshots/*.ppm, one per mood
PET_SPEED=60 cargo run           # needs tick 60x faster
```

Saves go to `pet_save.txt` locally (`PET_SAVE_PATH` overrides). On device, `pak/launch.sh` sets it under `$USERDATA_PATH/pocket-pet/` and redirects stdout/stderr to `log.txt` there.

## pocket-pet architecture

- Cargo features select the backend: `desktop` (default, pulls in `minifb`) vs `device` (zero dependencies, std only). `package.sh` builds with `--no-default-features --features device`. `main.rs` picks the `Platform` impl via `#[cfg(feature = "desktop")]`.
- `platform::Platform` trait is the only hardware boundary: `poll()` buttons in, `present()` a `Canvas` out. Everything else (`game`, `render`, `app`) is pure and unit-tested on the host.
- `game.rs`: stats/actions/moods and the text save format. `app.rs`: input → game glue, autosave, offline catch-up (needs keep ticking while off, capped at 8h). `render.rs`: software renderer into a 320x240 buffer, all art drawn in code (no assets).
- `platform/device.rs` writes directly to `/dev/fb0` (scaled 2x to 640x480) and reads evdev `/dev/input/event0`. **Not yet verified on hardware**: fb path/format/stride and key codes are best guesses. `input_event` size differs between 32-bit ARM (16 bytes) and 64-bit hosts (24). If the screen is black/garbled, check `log.txt` and `/sys/class/graphics/fb0/{virtual_size,bits_per_pixel,stride}` on the device; the fallback plan is an SDL backend.
- Release profile is size-optimized (`opt-level="s"`, LTO, `panic="abort"`, strip).
