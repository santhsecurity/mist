#!/usr/bin/env bash
set -euo pipefail

REPO_DIR="$(cd "$(dirname "$0")" && pwd)"
BIN_DIR="$HOME/.local/bin"
SERVICE_DIR="$HOME/.config/systemd/user"
DATA_DIR="$HOME/.local/share/mist"
CONFIG_DIR="$HOME/.config/mist"

OS="$(uname -s)"
ARCH="$(uname -m)"
SESSION_TYPE="${XDG_SESSION_TYPE:-unknown}"

INTERACTIVE=false
[ -t 0 ] && INTERACTIVE=true

echo "━━━ Mist Installer ━━━"
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

# Auto-detect acceleration when the user did not already pass --features.
FEATURES=""
if [[ "$*" != *"--features"* ]]; then
    if [ "$OS" = "Darwin" ] && [ "$ARCH" = "arm64" ]; then
        FEATURES="coreml"
        echo "Detected Apple Silicon — enabling CoreML acceleration."
    elif [ "$OS" = "Linux" ] && pkg-config --exists openblas 2>/dev/null; then
        FEATURES="openblas"
        echo "Detected OpenBLAS — enabling CPU BLAS acceleration."
    fi
fi

# Build release binary
# Extra args (e.g. --features cuda) are passed through to cargo build.
echo "Building release binary (linker=$LINKER)..."
cd "$REPO_DIR"
if [ -n "$FEATURES" ]; then
    RUSTFLAGS="-C linker=$LINKER" cargo build --release --features "$FEATURES" "$@"
else
    RUSTFLAGS="-C linker=$LINKER" cargo build --release "$@"
fi

# Install binary
mkdir -p "$BIN_DIR"
cp "$REPO_DIR/target/release/mist" "$BIN_DIR/mist"
chmod +x "$BIN_DIR/mist"
echo "✓ Installed mist to $BIN_DIR/mist"

# Create directories
mkdir -p "$DATA_DIR" "$CONFIG_DIR"

# Typing-tool sanity check on Linux
if [ "$OS" = "Linux" ]; then
    if [ "$SESSION_TYPE" = "wayland" ]; then
        if ! command -v wtype &>/dev/null && ! command -v ydotool &>/dev/null; then
            echo ""
            echo "⚠ Warning: no typing tool found for Wayland."
            echo "  Install wtype or ydotool so Mist can type text at your cursor."
            echo "  Example: sudo apt install wtype"
        fi
    else
        if ! command -v xdotool &>/dev/null && ! command -v ydotool &>/dev/null && ! command -v wtype &>/dev/null; then
            echo ""
            echo "⚠ Warning: no typing tool found for X11."
            echo "  Install xdotool or ydotool so Mist can type text at your cursor."
            echo "  Example: sudo apt install xdotool"
        fi
    fi
fi

# Optionally pre-download the default model to remove first-run latency.
if $INTERACTIVE; then
    echo ""
    read -rp "Download the default Whisper model now (~466 MB)? [Y/n] " answer
    if [[ "$answer" =~ ^[Yy]?$ ]]; then
        mkdir -p "$DATA_DIR/models"
        MODEL_URL="https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en.bin"
        MODEL_FILE="$DATA_DIR/models/ggml-small.en.bin"
        if command -v curl &>/dev/null; then
            curl -L --progress-bar "$MODEL_URL" -o "$MODEL_FILE"
        elif command -v wget &>/dev/null; then
            wget --progress=bar:force "$MODEL_URL" -O "$MODEL_FILE"
        else
            echo "⚠ curl or wget not found; skipping model download. Mist will download it on first use."
        fi
    fi
fi

# Install systemd service
if command -v systemctl &>/dev/null; then
    mkdir -p "$SERVICE_DIR"
    cp "$REPO_DIR/systemd/mist.service" "$SERVICE_DIR/mist.service"
    sed -i "s|%h|$HOME|g" "$SERVICE_DIR/mist.service"
    systemctl --user daemon-reload
    echo "✓ Installed systemd user service"
fi

# Install desktop entry
DESKTOP_DIR="$HOME/.local/share/applications"
mkdir -p "$DESKTOP_DIR"
cp "$REPO_DIR/assets/mist.desktop" "$DESKTOP_DIR/mist.desktop"
echo "✓ Installed desktop entry"

# Run interactive setup
if $INTERACTIVE; then
    echo ""
    read -rp "Run interactive setup now? [Y/n] " answer
    if [[ "$answer" =~ ^[Yy]?$ ]]; then
        "$BIN_DIR/mist" setup
    fi
fi

echo ""
echo "✓ Installation complete"
echo ""
echo "Usage:"
echo "  mist                    # Run daemon"
echo "  mist run                # Explicitly run daemon"
echo "  mist setup              # Interactive configuration"
echo "  mist status             # Show status"
echo "  mist dictionary --help  # Manage dictionary"
echo ""
if command -v systemctl &>/dev/null; then
    echo "Enable auto-start:"
    echo "  systemctl --user enable mist"
    echo "  systemctl --user start mist"
fi
