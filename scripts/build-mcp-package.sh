#!/bin/bash
set -e

# Build script for @memoryx/codegraph-mcp npm package
# Run from project root: ./scripts/build-mcp-package.sh
#
# Prerequisites: Run ./scripts/prepare-binaries.sh first to build/collect binaries

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
MCP_PACKAGE="$PROJECT_ROOT/mcp-package"
PREBUILT_DIR="$PROJECT_ROOT/prebuilt"

echo "Building @memoryx/codegraph-mcp package..."

# Run prepare-binaries first to ensure we have latest builds
echo ""
echo "Step 1: Preparing binaries..."
"$SCRIPT_DIR/prepare-binaries.sh"

# Ensure mcp-package directories exist
mkdir -p "$MCP_PACKAGE/bin"
mkdir -p "$MCP_PACKAGE/models"

# Expected binary names
BINARIES=(
    "codegraph-lsp-darwin-arm64"
    "codegraph-lsp-darwin-x64"
    "codegraph-lsp-linux-x64"
    "codegraph-lsp-win32-x64.exe"
)

# Copy binaries from prebuilt to mcp-package
echo ""
echo "Step 2: Copying binaries to mcp-package/bin/..."
FOUND_COUNT=0
MISSING=()

for BINARY in "${BINARIES[@]}"; do
    if [ -f "$PREBUILT_DIR/$BINARY" ]; then
        cp "$PREBUILT_DIR/$BINARY" "$MCP_PACKAGE/bin/"
        chmod +x "$MCP_PACKAGE/bin/$BINARY" 2>/dev/null || true
        echo "  ✓ $BINARY"
        ((FOUND_COUNT++))
    else
        MISSING+=("$BINARY")
        echo "  ✗ $BINARY (not found)"
    fi
done

# Copy models
echo ""
echo "Step 3: Copying models..."
if [ -d "$PROJECT_ROOT/models/model2vec" ]; then
    rm -rf "$MCP_PACKAGE/models/model2vec"
    cp -r "$PROJECT_ROOT/models/model2vec" "$MCP_PACKAGE/models/"
    echo "  ✓ model2vec"
else
    echo "  ✗ models/model2vec not found"
fi

# Make scripts executable
chmod +x "$MCP_PACKAGE/bin/codegraph-mcp.js"
chmod +x "$MCP_PACKAGE/bin/postinstall.js"

# Summary
echo ""
echo "========================================"
echo "Package contents:"
ls -lh "$MCP_PACKAGE/bin/" | grep -E "codegraph-lsp|\.js$"

echo ""
echo "Found: $FOUND_COUNT/${#BINARIES[@]} binaries"

if [ ${#MISSING[@]} -gt 0 ]; then
    echo ""
    echo "Missing binaries (build on respective platforms):"
    for M in "${MISSING[@]}"; do
        echo "  - $M"
    done
    echo ""
    echo "Copy built binaries to: $PREBUILT_DIR/"
fi

echo ""
echo "Package size:"
du -sh "$MCP_PACKAGE"
du -sh "$MCP_PACKAGE/bin"
du -sh "$MCP_PACKAGE/models" 2>/dev/null || true

echo ""
echo "========================================"
if [ $FOUND_COUNT -eq ${#BINARIES[@]} ]; then
    echo "✓ All binaries present. Ready to publish!"
    echo ""
    echo "  cd mcp-package"
    echo "  npm publish --access public"
else
    echo "⚠ Missing binaries. Build on other platforms first."
fi
echo ""
echo "To test locally:"
echo "  cd mcp-package && npm link"
echo "  codegraph-mcp --help"
