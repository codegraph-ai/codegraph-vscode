# CodeGraph VS Code — TODO

> Last updated: 2026-02-25

## High Priority — MCP Tool Fixes

Issues discovered via MCP tool verification (162 scenarios, 78 pass / 30 fail / 54 warn).

### 8. Fix find_unused_code false positives (RC-4)
~~Core issue resolved.~~ False positives reduced from 158 → 50 at ≥0.8 confidence across 4 fixes (8a–8d + cross-file imports). Remaining 50 items are mostly Rust test helpers, VS Code callback methods, and ~19 interfaces only used within their defining file via `as` casts or inline patterns the parser doesn't yet detect.

Remaining false positive categories (not blocking, diminishing returns):
- **Rust test helpers** (~14): `create_test_backend`, `make_commit`, etc. — correctly flagged when `includeTests: false`
- **VS Code callbacks** (~8): `provideFollowups`, `getIcon`, `setup` — framework entry points called by VS Code runtime, not via code. Could add to `is_framework_entry_point` allowlist.
- **Classes instantiated cross-file** (~9): `MemoryTreeProvider`, `SymbolTreeProvider`, etc. — `new ClassName()` not detected as Instantiates edge cross-file
- **Interfaces used only inline** (~19): `*Params` interfaces used via `as` casts or in same-file `RequestType<>` generics without cross-file import

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

## Future / On Demand

### 6. Publish to VS Code Marketplace
Currently at v0.7.0 locally. Requires marketplace publisher setup, CI/CD pipeline for packaging, and automated VSIX builds.

### 19. Extend type reference extraction to other languages
TypeScript type reference extraction (8c) now works for parameter types, return types, interface fields. Could extend to Rust (trait bounds, generic params, struct field types), Go (interface embedding, struct field types), etc.

### 20. Detect type references in expressions (generic args, `as` casts)
Interfaces like `*Params` used as generic type arguments (`new RequestType<DependencyGraphParams, ...>`) or `as` casts (`params as CallGraphParams`) are not detected by the current type reference extraction, which only scans function parameter/return annotations and interface field types. Needs extraction from `new_expression` type arguments, `as_expression` type targets, and variable type annotations (`const x: MyType`). Would eliminate ~19 remaining interface false positives in find_unused_code.

### 21. Cross-file `new ClassName()` instantiation detection
`new ClassName()` in another file doesn't create an Instantiates edge to the class definition. The mapper only creates Instantiates for same-file `new` expressions. Needs cross-file resolution similar to how `resolve_cross_file_imports` works for Imports edges.

---

## Completed

- ~~Fix find_unused_code false positives — all 4 sub-issues (#8a–8d)~~ — Reduced from 158 → 50 at ≥0.8 confidence. Four fixes: (a) Rust macro body call extraction via `extract_calls_from_macro()` heuristic (09a94df); (b) Rust method reference detection for `Self::method` / `self.method` used as values (f313da6); (c) TypeScript type annotation References edges via new `TypeReference` IR struct, `extract_type_names()` recursive extractor, and mapper edge creation (f313da6); (d) Arrow function call attribution — nested arrows recurse into enclosing function's context (f313da6). Plus: cross-file Imports edges now count as usage in find_unused_code (e39157e).
- ~~Fix find_unused_code core detection (#8 core)~~ — Imports/ImportsFrom edges no longer counted as usage, Type/Interface nodes no longer blanket-skipped. Reduced from 0 → 362 unused at 0.5 confidence.
- ~~Fix Rust macro body call extraction (#8a)~~ — tree-sitter treats macro invocation bodies as opaque `token_tree` nodes. Added heuristic `extract_calls_from_macro()` in codegraph-rust visitor. Handles `Self::method()`, `self.method()`, bare `func()`. Verified: `handle_event` no longer flagged as unused. (09a94df, be9a66f)
- ~~Implement real cyclomatic complexity scoring (#16)~~ — Added to all parsers in codegraph-monorepo (61d07fd, bcd7173, 1097a32). Rust, Go, PHP, Ruby, Java, C, C++, C#, Kotlin all now compute branches, loops, logical operators, nesting depth. MCP handler already reads complexity properties from graph.
- ~~Fix MCP transport CPU spin on client disconnect~~ — EOF from stdin returned `Ok(None)` causing tight infinite loop at 100% CPU per orphaned process. Fixed both sync and async transports to return `Err(UnexpectedEof)` (transport.rs).
- ~~Move memory storage to ~/.codegraph/projects/~~ (f19fc5e) — Project-derived slug `<name>-<4hex>`, auto-migration from workspace-local path, on-demand DB opening.
- ~~Fix get_ai_context to return actual context (#7)~~ (26a41e0) — Rewrote MCP handler, now returns source code, callers/callees, dependencies, usage examples, and architecture layer.
- ~~v0.7.0: 4 new language parsers + MCP reliability~~ (928267a) — PHP, Ruby, Swift, Tcl parsers, graph re-indexing dedup, find_related_tests, find_unused_code scope, get_call_graph dedup, mine_git_history dedup, RocksDB LOG cleanup.
- ~~MCP tool correctness across 9 areas~~ (32abcd1) — index_directory node cleanup, find_related_tests rewrite, find_unused_code path filtering, get_call_graph dedup, mine_git_history dedup, tool descriptions.
- ~~Add 4 languages (#5)~~ — PHP, Ruby, Swift, Tcl added in v0.7.0 (928267a), bringing total to 14.
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
