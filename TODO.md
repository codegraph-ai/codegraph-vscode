# CodeGraph VS Code — TODO

> Last updated: 2026-02-28
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

## Strategic — Competitive Capabilities

Gaps identified via competitive analysis against Augment Code and Cursor. Numbered by priority tier.

### T1-1. Branch-aware graph indexing

**Current**: Zero branch awareness. Single `CodeGraph` instance (`Arc<RwLock<CodeGraph>>` in `backend.rs:22`) shared across all branches. File watcher (`watcher.rs`) triggers incremental updates but doesn't detect branch switches. After `git checkout feature-x`, the graph contains stale nodes from `main` until individual files happen to trigger watcher events.

**Target**: Detect branch switches and incrementally re-index changed files.

**Implementation approach** (extends existing incremental update in `watcher.rs:157-203`):
1. **Detect**: Watch `.git/HEAD` (regular file, changes on branch switch) via existing `notify` watcher. Store current branch in `CodeGraphBackend`.
2. **Diff**: On branch change, run `git diff --name-only old-branch..new-branch` to get changed file list.
3. **Update**: For each changed file, remove old nodes + re-index (same as `handle_file_change()`). For deleted files, remove nodes (same as `handle_file_remove()`). Then `resolve_cross_file_imports()` + `build_indexes()`.
4. **Optional**: Cache previous branch's graph snapshot for fast switch-back (serialize `CodeGraph` + `SymbolIndex` state).

**Why this works**: The existing file watcher + incremental update pipeline handles single-file changes in <300ms. A branch switch typically changes 10-100 files — incremental re-index should complete in seconds, far faster than Cursor's 10-minute Merkle polling. Augment has real-time per-user branch switching.

**Considerations**:
- Memory scoping: should memories be branch-specific? Probably not — architectural knowledge transcends branches. But git-mined memories could note the branch.
- Stash/rebase: `.git/HEAD` also changes during rebase. The diff-based approach handles this correctly since it compares actual file state.
- Detached HEAD: `git rev-parse --abbrev-ref HEAD` returns `HEAD` — fall back to full re-index.

**Effort**: Medium — most infrastructure exists. Main work is `.git/HEAD` watching + diff-based batch update.

### T1-2. Migrate to fastembed + configurable embedding model

**Current**: Model2Vec `potion-base-8M` (256d static embeddings, `model_download.rs`). Symbol search (`ai_query/text_index.rs`) is **BM25-only** with no semantic component. Memory search (`codegraph-memory/src/search.rs`) combines BM25(0.3) + Model2Vec(0.5) + graph proximity(0.2) but no reranking step. HNSW index via `instant-distance` crate. Meanwhile, tempera and smelt both use `fastembed v4` with `BGESmallENV15` (384d) — three projects, two embedding stacks.

**Target**: Replace `model2vec` crate with `fastembed` (ONNX Runtime). Aligns all three projects on one embedding stack. Make model choice configurable so users can trade speed vs quality.

**Phase 1 — Migrate to fastembed + BGE-Small (default)**:
- Replace `model2vec` dependency with `fastembed = "4"` in `codegraph-memory/Cargo.toml`
- Default model: `BGESmallENV15` (384d, ~33MB ONNX) — matches tempera/smelt, fast startup
- Remove `model_download.rs` auto-download logic (fastembed handles caching at `~/.codegraph/fastembed_cache/`)
- Update `EMBEDDING_DIM` from 256 → 384, rebuild HNSW index on first run
- Add semantic component to symbol search (currently BM25-only, `text_index.rs`)

**Phase 2 — Configurable model + experiment with code-tuned models**:
- Add `embedding_model` setting to codegraph config (enum or string matching fastembed model codes)
- Candidates for power users:
  - `JinaEmbeddingsV2BaseCode` (768d, ~270MB) — **only code-tuned model** in fastembed, trained on code+text pairs
  - `NomicEmbedTextV15` (768d, 8192 context) — long-context, good general quality
  - `GTEBaseENV15` (768d) — strong MTEB scores
  - `BGEBaseENV15` (768d) — step up from Small with same architecture
- Auto-detect dimension from model, no hardcoded `EMBEDDING_DIM`
- Invalidate + rebuild vector index when model changes

**Phase 3 — Add reranking** (optional, after model migration):
- Retrieve top-50 via BM25+vector, rerank top-10 via cross-encoder
- fastembed doesn't include rerankers in Rust crate yet — may need `ort` directly or wait for fastembed support

**Impact**: A query for "authentication logic" must reliably find `verifyJWT()` even without keyword overlap. Even BGE-Small (384d ONNX) is a substantial upgrade over Model2Vec (256d static lookup) for retrieval quality.

**Effort**: Phase 1 is low-medium — straightforward dependency swap, tempera/smelt provide reference implementation. Phase 2 is low — config plumbing. Phase 3 is medium — needs cross-encoder model selection.

### T1-3. Runtime dependency detection

**Current**: Pure static analysis via AST. Misses all runtime connections: HTTP calls (`fetch("/api/users")`), gRPC stubs, message queue producers/consumers, database queries.

**Target**: Parse string literals in function call arguments for HTTP routes, gRPC service names, queue topics. Create `RuntimeDependency` edge type linking call site to handler. Within single repo first, cross-repo later (T1-4).

**Starting point**: `extract_endpoints` MCP tool already identifies Express/FastAPI/Django route handlers. Missing piece is the call-site detection — finding `fetch()`, `axios.get()`, `requests.post()` calls and matching their URL argument to known routes.

**Effort**: Medium — regex/heuristic scanning of string arguments in known HTTP client functions. Could ship Express route matching in v0.8.

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

### 6. Publish to VS Code Marketplace
Currently at v0.7.1 locally. Cross-platform binaries built (darwin-arm64, darwin-x64, linux-x64, win32-x64). Requires marketplace publisher setup, CI/CD pipeline for packaging, and automated VSIX builds.

### 19. Extend type reference extraction to other languages
TypeScript type reference extraction (8c) now works for parameter types, return types, interface fields. Could extend to Rust (trait bounds, generic params, struct field types), Go (interface embedding, struct field types), etc.

### 20. Detect type references in expressions (generic args, `as` casts)
Interfaces like `*Params` used as generic type arguments (`new RequestType<DependencyGraphParams, ...>`) or `as` casts (`params as CallGraphParams`) are not detected by the current type reference extraction, which only scans function parameter/return annotations and interface field types. Needs extraction from `new_expression` type arguments, `as_expression` type targets, and variable type annotations (`const x: MyType`). Would eliminate ~19 remaining interface false positives in find_unused_code.

### 21. Cross-file `new ClassName()` instantiation detection
`new ClassName()` in another file doesn't create an Instantiates edge to the class definition. The mapper only creates Instantiates for same-file `new` expressions. Needs cross-file resolution similar to how `resolve_cross_file_imports` works for Imports edges.

---

## Completed

- ~~Fix MCP tool name mismatch (#22)~~ (45d284d) — Aligned `mine_git_file` → `mine_git_history_for_file` across server/package.json/toolManager. Added missing `reindex_workspace` to package.json and toolManager. All 27 tools now match across 3 layers.
- ~~Remove @codegraph chat participant~~ (38acce0) — Redundant with 26 language model tools via `#` picker. Removed chatParticipants from package.json, implementation class, tests, and extension.ts references.
- ~~Cross-platform binary builds~~ — Native builds for all 4 platforms: darwin-arm64 (local), darwin-x64 (local cross-compile), linux-x64 (WSL2 192.168.254.107), win32-x64 (Windows 192.168.254.103). VSIX now 50MB with all binaries.
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
