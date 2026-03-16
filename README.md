# CodeGraph

**Cross-language code intelligence for AI agents and developers.**

[![VS Code](https://img.shields.io/badge/VS%20Code-1.90+-blue.svg)](https://code.visualstudio.com/)
[![License](https://img.shields.io/badge/License-Apache%202.0-green.svg)](LICENSE)

CodeGraph builds a semantic graph of your codebase — functions, classes, imports, call chains — and exposes it through **31 tools**, a **VS Code extension**, and a **persistent memory layer**. AI agents get structured code understanding instead of grepping through files.

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
      "args": []
    }
  }
}
```

The server automatically indexes the current working directory. Claude Code sets the cwd to your project, so no `--workspace` flag is needed. For explicit control, pass `--workspace /path/to/project`.
</details>

<details>
<summary>Cursor</summary>

```json
{
  "mcp.servers": {
    "codegraph": {
      "command": "codegraph-mcp",
      "args": []
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

# Run as MCP server (indexes cwd by default)
./target/release/codegraph-lsp --mcp

# Or specify a workspace explicitly
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

The extension registers 30 tools as VS Code Language Model Tools, so Copilot and other VS Code AI features can use them directly.

To steer Copilot toward using CodeGraph tools, add this to your `.vscode/settings.json`:

```jsonc
{
  "github.copilot.chat.codeGeneration.instructions": [
    "When analyzing code structure, callers, callees, dependencies, or complexity, prefer codegraph_* tools over file search. CodeGraph has a pre-built semantic graph that returns structured results instantly."
  ]
}
```

---

## Tools

### Code Analysis

| Tool | What it does |
|------|-------------|
| `codegraph_get_dependency_graph` | File/module import relationships |
| `codegraph_get_call_graph` | Function call chains (callers and callees) |
| `codegraph_analyze_impact` | Blast radius of a change — what breaks if you modify, delete, or rename |
| `codegraph_get_ai_context` | Intent-aware code context (explain, modify, debug, test) |
| `codegraph_get_edit_context` | Change-aware context: function source + callers + tests + memories + git changes |
| `codegraph_get_curated_context` | Hierarchical context curation with priority-based token budget |
| `codegraph_analyze_complexity` | Cyclomatic complexity per function with detailed breakdown |
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
| `codegraph_cross_project_search` | Search symbols across all indexed projects |

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
| `codegraph_search_git_history` | Semantic + keyword search over git commit history |

### Workspace

| Tool | What it does |
|------|-------------|
| `codegraph_reindex_workspace` | Re-parse all configured paths and rebuild the graph (respects `excludePatterns` and `indexPaths`) |

**Command palette commands** (VS Code only):

| Command | What it does |
|---------|-------------|
| **CodeGraph: Index Directory** | Prompt to pick a directory and index it on-demand |
| **CodeGraph: Reindex Workspace** | Rebuild graph for all configured `indexPaths` (or all workspace folders if empty) |

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
| `codegraph.indexOnStartup` | `false` | Auto-index workspace on startup. Keep `false` for large workspaces — use the **Index Directory** command instead. |
| `codegraph.indexPaths` | `[]` | Explicit list of absolute paths to index. When set, only these directories are indexed (on startup if `indexOnStartup: true`, or on reindex). Empty means all workspace folders. |
| `codegraph.maxFileSizeKB` | `1024` | Skip files larger than this size (in KB). Protects against indexing large generated or binary files. |
| `codegraph.excludePatterns` | `[]` | Glob patterns matched against full file paths. Matching files and directories are skipped. |
| `codegraph.ai.maxContextTokens` | `4000` | Token budget for AI context |
| `codegraph.visualization.defaultDepth` | `3` | Default graph traversal depth |
| `codegraph.parallelParsing` | `true` | Parallel file parsing |

---

## Configuring Indexing & Exclusions

By default, **auto-indexing on startup is disabled** (`indexOnStartup: false`). This prevents runaway memory usage in large or mixed-content workspaces (e.g. fileshares, test artifact directories, monorepos with binaries).

### On-demand indexing

Use the command palette to index specific directories when you need them:

1. `Ctrl+Shift+P` → **CodeGraph: Index Directory**
2. Pick the directory to index (e.g. `src/`, `server/src/`)
3. The graph is built incrementally — subsequent saves update it automatically via file watchers

### Targeting specific paths on startup

To auto-index only your source directories (not the whole workspace):

```jsonc
// .vscode/settings.json
{
  "codegraph.indexOnStartup": true,
  "codegraph.indexPaths": [
    "/home/user/projects/myapp/src",
    "/home/user/projects/myapp/server/src"
  ]
}
```

### Indexing multiple projects / external directories

`codegraph.indexPaths` accepts any absolute paths — they don't have to be inside your workspace. This is useful for indexing reference codebases, driver source, or shared libraries alongside your project:

```jsonc
// .vscode/settings.json
{
  "codegraph.indexOnStartup": true,
  "codegraph.indexPaths": [
    "/home/user/projects/my-project",
    "/home/user/projects/open-vm-tools/open-vm-tools",
    "/home/user/projects/linux-ice-driver/src"
  ]
}
```

All paths are indexed into a single unified graph, so cross-project symbol search, call graph analysis, and dependency tracking work across them.

### Multi-root workspaces — settings merge caveat

VS Code resolves `codegraph.indexPaths` from the **first workspace folder's** `settings.json`. Array settings are **not merged** across folders — one folder wins and the others are ignored.

**Put all `codegraph.indexPaths` in a single `settings.json`** (either your primary folder's `.vscode/settings.json` or in User settings):

```jsonc
// primary-project/.vscode/settings.json  ← put it here only
{
  "codegraph.indexPaths": [
    "/home/user/projects/frontend",
    "/home/user/projects/backend",
    "/home/user/projects/shared-libs"
  ]
}
```

If you set `indexPaths` in multiple folders' settings, only one will take effect. This can silently cause missing indexes.

### Excluding directories and files

Add glob patterns to skip noise from logs, test artifacts, binaries, and large data directories:

```jsonc
// .vscode/settings.json
{
  "codegraph.excludePatterns": [
    "**/logs/**",
    "**/results/**",
    "**/binaries/**",
    "**/*.log",
    "**/*.vib",
    "**/*.sva"
  ]
}
```

Patterns follow standard glob syntax (`**` for any path segment, `*` for any filename characters). They are matched against the full absolute path of each file and directory.

The following directories are always skipped regardless of settings: `node_modules`, `target`, `.git`, `dist`, `build`, `out`, `__pycache__`, `vendor`, `DerivedData`, `tmp`, `coverage`, `htmlcov`, `results`, `logs`.

### Large workspace / fileshare example

For a workspace with a mounted fileshare (`/mnt/share`) alongside source code:

```jsonc
{
  "codegraph.indexOnStartup": false,
  "codegraph.excludePatterns": [
    "/mnt/share/**",
    "**/logs/**",
    "**/test_results/**",
    "**/*.bin",
    "**/*.iso"
  ]
}
```

Then index only what you need via **CodeGraph: Index Directory**.

### Reindexing

After changing exclude patterns or index paths, use **CodeGraph: Reindex Workspace** (`Ctrl+Shift+P`) to rebuild the graph with the new settings. This clears the existing graph and re-parses all configured paths.

### MCP server workspace discovery

When running as an MCP server (for Claude Code, Cursor, etc.), the workspace is determined in this order:

1. **Client `roots`** from MCP `initialize` params — sent automatically by MCP clients
2. **`--workspace` CLI flag** — explicit override
3. **Current working directory** — Claude Code sets this to the project directory

A global MCP config without `--workspace` just works:

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
