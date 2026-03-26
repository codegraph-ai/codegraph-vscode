---
mode: agent
description: >
  Debug a code issue using CodeGraph tools to trace call chains,
  find callers/callees, analyze impact, and locate related tests.
---

## Instructions

You are debugging a code issue. Use CodeGraph tools to build a precise understanding
of the code before suggesting fixes.

**Target:** ${input:target:Symbol name, file path, or description of the buggy behavior}

### Step 1 — Locate the symbol

Use `codegraph_symbol_search` to find the symbol(s) related to the target:
- Search for the function name, struct name, or error string
- If multiple matches, narrow by file path or type

Then use `codegraph_get_symbol_info` to get the symbol's type, location, and metadata.

### Step 2 — Understand the call context

Use `codegraph_get_callers` to find who calls this symbol — these are the entry
points through which the bug can be triggered.

Use `codegraph_get_callees` to see what this symbol depends on — downstream
failures here can propagate upward.

If the call chain is deep, use `codegraph_get_call_graph` for a broader view.

### Step 3 — Analyze impact

Use `codegraph_analyze_impact` to understand what would be affected by a change to
this symbol. This tells you:
- How many callers would be affected
- Which files and modules are involved
- Whether the change is safe or high-risk

### Step 4 — Find related tests

Use `codegraph_find_related_tests` to locate tests that exercise this code path.
If tests exist, read them to understand expected behavior and reproduce the bug.

### Step 5 — Get detailed context

Use `codegraph_get_detailed_symbol` for the full source of the symbol.

Use `codegraph_get_edit_context` when you're ready to make a fix — it provides
the symbol source plus surrounding context optimized for editing.

### Step 6 — Propose fix

Based on the call chain analysis and test coverage:
1. State the root cause
2. Propose the minimal fix
3. List which tests cover this path (or note if tests are missing)
4. Identify any callers that might need to be updated

### Best practices

- Always start with `codegraph_symbol_search` — don't grep blindly
- Use `codegraph_get_callers` before changing a function signature
- Use `codegraph_analyze_impact` before making any public API change
- Check `codegraph_find_related_tests` to know if you need to add tests
- Use `codegraph_get_ai_context` for a high-level summary when lost
