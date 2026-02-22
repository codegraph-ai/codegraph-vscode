# CodeGraph VS Code — TODO

> Last updated: 2026-02-21

## High Priority — MCP Tool Fixes

Issues discovered via MCP tool verification (162 scenarios, 78 pass / 30 fail / 54 warn).

### 7. Fix get_ai_context to return actual context (RC-5)
All 6 scenarios fail. Returns skeleton metadata instead of source code, callers/callees context, layer detection, and usage descriptions. The handler runs but produces empty/minimal results.

### 8. Fix find_unused_code false negatives (RC-4)
4/8 scenarios fail. Reports zero unused symbols in files that clearly have dead code. Partly addressed (7f758e1 improved test filtering and Contains edge checks) but core detection still misses cases.

### 9. Fix memory filtering: kinds, tags, currentOnly, offset (RC-6)
Memory tools ignore filter parameters. `kinds` filter returns unfiltered results, `tags` filter has no effect, `currentOnly: false` still excludes invalidated memories, `offset` pagination not implemented.

### 10. Fix traverse_graph edgeTypes and nodeTypes filters
4/8 scenarios warn. `edgeTypes: ["calls"]` and `nodeTypes: ["function"]` filters are accepted but don't constrain results — all edge/node types are returned regardless.

### 11. Fix symbolType filter in symbol_search for class/interface
`symbolType: "class"` returns functions. The type filter mapping doesn't correctly translate MCP parameter values to internal NodeType filtering.

### 12. Implement summary/compact response modes (RC-2)
`summary: true` and `compact: true` parameters are accepted but produce identical output to default mode. Affects get_dependency_graph, get_call_graph, analyze_coupling, traverse_graph.

## Medium Priority

### 3. Expose ComplexityMetrics in tool responses
MCP and LSP complexity handlers return raw integers. The `ComplexityMetrics` struct from codegraph-parser-api provides richer data: grade, breakdown by category, threshold comparison. Would improve `codegraph_analyze_complexity` output.

### 4. Use PropertyMap improvements
Adopt `StringList` / `IntList` property variants for multi-valued properties (e.g., storing multiple imported symbols on an edge as a `StringList` instead of a comma-separated string).

### 13. Fix result deduplication across tools (RC-8)
Some tools return duplicate entries for the same symbol. Affects symbol_search and find_entry_points when a function appears in multiple index paths.

### 14. Fix TS private method visibility indexing
TypeScript private methods are indexed but `visibility` property is not consistently set, causing `modifiers: ["private"]` filter in find_by_signature to miss them.

### 15. Fix memory_invalidate error on nonexistent IDs
`memory_invalidate` silently succeeds when given a non-existent memory ID. Should return an error.

### 16. Implement real Rust cyclomatic complexity scoring (RC-7)
MCP `analyze_complexity` returns 0 for Rust functions because the Rust parser doesn't compute cyclomatic complexity. TypeScript parser does. Need to add branch-counting to the Rust tree-sitter walker.

## ~~Medium Priority — codegraph-monorepo~~ (Completed)

~~### 17. Add type-safety tests to codegraph-monorepo mapper crates~~
~~### 18. Audit remaining .to_string() numeric properties in codegraph-monorepo~~
Both completed (3753293) — fixed `.to_string()` on booleans and integers across all 12 mapper crates, added `test_property_types` regression tests, updated integration tests.

## Future / On Demand

### 5. Add remaining language parsers
~~PHP, Ruby, Swift, and Tcl parsers are available in codegraph-monorepo if demand arises.~~ All added in v0.6.0. No remaining parsers to add.

### 6. Publish to VS Code Marketplace
Currently at v0.6.0 locally. Requires marketplace publisher setup, CI/CD pipeline for packaging, and automated VSIX builds.

---

## Completed

- ~~Add type-safety tests + audit .to_string() numeric properties (#17, #18)~~ — Fixed booleans and integers across all 12 mapper crates (3753293), added `test_property_types` regression tests, updated C#/Java/PHP integration tests.
- ~~Fix line-to-node resolution (RC-1)~~ — PropertyValue type mismatch: all mappers stored line numbers as strings, `get_int()` only matched Int variant. Fixed in codegraph-monorepo (8969901) with defensive getter + 11 mapper fixes.
- ~~Fix find_by_signature filters (RC-3)~~ — 7/10 scenarios fixed: glob-to-regex conversion, signature-based param counting fallback, return type extraction from signature, visibility string property check.
- ~~Improve unused code detection and test discovery~~ (7f758e1) — cross-file import resolution on init, Contains edge checks, test framework function filtering, is_test property, same-file test fallback.
- ~~v0.6.0: add 4 languages, call extraction for Rust/Go/C, git mining MCP tools~~ (d3a037e)
- ~~Fill in AI query engine TODOs~~ (4bbf24a) — edge_type, dependencies, dependents, has_tests
- ~~Implement MCP git mining tools~~ (ae3b0b0)
- ~~Broaden git mining to catch non-conventional commits~~ (ae3b0b0)
- ~~Use find_file_by_path() for file-node lookups~~ (0f008ba)
- ~~Replace hand-rolled BFS with built-in algorithms~~ (681e68d)
- ~~Auto-download Model2Vec embedding model~~ (231fad4)
- ~~Add Java, C++, Kotlin, C# parsers~~ (e8b1918)
- ~~Upgrade codegraph ecosystem to 0.2.0~~ (77f597a)
- ~~Replace manual node iteration with iter_nodes()~~ (77f597a)
- ~~Fix MCP complexity handler~~ (77f597a)
- ~~Add proximity fallback to MCP symbol tools~~ (4226e24, 963c79f)
- ~~Add compact mode and limit options to search tools~~ (800efbf)
- ~~Implement MCP transport~~ (7bfc499)
- ~~Implement remaining 18 MCP tools~~ (689bd36)
- ~~Add memory layer with git mining~~ (87a2c01)
- ~~Implement AI Agent Query Engine~~ (a40d75f)
