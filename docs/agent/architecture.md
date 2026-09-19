# Architecture and config schema

## One binary, two modes

- `src/main.rs` — entrypoint. `clipit --toggle` is a thin D-Bus client that
  pokes the running applet; without the flag it runs as a libcosmic panel
  applet.

## Module map

- `src/clipboard.rs` — Wayland clipboard polling (`wl-clipboard-rs`),
  image format sniffing, clipboard writes (regular + primary selection).
- `src/history.rs` — storage model, content-hash IDs and dedupe,
  private-permission file helpers. Deliberately free of i18n and cosmic
  dependencies.
- `src/app.rs` — all UI, cosmic-config settings, the D-Bus `Toggle`
  service, Super+V shortcut registration.
- `src/i18n.rs` — fluent loader plus the `fl!` macro.

## Config and schema

- `Config` in `src/app.rs` carries `#[version = 3]` (cosmic_config). Bump
  the version whenever its fields change; older configs on disk merge
  missing fields from `Default`.
- `history::Entry` must stay backward compatible: new fields need
  `#[serde(default)]`, and existing `history.json` files must keep loading.
