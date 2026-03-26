---
mode: agent
description: >
  Quick CodeGraph lookup — find a symbol, show its callers/callees,
  or answer a structural question about the codebase in a few steps.
---

## Instructions

You are answering a quick question about code structure. Use CodeGraph tools
for fast, precise answers instead of grepping.

**Question:** ${input:question:What do you want to know? (e.g. "who calls parse_config?", "what does Backend depend on?", "find all test files for the parser module")}

### Approach

Pick the most relevant CodeGraph tools based on the question type:

**"Who calls X?"**
→ `codegraph_symbol_search` to find X, then `codegraph_get_callers`

**"What does X call?"**
→ `codegraph_symbol_search` to find X, then `codegraph_get_callees`

**"What depends on module X?"**
→ `codegraph_find_by_imports` with the module path

**"Show me the call graph for X"**
→ `codegraph_symbol_search` to find X, then `codegraph_get_call_graph`

**"What tests cover X?"**
→ `codegraph_find_related_tests`

**"What would break if I change X?"**
→ `codegraph_analyze_impact`

**"Find symbols matching pattern"**
→ `codegraph_symbol_search`

**"Show the full source of X"**
→ `codegraph_get_detailed_symbol`

**"What are the entry points for this module?"**
→ `codegraph_find_entry_points`

**"Find unused code"**
→ `codegraph_find_unused_code`

**"What's the git history for this?"**
→ `codegraph_search_git_history` or `codegraph_mine_git_history_for_file`

### Output

Answer the question concisely. Include file paths and line numbers.
If the answer reveals something unexpected, mention it.
