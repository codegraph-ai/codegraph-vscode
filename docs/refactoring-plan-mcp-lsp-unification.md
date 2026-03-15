# Refactoring Plan: MCP/LSP Handler Unification

> **Status**: Plan
> **Date**: 2026-03-15
> **Scope**: server/src/mcp/server.rs, server/src/handlers/, server/src/backend.rs, server/src/ai_query/

## Problem Statement

The codegraph-lsp binary serves two protocols from the same codebase:

- **LSP mode** (default): CodeGraphBackend + handlers/ modules — used by VS Code extension
- **MCP mode** (--mcp): McpServer + inline handlers in mcp/server.rs (~5200 lines) — used by AI clients

Both paths implement the same domain operations (complexity analysis, unused code detection, caller/callee lookup, AI context assembly, etc.) with **separate code paths**, causing:

1. **Behavioral divergence**: Different defaults, different edge-case handling, different response shapes
2. **Property key mismatches**: LSP uses start_line/end_line, MCP uses line_start/line_end (with fallback)
3. **Duplicated detection logic**: find_unused_code has ~200 lines in MCP with is_mcp_test_node(), is_framework_entry_point(), is_trait_impl_method(), has_called_child_methods(), has_active_same_file_functions() — versus a simpler ~100-line version in LSP handle_find_unused_code
4. **Divergent AI context**: MCP get_ai_context() (~150 lines) reimplements handle_get_ai_context() from handlers/ai_context.rs (~600 lines) with different token budgeting, different related-symbol selection, and different response shapes
5. **Maintenance burden**: Every bug fix or feature must be applied in two places, or the protocols drift further apart

## Current Architecture

```
main.rs
+-- LSP mode --> tower_lsp::Server
|                 +-- CodeGraphBackend (backend.rs)
|                      +-- LanguageServer trait (standard LSP)
|                      +-- handle_custom_request() (custom_requests.rs)
|                      |    +-- handlers/ modules
|                      |         +-- ai_context.rs   (get_ai_context)
|                      |         +-- ai_query.rs     (symbol_search via lm tools)
|                      |         +-- custom.rs       (dep graph, call graph, impact, tests)
|                      |         +-- memory.rs       (memory operations)
|                      |         +-- metrics.rs      (complexity, unused code, coupling)
|                      |         +-- navigation.rs   (node location, workspace symbols)
|                      +-- QueryEngine (ai_query/engine.rs)
|                      +-- graph, parsers, memory_manager, symbol_index
|
+-- MCP mode --> McpServer (mcp/server.rs)
                  +-- McpBackend
                       +-- execute_tool() — 5200-line match block
                       |    +-- 31 tool handlers (inline)
                       |    +-- ~15 private helper methods
                       |    +-- ~10 static utility functions
                       +-- QueryEngine (shared type, separate instance)
                       +-- graph, parsers, memory_manager (no symbol_index)
```

### Key Differences Between Paths

| Aspect | LSP Path | MCP Path |
|--------|----------|----------|
| **Node resolution** | find_nearest_node() via symbol_index + Position | find_nearest_node_with_fallback() via graph.query() + line number |
| **Property keys** | Reads start_line/end_line (some line_start/line_end) | Reads line_start/line_end with fallback to start_line/end_line |
| **includePrivate default** | Not applicable (no filter) | true for symbol_search |
| **find_unused_code** | Simpler: checks Calls + usage edges, basic name filtering | Full: is_mcp_test_node(), framework entries, trait impls, struct heuristics, build output filtering |
| **get_ai_context** | Rich typed structs (AIContextResponse), intent-based context (explain/modify/debug/test), token budget with TokenBudget struct | Ad-hoc JSON building, simplified intent routing, inline budget math |
| **complexity** | Already unified via analyze_file_complexity() shared function | Calls shared function, but formats output differently (snake_case JSON vs camelCase structs) |
| **Source code access** | get_node_source_code() reads from disk via node location | get_symbol_source() reads from disk via node properties |
| **Error handling** | Returns tower_lsp::jsonrpc::Result<T> | Returns Result<Value, String> |

## Proposed Architecture

### Layer 1: Domain Services (transport-agnostic)

Extract all domain logic into a new domain/ module that knows nothing about LSP, MCP, JSON-RPC, or tower-lsp types.

```
server/src/domain/
+-- mod.rs
+-- types.rs         — shared request/response types (no serde annotations yet)
+-- complexity.rs    — analyze_file_complexity (already exists in metrics.rs, move here)
+-- unused_code.rs   — find_unused_code with full detection logic
+-- ai_context.rs    — assemble AI context (unified from both paths)
+-- edit_context.rs  — get_edit_context (move from MCP)
+-- curated_context.rs — get_curated_context (move from MCP)
+-- callers.rs       — get_callers/get_callees with diagnostics
+-- symbol_info.rs   — get_symbol_info, get_detailed_symbol
+-- impact.rs        — analyze_impact
+-- coupling.rs      — analyze_coupling
+-- tests.rs         — find_related_tests
+-- node_resolution.rs — unified find_nearest_node (merge both strategies)
+-- source_code.rs   — get_symbol_source (unified disk reader)
```

Each domain function takes a \&DomainContext (graph + query engine + memory manager + parsers) and typed params, returns typed results:

```rust
// domain/types.rs
pub struct DomainContext {
    pub graph: Arc<RwLock<CodeGraph>>,
    pub query_engine: Arc<QueryEngine>,
    pub memory_manager: Arc<MemoryManager>,
    pub parsers: Arc<ParserRegistry>,
}

// domain/unused_code.rs
pub struct FindUnusedCodeParams {
    pub uri: Option<String>,
    pub scope: String,
    pub include_tests: bool,
    pub confidence: f64,
}

pub struct FindUnusedCodeResult {
    pub unused_items: Vec<UnusedItem>,
    pub summary: UnusedSummary,
}

pub async fn find_unused_code(
    ctx: \&DomainContext,
    params: FindUnusedCodeParams,
) -> Result<FindUnusedCodeResult, DomainError> {
    // One implementation, used by both LSP and MCP
}
```

### Layer 2: Transport Adapters (thin)

Each transport adapter converts wire-format requests to domain types and domain results back to wire-format responses.

**LSP adapter** (handlers/ — refactored to be thin):
```rust
// handlers/metrics.rs
pub async fn handle_find_unused_code(\&self, params: UnusedCodeParams) -> Result<UnusedCodeResponse> {
    let domain_params = FindUnusedCodeParams::from(params);
    let result = domain::find_unused_code(\&self.domain_ctx(), domain_params).await
        .map_err(|e| tower_lsp::jsonrpc::Error::internal_error())?;
    Ok(UnusedCodeResponse::from(result))
}
```

**MCP adapter** (mcp/server.rs — refactored to be thin):
```rust
"codegraph_find_unused_code" => {
    let params = FindUnusedCodeParams {
        uri: args.get("uri").and_then(|v| v.as_str()).map(String::from),
        scope: args.get("scope").and_then(|v| v.as_str()).unwrap_or("file").to_string(),
        include_tests: args.get("includeTests").and_then(|v| v.as_bool()).unwrap_or(false),
        confidence: args.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.7),
    };
    let result = domain::find_unused_code(\&self.domain_ctx(), params).await?;
    Ok(serde_json::to_value(result)?)
}
```

### Layer 3: Shared Infrastructure

Unify the helper functions that are duplicated:

| Current (duplicated) | Unified location |
|---------------------|-----------------|
| McpServer::find_nearest_node_with_fallback() + CodeGraphBackend::find_nearest_node() | domain::node_resolution::find_nearest_node() |
| McpServer::get_symbol_source() + CodeGraphBackend::get_node_source_code() | domain::source_code::get_symbol_source() |
| McpServer::is_mcp_test_node() + LSP inline test checks | domain::unused_code::is_test_node() |
| McpServer::is_framework_entry_point() / is_trait_impl_method() | domain::unused_code:: (promote to shared) |
| McpServer::has_called_child_methods() / has_active_same_file_functions() | domain::unused_code:: (promote) |
| McpServer::get_intent_related_symbols() + CodeGraphBackend::get_explanation_context() etc. | domain::ai_context::get_intent_context() |
| CodeGraphBackend::detect_layer() + McpServer::get_node_architecture() | domain::ai_context::detect_layer() |

## Migration Plan

### Phase 0: Preparatory (no behavior change)

**Goal**: Create the domain/ module structure without changing any behavior.

1. Create server/src/domain/mod.rs with DomainContext struct
2. Move analyze_file_complexity() from handlers/metrics.rs to domain/complexity.rs (it is already shared — this just gives it a proper home)
3. Move complexity helper functions (complexity_grade, file_grade, get_complexity_from_node) alongside it
4. Re-export from handlers/metrics.rs so existing code does not break
5. Move detect_layer() from handlers/ai_context.rs to domain/ai_context.rs

**Files modified**: server/src/lib.rs, new server/src/domain/*.rs, server/src/handlers/metrics.rs (thin wrapper)

**Tests**: All existing tests pass unchanged.

### Phase 1: Node Resolution Unification

**Goal**: One find_nearest_node() implementation used by both paths.

1. Create domain/node_resolution.rs with a unified function that:
   - Accepts either a NodeId directly or (uri, line) pair
   - Queries the graph for file nodes (like MCP does)
   - Falls back to symbol_index if available (like LSP does)
   - Returns (NodeId, bool /* used_fallback */)
   - Handles both line_start/start_line property conventions
2. Update McpServer::find_nearest_node_with_fallback() to delegate
3. Update CodeGraphBackend::find_nearest_node() to delegate
4. Standardize property key reading: create domain::node_props::get_line_start(node) helpers (similar to what handlers/custom.rs already has at line 12-42) and use them everywhere

**Files modified**: new domain/node_resolution.rs, mcp/server.rs, backend.rs, handlers/custom.rs

**Tests**: Add unit tests for the unified resolution function.

### Phase 2: Source Code Access Unification

**Goal**: One get_symbol_source() used by both paths.

1. Create domain/source_code.rs with get_symbol_source(graph, node_id) -> Option<String>
2. Migrate MCP get_symbol_source() (reads from disk via properties)
3. Migrate LSP get_node_source_code() (reads from disk via location)
4. Both paths delegate to the new function

**Files modified**: new domain/source_code.rs, mcp/server.rs, backend.rs

### Phase 3: Unused Code Detection Unification

**Goal**: MCP richer detection logic becomes the single implementation.

The MCP find_unused_code() is strictly better than the LSP version — it handles:
- is_test_node() with path-based detection
- is_framework_entry_point() (main, activate, deactivate, setUp, etc.)
- is_trait_impl_method() (fmt, eq, hash, clone, etc.)
- has_called_child_methods() (struct-used-via-methods heuristic)
- has_active_same_file_functions() (Rust impl sibling heuristic)
- Build output path exclusion

1. Create domain/unused_code.rs with the MCP logic (cleaned up, typed params/results)
2. Move all helper functions (is_test_node, is_framework_entry_point, etc.)
3. LSP handler becomes a thin adapter (convert params, call domain, convert result)
4. MCP handler becomes a thin adapter (extract args from JSON, call domain, serialize result)
5. Delete the old LSP handle_find_unused_code logic and the MCP inline logic

**Files modified**: new domain/unused_code.rs, handlers/metrics.rs, mcp/server.rs

**Behavioral change**: LSP path gains the richer detection (improvement, not regression).

### Phase 4: AI Context Unification

**Goal**: One AI context assembly used by both paths.

This is the most complex phase because the two implementations differ significantly:

| Feature | LSP (ai_context.rs) | MCP (server.rs) |
|---------|---------------------|-----------------|
| Token budgeting | TokenBudget struct | Inline arithmetic |
| Intent routing | 4 async methods (explain/modify/debug/test) | get_intent_related_symbols() synchronous |
| Source code | Via get_node_source_code() | Via get_symbol_source() |
| Architecture | get_architecture_info() with detect_layer() | get_node_architecture() |
| Response | Typed AIContextResponse | Ad-hoc serde_json::Value |

Strategy:
1. Create domain/ai_context.rs with:
   - TokenBudget (promote from LSP)
   - Unified intent-based context assembly (keep LSP richer 4-method approach)
   - AiContextResult struct (transport-neutral, with serde derives)
2. The LSP handler converts AIContextParams to domain params, calls domain, returns typed result
3. The MCP handler extracts args from JSON, calls domain, serializes result

**Files modified**: new domain/ai_context.rs, handlers/ai_context.rs (slim down), mcp/server.rs (remove inline implementation)

### Phase 5: Callers/Callees/Symbol Info Adapters

**Goal**: Thin MCP handlers that just extract params and delegate to QueryEngine.

The MCP handlers for get_callers, get_callees, get_symbol_info, get_detailed_symbol already delegate to QueryEngine — but they add diagnostic info, fallback metadata, and response wrapping inline. This can be standardized:

1. Create domain/callers.rs with a wrapper that adds diagnostics:
   ```rust
   pub struct CallerResult {
       pub callers: Vec<CallInfo>,
       pub symbol_name: String,
       pub diagnostic: Option<Diagnostic>,
       pub fallback: Option<FallbackInfo>,
   }
   ```
2. Both LSP and MCP handlers become thin adapters
3. Similarly for get_symbol_info and get_detailed_symbol

**Files modified**: new domain/callers.rs, domain/symbol_info.rs, mcp/server.rs, handlers

### Phase 6: Remaining Tool Handlers

**Goal**: Complete the migration for all remaining tools.

Tools that already delegate cleanly to QueryEngine (just param extraction + serialization):
- symbol_search — already uses query_engine.symbol_search()
- find_entry_points — already uses query_engine.find_entry_points_opts()
- find_by_imports — already uses query_engine.find_by_imports()
- find_by_signature — already uses query_engine.find_by_signature()
- traverse_graph — already uses query_engine.traverse()
- get_dependency_graph — has LSP handler in custom.rs
- get_call_graph — has LSP handler in custom.rs
- analyze_impact — has LSP handler in custom.rs

For these, the MCP handlers are already essentially adapters over QueryEngine. The main work is:
1. Ensure consistent default values between LSP and MCP
2. Document the canonical defaults in one place
3. Consider moving custom.rs graph-walking handlers (dependency_graph, call_graph, impact) into domain/

Tools with no LSP equivalent (MCP-only):
- get_edit_context — MCP-only, move to domain/edit_context.rs
- get_curated_context — MCP-only, move to domain/curated_context.rs
- cross_project_search — MCP-only, move to domain/cross_project.rs
- memory_* (8 tools) — MCP-only, already delegates to MemoryManager
- mine_git_history* / search_git_history — MCP-only, delegates to GitMiner
- reindex_workspace — both paths have this

### Phase 7: MCP Server Slimming

**Goal**: Reduce mcp/server.rs from ~5200 lines to ~1000 lines.

After phases 1-6, execute_tool() should be a thin match block where each arm:
1. Extracts parameters from args: Value
2. Constructs a domain params struct
3. Calls a domain function
4. Serializes the result

The 31 tool handlers, currently inline with graph walking, diagnostic generation, and response formatting, become ~20-30 lines each.

## Risk Assessment

| Risk | Mitigation |
|------|-----------|
| Behavior regression in LSP path when switching to richer MCP logic | Run full test suite after each phase; keep old handlers as deprecated temporarily |
| Response shape changes break VS Code extension | LSP response types are already typed structs — maintain the same serde output |
| Response shape changes break MCP clients | MCP clients read JSON — ensure same keys. The MCP test suite (181 scenarios) validates this |
| Large diff makes review hard | Phase per PR: each phase is independently testable and mergeable |

## Effort Estimates

| Phase | Complexity | Estimated Files Changed |
|-------|-----------|----------------------|
| Phase 0 | Low | 4-5 (create domain module, move complexity) |
| Phase 1 | Medium | 5-6 (node resolution unification) |
| Phase 2 | Low | 3-4 (source code access) |
| Phase 3 | Medium | 3-4 (unused code — MCP logic is well-tested) |
| Phase 4 | High | 4-5 (AI context — significant design work) |
| Phase 5 | Medium | 4-5 (callers/symbol info) |
| Phase 6 | Low-Medium | 3-4 per tool group |
| Phase 7 | Low | 1-2 (cleanup) |

## Success Criteria

1. **Zero duplication**: Every domain operation has exactly one implementation
2. **Test parity**: MCP test suite (181 scenarios) passes with no regressions
3. **LSP test parity**: All existing LSP tests pass
4. **server.rs < 1500 lines**: Down from ~5200
5. **Consistent behavior**: Both protocols return equivalent results for the same input
6. **Property key standardization**: One canonical set of property key names, no fallback chains

## Appendix: Inventory of Duplicated Functions

### Fully Duplicated (separate implementations in both paths)

| Function | LSP Location | MCP Location | Lines (LSP/MCP) |
|----------|-------------|-------------|-----------------|
| find_nearest_node | backend.rs:521 | mcp/server.rs:2373 | ~70/~70 |
| get_symbol_source | backend.rs (get_node_source_code) | mcp/server.rs:2447 | ~30/~40 |
| find_unused_code | handlers/metrics.rs:441 | mcp/server.rs:4536 | ~100/~200 |
| get_ai_context | handlers/ai_context.rs:149 | mcp/server.rs:2863 | ~600/~150 |
| find_related_tests | handlers/custom.rs:239 | mcp/server.rs:4378 | ~100/~160 |
| get_dependency_graph | handlers/custom.rs:114 | mcp/server.rs:1429 | ~120/~50 |
| get_call_graph | handlers/custom.rs:572 | mcp/server.rs:1481 | ~190/~50 |
| analyze_impact | handlers/custom.rs:766 | mcp/server.rs:1536 | ~210/~60 |
| analyze_coupling | handlers/metrics.rs | mcp/server.rs:1594 | ~100/~30 |

### Already Shared (single implementation, both paths call it)

| Function | Location | Used By |
|----------|----------|---------|
| analyze_file_complexity() | handlers/metrics.rs:293 | LSP handler + MCP handler |
| complexity_grade() / file_grade() | handlers/metrics.rs:224 | Both via analyze_file_complexity |
| QueryEngine::symbol_search() | ai_query/engine.rs | Both |
| QueryEngine::get_callers() / get_callees() | ai_query/engine.rs | Both |
| QueryEngine::get_symbol_info() | ai_query/engine.rs | Both |

### MCP-Only (no LSP equivalent)

| Function | Location |
|----------|----------|
| get_edit_context() | mcp/server.rs:3012 |
| get_curated_context() | mcp/server.rs:3330 |
| cross_project_search() | mcp/server.rs:190 |
| is_framework_entry_point() | mcp/server.rs:5016 |
| is_trait_impl_method() | mcp/server.rs:5080 |
| has_called_child_methods() | mcp/server.rs:4920 |
| has_active_same_file_functions() | mcp/server.rs:4971 |
| generate_test_path_patterns() | mcp/server.rs:4877 |
| is_build_output_path() | mcp/server.rs:4910 |
