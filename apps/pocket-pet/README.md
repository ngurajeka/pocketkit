# Pocket Pet

A tiny virtual pet for the Miyoo Mini+ (MinUI), written in Rust.
Feed, play with, wash and put your pet to bed. Its needs keep ticking while the
console is off (up to 8 hours' worth).

No microphone on the Mini+, so there's no voice mimic. Everything is drawn in
code (`src/render.rs`), so there are no art assets yet.

## Controls

| Mini+ | Desktop preview | Action |
|---|---|---|
| Left / Right (or L1 / R1) | Left / Right (or A / S) | Choose Feed, Play, Sleep, Clean |
| A | Z or Space | Do it (a head-shake means "not now") |
| MENU | Esc | Save and quit |

## Develop on your computer

```sh
cargo run --release          # opens a 640x480 preview window
cargo test                   # game logic, save format, renderer smoke tests
cargo run --example snapshot # writes snapshots/*.ppm, one per mood
PET_SPEED=60 cargo run       # needs run 60x faster, handy for testing moods
```

Saves go to `pet_save.txt` (override with `PET_SAVE_PATH`).

## Build for the Miyoo Mini+

```sh
rustup target add armv7-unknown-linux-musleabihf
cargo install cross            # needs Docker; or use cargo-zigbuild, see package.sh
./package.sh                   # -> dist/Pocket Pet.pak
```

Copy `dist/Pocket Pet.pak` into `Tools/miyoomini/` on the SD card (use a spare
card, not your main one). After a run, check `.userdata/.../pocket-pet/log.txt`
for errors.

## Status

Game logic, save/load and the renderer are unit tested; the pak runs on a Miyoo Mini+.
The device backend (`src/platform/device.rs`) rotates every frame 180 degrees because
the panel is mounted upside down. If the screen ever stays black or garbled, read
`log.txt` and check `/sys/class/graphics/fb0/{virtual_size,bits_per_pixel,stride}`.

## Layout

- `src/game.rs`: pet stats, actions, moods, save format (pure, tested)
- `src/render.rs`: software renderer (320x240, scaled 2x on device)
- `src/app.rs`: input handling, autosave, offline catch-up
- `src/platform/`: `desktop.rs` (preview window) and `device.rs` (Mini+)
