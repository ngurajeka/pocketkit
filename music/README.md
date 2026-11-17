# Music

A music player for the Miyoo Mini+ (MinUI) in the style of the classic late-90s desktop
players: LED time display, scrolling title, spectrum analyzer, volume slider, transport
buttons and a playlist window. All artwork is original and drawn in code.

Plays MP3, FLAC, OGG Vorbis, WAV and AAC/M4A (decoded in pure Rust by Symphonia) from
`Music/` at the root of the SD card, subfolders included, in path order.

## Controls

| Mini+ | Desktop preview | Action |
|---|---|---|
| Up / Down | arrows | move in the playlist |
| A | Z or Space | play the selected track |
| START | Enter | pause / resume (or play) |
| B | X | stop |
| L1 / R1 | Q / W | previous / next track (L1 restarts the track after 3 s) |
| Left / Right | arrows | seek -5 s / +5 s (hold to keep seeking) |
| L2 / R2 | E / R | volume down / up |
| X | C | shuffle on / off |
| Y | V | repeat: off / all / one |
| SELECT | Tab | jump to the playing track |
| MENU | Esc | quit (volume, shuffle, repeat and the last track are remembered) |

The player's volume works on top of the system volume (the Mini's volume keys).

## Develop on your computer

```sh
MUSIC_DIR=~/Music cargo run --release   # preview window; audio through pw-play, paplay or aplay
cargo test                              # decoder, resampler, FFT, play order, player thread, screen
cargo run --example snapshot            # snapshots/*.ppm (generated test tones, no audio)
```

Without `MUSIC_DIR` it uses `~/Music` (or `./music`); settings go to `./state.txt`
(`MUSIC_STATE`).

## Build for the Miyoo Mini+

```sh
./package.sh     # -> dist/Music.pak   (needs MIYOO_TOOLCHAIN, see below)
```

Audio on the Mini goes through the SigmaStar vendor library `MI_AO` (`src/ao_shim.c`),
which is only available for glibc, so this pak is built for `armv7-unknown-linux-gnueabihf`
with the Miyoo Mini toolchain (Linaro GCC 8.3, glibc 2.28 sysroot with `libmi_ao`) that
MinUI itself uses:
<https://github.com/shauninman/miyoomini-toolchain-buildroot/releases/tag/v0.0.3>.
The vendor libraries are linked statically; only the device's glibc is used at runtime.
The repo's Nix dev shell downloads it and sets `MIYOO_TOOLCHAIN` and the compiler/linker
variables (x86_64 Linux only).

Copy `dist/Music.pak` into `Tools/miyoomini/` and your music into `Music/` on the SD card.
Errors go to `.userdata/.../music/log.txt`.

## Status

Verified here: decoding, seeking, resampling, spectrum, play order, settings, the player
thread and the screen (tests and snapshots), plus the ARM glibc build running under
`qemu-arm`. **Not yet run on the device**: the MI_AO output follows the Miyoo Mini SDL
audio driver's settings but hasn't played sound yet. If it fails, `log.txt` shows the
MI_AO error and the player keeps running silently.
