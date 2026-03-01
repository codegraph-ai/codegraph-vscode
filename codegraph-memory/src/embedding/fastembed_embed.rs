//! Fastembed wrapper for codegraph-memory
//!
//! Uses BGE-Small-EN-v1.5 (384d) via ONNX Runtime for semantic embeddings.

use crate::error::{MemoryError, Result};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use std::path::PathBuf;

/// Embedding dimension for BGE-Small-EN-v1.5
pub(crate) const EMBEDDING_DIM: usize = 384;

/// Fastembed-based text embedding model
pub(crate) struct FastembedEmbedding {
    model: TextEmbedding,
}

impl FastembedEmbedding {
    /// Create a new FastembedEmbedding with BGE-Small-EN-v1.5
    ///
    /// The model is automatically downloaded to `cache_dir` on first use.
    pub(crate) fn new(cache_dir: PathBuf) -> Result<Self> {
        // Set cache path env var to prevent fastembed from polluting CWD
        // (same pattern as tempera indexer.rs)
        std::env::set_var("FASTEMBED_CACHE_PATH", &cache_dir);

        let options = InitOptions::new(EmbeddingModel::BGESmallENV15)
            .with_cache_dir(cache_dir)
            .with_show_download_progress(true);

        let model = TextEmbedding::try_new(options)
            .map_err(|e| MemoryError::model(format!("Failed to load fastembed model: {e}")))?;

        Ok(Self { model })
    }

    /// Generate embedding for a single text
    pub(crate) fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let results = self
            .model
            .embed(vec![text.to_string()], None)
            .map_err(|e| MemoryError::embedding(format!("Embedding failed: {e}")))?;

        results
            .into_iter()
            .next()
            .ok_or_else(|| MemoryError::embedding("Empty embedding result"))
    }

    /// Generate embeddings for a batch of texts
    pub(crate) fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let owned: Vec<String> = texts.iter().map(|t| t.to_string()).collect();
        self.model
            .embed(owned, None)
            .map_err(|e| MemoryError::embedding(format!("Batch embedding failed: {e}")))
    }

    /// Get the embedding dimension (384 for BGE-Small-EN-v1.5)
    pub(crate) fn dimension(&self) -> usize {
        EMBEDDING_DIM
    }
}
