# CodeGraph IDE — Architecture Document

> **Status**: Design / Kickstart Document
> **Date**: 2026-03-08
> **Base**: Lapce v0.4.6 (Apache-2.0 fork)
> **Core differentiator**: Deep code graph integration + multi-provider AI — the IDE that understands your code structurally, not just textually.

---

## 1. Vision

A fast, Rust-native IDE where:

1. **The code graph is always live.** Every file is parsed once by tree-sitter; the same AST feeds both syntax highlighting and the CodeGraph engine. The graph updates incrementally on every keystroke.
2. **AI has structural understanding.** The AI assistant doesn't search your codebase — it *queries the graph*. Call chains, impact analysis, dead code, architectural layers — all available as direct in-process graph traversals with microsecond latency.
3. **Everything runs locally.** No cloud dependency for core features. AI providers are pluggable (Claude API, OpenRouter, Ollama, Copilot), but the graph intelligence works offline.

### What makes developers switch

- Open any project → see its architecture in seconds (live, interactive graph panel)
- Before any edit → see the blast radius (impact preview)
- AI assistant answers "how does authentication work?" with structural traces, not grep results
- Dead code is visually dimmed. Architectural violations are squiggly underlines. All continuous, all live.

---

## 2. Why Lapce

| Property | Benefit |
|----------|---------|
| **Rust** | Zero FFI boundary with CodeGraph engine. Same language, same allocator, shared data structures. |
| **GPU rendering** (Floem/wgpu) | Graph visualizations rendered natively on GPU. No webview overhead. |
| **Tree-sitter built-in** | 170+ language grammars already loaded. Parse once, use twice. |
| **Proxy architecture** | Clean backend separation. Graph engine runs in proxy alongside LSP/DAP. Remote development works natively. |
| **Rope data structure** | O(log n) edits, COW snapshots. Graph can read buffer state without blocking the editor. |
| **Apache-2.0** | Permissive license. Fork freely. |
| **~90MB RAM, ~60MB disk** | Lightweight baseline. Room for graph + AI overhead. |
| **Pre-1.0** | Small codebase (~4 crates). Easier to deeply integrate vs. forking VS Code's 1M+ LOC. |

### Lapce limitations to accept

- Plugin ecosystem is tiny (vs. VS Code's 70K+). This IDE wins on built-in capabilities, not extensions.
- Floem documentation is sparse. UI work will involve reading Floem source.
- Some baseline features still catching up (code folding, .editorconfig, smooth scrolling).
- No webview support — all UI must be native Floem widgets.

---

## 3. System Architecture

```
┌──────────────────────────────────────────────────────────┐
│  lapce-app (Floem / wgpu)                                │
│                                                          │
│  ┌────────┐ ┌──────────┐ ┌──────────┐ ┌──────────────┐  │
│  │ Editor │ │ AI Chat  │ │ Graph    │ │ Impact       │  │
│  │        │ │ Panel    │ │ Explorer │ │ Preview      │  │
│  └───┬────┘ └────┬─────┘ └────┬─────┘ └──────┬───────┘  │
│      │           │            │               │          │
│  ────┴───────────┴────────────┴───────────────┴────────  │
│              Reactive signals (Floem / RwSignal<T>)      │
│                                                          │
│  ┌────────────────────────────────────────────────────┐  │
│  │  Proxy Client (lapce-rpc, JSON-over-stdio)        │  │
│  └─────────────────────┬──────────────────────────────┘  │
└────────────────────────┼─────────────────────────────────┘
                         │ stdin/stdout (JSON-RPC)
┌────────────────────────┼─────────────────────────────────┐
│  lapce-proxy           │                                 │
│                        ▼                                 │
│  ┌─────────────────────────────────────────────────────┐ │
│  │  Dispatcher (dispatch.rs)                           │ │
│  │  Routes ProxyRequest / ProxyNotification            │ │
│  └──┬──────────┬──────────┬──────────┬─────────────────┘ │
│     │          │          │          │                    │
│  ┌──▼──┐  ┌───▼────┐ ┌───▼─────┐ ┌──▼───────────────┐   │
│  │ LSP │  │Terminal│ │  Git    │ │ CodeGraph Engine │   │
│  │ DAP │  │  PTY   │ │ (git2) │ │ (see §4)         │   │
│  └─────┘  └────────┘ └────────┘ └──┬────────────────┘   │
│                                    │                     │
│  ┌─────────────────────────────────▼──────────────────┐  │
│  │  AI Engine (see §5)                                │  │
│  │  ┌─────────────┐ ┌────────────┐ ┌───────────────┐ │  │
│  │  │ LLM Router  │ │ Context    │ │ Edit Applier  │ │  │
│  │  │ (providers) │ │ Assembler  │ │ (diff/merge)  │ │  │
│  │  └─────────────┘ └────────────┘ └───────────────┘ │  │
│  └────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────┘
```

### New crates in the workspace

| Crate | Purpose |
|-------|---------|
| `lapce-graph` | CodeGraph engine: graph DB, query engine, symbol index, memory, embeddings. Ported from `codegraph-lsp` server logic. |
| `lapce-ai` | AI engine: provider trait, LLM router, context assembler, edit applier, streaming. |
| `lapce-ai-providers` | Provider implementations: Anthropic, OpenAI-compat (OpenRouter/Ollama/z.ai), Copilot bridge. |

Existing crates modified:

| Crate | Changes |
|-------|---------|
| `lapce-core` | Hook `Syntax::parse()` to emit AST to graph engine. Expose `Tree` access. |
| `lapce-proxy` | Add `CodeGraphEngine` + `AiEngine` as core subsystems (not plugins). New `ProxyRequest`/`ProxyNotification` variants. |
| `lapce-rpc` | Add graph + AI RPC message types. |
| `lapce-app` | New panels (AI Chat, Graph Explorer, Impact Preview). Inline edit rendering. |

---

## 4. CodeGraph Engine Integration

### 4.1 Parse once, use twice

Lapce's `Syntax::parse()` produces a tree-sitter `Tree` for syntax highlighting. Currently this tree is only used for highlight spans. The integration:

```
File edited → Rope updated → Syntax::parse() → tree-sitter Tree
                                                     │
                                    ┌────────────────┴────────────────┐
                                    ▼                                 ▼
                           SyntaxLayers::highlight()         GraphUpdater::update()
                           (existing — highlight spans)      (new — graph nodes/edges)
```

**How it connects:**

1. `Syntax` in `lapce-core` already holds the `Tree`. After each incremental parse, it emits a signal.
2. `lapce-graph` subscribes to this signal. It receives `(&Tree, &Rope, LapceLanguage)`.
3. `GraphUpdater` walks the tree using the existing CodeGraph parser crates (codegraph-typescript, codegraph-rust, etc.) but **feeds them the already-parsed tree** instead of re-parsing from source text.
4. The graph update is incremental — only the changed subtree is re-walked.

**Parser crate adaptation needed:** Current CodeGraph parsers call `tree_sitter::Parser::parse()` internally. They need a mode where they accept a pre-built `Tree` and walk it directly. This is a moderate refactor — the walker/visitor logic stays the same, only the parse entry point changes.

### 4.2 Graph storage

| Component | Storage | Scope |
|-----------|---------|-------|
| Code graph (nodes, edges) | In-memory `CodeGraph` | Per-project, rebuilt on open |
| Symbol index (BM25 + embeddings) | In-memory | Per-project, rebuilt on open |
| Persistent memory (decisions, debugging context) | RocksDB at `~/.codegraph/projects/<slug>/` | Persists across sessions |
| Shared cross-project DB | RocksDB at `~/.codegraph/graph.db` | Persists, shared across projects |

The in-memory graph rebuilds fast (existing CodeGraph indexes 69 files in <2s). For large projects, persist the graph to disk and load on startup with incremental updates.

### 4.3 Graph-powered editor features

These are built into the editor, not exposed through LSP or plugins:

| Feature | Implementation | UX |
|---------|---------------|-----|
| **Impact preview** | On cursor move, query `callers()` + `find_related_tests()`. Display in sidebar panel. | Sidebar shows callers, tests, consumers. Updates as cursor moves. |
| **Dead code dimming** | On graph rebuild, compute nodes with zero incoming edges from entry points. Emit as `Spans<Style>` with reduced opacity. | Unreachable code renders at 40% opacity. |
| **Architectural diagnostics** | User-defined rules (e.g., "ui/ cannot import db/"). Graph checks on every update. Violations become `Diagnostic` entries. | Squiggly underlines + Problems panel, like type errors. |
| **Structural search** | Command palette command: "Find functions with complexity > C and no tests". Graph query, results in search panel. | Like workspace symbol search, but with structural predicates. |
| **Inline complexity** | Code lens showing complexity grade (A-F) per function. Graph-derived, always up to date. | Subtle annotation above function signatures. |

### 4.4 RPC additions for graph

New `ProxyRequest` variants:

```rust
enum ProxyRequest {
    // ... existing variants ...

    // Graph queries
    GraphGetCallers { uri: Url, line: u32 },
    GraphGetCallees { uri: Url, line: u32 },
    GraphGetImpact { uri: Url, line: u32, mode: ImpactMode },
    GraphSymbolSearch { query: String, symbol_type: Option<String>, limit: u32 },
    GraphGetDependencyGraph { uri: Url, depth: u32 },
    GraphGetComplexity { uri: Url },
    GraphFindUnusedCode { scope: Scope, confidence: f64 },
    GraphFindRelatedTests { uri: Url, line: u32 },
    GraphGetEditContext { uri: Url, line: u32 },
    GraphCrossProjectSearch { query: String, symbol_type: Option<String> },
    GraphGetArchViolations {},
}
```

New `CoreNotification` variants (proxy → frontend):

```rust
enum CoreNotification {
    // ... existing variants ...

    GraphRebuilt { file_count: u32, node_count: u32, edge_count: u32 },
    GraphDeadCodeUpdate { dead_ranges: Vec<(PathBuf, Range)> },
    GraphArchViolation { uri: Url, range: Range, rule: String, message: String },
}
```

---

## 5. AI Engine Architecture

### 5.1 Provider abstraction

Two API families, one trait:

```rust
#[async_trait]
pub trait AiProvider: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn models(&self) -> Vec<ModelInfo>;

    async fn chat_stream(
        &self,
        request: ChatRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = ChatEvent> + Send>>>;

    fn supports_tools(&self) -> bool;
    fn supports_vision(&self) -> bool;
}

pub enum ChatEvent {
    Text(String),
    ToolCall { id: String, name: String, arguments: String },
    Usage { input_tokens: u32, output_tokens: u32 },
    Done,
    Error(String),
}
```

### 5.2 Provider implementations

| Provider | Crate/Module | API Format | Auth | Streaming | Notes |
|----------|-------------|------------|------|-----------|-------|
| **Anthropic (Claude)** | `anthropic.rs` | Anthropic Messages API | `x-api-key` header | SSE (`message_start`, `content_block_delta`, ...) | Tool use, prompt caching, extended thinking. Custom HTTP path (not OpenAI-compatible). |
| **OpenAI-compatible** | `openai_compat.rs` | OpenAI Chat Completions | `Authorization: Bearer` | SSE (`data: {"choices":[{"delta":...}]}`) | Covers OpenRouter, Ollama (localhost), z.ai, direct OpenAI. Single implementation, configurable base URL. |
| **GitHub Copilot** | `copilot.rs` | Copilot Language Server (LSP) | OAuth device flow → session token | Via language server stdio | Requires Node.js runtime. Spawn `@github/copilot-language-server` as child process. Handles inline completion + chat. |

**OpenRouter free models** for zero-cost AI features:
- `deepseek/deepseek-chat-v3-0324:free` — strong coding model
- `deepseek/deepseek-r1:free` — reasoning model
- `meta-llama/llama-4-maverick:free` — general purpose
- `qwen/qwq-32b:free` — reasoning
- `google/gemini-2.5-pro-exp-03-25:free` — experimental but capable
- Rate limits: ~20 req/min, ~200/day on free tier

**Ollama (local models)** for offline/private use:
- `http://localhost:11434/v1/chat/completions` (OpenAI-compatible, same code path as OpenRouter)
- Best local models for code: Qwen2.5-Coder-32B (quality) or Qwen2.5-Coder-7B (laptop-friendly)
- No API key needed

### 5.3 LLM Router

```rust
pub struct LlmRouter {
    providers: HashMap<String, Arc<dyn AiProvider>>,
    active_model: RwLock<ModelId>,  // e.g., "anthropic/claude-sonnet-4-6"
    fallback_model: Option<ModelId>,
}

// ModelId format: "provider_id/model_id" (matches Zed's convention)
pub struct ModelId {
    provider: String,  // "anthropic", "openrouter", "ollama", "copilot"
    model: String,     // "claude-sonnet-4-6", "deepseek/deepseek-chat-v3-0324:free"
}
```

Users configure providers in settings:

```toml
[ai]
default_model = "anthropic/claude-sonnet-4-6"

[ai.providers.anthropic]
api_key = "sk-ant-..."

[ai.providers.openrouter]
api_key = "sk-or-..."

[ai.providers.ollama]
base_url = "http://localhost:11434"
# no API key needed

[ai.providers.copilot]
enabled = true
# auth via OAuth device flow (browser-based)
```

### 5.4 Context Assembler — the graph advantage

This is where deep integration pays off. The Context Assembler queries the CodeGraph engine directly (in-process, no serialization) to build structurally relevant context for every AI request.

**How it works:**

```
User message: "Refactor the validate_token function to return a proper error type"
                                        │
                                        ▼
                              ┌─────────────────────┐
                              │  Context Assembler   │
                              └──────────┬──────────┘
                                         │
            ┌────────────────────────────┼────────────────────────────┐
            ▼                            ▼                            ▼
   symbol_search("validate_token")  get_callers(node_id)    find_related_tests(node_id)
   → source code + signature        → 14 call sites          → 3 test functions
            │                            │                            │
            ▼                            ▼                            ▼
   get_complexity(node_id)          get_callees(node_id)    search_memory("error handling")
   → grade C, 12 branches           → 5 downstream calls    → 2 relevant memories
            │                            │                            │
            └────────────────────────────┴────────────────────────────┘
                                         │
                                         ▼
                              ┌─────────────────────┐
                              │  Structured Prompt   │
                              │                      │
                              │  System: project ctx │
                              │  + graph context:    │
                              │    - function source  │
                              │    - 14 callers       │
                              │    - 3 tests          │
                              │    - 5 callees        │
                              │    - complexity: C    │
                              │    - memories         │
                              │  User: "Refactor..." │
                              └─────────────────────┘
```

**vs. Cursor's approach:**
- Cursor: embed all files → vector search "validate_token" → get 5 similar chunks → hope they're relevant
- CodeGraph IDE: graph query → get *exactly* the callers, tests, callees, and error handling patterns → always relevant

**Context budget:** Token-budgeted allocation (reuse CodeGraph's existing `get_curated_context` logic):
- 40% — target symbol source + callers
- 25% — related tests
- 15% — callees and dependencies
- 10% — persistent memories
- 10% — metadata (complexity, architecture layer, git history)

### 5.5 Edit application

Learning from Cursor and Zed, use the **two-stage pattern**:

1. **Planning stage**: The frontier model (Claude, GPT-4o) returns the edit plan as structured tool calls or annotated code blocks.
2. **Apply stage**: A deterministic merge algorithm applies the edit to the file buffer.

For the MVP, use a simple search/replace approach:
- The AI returns old/new code blocks (like Claude Code's `Edit` tool)
- The editor applies them as `Rope` operations
- Changes render as inline diffs (green/red) with accept/reject per hunk

Future enhancement: train or use a specialized fast-apply model (like Cursor's speculative edits or Morph's 7B apply model) for full-file rewrites.

### 5.6 Tool system

The AI can call tools (similar to Zed's agent loop):

```rust
pub enum AiTool {
    // File operations
    ReadFile { path: PathBuf },
    EditFile { path: PathBuf, old: String, new: String },
    CreateFile { path: PathBuf, content: String },
    ListFiles { path: PathBuf, pattern: Option<String> },

    // Terminal
    RunCommand { command: String, cwd: Option<PathBuf> },

    // Graph queries (unique to this IDE)
    GraphCallers { uri: Url, line: u32 },
    GraphCallees { uri: Url, line: u32 },
    GraphImpact { uri: Url, line: u32 },
    GraphSearch { query: String, symbol_type: Option<String> },
    GraphComplexity { uri: Url },
    GraphUnusedCode { scope: String },
    GraphFindTests { uri: Url, line: u32 },
    GraphCrossProject { query: String },
    GraphArchLayer { uri: Url },

    // Memory
    MemorySearch { query: String },
    MemoryStore { summary: String, kind: String },

    // Search
    GrepSearch { pattern: String, path: Option<PathBuf> },
    SemanticSearch { query: String },
}
```

**Agent loop** (same pattern as Zed):

```
1. Build request (system prompt + messages + tools)
2. Stream LLM response
3. If response contains tool calls:
   a. Execute tools concurrently
   b. Append tool results to conversation
   c. Go to 1
4. If response is text only:
   a. Render in chat panel
   b. Apply any file edits with inline diff
   c. End turn
```

---

## 6. UI Panels (Floem)

### 6.1 AI Chat Panel

Added as `PanelKind::AiChat` in Lapce's panel system.

**Components:**
- Message list (`virtual_list` for performance with long conversations)
- Markdown renderer (reuse Lapce's existing `parse_markdown()` from `lapce-app/src/markdown.rs`)
- Code blocks with syntax highlighting (tree-sitter, already available)
- Text input with send button
- Model selector dropdown
- Inline diff preview for proposed edits

**Streaming rendering:**
- Use `create_ext_action` (Floem) to bridge async AI stream → reactive signal updates
- Append tokens to a `RwSignal<String>` as they arrive
- Re-parse markdown incrementally on each chunk

**File edit UX:**
When the AI proposes a file edit:
1. Show the diff inline in the chat (green/red)
2. "Apply" button opens the file and shows the diff in the editor buffer
3. Accept/reject per hunk with keyboard shortcuts
4. Checkpoint created before apply (restorable)

### 6.2 Graph Explorer Panel

Added as `PanelKind::GraphExplorer`.

**Two modes:**

1. **Symbol view** (default): Shows the current symbol under cursor with:
   - Callers (incoming edges)
   - Callees (outgoing edges)
   - Related tests
   - Complexity grade
   - Architecture layer
   - Updates reactively as cursor moves (debounced 200ms)

2. **Architecture view**: Shows the project's module dependency graph:
   - Rendered as a node-link diagram using Floem's `canvas` view + wgpu
   - Clickable nodes navigate to the corresponding file
   - Edge colors indicate relationship type (imports, calls, type references)
   - Highlights coupling hotspots and circular dependencies

### 6.3 Impact Preview Panel

Added as `PanelKind::ImpactPreview`.

- Activated on demand (keyboard shortcut) or always-visible in sidebar
- Shows: "If you change this function..."
  - N direct callers (with file:line links)
  - M transitive dependents
  - K tests that exercise this code
  - Risk score (0.0 - 1.0)
- Color-coded: green (low risk, well-tested) → red (high risk, no tests, many dependents)

---

## 7. Implementation Phases

### Phase 0 — Fork and validate (2-3 weeks)

**Goal:** Fork Lapce, add CodeGraph as a workspace crate, verify tree-sitter integration.

- [ ] Fork Lapce, get it building on macOS/Linux
- [ ] Add `lapce-graph` crate to workspace
- [ ] Port CodeGraph engine (graph, parsers, query engine, symbol index) into `lapce-graph`
- [ ] Hook `Syntax::parse()` to feed tree-sitter `Tree` to graph engine
- [ ] Verify: open a Rust/TS project → graph builds automatically → log node/edge counts
- [ ] Verify: edit a file → graph updates incrementally

**Success criteria:** Graph builds and updates without user interaction. No re-parse penalty.

### Phase 1 — Graph panels (3-4 weeks)

**Goal:** Visual graph features that demonstrate the value.

- [ ] Add `PanelKind::GraphExplorer` — symbol view (callers, callees, tests for cursor symbol)
- [ ] Add `PanelKind::ImpactPreview` — blast radius for current function
- [ ] Dead code dimming — unreachable code renders at reduced opacity
- [ ] Structural search — command palette for "find functions matching [predicates]"
- [ ] Inline complexity code lens — A-F grade above function signatures

**Success criteria:** Open a project, navigate code, see structural information live. This is the demo-able milestone.

### Phase 2 — AI chat with graph context (4-6 weeks)

**Goal:** AI assistant panel with multi-provider support and graph-powered context.

- [ ] Add `lapce-ai` and `lapce-ai-providers` crates
- [ ] Implement `AiProvider` trait + Anthropic provider (Claude API)
- [ ] Implement OpenAI-compatible provider (covers OpenRouter, Ollama, z.ai)
- [ ] Add `PanelKind::AiChat` — message list, markdown rendering, streaming
- [ ] Context Assembler — graph queries → structured prompt
- [ ] Basic tool system — read_file, edit_file, grep, graph queries
- [ ] Agent loop — tool calls → execute → continue
- [ ] Inline diff rendering for AI-proposed edits (accept/reject per hunk)

**Success criteria:** Ask the AI about your code, get structurally-informed answers. Apply edits from chat.

### Phase 3 — Copilot + inline completion (2-3 weeks)

**Goal:** Tab completion and Copilot integration.

- [ ] Copilot provider — spawn language server, OAuth device flow
- [ ] Inline completion (ghost text) — extend existing `InlineCompletionData`
- [ ] Graph-enhanced completions — inject caller/callee context into completion requests

### Phase 4 — Advanced features (ongoing)

- [ ] Architectural constraint rules (declarative, checked continuously)
- [ ] Cross-project graph (shared RocksDB, already built in CodeGraph)
- [ ] Persistent memory across sessions (already built in CodeGraph)
- [ ] Branch-aware graph updates (already built in CodeGraph — BranchWatcher)
- [ ] Multi-file edit composer (checkpoint-based, like Cursor Composer)
- [ ] Shadow workspace for AI edit validation (lint/typecheck before presenting)

---

## 8. Technology Stack

| Component | Technology | Rationale |
|-----------|-----------|-----------|
| Language | Rust (edition 2024) | Same as Lapce + CodeGraph. Zero FFI. |
| UI framework | Floem (wgpu/vello) | Lapce's native UI. GPU-accelerated. |
| Editor core | lapce-xi-rope | Lapce's rope. O(log n) edits. |
| Parsing | tree-sitter | Already in Lapce (170+ grammars). Shared with graph. |
| Graph storage | codegraph (in-memory) + RocksDB (persistence) | Existing CodeGraph stack. |
| Embeddings | fastembed BGE-Small-EN-v1.5 | Already in CodeGraph. 384d ONNX, local inference. |
| HTTP client | reqwest | Async, streaming, TLS. For AI API calls. |
| Async runtime | tokio | Already used by Lapce proxy. |
| SSE parsing | eventsource-stream or manual | For AI streaming responses. |
| Serialization | serde + serde_json | Already used everywhere. |
| Terminal | alacritty_terminal | Already in Lapce. For AI tool execution. |
| Git | git2 | Already in Lapce. |
| IPC | JSON-over-stdio (lapce-rpc) | Already in Lapce. Proxy ↔ frontend. |

### External dependencies (runtime)

| Dependency | Required by | When needed |
|------------|-------------|-------------|
| Node.js | Copilot language server | Only if user enables GitHub Copilot |
| ONNX Runtime | fastembed (embeddings) | Always (bundled as dynamic library) |
| RocksDB | Persistent memory, cross-project DB | Always (statically linked) |

---

## 9. Configuration

```toml
# ~/.config/lapce-codegraph/settings.toml

[graph]
enabled = true                    # can disable for large monorepos
max_files = 10000                 # skip graph for repos > N files
persistence = true                # persist graph across sessions
cross_project = true              # enable cross-project search

[graph.rules]
# Architectural constraints (checked continuously)
# Format: "source_pattern cannot_import target_pattern"
deny = [
    "ui/** -> db/**",
    "api/handlers/** -> api/handlers/**",  # no handler-to-handler calls
]

[ai]
default_model = "openrouter/deepseek/deepseek-chat-v3-0324:free"

[ai.providers.anthropic]
api_key_cmd = "security find-generic-password -s anthropic-api-key -w"

[ai.providers.openrouter]
api_key = "sk-or-..."

[ai.providers.ollama]
base_url = "http://localhost:11434"

[ai.providers.copilot]
enabled = false

[ai.context]
token_budget = 32000              # max tokens for context assembly
include_callers = true
include_tests = true
include_memories = true
include_git_history = true
```

---

## 10. Competitive Positioning

| Capability | CodeGraph IDE | Cursor | Zed | Windsurf |
|---|---|---|---|---|
| **Code graph** | Full AST-parsed, always-live, 14 languages | None | None | AI-generated Codemaps (approximate) |
| **AI context source** | Graph queries (structural, exact) | Vector search (semantic, approximate) | Explicit @-mentions + tools | RAG + user action tracking |
| **Impact analysis** | In-process, microsecond, pre-edit | None | None | None |
| **Dead code detection** | Continuous, visual dimming | None | None | None |
| **Architectural rules** | Continuous validation, diagnostics | None | None | None |
| **AI providers** | Claude, OpenRouter, Ollama, Copilot | OpenAI, Claude, custom | OpenAI, Claude, Ollama, Bedrock | Proprietary (SWE-1.5) |
| **Runs fully local** | Yes (graph + Ollama) | No (cloud embeddings) | Partial (needs provider) | No (cloud) |
| **Performance** | GPU-rendered, ~90MB baseline | Electron (~500MB) | GPU-rendered, ~150MB | Electron (~500MB) |
| **Open source** | Yes (Apache-2.0) | No | Yes (AGPL) | No |
| **Extension ecosystem** | Minimal (WASI plugins) | VS Code extensions | Zed extensions | VS Code extensions |
| **Persistent memory** | Yes (RocksDB, code-linked) | No | No | No |
| **Cross-project search** | Yes (shared graph DB) | No | No | No |

**Thesis**: Cursor and Windsurf proved that AI-enhanced IDEs are what developers want. But their AI is blind — it searches text, not structure. CodeGraph IDE gives the AI eyes: a complete, live, queryable map of the codebase. This produces measurably better AI responses because the context is structurally relevant, not just textually similar.

---

## 11. Naming

Working title: **CodeGraph IDE** (or shorter: **Graph**, **Forge**, **Lattice**, **Nexus**)

Requirements:
- Short, memorable
- Suggests structural understanding / connections
- Available as a domain name
- Not already a popular dev tool

---

## 12. Open Questions

1. **Parser crate refactor scope**: How much work to make codegraph-typescript etc. accept a pre-built tree-sitter `Tree` instead of re-parsing? Is it worth the refactor, or is re-parsing fast enough?

2. **Graph panel rendering**: Use Floem `canvas` with manual wgpu draw calls for the architecture graph? Or build it from Floem layout primitives (`dyn_stack`, SVG)? The canvas approach is more flexible but more work.

3. **Embedding model bundling**: fastembed BGE-Small is ~30MB. Bundle with the binary, or download on first use? Affects binary size vs. first-run experience.

4. **Copilot dependency on Node.js**: Accept the Node.js runtime dependency for Copilot users? Or implement the Copilot auth flow natively in Rust? (Complex, undocumented protocol.)

5. **Monorepo support**: How does the graph handle large monorepos (100K+ files)? Lazy indexing by opened directories? User-configurable scope?

6. **Upstream contributions**: Contribute non-AI improvements back to Lapce upstream? Maintain a clean fork boundary?

---

## References

- [Lapce IDE](https://github.com/lapce/lapce) — Base IDE (Apache-2.0)
- [Floem UI framework](https://github.com/lapce/floem) — Lapce's UI framework
- [Cursor architecture](https://cursor.com/blog/instant-apply) — Fast apply / speculative edits
- [Cursor shadow workspace](https://cursor.com/blog/shadow-workspace) — Validation approach
- [Zed AI agent source](https://github.com/zed-industries/zed/tree/main/crates/agent) — Open-source reference for multi-provider AI
- [Zed ACP protocol](https://zed.dev/acp) — Agent Client Protocol
- [CodeGraph engine](https://github.com/anthropics/codegraph) — Existing graph engine
- [Structure editors (Tratt)](https://tratt.net/laurie/blog/2024/structured_editing_and_incremental_parsing.html) — Why incremental parsing > projectional editing
- [Dark's structured editor failure](https://blog.darklang.com/gpt/) — Lessons learned
- [Code Compass study](https://arxiv.org/html/2405.06271v1) — Developer comprehension pain points
