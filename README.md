# CodeGraph

**Cross-language code intelligence for AI agents and developers.**

[![VS Code Marketplace](https://img.shields.io/visual-studio-marketplace/i/astudioplus.codegraph?label=VS%20Code%20installs)](https://marketplace.visualstudio.com/items?itemName=astudioplus.codegraph)
[![npm](https://img.shields.io/npm/v/@memoryx/codegraph-mcp)](https://www.npmjs.com/package/@memoryx/codegraph-mcp)
[![License](https://img.shields.io/badge/License-Apache%202.0-green.svg)](LICENSE)

CodeGraph builds a semantic graph of your codebase — functions, classes, imports, call chains — and exposes it through **35 tools**, a **VS Code extension**, and a **persistent memory layer**. AI agents get structured code understanding instead of grepping through files.

## Quick Start

### MCP Server (Claude Code, Cursor, any MCP client)

```bash
npm install -g @memoryx/codegraph-mcp
```

Add to `~/.claude.json` (or your MCP client config):

```json
{
  "mcpServers": {
    "codegraph": {
      "command": "codegraph-mcp",
      "args": []
    }
  }
}
```

The server indexes the current working directory automatically. See `examples/` for advanced configs (multi-project, exclusions, model selection).

### VS Code Extension

Install from the [VS Code Marketplace](https://marketplace.visualstudio.com/items?itemName=astudioplus.codegraph) or manually:

```bash
code --install-extension codegraph-0.11.2.vsix
```

The extension registers all 35 MCP tools plus 3 additional VS Code-specific tools as Language Model Tools. To steer Copilot toward using them:

```jsonc
// .vscode/settings.json
{
  "github.copilot.chat.codeGeneration.instructions": [
    "When analyzing code structure, callers, callees, dependencies, or complexity, prefer codegraph_* tools over file search. CodeGraph has a pre-built semantic graph that returns structured results instantly."
  ]
}
```

---

## Configuration

### MCP Server flags

| Flag | Default | Description |
|------|---------|-------------|
| `--workspace <path>` | current dir | Directories to index (repeatable for multi-project) |
| `--exclude <dir>` | — | Directories to skip (repeatable) |
| `--embedding-model <model>` | `jina-code-v2` | `jina-code-v2` (768d, best quality) or `bge-small` (384d, 5x faster) |
| `--max-files <n>` | 5000 | Maximum files to index |

### VS Code settings

```jsonc
{
  "codegraph.indexOnStartup": true,
  "codegraph.indexPaths": ["/path/to/project-a", "/path/to/project-b"],
  "codegraph.excludePatterns": ["**/cmake-build-debug/**", "**/generated/**"],
  "codegraph.embeddingModel": "jina-code-v2",
  "codegraph.maxFileSizeKB": 1024,
  "codegraph.debug": false
}
```

### Embedding models

| Model | Dimensions | Speed (real code) | Clone detection | Download |
|-------|-----------|-------------------|-----------------|----------|
| `jina-code-v2` (default) | 768 | ~10 fn/sec | Excellent — clean separation at threshold 0.7 | 642MB |
| `bge-small` | 384 | ~64 fn/sec | Limited — no usable threshold for clones | 127MB |

Built-in exclusions (always skipped): `node_modules`, `target`, `dist`, `build`, `out`, `.git`, `__pycache__`, `vendor`, `DerivedData`, `tmp`, `coverage`, `logs`.

See `examples/` for complete configs for Claude Code, VS Code, and Cursor.

---

## Tools (35)

### Code Analysis (9)

| Tool | What it does |
|------|-------------|
| `get_ai_context` | **Primary context tool.** Intent-aware (explain/modify/debug/test) with token budgeting. Returns source, related symbols, imports, siblings, debug hints. |
| `get_edit_context` | Everything needed before editing: source + callers + tests + memories + git history |
| `get_curated_context` | Cross-codebase context for a natural language query ("how does auth work?") |
| `get_dependency_graph` | File/module import relationships with depth control |
| `get_call_graph` | Function call chains (callers and callees) |
| `analyze_impact` | Blast radius prediction — what breaks if you modify, delete, or rename |
| `analyze_complexity` | Cyclomatic complexity with breakdown (branches, loops, nesting, exceptions, early returns) |
| `find_unused_code` | Dead code detection with confidence scoring |
| `analyze_coupling` | Module coupling metrics and instability scores |

### Code Similarity (4)

Powered by configurable embeddings — [Jina Code V2](https://huggingface.co/jinaai/jina-embeddings-v2-base-code) (default) or BGE-Small for faster indexing.

| Tool | What it does |
|------|-------------|
| `find_duplicates` | Detect duplicate/near-duplicate functions across the codebase. Threshold 0.7 for clones, 0.9+ for near-exact copies. |
| `find_similar` | Find functions most similar to a given function. "Does this already exist?" |
| `cluster_symbols` | Group functions by semantic similarity — discovers patterns like "all DB access", "all error handlers" |
| `compare_symbols` | Deep comparison of two functions: similarity score, structural diff, shared callers/callees, verdict |

### Code Navigation (12)

| Tool | What it does |
|------|-------------|
| `symbol_search` | Find symbols by name or natural language (hybrid BM25 + semantic search) |
| `get_callers` / `get_callees` | Who calls this? What does it call? (with transitive depth) |
| `get_detailed_symbol` | Full symbol info: source, callers, callees, complexity |
| `get_symbol_info` | Quick metadata: signature, visibility, kind |
| `find_by_imports` | Find files importing a module |
| `find_by_signature` | Search by param count, return type, modifiers |
| `find_entry_points` | Main functions, HTTP handlers, CLI commands, event handlers |
| `find_related_tests` | Tests that exercise a given function |
| `traverse_graph` | Custom graph traversal with edge/node type filters |
| `cross_project_search` | Search across all indexed projects |

### Memory (10)

Persistent AI context across sessions — debugging insights, architectural decisions, known issues.

| Tool | What it does |
|------|-------------|
| `memory_store` / `memory_get` / `memory_search` | Store, retrieve, search memories (BM25 + semantic) |
| `memory_context` | Get memories relevant to a file/function |
| `memory_list` / `memory_invalidate` / `memory_stats` | Browse, retire, monitor |
| `mine_git_history` / `mine_git_history_for_file` | Auto-create memories from commits |
| `search_git_history` | Semantic search over commit history |

All tool names are prefixed with `codegraph_` (e.g. `codegraph_get_ai_context`). Tools that target a specific symbol accept `uri` + `line` or `nodeId` from `symbol_search` results.

---

## Languages

17 languages parsed via tree-sitter — all with functions, imports, call graph, complexity metrics, dependency graphs, symbol search, impact analysis, and unused code detection:

TypeScript/JS, Python, Rust, Go, C, C++, Java, Kotlin, C#, PHP, Ruby, Swift, Tcl, Verilog/SystemVerilog, COBOL, Fortran

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
            │  Shared Domain Layer (16 modules)  │
            ├─────────────────────────────┤
            │  17 tree-sitter parsers     │
            │  Semantic graph engine      │
            │  AI query engine (BM25)     │
            │  Memory layer (RocksDB)     │
            │  Configurable embeddings    │
            │  HNSW vector index          │
            └─────────────────────────────┘
```

A single Rust binary serves both MCP and LSP. Both protocols call the same domain layer — identical logic, identical results.

- **Indexing**: Sub-15s for 800+ files. Incremental re-indexing on file changes.
- **Queries**: Sub-100ms for navigation. Cross-file import and call resolution at index time.
- **Embeddings**: Jina Code V2 (768d) or BGE-Small (384d). Auto-downloads on first run. Configurable via `--embedding-model` or `codegraph.embeddingModel`.

---

## Building from Source

```bash
git clone https://github.com/codegraph-ai/codegraph-vscode
cd codegraph-vscode
npm install
cargo build --release -p codegraph-lsp    # Rust server
npm run esbuild                           # TypeScript extension
npx @vscode/vsce package                  # VSIX
```

Requires Node.js 18+, Rust stable, VS Code 1.90+.

---

## License

Apache-2.0
