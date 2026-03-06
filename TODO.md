# CodeGraph VS Code — TODO

> Last updated: 2026-03-05 (v0.8.2, 316 tests, T1-1 complete)
>
> See also: [docs/competitive-analysis.md](docs/competitive-analysis.md) for full competitive context.

## High Priority — MCP Tool Fixes

Issues discovered via MCP tool verification (162 scenarios, 78 pass / 30 fail / 54 warn).

### 8. Fix find_unused_code false positives (RC-4)
~~Core issue resolved.~~ False positives reduced from 158 → 50 at ≥0.8 confidence across 4 fixes (8a–8d + cross-file imports). Remaining 50 items are mostly Rust test helpers, VS Code callback methods, and ~19 interfaces only used within their defining file via `as` casts or inline patterns the parser doesn't yet detect.

Remaining false positive categories (not blocking, diminishing returns):
- **Rust test helpers** (~14): `create_test_backend`, `make_commit`, etc. — correctly flagged when `includeTests: false`
- **VS Code callbacks** (~8): `provideFollowups`, `getIcon`, `setup` — framework entry points called by VS Code runtime, not via code. Could add to `is_framework_entry_point` allowlist.
- **Classes instantiated cross-file** (~9): `MemoryTreeProvider`, `SymbolTreeProvider`, etc. — `new ClassName()` not detected as Instantiates edge cross-file
- **Interfaces used only inline** (~19): `*Params` interfaces used via `as` casts or in same-file `RequestType<>` generics without cross-file import

### ~~9. Fix memory filtering: kinds, tags, currentOnly, offset (RC-6)~~
~~Fixed (7273769). Most filters already worked. Remaining gap was `memory_context` missing `currentOnly` parameter — now parsed and passed to SearchConfig. See also #16.~~

### ~~10. Fix traverse_graph edgeTypes and nodeTypes filters~~
~~Fixed (8be0e51). Edge type and node type filters now correctly constrain results. Traverse summary mode also implemented.~~

### ~~11. Fix symbolType filter in symbol_search for class/interface~~
~~Fixed (8be0e51). Type filter mapping now correctly translates MCP parameter values to internal NodeType filtering.~~

### ~~12. Implement summary/compact response modes (RC-2)~~
~~Fixed (8be0e51). Summary and compact modes now produce condensed output for get_dependency_graph, get_call_graph, analyze_coupling, traverse_graph.~~

## Medium Priority

### ~~3. Expose ComplexityMetrics in tool responses~~
~~Fixed (792f40e). MCP `analyze_complexity` now returns full details (branches, loops, logical_operators, nesting_depth, exception_handlers, early_returns, lines_of_code), line_start/line_end per function, overall_grade in summary, and actionable recommendations. At parity with LSP handler.~~

### 4. Use PropertyMap improvements
Adopt `StringList` / `IntList` property variants for multi-valued properties (e.g., storing multiple imported symbols on an edge as a `StringList` instead of a comma-separated string).

### ~~13. Fix result deduplication across tools (RC-8)~~
~~Fixed (6a5a43f). Added deduplication to symbol_search and find_entry_points handlers.~~

### ~~14. Fix TS private method visibility indexing~~
~~Fixed (20ea74a). TypeScript was the only mapper not transferring `func.visibility` to graph node properties. Added `.with("visibility", ...)` to functions, classes, and interfaces in the TS mapper. Integration test verifies private/protected/public.~~

### ~~24. Expose visibility string property in get_symbol_info~~
~~Fixed (5f4752f). MCP `get_symbol_info` now reads the `visibility` string property and exposes it. `is_public` boolean fallback improved to derive from visibility string when booleans absent.~~

### ~~15. Fix memory_invalidate error on nonexistent IDs~~
~~Fixed (6a5a43f). memory_invalidate now returns an error for non-existent IDs.~~

### ~~16. Fix memory_stats byKind serialization~~
~~Fixed (7273769). Added `MemoryKind::discriminant_name()` returning clean `"debug_context"` etc. Applied in all 4 response sites in server.rs + stats() in storage.rs.~~

## Strategic — Competitive Capabilities

Gaps identified via competitive analysis against Augment Code and Cursor. Numbered by priority tier.

### ~~T1-1. Branch-aware graph indexing~~
~~Fixed (c371f1f). Watches `.git/HEAD` for branch switches with 2s debounce, diffs changed files via `git diff --name-status`, and batch re-indexes only what changed. Handles worktrees, detached HEAD, interactive rebase. New module `branch_watcher.rs` (~300 lines), 4 new `GitExecutor` methods, integrated into `CodeGraphBackend::initialized()`.~~

### T1-2. Configurable embedding model + semantic symbol search

~~**Phase 1 — Migrate to fastembed + BGE-Small**~~ (12108c1): Done. Replaced `model2vec` (256d) with `fastembed v4` BGE-Small-EN-v1.5 (384d ONNX). All three projects now share the same embedding stack. Database migration v3→v4 auto-clears old vectors and re-embeds on load. Verified: semantic search works with zero keyword overlap.

**Remaining — Phase 1 gap**: Symbol search (`ai_query/text_index.rs`) is still **BM25-only** with no semantic component. Should add fastembed embedding to the symbol search pipeline for queries like "authentication logic" → `verifyJWT()`.

**Phase 2 — Configurable model + experiment with code-tuned models**:
- Add `embedding_model` setting to codegraph config (enum or string matching fastembed model codes)
- Candidates for power users:
  - `JinaEmbeddingsV2BaseCode` (768d, ~270MB) — **only code-tuned model** in fastembed, trained on code+text pairs
  - `NomicEmbedTextV15` (768d, 8192 context) — long-context, good general quality
  - `GTEBaseENV15` (768d) — strong MTEB scores
  - `BGEBaseENV15` (768d) — step up from Small with same architecture
- Auto-detect dimension from model, no hardcoded `EMBEDDING_DIM`
- Invalidate + rebuild vector index when model changes

**Phase 3 — Add reranking** (optional):
- Retrieve top-50 via BM25+vector, rerank top-10 via cross-encoder
- fastembed doesn't include rerankers in Rust crate yet — may need `ort` directly or wait for fastembed support

**Effort**: Phase 2 is low — config plumbing. Phase 3 is medium — needs cross-encoder model selection.

### T1-3. Runtime dependency detection

**Current**: Pure static analysis via AST. Misses all runtime connections: HTTP calls (`fetch("/api/users")`), gRPC stubs, message queue producers/consumers, database queries.

**Target**: Parse string literals in function call arguments for HTTP routes, gRPC service names, queue topics. Create `RuntimeDependency` edge type linking call site to handler. Within single repo first, cross-repo later (T1-4).

**Starting point**: `extract_endpoints` MCP tool already identifies Express/FastAPI/Django route handlers. Missing piece is the call-site detection — finding `fetch()`, `axios.get()`, `requests.post()` calls and matching their URL argument to known routes.

**Effort**: Medium — regex/heuristic scanning of string arguments in known HTTP client functions.

### T1-4. Cross-repository graph linking

**Current**: Single workspace only (`workspace_folders` in `backend.rs:30`). Can't trace dependencies across repo boundaries.

**Target**: Index multiple repos. Detect REST/gRPC/queue connections between services via runtime dependency detection (T1-3). Enable cross-repo impact analysis ("changing this API endpoint breaks 3 consumers in other repos").

**Depends on**: T1-3 (runtime dependency detection within single repo first).

**Effort**: High — needs multi-workspace coordination, service registry, shared graph instance or graph federation. This is Augment's killer feature and their hardest engineering investment.

### T2-1. Hierarchical context curation

**Current**: `get_ai_context` has simple token budgeting (4000 default, `chars/4` estimate in `ai_context.rs:125`). Includes symbols until budget exhausted with no prioritization beyond intent-specific ordering. Other tools return raw results.

**Target**: Higher-level retrieval pipeline: identify relevant modules → zoom into implementation → walk dependency chain → assemble curated context with token budget awareness. Like Augment's "Infinite Context Window" — broad identification then focused deep-dive.

**Approach**: New MCP tool `get_curated_context` that composes `get_ai_context` + `get_dependency_graph` + `memory_context` + `analyze_impact` into a single response, respecting a configurable token budget (default: 8000). Prioritize by relevance, not discovery order.

**Effort**: Medium — compose existing tools, add smarter token allocation.

### T2-2. Change-aware automatic context

**Current**: Agent must manually call individual tools to build context for an edit.

**Target**: Single endpoint: given a file + line being edited, automatically assemble: function source + all callers + tests + related memories + recent git changes to that function. What Augment does proactively.

**Implementation**: New MCP tool `get_edit_context(file, line)` that internally calls `get_ai_context(intent=modify)` + `memory_context(file)` + git log for the function's line range. Low effort — all data sources exist.

**Effort**: Low — composition of existing tools into one call.

### T2-3. Searchable commit history

**Current**: `mine_git_history` (`git_mining/miner.rs`) extracts memories from commits via pattern matching (BugFix, BreakingChange, etc. at 0.7-0.95 confidence). One-time bootstrapping — not queryable alongside code. Only 6 of 10 commit categories create memories; Test/Docs/Refactor/Other are skipped.

**Target**: Make git history a first-class retrieval source. "What changed authentication last month?" returns relevant commits + affected functions + diff context. Index all commits (not just pattern-matched ones) with embeddings for semantic search.

**Effort**: Medium — extend git mining to maintain a searchable commit index with embeddings. Could store in same RocksDB as memories with a new kind.

### T3-1. Architectural layer detection

**Current**: `get_ai_context` returns a `module` field and `detected_layer` but layer detection is basic (inferred from file path patterns). No concept of service/controller/repository/utility boundaries.

**Target**: Auto-classify modules based on naming conventions, dependency patterns, and framework usage. Enable queries like "show me all database access patterns" and architectural violation detection (UI calling DB directly, circular layer dependencies).

**Effort**: High — needs heuristic classification rules per framework + layer boundary enforcement.

### T3-2. Multi-user shared graph

**Current**: Single-user, fully local. Graph in `~/.codegraph/projects/<slug>/`, memories in same location.

**Target**: Team-wide shared graph + memories. "The intern debugged this same issue last week — here's what they found." Shared architectural decisions, shared known issues.

**Effort**: High — needs network transport, auth, conflict resolution, selective sharing.

---

## Future / On Demand

### 6. Publish to VS Code Marketplace + npm
v0.8.2 VSIX built (`codegraph-0.8.2.vsix`, 70MB, all 4 platform binaries). npm package `@memoryx/codegraph-mcp` ready in `mcp-package/`. Remaining: Azure DevOps PAT refresh (current token expired), then `npx @vscode/vsce publish` and `npm publish --access public`.

### ~~22. Windows: bundle or auto-download onnxruntime.dll~~
~~Fixed (7273769). Added `ensure_ort_dll()` in fastembed_embed.rs that auto-downloads ONNX Runtime v1.20.0 from GitHub releases on first run. Sets `ORT_DYLIB_PATH` before fastembed init. Gated with `#[cfg(target_os = "windows")]`. Verified on Windows — binary starts and runs cleanly.~~

### 23. Linux binary requires glibc 2.39+ (Ubuntu 24.04+)
Linux binary is built with native `cargo build` on Ubuntu 24.04 (glibc 2.39). Won't work on Ubuntu 22.04 (glibc 2.35). zigbuild can't cross-compile ONNX Runtime C++ code (`std::filesystem` symbols). Accepted tradeoff — Ubuntu 22.04 is EOL April 2027.

### 19. Extend type reference extraction to other languages
TypeScript type reference extraction (8c) now works for parameter types, return types, interface fields. Could extend to Rust (trait bounds, generic params, struct field types), Go (interface embedding, struct field types), etc.

### 20. Detect type references in expressions (generic args, `as` casts)
Interfaces like `*Params` used as generic type arguments (`new RequestType<DependencyGraphParams, ...>`) or `as` casts (`params as CallGraphParams`) are not detected by the current type reference extraction, which only scans function parameter/return annotations and interface field types. Needs extraction from `new_expression` type arguments, `as_expression` type targets, and variable type annotations (`const x: MyType`). Would eliminate ~19 remaining interface false positives in find_unused_code.

### 21. Cross-file `new ClassName()` instantiation detection
`new ClassName()` in another file doesn't create an Instantiates edge to the class definition. The mapper only creates Instantiates for same-file `new` expressions. Needs cross-file resolution similar to how `resolve_cross_file_imports` works for Imports edges.

---

## Completed

- ~~Branch-aware graph indexing (T1-1)~~ (c371f1f) — Watches `.git/HEAD` for branch switches (2s debounce), diffs via `git diff --name-status old..new`, batch re-indexes changed files. Handles worktrees, detached HEAD, interactive rebase. New `branch_watcher.rs` module, 4 new `GitExecutor` methods, `pub(crate)` on FileWatcher helpers.
- ~~Fix TS private method visibility indexing (#14)~~ (20ea74a) — TypeScript mapper was the only one not transferring visibility to graph properties. Added `.with("visibility", ...)` for functions, classes, interfaces. Integration test for private/protected/public.
- ~~Expose visibility string property in get_symbol_info (#24)~~ (5f4752f) — MCP handler now reads visibility string and exposes it. `is_public` fallback derives from visibility string when booleans absent.
- ~~Expose ComplexityMetrics in MCP response (#3)~~ (792f40e) — MCP `analyze_complexity` now returns full breakdown (exception_handlers, early_returns, lines_of_code), line range, overall_grade, and recommendations. Parity with LSP handler.
- ~~Fix fastembed cache CWD pollution~~ (fd1dd98) — Wrong env var name (`FASTEMBED_CACHE_PATH` → `FASTEMBED_CACHE_DIR`) caused `.fastembed_cache/` to appear in workspace dir. Fixed in codegraph-memory, tempera, and smelt.
- ~~Fix result deduplication (#13) and memory_invalidate error (#15)~~ (6a5a43f) — Dedup in symbol_search/find_entry_points. memory_invalidate returns error for non-existent IDs.
- ~~Fix traverse_graph filters (#10), symbolType filter (#11), summary/compact modes (#12)~~ (8be0e51) — Edge/node type filters, symbol type mapping, and summary modes all working.
- ~~v0.8.1: fix memory kind serialization (#16), add currentOnly to memory_context (#9), Windows ONNX Runtime auto-download (#22), null LSP response guard~~ (7273769, eee5fbc) — `MemoryKind::discriminant_name()` for clean kind strings in all responses. `memory_context` now accepts `currentOnly` param. Windows auto-downloads `onnxruntime.dll` v1.20.0 on first run. `sendRequestWithRetry` guards against null LSP responses in all 26 vscode.lm tool handlers.
- ~~v0.8.0: MCP npm package, VSIX, version alignment~~ (8acba43) — Created `@memoryx/codegraph-mcp` npm package with Node.js launcher (`codegraph-mcp.js`) and postinstall verification. Aligned all versions to 0.8.0 (workspace Cargo.toml, both crates via `version.workspace = true`, npm package, VS Code extension). Rewrote README as feature presentation. Built `codegraph-0.8.0.vsix` with all 4 platform binaries (70MB).
- ~~Rebuild all platform binaries for 0.8.0~~ — Rebuilt all 4 binaries: darwin-arm64/x64 (local), linux-x64 (native cargo build on WSL2 with rustls TLS), win32-x64 (ort-load-dynamic to avoid CRT mismatch). Key fixes: fastembed uses `hf-hub-rustls-tls` (avoids OpenSSL on Linux), platform-conditional Cargo.toml deps (`ort-download-binaries` on macOS/Linux, `ort-load-dynamic` on Windows).
- ~~Migrate embedding engine from Model2Vec to fastembed BGE-Small-EN-v1.5 (T1-2 Phase 1)~~ (12108c1) — Replaced model2vec (256d static) with fastembed v4 (384d ONNX). Removed model_download.rs, discovery.rs, ureq dependency. Added v3→v4 database migration. All three projects (codegraph, tempera, smelt) now share the same embedding stack. Verified: semantic search works with zero keyword overlap.
- ~~Fix MCP tool name mismatch (#22)~~ (45d284d) — Aligned `mine_git_file` → `mine_git_history_for_file` across server/package.json/toolManager. Added missing `reindex_workspace` to package.json and toolManager. All 27 tools now match across 3 layers.
- ~~Remove @codegraph chat participant~~ (38acce0) — Redundant with 26 language model tools via `#` picker. Removed chatParticipants from package.json, implementation class, tests, and extension.ts references.
- ~~Cross-platform binary builds~~ — Native builds for all 4 platforms: darwin-arm64 (local), darwin-x64 (local cross-compile), linux-x64 (WSL2 192.168.254.107), win32-x64 (Windows 192.168.254.103). VSIX now 70MB with all binaries.
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
- ~~Auto-download Model2Vec embedding model~~ (231fad4) — superseded by fastembed migration (12108c1)
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
