# CodeGraph Memory Layer: Development Instructions

**Document Version:** 1.0  
**Target:** Claude Code / AI Coding Agents  
**Status:** Implementation Specification

---

## Overview

You are implementing a persistent memory layer for CodeGraph, an open-source VS Code extension that transforms codebases into queryable graph databases. This memory system stores project knowledge (architectural decisions, debugging insights, conventions) that persists across AI agent sessions.

### Project Context

CodeGraph uses:
- **Rust** for core implementation
- **RocksDB** for persistent storage (already used by the `codegraph` crate)
- **tower-lsp** for VS Code integration
- **HashMap-based indexes** for queries (no petgraph)
- **tree-sitter** for parsing

The memory layer extends CodeGraph with:
- Bi-temporal knowledge tracking (Graphiti-inspired)
- Hybrid search (BM25 + semantic + graph proximity)
- MCP tool definitions for AI agent access
- Auto-invalidation when linked code changes
- Bundled embedding model (no runtime downloads)

### Embedding Model Packaging

The semantic embedding model (`potion-base-8M`, ~32MB) is:
- Downloaded once during development/CI build
- Bundled into the vsix extension package
- Loaded from local path at runtime (no network required)
- NOT committed to git (downloaded in CI)

---

## Architecture Foundation

### Existing Crate Structure

```
codegraph/           # Core crate (RocksDB, graph storage)
codegraph-lsp/       # LSP server (tower-lsp)
codegraph-typescript/
codegraph-python/
codegraph-rust/
codegraph-go/
codegraph-c/
```

### New Crate to Create

```
codegraph-memory/
â”œâ”€â”€ Cargo.toml
â”œâ”€â”€ src/
â”‚   â”œâ”€â”€ lib.rs           # Public API exports
â”‚   â”œâ”€â”€ node.rs          # MemoryNode types and builders
â”‚   â”œâ”€â”€ storage.rs       # RocksDB integration
â”‚   â”œâ”€â”€ temporal.rs      # Bi-temporal model
â”‚   â”œâ”€â”€ embedding.rs     # model2vec integration
â”‚   â”œâ”€â”€ search.rs        # Hybrid search engine
â”‚   â”œâ”€â”€ linker.rs        # Links to code graph nodes
â”‚   â””â”€â”€ mcp.rs           # MCP tool definitions
â””â”€â”€ tests/
    â”œâ”€â”€ storage_tests.rs
    â”œâ”€â”€ search_tests.rs
    â””â”€â”€ temporal_tests.rs
```

---

## Implementation Phases

### Phase 1: Core Data Model (Week 1-2)

#### 1.1 Create `codegraph-memory/Cargo.toml`

```toml
[package]
name = "codegraph-memory"
version = "0.1.0"
edition = "2021"
license = "Apache-2.0 OR MIT"
description = "Persistent memory layer for CodeGraph"

[dependencies]
# Core CodeGraph integration
codegraph = "0.1"

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# Identifiers
uuid = { version = "1.0", features = ["v4", "serde"] }

# Temporal handling
chrono = { version = "0.4", features = ["serde"] }

# Embeddings (official Rust implementation)
model2vec-rs = "0.1"

# Async runtime
tokio = { version = "1.0", features = ["full"] }

# Error handling
thiserror = "1.0"
anyhow = "1.0"

[dev-dependencies]
tempfile = "3.0"
criterion = "0.5"
```

#### 1.2 Implement Memory Node Types (`src/node.rs`)

Create these core types:

**MemoryId:** UUID-based unique identifier for memory nodes.

**MemoryKind:** Enum with variants:
- `ArchitecturalDecision { decision, rationale, alternatives_considered, stakeholders }`
- `DebugContext { problem_description, root_cause, solution, symptoms, related_errors }`
- `KnownIssue { description, severity, workaround, tracking_id }`
- `Convention { name, description, pattern, anti_pattern }`
- `ProjectContext { topic, description, tags }`

**IssueSeverity:** Enum with `Critical`, `High`, `Medium`, `Low`, `Info`.

**TemporalMetadata:** Bi-temporal tracking struct:
- `valid_at: DateTime<Utc>` - when knowledge became true
- `invalid_at: Option<DateTime<Utc>>` - when knowledge ceased to be true
- `created_at: DateTime<Utc>` - system creation time
- `superseded_at: Option<DateTime<Utc>>` - when replaced by newer info
- `commit_hash: Option<String>` - git context
- `version_tag: Option<String>` - version context

**CodeLink:** Links memory to code graph nodes:
- `node_id: String` - reference to CodeGraph NodeId
- `node_type: LinkedNodeType` - Function/Class/Module/File/Variable/Import
- `relevance: f32` - 0.0 to 1.0 relationship strength
- `line_range: Option<(u32, u32)>` - specific lines if applicable

**MemoryNode:** The core struct:
- `id: MemoryId`
- `kind: MemoryKind`
- `title: String`
- `content: String`
- `temporal: TemporalMetadata`
- `code_links: Vec<CodeLink>`
- `embedding: Option<Vec<f32>>`
- `tags: Vec<String>`
- `source: MemorySource`
- `confidence: f32`

**MemorySource:** Enum tracking origin:
- `UserProvided { author }`
- `CodeExtracted { file_path }`
- `ConversationDerived { conversation_id }`
- `ExternalDoc { url }`

Implement a builder pattern for `MemoryNode` with fluent methods:
- `.architectural_decision(decision, rationale)`
- `.debug_context(problem, solution)`
- `.title(title)`
- `.content(content)`
- `.link_to_code(node_id, node_type)`
- `.tag(tag)`
- `.at_commit(hash)`
- `.build() -> Result<MemoryNode, _>`

#### 1.3 Implement Storage Layer (`src/storage.rs`)

Use RocksDB column families:
- `CF_MEMORIES` - main memory storage
- `CF_BY_CODE_NODE` - index: code_node_id â†’ memory_ids
- `CF_BY_TAG` - index: tag â†’ memory_ids
- `CF_EMBEDDINGS` - stored embeddings for vector search
- `CF_TEMPORAL_INDEX` - temporal queries (optional)

**MemoryStore methods:**
- `open(path) -> Result<Self>` - open/create store
- `put(&self, node: &MemoryNode) -> Result<()>` - store with indexing
- `get(&self, id: MemoryId) -> Result<Option<MemoryNode>>`
- `find_by_code_node(&self, code_node_id: &str) -> Result<Vec<MemoryNode>>`
- `find_by_tag(&self, tag: &str) -> Result<Vec<MemoryNode>>`
- `invalidate(&self, id: MemoryId, reason: &str) -> Result<()>`
- `get_all_current(&self) -> Result<Vec<MemoryNode>>` - non-invalidated only
- `get_all_embeddings(&self) -> Result<Vec<(MemoryId, Vec<f32>)>>`

**Embedding serialization helpers:**
- `embedding_to_bytes(embedding: &[f32]) -> Vec<u8>` - le bytes
- `bytes_to_embedding(bytes: &[u8]) -> Vec<f32>`

**MemoryError enum:**
- `RocksDB(rocksdb::Error)`
- `Serialization(serde_json::Error)`
- `ColumnFamilyNotFound`
- `Uuid(uuid::Error)`

---

### Phase 2: Embedding Integration (Week 2-3)

#### 2.1 Use Official `model2vec-rs` Crate (`src/embedding.rs`)

Use the official Rust implementation from MinishLab: <https://crates.io/crates/model2vec-rs>

**Add to Cargo.toml:**
```toml
[dependencies]
model2vec-rs = "0.1"
```

**Features of model2vec-rs:**
- 1.7x faster than Python implementation
- Supports f32, f16, and i8 weight types (safetensors)
- Batch processing with configurable batch size
- Configurable max sequence length

#### 2.2 Model Bundling Strategy

The embedding model is bundled with the VS Code extension, not downloaded at runtime.

**Directory structure in extension:**
```
codegraph-vscode/
â”œâ”€â”€ models/
â”‚   â””â”€â”€ potion-base-8M/
â”‚       â”œâ”€â”€ config.json
â”‚       â”œâ”€â”€ model.safetensors
â”‚       â”œâ”€â”€ tokenizer.json
â”‚       â””â”€â”€ tokenizer_config.json
â”œâ”€â”€ out/
â”‚   â””â”€â”€ codegraph-lsp (binary)
â”œâ”€â”€ package.json
â””â”€â”€ ...
```

**Build-time model download (one-time setup):**

Create `scripts/download-model.sh`:
```bash
#!/bin/bash
# Download model during development/CI build
MODEL_ID="minishlab/potion-base-8M"
MODEL_DIR="models/potion-base-8M"

mkdir -p "$MODEL_DIR"

# Download from HuggingFace
curl -L "https://huggingface.co/${MODEL_ID}/resolve/main/config.json" -o "${MODEL_DIR}/config.json"
curl -L "https://huggingface.co/${MODEL_ID}/resolve/main/model.safetensors" -o "${MODEL_DIR}/model.safetensors"
curl -L "https://huggingface.co/${MODEL_ID}/resolve/main/tokenizer.json" -o "${MODEL_DIR}/tokenizer.json"
curl -L "https://huggingface.co/${MODEL_ID}/resolve/main/tokenizer_config.json" -o "${MODEL_DIR}/tokenizer_config.json"

echo "Model downloaded to ${MODEL_DIR}"
```

**Include in vsix package (package.json):**
```json
{
  "files": [
    "out/**/*",
    "models/**/*"
  ]
}
```

**Or use .vscodeignore to explicitly include:**
```
# .vscodeignore
!models/**
```

#### 2.3 Wrapper Implementation (`src/embedding.rs`)

```rust
use model2vec::Model2Vec;
use std::path::{Path, PathBuf};

/// Wrapper around model2vec-rs for CodeGraph memory embeddings
/// 
/// The model is bundled with the extension and loaded from a local path.
pub struct MemoryEmbedder {
    model: Model2Vec,
}

impl MemoryEmbedder {
    /// Load model from the extension's bundled models directory
    /// 
    /// # Arguments
    /// * `extension_path` - Root path of the VS Code extension
    /// * `model_name` - Name of the model directory (default: "potion-base-8M")
    pub fn from_extension(
        extension_path: impl AsRef<Path>,
        model_name: Option<&str>,
    ) -> Result<Self, EmbedderError> {
        let model_name = model_name.unwrap_or("potion-base-8M");
        let model_path = extension_path.as_ref().join("models").join(model_name);
        Self::from_local(model_path)
    }
    
    /// Load from a specific local directory
    pub fn from_local(path: impl AsRef<Path>) -> Result<Self, EmbedderError> {
        let path_str = path.as_ref()
            .to_str()
            .ok_or(EmbedderError::InvalidPath)?;
        
        let model = Model2Vec::from_pretrained(path_str, None, None)
            .map_err(EmbedderError::ModelLoad)?;
        
        Ok(Self { model })
    }
    
    /// Embed a single text, returns normalized vector
    pub fn embed(&self, text: &str) -> Vec<f32> {
        let texts = vec![text.to_string()];
        let embeddings = self.model.encode(&texts);
        embeddings.into_iter().next().unwrap_or_default()
    }
    
    /// Embed multiple texts in batch
    pub fn embed_batch(&self, texts: &[&str]) -> Vec<Vec<f32>> {
        let texts: Vec<String> = texts.iter().map(|s| s.to_string()).collect();
        self.model.encode(&texts)
    }
    
    /// Embed with custom parameters
    pub fn embed_batch_with_args(
        &self, 
        texts: &[&str], 
        max_length: Option<usize>,
        batch_size: usize,
    ) -> Vec<Vec<f32>> {
        let texts: Vec<String> = texts.iter().map(|s| s.to_string()).collect();
        self.model.encode_with_args(&texts, max_length, batch_size)
    }
    
    /// Cosine similarity between two embeddings
    pub fn similarity(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() {
            return 0.0;
        }
        a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
    }
    
    /// Get embedding dimension (useful for validation)
    pub fn dimension(&self) -> usize {
        // Embed a test string to get dimension
        self.embed("test").len()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EmbedderError {
    #[error("Model loading failed: {0}")]
    ModelLoad(anyhow::Error),
    
    #[error("Invalid path encoding")]
    InvalidPath,
    
    #[error("Model directory not found: {0}")]
    ModelNotFound(PathBuf),
}
```

#### 2.4 LSP Server Initialization

The LSP server receives the extension path and initializes the embedder:

```rust
// In codegraph-lsp initialization
pub struct ServerState {
    // ... other fields
    memory_embedder: MemoryEmbedder,
    memory_store: MemoryStore,
    memory_search: Mutex<MemorySearch>,
}

impl ServerState {
    pub fn new(extension_path: PathBuf, workspace_root: PathBuf) -> Result<Self> {
        // Load bundled model
        let memory_embedder = MemoryEmbedder::from_extension(&extension_path, None)?;
        
        // Initialize memory store in workspace
        let memory_db_path = workspace_root.join(".codegraph").join("memory");
        let memory_store = MemoryStore::open(&memory_db_path)?;
        
        // Build search index
        let memory_search = MemorySearch::new(
            memory_store.clone(),
            memory_embedder.clone(),
        )?;
        
        Ok(Self {
            memory_embedder,
            memory_store,
            memory_search: Mutex::new(memory_search),
            // ...
        })
    }
}
```

#### 2.5 Recommended Model

Use `minishlab/potion-base-8M`:
- **Size:** ~32MB (fits comfortably in vsix)
- **Dimension:** 256
- **Speed:** ~0.5ms per embedding
- **Quality:** Good for code/technical content

Alternative smaller model if size is critical:
- `minishlab/potion-base-2M`: ~8MB, 256 dim, slightly lower quality

#### 2.6 CI/CD Integration

**GitHub Actions example:**
```yaml
jobs:
  build:
    steps:
      - uses: actions/checkout@v4
      
      - name: Download embedding model
        run: ./scripts/download-model.sh
      
      - name: Build Rust LSP
        run: cargo build --release -p codegraph-lsp
      
      - name: Package extension
        run: vsce package
        # models/ directory is included in vsix
```

**Testing the bundled model:**
```rust
#[test]
fn test_bundled_model_loads() {
    // In tests, use the repo root as extension path
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let extension_path = PathBuf::from(manifest_dir).parent().unwrap();
    
    let embedder = MemoryEmbedder::from_extension(extension_path, None);
    assert!(embedder.is_ok(), "Bundled model should load");
    
    let embedder = embedder.unwrap();
    assert_eq!(embedder.dimension(), 256);
}

#[test]
fn test_embedding_generation() {
    let embedder = load_test_embedder();
    
    let embedding = embedder.embed("authentication middleware");
    assert_eq!(embedding.len(), 256);
    
    // Check normalization (L2 norm should be ~1.0)
    let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
    assert!((norm - 1.0).abs() < 0.01);
}

#[test]
fn test_similarity() {
    let embedder = load_test_embedder();
    
    let emb1 = embedder.embed("user authentication login");
    let emb2 = embedder.embed("auth login user verification");
    let emb3 = embedder.embed("database schema migration");
    
    let sim_related = MemoryEmbedder::similarity(&emb1, &emb2);
    let sim_unrelated = MemoryEmbedder::similarity(&emb1, &emb3);
    
    assert!(sim_related > sim_unrelated);
}

fn load_test_embedder() -> MemoryEmbedder {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let extension_path = PathBuf::from(manifest_dir).parent().unwrap();
    MemoryEmbedder::from_extension(extension_path, None).unwrap()
}
```

---

### Phase 3: Hybrid Search (Week 3-4)

#### 3.1 Search Configuration (`src/search.rs`)

**SearchConfig struct:**
- `limit: usize` - max results (default 10)
- `bm25_weight: f32` - text weight (default 0.3)
- `semantic_weight: f32` - embedding weight (default 0.5)
- `graph_weight: f32` - code proximity weight (default 0.2)
- `current_only: bool` - filter invalidated (default true)
- `tags: Vec<String>` - tag filter
- `kinds: Vec<MemoryKindFilter>` - type filter

**MemoryKindFilter enum:** Matches MemoryKind variants for filtering.

#### 3.2 BM25 Index

**BM25Index struct:**
- `inverted: HashMap<String, Vec<(MemoryId, f32)>>` - term â†’ postings
- `doc_lengths: HashMap<MemoryId, f32>`
- `avg_doc_length: f32`
- `num_docs: usize`

**Methods:**
- `build(memories: &[MemoryNode]) -> Self`
- `tokenize(text: &str) -> Vec<String>` - lowercase, filter len > 2
- `search(&self, query: &str, limit: usize) -> Vec<(MemoryId, f32)>`

Use BM25 parameters: `k1 = 1.2`, `b = 0.75`

#### 3.3 Memory Search Engine

**MemorySearch struct:**
- `store: MemoryStore`
- `embedder: MemoryEmbedder`
- `bm25_index: BM25Index`

**Methods:**
- `new(store, embedder) -> Result<Self>` - builds BM25 index
- `rebuild_index(&mut self) -> Result<()>` - after adding memories
- `search(&self, query, code_context, config) -> Result<Vec<SearchResult>>`
- `semantic_search(&self, query_embedding, limit) -> Result<Vec<(MemoryId, f32)>>`
- `calculate_graph_score(&self, memory, code_context) -> f32`

**Search algorithm:**
1. Run BM25 search â†’ top NÃ—3 candidates
2. Run semantic search â†’ top NÃ—3 candidates
3. Merge candidates by MemoryId
4. Calculate graph proximity scores for each
5. Compute weighted final score: `bm25*w1 + semantic*w2 + graph*w3`
6. Sort by score descending
7. Apply filters (tags, kinds, current_only)
8. Return top N results

**SearchResult struct:**
- `memory: MemoryNode`
- `score: f32`
- `match_reasons: Vec<MatchReason>`

**MatchReason enum:** `TextMatch`, `SemanticSimilarity`, `CodeProximity`

---

### Phase 4: MCP Tools & LSP Integration (Week 4-5)

#### 4.1 MCP Tool Definitions (`src/mcp.rs`)

Define these tools with JSON Schema input specifications:

**codegraph_add_memory:**
```json
{
  "properties": {
    "kind": { "enum": ["architectural_decision", "debug_context", "known_issue", "convention", "project_context"] },
    "title": { "type": "string" },
    "content": { "type": "string" },
    "linkedNodes": { "type": "array", "items": { "type": "string" } },
    "tags": { "type": "array", "items": { "type": "string" } }
  },
  "required": ["kind", "title", "content"]
}
```

**codegraph_search_memory:**
```json
{
  "properties": {
    "query": { "type": "string" },
    "codeContext": { "type": "array", "items": { "type": "string" } },
    "kinds": { "type": "array", "items": { "type": "string" } },
    "tags": { "type": "array", "items": { "type": "string" } },
    "limit": { "type": "number", "default": 10 }
  },
  "required": ["query"]
}
```

**codegraph_get_context:**
```json
{
  "properties": {
    "nodeIds": { "type": "array", "items": { "type": "string" } },
    "includeRelated": { "type": "boolean", "default": true }
  },
  "required": ["nodeIds"]
}
```

**codegraph_invalidate_memory:**
```json
{
  "properties": {
    "memoryId": { "type": "string" },
    "reason": { "type": "string" }
  },
  "required": ["memoryId", "reason"]
}
```

#### 4.2 LSP Handler Integration

In `codegraph-lsp/src/handlers/`, add memory handlers:

**handle_add_memory:**
1. Parse params into MemoryNodeBuilder
2. Generate embedding via embedder
3. Store via MemoryStore
4. Rebuild search index
5. Return success + memory ID

**handle_search_memory:**
1. Build SearchConfig from params
2. Call MemorySearch.search()
3. Map results to response format
4. Return MemoryResult array

**handle_get_context:**
1. For each nodeId, call store.find_by_code_node()
2. Optionally expand to related code nodes
3. Deduplicate and return

**handle_invalidate_memory:**
1. Parse memory ID
2. Call store.invalidate()
3. Return success

---

### Phase 5: Temporal Intelligence (Week 5-6)

#### 5.1 Auto-Invalidation (`src/temporal.rs`)

**TemporalManager struct:**
- `store: MemoryStore`

**CodeChangeType enum:**
- `Deleted`
- `SignatureChanged`
- `MajorRefactor`
- `MinorEdit`

**MemoryReviewSuggestion struct:**
- `memory_id: MemoryId`
- `reason: String`
- `suggested_action: SuggestedAction`

**SuggestedAction enum:** `Invalidate`, `Review`, `Update`

**Methods:**
- `on_code_changed(&self, node_id, change_type) -> Result<Vec<MemoryReviewSuggestion>>`
- `get_knowledge_at_commit(&self, commit_hash) -> Result<Vec<MemoryNode>>`

**Auto-invalidation logic:**
- `Deleted` â†’ always suggest review
- `SignatureChanged` â†’ always suggest review
- `MajorRefactor` â†’ always suggest review
- `MinorEdit` → only if edit overlaps linked line_range

---

### Phase 6: Git History Integration (Week 6-7)

Git history provides a rich source of initial project knowledge. Instead of starting with an empty memory, CodeGraph can bootstrap the memory system by scanning and analyzing git history to extract architectural decisions, debugging insights, and conventions established over time.

#### 6.1 Git Integration Setup

**Add git2 dependency to Cargo.toml:**
```toml
[dependencies]
# Git integration
git2 = "0.19"
```

**Create new module (`src/git_history.rs`):**
```rust
use git2::{Repository, Commit, Diff, DiffOptions, Sort};
use chrono::{DateTime, Utc, TimeZone};
use std::path::Path;
use std::collections::HashMap;

use crate::node::{MemoryNode, MemoryKind, MemorySource, LinkedNodeType};
use crate::storage::MemoryStore;
use crate::temporal::TemporalMetadata;
```

#### 6.2 Core Data Structures

**GitScanConfig:** Configuration for history scanning:
- `max_commits: Option<usize>` - limit commits to scan (default: 1000)
- `since: Option<DateTime<Utc>>` - only commits after this date
- `until: Option<DateTime<Utc>>` - only commits before this date
- `include_merge_commits: bool` - whether to analyze merge commits
- `branch: Option<String>` - specific branch to scan (default: HEAD)
- `file_patterns: Vec<String>` - glob patterns to focus on (e.g., `["*.rs", "*.ts"]`)
- `commit_message_patterns: CommitPatterns` - regex patterns for classification

**CommitPatterns:** Regex patterns for classifying commits:
- `bug_fix: Vec<Regex>` - patterns like `fix:`, `bug:`, `fixes #\d+`
- `feature: Vec<Regex>` - patterns like `feat:`, `feature:`, `add:`
- `refactor: Vec<Regex>` - patterns like `refactor:`, `restructure:`, `reorganize:`
- `docs: Vec<Regex>` - patterns like `docs:`, `documentation:`
- `breaking_change: Vec<Regex>` - patterns like `BREAKING:`, `!:`
- `deprecation: Vec<Regex>` - patterns like `deprecate:`, `deprecated:`

**Default patterns (conventional commits):**
```rust
impl Default for CommitPatterns {
    fn default() -> Self {
        Self {
            bug_fix: vec![
                regex!(r"(?i)^fix(\(.+\))?[!:]"),
                regex!(r"(?i)fixes?\s+#\d+"),
                regex!(r"(?i)^bug(\(.+\))?[!:]"),
                regex!(r"(?i)\bhotfix\b"),
            ],
            feature: vec![
                regex!(r"(?i)^feat(\(.+\))?[!:]"),
                regex!(r"(?i)^feature(\(.+\))?[!:]"),
                regex!(r"(?i)^add(\(.+\))?[!:]"),
            ],
            refactor: vec![
                regex!(r"(?i)^refactor(\(.+\))?[!:]"),
                regex!(r"(?i)^restructure"),
                regex!(r"(?i)^rewrite"),
            ],
            docs: vec![
                regex!(r"(?i)^docs?(\(.+\))?[!:]"),
                regex!(r"(?i)^documentation"),
            ],
            breaking_change: vec![
                regex!(r"(?i)^.+!:"),
                regex!(r"(?i)BREAKING[\s-]?CHANGE"),
            ],
            deprecation: vec![
                regex!(r"(?i)deprecate"),
                regex!(r"(?i)^remove(\(.+\))?[!:].*(?:deprecated|legacy)"),
            ],
        }
    }
}
```

**CommitClassification:** Result of analyzing a commit:
- `commit_type: CommitType` - BugFix, Feature, Refactor, Docs, Chore, BreakingChange, Deprecation
- `scope: Option<String>` - extracted scope from conventional commit
- `summary: String` - commit subject line
- `body: Option<String>` - full commit message body
- `breaking_change_note: Option<String>` - extracted breaking change description
- `issue_refs: Vec<String>` - extracted issue/PR references (#123, JIRA-456)
- `files_changed: Vec<FileChange>` - list of changed files with change type

**FileChange:** Information about a changed file:
- `path: String` - file path
- `change_type: FileChangeType` - Added, Modified, Deleted, Renamed
- `old_path: Option<String>` - for renames, the previous path
- `additions: u32` - lines added
- `deletions: u32` - lines deleted

**GitMemoryExtraction:** Extracted memory from a commit:
- `memory_kind: MemoryKind` - which kind of memory to create
- `title: String` - generated title
- `content: String` - generated content
- `commit_hash: String` - source commit
- `commit_time: DateTime<Utc>` - when commit was made
- `author: String` - commit author
- `file_links: Vec<(String, LinkedNodeType)>` - files to link to
- `tags: Vec<String>` - auto-generated tags
- `confidence: f32` - extraction confidence (0.0-1.0)

#### 6.3 Git History Scanner

**GitHistoryScanner struct:**
```rust
pub struct GitHistoryScanner {
    repo: Repository,
    config: GitScanConfig,
    patterns: CommitPatterns,
}

impl GitHistoryScanner {
    /// Open repository at the given path
    pub fn open(repo_path: impl AsRef<Path>, config: GitScanConfig) -> Result<Self, GitError>;
    
    /// Scan history and extract memories
    pub fn scan(&self) -> Result<GitScanResult, GitError>;
    
    /// Scan a single commit
    pub fn analyze_commit(&self, commit: &Commit) -> Result<Option<CommitClassification>, GitError>;
    
    /// Extract memory from classified commit
    pub fn extract_memory(&self, classification: &CommitClassification, commit: &Commit) 
        -> Result<Option<GitMemoryExtraction>, GitError>;
    
    /// Get blame information for a file
    pub fn get_blame(&self, file_path: &str) -> Result<Vec<BlameChunk>, GitError>;
    
    /// Get file history (commits that touched this file)
    pub fn get_file_history(&self, file_path: &str, limit: usize) -> Result<Vec<FileHistoryEntry>, GitError>;
    
    /// Detect high-churn files
    pub fn detect_hotspots(&self, threshold: usize) -> Result<Vec<ChurnHotspot>, GitError>;
    
    /// Detect co-change patterns (files that change together)
    pub fn detect_coupling(&self, min_coupling: f32) -> Result<Vec<FileCoupling>, GitError>;
}
```

**GitScanResult:** Results of a full history scan:
- `memories: Vec<GitMemoryExtraction>` - extracted memories
- `hotspots: Vec<ChurnHotspot>` - high-churn files
- `couplings: Vec<FileCoupling>` - co-change patterns
- `statistics: ScanStatistics` - scan metadata
- `skipped_commits: usize` - commits that didn't produce memories
- `errors: Vec<(String, GitError)>` - non-fatal errors encountered

**ScanStatistics:**
- `total_commits: usize`
- `analyzed_commits: usize`
- `date_range: (DateTime<Utc>, DateTime<Utc>)`
- `unique_authors: usize`
- `files_touched: usize`
- `scan_duration: Duration`

#### 6.4 Memory Extraction Rules

**Rule 1: Bug Fixes → DebugContext**

When a commit matches bug fix patterns:
```rust
fn extract_debug_context(classification: &CommitClassification, commit: &Commit) -> Option<GitMemoryExtraction> {
    // Title: "Bug fix: {summary}"
    // Content: Structured from commit message
    //   - Problem: inferred from files changed + summary
    //   - Solution: commit message body or summary
    //   - Related errors: extracted keywords (error, exception, crash, fail)
    // Tags: ["debugging", "bug-fix"] + extracted scope
    // Confidence: higher if body explains the fix, lower if just summary
    
    let content = format!(
        "**Problem:** Issue affecting {}\n\n\
         **Solution:** {}\n\n\
         **Commit:** {} by {} on {}\n\n\
         {}",
        files_summary(&classification.files_changed),
        classification.summary,
        &commit.id().to_string()[..8],
        commit.author().name().unwrap_or("unknown"),
        format_time(commit.time()),
        classification.body.as_deref().unwrap_or("")
    );
    
    Some(GitMemoryExtraction {
        memory_kind: MemoryKind::DebugContext {
            problem_description: classification.summary.clone(),
            root_cause: classification.body.clone(),
            solution: extract_solution(&classification),
            symptoms: extract_error_keywords(&classification),
            related_errors: extract_issue_refs(&classification),
        },
        // ... other fields
    })
}
```

**Rule 2: Features → ArchitecturalDecision**

When a commit adds significant new functionality:
```rust
fn extract_architectural_decision(classification: &CommitClassification, commit: &Commit) -> Option<GitMemoryExtraction> {
    // Only extract if:
    // - Adds new files (not just modifications)
    // - Has meaningful commit body explaining the feature
    // - Changes more than just tests
    
    let new_files: Vec<_> = classification.files_changed
        .iter()
        .filter(|f| f.change_type == FileChangeType::Added && !is_test_file(&f.path))
        .collect();
    
    if new_files.is_empty() && classification.body.is_none() {
        return None; // Not enough context
    }
    
    Some(GitMemoryExtraction {
        memory_kind: MemoryKind::ArchitecturalDecision {
            decision: classification.summary.clone(),
            rationale: classification.body.clone().unwrap_or_default(),
            alternatives_considered: None, // Can't infer from git
            stakeholders: vec![commit.author().name().unwrap_or("unknown").to_string()],
        },
        confidence: if classification.body.is_some() { 0.7 } else { 0.5 },
        // ... other fields
    })
}
```

**Rule 3: Breaking Changes → KnownIssue**

When a commit introduces breaking changes:
```rust
fn extract_known_issue_from_breaking(classification: &CommitClassification, commit: &Commit) -> Option<GitMemoryExtraction> {
    let breaking_note = classification.breaking_change_note.as_ref()?;
    
    Some(GitMemoryExtraction {
        memory_kind: MemoryKind::KnownIssue {
            description: format!("Breaking change: {}", breaking_note),
            severity: IssueSeverity::High,
            workaround: extract_migration_hint(breaking_note),
            tracking_id: classification.issue_refs.first().cloned(),
        },
        tags: vec!["breaking-change".into(), "migration".into()],
        confidence: 0.8,
        // ... other fields
    })
}
```

**Rule 4: Deprecations → KnownIssue**

When a commit deprecates functionality:
```rust
fn extract_deprecation(classification: &CommitClassification, commit: &Commit) -> Option<GitMemoryExtraction> {
    Some(GitMemoryExtraction {
        memory_kind: MemoryKind::KnownIssue {
            description: format!("Deprecated: {}", classification.summary),
            severity: IssueSeverity::Medium,
            workaround: classification.body.clone(),
            tracking_id: None,
        },
        tags: vec!["deprecated".into(), "migration".into()],
        // ... other fields
    })
}
```

**Rule 5: Hotspots → ProjectContext**

Files with unusually high churn indicate architectural significance:
```rust
fn extract_hotspot_context(hotspot: &ChurnHotspot) -> GitMemoryExtraction {
    GitMemoryExtraction {
        memory_kind: MemoryKind::ProjectContext {
            topic: format!("High-activity area: {}", hotspot.file_path),
            description: format!(
                "This file has been modified {} times across {} commits. \
                 Recent changes: {}. This may indicate: active development, \
                 architectural complexity, or technical debt.",
                hotspot.change_count,
                hotspot.unique_commits,
                hotspot.recent_changes.join(", ")
            ),
            tags: vec!["hotspot", "architecture"],
        },
        confidence: 0.6,
        // ... other fields
    }
}
```

**Rule 6: File Coupling → Convention**

Files that frequently change together suggest conventions or dependencies:
```rust
fn extract_coupling_convention(coupling: &FileCoupling) -> Option<GitMemoryExtraction> {
    // Only extract strong couplings (>70% co-change rate)
    if coupling.coupling_strength < 0.7 {
        return None;
    }
    
    Some(GitMemoryExtraction {
        memory_kind: MemoryKind::Convention {
            name: format!("Co-change pattern: {} ↔ {}", coupling.file_a, coupling.file_b),
            description: format!(
                "These files change together in {:.0}% of commits ({} of {} times). \
                 This suggests a dependency or convention that changes to one \
                 often require changes to the other.",
                coupling.coupling_strength * 100.0,
                coupling.co_change_count,
                coupling.total_changes
            ),
            pattern: Some(format!("When modifying {}, also check {}", coupling.file_a, coupling.file_b)),
            anti_pattern: None,
        },
        confidence: coupling.coupling_strength,
        // ... other fields
    })
}
```

#### 6.5 Incremental Updates

**IncrementalScanner:** Updates memory from new commits:
```rust
pub struct IncrementalScanner {
    scanner: GitHistoryScanner,
    store: MemoryStore,
    last_scanned_commit: Option<String>,
}

impl IncrementalScanner {
    /// Scan only commits since last scan
    pub fn scan_incremental(&mut self) -> Result<IncrementalScanResult, GitError>;
    
    /// Get the last scanned commit hash
    pub fn last_scanned(&self) -> Option<&str>;
    
    /// Force rescan from a specific commit
    pub fn rescan_from(&mut self, commit_hash: &str) -> Result<IncrementalScanResult, GitError>;
}

pub struct IncrementalScanResult {
    pub new_memories: Vec<MemoryNode>,
    pub updated_memories: Vec<MemoryId>, // Memories that needed updating
    pub invalidated_memories: Vec<MemoryId>, // Memories about deleted code
    pub new_commit_range: (String, String), // from_hash..to_hash
}
```

**Tracking scan state (stored in RocksDB):**
```rust
// CF_GIT_SCAN_STATE column family
struct GitScanState {
    last_commit_hash: String,
    last_scan_time: DateTime<Utc>,
    branch: String,
    commits_scanned: usize,
}
```

#### 6.6 Code Link Resolution

**Linking extracted memories to CodeGraph nodes:**
```rust
pub struct CodeLinkResolver {
    graph: CodeGraph, // Reference to main code graph
}

impl CodeLinkResolver {
    /// Resolve file paths to CodeGraph node IDs
    pub fn resolve_file_links(
        &self,
        file_changes: &[FileChange],
    ) -> Vec<CodeLink> {
        file_changes.iter().filter_map(|change| {
            // Skip deleted files (no node to link to)
            if change.change_type == FileChangeType::Deleted {
                return None;
            }
            
            // Find the file node in the graph
            let node_id = self.graph.find_file_node(&change.path)?;
            
            Some(CodeLink {
                node_id,
                node_type: LinkedNodeType::File,
                relevance: match change.change_type {
                    FileChangeType::Added => 1.0,
                    FileChangeType::Modified => 0.8,
                    FileChangeType::Renamed => 0.6,
                    FileChangeType::Deleted => 0.0,
                },
                line_range: None, // File-level link
            })
        }).collect()
    }
    
    /// Attempt to link to specific functions/classes based on diff analysis
    pub fn resolve_symbol_links(
        &self,
        commit: &Commit,
        diff: &Diff,
    ) -> Result<Vec<CodeLink>, GitError> {
        // Parse diff to find function/class boundaries that were modified
        // Link to those specific symbols if they exist in the current graph
        // This provides more granular linking than file-level
        todo!()
    }
}
```

#### 6.7 MCP Tool Extension

**Add new MCP tool for git-based memory initialization:**

```json
{
  "name": "codegraph_initialize_memory_from_git",
  "description": "Scan git history to bootstrap memory with historical project knowledge. Extracts bug fixes, features, breaking changes, and conventions from commit history.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "maxCommits": {
        "type": "number",
        "description": "Maximum commits to scan (default: 1000)"
      },
      "since": {
        "type": "string",
        "description": "Only scan commits after this ISO date"
      },
      "branch": {
        "type": "string",
        "description": "Branch to scan (default: current HEAD)"
      },
      "includeHotspots": {
        "type": "boolean",
        "description": "Detect and record high-churn files (default: true)"
      },
      "includeCoupling": {
        "type": "boolean",
        "description": "Detect co-change patterns (default: true)"
      },
      "minConfidence": {
        "type": "number",
        "description": "Minimum confidence threshold for memory extraction (0.0-1.0, default: 0.5)"
      }
    }
  }
}
```

**Add tool for file history query:**

```json
{
  "name": "codegraph_get_file_history",
  "description": "Get the git history for a specific file, including who changed it and why.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "filePath": {
        "type": "string",
        "description": "Path to the file relative to repository root"
      },
      "limit": {
        "type": "number",
        "description": "Maximum commits to return (default: 20)"
      },
      "includeMemories": {
        "type": "boolean",
        "description": "Include any memories linked to these commits (default: true)"
      }
    },
    "required": ["filePath"]
  }
}
```

#### 6.8 LSP Handler Implementation

```rust
// Add to LSP handlers

async fn handle_initialize_memory_from_git(
    &self,
    params: InitializeMemoryFromGitParams,
) -> Result<InitializeMemoryFromGitResult, LspError> {
    let config = GitScanConfig {
        max_commits: params.max_commits,
        since: params.since.map(|s| DateTime::parse_from_rfc3339(&s).ok()).flatten().map(|d| d.with_timezone(&Utc)),
        until: None,
        include_merge_commits: false,
        branch: params.branch,
        file_patterns: vec![], // Scan all files
        commit_message_patterns: CommitPatterns::default(),
    };
    
    let scanner = GitHistoryScanner::open(&self.workspace_root, config)?;
    let scan_result = scanner.scan()?;
    
    // Filter by confidence
    let min_confidence = params.min_confidence.unwrap_or(0.5);
    let memories: Vec<_> = scan_result.memories
        .into_iter()
        .filter(|m| m.confidence >= min_confidence)
        .collect();
    
    // Resolve code links
    let resolver = CodeLinkResolver::new(&self.code_graph);
    
    // Store extracted memories
    let mut stored_count = 0;
    for extraction in memories {
        let code_links = resolver.resolve_file_links(&extraction.file_links);
        
        let memory = MemoryNode::builder()
            .kind(extraction.memory_kind)
            .title(&extraction.title)
            .content(&extraction.content)
            .source(MemorySource::CodeExtracted { 
                file_path: format!("git:{}", extraction.commit_hash) 
            })
            .at_commit(&extraction.commit_hash)
            .with_temporal(TemporalMetadata {
                valid_at: extraction.commit_time,
                created_at: Utc::now(),
                commit_hash: Some(extraction.commit_hash.clone()),
                ..Default::default()
            })
            .confidence(extraction.confidence)
            .tags(extraction.tags)
            .code_links(code_links)
            .build()?;
        
        self.memory_store.put(&memory)?;
        stored_count += 1;
    }
    
    // Store hotspots if requested
    if params.include_hotspots.unwrap_or(true) {
        for hotspot in scan_result.hotspots {
            let memory = extract_hotspot_context(&hotspot);
            self.memory_store.put(&memory.to_memory_node())?;
            stored_count += 1;
        }
    }
    
    // Store coupling patterns if requested
    if params.include_coupling.unwrap_or(true) {
        for coupling in scan_result.couplings {
            if let Some(memory) = extract_coupling_convention(&coupling) {
                self.memory_store.put(&memory.to_memory_node())?;
                stored_count += 1;
            }
        }
    }
    
    // Rebuild search index
    self.memory_search.lock().await.rebuild_index()?;
    
    Ok(InitializeMemoryFromGitResult {
        memories_created: stored_count,
        commits_analyzed: scan_result.statistics.analyzed_commits,
        hotspots_detected: scan_result.hotspots.len(),
        couplings_detected: scan_result.couplings.len(),
        date_range: scan_result.statistics.date_range,
    })
}
```

#### 6.9 Background Scanning

**Automatic incremental updates on git changes:**

```rust
// FileSystemWatcher integration
impl GitWatcher {
    /// Watch for changes to .git/HEAD and trigger incremental scan
    pub fn watch_git_changes(&self, callback: impl Fn(IncrementalScanResult)) {
        // Watch .git/refs/heads/* for branch updates
        // Watch .git/HEAD for checkout changes
        // Debounce and trigger incremental scan
    }
}

// VS Code extension hook
// In codegraph-vscode/src/extension.ts
const gitExtension = vscode.extensions.getExtension('vscode.git');
if (gitExtension) {
    const git = gitExtension.exports.getAPI(1);
    git.onDidChangeState(async (e) => {
        // Trigger incremental memory scan when git state changes
        await client.sendRequest('codegraph/incrementalMemoryScan', {});
    });
}
```

#### 6.10 Error Handling

**GitError enum:**
```rust
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("Git repository not found at {0}")]
    RepositoryNotFound(PathBuf),
    
    #[error("Not a git repository")]
    NotARepository,
    
    #[error("Failed to open repository: {0}")]
    OpenError(#[from] git2::Error),
    
    #[error("Invalid commit reference: {0}")]
    InvalidCommitRef(String),
    
    #[error("Branch not found: {0}")]
    BranchNotFound(String),
    
    #[error("Diff analysis failed: {0}")]
    DiffError(String),
    
    #[error("Scan timeout after {0:?}")]
    ScanTimeout(Duration),
    
    #[error("Memory extraction failed: {0}")]
    ExtractionError(String),
}
```

**Graceful degradation:**
- If git is not available, skip git-based memory initialization
- If a commit fails to parse, log warning and continue
- If diff analysis fails for a commit, store memory without detailed file links
- If code link resolution fails, store memory with file paths as tags instead

#### 6.11 Performance Considerations

**Performance targets:**
| Operation | Target | Notes |
|-----------|--------|-------|
| Initial scan (1000 commits) | < 30s | One-time operation |
| Incremental scan (10 commits) | < 2s | After git operations |
| File history lookup | < 100ms | Per-file query |
| Hotspot detection | < 5s | Across full history |

**Optimization strategies:**
1. **Parallel commit analysis:** Use `rayon` for parallel commit classification
2. **Lazy diff loading:** Only compute diffs for commits that match patterns
3. **Batch embedding generation:** Embed all extracted memories in batch
4. **Commit caching:** Cache parsed commit metadata in RocksDB
5. **Early termination:** Stop scanning when hitting `max_commits` or `since` date

```rust
// Parallel commit processing
use rayon::prelude::*;

let classifications: Vec<_> = commits
    .par_iter()
    .filter_map(|commit| self.analyze_commit(commit).ok().flatten())
    .collect();
```

---

## Performance Targets

| Operation | Target Latency | Notes |
|-----------|---------------|-------|
| Add memory | < 50ms | Including embedding generation |
| Search (10 results) | < 20ms | Hybrid BM25 + semantic + graph |
| Get context (single node) | < 5ms | Direct RocksDB lookup |
| Invalidate | < 10ms | Update + re-index |
| Embedding generation | < 10ms | model2vec is ~100x faster than transformers |

---

## Testing Requirements

### Unit Tests

Each module requires tests:

**storage_tests.rs:**
- `test_memory_roundtrip` - put/get cycle
- `test_find_by_code_node` - index lookup
- `test_find_by_tag` - tag index lookup
- `test_invalidation` - temporal invalidation
- `test_multiple_code_links` - multi-link handling

**search_tests.rs:**
- `test_bm25_ranking` - BM25 score ordering
- `test_semantic_similarity` - embedding similarity
- `test_hybrid_search` - combined ranking
- `test_filter_by_tags` - tag filtering
- `test_filter_by_kind` - type filtering
- `test_current_only_filter` - excludes invalidated

**temporal_tests.rs:**
- `test_temporal_metadata_defaults` - new_current()
- `test_is_current` - validity checks
- `test_was_valid_at` - point-in-time queries
- `test_auto_invalidation_suggestions` - change detection

### Integration Tests

**Full workflow test:**
```rust
#[test]
fn test_full_memory_workflow() {
    // 1. Create store
    // 2. Add memory with code links
    // 3. Generate embedding
    // 4. Search and verify found
    // 5. Invalidate
    // 6. Search and verify NOT found (current_only=true)
    // 7. Search with current_only=false and verify found
}
```

### Benchmarks

Create `benches/memory_bench.rs`:
- `bench_add_memory` - insertion performance
- `bench_search_10` - search with 10 results
- `bench_search_100` - search with 100 results
- `bench_embedding_generation` - embedding speed
- `bench_bm25_index_build` - index construction

---

## Code Style Guidelines

Follow CodeGraph conventions:

1. **Error handling:** Use `thiserror` for error types, `Result<T, Error>` returns
2. **Serialization:** Derive `Serialize, Deserialize` for all public types
3. **Documentation:** Doc comments on all public items
4. **Testing:** Minimum 80% coverage target
5. **Performance:** Use `#[inline]` for hot paths, avoid allocations in loops
6. **Naming:** Snake_case for functions/variables, CamelCase for types

---

## Implementation Checklist

### Phase 1: Core Data Model
- [ ] Create crate structure and Cargo.toml
- [ ] Implement MemoryId with UUID
- [ ] Implement MemoryKind variants
- [ ] Implement TemporalMetadata with bi-temporal methods
- [ ] Implement CodeLink and LinkedNodeType
- [ ] Implement MemoryNode with all fields
- [ ] Implement MemorySource variants
- [ ] Implement MemoryNodeBuilder with fluent API
- [ ] Implement MemoryStore with RocksDB
- [ ] Implement all indexes (by_code_node, by_tag, embeddings)
- [ ] Write storage unit tests

### Phase 2: Embeddings
- [ ] Add model2vec-rs dependency to Cargo.toml
- [ ] Create scripts/download-model.sh for build-time download
- [ ] Download potion-base-8M model to models/ directory
- [ ] Update package.json to include models/ in vsix
- [ ] Implement MemoryEmbedder wrapper struct
- [ ] Implement from_extension() for bundled model loading
- [ ] Implement from_local() for custom paths
- [ ] Implement embed() for single text
- [ ] Implement embed_batch() for multiple texts
- [ ] Implement cosine similarity helper
- [ ] Add dimension() method for validation
- [ ] Update LSP server initialization to load bundled model
- [ ] Add CI step to download model before packaging
- [ ] Write embedding unit tests with bundled model

### Phase 3: Search
- [ ] Implement SearchConfig
- [ ] Implement BM25Index with inverted index
- [ ] Implement BM25 scoring (k1=1.2, b=0.75)
- [ ] Implement MemorySearch struct
- [ ] Implement hybrid search algorithm
- [ ] Implement graph proximity scoring
- [ ] Implement result filtering
- [ ] Implement SearchResult with match reasons
- [ ] Write search unit tests

### Phase 4: MCP & LSP
- [ ] Define AddMemoryTool schema
- [ ] Define SearchMemoryTool schema
- [ ] Define GetContextTool schema
- [ ] Define InvalidateMemoryTool schema
- [ ] Implement handle_add_memory in LSP
- [ ] Implement handle_search_memory in LSP
- [ ] Implement handle_get_context in LSP
- [ ] Implement handle_invalidate_memory in LSP
- [ ] Write integration tests

### Phase 5: Temporal
- [ ] Implement TemporalManager
- [ ] Implement on_code_changed detection
- [ ] Implement get_knowledge_at_commit
- [ ] Implement auto-invalidation suggestions
- [ ] Write temporal unit tests

### Phase 6: Git History
- [ ] Add git2 dependency to Cargo.toml
- [ ] Implement GitScanConfig and CommitPatterns
- [ ] Implement CommitClassification parsing
- [ ] Implement GitHistoryScanner with scan() method
- [ ] Implement commit analysis with regex patterns
- [ ] Implement memory extraction rules:
  - [ ] Bug fixes → DebugContext
  - [ ] Features → ArchitecturalDecision
  - [ ] Breaking changes → KnownIssue
  - [ ] Deprecations → KnownIssue
  - [ ] Hotspots → ProjectContext
  - [ ] File coupling → Convention
- [ ] Implement CodeLinkResolver for file → node mapping
- [ ] Implement IncrementalScanner for ongoing updates
- [ ] Add `codegraph_initialize_memory_from_git` MCP tool
- [ ] Add `codegraph_get_file_history` MCP tool
- [ ] Implement LSP handlers for git memory tools
- [ ] Add git change watcher for automatic updates
- [ ] Implement GitError with graceful degradation
- [ ] Write unit tests for commit classification
- [ ] Write unit tests for memory extraction rules
- [ ] Write integration test for full git scan
- [ ] Add benchmarks for scan performance
- [ ] Document git memory initialization in README

### Final
- [ ] Run all benchmarks and verify targets
- [ ] Achieve 80%+ test coverage
- [ ] Document public API
- [ ] Update main codegraph-lsp to include memory handlers

---

## Reference: Example Usage

```rust
// Creating a memory
let memory = MemoryNode::builder()
    .debug_context(
        "API returns 500 on large payloads",
        "Increase nginx client_max_body_size to 10M"
    )
    .title("Nginx body size limit fix")
    .content("The /upload endpoint fails with 500 error when payload exceeds 1MB. Root cause: nginx default client_max_body_size is 1M. Solution: Add 'client_max_body_size 10M;' to nginx.conf in the server block.")
    .link_to_code("upload_handler_fn_123", LinkedNodeType::Function)
    .tag("nginx")
    .tag("infrastructure")
    .tag("debugging")
    .at_commit("abc123def")
    .build()
    .unwrap();

// Storing
store.put(&memory)?;

// Searching
let config = SearchConfig {
    limit: 5,
    bm25_weight: 0.3,
    semantic_weight: 0.5,
    graph_weight: 0.2,
    current_only: true,
    tags: vec!["nginx".to_string()],
    kinds: vec![MemoryKindFilter::DebugContext],
};

let results = search.search(
    "upload file size error",
    &["upload_handler_fn_123"],
    &config
)?;

// Invalidating
store.invalidate(memory.id, "nginx config moved to docker-compose")?;
```

---

## Questions During Implementation

If you encounter ambiguity:

1. **RocksDB integration:** The `codegraph` crate already uses RocksDB. Check if you should share the DB instance or create a separate one. Separate is safer for initial implementation.

2. **NodeId type:** The actual NodeId type from `codegraph` crate should be used instead of String where possible. Check the crate's public API.

3. **Async vs sync:** The LSP handlers are async (tower-lsp). Storage operations can be sync with blocking tasks or made async with tokio's async-compatible RocksDB wrapper.

4. **Extension path discovery:** The LSP server needs the extension installation path to find bundled models. This is typically passed via:
   - Initialize params from VS Code
   - Environment variable set by extension activation
   - Resolved from the LSP binary location

5. **Model selection:** Default to `potion-base-8M` (~32MB). If extension size becomes a concern, switch to `potion-base-2M` (~8MB) with slight quality tradeoff.

6. **Index rebuilding:** Consider incremental index updates vs full rebuilds. Start with full rebuilds, optimize later if needed.

7. **Model files in git:** The model files (~32MB) should NOT be committed to git. Add `models/` to `.gitignore` and download during CI/build. The vsix package will include them.
