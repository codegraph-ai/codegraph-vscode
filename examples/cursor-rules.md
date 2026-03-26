# CodeGraph — Cursor Rules

Add to `.cursorrules` in your project root:

---

When analyzing code structure, dependencies, or quality, use CodeGraph MCP tools instead of searching files manually:

- **Find code**: Use `codegraph_symbol_search` with natural language ("function that validates email") instead of grepping
- **Understand impact**: Use `codegraph_analyze_impact` before modifying a function to see what breaks
- **Trace calls**: Use `codegraph_get_callers` / `codegraph_get_callees` instead of text search for function references
- **Check duplicates**: Use `codegraph_find_duplicates` before writing new utility functions to avoid duplication
- **Get context**: Use `codegraph_get_ai_context` with intent="explain" for understanding, intent="modify" before editing
- **Find tests**: Use `codegraph_find_related_tests` to know which tests to run after changes
- **Check complexity**: Use `codegraph_analyze_complexity` to identify functions that need refactoring

All tools use `uri` (file URI like `file:///path/to/file.rs`) and `line` (0-indexed) to identify symbols. Use `nodeId` from `symbol_search` results as an alternative.
