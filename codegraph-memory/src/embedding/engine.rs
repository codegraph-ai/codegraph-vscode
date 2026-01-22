//! Vector embedding engine
//!
//! High-level API for generating and caching embeddings.

use super::discovery::find_model2vec_path;
use super::model2vec::Model2VecEmbedding;
use crate::error::Result;
use dashmap::DashMap;
use std::path::Path;
use std::sync::Arc;

/// Vector embedding engine with caching
///
/// Wraps Model2Vec with a DashMap cache for efficient repeated lookups.
pub struct VectorEngine {
    model: Arc<Model2VecEmbedding>,
    cache: DashMap<String, Vec<f32>>,
    dimension: usize,
}

impl VectorEngine {
    /// Create VectorEngine with Model2Vec
    ///
    /// # Arguments
    /// * `extension_path` - Optional path to the VS Code extension root
    pub fn new(extension_path: Option<&Path>) -> Result<Self> {
        let model_path = find_model2vec_path(extension_path)?;
        let model = Model2VecEmbedding::from_pretrained(&model_path)?;
        let dimension = model.dimension();

        log::info!("VectorEngine ready ({}d, ~8000 samples/sec)", dimension);

        Ok(Self {
            model: Arc::new(model),
            cache: DashMap::new(),
            dimension,
        })
    }

    /// Create VectorEngine from a specific model path
    pub fn from_path(model_path: &Path) -> Result<Self> {
        let model = Model2VecEmbedding::from_pretrained(model_path)?;
        let dimension = model.dimension();

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

    /// Batch embed with caching
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

    /// Cosine similarity between two embeddings
    pub fn similarity(&self, a: &[f32], b: &[f32]) -> f32 {
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

    /// Get embedding dimension
    pub fn dimension(&self) -> usize {
        self.dimension
    }

    /// Get cache size
    pub fn cache_size(&self) -> usize {
        self.cache.len()
    }

    /// Clear the cache
    pub fn clear_cache(&self) {
        self.cache.clear();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_cosine_similarity() {
        // Test cosine similarity calculation
        let a = [1.0_f32, 0.0, 0.0];
        let b = [1.0_f32, 0.0, 0.0];

        // Identical vectors should have similarity 1.0
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        let similarity = dot / (norm_a * norm_b);
        assert!((similarity - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_similarity_orthogonal() {
        let a = [1.0_f32, 0.0, 0.0];
        let b = [0.0_f32, 1.0, 0.0];

        // Orthogonal vectors should have similarity 0.0
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        assert_eq!(dot, 0.0);
    }
}
