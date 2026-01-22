//! Model2Vec static embeddings
//!
//! Ultra-fast embeddings using static lookup tables (~8000 samples/sec).

use crate::error::{MemoryError, Result};
use std::path::Path;

/// Model2Vec configuration
#[derive(Debug, Clone)]
pub struct Model2VecConfig {
    /// Maximum sequence length (default: 512)
    pub max_length: usize,
    /// Whether to normalize embeddings (default: true)
    pub normalize: bool,
    /// Batch size for encoding (default: 1024)
    pub batch_size: usize,
}

impl Default for Model2VecConfig {
    fn default() -> Self {
        Self {
            max_length: 512,
            normalize: true,
            batch_size: 1024,
        }
    }
}

/// Model2Vec embedding model wrapper
///
/// Provides ultra-fast static embeddings (~8000 samples/sec).
/// Uses potion-base-8M model (256 dimensions).
pub struct Model2VecEmbedding {
    model: model2vec::Model2Vec,
    config: Model2VecConfig,
    dimension: usize,
}

impl Model2VecEmbedding {
    /// Load Model2Vec from local path
    ///
    /// Required files in the directory:
    /// - model.safetensors
    /// - tokenizer.json
    /// - config.json
    pub fn from_pretrained(model_path: &Path) -> Result<Self> {
        Self::from_pretrained_with_config(model_path, Model2VecConfig::default())
    }

    /// Load Model2Vec with custom configuration
    pub fn from_pretrained_with_config(model_path: &Path, config: Model2VecConfig) -> Result<Self> {
        let path_str = model_path
            .to_str()
            .ok_or_else(|| MemoryError::invalid_path("Invalid UTF-8 in model path"))?;

        // Validate model files exist
        let safetensors_path = model_path.join("model.safetensors");
        if !safetensors_path.exists() {
            return Err(MemoryError::model(format!(
                "Model2Vec model not found at: {}",
                safetensors_path.display()
            )));
        }

        log::info!("Loading Model2Vec from: {}", model_path.display());

        // Load model
        let model = model2vec::Model2Vec::from_pretrained(
            path_str,
            Some(config.normalize),
            None, // No subfolder
        )
        .map_err(|e| MemoryError::model(format!("Failed to load Model2Vec: {}", e)))?;

        // Get dimension by encoding test string
        let test_embed = model
            .encode(["test"])
            .map_err(|e| MemoryError::model(format!("Failed to encode test string: {}", e)))?;
        let dimension = test_embed.shape()[1];

        log::info!(
            "Loaded Model2Vec ({}d, max {} tokens, normalize: {})",
            dimension,
            config.max_length,
            config.normalize
        );

        Ok(Self {
            model,
            config,
            dimension,
        })
    }

    /// Embed single text, returns normalized vector
    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let embeddings = self
            .model
            .encode([text])
            .map_err(|e| MemoryError::embedding(format!("Failed to encode text: {}", e)))?;

        Ok(embeddings.row(0).to_vec())
    }

    /// Batch embed multiple texts (ultra-fast)
    pub fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        let embeddings = self
            .model
            .encode(texts)
            .map_err(|e| MemoryError::embedding(format!("Failed to encode texts: {}", e)))?;

        Ok(embeddings
            .rows()
            .into_iter()
            .map(|row| row.to_vec())
            .collect())
    }

    /// Get embedding dimension (256 for potion-base-8M)
    pub fn dimension(&self) -> usize {
        self.dimension
    }

    /// Get configuration
    pub fn config(&self) -> &Model2VecConfig {
        &self.config
    }
}
