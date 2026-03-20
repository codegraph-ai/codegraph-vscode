# MCP Registry & Directory Listings

> Track submission status across MCP directories and registries.
> Last updated: 2026-03-20

## Tier 1 — High Traffic

| Registry | URL | Status | Notes |
|---|---|---|---|
| Smithery | smithery.ai | **TODO** | Needs `smithery.yaml` in repo root. Largest MCP directory. |
| PulseMCP | pulsemcp.com/servers | **TODO** | Submit via pulsemcp.com/submit or API. |
| Glama | glama.ai/mcp/servers | **TODO** | Backed by MCP working group. Auto-indexes from GitHub. |
| LobeHub | lobehub.com/mcp | **TODO** | Coraline is already listed here. Submit via GitHub PR to plugin index. |

## Tier 2 — Niche / Community

| Registry | URL | Status | Notes |
|---|---|---|---|
| awesome-mcp-servers | github.com/punkpeye/awesome-mcp-servers | **TODO** | Curated list, 12k+ stars. Submit via PR. Category: "Code Intelligence". |
| mcp-get | mcp-get.com | **TODO** | Package manager. `npx @anthropic/mcp-get install @memoryx/codegraph-mcp`. |
| MCPHub | mcphub.io | **TODO** | Community directory. |
| Composio | composio.dev/mcp | **TODO** | MCP tool directory. |
| mcp.run | mcp.run | **SKIP** | Runs servers as WASM — not compatible with native Rust binary. |

## Tier 3 — Package Registries

| Registry | URL | Status | Notes |
|---|---|---|---|
| npm | npmjs.com/@memoryx/codegraph-mcp | **Published** | v0.9.1. Install: `npm install -g @memoryx/codegraph-mcp` |
| crates.io | crates.io | **TODO** | Publish `codegraph-lsp` binary crate for `cargo install`. |
| VS Code Marketplace | marketplace.visualstudio.com | **In progress** | Handled separately. |

## Submission Requirements

Most registries need:
- Public GitHub repo URL
- npm package name or install command
- Tool list with descriptions (31 tools)
- Category: "Code Intelligence" / "Development Tools"
- Short description + README

### Smithery-specific

Needs `smithery.yaml` in repo root:
```yaml
startCommand:
  type: stdio
  configSchema:
    type: object
    properties:
      workspace:
        type: string
        description: Path to workspace directory (optional, defaults to cwd)
  command: npx
  args:
    - -y
    - "@memoryx/codegraph-mcp"
```

### Listing Description (short)

> CodeGraph — Open-source code intelligence MCP server. Builds a semantic graph of your codebase (functions, classes, imports, call chains) and exposes it through 31 tools. Callers, callees, impact analysis, complexity metrics, unused code detection, AI context assembly, persistent memory, cross-project search. 15 languages via tree-sitter. Single Rust binary, local-first.

### Listing Description (one-liner)

> Semantic code graph with 31 MCP tools — callers, callees, impact analysis, complexity, unused code, AI context. 15 languages. Local-first Rust binary.

## Tracking

- [ ] Create `smithery.yaml`
- [ ] Submit to Smithery
- [ ] Submit to PulseMCP
- [ ] Submit to Glama
- [ ] Submit to LobeHub
- [ ] PR to awesome-mcp-servers
- [ ] Register on mcp-get
- [ ] Submit to MCPHub
- [ ] Submit to Composio
- [ ] Publish to crates.io
