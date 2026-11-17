# Settings

Settings for MinUI and for our paks, on the Miyoo Mini+. Up/Down picks a row,
Left/Right (or A) changes it; every change is saved right away.

| Row | What it does |
|---|---|
| Screen off after | Never / 30 s / 1 / 2 / 5 / 10 min of no input, in **our** paks (Music keeps playing). Any button wakes the screen; POWER turns it off right away. |
| Brightness | MinUI's brightness (0-10), same as START + L/R. |
| Wi-Fi at startup | Turns Wi-Fi on at boot with your saved networks (`Wifi.pak/wifi --up`). |
| ADB at startup | Also starts ADB at boot (needs ADBD.pak), so `deploy.sh` works without opening anything. No password: trusted networks only. |
| Hardware clock | MinUI's `enable-rtc`: use the Mini+'s battery-backed clock. Next boot. |
| 560p screen mode | MinUI's `enable-560p`; only shown on screens that support 752x560. |
| Music folder | Where Music looks for songs (type a full path). |
| Browser search | Search engine for Web Browser (Brave, DuckDuckGo Lite, Wiby). |

MinUI's own auto-sleep (30 s in its menus, power-off after 2 min asleep) is compiled
into MinUI and can't be changed from a pak.

## Where things are stored

- Our paks' settings: `.userdata/miyoomini/minui-paks.txt` (read by `common`, so every
  pak using `minui_common::run` honours the screen-off time).
- Startup options: a marked block at the end of `.userdata/miyoomini/auto.sh`, MinUI's
  boot hook. Only the lines between `# >>> minui-paks` and `# <<< minui-paks` are ever
  changed; the original file is copied to `auto.sh.before-settings-pak` before the first
  change. The block runs in the background and logs to `/tmp/minui-paks-startup.log`.
- Brightness: MinUI's shared settings (`/tmp/SharedSettings`) and `msettings.bin`.
- Flags: `enable-rtc`, `enable-560p` in `.userdata/miyoomini/`.

## Develop

```sh
cargo run --release           # preview; everything is written under ./sandbox
cargo test
cargo run --example snapshot  # snapshots/settings.ppm
./package.sh                  # -> dist/Settings.pak
```
