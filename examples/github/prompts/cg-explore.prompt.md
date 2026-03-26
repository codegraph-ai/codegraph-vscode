---
mode: agent
description: >
  Understand unfamiliar code using CodeGraph tools.
  Traces entry points, dependency graphs, coupling, and symbol relationships
  to build a mental model of how a module or feature works.
---

## Instructions

You are exploring an unfamiliar codebase or module to understand how it works.
Use CodeGraph tools systematically to build understanding from the outside in.

**Target:** ${input:target:Module path, file, feature name, or question about the code}

### Step 1 — Find entry points

Use `codegraph_find_entry_points` to discover the public API and top-level
entry points for the target module or file. These are the "doors in" to the code.

### Step 2 — Map the dependency graph

Use `codegraph_get_dependency_graph` to see what the target imports and depends on.
This shows the module's external dependencies and internal structure.

Use `codegraph_find_by_imports` to find all files that import the target — these
are the consumers/clients of this code.

### Step 3 — Understand coupling

Use `codegraph_analyze_coupling` to see how tightly coupled the target is to other
modules. High coupling = risky to change.

### Step 4 — Drill into key symbols

For the most important symbols found in steps 1-3:
- Use `codegraph_get_detailed_symbol` for full source and documentation
- Use `codegraph_get_call_graph` for call flow visualization
- Use `codegraph_get_callers` / `codegraph_get_callees` for specific symbol chains

### Step 5 — Check complexity

Use `codegraph_analyze_complexity` to identify complex areas that may need extra
attention or refactoring.

### Step 6 — Search git history for context

Use `codegraph_search_git_history` with relevant keywords to find commits that
explain why code was written this way.

Use `codegraph_mine_git_history_for_file` for a specific file's evolution.

### Step 7 — Summarize

Produce a brief summary covering:
1. **Purpose** — what does this module/feature do?
2. **Entry points** — how is it invoked?
3. **Key dependencies** — what does it rely on?
4. **Consumers** — who uses it?
5. **Complexity hotspots** — where are the dragons?
6. **Test coverage** — are there related tests?
