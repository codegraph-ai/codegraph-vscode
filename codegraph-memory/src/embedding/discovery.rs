//! Model path discovery utilities
//!
//! Finds embedding models across different installation scenarios.

use crate::error::{MemoryError, Result};
use std::path::{Path, PathBuf};

/// Find Model2Vec model path with priority:
/// 1. CODEGRAPH_MODELS_PATH environment variable (npm package)
/// 2. Bundled location (relative to extension)
/// 3. Environment variable MODEL2VEC_PATH
/// 4. User home directory (~/.codegraph/models/model2vec)
pub fn find_model2vec_path(extension_path: Option<&Path>) -> Result<PathBuf> {
    // Priority 1: CODEGRAPH_MODELS_PATH (set by npm package wrapper)
    if let Ok(models_path) = std::env::var("CODEGRAPH_MODELS_PATH") {
        let path = PathBuf::from(&models_path);
        if path.join("model.safetensors").exists() {
            log::info!("Using CODEGRAPH_MODELS_PATH: {}", path.display());
            return Ok(path);
        }
        log::warn!(
            "CODEGRAPH_MODELS_PATH set but model not found: {}",
            models_path
        );
    }

    // Priority 2: Bundled with extension
    if let Some(ext_path) = extension_path {
        let bundled = ext_path.join("models").join("model2vec");
        if bundled.join("model.safetensors").exists() {
            log::info!("Using bundled Model2Vec: {}", bundled.display());
            return Ok(bundled);
        }
    }

    // Priority 3: Environment variable MODEL2VEC_PATH
    if let Ok(model_path) = std::env::var("MODEL2VEC_PATH") {
        let path = PathBuf::from(&model_path);
        if path.join("model.safetensors").exists() {
            log::info!("Using MODEL2VEC_PATH: {}", path.display());
            return Ok(path);
        }
        log::warn!("MODEL2VEC_PATH set but model not found: {}", model_path);
    }

    // Priority 4: User home directory
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        let user_path = PathBuf::from(home)
            .join(".codegraph")
            .join("models")
            .join("model2vec");
        if user_path.join("model.safetensors").exists() {
            log::info!("Using user Model2Vec: {}", user_path.display());
            return Ok(user_path);
        }
    }

    Err(MemoryError::model(
        "Model2Vec model not found. Checked:\n\
         - CODEGRAPH_MODELS_PATH environment variable\n\
         - Bundled location (extension/models/model2vec)\n\
         - MODEL2VEC_PATH environment variable\n\
         - ~/.codegraph/models/model2vec\n\
         \n\
         Run 'scripts/download-model.sh' to download the model.",
    ))
}

/// Get potential bundled model paths for searching
#[allow(dead_code)]
pub fn get_bundled_model2vec_paths(extension_path: Option<&Path>) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // From extension path
    if let Some(ext_path) = extension_path {
        paths.push(ext_path.join("models").join("model2vec"));
    }

    // From current working directory
    if let Ok(cwd) = std::env::current_dir() {
        paths.push(cwd.join("models").join("model2vec"));
    }

    // Common relative paths
    paths.push(PathBuf::from("models/model2vec"));
    paths.push(PathBuf::from("../models/model2vec"));

    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_bundled_paths_not_empty() {
        let paths = get_bundled_model2vec_paths(None);
        assert!(!paths.is_empty());
    }

    #[test]
    fn test_find_model_without_extension_path() {
        // This should fail gracefully when model is not present
        let result = find_model2vec_path(None);
        // Either finds the model or returns an error - both are valid
        match result {
            Ok(path) => assert!(path.exists()),
            Err(e) => assert!(e.to_string().contains("not found")),
        }
    }
}
