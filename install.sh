#!/usr/bin/env bash
set -euo pipefail

REPO_DIR="$(cd "$(dirname "$0")" && pwd)"
BIN_DIR="$HOME/.local/bin"
SERVICE_DIR="$HOME/.config/systemd/user"
DATA_DIR="$HOME/.local/share/flow"
CONFIG_DIR="$HOME/.config/flow"

echo "━━━ Flow Installer ━━━"
echo ""

# Detect a suitable linker — prefer cc, fall back to gcc, then clang.
LINKER=""
for candidate in cc gcc clang; do
    if command -v "$candidate" &>/dev/null; then
        LINKER="$candidate"
        break
    fi
done

if [ -z "$LINKER" ]; then
    echo "Error: No C linker found (tried cc, gcc, clang). Install one and retry."
    exit 1
fi

# Build release binary
# Extra args (e.g. --features cuda) are passed through to cargo build.
echo "Building release binary (linker=$LINKER)..."
cd "$REPO_DIR"
RUSTFLAGS="-C linker=$LINKER" cargo build --release "$@"

# Install binary
mkdir -p "$BIN_DIR"
cp "$REPO_DIR/target/release/flow" "$BIN_DIR/flow"
chmod +x "$BIN_DIR/flow"
echo "✓ Installed flow to $BIN_DIR/flow"

# Create directories
mkdir -p "$DATA_DIR" "$CONFIG_DIR"

# Install systemd service
if command -v systemctl &>/dev/null; then
    mkdir -p "$SERVICE_DIR"
    cp "$REPO_DIR/systemd/flow.service" "$SERVICE_DIR/flow.service"
    sed -i "s|%h|$HOME|g" "$SERVICE_DIR/flow.service"
    systemctl --user daemon-reload
    echo "✓ Installed systemd user service"
    echo ""
    echo "Enable auto-start with:"
    echo "  systemctl --user enable flow"
    echo "Start now with:"
    echo "  systemctl --user start flow"
fi

# Install desktop entry
DESKTOP_DIR="$HOME/.local/share/applications"
mkdir -p "$DESKTOP_DIR"
cp "$REPO_DIR/assets/flow.desktop" "$DESKTOP_DIR/flow.desktop"
echo "✓ Installed desktop entry"

echo ""
echo "✓ Installation complete"
echo ""
echo "Usage:"
echo "  flow                    # Run daemon"
echo "  flow setup              # Interactive configuration"
echo "  flow run                # Explicitly run daemon"
