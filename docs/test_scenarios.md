# CodeGraph Tool Test Scenarios

Comprehensive test plan covering all 31 MCP tools and their LSP equivalents.
Each scenario specifies the MCP call, the equivalent LSP call, expected behavior, and edge cases.

**Test projects**:
- codegraph-vscode itself (~85 indexed files, Rust + TypeScript)
- open-vm-tools (~800+ C/C++ files) — large C codebase stress test
- aws-mainframe-modernization-carddemo (~80 COBOL files) — enterprise language test
- Fortran-Astrodynamics-Toolkit (~63 Fortran files) — scientific computing test

**Prerequisites**:
- MCP server running (indexes lazily on first tool call)
- LSP server running in VS Code with workspace loaded
- Embedding model: Jina Code V2 (768d, downloaded automatically on first run ~642MB)

---

## 1. codegraph_get_dependency_graph

**LSP**: `codegraph.getDependencyGraph`

### 1.1 Basic — single file, default options
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs" }
LSP: { "uri": "file:///path/to/server/src/mcp/server.rs" }
```
**Expected**: Returns nodes (CodeFile, Module, Function) and edges (depends_on). Should include imports like `codegraph::CodeGraph`, `serde_json::Value`, `super::protocol::*`.

### 1.2 With depth=1
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "depth": 1 }
LSP: { "uri": "file:///path/to/server/src/mcp/server.rs", "depth": 1 }
```
**Expected**: Only direct dependencies, no transitive. Fewer nodes/edges than default depth=3.

### 1.3 With depth=10 (deep traversal)
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "depth": 10 }
```
**Expected**: Full transitive dependency tree. Should not hang or OOM.

### 1.4 Direction — imports only
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "direction": "imports" }
```
**Expected**: Only outgoing dependencies (what this file uses). No incoming edges.

### 1.5 Direction — importedBy only
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "direction": "importedBy" }
```
**Expected**: Only files that depend on server.rs. No outgoing edges.

### 1.6 Direction — both (default)
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "direction": "both" }
```
**Expected**: Both imports and importedBy edges.

### 1.7 Include external dependencies
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "includeExternal": true }
```
**Expected**: Includes nodes for external crates (tokio, serde_json, tower_lsp).

### 1.8 Summary mode
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "summary": true }
```
**Expected**: Condensed output with counts rather than full node/edge lists.

### 1.9 Invalid URI
```json
MCP: { "uri": "file:///nonexistent/file.rs" }
```
**Expected**: Empty graph or error message, not a crash.

### 1.10 TypeScript file
```json
MCP: { "uri": "file:///path/to/src/toolManager.ts" }
```
**Expected**: Shows TS import relationships (vscode, ./lspClient, etc.).

---

## 2. codegraph_get_call_graph

**LSP**: `codegraph.getCallGraph`

### 2.1 Basic — function with known callers and callees
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "line": 597 }
LSP: { "uri": "file:///path/to/server/src/mcp/server.rs", "position": { "line": 597, "character": 0 } }
```
**Expected**: `handle_request` with callers (run) and callees (handle_initialize, handle_tools_call, etc.).

### 2.2 Direction — callers only
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "line": 597, "direction": "callers" }
```
**Expected**: Only upstream callers of handle_request.

### 2.3 Direction — callees only
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "line": 597, "direction": "callees" }
```
**Expected**: Only functions called by handle_request.

### 2.4 Depth variation
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "line": 597, "depth": 1 }
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "line": 597, "depth": 5 }
```
**Expected**: depth=1 shows only direct callers/callees. depth=5 shows deeper chains.

### 2.5 Summary mode
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "line": 597, "summary": true }
```
**Expected**: Condensed summary with counts.

### 2.6 Line 0 (file-level node)
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "line": 0 }
```
**Expected**: Resolves to file-level node. Should use nearest-symbol fallback.

### 2.7 Line pointing to non-function (import statement)
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "line": 5 }
```
**Expected**: Falls back to nearest function symbol.

### 2.8 Character position specified
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "line": 597, "character": 10 }
```
**Expected**: Same as line-only, character refines position within line.

---

## 3. codegraph_analyze_impact

**LSP**: `codegraph.analyzeImpact`

### 3.1 Modify impact
```json
MCP: { "uri": "file:///path/to/server/src/ai_query/engine.rs", "line": 175, "changeType": "modify" }
LSP: { "uri": "file:///path/to/server/src/ai_query/engine.rs", "position": { "line": 175, "character": 0 }, "analysis_type": "modify" }
```
**Expected**: Shows direct/indirect impacts, risk score. `get_callees` is called from multiple places.

### 3.2 Delete impact
```json
MCP: { "uri": "file:///path/to/server/src/ai_query/engine.rs", "line": 175, "changeType": "delete" }
```
**Expected**: Higher risk score than modify. Lists all callers that would break.

### 3.3 Rename impact
```json
MCP: { "uri": "file:///path/to/server/src/ai_query/engine.rs", "line": 175, "changeType": "rename" }
```
**Expected**: Shows all reference sites that need updating.

### 3.4 Summary mode
```json
MCP: { "uri": "file:///path/to/server/src/ai_query/engine.rs", "line": 175, "summary": true }
```
**Expected**: Condensed impact summary.

### 3.5 Low-impact function (leaf node, no callers)
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "line": 615, "changeType": "modify" }
```
**Expected**: Low risk, few or no impacts (is_mcp_test_node is a private helper).

---

## 4. codegraph_get_ai_context

**LSP**: `codegraph.getAIContext`

### 4.1 Explain intent (default)
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "line": 597, "intent": "explain" }
LSP: { "uri": "file:///path/to/server/src/mcp/server.rs", "position": { "line": 597, "character": 0 }, "intent": "explain" }
```
**Expected**: Returns primary context (symbol info, source), related symbols, dependencies. Optimized for understanding.

### 4.2 Modify intent
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "line": 597, "intent": "modify" }
```
**Expected**: Includes callers/callees that would be affected by changes.

### 4.3 Debug intent
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "line": 597, "intent": "debug" }
```
**Expected**: Includes execution flow context, related error patterns.

### 4.4 Test intent
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "line": 597, "intent": "test" }
```
**Expected**: Includes test-relevant context (related tests, dependencies to mock).

### 4.5 Token budget — small
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "line": 597, "maxTokens": 500 }
```
**Expected**: Truncated context fitting within 500 token budget.

### 4.6 Token budget — large
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "line": 597, "maxTokens": 10000 }
```
**Expected**: More comprehensive context with full source code and more related symbols.

---

## 5. codegraph_get_edit_context

**LSP**: N/A (MCP-only tool)

### 5.1 Basic — function at line
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "line": 597 }
```
**Expected**: Returns function source + callers + tests + memories + recent git changes for the function at line 597.

### 5.2 Different file
```json
MCP: { "uri": "file:///path/to/server/src/ai_query/engine.rs", "line": 175 }
```
**Expected**: Edit context for get_callees — source, callers from server.rs, related tests.

### 5.3 Without line (file-level)
```json
MCP: { "uri": "file:///path/to/server/src/mcp/tools.rs" }
```
**Expected**: File-level edit context with module overview.

### 5.4 Token budget
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "line": 597, "maxTokens": 500 }
```
**Expected**: Truncated context fitting within 500 token budget.

---

## 6. codegraph_get_curated_context

**LSP**: N/A (MCP-only tool)

### 6.1 Basic query
```json
MCP: { "query": "memory persistence" }
```
**Expected**: Curated context with symbol matches, source code, callers, dependency info, and memories. Token-budgeted sections (40% symbols, 25% callers, 15% memories, 10% deps, 10% meta).

### 6.2 With token budget
```json
MCP: { "query": "MCP server request handling", "maxTokens": 2000 }
```
**Expected**: Comprehensive curated context within 2000 token budget.

### 6.3 Small token budget
```json
MCP: { "query": "graph traversal", "maxTokens": 500 }
```
**Expected**: Minimal but useful context within tight budget.

### 6.4 No matches
```json
MCP: { "query": "xyznonexistentquery123" }
```
**Expected**: Empty or minimal context, no crash.

---

## 7. codegraph_find_related_tests

**LSP**: `codegraph.findRelatedTests`

### 7.1 Rust file with tests in same file
```json
MCP: { "uri": "file:///path/to/server/src/handlers/custom.rs", "line": 360 }
LSP: { "uri": "file:///path/to/server/src/handlers/custom.rs", "position": { "line": 360, "character": 0 } }
```
**Expected**: Finds test_is_test_node_by_name, test_is_test_node_by_path, etc. (same_file relationship).

### 7.2 Rust file with cross-file test callers
```json
MCP: { "uri": "file:///path/to/server/src/ai_query/engine.rs", "line": 175 }
```
**Expected**: Finds tests that call get_callees (calls_target relationship) or same-file tests.

### 7.3 TypeScript file
```json
MCP: { "uri": "file:///path/to/src/toolManager.ts", "line": 50 }
```
**Expected**: Finds .test.ts or .spec.ts companions if they exist.

### 7.4 Limit parameter
```json
MCP: { "uri": "file:///path/to/server/src/handlers/custom.rs", "line": 360, "limit": 3 }
```
**Expected**: Returns at most 3 tests.

### 7.5 File with no tests
```json
MCP: { "uri": "file:///path/to/src/extension.ts", "line": 10 }
```
**Expected**: Empty test list or minimal same-file results.

---

## 8. codegraph_get_symbol_info

**LSP**: `codegraph.getSymbolInfo` (uses native LSP providers)

### 8.1 Function symbol
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "line": 597 }
```
**Expected**: Returns name, kind (Function), signature, docstring, is_public, location, reference_count.

### 8.2 Class/struct symbol
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "line": 158 }
```
**Expected**: Returns McpServer class info.

### 8.3 With references
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "line": 597, "includeReferences": true }
```
**Expected**: Includes all reference locations across codebase.

### 8.4 Without references (default)
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "line": 597, "includeReferences": false }
```
**Expected**: No references array, faster response.

### 8.5 Character position for overloaded line
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "line": 597, "character": 20 }
```
**Expected**: May resolve to different symbol at specific column.

---

## 9. codegraph_analyze_complexity

**LSP**: `codegraph.analyzeComplexity`

### 9.1 Full file analysis
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs" }
LSP: { "uri": "file:///path/to/server/src/mcp/server.rs" }
```
**Expected**: Complexity scores for all functions in the file. execute_tool should have high complexity.

### 9.2 Specific function
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "line": 603 }
```
**Expected**: Complexity score for execute_tool function only.

### 9.3 Custom threshold
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "threshold": 5 }
```
**Expected**: More functions flagged (lower threshold).

```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "threshold": 30 }
```
**Expected**: Fewer or no functions flagged (higher threshold).

### 9.4 Summary mode
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "summary": true }
```
**Expected**: Condensed complexity summary.

### 9.5 Simple file (low complexity)
```json
MCP: { "uri": "file:///path/to/server/src/mcp/tools.rs" }
```
**Expected**: Low complexity scores, no flagged functions at default threshold.

---

## 10. codegraph_find_unused_code

**LSP**: `codegraph.findUnusedCode`

### 10.1 File scope — active TS file
```json
MCP: { "uri": "file:///path/to/src/toolManager.ts", "scope": "file" }
LSP: { "uri": "file:///path/to/src/toolManager.ts", "scope": "file" }
```
**Expected**: 0 or very few unused items (arrow functions and constructors filtered out).

### 10.2 File scope — Rust file
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "scope": "file" }
```
**Expected**: Few unused items. `new` and `run` may show as unused (entry points).

### 10.3 Workspace scope
```json
MCP: { "scope": "workspace" }
```
**Expected**: Reasonable number of unused items (not 365 false positives). Cross-file call resolution should eliminate most false positives.

### 10.4 Include tests
```json
MCP: { "scope": "workspace", "includeTests": true }
```
**Expected**: Test functions included in analysis. May have more items.

### 10.5 Exclude tests (default)
```json
MCP: { "scope": "workspace", "includeTests": false }
```
**Expected**: Test functions (test_*, *_test, describe, it) excluded.

### 10.6 High confidence threshold
```json
MCP: { "scope": "workspace", "confidence": 0.9 }
```
**Expected**: Only high-confidence unused items. Exported functions excluded.

### 10.7 Low confidence threshold
```json
MCP: { "scope": "workspace", "confidence": 0.3 }
```
**Expected**: More items including exported-but-uncalled and possible entry points.

### 10.8 No URI with file scope
```json
MCP: { "scope": "file" }
```
**Expected**: Empty result or error (file scope requires URI).

---

## 11. codegraph_analyze_coupling

**LSP**: `codegraph.analyzeCoupling`

### 11.1 Highly coupled file
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs" }
LSP: { "uri": "file:///path/to/server/src/mcp/server.rs" }
```
**Expected**: High coupling metrics (server.rs imports many modules).

### 11.2 Include external
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "includeExternal": true }
```
**Expected**: External dependencies (tokio, serde_json, codegraph) included in coupling analysis.

### 11.3 Depth variation
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "depth": 1 }
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "depth": 5 }
```
**Expected**: Deeper analysis reveals more coupling paths.

### 11.4 Summary mode
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "summary": true }
```
**Expected**: Condensed coupling summary.

### 11.5 Low coupling file
```json
MCP: { "uri": "file:///path/to/server/src/mcp/tools.rs" }
```
**Expected**: Lower coupling metrics.

---

## 12. codegraph_symbol_search

**LSP**: `codegraph.symbolSearch`

### 12.1 Exact name search
```json
MCP: { "query": "handle_request" }
LSP: { "query": "handle_request" }
```
**Expected**: Finds handle_request in server.rs. Should rank exact matches highest.

### 12.2 Partial name search
```json
MCP: { "query": "handle" }
```
**Expected**: Multiple matches: handle_request, handle_initialize, handle_tools_call, etc.

### 12.3 Filter by symbol type — function
```json
MCP: { "query": "handle", "symbolType": "function" }
```
**Expected**: Only function symbols, no classes/modules.

### 12.4 Filter by symbol type — class
```json
MCP: { "query": "McpServer", "symbolType": "class" }
```
**Expected**: Only class/struct results.

### 12.5 Filter by symbol type — interface
```json
MCP: { "query": "Backend", "symbolType": "interface" }
```
**Expected**: Only interface/trait results.

### 12.6 Limit results
```json
MCP: { "query": "get", "limit": 5 }
```
**Expected**: At most 5 results.

### 12.7 Exclude private symbols
```json
MCP: { "query": "handle", "includePrivate": false }
```
**Expected**: Only public/exported symbols.

### 12.8 Compact mode
```json
MCP: { "query": "handle", "compact": true }
```
**Expected**: Minimal info (name, kind, location) — no signatures or docstrings.

### 12.9 Non-compact mode (default)
```json
MCP: { "query": "handle", "compact": false }
```
**Expected**: Full info including signatures and docstrings.

### 12.10 No results
```json
MCP: { "query": "xyznonexistent" }
```
**Expected**: Empty results array, no crash.

---

## 13. codegraph_find_by_imports

**LSP**: `codegraph.findByImports`

### 13.1 Contains match (default)
```json
MCP: { "moduleName": "codegraph" }
LSP: { "libraries": ["codegraph"] }
```
**Expected**: All files importing anything from codegraph crate.

### 13.2 Exact match
```json
MCP: { "moduleName": "codegraph::CodeGraph", "matchMode": "exact" }
```
**Expected**: Only files importing exactly `codegraph::CodeGraph`.

### 13.3 Prefix match
```json
MCP: { "moduleName": "super::", "matchMode": "prefix" }
```
**Expected**: Files with imports starting with `super::`.

### 13.4 Fuzzy match
```json
MCP: { "moduleName": "query", "matchMode": "fuzzy" }
```
**Expected**: Files importing anything matching "query" loosely.

### 13.5 Limit results
```json
MCP: { "moduleName": "codegraph", "limit": 5 }
```
**Expected**: At most 5 results.

### 13.6 Module not found
```json
MCP: { "moduleName": "nonexistent_module_xyz" }
```
**Expected**: Empty results.

### 13.7 TypeScript module
```json
MCP: { "moduleName": "vscode" }
```
**Expected**: TS files importing from vscode namespace.

---

## 14. codegraph_find_entry_points

**LSP**: `codegraph.findEntryPoints`

### 14.1 Default — architectural entry points only
```json
MCP: {}
LSP: {}
```
**Expected**: Main functions, event handlers, CLI commands. No tests, no PublicApi (too noisy). Default limit 50.

### 14.2 All entry types (explicit)
```json
MCP: { "entryType": "all" }
```
**Expected**: Mix of main functions, test entries, HTTP handlers, event handlers, CLI commands, PublicApi. Limit 50.

### 14.3 Filter — main only
```json
MCP: { "entryType": "main" }
```
**Expected**: Only main/entry point functions (main.rs main, extension.ts activate).

### 14.4 Filter — test only
```json
MCP: { "entryType": "test" }
```
**Expected**: All test functions (test_*, #[test]). Should not appear in default results.

### 14.5 Filter — http_handler
```json
MCP: { "entryType": "http_handler" }
```
**Expected**: HTTP/request handler functions (may be empty for this project).

### 14.6 Filter — event_handler
```json
MCP: { "entryType": "event_handler" }
```
**Expected**: Event handler functions (handle_*, on_*).

### 14.7 Framework filter
```json
MCP: { "entryType": "all", "framework": "actix" }
```
**Expected**: Empty (this project doesn't use actix).

### 14.8 Compact mode
```json
MCP: { "entryType": "test", "compact": true }
```
**Expected**: Minimal test entry info.

### 14.9 Limit
```json
MCP: { "entryType": "all", "limit": 5 }
```
**Expected**: At most 5 entry points.

### 14.10 Response size — stress test
```json
MCP: { "entryType": "all", "limit": 1000 }
```
**Expected**: Should not exceed ~100KB response. Previously returned 482KB on test-heavy repos without limit.

---

## 15. codegraph_traverse_graph

**LSP**: `codegraph.traverseGraph`

### 15.1 Outgoing traversal from function (using uri+line)
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "line": 597, "direction": "outgoing" }
LSP: { "uri": "file:///path/to/server/src/mcp/server.rs", "line": 597, "direction": "outgoing" }
```
**Expected**: Functions and modules reachable from handle_request.

### 15.2 Incoming traversal
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "line": 597, "direction": "incoming" }
```
**Expected**: Functions that reach handle_request.

### 15.3 Both directions
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "line": 597, "direction": "both" }
```
**Expected**: Full neighborhood.

### 15.4 Using startNodeId
```json
MCP: { "startNodeId": "597", "direction": "outgoing" }
```
**Expected**: Same as uri+line if node 597 is handle_request.

### 15.5 Filter by edge types
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "line": 597, "edgeTypes": ["calls"] }
```
**Expected**: Only call relationships, no import edges.

```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "line": 597, "edgeTypes": ["imports"] }
```
**Expected**: Only import relationships.

### 15.6 Filter by node types
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "line": 0, "nodeTypes": ["Function"] }
```
**Expected**: Only function nodes in traversal results.

### 15.7 Max depth
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "line": 597, "maxDepth": 1 }
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "line": 597, "maxDepth": 5 }
```
**Expected**: depth=1 direct neighbors only. depth=5 deeper graph exploration.

### 15.8 Limit
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "line": 597, "limit": 10 }
```
**Expected**: At most 10 traversal nodes.

---

## 16. codegraph_find_by_signature

**LSP**: `codegraph.findBySignature`

### 16.1 Name pattern with wildcard
```json
MCP: { "namePattern": "handle*" }
LSP: { "name_pattern": "handle*" }
```
**Expected**: All functions starting with "handle".

### 16.2 Name pattern suffix
```json
MCP: { "namePattern": "*Handler" }
```
**Expected**: Functions ending with "Handler".

### 16.3 Exact param count
```json
MCP: { "paramCount": 0 }
```
**Expected**: Functions taking no parameters.

### 16.4 Min/max params
```json
MCP: { "minParams": 3, "maxParams": 5 }
```
**Expected**: Functions with 3-5 parameters.

### 16.5 Return type filter
```json
MCP: { "returnType": "Result" }
```
**Expected**: Functions returning Result types.

### 16.6 Modifier filter — async
```json
MCP: { "modifiers": ["async"] }
```
**Expected**: Only async functions.

### 16.7 Modifier filter — public
```json
MCP: { "modifiers": ["public"] }
```
**Expected**: Only public functions.

### 16.8 Combined filters
```json
MCP: { "namePattern": "handle*", "modifiers": ["async"], "minParams": 1 }
```
**Expected**: Async functions starting with "handle" that take at least 1 param.

### 16.9 Limit
```json
MCP: { "namePattern": "*", "limit": 5 }
```
**Expected**: At most 5 results.

### 16.10 No matches
```json
MCP: { "namePattern": "zzz*", "returnType": "NeverExistsType" }
```
**Expected**: Empty results.

---

## 17. codegraph_get_callers

**LSP**: `codegraph.getCallers`

### 17.1 Using uri+line
```json
MCP: { "uri": "file:///path/to/server/src/ai_query/engine.rs", "line": 175 }
LSP: { "uri": "file:///path/to/server/src/ai_query/engine.rs", "line": 175 }
```
**Expected**: Multiple callers of get_callees from server.rs, ai_query.rs.

### 17.2 Using nodeId
```json
MCP: { "nodeId": "175" }
```
**Expected**: Same result as uri+line for the same node.

### 17.3 Depth=1 (default)
```json
MCP: { "nodeId": "175", "depth": 1 }
```
**Expected**: Direct callers only.

### 17.4 Depth=3 (transitive callers)
```json
MCP: { "nodeId": "175", "depth": 3 }
```
**Expected**: Direct callers AND their callers up to 3 levels.

### 17.5 Function with no callers
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "line": 596 }
```
**Expected**: 0 callers for `run` (it's an entry point).

### 17.6 Cross-file callers (Rust)
```json
MCP: { "nodeId": "175" }
```
**Expected**: Callers from different files (server.rs calling engine.rs function).

### 17.7 Diagnostic output when no callers
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "line": 596 }
```
**Expected**: Response includes diagnostic with node_found, symbol_name, total_edges_in_graph.

---

## 18. codegraph_get_callees

**LSP**: `codegraph.getCallees`

### 18.1 Using uri+line
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "line": 597 }
LSP: { "uri": "file:///path/to/server/src/mcp/server.rs", "line": 597 }
```
**Expected**: Functions called by handle_request.

### 18.2 Using nodeId
```json
MCP: { "nodeId": "597" }
```
**Expected**: Same as above.

### 18.3 Depth=1 (default)
```json
MCP: { "nodeId": "597", "depth": 1 }
```
**Expected**: Direct callees only.

### 18.4 Depth=3 (transitive callees)
```json
MCP: { "nodeId": "597", "depth": 3 }
```
**Expected**: Full call chain up to 3 levels deep.

### 18.5 Leaf function (no callees)
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "line": 615 }
```
**Expected**: 0 callees for is_mcp_test_node (pure logic, no function calls).

---

## 19. codegraph_get_detailed_symbol

**LSP**: `codegraph.getDetailedSymbolInfo`

### 19.1 Full details (defaults)
```json
MCP: { "uri": "file:///path/to/server/src/ai_query/engine.rs", "line": 175 }
LSP: { "uri": "file:///path/to/server/src/ai_query/engine.rs", "line": 175, "include_source": true, "include_callers": true, "include_callees": true }
```
**Expected**: Symbol info + source code + callers list + callees list.

### 19.2 Using nodeId
```json
MCP: { "nodeId": "175" }
```
**Expected**: Same as uri+line.

### 19.3 Without source
```json
MCP: { "nodeId": "175", "includeSource": false }
```
**Expected**: No source code in response. Faster.

### 19.4 Without callers
```json
MCP: { "nodeId": "175", "includeCallers": false }
```
**Expected**: No callers list.

### 19.5 Without callees
```json
MCP: { "nodeId": "175", "includeCallees": false }
```
**Expected**: No callees list.

### 19.6 Minimal — no source, no callers, no callees
```json
MCP: { "nodeId": "175", "includeSource": false, "includeCallers": false, "includeCallees": false }
```
**Expected**: Only symbol metadata (name, kind, location, signature, complexity).

### 19.7 Class/struct symbol
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "line": 158 }
```
**Expected**: Class info for McpServer with contained methods.

---

## 20. codegraph_memory_store

**LSP**: `codegraph.memoryStore`

### 20.1 Debug context
```json
MCP: {
  "kind": "debug_context",
  "title": "Test debug memory",
  "content": "Found that cross-file resolution was missing",
  "problem": "MCP tools returned 0 callers for cross-file calls",
  "solution": "Added resolve_cross_file_imports call to index_workspace"
}
```
**Expected**: Memory stored with ID returned. Kind-specific fields (problem/solution) populated.

### 20.2 Architectural decision
```json
MCP: {
  "kind": "architectural_decision",
  "title": "Test architecture memory",
  "content": "Use query engine indexes for caller/callee lookups",
  "decision": "Pre-build caller/callee indexes from Calls edges",
  "rationale": "Graph traversal per-query is too slow for MCP tool responses"
}
```
**Expected**: Memory stored with decision/rationale fields.

### 20.3 Known issue
```json
MCP: {
  "kind": "known_issue",
  "title": "Test known issue",
  "content": "find_unused_code has overcorrection",
  "description": "Contains edge check marks all functions as used",
  "severity": "medium"
}
```
**Expected**: Memory stored with severity level.

### 20.4 Convention
```json
MCP: {
  "kind": "convention",
  "title": "Test convention",
  "content": "Use conventional commits",
  "description": "All commit messages should follow conventional commit format"
}
```
**Expected**: Memory stored.

### 20.5 Project context
```json
MCP: {
  "kind": "project_context",
  "title": "Test project context",
  "content": "This is a VS Code extension with Rust LSP server"
}
```
**Expected**: Memory stored.

### 20.6 With tags
```json
MCP: {
  "kind": "convention",
  "title": "Rust error handling",
  "content": "Use anyhow::Result in binaries",
  "tags": ["rust", "error-handling", "convention"]
}
```
**Expected**: Memory stored with searchable tags.

### 20.7 With confidence
```json
MCP: {
  "kind": "debug_context",
  "title": "Uncertain fix",
  "content": "May need further investigation",
  "confidence": 0.5
}
```
**Expected**: Memory stored with confidence=0.5.

---

## 21. codegraph_memory_search

**LSP**: `codegraph.memorySearch`

### 21.1 Basic text search
```json
MCP: { "query": "cross-file resolution" }
LSP: { "query": "cross-file resolution" }
```
**Expected**: Returns memories matching the query with relevance scores.

### 21.2 Filter by tags
```json
MCP: { "query": "error", "tags": ["rust"] }
```
**Expected**: Only memories tagged with "rust".

### 21.3 Filter by kinds
```json
MCP: { "query": "resolution", "kinds": ["debug_context"] }
```
**Expected**: Only debug_context memories.

### 21.4 Limit results
```json
MCP: { "query": "test", "limit": 3 }
```
**Expected**: At most 3 results.

### 21.5 Include invalidated
```json
MCP: { "query": "test", "currentOnly": false }
```
**Expected**: Includes invalidated memories.

### 21.6 Code context boosting
```json
MCP: { "query": "callers", "codeContext": ["175"] }
```
**Expected**: Results boosted by proximity to node 175.

### 21.7 No matches
```json
MCP: { "query": "xyznonexistentquery123" }
```
**Expected**: Empty results.

---

## 22. codegraph_memory_get

**LSP**: `codegraph.memoryGet`

### 22.1 Valid ID
```json
MCP: { "id": "<valid-memory-id>" }
```
**Expected**: Full memory details including kind-specific data, tags, timestamps.

### 22.2 Invalid ID
```json
MCP: { "id": "nonexistent-id-12345" }
```
**Expected**: Null/empty response or error, not a crash.

---

## 23. codegraph_memory_context

**LSP**: `codegraph.memoryContext`

### 23.1 File-level context
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs" }
LSP: { "uri": "file:///path/to/server/src/mcp/server.rs" }
```
**Expected**: Memories relevant to server.rs (debugging sessions, architecture decisions).

### 23.2 Line-level context
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "line": 597 }
```
**Expected**: Memories specifically relevant to handle_request function.

### 23.3 Filter by kinds
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "kinds": ["known_issue"] }
```
**Expected**: Only known_issue memories for this file.

### 23.4 Limit
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "limit": 2 }
```
**Expected**: At most 2 memories.

### 23.5 File with no memories
```json
MCP: { "uri": "file:///path/to/some/rarely-touched-file.rs" }
```
**Expected**: Empty results.

---

## 24. codegraph_memory_invalidate

**LSP**: `codegraph.memoryInvalidate`

### 24.1 Valid memory
```json
MCP: { "id": "<valid-memory-id>" }
```
**Expected**: Memory marked as invalidated. Still retrievable with currentOnly=false but excluded from default searches.

### 24.2 Already invalidated memory
```json
MCP: { "id": "<already-invalidated-id>" }
```
**Expected**: No error, idempotent operation.

### 24.3 Invalid ID
```json
MCP: { "id": "nonexistent-id" }
```
**Expected**: Error or failure status, not crash.

---

## 25. codegraph_memory_list

**LSP**: `codegraph.memoryList`

### 25.1 Default (all current memories)
```json
MCP: {}
```
**Expected**: List of non-invalidated memories with IDs, titles, kinds.

### 25.2 Filter by kinds
```json
MCP: { "kinds": ["debug_context", "known_issue"] }
```
**Expected**: Only debug_context and known_issue memories.

### 25.3 Filter by tags
```json
MCP: { "tags": ["rust"] }
```
**Expected**: Only memories tagged "rust".

### 25.4 Include invalidated
```json
MCP: { "currentOnly": false }
```
**Expected**: All memories including invalidated ones.

### 25.5 Pagination
```json
MCP: { "limit": 5, "offset": 0 }
MCP: { "limit": 5, "offset": 5 }
```
**Expected**: First page (0-4), second page (5-9).

---

## 26. codegraph_memory_stats

**LSP**: `codegraph.memoryStats`

### 26.1 Basic stats
```json
MCP: {}
```
**Expected**: Total count, breakdown by kind, storage info. Should reflect memories stored during testing.

---

## 27. codegraph_mine_git_history

**LSP**: `codegraph.mineGitHistory`

### 27.1 Default settings
```json
MCP: {}
LSP: {}
```
**Expected**: Mines up to 500 commits. Creates memories from commit patterns (bug fixes, refactors, features).

### 27.2 Limited commits
```json
MCP: { "maxCommits": 10 }
```
**Expected**: Only processes last 10 commits. Fewer memories created.

### 27.3 High confidence threshold
```json
MCP: { "minConfidence": 0.9 }
```
**Expected**: Fewer but higher-quality memories.

### 27.4 Low confidence threshold
```json
MCP: { "minConfidence": 0.3 }
```
**Expected**: More memories from marginal commit patterns.

### 27.5 Idempotency check
Run twice:
```json
MCP: { "maxCommits": 10 }
MCP: { "maxCommits": 10 }
```
**Expected**: Second run should not create duplicate memories.

---

## 28. codegraph_mine_git_history_for_file

**LSP**: `codegraph.mineGitHistoryForFile`

### 28.1 File with rich history
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs" }
LSP: { "uri": "file:///path/to/server/src/mcp/server.rs" }
```
**Expected**: File-specific memories from git log (recent changes, hotspot patterns).

### 28.2 Limited commits
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "maxCommits": 5 }
```
**Expected**: Only last 5 commits for this file analyzed.

### 28.3 High confidence threshold
```json
MCP: { "uri": "file:///path/to/server/src/mcp/server.rs", "minConfidence": 0.9 }
```
**Expected**: Fewer memories.

### 28.4 New/rarely-changed file
```json
MCP: { "uri": "file:///path/to/server/src/mcp/tools.rs" }
```
**Expected**: Few or no memories (limited git history).

---

## 29. codegraph_search_git_history

**LSP**: N/A (MCP-only tool)

### 29.1 Basic keyword search
```json
MCP: { "query": "fix unused code" }
```
**Expected**: Returns commits matching the query via semantic memory search + keyword git log. Results include commit hash, message, relevance score.

### 29.2 With time range
```json
MCP: { "query": "refactor", "timeRange": "last_month" }
```
**Expected**: Only commits from the last month matching "refactor".

### 29.3 Semantic search (zero keyword overlap)
```json
MCP: { "query": "persistent storage retrieval" }
```
**Expected**: Returns semantically relevant commits even without exact keyword matches (e.g., memory store commits).

### 29.4 Limit results
```json
MCP: { "query": "fix", "limit": 5 }
```
**Expected**: At most 5 results.

### 29.5 No matches
```json
MCP: { "query": "xyznonexistentquery123" }
```
**Expected**: Empty results, no crash.

---

## 30. codegraph_cross_project_search

**LSP**: N/A (MCP-only tool)

### 30.1 Basic search
```json
MCP: { "query": "Visitor" }
```
**Expected**: Returns symbols matching "Visitor" from other indexed projects (not the current project). Each result includes project slug, workspace path, file, line numbers.

### 30.2 Filter by symbol type
```json
MCP: { "query": "Visitor", "symbolType": "class" }
```
**Expected**: Only class/struct results from other projects.

### 30.3 Function search
```json
MCP: { "query": "parse", "symbolType": "function", "limit": 10 }
```
**Expected**: At most 10 function results from other projects containing "parse" in their name.

### 30.4 No other projects indexed
```json
MCP: { "query": "anything" }
```
**Expected**: If no other projects are indexed, returns empty results with `searched_projects: []`. No crash.

### 30.5 Current project excluded
```json
MCP: { "query": "McpServer" }
```
**Expected**: Does NOT return results from the current project. Only other indexed projects.

### 30.6 No matches across projects
```json
MCP: { "query": "xyznonexistentquery123" }
```
**Expected**: Empty results array with searched_projects listing the projects that were checked.

### 30.7 Limit
```json
MCP: { "query": "test", "limit": 5 }
```
**Expected**: At most 5 results across all projects.

---

## 31. codegraph_reindex_workspace

**LSP**: `codegraph.reindexWorkspace`

### 31.1 Basic reindex
```json
MCP: {}
LSP: {}
```
**Expected**: Returns file count (~70 for this project). Status "success". Graph cleared and rebuilt.

### 31.2 Verify post-reindex state
After reindex, run:
```json
MCP: codegraph_get_callers { "nodeId": "175" }
```
**Expected**: Callers still populated (cross-file resolution ran during reindex).

### 31.3 Consecutive reindexes
Run twice:
```json
MCP: {}
MCP: {}
```
**Expected**: Both succeed, same file count. No stale data.

---

## Cross-Cutting Test Scenarios

### CC.1 MCP vs LSP parity
For each tool, verify the same query returns equivalent results via both MCP and LSP paths:
- `get_callers` via MCP nodeId vs LSP uri+line
- `find_unused_code` via MCP scope=file vs LSP scope=file
- `analyze_complexity` via MCP vs LSP for same file

### CC.2 Fallback behavior
- Line pointing to whitespace → nearest symbol fallback
- Line 0 → file-level node
- Line beyond file end → error or last symbol
- Invalid URI format → error message

### CC.3 Cross-file call resolution
After reindex:
- Rust→Rust cross-file callers work (engine.rs function called from server.rs)
- TS→TS same-file callers work
- TS→TS cross-file callers work (if any direct calls exist in codebase)

### CC.4 Edge type coverage
Verify these edge types are created:
- `Calls` — function calls (same-file + cross-file)
- `Imports` — module imports
- `Contains` — class→method, file→function
- `DependsOn` — file→file dependencies

### CC.5 Node type coverage
Verify these node types appear:
- `Function` — functions/methods
- `Class` — classes/structs/traits
- `Module` — import targets
- `CodeFile` — file nodes
- `Interface` — traits/interfaces

### CC.6 Language coverage
Run key tools (`symbol_search`, `get_call_graph`, `analyze_complexity`, `get_ai_context`) on files from each supported language:
- Rust: `server/src/mcp/server.rs`
- TypeScript: `src/toolManager.ts`
- C: `open-vm-tools/lib/misc/util_misc.c` (requires open-vm-tools workspace)
- C++: `open-vm-tools/services/plugins/dndcp/dndcp.cpp` (previously failed, now tolerant)
- COBOL: `aws-mainframe-modernization-carddemo/app/cbl/COACTUPC.cbl` (PERFORM call graph)
- Fortran: `Fortran-Astrodynamics-Toolkit/src/kepler_module.f90` (cross-file call resolution)
- Verilog: any `.sv` file (function/task parameter extraction)

### CC.7 Performance
- `reindex_workspace` completes in <15 seconds for 85 files (Mac M-series)
- `reindex_workspace` completes in <90 seconds for 85 files (Windows, older CPU)
- `symbol_search` responds in <50ms (Jina Code V2 768d)
- `get_callers`/`get_callees` respond in <200ms
- `find_unused_code` workspace scope completes in <2 seconds
- MCP `initialize` responds instantly (indexing deferred to first tool call)
- `find_entry_points` default response <50KB

### CC.8 Error resilience
- Invalid URI → meaningful error, no crash
- Invalid nodeId → meaningful error, no crash
- Empty parameters → sensible defaults
- Very large depth values → no hang (bounded traversal)
- C++ files with syntax errors → tolerant parsing, partial extraction (not rejection)
- Fortran files with preprocessor directives → tolerant parsing

### CC.9 Multi-project indexing
```json
args: ["--mcp", "--workspace", "/path/to/project1", "--workspace", "/path/to/project2"]
```
- Both projects indexed into single graph
- `symbol_search` returns results from all projects
- `get_call_graph` resolves cross-file calls within each project
- `cross_project_search` finds symbols across indexed projects

### CC.10 Embedding model (Jina Code V2)
- Model downloads automatically on first run (~642MB fp32 ONNX)
- v4→v5 database migration clears old BGE-Small 384d vectors
- Semantic search returns code-relevant results for NL queries
- Clone detection: unrelated functions score <0.2, true clones >0.7
