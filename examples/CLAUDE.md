# CodeGraph — CLAUDE.md Snippet

Add to your project's `CLAUDE.md` or `~/.claude/CLAUDE.md`:

---

## CodeGraph MCP Tools

CodeGraph provides 35 MCP tools for code intelligence. Use them instead of manual file search:

### When to use which tool
- **Starting exploration**: `codegraph_symbol_search` — find symbols by name or natural language
- **Before editing**: `codegraph_get_edit_context` — get source, callers, tests, and git history for the target
- **Understanding code**: `codegraph_get_ai_context` with `intent: "explain"` — full context with related symbols
- **Checking impact**: `codegraph_analyze_impact` — see what breaks if you modify/delete/rename
- **Finding duplicates**: `codegraph_find_duplicates` — before writing new code, check if it exists
- **Tracing calls**: `codegraph_get_callers` / `codegraph_get_callees` — who calls this? what does it call?
- **Finding tests**: `codegraph_find_related_tests` — which tests cover this function?
- **Code quality**: `codegraph_analyze_complexity` — identify refactoring candidates

### Parameter patterns
All tools that target a specific symbol accept either:
- `uri` + `line` (file URI like `file:///path/to/file.rs`, 0-indexed line number)
- `nodeId` (string ID from `symbol_search` results)

### Tips
- Run `codegraph_reindex_workspace` after major file changes
- Use `compact: true` on search tools for smaller responses
- `find_duplicates` with `threshold: 0.9` for near-exact clones, `0.7` for similar code
- `cluster_symbols` with `threshold: 0.8, minClusterSize: 3` for architectural patterns

### Embedding model
Default is `jina-code-v2` (768d, best clone detection quality). For faster indexing on slower hardware, configure `bge-small` (384d, 5x faster):
- MCP: add `--embedding-model bge-small` to args
- VS Code: set `"codegraph.embeddingModel": "bge-small"` in settings.json
