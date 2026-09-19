# Clipit

Clipboard history for the COSMIC panel. It watches what you copy, keeps a searchable list, and puts any entry back on the clipboard when you pick it.

This is a COSMIC applet, not a standalone window. It only makes sense on COSMIC (Wayland). Clicking an entry copies it. You paste with Ctrl+V like anything else.

Licensed under [MIT](LICENSE).

## Requirements

- COSMIC desktop on Wayland
- [Rust](https://rustup.rs/) 1.85 or newer (this crate uses edition 2024)
- `git` (libcosmic and cosmic-settings-config are pulled from GitHub)
- A C toolchain and the system libraries below

`just` is optional. The [justfile](justfile) is the same recipe set other COSMIC projects use.

## Dependencies

Rust crates come in through Cargo. You still need headers on the machine, because libcosmic and the Wayland stack link against them.

| What | Why |
| --- | --- |
| `pkg-config` / `pkgconf` | Find the libraries below at build time |
| `cmake` | Some native crates invoke it |
| `libxkbcommon` | Keyboard handling (the build dies without this) |
| `fontconfig`, `freetype`, `expat` | Text rendering in iced/libcosmic |
| `libwayland` | Wayland client protocol |

Debian, Ubuntu, Pop!_OS:

```sh
sudo apt install git pkgconf cmake \
  libexpat1-dev libfontconfig-dev libfreetype-dev \
  libxkbcommon-dev libwayland-dev
```

Fedora:

```sh
sudo dnf install git pkgconf cmake \
  expat-devel fontconfig-devel freetype-devel \
  libxkbcommon-devel wayland-devel
```

Arch:

```sh
sudo pacman -S --needed git pkgconf cmake \
  expat fontconfig freetype2 libxkbcommon wayland
```

Rust itself:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Optional, for `just build-release` / `just install-user`:

```sh
cargo install just
```

Runtime: COSMIC already has the compositor and session bus. No extra packages after the binary is installed, as long as `clipit` is on `PATH`.

## Install

Fastest path, user-local, no root for the applet itself (sudo only if the packages above are missing):

```sh
./setup.sh
```

That installs build packages if needed, builds `--release`, and copies:

- `~/.local/bin/clipit`
- `~/.local/share/applications/dev.clipit.Clipit.desktop`

`~/.local/bin` must be on `PATH`. Log out and back in once if `which clipit` fails.

Manual:

```sh
cargo build --release
just install-user
```

System-wide (needs root):

```sh
cargo build --release
sudo just install
```

That puts the binary in `/usr/bin` and the desktop file in `/usr/share/applications`.

Uninstall:

```sh
just uninstall-user    # ~/.local
sudo just uninstall    # /usr
```

## Add it to the panel

COSMIC Settings → Panel → Applets → **+** → Clipit.

If the panel already had Clipit configured, kill and restart it so it picks up a new binary:

```sh
pkill cosmic-panel
```

COSMIC restarts the panel on its own.

## Keyboard shortcut

On first launch the applet writes **Super+V → `clipit --toggle`** into COSMIC's custom shortcuts (`com.system76.CosmicSettings.Shortcuts`). Change or delete it in Settings → Keyboard like any other custom binding.

`clipit --toggle` talks to the running applet over D-Bus (`dev.clipit.Clipit`, method `Toggle`). Bind that command from any launcher if Super+V is already taken.

## Usage

Click the paste icon in the panel, or Super+V.

- Type to search
- ↑ / ↓ then Enter copies the highlighted row
- Click a row to copy and close
- ★ pins (survives Clear and size trim)
- Expand shows full text or a larger image
- Delete, then undo in the footer if you miss
- Gear opens settings

Copy, then paste in the other app with Ctrl+V. Clipit does not inject keystrokes into the focused window.

## Features

Capture:

- Text from any Wayland app that talks the regular clipboard
- Rich text: when an app offers `text/html`, Clipit stores it alongside the
  plain text and re-offers both on copy, so formatting survives a trip
  through history into HTML-aware apps. The expanded view can show the
  HTML source
- Images: PNG, JPEG, GIF, WebP, and BMP capture (probed in that order).
  Stored bytes are re-sniffed by magic before saving, so mislabeled offers
  do not poison the history. Re-copy serves the stored format's own MIME
  type. Animated GIFs capture whole and render their first frame in the
  popup
- Optional primary-selection capture: middle-click copies land in the same
  history (off by default, toggle in settings). Copies made from Clipit go
  to both the regular clipboard and the primary selection
- Re-copying the same content moves that row to the top and keeps the pin
- Ignore patterns: if the text contains a pattern, it is never stored
- Hard skips: text over 100 KB, images over 4 MB, non-UTF-8, undecodable
  image data
- Optional expiry of unpinned rows (1 / 7 / 30 days)

Popup and data:

- Search, pin, delete, undo, expand
- History size 50 / 100 / 250 / 500 / 1000
- Poll interval 300 / 800 / 1500 / 3000 ms
- Image capture on/off, primary-selection capture on/off
- Export a timestamped folder under `~/Documents`

Settings live in cosmic-config under `dev.clipit.Clipit` (version 2).

## Data and privacy

Clipboard history is a pile of secrets by nature. Passwords, tokens, and one-time codes land here unless you stop them.

Stored under `$XDG_DATA_HOME/clipit` (usually `~/.local/share/clipit`):

| Path | Contents |
| --- | --- |
| `history.json` | Text entries, HTML alternatives, image filenames, pins, timestamps |
| `images/*` | Captured images (`.png`, `.jpg`, `.webp`, `.bmp`) |

Files are written `0600`, directories `0700`. On startup Clipit tightens older files that were created with looser permissions. This is not encryption. Anyone who can run as your user can read the files. Other accounts on the machine cannot, if the permissions hold.

Ignore patterns are case-insensitive substrings, not regex, not a password vault. Add words you never want recorded (`password`, `token`, `secret`). Anything you already copied before adding a pattern stays in the file until you delete it.

Export writes the same data to `~/Documents/clipit-export-<unix-time>/` with the same owner-only permissions. Treat that folder like the live history.

## How it works

One libcosmic applet process. A tokio task polls the Wayland clipboard through `wl-clipboard-rs` (wlr-data-control) and pushes new text, HTML, or image bytes into history. Reads stop one byte past the size cap, so a huge clipboard offer cannot fill RAM. HTML is fetched only for brand-new content, so steady-state polling stays a single read per tick.

Clipit's own copies are hashed and ignored so they do not bounce back into the list. The value on the clipboard when the poller starts is a baseline and is not recorded. Changing poll rate or image capture restarts the poller.

Text entries are keyed by the plain-text hash; the HTML variant hangs off the same entry. Image entries key on the byte hash, which is also the payload filename. The stored format comes from a magic-byte sniff, not from the sending app's claim. History is newest-first, pins sorted to the top. Saves go through a temp file then rename, so a crash mid-write does not wipe the list.

## Development

```sh
just run          # build --release and run
just test         # cargo test
just check        # clippy -W clippy::pedantic
just build-debug
just build-release
```

Without just: `cargo test`, `cargo build --release`, `cargo run --release`.

Tests cover history logic: dedupe, pins, trim, prune, ignore, undo, owner-only file modes, and image path traversal.

First build clones `pop-os/libcosmic` and `pop-os/cosmic-settings-daemon` and compiles a large iced/wgpu tree. Expect minutes, not seconds.

## Translating

UI strings live in [i18n/en/clipit.ftl](i18n/en/clipit.ftl) using
[Fluent](https://projectfluent.org/) syntax. Clipit picks the language from
the desktop locale and falls back to English.

Shipped locales: en, ar, cs, de, el, es, fr, he, hi, hu, id, it, ja, ko,
nl, pl, pt-BR, ro, ru, sv, th, tr, uk, vi, zh-CN, zh-TW. Each covers every
message, including CLDR plural rules (one/few/many and friends), and a
test enforces that no locale drifts out of sync with the English set.

To add or fix a language, copy `i18n/en/clipit.ftl` into a new locale
folder, keep the message IDs, translate the values. A build test fails on
missing or extra message IDs, so nothing silently falls back. Note: the
shipped translations were drafted with AI assistance; corrections from
native speakers are welcome as ordinary pull requests.

## Limits

- No paste injection. Copy, then Ctrl+V. Wayland does not let one app type
  into another; faking that with `wtype`-style virtual keyboards breaks on
  focus changes and risks pasting into the wrong window. A clipboard
  manager that guesses where your keystrokes land is a liability, so
  Clipit does not try.
- Rich text is stored and re-served as-is; the popup preview shows plain
  text (or the HTML source) only. Rendering HTML in the list would mean
  shipping a browser.
- Animated GIFs render their first frame in the popup; full animation
  playback in the list is not implemented.
- Shipped translations are AI-drafted; they cover every message but have
  not all been reviewed by native speakers.

## License

[MIT](LICENSE).
