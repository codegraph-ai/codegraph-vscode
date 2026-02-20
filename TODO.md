# CodeGraph VS Code — TODO

> Last updated: 2026-02-20

## Medium Priority

### 3. Expose ComplexityMetrics in tool responses (Tier 3 #10)
MCP and LSP complexity handlers return raw integers. The `ComplexityMetrics` struct from codegraph-parser-api provides richer data: grade, breakdown by category, threshold comparison. Would improve `codegraph_analyze_complexity` output.

### 4. Use PropertyMap improvements (Tier 3 #13)
Adopt `StringList` / `IntList` property variants for multi-valued properties (e.g., storing multiple imported symbols on an edge as a `StringList` instead of a comma-separated string).

## Future / On Demand

### 5. Add remaining language parsers (Tier 3 #12)
PHP, Ruby, Swift, and Tcl parsers are available in codegraph-monorepo if demand arises. Tcl includes EDA/VLSI domain support.

### 6. Publish to VS Code Marketplace
Currently at v0.6.0 locally. Requires marketplace publisher setup, CI/CD pipeline for packaging, and automated VSIX builds.

---

## Completed

- ~~Upgrade codegraph ecosystem to 0.2.0~~ (77f597a)
- ~~Replace hand-rolled BFS with built-in algorithms~~ (681e68d)
- ~~Add Java, C++, Kotlin, C# parsers~~ (e8b1918)
- ~~Auto-download Model2Vec embedding model~~ (231fad4)
- ~~Use find_file_by_path() for file-node lookups~~ (0f008ba)
- ~~Replace manual node iteration with iter_nodes()~~ (77f597a)
- ~~Fix MCP complexity handler~~ (77f597a)
- ~~Add proximity fallback to MCP symbol tools~~ (4226e24, 963c79f)
- ~~Add compact mode and limit options to search tools~~ (800efbf)
- ~~Implement MCP transport~~ (7bfc499)
- ~~Implement remaining 18 MCP tools~~ (689bd36)
- ~~Add memory layer with git mining~~ (87a2c01)
- ~~Implement AI Agent Query Engine~~ (a40d75f)
- ~~Implement MCP git mining tools~~ (ae3b0b0)
- ~~Broaden git mining to catch non-conventional commits~~ (ae3b0b0)
- ~~Fill in AI query engine TODOs~~ (4bbf24a) — edge_type, dependencies, dependents, has_tests
