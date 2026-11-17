# Web Browser

A tiny text-mode web browser and downloader for the Miyoo Mini+ (MinUI), written in Rust.
It shows a page's text and links (no images, CSS or JavaScript), and saves files from
links to the SD card. HTTPS works (rustls + bundled Mozilla root certificates).

Needs Wi-Fi to be on (see `../wifi`).

## Controls

| Mini+ | Desktop preview | Page | Home | Keyboard |
|---|---|---|---|---|
| Up / Down | arrows | previous / next link (scrolls when none is on screen) | move | move |
| Left / Right, L1 / R1 | arrows, Q / W | page up / down | page up / down | move; L1 clear, R1 `.com` |
| A | Z or Space | open link | open | type key |
| Y | V | **save link to SD card** | | space |
| B | X | back (or cancel a download) | back to page, or quit | delete |
| X | C | edit address | type address | abc / ABC / symbols |
| START | Enter | home | | go |
| SELECT | Tab | normal / small text | normal / small text | cancel |
| MENU | Esc | quit | quit | quit |

Typing something without a dot, or with spaces, searches (Brave Search by default).
Opening a link that isn't a web page (zip, pdf, ...) downloads it automatically; Y
downloads any link, even web pages. Downloads go to `Downloads/` at the root of the SD
card, are written as `name.part` until complete, and never overwrite an existing file.

## Develop on your computer

```sh
cargo run --release                                 # 640x480 preview window
cargo test                                          # parser, layout, navigation, URL/filename helpers
cargo run --example snapshot                        # snapshots/*.ppm of each screen (no network)
cargo run --example fetch -- https://text.npr.org   # fetch + print a page as laid out on screen
cargo run --example fetch -- --save <url>           # download into ./downloads
```

Locally, bookmarks are read from `pak/bookmarks.txt` and downloads go to `./downloads`
(`BROWSER_BOOKMARKS` / `BROWSER_DOWNLOAD_DIR` override; `BROWSER_SEARCH_URL` changes the
search engine, the query is appended to it).

## Build for the Miyoo Mini+

From the repo's Nix dev shell (direnv):

```sh
./package.sh     # -> dist/Web Browser.pak   (or ../package.sh for every pak)
```

Copy `dist/Web Browser.pak` into `Tools/miyoomini/` on the SD card. Edit `bookmarks.txt`
inside it on your PC (`Name | URL` per line). After a run, check
`.userdata/.../web-browser/log.txt` for errors.

## Status

Verified here: parser, layout, navigation, rendering (unit tests and snapshots), real
HTTPS fetches and downloads on desktop, and the ARM build fetching over HTTPS under
`qemu-arm`.

Works on a Miyoo Mini+. The screen is drawn rotated 180 degrees because the panel is
mounted upside down (`FB_ROTATE=0` in `launch.sh` turns that off).

If the device clock is wrong (e.g. reset to 1970), certificates are checked against the
build date instead, so HTTPS keeps working.
