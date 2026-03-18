# CodeGraph VS Code — TODO

> Last updated: 2026-03-17 (v0.9.1, 334 tests, 31 MCP tools, 15 languages, unified domain architecture)
>
> See also: [docs/competitive-analysis.md](docs/competitive-analysis.md) | [docs/IDE_ARCHITECTURE.md](docs/IDE_ARCHITECTURE.md)

## Strategic — Competitive Capabilities

### T1-2. Configurable embedding model + semantic symbol search

~~**Phase 1 — Migrate to fastembed + BGE-Small**~~ (12108c1): Done.
~~**Phase 1 gap — Hybrid BM25 + semantic symbol search**~~ (d77d806e): Done.

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

~~**Phase 1 — Route handler + HTTP client detection**~~ (2df7ca2): Done.

**Phase 2 — Route matching + RuntimeCalls edges**: Stub exists in `create_runtime_call_edges()`. Needs URL argument capture from parsers to match `fetch("/api/users")` → `@app.get("/api/users")`. Blocked on parser support for string literal extraction in call arguments.

### T1-4. Cross-repository graph linking

~~**Phase 1 — Shared graph database + project registry**~~ (2df7ca2): Done.
~~**Phase 2 — Cross-project symbol search**~~ (6b6ac29): Done.

**Phase 3 — Cross-project impact analysis**: Extend `analyze_impact` to show consumers in other projects. Match route handlers in one project against HTTP client calls in others to create cross-project RuntimeCalls edges.

### T3-1. Architectural layer detection

**Current**: `get_ai_context` returns a `module` field and `detected_layer` but layer detection is basic (inferred from file path patterns). No concept of service/controller/repository/utility boundaries.

**Target**: Auto-classify modules based on naming conventions, dependency patterns, and framework usage. Enable queries like "show me all database access patterns" and architectural violation detection (UI calling DB directly, circular layer dependencies).

**Effort**: High — needs heuristic classification rules per framework + layer boundary enforcement.

### T3-2. Multi-user shared graph

**Current**: Single-user, fully local. Graph in `~/.codegraph/projects/<slug>/`, memories in same location.

**Target**: Team-wide shared graph + memories. Shared architectural decisions, shared known issues.

**Effort**: High — needs network transport, auth, conflict resolution, selective sharing.

## Strategic — IDE Fork

### T4-1. CodeGraph IDE (Lapce fork)

Fork Lapce v0.4.6 (Apache-2.0) and integrate CodeGraph as a core subsystem with AI chat. See [docs/IDE_ARCHITECTURE.md](docs/IDE_ARCHITECTURE.md) for full architecture.

**Phase 0 — Fork + validate**: Fork Lapce, add `lapce-graph` crate, hook tree-sitter `Syntax::parse()` to feed ASTs to graph engine. Verify parse-once-use-twice works.

**Phase 1 — Graph panels**: Graph Explorer (callers/callees/tests for cursor symbol), Impact Preview (blast radius), dead code dimming, structural search, inline complexity code lens.

**Phase 2 — AI chat with graph context**: Multi-provider AI (Claude API, OpenRouter, Ollama), Context Assembler using graph queries, agent loop with tool system, inline diff for proposed edits.

**Phase 3 — Copilot + inline completion**: GitHub Copilot via language server, graph-enhanced tab completion.

**Phase 4 — Advanced**: Architectural constraint rules, multi-file composer, shadow workspace validation.

---

## Future / On Demand

### 23. Linux binary requires glibc 2.39+ (Ubuntu 24.04+)
Linux binary is built with native `cargo build` on Ubuntu 24.04 (glibc 2.39). Won't work on Ubuntu 22.04 (glibc 2.35). zigbuild can't cross-compile ONNX Runtime C++ code (`std::filesystem` symbols). Accepted tradeoff — Ubuntu 22.04 is EOL April 2027.

### 21. Cross-file `new ClassName()` instantiation detection
Low priority — `new ClassName()` already creates a `Calls` edge via `visit_new_expression`, and `resolve_cross_file_imports` handles cross-file resolution. The only difference is using `Instantiates` instead of `Calls` edge type, which has no practical impact on `find_unused_code`.

### 32. Multi-arch symbol deduplication for C
In multi-platform C codebases (e.g. open-vm-tools), the same function exists in arch-specific files (`backdoorGcc32.c`, `backdoorGcc64.c`, `backdoorGcc64_arm64.c`). The graph creates separate nodes for each variant, but call edges point to only one. Querying callers of the other variant returns 0. Need either: (a) merge arch variants into a single logical symbol with multiple definitions, or (b) cross-link variants so querying any one returns callers of all.

### 33. Platform-aware indexing for C stubs
When all source files are indexed equally, `free()` resolves to the Solaris `kmem_free` stub (`kernelStubsSolaris.c`) and `g_mutex_lock` resolves to empty GLib stubs. Could be addressed by: (a) platform exclude patterns, (b) stub detection heuristic (empty body → lower precedence), or (c) user-configured platform filter.

### 35. Struct-dispatch / vtable caller detection for C
Functions registered in vtable structs are dispatched via struct field access, not called by name. Requires pointer/struct field analysis — fundamental limitation of static call-graph analysis for C callback/vtable patterns.

---

## Completed

- ~~Type reference extraction for Rust and Go (#19)~~ — Both parsers now extract type references from function signatures (parameter types, return types, generic bounds). Creates References edges that prevent find_unused_code false positives.
- ~~Type references in expressions (#20)~~ — TypeScript: variable type annotations (`const x: MyType`) now create References edges. Generic type args and `as` casts were already handled.
- ~~Caller snippet truncation (#34)~~ — Large callers (>30 lines) truncated to function signature + call site ± 5 context lines with `// ... (N lines omitted)` markers.
- ~~Architecture neighbor descriptions (#36)~~ — `architecture.neighbors` now returns `[{module, relationship}]` instead of plain strings. Relationship types: calls, called_by, imports, imported_by.
- ~~File watcher for on-demand indexing (#30)~~ — `handle_index_directory()` now starts or extends the file watcher after indexing, so file changes trigger incremental re-indexing.
- ~~VSIX cleanup (#31)~~ — Added `.vscodeignore` to exclude source, dev files, log.md, duplicate binary. VSIX reduced from 361MB to 63MB.
- ~~MCP/LSP domain unification (Phases 0-9)~~ — Created 16 domain modules (4417 lines) as single source of truth for all tool handlers. Both MCP and LSP call identical domain functions with typed Result structs. mcp/server.rs reduced from ~5200 to ~2851 lines (-45%). handlers/ai_context.rs from 1051 to 231 lines (-78%). AI context improvements: signature-only mode, file-level imports, sibling functions, debug hints.
- ~~Verilog/SystemVerilog parser (15th language)~~ (9689974, efe8ed4) — New codegraph-verilog crate with full SV 1800-2023 support. Extracts modules, functions, tasks, always blocks, classes, interfaces, packages, module instantiations (→calls), imports. File extensions: .v, .vh, .sv, .svh. 49 tests.
- ~~Call graph extraction verified for all 15 languages~~ — All 8 "structure only" languages already had call extraction implemented. Added integration tests for Java, C++, Kotlin, C#, PHP, Ruby, Swift, Tcl. All confirmed working.
- ~~VMK/kernel type preprocessing fix~~ (1831f2d) — tree-sitter silently misparses VMK typedefs without ERROR nodes, so tolerant fallback never triggered. Now detects VMK patterns upfront and preprocesses before strict parse.
- ~~includePrivate default fix~~ (dac8f4e) — VS Code LM symbol_search defaulted includePrivate to false, filtering out static C functions. Fixed to true (matching MCP).
- ~~QueryBuilder node iteration bug~~ — iterated 0..node_count() which missed nodes added after deletions. Fixed to use nodes_iter() over actual HashMap keys.
- ~~MCP roots-based workspace discovery~~ — MCP initialize now accepts client roots for workspace detection. Global MCP config works without --workspace flag.
- ~~Consolidate complexity analysis — single source of truth~~ (dd96a8b, 1d6afba) — Deleted ~130 lines of duplicated `analyze_complexity()` from mcp/server.rs. Extracted shared `analyze_file_complexity()` free function. MCP handler is now a thin JSON adapter. Fixed `QueryBuilder` bug: iterated `0..node_count()` which missed nodes added after deletions (caused cross-file resolution to fail after did_open). Added `nodes_iter()` to CodeGraph. Aligned all complexity property keys to `complexity_` prefix convention across all 14 parsers and server consumers. MCP `initialize` now accepts client `roots` for workspace discovery (global MCP config without per-project `--workspace`). Added ICE driver integration test (84 C files, 3690 functions, 100% parse rate).
- ~~On-demand Index Directory command~~ (e37b5b0) — Added `CodeGraphConfig` with `index_on_startup` (default false), `codegraph.indexDirectory` VS Code command with folder picker, `handle_index_directory()` and `handle_update_configuration()` LSP handlers, safety limits (MAX_INDEX_DEPTH=20, MAX_INDEXED_FILES=5000), exclude glob patterns, file size limits. `did_open` now removes old entries before re-parse and rebuilds query indexes.
- ~~Fix `codegraph_reindex_workspace` null return~~ — Server returned `Ok(None)`/`Ok(Value::Null)` in both `custom_requests.rs` and `backend.rs`, causing toolManager to throw "returned null — server may be busy or restarting". Fixed both handlers to return `{ status, message, files_indexed }`. Reindex now works end-to-end; verified 512 files indexed on workspace with two source folders.
- ~~Adopt PropertyValue::StringList for multi-valued properties (#4)~~ (a79ef71, 6521447) — All 14 language mappers now use native `StringList` instead of `.join(",")` comma-separated strings. Properties migrated: symbols, attributes, annotations, parameters, unresolved_calls, unresolved_type_refs, type_parameters, required_methods. Added `get_string_list_compat()` for backwards-compatible reading. Updated 5 consumers in codegraph-vscode server.
- ~~Fix 4 MCP tool bugs (#25-28)~~ (17e061d) — includeReferences flag (get_symbol_info), idempotent memory_invalidate, similarity threshold (search_git_history), summary mode keys (analyze_impact).
- ~~MCP tool verification suite~~ (890e79a) — 31 tools, 181 scenarios in `test_scenarios.md`. Results: 149/159 pass, 3 fail, 7 warn. Verification skill runs full suite with parallel agents.
- ~~Cross-project symbol search (T1-4 Phase 2)~~ (6b6ac29) — New `codegraph_cross_project_search` MCP tool. Searches symbols across all indexed projects via shared RocksDB.
- ~~Runtime dependency detection + shared graph persistence (T1-3p1, T1-4p1)~~ (2df7ca2) — Route handler detection (Flask/FastAPI/NestJS/Spring), HTTP client detection, shared RocksDB with NamespacedBackend, project registry.
- ~~Docs cleanup~~ — Deleted 7 obsolete docs (246KB): ai_agent_query_architecture, AI_TOOL_EXAMPLES, memory instructions/plan/status, RED_TEAM_TIER2, UPGRADE_RECOMMENDATIONS. Updated competitive-analysis.md with current state (31 tools, fastembed, cross-project, branch-aware).
- ~~Fix find_unused_code false positives (#8)~~ (0143da9) — 158→0 false positives at ≥0.8 confidence. Test helper detection, same-file struct heuristic, expanded trait impl allowlist.
- ~~Hierarchical context curation (T2-1)~~ (f8ee5f1) — New `get_curated_context` MCP tool: symbol search → source → dependency expansion → memories, with priority-based token budget allocation.
- ~~Change-aware automatic context (T2-2)~~ (be4dd36) — New `get_edit_context` MCP tool: file+line → function source + callers + tests + memories + git changes in one call.
- ~~Searchable commit history (T2-3)~~ (f8ee5f1) — New `search_git_history` MCP tool: semantic + keyword + time_range search over git history via existing memories.
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
- ~~Add hybrid BM25 + semantic symbol search (T1-2 Phase 1 gap)~~ (d77d806e) — Reused VectorEngine from MemoryManager in QueryEngine. `build_symbol_vectors()` batch-embeds all symbols. `symbol_search()` now uses 0.4×BM25 + 0.6×semantic hybrid scoring. Zero-keyword-overlap queries return semantically relevant results.
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
