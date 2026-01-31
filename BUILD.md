# Build Instructions

This document covers building CodeGraph for distribution as a VS Code extension (VSIX) and standalone npm package (MCP server).

## Prerequisites

- **Node.js** >= 18.0.0
- **Rust** >= 1.75.0 (with cargo)
- **npm** or **pnpm**

### Platform-specific toolchains

For cross-compilation on Mac ARM to x64:
```bash
rustup target add x86_64-apple-darwin
```

## Project Structure

```
codegraph-vscode/
├── src/                    # VS Code extension (TypeScript)
├── server/                 # LSP server (Rust)
├── codegraph-memory/       # Memory system crate (Rust)
├── models/model2vec/       # Embedding model (bundled)
├── prebuilt/               # Cross-platform binaries (git-ignored)
├── bin/                    # Binaries for VSIX (git-ignored)
├── mcp-package/            # npm package structure
└── scripts/                # Build scripts
```

## Binary Management

### Source of Truth: `prebuilt/`

All cross-platform binaries are stored in `prebuilt/`. This folder is git-ignored and must be populated manually for non-native platforms.

```
prebuilt/
├── codegraph-lsp-darwin-arm64    # Mac ARM (M1/M2/M3)
├── codegraph-lsp-darwin-x64      # Mac Intel
├── codegraph-lsp-linux-x64       # Linux x64
└── codegraph-lsp-win32-x64.exe   # Windows x64
```

### Building on Each Platform

**Mac ARM (primary dev machine):**
```bash
# Builds darwin-arm64 natively + cross-compiles darwin-x64
./scripts/prepare-binaries.sh
```

**Linux:**
```bash
cargo build --release -p codegraph-lsp
# Copy to Mac: target/release/codegraph-lsp → prebuilt/codegraph-lsp-linux-x64
```

**Windows:**
```powershell
cargo build --release -p codegraph-lsp
# Copy to Mac: target\release\codegraph-lsp.exe → prebuilt/codegraph-lsp-win32-x64.exe
```

## Building the VS Code Extension (VSIX)

### Quick Build

```bash
npm run package
```

This runs:
1. `esbuild` - Bundles TypeScript extension
2. `prepare-binaries.sh` - Builds/copies binaries to `bin/`
3. `vsce package` - Creates VSIX

### Output

```
codegraph-0.5.0.vsix (~62 MB)
├── bin/                    # Platform binaries
├── models/model2vec/       # Embedding model
├── out/extension.js        # Bundled extension
└── ...
```

### Install Locally

```bash
code --install-extension codegraph-0.5.0.vsix
```

## Building the npm Package (MCP Server)

### Quick Build

```bash
./scripts/build-mcp-package.sh
```

This:
1. Runs `prepare-binaries.sh` (builds + cross-compiles)
2. Copies binaries from `prebuilt/` to `mcp-package/bin/`
3. Copies models to `mcp-package/models/`

### Output

```
mcp-package/ (~133 MB)
├── package.json            # @memoryx/codegraph-mcp
├── bin/
│   ├── codegraph-mcp.js    # Entry point
│   ├── postinstall.js      # Setup script
│   └── codegraph-lsp-*     # Platform binaries
├── models/model2vec/       # Embedding model
└── README.md
```

### Test Locally

```bash
cd mcp-package
npm link
codegraph-mcp --help
```

### Publish to npm

```bash
cd mcp-package
npm publish --access public
```

## Development Workflow

### Running the Extension in Development

```bash
# Build TypeScript (watch mode)
npm run esbuild-watch

# Build Rust server
cargo build --release -p codegraph-lsp

# Press F5 in VS Code to launch Extension Development Host
```

### Running MCP Server Directly

```bash
# From project root
./target/release/codegraph-lsp --mcp --workspace /path/to/project
```

### Running Tests

```bash
# TypeScript tests
npm test

# Rust tests
cargo test --workspace

# CI checks
./scripts/ci-checks.sh
```

## MCP Client Configuration

### Claude Desktop / Claude Code CLI

Add to `~/.claude.json`:

```json
{
  "mcpServers": {
    "codegraph": {
      "command": "codegraph-mcp"
    }
  }
}
```

### Cursor

Add to settings:

```json
{
  "mcp.servers": {
    "codegraph": {
      "command": "codegraph-mcp"
    }
  }
}
```

## Troubleshooting

### RocksDB Build Issues

If you encounter RocksDB compilation errors:

```bash
cargo clean -p librocksdb-sys
cargo build --release -p codegraph-lsp
```

### Missing Cross-Compilation Target

```bash
rustup target add x86_64-apple-darwin  # Mac x64
rustup target add x86_64-unknown-linux-gnu  # Linux (requires linker)
```

### Model Not Found

The embedding model must be at one of:
1. `CODEGRAPH_MODELS_PATH` environment variable
2. `<extension>/models/model2vec/`
3. `~/.codegraph/models/model2vec/`

Download model:
```bash
./scripts/download-model.sh
```

## Release Checklist

1. Update version in `package.json`
2. Update version in `mcp-package/package.json`
3. Update `CHANGELOG.md`
4. Build binaries on all platforms (or use existing from `prebuilt/`)
5. Run tests: `npm test && cargo test --workspace`
6. Build VSIX: `npm run package`
7. Build npm package: `./scripts/build-mcp-package.sh`
8. Test both packages locally
9. Publish:
   - VSIX: Upload to VS Code Marketplace
   - npm: `cd mcp-package && npm publish --access public`
10. Create git tag: `git tag v0.5.0 && git push --tags`
