# Changelog

All notable changes to the CodeGraph extension will be documented in this file.

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
