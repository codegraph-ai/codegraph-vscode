#!/bin/bash
set -e

# Prepare binaries from prebuilt/ to bin/ for VSIX packaging
# Also cross-compiles for darwin-x64 if on darwin-arm64
# Run from project root: ./scripts/prepare-binaries.sh

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
PREBUILT_DIR="$PROJECT_ROOT/prebuilt"
BIN_DIR="$PROJECT_ROOT/bin"

echo "Preparing binaries for VSIX packaging..."

mkdir -p "$PREBUILT_DIR"
mkdir -p "$BIN_DIR"

# Expected binary names
BINARIES=(
    "codegraph-lsp-darwin-arm64"
    "codegraph-lsp-darwin-x64"
    "codegraph-lsp-linux-x64"
    "codegraph-lsp-win32-x64.exe"
)

# Detect current platform
PLATFORM=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$PLATFORM" in
    darwin) PLATFORM_STR="darwin" ;;
    linux) PLATFORM_STR="linux" ;;
    mingw*|msys*|cygwin*) PLATFORM_STR="win32" ;;
    *) PLATFORM_STR="" ;;
esac

case "$ARCH" in
    arm64|aarch64) ARCH_STR="arm64" ;;
    x86_64) ARCH_STR="x64" ;;
    *) ARCH_STR="" ;;
esac

# Build for current platform
echo ""
echo "Building for current platform ($PLATFORM_STR-$ARCH_STR)..."
cargo build --release -p codegraph-lsp

CURRENT_BINARY="codegraph-lsp-${PLATFORM_STR}-${ARCH_STR}"
if [ "$PLATFORM_STR" = "win32" ]; then
    CURRENT_BINARY="${CURRENT_BINARY}.exe"
    cp "$PROJECT_ROOT/target/release/codegraph-lsp.exe" "$PREBUILT_DIR/$CURRENT_BINARY"
else
    cp "$PROJECT_ROOT/target/release/codegraph-lsp" "$PREBUILT_DIR/$CURRENT_BINARY"
    chmod +x "$PREBUILT_DIR/$CURRENT_BINARY"
fi
echo "Saved: prebuilt/$CURRENT_BINARY"

# Cross-compile for Mac x64 if on Mac ARM
if [ "$PLATFORM_STR" = "darwin" ] && [ "$ARCH_STR" = "arm64" ]; then
    echo ""
    echo "Cross-compiling for darwin-x64..."
    if rustup target list --installed | grep -q "x86_64-apple-darwin"; then
        cargo build --release -p codegraph-lsp --target x86_64-apple-darwin
        cp "$PROJECT_ROOT/target/x86_64-apple-darwin/release/codegraph-lsp" "$PREBUILT_DIR/codegraph-lsp-darwin-x64"
        chmod +x "$PREBUILT_DIR/codegraph-lsp-darwin-x64"
        echo "Saved: prebuilt/codegraph-lsp-darwin-x64"
    else
        echo "  ⚠ x86_64-apple-darwin target not installed. Run:"
        echo "    rustup target add x86_64-apple-darwin"
    fi
fi

# Copy from prebuilt/ to bin/
echo ""
echo "Copying binaries to bin/..."
FOUND_COUNT=0
MISSING=()

for BINARY in "${BINARIES[@]}"; do
    if [ -f "$PREBUILT_DIR/$BINARY" ]; then
        cp "$PREBUILT_DIR/$BINARY" "$BIN_DIR/"
        chmod +x "$BIN_DIR/$BINARY" 2>/dev/null || true
        echo "  ✓ $BINARY"
        ((FOUND_COUNT++))
    else
        MISSING+=("$BINARY")
        echo "  ✗ $BINARY (not in prebuilt/)"
    fi
done

echo ""
echo "Found: $FOUND_COUNT/${#BINARIES[@]} binaries"

if [ ${#MISSING[@]} -gt 0 ]; then
    echo ""
    echo "Missing binaries - build on respective platforms and copy to prebuilt/:"
    for M in "${MISSING[@]}"; do
        echo "  - $M"
    done
fi

echo ""
echo "bin/ contents:"
ls -lh "$BIN_DIR/" | grep codegraph-lsp
