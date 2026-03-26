# GitHub Copilot Workspace Instructions

## CodeGraph Tools — Use These First

The CodeGraph extension provides MCP tools for structural code analysis. **Always prefer
CodeGraph tools over grep/find for navigating code.** These tools work on any indexed
workspace folder.

### When to use which tool

| Task | Tool | Example |
|---|---|---|
| Find a function/struct/type | `codegraph_symbol_search` | Find `parse_config` across all files |
| Who calls this function? | `codegraph_get_callers` | Find all callers of `did_open` |
| What does this function call? | `codegraph_get_callees` | Trace downstream from `index_directory` |
| Full call chain visualization | `codegraph_get_call_graph` | See the call tree rooted at `execute_command` |
| Impact of changing something | `codegraph_analyze_impact` | Before renaming a public method |
| What imports this module? | `codegraph_find_by_imports` | Find all consumers of a module |
| Module entry points | `codegraph_find_entry_points` | Understand a module's public API |
| Dependencies of a module | `codegraph_get_dependency_graph` | See what a file/module imports |
| Module coupling analysis | `codegraph_analyze_coupling` | Assess how tightly coupled code is |
| Complexity hotspots | `codegraph_analyze_complexity` | Find overly complex functions |
| Dead code detection | `codegraph_find_unused_code` | Find functions with no callers |
| Find tests for a symbol | `codegraph_find_related_tests` | Check test coverage before changing code |
| Full symbol source | `codegraph_get_detailed_symbol` | Read complete function body + docs |
| Context for editing | `codegraph_get_edit_context` | Get source + surrounding context for edits |
| AI-friendly summary | `codegraph_get_ai_context` | High-level overview when getting oriented |
| Curated context bundle | `codegraph_get_curated_context` | Rich context package for a code area |
| Traverse the graph | `codegraph_traverse_graph` | Walk call/import edges manually |
| Git history search | `codegraph_search_git_history` | Find commits mentioning a keyword |
| File git history | `codegraph_mine_git_history_for_file` | See evolution of a specific file |
| Re-index workspace | `codegraph_reindex_workspace` | Refresh index after major changes |
| Find by function signature | `codegraph_find_by_signature` | Match functions by parameter types |

### CodeGraph workflow patterns

**Debugging:** `symbol_search` → `get_callers` → `get_call_graph` → `find_related_tests` → fix
**Exploring:** `find_entry_points` → `get_dependency_graph` → `get_detailed_symbol` → summarize
**Planning changes:** `symbol_search` → `analyze_impact` → `get_callers` → `find_related_tests` → plan
**Quick lookup:** `symbol_search` → `get_callers` or `get_callees` → answer

### CodeGraph memory tools

CodeGraph has its own persistent memory system for storing notes about code:

| Tool | Purpose |
|---|---|
| `codegraph_memory_store` | Save a note linked to code symbols |
| `codegraph_memory_get` | Retrieve a specific memory |
| `codegraph_memory_search` | Search memories by keyword |
| `codegraph_memory_list` | List all stored memories |
| `codegraph_memory_context` | Get memories relevant to current context |
| `codegraph_memory_invalidate` | Remove outdated memories |
| `codegraph_memory_stats` | Memory system statistics |

Use memories to record investigation findings, known issues, architectural decisions,
or tricky code behavior so you don't have to rediscover them.

---