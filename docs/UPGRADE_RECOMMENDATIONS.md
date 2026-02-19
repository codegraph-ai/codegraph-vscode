# CodeGraph Crate Upgrade Recommendations

> **Date**: 2026-02-18
> **Scope**: Upgrading codegraph ecosystem dependencies in codegraph-vscode
> **Source**: codegraph-monorepo (latest) vs codegraph-vscode (current)

---

## Current State

| Crate | vscode uses | monorepo has | Status |
|---|---|---|---|
| **codegraph** | 0.1.1 | **0.2.0** | Behind (major) |
| **codegraph-parser-api** | 0.2.0 | **0.2.1** | Behind (patch) |
| **codegraph-typescript** | 0.3.2 | **0.4.0** | Behind (significant) |
| **codegraph-rust** | 0.2.1 | 0.2.1 | In sync |
| **codegraph-python** | 0.4.1 | 0.4.1 | In sync |
| **codegraph-go** | 0.1.5 | 0.1.4 | vscode ahead (crates.io has 0.1.5, monorepo not synced) |
| **codegraph-c** | 0.1.3 | 0.1.3 | In sync |
| **New parsers** | — | cpp, java, kotlin, csharp, php, ruby, swift, tcl | Not adopted |

---

## What the Upgrades Bring

### codegraph 0.1.1 → 0.2.0

**Graph iteration & batch ops:**
- `iter_nodes()` / `iter_edges()` — replaces manual `node_count()` + ID loop pattern currently used in `ai_query/engine.rs`
- `add_nodes_batch()` / `add_edges_batch()` — atomic bulk inserts, faster initial indexing

**Export formats (new module):**
- `export_dot()` / `export_dot_styled(DotOptions)` — Graphviz visualization
- `export_json()` / `export_json_filtered()` — D3.js-compatible format
- `export_csv()` / `export_triples()` — CSV and RDF formats
- Could directly power the webview graph panel, which currently builds its own JSON

**QueryBuilder enhancements:**
- `file_pattern("**/*.rs")` — glob-based file filtering
- `name_contains(substr)` / `name_matches(regex)` — text search on names
- `count()` / `exists()` — without allocating results
- `custom(predicate)` — arbitrary filter closures
- The server currently only uses `.property()` and `.node_type()` queries

**Graph algorithms (new methods on CodeGraph):**
- `transitive_dependencies(file_id, max_depth)` / `transitive_dependents()`
- `call_chain(from, to, max_depth)` — find call paths between functions
- `circular_deps()` — Tarjan's SCC cycle detection
- `bfs()` / `dfs()` / `find_all_paths()` / `find_strongly_connected_components()`
- The server currently implements BFS/DFS manually in handlers — these built-in methods would replace ~200 lines of hand-rolled traversal code

**PropertyMap improvements:**
- New variants: `StringList(Vec<String>)`, `IntList(Vec<i64>)`, `Null`
- New methods: `remove()`, `is_empty()`, `get_string_list()`, `get_int_list()`
- `FromIterator` impl for ergonomic construction

**Helpers:**
- `find_file_by_path(graph, path)` — path→NodeId lookup
- `node_ids_to_paths(graph, ids)` — bulk NodeId→path resolution
- `FunctionMetadata` struct for richer function node creation

**Breaking changes to watch:**
- `StorageBackend` trait now requires `write_batch()` (not used directly — vscode only calls `in_memory()`)
- `GraphError` has new variants (`FileNotFound`, `PropertyNotFound`, `PropertyTypeMismatch`) — exhaustive matches would break
- `PropertyValue` has new variants — same concern

### codegraph-parser-api 0.2.0 → 0.2.1

- **`ComplexityMetrics`** struct: cyclomatic complexity, branches, loops, logical operators, nesting depth, exception handlers, early returns, letter grading (A-F)
- **`ComplexityBuilder`** for incremental complexity tracking during AST traversal
- **`FunctionEntity.complexity: Option<ComplexityMetrics>`** — parsers now attach real complexity data to functions
- The vscode server already reads complexity as raw integer properties from the graph — the upgrade means parsers populate these properties with real AST-derived metrics instead of estimates

### codegraph-typescript 0.3.2 → 0.4.0

The biggest practical win since TypeScript/JavaScript is the most common language:

- **Method extraction** — classes now emit method nodes (missing in 0.3.x)
- **Call expression tracking** — `this.foo()`, `obj.method()`, nested calls, `await` calls now create `Calls` edges
- **Cyclomatic complexity** — real AST-based complexity for every function/method
- **Triple-slash directive parsing** — `/// <reference path="..." />` creates import relationships
- **Full JSX/TSX support** — proper tree-sitter grammar selection by extension
- **Parallel parsing** — rayon-based multi-threaded file parsing when `config.parallel = true`

### New language parsers (8 available)

| Parser | Version | Grammar | Notes |
|---|---|---|---|
| codegraph-cpp | 0.2.0 | tree-sitter-cpp 0.22 | C++ support |
| codegraph-java | 0.1.1 | tree-sitter-java 0.21 | Java support |
| codegraph-kotlin | 0.1.1 | tree-sitter-kotlin 0.3 | .kt/.kts, data classes |
| codegraph-csharp | 0.1.1 | tree-sitter-c-sharp 0.21 | C# support |
| codegraph-php | 0.2.0 | tree-sitter-php 0.22 | PHP support |
| codegraph-ruby | 0.2.0 | tree-sitter-ruby 0.21 | Ruby support |
| codegraph-swift | 0.1.1 | tree-sitter-swift 0.5 | Swift support |
| codegraph-tcl | 0.1.0 | custom (cc build) | Tcl with EDA/VLSI support |

All require codegraph 0.2.0 + parser-api 0.2.1.

---

## Recommendations

### Tier 1 — High Value (do first)

#### 1. Bump core dependencies

Update `Cargo.toml` workspace dependencies:

```toml
codegraph = "0.2.0"
codegraph-parser-api = "0.2.1"
codegraph-typescript = "0.4.0"
```

This is the prerequisite for everything else. The `codegraph-typescript` bump is bundled here because it requires the core bumps and delivers the most user-visible improvement.

#### 2. Replace hand-rolled graph traversal with built-in algorithms

The server currently implements BFS/DFS manually in `handlers/navigation.rs`, `handlers/ai_context.rs`, and `handlers/custom.rs`. Replace with:

- `graph.transitive_dependencies(file_id, max_depth)` for dependency graph traversal
- `graph.transitive_dependents(file_id, max_depth)` for reverse dependency traversal
- `graph.call_chain(from_func, to_func, max_depth)` for call path finding
- `graph.circular_deps()` for cycle detection
- `graph.bfs()` / `graph.dfs()` for general traversal

Estimated reduction: ~200 lines of traversal code.

#### 3. Replace manual node iteration with `iter_nodes()` / `iter_edges()`

In `ai_query/engine.rs`, the `build_indexes()` method iterates nodes by counting up to `node_count()` and calling `get_node()` on each ID. Replace with `graph.iter_nodes()`.

#### 4. Fix MCP complexity handler

`server/src/mcp/server.rs` estimates complexity from callee count (`callees.len().max(1)`). The upgraded parsers populate real AST-derived `"complexity"` properties on function nodes. Update the MCP handler to read these properties (like the LSP handler in `handlers/metrics.rs` already does).

### Tier 2 — Moderate Value

#### 5. Use QueryBuilder enhancements in handlers

Replace manual property filtering with:
- `graph.query().name_contains(query)` in symbol search
- `graph.query().file_pattern("**/*.rs")` in file-scoped queries
- `graph.query().node_type(X).count()` instead of `.execute().len()`
- `graph.query().node_type(X).exists()` for existence checks

#### 6. Use built-in export formats for graph visualization

`views/graphPanel.ts` and the dependency/call graph handlers build custom JSON responses. The new `export_json()` / `export_json_filtered()` methods produce D3.js-compatible output directly, and `export_dot_styled(DotOptions)` could support Graphviz rendering.

#### 7. Use batch operations for faster indexing

Replace sequential `add_node()` / `add_edge()` calls during workspace indexing with `add_nodes_batch()` / `add_edges_batch()` for atomic, faster bulk inserts.

#### 8. Add new language parsers

Priority order based on user demand:
1. **Java** (`codegraph-java`) — enterprise users
2. **C++** (`codegraph-cpp`) — systems programming users
3. **Kotlin** (`codegraph-kotlin`) — Android/JVM users
4. **C#** (`codegraph-csharp`) — .NET users

Each parser addition requires:
- Adding the crate dependency to `server/Cargo.toml`
- Adding the parser to `ParserRegistry::new()` in `parser_registry.rs`
- Adding the language to `documentSelector` in `extension.ts`
- Adding the language to `activationEvents` and `codegraph.languages` defaults in `package.json`

#### 9. Sync codegraph-go

`codegraph-go 0.1.5` is published on crates.io and used by vscode, but the monorepo is at 0.1.4. Either:
- Backport the 0.1.5 changes to the monorepo, or
- Verify 0.1.5 is compatible with codegraph 0.2.0 + parser-api 0.2.1

### Tier 3 — Nice-to-Have

#### 10. Expose ComplexityMetrics directly in tool responses

Instead of returning raw integers for complexity, use the `ComplexityMetrics` struct to provide richer responses including grade, breakdown by category, and threshold comparison.

#### 11. Use `find_file_by_path()` helper

Replace manual `graph.query().property("path", path_value).execute()` lookups with the built-in `find_file_by_path(graph, path)` helper for cleaner, faster file node resolution.

#### 12. Add remaining parsers

PHP, Ruby, Swift, and Tcl parsers are available if demand arises. Tcl is niche but includes EDA/VLSI domain support.

#### 13. Use PropertyMap improvements

Adopt `StringList` / `IntList` property variants for multi-valued properties (e.g., storing multiple imported symbols on an edge as a `StringList` instead of a comma-separated string).

---

## Upgrade Path

All monorepo parser crates depend on `codegraph 0.2.0` + `codegraph-parser-api 0.2.1`. The upgrade must happen atomically:

```
Step 1: Bump codegraph + codegraph-parser-api + codegraph-typescript in Cargo.toml
Step 2: Fix any breaking changes (GraphError/PropertyValue exhaustive matches)
Step 3: cargo build --release to verify compilation
Step 4: Run test suite, fix any failures
Step 5: Refactor handlers to use new APIs (Tier 1 items 2-4)
Step 6: Add new parsers (Tier 2 item 8)
```

**Breaking change risk is LOW** — the vscode server uses `CodeGraph::in_memory()` exclusively (no custom `StorageBackend` impls), doesn't exhaustively match `GraphError` or `PropertyValue` variants, and calls parsers only through the `CodeParser` trait.
