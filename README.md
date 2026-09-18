# Clipit

Clipboard history applet for the COSMIC desktop. Lives in the panel, records
everything you copy, and lets you search, pin, preview, and re-paste it.

## Features

### Capture
- Automatic clipboard history (text, any Wayland app)
- PNG image capture with on-disk storage and thumbnails
- Duplicate detection: re-copying moves an entry to the top instead of
  duplicating it (pin state preserved)
- Ignore patterns: text containing a pattern is never recorded
- Skip rules: >100 KB text, >4 MB images, non-UTF-8 text
- History persists across sessions (`$XDG_DATA_HOME/clipit/history.json`)
- Optional expiry of old unpinned entries (1/7/30 days)

### Popup
- Search box (auto-focused on open)
- Keyboard navigation: ↑/↓ move, Enter re-copies, Escape closes
- One-click re-copy of any entry (text or image), popup closes for pasting
- Expand any entry to view full text or a larger image preview
- Pin entries (★) so they survive Clear and trimming
- Delete single entries; undo (↶) restores the last deleted entries
- Relative timestamps ("now", "5m", "2h", "3d") and pinned/item stats

### Global keyboard shortcut
- On startup the applet registers **Super+V → open clipboard popup** in the
  COSMIC shortcuts config (`com.system76.CosmicSettings.Shortcuts`)
- The binding shows up in COSMIC Settings → Keyboard, where it can be changed
  or removed like any custom shortcut
- Under the hood: `clipit --toggle` calls the applet's D-Bus method
  (`dev.clipit.Clipit` / `Toggle`), so any launcher or keybinding tool can
  trigger it

### Settings (gear in the popup footer)
- Max history size (50 / 100 / 250 / 500 / 1000)
- Poll rate (300 / 800 / 1500 / 3000 ms)
- Expiry (never / 1 / 7 / 30 days)
- Image capture on/off
- Ignore-pattern editor
- Export history to a folder (`~/Documents/clipit-export-<timestamp>/`)

Settings persist via cosmic-config (`dev.clipit.Clipit`, version 2).

## Build

Requires Rust and `pkg-config` + `libxkbcommon` dev files.

```sh
cargo build --release
```

## Install

User install (no root):

```sh
just install-user
```

System install:

```sh
sudo just install
```

Then add **Clipit** to the panel: COSMIC Settings → Panel → Applets → **+**
(or reload the panel if it was pre-configured).

## How it works

The applet is a single libcosmic panel applet process. A background tokio task
polls the Wayland clipboard (via `wl-clipboard-rs`, data-control protocol)
and feeds new text/images into the history. Copies made by Clipit itself are
suppressed from re-capture via a content hash. History is stored newest-first,
pinned entries sorted to the top. Entries are identified by a content hash,
which also names stored image files.

The poller restarts automatically when poll rate or image capture settings
change; the clipboard value present at (re)start is treated as a baseline and
not recorded.

## Tests

```sh
cargo test    # history logic: dedupe, pins, trim, prune, ignore, undo
```

## Notes / roadmap

- Clicking an entry copies it; paste with Ctrl+V. Direct paste injection is
  not implemented (compositor-dependent).
- Primary-selection (middle-click) capture is not implemented.
- English strings are hardcoded; no i18n yet.
- Text/HTML rich content is not preserved; plain text only.
