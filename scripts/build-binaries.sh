#!/bin/bash
# Build LSP server binaries for macOS (both architectures)
# Linux and Windows binaries are built via GitHub Actions

set -e

echo "Building CodeGraph LSP server binaries for macOS..."

# Ensure we're on macOS
if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "This script is for macOS only. Linux/Windows binaries are built via GitHub Actions."
    exit 1
fi

# Install Rust targets if needed
echo "Ensuring Rust targets are installed..."
rustup target add x86_64-apple-darwin aarch64-apple-darwin 2>/dev/null || true

# Create bin directory
mkdir -p bin

echo ""
echo "Building for aarch64-apple-darwin (Apple Silicon)..."
cargo build --release --target aarch64-apple-darwin
cp target/aarch64-apple-darwin/release/codegraph-lsp bin/codegraph-lsp-darwin-arm64
chmod +x bin/codegraph-lsp-darwin-arm64
echo "✓ Built bin/codegraph-lsp-darwin-arm64"

echo ""
echo "Building for x86_64-apple-darwin (Intel)..."
cargo build --release --target x86_64-apple-darwin
cp target/x86_64-apple-darwin/release/codegraph-lsp bin/codegraph-lsp-darwin-x64
chmod +x bin/codegraph-lsp-darwin-x64
echo "✓ Built bin/codegraph-lsp-darwin-x64"

echo ""
echo "============================================"
echo "Build complete! macOS binaries are in bin/"
echo "============================================"
ls -la bin/codegraph-lsp-darwin-*

echo ""
echo "Note: Linux and Windows binaries are built via GitHub Actions."
echo "Download them from the CI artifacts after a successful build."
