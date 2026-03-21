# MCP Registry & Directory Listings

> Track submission status across MCP directories and registries.
> Last updated: 2026-03-20

## Tier 1 — High Traffic

| Registry | URL | Status | Notes |
|---|---|---|---|
| Smithery | smithery.ai | **BLOCKED** | Requires Streamable HTTP transport + public HTTPS URL. Our server is stdio-only. Need to add HTTP transport or deploy behind mcp-proxy. |
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

## Official MCP Registry + PulseMCP Publishing

PulseMCP ingests from the official MCP registry automatically. Publish once, get listed on both.

### Prerequisites
- `mcpName` field in `mcp-package/package.json` (done: `io.github.codegraph-ai/codegraph`)
- `mcp-package/server.json` with registry schema (done)
- `mcp-publisher` CLI installed at `~/bin/mcp-publisher`

### Publishing Steps
```bash
cd /Users/anvanster/projects/codegraph-vscode/mcp-package

# 1. Publish npm package (must include mcpName field)
npm publish --access public

# 2. Login to MCP registry via GitHub (one-time, opens browser)
~/bin/mcp-publisher login github

# 3. Publish server metadata to official registry
~/bin/mcp-publisher publish

# 4. Verify
curl "https://registry.modelcontextprotocol.io/v0.1/servers?search=io.github.codegraph-ai/codegraph"
```

PulseMCP picks up new entries within a week. Email `hello@pulsemcp.com` to expedite.

## Tracking

- [ ] ~~Create `smithery.yaml`~~ — BLOCKED (requires HTTP transport)
- [ ] ~~Submit to Smithery~~ — BLOCKED
- [x] Submit to PulseMCP (via official registry) — published 2026-03-20
- [x] Submit to Glama — submitted for review 2026-03-20
- [ ] Submit to LobeHub — likely auto-indexes from official MCP registry. No public submit process found. Monitor.
- [x] PR to awesome-mcp-servers — PR #3579 submitted 2026-03-20
- [x] Register on mcp-get — PR submitted 2026-03-20
- [ ] Submit to MCPHub — no clear submission process found, monitor
- [ ] Submit to Composio — no submission form found, monitor
- [ ] Publish to crates.io — requires publishing all codegraph-monorepo crates first (path deps → registry deps). Deferred.
