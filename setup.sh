#!/usr/bin/env bash
# Clipit setup: install build dependencies, build, and install the applet
# for the current user (~/.local). Run again any time to rebuild.
set -euo pipefail

cd "$(dirname "$0")"

echo "==> Checking build tools"
if ! command -v cargo >/dev/null 2>&1; then
    echo "Rust is required. Install it first:"
    echo "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
fi

# System build dependencies: pkg-config + libxkbcommon dev files.
if ! pkg-config --exists xkbcommon 2>/dev/null; then
    echo "==> Installing libxkbcommon development files (sudo)"
    if command -v apt-get >/dev/null 2>&1; then
        sudo apt-get update -y
        sudo apt-get install -y pkg-config libxkbcommon-dev
    elif command -v dnf >/dev/null 2>&1; then
        sudo dnf install -y pkgconf-pkg-config libxkbcommon-devel
    elif command -v pacman >/dev/null 2>&1; then
        sudo pacman -S --needed pkgconf libxkbcommon
    else
        echo "Please install: pkg-config + libxkbcommon development package"
        exit 1
    fi
fi

echo "==> Building (release)"
cargo build --release

echo "==> Installing to ~/.local"
install -d "$HOME/.local/bin" "$HOME/.local/share/applications"
install -m0755 target/release/clipit "$HOME/.local/bin/clipit"
install -m0644 resources/dev.clipit.Clipit.desktop \
    "$HOME/.local/share/applications/dev.clipit.Clipit.desktop"

cat <<'EOF'

Done. Finish setup:
  1. Make sure ~/.local/bin is on PATH (log out/in once if unsure).
  2. COSMIC Settings -> Panel -> Applets -> "+" -> add "Clipit".
  3. Super+V opens the popup (register automatically on applet start;
     editable in Settings -> Keyboard).

Useful commands:
  just run     # run the applet in dev mode
  just test    # unit tests
  just install-user
EOF
