---
mode: agent
description: >
  Plan a code change using CodeGraph to analyze impact, find all affected
  callers, locate tests, and produce an edit plan before writing code.
---

## Instructions

You are planning a code change (refactor, feature addition, or API modification).
Use CodeGraph tools to assess the blast radius before writing any code.

**Change description:** ${input:change:What you want to change (e.g. "rename method X", "add parameter to function Y", "extract class from module Z")}

### Step 1 — Identify affected symbols

Use `codegraph_symbol_search` to find all symbols involved in the change.

For each symbol, use `codegraph_get_symbol_info` to confirm its type, location,
and visibility (public vs private).

### Step 2 — Impact analysis

Use `codegraph_analyze_impact` on each symbol being modified.
This produces a report of:
- Direct callers that must be updated
- Transitive dependents that may break
- Files that need changes

### Step 3 — Map all callers

For each public symbol being changed, use `codegraph_get_callers` to get the
complete caller list. These are the sites that need updating.

Use `codegraph_traverse_graph` if you need to follow the call chain deeper
than one level.

### Step 4 — Check for unused code

Use `codegraph_find_unused_code` to identify dead code in the target area.
Clean it up as part of the change to avoid confusion.

### Step 5 — Find tests to update

Use `codegraph_find_related_tests` for each modified symbol.
Tests that call the changed function need updating too.

### Step 6 — Get edit context

Use `codegraph_get_edit_context` for each symbol that needs modification.
This provides the source with enough surrounding context for precise edits.

### Step 7 — Produce the edit plan

Output a structured plan:

```
## Edit Plan: <change description>

### Files to modify (N files, M symbols)
1. `path/to/file.rs` — <what changes here>
   - Symbol: `function_name` (line NN)
   - Change: <specific edit>
2. ...

### Tests to update
- `tests/test_file.rs` — update call to `function_name`
- ...

### Dead code to remove
- `old_helper()` in `path/to/utils.rs` — no callers

### Risk assessment
- Impact: <low/medium/high> — N callers across M files
- Test coverage: <adequate/sparse/none>
```

Wait for user approval before executing the plan.
