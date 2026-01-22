# CodeGraph Memory Layer Implementation Plan

**Created:** 2026-01-21
**Status:** Planning
**Target:** 6-7 weeks implementation

---

## Executive Summary

This plan outlines the implementation of a persistent memory layer for CodeGraph, enabling AI agents to store and retrieve project knowledge (architectural decisions, debugging insights, conventions) that persists across sessions.

### Current State Analysis

| Aspect | Current State | Target State |
|--------|--------------|--------------|
| Graph Storage | In-memory only (`CodeGraph::in_memory()`) | RocksDB persistence for memory |
| Search | BM25 text search only | Hybrid: BM25 + semantic + graph proximity |
| Embeddings | None | Bundled model2vec (potion-base-8M) |
| AI Tools | 9 Language Model Tools | +4 memory tools |
| Knowledge Persistence | None | Bi-temporal with auto-invalidation |

### Key Integration Points

1. **Backend** (`server/src/backend.rs`) - Add MemoryStore and MemorySearch
2. **Query Engine** (`server/src/ai_query/`) - Extend with memory primitives
3. **Handlers** (`server/src/handlers/`) - New memory handlers
4. **Tool Manager** (`src/ai/toolManager.ts`) - Register memory tools
5. **Types** (`src/types.ts`) - Memory request/response types

---

## Phase 1: Core Data Model (Week 1-2)

### 1.1 Create Crate Structure

**Location:** `/Users/anvanster/projects/codegraph-vscode/server/codegraph-memory/`

```
codegraph-memory/
├── Cargo.toml
├── src/
│   ├── lib.rs           # Public API exports
│   ├── node.rs          # MemoryNode types and builders
│   ├── storage.rs       # RocksDB integration
│   ├── temporal.rs      # Bi-temporal model
│   ├── embedding.rs     # model2vec integration
│   ├── search.rs        # Hybrid search engine
│   ├── linker.rs        # Links to code graph nodes
│   ├── git_history.rs   # Git commit analysis
│   └── mcp.rs           # MCP tool definitions
└── tests/
    ├── storage_tests.rs
    ├── search_tests.rs
    └── temporal_tests.rs
```

### 1.2 Dependencies (Cargo.toml)

> **Reference:** `~/projects/stellarion-main/native/rust-core/Cargo.toml`

```toml
[package]
name = "codegraph-memory"
version = "0.1.0"
edition = "2021"
license = "Apache-2.0 OR MIT"
description = "Persistent memory layer for CodeGraph"

[dependencies]
# Storage
rocksdb = { version = "0.22", features = ["multi-threaded-cf"] }

# HNSW index for O(log n) semantic search (from stellarion)
instant-distance = "0.6"

# Concurrent data structures (from stellarion)
dashmap = "5"
parking_lot = "0.12"

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
bincode = "1.3"  # Efficient binary serialization (from stellarion)

# Identifiers
uuid = { version = "1.0", features = ["v4", "serde"] }

# Temporal handling
chrono = { version = "0.4", features = ["serde"] }

# Embeddings - Model2Vec only (from stellarion)
model2vec = "0.3"

# Git integration
git2 = "0.19"

# Regex for commit parsing
regex = "1.10"
lazy_static = "1.4"

# Async runtime
tokio = { version = "1.0", features = ["full"] }

# Parallel processing
rayon = "1.8"

# Error handling
thiserror = "1.0"
anyhow = "1.0"

# Logging
log = "0.4"

[dev-dependencies]
tempfile = "3.0"
criterion = "0.5"
tokio-test = "0.4"
```

### 1.3 Core Types Implementation

#### MemoryId (`src/node.rs:1-20`)
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MemoryId(pub Uuid);

impl MemoryId {
    pub fn new() -> Self { Self(Uuid::new_v4()) }
    pub fn from_uuid(uuid: Uuid) -> Self { Self(uuid) }
}
```

#### MemoryKind (`src/node.rs:22-80`)
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MemoryKind {
    ArchitecturalDecision {
        decision: String,
        rationale: String,
        alternatives_considered: Option<Vec<String>>,
        stakeholders: Vec<String>,
    },
    DebugContext {
        problem_description: String,
        root_cause: Option<String>,
        solution: String,
        symptoms: Vec<String>,
        related_errors: Vec<String>,
    },
    KnownIssue {
        description: String,
        severity: IssueSeverity,
        workaround: Option<String>,
        tracking_id: Option<String>,
    },
    Convention {
        name: String,
        description: String,
        pattern: Option<String>,
        anti_pattern: Option<String>,
    },
    ProjectContext {
        topic: String,
        description: String,
        tags: Vec<String>,
    },
}
```

#### TemporalMetadata (`src/temporal.rs`)
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalMetadata {
    pub valid_at: DateTime<Utc>,
    pub invalid_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub superseded_at: Option<DateTime<Utc>>,
    pub commit_hash: Option<String>,
    pub version_tag: Option<String>,
}

impl TemporalMetadata {
    pub fn new_current() -> Self { ... }
    pub fn is_current(&self) -> bool { ... }
    pub fn was_valid_at(&self, time: DateTime<Utc>) -> bool { ... }
}
```

#### MemoryNode (`src/node.rs:100-150`)
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryNode {
    pub id: MemoryId,
    pub kind: MemoryKind,
    pub title: String,
    pub content: String,
    pub temporal: TemporalMetadata,
    pub code_links: Vec<CodeLink>,
    pub embedding: Option<Vec<f32>>,
    pub tags: Vec<String>,
    pub source: MemorySource,
    pub confidence: f32,
}
```

### 1.4 RocksDB Storage Layer

> **Reference Implementation:** `~/projects/stellarion-main/native/rust-core/src/memory/store.rs`

**Key Prefixes (stellarion pattern):**
- `mem:{id}` - Serialized MemoryNode (bincode)
- `vec:{id}` - Embedding vector (bincode Vec<f32>)
- `idx:code:{node_id}` - Code node → memory IDs
- `idx:tag:{tag}` - Tag → memory IDs
- `git:state` - Git scan state

**RocksDB Configuration (from stellarion):**
```rust
let mut opts = Options::default();
opts.create_if_missing(true);
opts.set_max_background_jobs(2);
opts.set_bytes_per_sync(1048576);  // 1MB
opts.set_compression_type(rocksdb::DBCompressionType::Lz4);
opts.set_use_fsync(true);
```

**Storage Implementation (`src/storage.rs`):**
```rust
use anyhow::{Context, Result};
use dashmap::DashMap;
use instant_distance::{Builder, HnswMap, Point, Search};
use parking_lot::RwLock;
use rocksdb::{IteratorMode, Options, WriteBatch, DB};
use std::sync::Arc;

/// HNSW point wrapper for O(log n) semantic search
#[derive(Clone)]
struct MemoryPoint {
    id: String,
    vector: Vec<f32>,
}

impl Point for MemoryPoint {
    fn distance(&self, other: &Self) -> f32 {
        // Cosine distance = 1 - similarity (HNSW finds minimum)
        1.0 - cosine_similarity(&self.vector, &other.vector)
    }
}

/// RocksDB + HNSW memory store (stellarion pattern)
pub struct MemoryStore {
    db: Arc<DB>,
    memory_cache: Arc<DashMap<String, MemoryNode>>,
    vector_cache: Arc<DashMap<String, Vec<f32>>>,
    hnsw_index: Arc<RwLock<Option<HnswIndex>>>,
    hnsw_points: Arc<RwLock<Vec<MemoryPoint>>>,
    engine: Arc<VectorEngine>,
    dimension: usize,
}

impl MemoryStore {
    pub fn new(path: impl AsRef<Path>, engine: Arc<VectorEngine>) -> Result<Self> {
        let path = path.as_ref();
        std::fs::create_dir_all(path)?;

        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.set_compression_type(rocksdb::DBCompressionType::Lz4);

        let db = DB::open(&opts, path)?;
        let dimension = engine.dimension();

        let store = Self {
            db: Arc::new(db),
            memory_cache: Arc::new(DashMap::new()),
            vector_cache: Arc::new(DashMap::new()),
            hnsw_index: Arc::new(RwLock::new(None)),
            hnsw_points: Arc::new(RwLock::new(Vec::new())),
            engine,
            dimension,
        };

        store.load_cache()?;
        Ok(store)
    }

    /// Load existing memories into cache on startup
    fn load_cache(&self) -> Result<()> {
        let mut points = Vec::new();
        let iter = self.db.iterator(IteratorMode::Start);

        for item in iter {
            let (key, value) = item?;
            let key_str = String::from_utf8_lossy(&key);

            if key_str.starts_with("mem:") {
                let id = key_str.strip_prefix("mem:").unwrap().to_string();
                let memory: MemoryNode = bincode::deserialize(&value)?;

                if memory.temporal.is_current() {
                    self.memory_cache.insert(id.clone(), memory);

                    // Load vector
                    if let Ok(Some(vec_bytes)) = self.db.get(format!("vec:{}", id).as_bytes()) {
                        if let Ok(vector) = bincode::deserialize::<Vec<f32>>(&vec_bytes) {
                            self.vector_cache.insert(id.clone(), vector.clone());
                            points.push(MemoryPoint { id, vector });
                        }
                    }
                }
            }
        }

        if !points.is_empty() {
            self.rebuild_hnsw_index(points)?;
        }
        Ok(())
    }

    /// Store memory with embedding
    pub async fn put(&self, mut node: MemoryNode) -> Result<String> {
        let id = node.id.to_string();

        // Generate embedding if not present
        if node.embedding.is_none() {
            let text = format!("{} {}", node.title, node.content);
            let vector = self.engine.embed(&text)?;
            node.embedding = Some(vector.clone());

            // Persist vector
            let vec_key = format!("vec:{}", id);
            self.db.put(vec_key.as_bytes(), bincode::serialize(&vector)?)?;
            self.vector_cache.insert(id.clone(), vector.clone());

            // Update HNSW
            let mut points = self.hnsw_points.write();
            points.push(MemoryPoint { id: id.clone(), vector });
            let all_points = points.clone();
            drop(points);
            self.rebuild_hnsw_index(all_points)?;
        }

        // Persist memory
        let mem_key = format!("mem:{}", id);
        self.db.put(mem_key.as_bytes(), bincode::serialize(&node)?)?;
        self.memory_cache.insert(id.clone(), node);

        self.db.flush()?;
        Ok(id)
    }

    pub fn get(&self, id: &str) -> Option<MemoryNode> {
        self.memory_cache.get(id).map(|e| e.clone())
    }

    pub fn find_by_code_node(&self, code_node_id: &str) -> Vec<MemoryNode> {
        self.memory_cache
            .iter()
            .filter(|entry| {
                entry.value().code_links.iter().any(|l| l.node_id == code_node_id)
            })
            .map(|e| e.value().clone())
            .collect()
    }

    pub fn find_by_tag(&self, tag: &str) -> Vec<MemoryNode> {
        self.memory_cache
            .iter()
            .filter(|entry| entry.value().tags.contains(&tag.to_string()))
            .map(|e| e.value().clone())
            .collect()
    }

    pub fn invalidate(&self, id: &str, reason: &str) -> Result<()> {
        if let Some(mut entry) = self.memory_cache.get_mut(id) {
            entry.temporal.invalid_at = Some(chrono::Utc::now());
            let mem_key = format!("mem:{}", id);
            self.db.put(mem_key.as_bytes(), bincode::serialize(&*entry)?)?;
        }
        Ok(())
    }

    pub fn get_all_current(&self) -> Vec<MemoryNode> {
        self.memory_cache
            .iter()
            .filter(|e| e.value().temporal.is_current())
            .map(|e| e.value().clone())
            .collect()
    }

    fn rebuild_hnsw_index(&self, points: Vec<MemoryPoint>) -> Result<()> {
        if points.is_empty() {
            *self.hnsw_index.write() = None;
            return Ok(());
        }

        let hnsw = Builder::default()
            .ef_construction(100)
            .build(points.clone(), points.clone());

        *self.hnsw_index.write() = Some(HnswIndex { hnsw });
        Ok(())
    }

    pub fn stats(&self) -> serde_json::Value {
        serde_json::json!({
            "total_memories": self.memory_cache.len(),
            "dimension": self.dimension,
            "hnsw_index_size": self.hnsw_points.read().len(),
        })
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() { return 0.0; }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 { 0.0 } else { dot / (norm_a * norm_b) }
}
```

### 1.5 Deliverables

- [ ] `codegraph-memory/Cargo.toml` with dependencies
- [ ] `src/lib.rs` with public exports
- [ ] `src/node.rs` with all memory types + builder pattern
- [ ] `src/temporal.rs` with bi-temporal logic
- [ ] `src/storage.rs` with RocksDB integration
- [ ] `tests/storage_tests.rs` with roundtrip tests

---

## Phase 2: Embedding Integration (Week 2-3)

> **Reference Implementation:** `~/projects/stellarion-main/native/rust-core/src/embeddings/model2vec.rs`

### 2.1 Model Bundling Strategy

**Model Discovery Priority (from stellarion):**
1. Bundled location (`models/model2vec/`)
2. Environment variable `MODEL2VEC_PATH`
3. User home directory (`~/.codegraph/models/model2vec`)

**Build-time Model Download:**

Create `scripts/download-model.sh`:
```bash
#!/bin/bash
MODEL_ID="minishlab/potion-base-8M"
MODEL_DIR="models/model2vec"

mkdir -p "$MODEL_DIR"

# Required files for model2vec crate
curl -L "https://huggingface.co/${MODEL_ID}/resolve/main/config.json" \
     -o "${MODEL_DIR}/config.json"
curl -L "https://huggingface.co/${MODEL_ID}/resolve/main/model.safetensors" \
     -o "${MODEL_DIR}/model.safetensors"
curl -L "https://huggingface.co/${MODEL_ID}/resolve/main/tokenizer.json" \
     -o "${MODEL_DIR}/tokenizer.json"
curl -L "https://huggingface.co/${MODEL_ID}/resolve/main/tokenizer_config.json" \
     -o "${MODEL_DIR}/tokenizer_config.json"

echo "Model downloaded to ${MODEL_DIR}"
```

**Directory Structure:**
```
codegraph-vscode/
├── models/
│   └── model2vec/         # ~15MB (potion-base-8M)
│       ├── config.json
│       ├── model.safetensors
│       ├── tokenizer.json
│       └── tokenizer_config.json
```

**Update .gitignore:**
```
models/
```

**Update .vscodeignore:**
```
!models/**
```

### 2.2 Model Discovery (`src/embedding/discovery.rs`)

```rust
//! Model path discovery (pattern from stellarion)
use anyhow::Result;
use std::path::PathBuf;

/// Find Model2Vec model path with priority:
/// 1. Bundled location (relative to extension)
/// 2. Environment variable MODEL2VEC_PATH
/// 3. User home directory (~/.codegraph/models/model2vec)
pub fn find_model2vec_path(extension_path: Option<&Path>) -> Result<PathBuf> {
    // Priority 1: Bundled with extension
    if let Some(ext_path) = extension_path {
        let bundled = ext_path.join("models").join("model2vec");
        if bundled.join("model.safetensors").exists() {
            eprintln!("📦 Using bundled Model2Vec: {}", bundled.display());
            return Ok(bundled);
        }
    }

    // Priority 2: Environment variable
    if let Ok(model_path) = std::env::var("MODEL2VEC_PATH") {
        let path = PathBuf::from(&model_path);
        if path.join("model.safetensors").exists() {
            eprintln!("🔧 Using MODEL2VEC_PATH: {}", path.display());
            return Ok(path);
        }
    }

    // Priority 3: User home directory
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        let user_path = PathBuf::from(home)
            .join(".codegraph")
            .join("models")
            .join("model2vec");
        if user_path.join("model.safetensors").exists() {
            eprintln!("🏠 Using user Model2Vec: {}", user_path.display());
            return Ok(user_path);
        }
    }

    Err(anyhow::anyhow!(
        "Model2Vec model not found. Run 'scripts/download-model.sh' to download."
    ))
}
```

### 2.3 Embedder Implementation (`src/embedding/model2vec.rs`)

```rust
//! Model2Vec embedding (pattern from stellarion)
use anyhow::{anyhow, Context, Result};
use std::path::Path;
use std::sync::Arc;

/// Model2Vec configuration
#[derive(Debug, Clone)]
pub struct Model2VecConfig {
    pub max_length: usize,   // default: 512
    pub normalize: bool,     // default: true
    pub batch_size: usize,   // default: 1024
}

impl Default for Model2VecConfig {
    fn default() -> Self {
        Self { max_length: 512, normalize: true, batch_size: 1024 }
    }
}

/// Model2Vec embedding model (~8000 samples/sec)
pub struct Model2VecEmbedding {
    model: model2vec::Model2Vec,
    config: Model2VecConfig,
    dimension: usize,
}

impl Model2VecEmbedding {
    /// Load from local path (required files: model.safetensors, tokenizer.json, config.json)
    pub fn from_pretrained(model_path: &Path) -> Result<Self> {
        Self::from_pretrained_with_config(model_path, Model2VecConfig::default())
    }

    pub fn from_pretrained_with_config(model_path: &Path, config: Model2VecConfig) -> Result<Self> {
        let path_str = model_path.to_str()
            .ok_or_else(|| anyhow!("Invalid model path"))?;

        // Validate model files exist
        let safetensors_path = model_path.join("model.safetensors");
        if !safetensors_path.exists() {
            return Err(anyhow!(
                "Model2Vec model not found at: {}",
                safetensors_path.display()
            ));
        }

        // Load model (from stellarion pattern)
        let model = model2vec::Model2Vec::from_pretrained(
            path_str,
            Some(config.normalize),
            None,  // No subfolder
        ).context("Failed to load Model2Vec model")?;

        // Get dimension by encoding test string
        let test_embed = model.encode(&["test"])
            .context("Failed to encode test string")?;
        let dimension = test_embed.shape()[1];

        Ok(Self { model, config, dimension })
    }

    /// Embed single text (returns normalized 256d vector)
    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let embeddings = self.model.encode(&[text])
            .context("Failed to encode text")?;
        Ok(embeddings.row(0).to_vec())
    }

    /// Batch embed (ultra-fast, ~8000 samples/sec)
    pub fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }
        let embeddings = self.model.encode(texts)
            .context("Failed to encode texts")?;
        Ok(embeddings.rows().into_iter().map(|row| row.to_vec()).collect())
    }

    pub fn dimension(&self) -> usize { self.dimension }
}

/// Thread-safe wrapper (Arc + Mutex pattern from stellarion)
pub struct ThreadSafeModel2Vec {
    model: Arc<std::sync::Mutex<Model2VecEmbedding>>,
}

impl ThreadSafeModel2Vec {
    pub fn new(model: Model2VecEmbedding) -> Self {
        Self { model: Arc::new(std::sync::Mutex::new(model)) }
    }

    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let guard = self.model.lock().map_err(|e| anyhow!("Lock poisoned: {}", e))?;
        guard.embed(text)
    }

    pub fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let guard = self.model.lock().map_err(|e| anyhow!("Lock poisoned: {}", e))?;
        guard.embed_batch(texts)
    }

    pub fn dimension(&self) -> usize {
        self.model.lock().map(|g| g.dimension()).unwrap_or(256)
    }
}
```

### 2.4 VectorEngine Wrapper (`src/embedding/engine.rs`)

```rust
//! Simplified VectorEngine - Model2Vec only (no fallbacks)
use anyhow::Result;
use dashmap::DashMap;
use std::path::Path;
use std::sync::Arc;

use super::discovery::find_model2vec_path;
use super::model2vec::Model2VecEmbedding;

/// Vector embedding engine (Model2Vec only)
pub struct VectorEngine {
    model: Arc<Model2VecEmbedding>,
    cache: DashMap<String, Vec<f32>>,
    dimension: usize,
}

impl VectorEngine {
    /// Create VectorEngine with Model2Vec
    pub fn new(extension_path: Option<&Path>) -> Result<Self> {
        let model_path = find_model2vec_path(extension_path)?;
        let model = Model2VecEmbedding::from_pretrained(&model_path)?;
        let dimension = model.dimension();

        eprintln!("✅ Loaded Model2Vec ({}d, static embeddings)", dimension);
        eprintln!("✅ Semantic search: READY (~8000 samples/sec)");

        Ok(Self {
            model: Arc::new(model),
            cache: DashMap::new(),
            dimension,
        })
    }

    /// Generate embedding with caching
    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        // Check cache first
        if let Some(cached) = self.cache.get(text) {
            return Ok(cached.clone());
        }

        // Generate and cache
        let embedding = self.model.embed(text)?;
        self.cache.insert(text.to_string(), embedding.clone());
        Ok(embedding)
    }

    /// Batch embed with parallel caching
    pub fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        // Check cache for all texts
        let mut results: Vec<Option<Vec<f32>>> = texts
            .iter()
            .map(|text| self.cache.get(*text).map(|v| v.clone()))
            .collect();

        // Find uncached texts
        let uncached: Vec<(usize, &str)> = results
            .iter()
            .enumerate()
            .filter(|(_, cached)| cached.is_none())
            .map(|(i, _)| (i, texts[i]))
            .collect();

        if uncached.is_empty() {
            return Ok(results.into_iter().flatten().collect());
        }

        // Batch embed uncached texts
        let uncached_texts: Vec<&str> = uncached.iter().map(|(_, t)| *t).collect();
        let new_embeddings = self.model.embed_batch(&uncached_texts)?;

        // Update cache and results
        for ((idx, text), emb) in uncached.iter().zip(new_embeddings.into_iter()) {
            self.cache.insert(text.to_string(), emb.clone());
            results[*idx] = Some(emb);
        }

        Ok(results.into_iter().flatten().collect())
    }

    /// Cosine similarity
    pub fn similarity(&self, a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() { return 0.0; }
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm_a == 0.0 || norm_b == 0.0 { 0.0 } else { dot / (norm_a * norm_b) }
    }

    pub fn dimension(&self) -> usize { self.dimension }
    pub fn cache_size(&self) -> usize { self.cache.len() }
}
```

### 2.5 Backend Integration

**Update `server/src/backend.rs`:**
```rust
pub struct CodeGraphBackend {
    // ... existing fields
    pub memory_engine: Arc<VectorEngine>,
    pub memory_store: Arc<MemoryStore>,
    pub memory_search: Arc<Mutex<MemorySearch>>,
}

impl CodeGraphBackend {
    pub fn new(extension_path: PathBuf, workspace_root: PathBuf) -> Result<Self> {
        // Load Model2Vec engine
        let memory_engine = Arc::new(
            VectorEngine::new(Some(&extension_path))?
        );

        // Initialize memory store with RocksDB
        let memory_db_path = workspace_root.join(".codegraph").join("memory");
        let memory_store = Arc::new(MemoryStore::new(&memory_db_path, memory_engine.clone())?);

        // Build search index
        let memory_search = Arc::new(Mutex::new(
            MemorySearch::new(memory_store.clone(), memory_engine.clone())?
        ));

        // ... rest of initialization
    }
}
```

### 2.6 Deliverables

- [ ] `scripts/download-model.sh` script
- [ ] Update `.gitignore` to exclude models/
- [ ] Update `.vscodeignore` to include models/ in vsix
- [ ] `src/embedding.rs` with MemoryEmbedder wrapper
- [ ] Update CI workflow to download model
- [ ] Tests for embedding generation and similarity

---

## Phase 3: Hybrid Search (Week 3-4)

### 3.1 Search Configuration

```rust
#[derive(Debug, Clone)]
pub struct SearchConfig {
    pub limit: usize,           // default 10
    pub bm25_weight: f32,       // default 0.3
    pub semantic_weight: f32,   // default 0.5
    pub graph_weight: f32,      // default 0.2
    pub current_only: bool,     // default true
    pub tags: Vec<String>,
    pub kinds: Vec<MemoryKindFilter>,
}
```

### 3.2 BM25 Index

Reuse pattern from existing `server/src/ai_query/text_index.rs`:

```rust
pub struct BM25Index {
    inverted: HashMap<String, Vec<(MemoryId, f32)>>,
    doc_lengths: HashMap<MemoryId, f32>,
    avg_doc_length: f32,
    num_docs: usize,
}

impl BM25Index {
    pub fn build(memories: &[MemoryNode]) -> Self;
    pub fn search(&self, query: &str, limit: usize) -> Vec<(MemoryId, f32)>;
}
```

### 3.3 Hybrid Search Engine

```rust
pub struct MemorySearch {
    store: Arc<MemoryStore>,
    embedder: Arc<MemoryEmbedder>,
    bm25_index: BM25Index,
}

impl MemorySearch {
    pub fn new(store: Arc<MemoryStore>, embedder: Arc<MemoryEmbedder>) -> Result<Self>;

    pub fn rebuild_index(&mut self) -> Result<()>;

    pub fn search(
        &self,
        query: &str,
        code_context: &[String],  // Current code node IDs
        config: &SearchConfig,
    ) -> Result<Vec<SearchResult>>;
}
```

**Search Algorithm:**
1. BM25 search → top N×3 candidates
2. Semantic search (embed query, compare) → top N×3 candidates
3. Merge candidates by MemoryId
4. Calculate graph proximity scores (linked code distance)
5. Weighted final score: `bm25*0.3 + semantic*0.5 + graph*0.2`
6. Apply filters (tags, kinds, current_only)
7. Return top N results

### 3.4 Deliverables

- [ ] `src/search.rs` with SearchConfig, SearchResult
- [ ] BM25Index implementation
- [ ] MemorySearch with hybrid algorithm
- [ ] Graph proximity scoring
- [ ] `tests/search_tests.rs`

---

## Phase 4: MCP Tools & LSP Integration (Week 4-5)

### 4.1 MCP Tool Definitions

**4 New Tools:**

| Tool | Purpose | Input | Output |
|------|---------|-------|--------|
| `codegraph_add_memory` | Store new memory | kind, title, content, links, tags | memory_id |
| `codegraph_search_memory` | Hybrid search | query, context, filters, limit | MemoryResult[] |
| `codegraph_get_context` | Get memory for code | nodeIds, includeRelated | MemoryNode[] |
| `codegraph_invalidate_memory` | Mark obsolete | memoryId, reason | success |

### 4.2 LSP Handler Implementation

**Create `server/src/handlers/memory.rs`:**

```rust
pub async fn handle_add_memory(
    backend: &CodeGraphBackend,
    params: AddMemoryParams,
) -> Result<AddMemoryResponse, LspError> {
    // 1. Build MemoryNode from params
    // 2. Generate embedding
    // 3. Store in RocksDB
    // 4. Rebuild search index
    // 5. Return memory ID
}

pub async fn handle_search_memory(
    backend: &CodeGraphBackend,
    params: SearchMemoryParams,
) -> Result<Vec<MemoryResult>, LspError> {
    // 1. Build SearchConfig
    // 2. Execute hybrid search
    // 3. Map to response format
}

pub async fn handle_get_context(
    backend: &CodeGraphBackend,
    params: GetContextParams,
) -> Result<Vec<MemoryNode>, LspError> {
    // 1. For each nodeId, find linked memories
    // 2. Optionally expand to related nodes
    // 3. Deduplicate and return
}

pub async fn handle_invalidate_memory(
    backend: &CodeGraphBackend,
    params: InvalidateMemoryParams,
) -> Result<InvalidateResponse, LspError> {
    // 1. Parse memory ID
    // 2. Call store.invalidate()
    // 3. Return success
}
```

### 4.3 TypeScript Tool Registration

**Update `src/ai/toolManager.ts`:**

```typescript
// Add memory tools
private registerMemoryTools(): void {
    // codegraph_add_memory
    this.registerTool({
        name: 'codegraph_add_memory',
        description: 'Store project knowledge...',
        inputSchema: {
            type: 'object',
            properties: {
                kind: { enum: ['architectural_decision', 'debug_context', ...] },
                title: { type: 'string' },
                content: { type: 'string' },
                linkedNodes: { type: 'array', items: { type: 'string' } },
                tags: { type: 'array', items: { type: 'string' } }
            },
            required: ['kind', 'title', 'content']
        },
        invoke: async (params) => this.invokeAddMemory(params)
    });

    // ... register other 3 tools
}
```

### 4.4 TypeScript Type Definitions

**Update `src/types.ts`:**

```typescript
// Memory Types
export interface AddMemoryParams {
    kind: MemoryKind;
    title: string;
    content: string;
    linkedNodes?: string[];
    tags?: string[];
}

export interface SearchMemoryParams {
    query: string;
    codeContext?: string[];
    kinds?: MemoryKind[];
    tags?: string[];
    limit?: number;
}

export interface MemoryResult {
    id: string;
    kind: MemoryKind;
    title: string;
    content: string;
    score: number;
    matchReasons: MatchReason[];
    codeLinks: CodeLink[];
    temporal: TemporalMetadata;
}

export type MemoryKind =
    | 'architectural_decision'
    | 'debug_context'
    | 'known_issue'
    | 'convention'
    | 'project_context';

export type MatchReason = 'text_match' | 'semantic_similarity' | 'code_proximity';
```

### 4.5 Deliverables

- [ ] `src/mcp.rs` with tool definitions
- [ ] `server/src/handlers/memory.rs` with 4 handlers
- [ ] Update `server/src/handlers/mod.rs` to export
- [ ] Update `src/ai/toolManager.ts` to register tools
- [ ] Update `src/types.ts` with memory types
- [ ] Integration tests

---

## Phase 5: Temporal Intelligence (Week 5-6)

### 5.1 Auto-Invalidation System

```rust
pub struct TemporalManager {
    store: Arc<MemoryStore>,
}

impl TemporalManager {
    /// Called when code changes, returns memories needing review
    pub fn on_code_changed(
        &self,
        node_id: &str,
        change_type: CodeChangeType,
    ) -> Result<Vec<MemoryReviewSuggestion>>;

    /// Get knowledge valid at a specific commit
    pub fn get_knowledge_at_commit(
        &self,
        commit_hash: &str,
    ) -> Result<Vec<MemoryNode>>;
}
```

### 5.2 Integration with FileWatcher

**Update `server/src/watcher.rs`:**

```rust
// When file changes detected:
async fn on_file_changed(&self, path: &Path) {
    // 1. Reparse file
    // 2. Identify changed symbols
    // 3. Call temporal_manager.on_code_changed() for each
    // 4. Notify client of memories needing review
}
```

### 5.3 Deliverables

- [ ] `src/temporal.rs` with TemporalManager
- [ ] CodeChangeType detection
- [ ] MemoryReviewSuggestion generation
- [ ] Integration with file watcher
- [ ] `tests/temporal_tests.rs`

---

## Phase 6: Git History Integration (Week 6-7)

### 6.1 Git Scanner Implementation

```rust
pub struct GitHistoryScanner {
    repo: Repository,
    config: GitScanConfig,
    patterns: CommitPatterns,
}

impl GitHistoryScanner {
    pub fn open(repo_path: impl AsRef<Path>, config: GitScanConfig) -> Result<Self>;

    /// Full history scan
    pub fn scan(&self) -> Result<GitScanResult>;

    /// Analyze single commit
    pub fn analyze_commit(&self, commit: &Commit) -> Result<Option<CommitClassification>>;

    /// Extract memory from classified commit
    pub fn extract_memory(&self, classification: &CommitClassification) -> Result<Option<GitMemoryExtraction>>;
}
```

### 6.2 Memory Extraction Rules

| Commit Type | Memory Kind | Confidence |
|-------------|-------------|------------|
| Bug fix (fix:, fixes #123) | DebugContext | 0.7-0.9 |
| Feature (feat:, add:) | ArchitecturalDecision | 0.5-0.7 |
| Breaking change (BREAKING:) | KnownIssue (High) | 0.8 |
| Deprecation (deprecate:) | KnownIssue (Medium) | 0.7 |
| High-churn files | ProjectContext | 0.6 |
| Co-change patterns (>70%) | Convention | 0.7-1.0 |

### 6.3 New MCP Tools

**codegraph_initialize_memory_from_git:**
```json
{
    "name": "codegraph_initialize_memory_from_git",
    "inputSchema": {
        "properties": {
            "maxCommits": { "type": "number", "default": 1000 },
            "since": { "type": "string", "description": "ISO date" },
            "branch": { "type": "string" },
            "includeHotspots": { "type": "boolean", "default": true },
            "includeCoupling": { "type": "boolean", "default": true },
            "minConfidence": { "type": "number", "default": 0.5 }
        }
    }
}
```

**codegraph_get_file_history:**
```json
{
    "name": "codegraph_get_file_history",
    "inputSchema": {
        "properties": {
            "filePath": { "type": "string" },
            "limit": { "type": "number", "default": 20 },
            "includeMemories": { "type": "boolean", "default": true }
        },
        "required": ["filePath"]
    }
}
```

### 6.4 Performance Targets

| Operation | Target | Notes |
|-----------|--------|-------|
| Initial scan (1000 commits) | < 30s | One-time operation |
| Incremental scan (10 commits) | < 2s | After git operations |
| File history lookup | < 100ms | Per-file query |

### 6.5 Deliverables

- [ ] `src/git_history.rs` with GitHistoryScanner
- [ ] CommitPatterns with regex patterns
- [ ] Memory extraction rules
- [ ] IncrementalScanner for ongoing updates
- [ ] 2 new MCP tools
- [ ] LSP handlers for git tools
- [ ] Git change watcher integration
- [ ] Performance benchmarks

---

## Integration Checklist

### Workspace Configuration

- [ ] Add `codegraph-memory` to workspace members in root `Cargo.toml`
- [ ] Update `server/Cargo.toml` to depend on `codegraph-memory`

### CI/CD Updates

- [ ] Add model download step to `.github/workflows/ci.yml`
- [ ] Update `scripts/build-binaries.sh` to include model
- [ ] Update `scripts/build.sh` to call download script

### Testing

- [ ] Unit tests for all modules (80%+ coverage)
- [ ] Integration tests for full workflows
- [ ] Benchmarks for performance targets

### Documentation

- [ ] Update README with memory feature docs
- [ ] Document MCP tools in AI_TOOL_EXAMPLES.md
- [ ] Add memory API to extension documentation

---

## Risk Mitigation

| Risk | Impact | Mitigation |
|------|--------|------------|
| model2vec-rs API changes | High | Pin version, fallback to Python wrapper |
| RocksDB conflicts with CodeGraph | Medium | Separate DB instance, different path |
| Extension size (32MB model) | Medium | Option for potion-base-2M (8MB) |
| Git scan performance | Low | Parallel processing, lazy diffs |
| Memory exhaustion | Medium | LRU cache for embeddings, streaming |

---

## Success Criteria

1. **Functionality:** All 6 MCP tools working via Language Model Tools API
2. **Performance:** Search < 20ms, add memory < 50ms, embedding < 10ms
3. **Quality:** 80%+ test coverage, no clippy warnings
4. **Integration:** Works with existing 9 AI tools seamlessly
5. **Persistence:** Memory survives VS Code restart
6. **Scalability:** Handles 10,000+ memories efficiently

---

## Next Steps

1. **Start Phase 1:** Create crate structure and core types
2. **Verify Dependencies:** Ensure model2vec-rs and git2 work in environment
3. **Set Up CI:** Add model download to build pipeline
4. **Begin TDD:** Write tests before implementation per project convention
