# Changelog

All notable changes to the CodeGraph extension will be documented in this file.

## [0.5.0] - 2026-01-29

### Added

- **MCP Server Transport**: CodeGraph LSP now supports Model Context Protocol (MCP) as a second transport
  - Run with `codegraph-lsp --mcp` for JSON-RPC 2.0 over stdio
  - Compatible with Claude Desktop, Cursor, Claude Code CLI, and other MCP clients
  - All 26 tools available via MCP including code analysis, navigation, search, and memory
- **Standalone npm Package**: Distribute MCP server independently from VS Code extension
  - Install globally: `npm install -g @memoryx/codegraph-mcp`
  - Run: `codegraph-mcp` (automatically passes `--mcp` flag)
- **Memory System Tools** (9 new tools):
  - `codegraph_memory_store` - Persist debugging insights, architectural decisions, conventions
  - `codegraph_memory_search` - Hybrid BM25 + semantic search across stored memories
  - `codegraph_memory_get` - Retrieve full memory details by ID
  - `codegraph_memory_context` - Find memories relevant to current code location
  - `codegraph_memory_list` - List all memories with filtering
  - `codegraph_memory_invalidate` - Mark outdated memories without deleting
  - `codegraph_memory_stats` - Get memory system statistics
  - `codegraph_mine_git_history` - Create memories from git commit patterns
  - `codegraph_mine_git_file` - Mine history for specific files

### Changed

- On-demand database opening to avoid RocksDB lock conflicts when running multiple instances
- Model path discovery now supports `CODEGRAPH_MODELS_PATH` environment variable for npm package

### Fixed

- Extension file type matching (`.py` vs `py` format)
- Memory initialization with proper extension path resolution

## [0.4.0] - 2026-01-22

### Added

- **Episodic Memory System**: Graph-based memory with semantic search
  - Model2Vec embeddings (256d, ~8000 samples/sec)
  - RocksDB-backed persistent storage
  - Temporal tracking with utility propagation

## [0.3.1] - 2026-01-01

### Fixed

- **Cross-file import resolution**: Exported classes and functions are now correctly detected as "used" when imported by other files
- **Cross-file call resolution**: Function calls to symbols in other files now create proper call edges in the dependency graph
- **Reindex workspace command**: Now properly re-parses all workspace files and resolves cross-file relationships
- **`codegraph_find_unused_code` false positives**: Fixed issue where exported symbols that were imported elsewhere were incorrectly flagged as unused
- **Framework entry points**: VS Code extension entry points (`activate`/`deactivate`) are no longer flagged as unused
- **Trait implementations**: Rust trait methods and LSP protocol handlers are now excluded from unused code detection

### Changed

- TypeScript parser now stores unresolved calls for post-processing cross-file resolution
- LSP document handlers (`did_open`, `did_change`, `did_save`) now trigger cross-file resolution after parsing
- All language parsers (Python, Rust, Go, C) now support cross-file call resolution

## [0.3.0] - 2025-12-31

### Added

- **AI Agent Query Engine**: Fast, composable query primitives for AI agents to explore codebases
  - BM25-based text index with intelligent tokenization (camelCase, snake_case, acronyms)
  - Sub-10ms query performance for symbol search
  - 8 new Language Model Tools (now 17 total):
    - `codegraph_symbol_search` - Fast text-based symbol search with BM25 ranking
    - `codegraph_find_by_imports` - Discover code by imported libraries/modules
    - `codegraph_find_entry_points` - Detect architectural entry points (HTTP handlers, CLI, main, etc.)
    - `codegraph_traverse_graph` - Custom graph traversal with filters and depth control
    - `codegraph_get_callers` - Find all callers of a function
    - `codegraph_get_callees` - Find all functions called by a function
    - `codegraph_get_detailed_symbol` - Rich metadata retrieval for any symbol
    - `codegraph_find_by_signature` - Find functions by signature pattern (name, params, return type, modifiers)

### Changed

- Improved query engine architecture for better composability

## [0.2.1] - 2025-12-31

### Changed

- Minor documentation fixes.

## [0.2.0] - 2025-12-30

### Added

- **C Language Support**: Full parsing support for C source files using `codegraph-c` parser
  - Tolerant kernel mode for parsing Linux kernel drivers and system code without `compile_commands.json`
  - Handles kernel macros (`__init`, `__exit`, `likely()`, etc.) and GCC extensions
  - For full semantic analysis, generate a compilation database using `bear -- make`
- Extension icon for VS Code marketplace
- 3 additional Language Model Tools (now 9 total):
  - `codegraph_analyze_complexity` - Measure cyclomatic and cognitive complexity
  - `codegraph_find_unused_code` - Detect dead code for cleanup
  - `codegraph_analyze_coupling` - Analyze module coupling and cohesion
