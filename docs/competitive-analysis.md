# Competitive Analysis: CodeGraph vs Augment Code vs Cursor

> Date: 2026-02-28

## Executive Summary

CodeGraph's structural intelligence (AST-parsed dependency graphs, call graphs, impact analysis, complexity scoring, coupling metrics) is genuinely superior to both Augment Code and Cursor. Neither competitor builds a real graph from parsed code. Augment's strength is retrieval quality (custom embeddings, hierarchical curation, cross-repo intelligence). Cursor relies entirely on embeddings + grep + LSP linting with no structural understanding.

CodeGraph's biggest gaps are embedding quality, cross-repo support, and runtime dependency detection. Its biggest moats are 27 granular MCP tools, persistent memory with code links, fully-local execution, and provably correct structural analysis.

---

## Feature Comparison Matrix

| Capability | CodeGraph | Augment Code | Cursor |
|---|:---:|:---:|:---:|
| **Dependency graph** | Full (11 edge types, AST-parsed) | Proprietary (undisclosed internals) | None |
| **Call graph** | Full with configurable depth | Partial (inferred) | None |
| **Impact analysis** | 3 modes: modify/delete/rename | "Architectural drift" alerts | None |
| **Complexity scoring** | Cyclomatic + cognitive, A-F grades | None published | None |
| **Coupling metrics** | Afferent/efferent/instability | None published | None |
| **Dead code detection** | Confidence-scored, cross-file aware | None | None |
| **Persistent AI memory** | RocksDB + graph-linked, auto-invalidation | None | None (session-only) |
| **Git history mining** | Auto-extract knowledge from commits | LLM-summarized commits as context | None |
| **Entry point discovery** | Framework-aware (6 frameworks) | Unknown | None |
| **Intent-aware context** | 4 modes: explain/modify/debug/test | Single retrieval mode | Single retrieval mode |
| **MCP tool granularity** | 27 specialized tools | 1 tool (codebase-retrieval) | 6 agent tools |
| **Semantic search** | Model2Vec + BM25 + graph proximity | Custom paired embeddings + reranker | Embeddings + cross-encoder reranker |
| **Cross-repo intelligence** | Single workspace only | Multi-repo, service-to-service | Single workspace only |
| **Runtime dependency detection** | None (static analysis only) | REST, gRPC, queues, DB migrations | None |
| **Runs locally** | Fully local, no cloud | Cloud-dependent | Cloud-dependent (embeddings) |
| **Languages** | 14 (tree-sitter + rustpython) | ~14 | Via tree-sitter (chunking only) |
| **Context compression** | Raw results (except get_ai_context) | Hierarchical curation, 200K tokens | Fixed token budget chunking |
| **Branch-aware indexing** | No | Per-user, real-time branch switching | Shared indices via Merkle trees |
| **Team/shared indices** | No | Per-user with dedup across tenants | 92% reuse across teammates |
| **Incremental updates** | File watcher, 300ms debounce | ~45 seconds for changed files | 10-minute polling via Merkle diff |
| **External source integration** | None (memory stores knowledge only) | Jira, Confluence, design docs | @docs, @web |

---

## Competitor Deep Dives

### Augment Code

**Architecture**: Cloud-based Context Engine built on Google Cloud (PubSub, BigTable, AI Hypercomputer). Custom-trained embedding and retrieval models trained in pairs for maximum quality. Separate embedding strategies for code, documentation, and graph relationships.

**Indexing Pipeline**:
1. AST parsing + static analysis extracting imports, function calls, REST endpoints, gRPC stubs, queue listeners, DB migrations
2. Semantic dependency analysis building a living dependency graph
3. Commit history indexing via LLM-summarized diffs (Gemini 2.0 Flash)
4. Custom embedding generation on self-hosted NVIDIA GPUs
5. Per-user, branch-aware indices with real-time updates

**Key Differentiators**:
- **Cross-repository intelligence**: Maintains understanding of how services interact across repo boundaries. Teams report 60% reduction in cross-repo refactoring time.
- **Context Engine MCP**: Exposed as an MCP server usable by any agent. Improved agent performance by 30-80% across Claude Code, Cursor, and Codex on real-world PRs.
- **Hierarchical retrieval**: Broad service identification -> focused analysis -> dependency traversal. "Infinite Context Window" approach.
- **Runtime dependency detection**: Parses string literals for HTTP routes, gRPC service names, queue topics, DB migrations. Finds connections static analysis misses.
- **Scale**: 500K files in ~6 minutes initial index, ~45 second incremental updates. 50K files/minute throughput.

**Weaknesses**:
- Cloud-dependent (code leaves the machine for embedding)
- "Fails catastrophically when limits are exceeded" (no graceful degradation)
- Quadratic compute cost scaling
- Opaque internals (chunking, vector DB, ranking algorithms undisclosed)
- Credit-based pricing constraints (125-600 messages/month)
- No published complexity/coupling/dead-code analysis
- No persistent memory layer

**SWE-bench Pro**: #1 at 51.80% — demonstrating that context quality trumps model quality.

### Cursor

**Architecture**: RAG-based system with Turbopuffer vector database. AST-based chunking via tree-sitter, embeddings stored server-side (code never stored, only vectors + encrypted metadata).

**Indexing Pipeline**:
1. Merkle tree file discovery (hash-based change detection, 10-minute polling)
2. Tree-sitter AST chunking at logical boundaries (functions, classes)
3. Server-side embedding generation (model undisclosed, possibly Voyage Code)
4. Turbopuffer ANN storage with obfuscated file paths
5. Shared indices across teammates (92% reuse via simhash matching)

**Key Differentiators**:
- **Shadow workspace**: Hidden Electron window for LSP linting validation of proposed edits
- **Subagents**: Up to 8 parallel agents in git worktrees
- **UX polish**: Deeply integrated IDE experience
- **Shared indexing**: 7.87s → 525ms median time-to-first-query via teammate index reuse

**Weaknesses** (CodeGraph's opportunity):
- **No dependency graph** — cannot trace which files depend on which
- **No call graph** — cannot trace function call chains
- **No type flow analysis** — cannot track type propagation across modules
- **No impact analysis** — cannot predict blast radius of changes
- **No architectural awareness** — no concept of layers or module boundaries
- **No persistent memory** — every session starts from zero
- **No complexity analysis** — no code quality metrics
- **Rust not supported** in shadow workspace (rust-analyzer conflicts)
- Indices deleted after 6 weeks of inactivity

**Third-party tools like CodeGraph, Depwire, and Deep Graph MCP have emerged specifically to fill Cursor's structural gaps via MCP.**

---

## CodeGraph Strengths

### 1. Structural Intelligence (Unique)
27 MCP tools providing granular, composable access to code structure. The graph model tracks 8 node types and 11 edge types with full property support. No competitor provides this level of structural detail through a standardized protocol.

### 2. Intent-Aware Context (Unique)
`get_ai_context` selects different related symbols based on the AI's intent:
- **explain**: returns dependencies + callers (understand the code)
- **modify**: returns tests + callers (know what to update)
- **debug**: traces call chain to entry point (find the bug)
- **test**: returns example tests + mockable dependencies (write tests)

### 3. Persistent Memory with Code Links (Unique)
5 memory types (debug_context, architectural_decision, known_issue, convention, project_context) stored in RocksDB with HNSW vector index. Memories link to code graph nodes and auto-invalidate when linked code changes. No competitor has this.

### 4. Comprehensive Code Quality Metrics
- Cyclomatic + cognitive complexity with A-F grading across 14 languages
- Dead code detection with confidence scoring
- Module coupling (afferent/efferent/instability)
- Refactoring opportunity identification

### 5. Fully Local Execution
No cloud dependency. No data leaves the machine. No API keys needed for core functionality. Critical for enterprise security requirements and air-gapped environments.

### 6. MCP-Native
Works with any MCP-compatible agent: Claude Code, Cursor, Codex, Zed. Not locked to a single IDE or vendor.

---

## CodeGraph Gaps — Priority Roadmap

### Tier 1: Close Critical Gaps

**1. Better Embeddings + Reranking**
- **Current**: Model2Vec static embeddings — optimized for speed (~8000 samples/sec), not retrieval quality
- **Target**: Code-tuned embedding model (UniXcoder, Voyage Code, or CodeBERT) + cross-encoder reranking step
- **Why**: A query for "authentication logic" must reliably find `verifyJWT()` even without keyword overlap. Augment and Cursor both use specialized code embeddings with reranking.
- **Impact**: Dramatically improves `symbol_search`, `memory_search`, and any semantic query
- **Effort**: Medium — swap embedding model in indexer, add reranker to retrieval pipeline

**2. Cross-Repository Graph Linking**
- **Current**: Single workspace only
- **Target**: Index multiple repos, detect REST/gRPC/queue connections between services
- **Why**: Enterprise codebases span dozens of repos. Service-to-service dependencies are invisible to single-repo analysis. This is Augment's killer feature.
- **Impact**: Enables impact analysis across service boundaries
- **Effort**: High — needs service discovery heuristics, multi-workspace coordination, HTTP route matching

**3. Runtime Dependency Detection**
- **Current**: Pure static analysis via AST — misses all runtime connections
- **Target**: Parse string literals for HTTP routes, gRPC service names, queue topic names. Link `fetch("/api/users")` to the Express route handler in another file.
- **Why**: Modern architectures communicate via HTTP/gRPC/queues, not imports. Static-only analysis misses the most important connections in microservice architectures.
- **Impact**: Reveals the true dependency structure of distributed systems
- **Effort**: Medium — regex/heuristic scanning of string arguments in function calls

### Tier 2: Differentiate

**4. Hierarchical Context Curation**
- **Current**: Tools return raw results; the agent decides what to include
- **Target**: Build a curation pipeline: identify relevant services -> zoom into implementation -> walk dependency chain. Return a curated context package with token budget awareness.
- **Why**: Augment's "Infinite Context Window" approach outperforms raw retrieval. Raw graph data can overwhelm context windows.
- **Effort**: Medium — compose existing tools into a higher-level retrieval pipeline

**5. Change-Aware Automatic Context**
- **Current**: Agent must manually call individual tools
- **Target**: When the user is modifying code, automatically assemble: function being edited + all callers + all tests + related memories + recent git changes to that function
- **Why**: This is what makes Augment's context engine feel magical — it proactively gathers the right context. CodeGraph has all the data but requires manual tool orchestration.
- **Effort**: Low — compose existing tools into a single "what do I need to know about this edit?" endpoint

**6. Commit History as Searchable Context**
- **Current**: `mine_git_history` extracts memories from commits (one-time bootstrapping). Not queryable alongside code.
- **Target**: Make git history a first-class retrieval source. "What changed authentication last month?" returns relevant commits + affected functions + the diff context.
- **Why**: Augment's "Context Lineage" injects LLM-summarized commit history into every prompt automatically.
- **Effort**: Medium — extend git mining to maintain a searchable commit index

### Tier 3: Moat Building

**7. Architectural Layer Detection**
- **Current**: Modules are flat — no concept of "this is a service layer" vs "this is a data layer"
- **Target**: Auto-classify modules as service/controller/repository/utility/test based on naming conventions, dependency patterns, and framework usage. Enable queries like "show me all database access patterns."
- **Why**: Neither Augment nor Cursor does this structurally. Would enable architectural violation detection (UI calling DB directly, circular layer dependencies).
- **Effort**: High — needs heuristic classification + layer boundary rules

**8. Cross-Session Learning (Refine Existing Moat)**
- **Current**: Tempera BKMs + codegraph memory already provide persistent learning. No competitor has this.
- **Target**: Push harder. Auto-capture debugging strategies when errors are resolved. Surface relevant memories proactively. Track which context retrievals actually led to successful edits.
- **Why**: This is the hardest feature to replicate. A system that gets better at helping you the more you use it is the ultimate moat.
- **Effort**: Incremental — infrastructure exists, refine capture triggers and retrieval quality

**9. Multi-User Shared Graph**
- **Current**: Single-user only
- **Target**: Team-wide code intelligence. Shared graph + shared memories across developers. "The intern just debugged the same issue last week — here's what they found."
- **Why**: Augment does per-user indices with dedup. Cursor shares embedding indices. A shared structural graph + shared team memories would be more valuable than either.
- **Effort**: High — needs auth, conflict resolution, network transport

---

## Strategic Positioning

### CodeGraph's Thesis
**Context quality is the bottleneck for AI coding agents.** Augment proved this on SWE-bench Pro (#1 with identical models — only context differed). But Augment's approach is cloud-dependent, opaque, and expensive. CodeGraph provides deeper structural intelligence, runs fully local, and exposes everything through composable MCP tools that work with any agent.

### Attack Vectors
1. **Against Cursor**: "Your agent is blind. It has no map of the codebase. CodeGraph gives it eyes." Cursor's own users are already installing MCP tools like CodeGraph to fill structural gaps.
2. **Against Augment**: "Same intelligence, no cloud dependency. Your code never leaves your machine." Enterprise security teams will prefer local-first over shipping code to Google Cloud.
3. **For both**: CodeGraph as MCP server works WITH both Cursor and Claude Code. It's not a competing IDE — it's infrastructure that makes any agent better.

### Defensibility
- **27 specialized MCP tools** are hard to replicate — each encodes domain knowledge about how AI agents use code context
- **Persistent memory with code links** is a unique concept no competitor has attempted
- **14-language AST parsing** with real graph construction is months of engineering work
- **Fully local** means no ongoing cloud costs, no data privacy concerns, no vendor lock-in
