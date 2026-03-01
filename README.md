# CodeGraph

**Cross-language code intelligence for AI agents and developers.**

[![VS Code](https://img.shields.io/badge/VS%20Code-1.90+-blue.svg)](https://code.visualstudio.com/)
[![License](https://img.shields.io/badge/License-Apache%202.0-green.svg)](LICENSE)

CodeGraph builds a semantic graph of your codebase — functions, classes, imports, call chains — and exposes it through **27 MCP tools**, a **VS Code extension**, and a **persistent memory layer**. AI agents get structured code understanding instead of grepping through files.

## Two Ways to Use It

### MCP Server (for Claude, Cursor, any MCP client)

```bash
# Install globally
npm install -g @memoryx/codegraph-mcp
```

Add to your MCP client config:

<details>
<summary>Claude Code / Claude Desktop (~/.claude.json)</summary>

```json
{
  "mcpServers": {
    "codegraph": {
      "command": "codegraph-mcp",
      "args": ["--workspace", "/path/to/your/project"]
    }
  }
}
```
</details>

<details>
<summary>Cursor</summary>

```json
{
  "mcp.servers": {
    "codegraph": {
      "command": "codegraph-mcp",
      "args": ["--workspace", "/path/to/your/project"]
    }
  }
}
```
</details>

<details>
<summary>From source (development)</summary>

```bash
git clone https://github.com/codegraph-ai/codegraph-vscode
cd codegraph-vscode
cargo build --release -p codegraph-lsp

# Run as MCP server
./target/release/codegraph-lsp --mcp --workspace /path/to/your/project
```
</details>

### VS Code Extension

Install from `.vsix` or build from source:

```bash
git clone https://github.com/codegraph-ai/codegraph-vscode
cd codegraph-vscode
./scripts/build.sh --install
```

The extension registers all 27 tools as VS Code Language Model Tools, so Copilot and other VS Code AI features can use them directly.

---

## Tools

### Code Analysis

| Tool | What it does |
|------|-------------|
| `codegraph_get_dependency_graph` | File/module import relationships |
| `codegraph_get_call_graph` | Function call chains (callers and callees) |
| `codegraph_analyze_impact` | Blast radius of a change — what breaks if you modify, delete, or rename |
| `codegraph_get_ai_context` | Intent-aware code context (explain, modify, debug, test) |
| `codegraph_analyze_complexity` | Cyclomatic and cognitive complexity per function |
| `codegraph_find_unused_code` | Dead code detection (file, module, or workspace scope) |
| `codegraph_analyze_coupling` | Module coupling metrics and instability scores |

### Code Navigation

| Tool | What it does |
|------|-------------|
| `codegraph_symbol_search` | Find functions/classes/types by name or pattern |
| `codegraph_get_callers` | Who calls this function? (with transitive depth) |
| `codegraph_get_callees` | What does this function call? |
| `codegraph_get_detailed_symbol` | Full symbol info: source, callers, callees, complexity |
| `codegraph_get_symbol_info` | Quick metadata: signature, visibility, kind |
| `codegraph_find_by_imports` | Find files that import a module |
| `codegraph_find_by_signature` | Search by parameter count, return type, modifiers |
| `codegraph_find_entry_points` | Discover main functions, HTTP handlers, tests |
| `codegraph_find_related_tests` | Find tests that exercise a given function |
| `codegraph_traverse_graph` | Custom graph traversal with edge/node type filters |

### Memory (Persistent AI Context)

AI agents can store and recall knowledge across sessions — debugging insights, architectural decisions, known issues.

| Tool | What it does |
|------|-------------|
| `codegraph_memory_store` | Create a memory (5 kinds: debug, decision, issue, convention, context) |
| `codegraph_memory_search` | Hybrid search: BM25 + semantic embeddings + graph proximity |
| `codegraph_memory_get` | Retrieve a memory by ID |
| `codegraph_memory_list` | Browse with filters (kind, tags, pagination) |
| `codegraph_memory_context` | Get memories relevant to a file/function |
| `codegraph_memory_invalidate` | Mark a memory as outdated |
| `codegraph_memory_stats` | Storage statistics |
| `codegraph_mine_git_history` | Auto-create memories from commit patterns |
| `codegraph_mine_git_history_for_file` | Mine git history for a specific file |

### Workspace

| Tool | What it does |
|------|-------------|
| `codegraph_reindex_workspace` | Re-parse all files and rebuild the graph |

---

## Languages

14 languages, all parsed via tree-sitter into the same graph structure:

| Language | Functions | Imports | Call Graph | Complexity |
|----------|:---------:|:-------:|:----------:|:----------:|
| TypeScript / JavaScript | Yes | Yes | Yes | Yes |
| Python | Yes | Yes | Yes | Yes |
| Rust | Yes | Yes | Yes | Yes |
| Go | Yes | Yes | Yes | Yes |
| C | Yes | Yes | Yes | Yes |
| C++ | Yes | Yes | -- | Yes |
| Java | Yes | Yes | -- | Yes |
| Kotlin | Yes | Yes | -- | Yes |
| C# | Yes | Yes | -- | Yes |
| PHP | Yes | Yes | -- | Yes |
| Ruby | Yes | Yes | -- | Yes |
| Swift | Yes | Yes | -- | Yes |
| Tcl | Yes | Yes | -- | Yes |

All languages get dependency graphs, symbol search, impact analysis, and unused code detection. Call graph extraction is available for the top 5 languages.

---

## Architecture

```
MCP Client (Claude, Cursor, ...)        VS Code Extension
        |                                       |
    MCP (stdio)                            LSP Protocol
        |                                       |
        └───────────┐               ┌───────────┘
                    ▼               ▼
            ┌─────────────────────────────┐
            │     Rust Server (tower-lsp) │
            ├─────────────────────────────┤
            │  14 tree-sitter parsers     │
            │  Semantic graph engine      │
            │  AI query engine (BM25)     │
            │  Memory layer (RocksDB)     │
            │  Fastembed (384d ONNX)      │
            │  HNSW vector index          │
            └─────────────────────────────┘
```

A single Rust binary serves both transports. The `--mcp` flag switches from LSP to MCP (stdio JSON-RPC).

- **Indexing**: Sub-10s for 100k LOC. Incremental re-indexing on file changes via `notify` file watcher.
- **Queries**: Sub-100ms for navigation (callers, callees, symbols). Cross-file import and call resolution happens at index time.
- **Embeddings**: fastembed BGE-Small-EN-v1.5 (384d ONNX Runtime). Model auto-downloads on first run to `~/.codegraph/fastembed_cache/`.
- **Storage**: RocksDB for memories/vectors, HNSW (instant-distance) for O(log n) semantic search.

---

## Configuration (VS Code)

| Setting | Default | Description |
|---------|---------|-------------|
| `codegraph.enabled` | `true` | Enable/disable the extension |
| `codegraph.languages` | all 14 | Languages to index |
| `codegraph.indexOnStartup` | `true` | Index workspace on startup |
| `codegraph.maxFileSizeKB` | `1024` | Max file size to index |
| `codegraph.excludePatterns` | node_modules, target, ... | Glob patterns to exclude |
| `codegraph.ai.maxContextTokens` | `4000` | Token budget for AI context |
| `codegraph.visualization.defaultDepth` | `3` | Default graph traversal depth |
| `codegraph.parallelParsing` | `true` | Parallel file parsing |

---

## Building from Source

```bash
git clone https://github.com/codegraph-ai/codegraph-vscode
cd codegraph-vscode
npm install

# Build everything (TypeScript + Rust + package + install)
./scripts/build.sh --install

# Or build components separately:
cargo build --release -p codegraph-lsp    # Rust server only
npm run esbuild                           # TypeScript extension only
```

### Requirements

- **Node.js 18+** and npm
- **Rust toolchain** (stable)
- **VS Code 1.90+** (for the extension)

---

## License

Apache-2.0
