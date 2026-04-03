#!/usr/bin/env bash
# scripts/install_fpcalc.sh
# Install the Chromaprint fpcalc CLI tool needed for audio fingerprinting.
# Run once after cloning:   bash scripts/install_fpcalc.sh

set -euo pipefail

OS="$(uname -s)"

check_already_installed() {
    if command -v fpcalc &>/dev/null; then
        echo "✓ fpcalc already installed: $(fpcalc -version 2>&1 | head -1)"
        exit 0
    fi
}

check_already_installed

case "$OS" in
    Darwin)
        if command -v brew &>/dev/null; then
            echo "→ Installing chromaprint via Homebrew…"
            brew install chromaprint
        else
            echo "Homebrew not found. Install from https://brew.sh then re-run."
            exit 1
        fi
        ;;
    Linux)
        if command -v apt-get &>/dev/null; then
            echo "→ Installing libchromaprint-tools via apt…"
            sudo apt-get update -qq
            sudo apt-get install -y libchromaprint-tools
        elif command -v dnf &>/dev/null; then
            echo "→ Installing chromaprint-tools via dnf…"
            sudo dnf install -y chromaprint-tools
        elif command -v pacman &>/dev/null; then
            echo "→ Installing chromaprint via pacman…"
            sudo pacman -S --noconfirm chromaprint
        else
            echo "Unsupported Linux distribution. Please install fpcalc manually:"
            echo "  https://acoustid.org/chromaprint"
            exit 1
        fi
        ;;
    *)
        echo "Unsupported OS: $OS"
        echo "Download fpcalc from https://acoustid.org/chromaprint and add it to your PATH."
        exit 1
        ;;
esac

check_already_installed
echo "Done — fpcalc is ready."
