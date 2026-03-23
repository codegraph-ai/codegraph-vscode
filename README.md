# CodeGraph

**Cross-language code intelligence for AI agents and developers.**

[![VS Code](https://img.shields.io/badge/VS%20Code-1.90+-blue.svg)](https://code.visualstudio.com/)
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

The server indexes the current working directory automatically — no `--workspace` flag needed. It also accepts MCP `roots` from the client for workspace discovery.

**Multi-project indexing** — index multiple codebases into a single graph:

```json
{
  "mcpServers": {
    "codegraph": {
      "command": "codegraph-mcp",
      "args": [
        "--workspace", "/path/to/frontend",
        "--workspace", "/path/to/backend",
        "--workspace", "/path/to/shared-lib"
      ]
    }
  }
}
```

### VS Code Extension

```bash
code --install-extension codegraph-0.11.0.vsix
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

## Tools (35)

### Code Analysis (9)

| Tool | What it does |
|------|-------------|
| `get_ai_context` | **Primary context tool.** Intent-aware (explain/modify/debug/test) with token budgeting. Returns source, related symbols (full or signature-only), file imports, sibling functions, debug hints, architecture. |
| `get_edit_context` | Everything needed before editing: source + callers + tests + memories + git history |
| `get_curated_context` | Cross-codebase context for a natural language query ("how does auth work?") |
| `get_dependency_graph` | File/module import relationships with depth control |
| `get_call_graph` | Function call chains (callers and callees) |
| `analyze_impact` | Blast radius prediction — what breaks if you modify, delete, or rename |
| `analyze_complexity` | Cyclomatic complexity with breakdown (branches, loops, nesting, exceptions, early returns) |
| `find_unused_code` | Dead code detection with confidence scoring |
| `analyze_coupling` | Module coupling metrics and instability scores |

### Code Similarity (4)

Powered by [Jina Code V2](https://huggingface.co/jinaai/jina-embeddings-v2-base-code) embeddings — code-aware semantic search trained on 150M+ code pairs across 30 languages.

| Tool | What it does |
|------|-------------|
| `find_duplicates` | Detect duplicate/near-duplicate functions across the codebase. Threshold 0.7 for clones, 0.9+ for near-exact copies. |
| `find_similar` | Find functions most similar to a given function. "Does this already exist?" |
| `cluster_symbols` | Group functions by semantic similarity — discovers patterns like "all DB access", "all error handlers" |
| `compare_symbols` | Deep comparison of two functions: similarity score, structural diff, shared callers/callees, verdict |

### Code Navigation (12)

| Tool | What it does |
|------|-------------|
| `symbol_search` | Find symbols by name or natural language (hybrid BM25 + Jina Code V2 semantic search) |
| `get_callers` / `get_callees` | Who calls this? What does it call? (with transitive depth) |
| `get_detailed_symbol` | Full symbol info: source, callers, callees, complexity |
| `get_symbol_info` | Quick metadata: signature, visibility, kind |
| `find_by_imports` | Find files importing a module (`moduleName` param) |
| `find_by_signature` | Search by param count, return type, modifiers |
| `find_entry_points` | Main functions, HTTP handlers, tests |
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

All tool names are prefixed with `codegraph_` (e.g. `codegraph_get_ai_context`).

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
            │  Jina Code V2 (768d ONNX)  │
            │  HNSW vector index          │
            └─────────────────────────────┘
```

A single Rust binary serves both MCP and LSP. Both protocols call the same domain layer — identical logic, identical results.

- **Indexing**: Sub-10s for 100k LOC. Incremental re-indexing on file changes.
- **Queries**: Sub-100ms for navigation. Cross-file import and call resolution at index time.
- **Embeddings**: Jina Code V2 (768d, code-aware). Auto-downloads on first run (~642MB).

---

## Indexing Configuration

Auto-indexing is **off by default**. Use the command palette (`CodeGraph: Index Directory`) for on-demand indexing, or configure paths:

```jsonc
// .vscode/settings.json
{
  "codegraph.indexOnStartup": true,
  "codegraph.indexPaths": [
    "/path/to/project-a",
    "/path/to/project-b"
  ],
  "codegraph.excludePatterns": ["**/logs/**", "**/*.bin"],
  "codegraph.maxFileSizeKB": 1024
}
```

`indexPaths` accepts any absolute paths — they don't have to be inside your workspace. All paths are indexed into a single unified graph. In multi-root workspaces, put `indexPaths` in **one** `settings.json` only (arrays are not merged across folders).

Always-skipped directories: `node_modules`, `target`, `.git`, `dist`, `build`, `out`, `__pycache__`, `vendor`, `DerivedData`, `tmp`, `coverage`, `logs`.

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
