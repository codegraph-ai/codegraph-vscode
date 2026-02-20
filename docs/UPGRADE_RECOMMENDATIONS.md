# CodeGraph Crate Upgrade Recommendations

> **Date**: 2026-02-18 (updated 2026-02-19)
> **Scope**: Upgrading codegraph ecosystem dependencies in codegraph-vscode
> **Source**: codegraph-monorepo (latest) vs codegraph-vscode (current)

---

## Current State

| Crate | vscode uses | monorepo has | Status |
|---|---|---|---|
| **codegraph** | 0.2.0 | 0.2.0 | In sync |
| **codegraph-parser-api** | 0.2.1 | 0.2.1 | In sync |
| **codegraph-typescript** | 0.4.0 | 0.4.0 | In sync |
| **codegraph-rust** | 0.2.1 | 0.2.1 | In sync |
| **codegraph-python** | 0.4.1 | 0.4.1 | In sync |
| **codegraph-go** | 0.1.5 | 0.1.4 | vscode ahead (crates.io has 0.1.5, monorepo not synced) |
| **codegraph-c** | 0.1.3 | 0.1.3 | In sync |
| **codegraph-cpp** | 0.2.0 | 0.2.0 | In sync |
| **codegraph-java** | 0.1.1 | 0.1.1 | In sync |
| **codegraph-kotlin** | 0.1.1 | 0.1.1 | In sync |
| **codegraph-csharp** | 0.1.1 | 0.1.1 | In sync |
| **Remaining parsers** | — | php, ruby, swift, tcl | Not adopted |

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

### Tier 1 — High Value ✅ COMPLETED

#### 1. ✅ Bump core dependencies (77f597a)

Updated codegraph 0.2.0, codegraph-parser-api 0.2.1, codegraph-typescript 0.4.0.

#### 2. ✅ Replace hand-rolled graph traversal with built-in algorithms (681e68d)

Replaced 3 of 5 hand-rolled BFS traversals with built-in algorithms:
- Dependency graph handler → `transitive_dependencies()`/`transitive_dependents()`
- Impact analysis → `graph.bfs()`
- MCP dependency graph → `graph.bfs()`

Call graph handler and debug context kept hand-rolled (need edge-type-filtered BFS / budget-constrained single-path walk).

#### 3. ✅ Replace manual node iteration with `iter_nodes()` (77f597a)

Replaced `node_count()` + ID loop in `ai_query/engine.rs` with `graph.iter_nodes()`.

#### 4. ✅ Fix MCP complexity handler (77f597a)

MCP handler now reads real AST-derived complexity properties instead of estimating from callee count.

#### Additional: ✅ Add Java, C++, Kotlin, C# parsers (e8b1918)

Added 4 new language parsers with full test coverage.

#### Additional: ✅ Auto-download Model2Vec embeddings (231fad4)

Added `ureq`-based auto-download of potion-base-8M model on first start. All 25 MCP tools now functional.

### Tier 2 — Moderate Value ✅ COMPLETED

#### 5. ✅ Use QueryBuilder enhancements in handlers

Replaced 2 file-node lookups with `find_file_by_path()`:
- `custom.rs` dependency graph handler
- `mcp/server.rs` MCP dependency graph handler

Investigated but skipped:
- `name_contains()` — symbol search uses custom text index, not `graph.query()`. No handlers do name filtering via QueryBuilder.
- `count()` — already used in `resources.rs`. No other standalone use cases (sites that check `is_empty()` also need the result nodes).
- `exists()` — same issue; replaced by `find_file_by_path()` returning `Option<NodeId>` which is cleaner.

#### 6. Skipped — Export formats incompatible

`export_json()` uses D3-style `source`/`target` keys. VSCode convention uses `from`/`to` with string IDs. Not a drop-in replacement.

#### 7. Skipped — Batch operations semantic mismatch

`add_edges_batch()` is all-or-nothing (verifies all nodes exist, fails entire batch if any missing). Current code uses `let _ = add_edge()` (best-effort, silently ignores individual failures). Changing semantics would reduce resilience during incremental indexing.

#### 8. ✅ Add new language parsers (e8b1918)

Java, C++, Kotlin, C# — all added with full test coverage.

#### 9. Resolved — codegraph-go already in sync

Uses path dependency (`../codegraph-monorepo/crates/codegraph-go`). Already compatible with codegraph 0.2.0 + parser-api 0.2.1. No action needed.

### Tier 3 — Nice-to-Have

#### 10. Expose ComplexityMetrics directly in tool responses

Instead of returning raw integers for complexity, use the `ComplexityMetrics` struct to provide richer responses including grade, breakdown by category, and threshold comparison.

#### 11. ✅ Use `find_file_by_path()` helper (promoted to Tier 2 #5)

Completed as part of Tier 2 #5 above.

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
