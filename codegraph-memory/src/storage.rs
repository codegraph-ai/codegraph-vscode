//! RocksDB storage with HNSW indexing
//!
//! Persistent storage for memories using RocksDB with LZ4 compression.
//! Uses instant-distance HNSW for O(log n) semantic search.

use dashmap::DashMap;
use instant_distance::{Builder, HnswMap, Point, Search};
use parking_lot::RwLock;
use rocksdb::{IteratorMode, Options, DB};
use std::path::Path;
use std::sync::Arc;

use crate::embedding::VectorEngine;
use crate::error::Result;
use crate::node::MemoryNode;

/// HNSW point wrapper for semantic search
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

/// HNSW index wrapper
struct HnswIndex {
    hnsw: HnswMap<MemoryPoint, MemoryPoint>,
}

/// RocksDB-based memory store with HNSW indexing
pub struct MemoryStore {
    db: Arc<DB>,
    memory_cache: Arc<DashMap<String, MemoryNode>>,
    vector_cache: Arc<DashMap<String, Vec<f32>>>,
    hnsw_index: Arc<RwLock<Option<HnswIndex>>>,
    hnsw_points: Arc<RwLock<Vec<MemoryPoint>>>,
    engine: Arc<VectorEngine>,
}

impl MemoryStore {
    /// Create a new MemoryStore at the given path
    pub fn new(path: impl AsRef<Path>, engine: Arc<VectorEngine>) -> Result<Self> {
        let path = path.as_ref();
        std::fs::create_dir_all(path)?;

        // Run migration if needed before opening database
        crate::migration::migrate_if_needed(path)?;

        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.set_max_background_jobs(2);
        opts.set_bytes_per_sync(1048576); // 1MB
        opts.set_compression_type(rocksdb::DBCompressionType::Lz4);

        let db = DB::open(&opts, path)?;

        log::info!("MemoryStore opened at: {}", path.display());

        let store = Self {
            db: Arc::new(db),
            memory_cache: Arc::new(DashMap::new()),
            vector_cache: Arc::new(DashMap::new()),
            hnsw_index: Arc::new(RwLock::new(None)),
            hnsw_points: Arc::new(RwLock::new(Vec::new())),
            engine,
        };

        store.load_cache()?;
        Ok(store)
    }

    /// Load existing memories into cache on startup
    fn load_cache(&self) -> Result<()> {
        let mut count = 0;
        let mut skipped = 0;
        let mut points = Vec::new();
        let iter = self.db.iterator(IteratorMode::Start);

        for item in iter {
            let (key, value) = item?;
            let key_str = String::from_utf8_lossy(&key);

            if key_str.starts_with("mem:") {
                let id = key_str.strip_prefix("mem:").unwrap().to_string();

                // Gracefully handle deserialization errors
                match bincode::deserialize::<MemoryNode>(&value) {
                    Ok(memory) => {
                        if memory.temporal.is_current() {
                            self.memory_cache.insert(id.clone(), memory);

                            // Load vector
                            if let Ok(Some(vec_bytes)) =
                                self.db.get(format!("vec:{}", id).as_bytes())
                            {
                                if let Ok(vector) = bincode::deserialize::<Vec<f32>>(&vec_bytes) {
                                    self.vector_cache.insert(id.clone(), vector.clone());
                                    points.push(MemoryPoint { id, vector });
                                }
                            }

                            count += 1;
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to deserialize memory {}: {}. Skipping.", id, e);
                        skipped += 1;
                    }
                }
            }
        }

        if count > 0 {
            log::info!("Loaded {} memories from disk", count);
            if skipped > 0 {
                log::warn!("Skipped {} memories due to deserialization errors", skipped);
            }
            if !points.is_empty() {
                self.rebuild_hnsw_index(points)?;
            }
        }

        Ok(())
    }

    /// Store a memory with embedding
    pub async fn put(&self, mut node: MemoryNode) -> Result<String> {
        let id = node.id.to_string();

        // Generate embedding if not present
        if node.embedding.is_none() {
            let text = node.searchable_text();
            let vector = self.engine.embed(&text)?;
            node.embedding = Some(vector.clone());

            // Persist vector
            let vec_key = format!("vec:{}", id);
            self.db
                .put(vec_key.as_bytes(), bincode::serialize(&vector)?)?;
            self.vector_cache.insert(id.clone(), vector.clone());

            // Update HNSW
            let mut points = self.hnsw_points.write();
            points.push(MemoryPoint {
                id: id.clone(),
                vector,
            });
            let all_points = points.clone();
            drop(points);
            self.rebuild_hnsw_index(all_points)?;
        }

        // Persist memory
        let mem_key = format!("mem:{}", id);
        self.db
            .put(mem_key.as_bytes(), bincode::serialize(&node)?)?;
        self.memory_cache.insert(id.clone(), node);

        self.db.flush()?;
        Ok(id)
    }

    /// Get a memory by ID
    pub fn get(&self, id: &str) -> Option<MemoryNode> {
        self.memory_cache.get(id).map(|e| e.clone())
    }

    /// Find memories linked to a specific code node
    pub fn find_by_code_node(&self, code_node_id: &str) -> Vec<MemoryNode> {
        self.memory_cache
            .iter()
            .filter(|entry| {
                entry
                    .value()
                    .code_links
                    .iter()
                    .any(|l| l.node_id == code_node_id)
            })
            .map(|e| e.value().clone())
            .collect()
    }

    /// Find memories with a specific tag
    pub fn find_by_tag(&self, tag: &str) -> Vec<MemoryNode> {
        self.memory_cache
            .iter()
            .filter(|entry| entry.value().tags.contains(&tag.to_string()))
            .map(|e| e.value().clone())
            .collect()
    }

    /// Invalidate a memory
    pub fn invalidate(&self, id: &str, _reason: &str) -> Result<()> {
        if let Some(mut entry) = self.memory_cache.get_mut(id) {
            entry.temporal.invalidate();
            let mem_key = format!("mem:{}", id);
            self.db
                .put(mem_key.as_bytes(), bincode::serialize(&*entry)?)?;
            self.db.flush()?;
        }
        Ok(())
    }

    /// Delete a memory permanently
    pub fn delete(&self, id: &str) -> Result<bool> {
        let removed = self.memory_cache.remove(id).is_some();
        self.vector_cache.remove(id);

        let mem_key = format!("mem:{}", id);
        let vec_key = format!("vec:{}", id);
        self.db.delete(mem_key.as_bytes())?;
        self.db.delete(vec_key.as_bytes())?;
        self.db.flush()?;

        // Rebuild HNSW without this point
        let mut points = self.hnsw_points.write();
        points.retain(|p| p.id != id);
        let all_points = points.clone();
        drop(points);
        self.rebuild_hnsw_index(all_points)?;

        Ok(removed)
    }

    /// Get all current (non-invalidated) memories
    pub fn get_all_current(&self) -> Vec<MemoryNode> {
        self.memory_cache
            .iter()
            .filter(|e| e.value().temporal.is_current())
            .map(|e| e.value().clone())
            .collect()
    }

    /// Semantic search using HNSW
    pub fn semantic_search(&self, query_vector: &[f32], limit: usize) -> Vec<(String, f32)> {
        let index_guard = self.hnsw_index.read();
        let index = match index_guard.as_ref() {
            Some(idx) => idx,
            None => return self.linear_search(query_vector, limit),
        };

        let query_point = MemoryPoint {
            id: "query".to_string(),
            vector: query_vector.to_vec(),
        };

        let mut search = Search::default();
        let points = self.hnsw_points.read();
        let mut results = Vec::new();

        for candidate in index.hnsw.search(&query_point, &mut search) {
            let point = &points[candidate.pid.into_inner() as usize];
            let similarity = cosine_similarity(query_vector, &point.vector);
            results.push((point.id.clone(), similarity));

            if results.len() >= limit {
                break;
            }
        }

        results
    }

    /// Linear search fallback
    fn linear_search(&self, query_vector: &[f32], limit: usize) -> Vec<(String, f32)> {
        let mut results: Vec<(String, f32)> = self
            .vector_cache
            .iter()
            .map(|entry| {
                let similarity = cosine_similarity(query_vector, entry.value());
                (entry.key().clone(), similarity)
            })
            .collect();

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(limit);
        results
    }

    /// Rebuild HNSW index
    fn rebuild_hnsw_index(&self, points: Vec<MemoryPoint>) -> Result<()> {
        if points.is_empty() {
            *self.hnsw_index.write() = None;
            *self.hnsw_points.write() = Vec::new();
            return Ok(());
        }

        let hnsw = Builder::default()
            .ef_construction(100)
            .build(points.clone(), points.clone());

        *self.hnsw_points.write() = points;
        *self.hnsw_index.write() = Some(HnswIndex { hnsw });

        Ok(())
    }

    /// Get store statistics
    pub fn stats(&self) -> serde_json::Value {
        use std::collections::HashMap;

        let mut by_kind: HashMap<String, i32> = HashMap::new();
        let mut by_tag: HashMap<String, i32> = HashMap::new();

        // Use the memory cache (which only contains current memories)
        for entry in self.memory_cache.iter() {
            let memory = entry.value();

            // Count by kind
            let kind_str = format!("{:?}", memory.kind).to_lowercase();
            *by_kind.entry(kind_str).or_insert(0) += 1;

            // Count by tag
            for tag in &memory.tags {
                *by_tag.entry(tag.clone()).or_insert(0) += 1;
            }
        }

        let current = self.memory_cache.len();

        serde_json::json!({
            "totalMemories": current,
            "currentMemories": current,
            "invalidatedMemories": 0,
            "byKind": by_kind,
            "byTag": by_tag,
        })
    }

    /// Get the vector engine reference
    pub fn engine(&self) -> &Arc<VectorEngine> {
        &self.engine
    }
}

/// Calculate cosine similarity between two vectors
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        assert!(cosine_similarity(&a, &b).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![-1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) + 1.0).abs() < 0.001);
    }
}
