# Clipit

Clipboard history applet for the COSMIC desktop. Rust, libcosmic, Wayland-only.

## Commands

- `cargo test` — run from the repo root; tests read `i18n/` relative paths
- `cargo test <name>` — single test
- `cargo clippy -- -W clippy::pedantic` — the lint bar (same as `just check`)
- `cargo build --release` — the shipped artifact
- `./setup.sh` — installs system deps (sudo), builds, installs to `~/.local`

First build takes minutes: git dependencies pull the whole iced/wgpu tree.
Let it finish; it is not stuck.

The applet runs only under COSMIC/Wayland and has no headless mode. Verify
changes with tests plus a build, never by launching.

If a build panics in `smithay-client-toolkit/build.rs` with a pkg-config
error, the machine is missing system dev packages; per-distro package lists
live in README.md and `setup.sh`.

## Detailed guidelines

- [Architecture and config schema](docs/agent/architecture.md)
- [Hard conventions](docs/agent/conventions.md)
- [i18n rules](docs/agent/i18n.md)
- [Testing rules](docs/agent/testing.md)
