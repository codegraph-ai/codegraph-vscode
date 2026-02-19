# Red-Team Review: Tier 2 Implementation

> **Date**: 2026-02-18
> **Scope**: Adding Java, C++, Kotlin, C# parsers + QueryBuilder `.count()` optimization
> **Commit**: `e8b1918`

---

## Coupling Audit

`parser_registry.rs` has 12 transitive dependents (every handler, backend, watcher, MCP server).
However, the public API is unchanged — downstream files consume the registry through `Arc<dyn CodeParser>` trait objects. Risk score: 0.22/1.0.

The `.count()` change in `resources.rs` has minimal blast radius (3 affected files, risk 0.10/1.0).

**Verdict:** Coupling is structurally safe. No API contracts broken.

---

## Critical Issues

### 1. `.h` files silently misrouted for C++ projects

Both parsers claim `.h`:
- C parser: `[".c", ".h"]`
- C++ parser: `[".cpp", ".cc", ".cxx", ".hpp", ".hh", ".hxx", ".h"]`

In `parser_for_path()`, C is at index 4, C++ at index 6. First-match-wins means `.h` files are **always parsed as C, never C++**. This is wrong for C++ projects where `.h` files contain classes, templates, and namespaces.

In `language_for_path()`, the same ordering applies — `.h` returns `"c"` even in a C++ codebase.

**Resolution:** Document that `.h` defaults to C (matches most toolchain defaults where `.h` is C-compatible and `.hpp` signals C++). Add test for `.hpp` asserting `"cpp"`. Add missing extension tests for all variants.

---

## Concerns

### 2. No `.hpp`/`.cc`/`.kts` test coverage

Tests only cover one extension per new language (`.java`, `.cpp`, `.kt`, `.cs`). C++ has 7 extensions, Kotlin has 2 (`.kt`, `.kts`). None of the additional variants are tested.

### 3. Binary size increase unquantified

4 new tree-sitter grammars (native C code) linked into the binary. Debug binary is 95MB. Release binary with LTO will grow by an estimated 2-4MB. Should be measured for release builds.

### 4. `query().count()` vs `.execute().map(len)` semantics

Both are O(n) over all nodes. `.count()` avoids the `Vec<NodeId>` allocation. `count()` returns `Result<usize>` — the `unwrap_or(0)` handles errors identically to the old code. Correct optimization, no bug.

---

## Observations

### 5. `ParserRegistry` grows linearly

9 fields, 9 match arms, 9 array entries across 7 locations. Each new language adds ~15 lines. Acceptable at 9 languages; would benefit from `Vec<(String, Arc<dyn CodeParser>)>` at 15+.

### 6. VS Code `documentSelector` language IDs

Uses `"java"`, `"cpp"`, `"kotlin"`, `"csharp"` — all match VS Code conventions. However, if a user lacks the relevant language extension, these entries silently do nothing.

### 7. `parser_for_path` clones 9 `Arc`s per call

Builds a 9-element array of `Arc::clone()` for every path lookup. Negligible in practice (~90K atomic ops for 10K files) but could be optimized by iterating references.

---

## Action Items

- [x] Document `.h` → C default as intentional (doc comments on `parser_for_path` and `language_for_path`)
- [x] Add test coverage for `.hpp`, `.cc`, `.hxx`, `.hh`, `.cxx`, `.kts` extensions
- [x] Add `language_for_path` tests for C++ header variants (including `.h` → C assertion)
- [ ] Measure release binary size delta (deferred to release)
