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
if ! command -v git >/dev/null 2>&1; then
    echo "git is required (Cargo pulls libcosmic from GitHub)."
    exit 1
fi

# System build dependencies: pkg-config, cmake, and headers for
# xkbcommon / wayland / fontconfig / freetype / expat.
if ! pkg-config --exists xkbcommon 2>/dev/null \
    || ! pkg-config --exists fontconfig 2>/dev/null \
    || ! pkg-config --exists freetype2 2>/dev/null; then
    echo "==> Installing build dependencies (sudo)"
    if command -v apt-get >/dev/null 2>&1; then
        sudo apt-get update -y
        sudo apt-get install -y pkg-config cmake \
            libexpat1-dev libfontconfig-dev libfreetype-dev \
            libxkbcommon-dev libwayland-dev
    elif command -v dnf >/dev/null 2>&1; then
        sudo dnf install -y pkgconf cmake \
            expat-devel fontconfig-devel freetype-devel \
            libxkbcommon-devel wayland-devel
    elif command -v pacman >/dev/null 2>&1; then
        sudo pacman -S --needed pkgconf cmake \
            expat fontconfig freetype2 libxkbcommon wayland
    else
        echo "Please install: pkg-config, cmake, and dev packages for"
        echo "libxkbcommon, wayland, fontconfig, freetype, expat"
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
