//! Auto-download of Model2Vec embedding model
//!
//! Downloads the `minishlab/potion-base-8M` model from HuggingFace on first start
//! if it isn't found at `~/.codegraph/models/model2vec/`.
//! Both MCP and LSP modes share this single model location.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const HF_BASE_URL: &str = "https://huggingface.co/minishlab/potion-base-8M/resolve/main";
const MODEL_FILES: &[&str] = &["model.safetensors", "tokenizer.json", "config.json"];

/// Ensure the Model2Vec model is available at `~/.codegraph/models/model2vec/`,
/// downloading from HuggingFace if not present.
pub fn ensure_model_downloaded() -> Result<PathBuf, String> {
    let model_dir = model_dir()?;

    if has_model_files(&model_dir) {
        tracing::debug!("Model2Vec already available at: {}", model_dir.display());
        return Ok(model_dir);
    }

    download_model(&model_dir)?;
    Ok(model_dir)
}

fn model_dir() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .ok_or_else(|| "Cannot determine home directory".to_string())?;

    Ok(PathBuf::from(home)
        .join(".codegraph")
        .join("models")
        .join("model2vec"))
}

fn has_model_files(dir: &Path) -> bool {
    MODEL_FILES.iter().all(|f| dir.join(f).exists())
}

fn download_model(target_dir: &Path) -> Result<(), String> {
    let marker = target_dir.join(".downloading");

    // Clean up partial downloads from a previous interrupted attempt
    if marker.exists() {
        tracing::warn!("Found partial download, cleaning up...");
        if target_dir.exists() {
            fs::remove_dir_all(target_dir).map_err(|e| format!("Failed to clean up: {e}"))?;
        }
    }

    fs::create_dir_all(target_dir)
        .map_err(|e| format!("Failed to create {}: {e}", target_dir.display()))?;
    fs::write(&marker, "").map_err(|e| format!("Failed to write marker: {e}"))?;

    tracing::info!(
        "Downloading Model2Vec (potion-base-8M) to {}...",
        target_dir.display()
    );

    for filename in MODEL_FILES {
        let url = format!("{HF_BASE_URL}/{filename}");
        let dest = target_dir.join(filename);

        tracing::info!("  Downloading {filename}...");

        let response = ureq::get(&url)
            .call()
            .map_err(|e| format!("Failed to download {filename}: {e}"))?;

        let mut bytes = Vec::new();
        response
            .into_reader()
            .read_to_end(&mut bytes)
            .map_err(|e| format!("Failed to read {filename}: {e}"))?;

        let size_mb = bytes.len() as f64 / (1024.0 * 1024.0);
        tracing::info!("  {filename}: {size_mb:.1} MB");

        let mut file = fs::File::create(&dest)
            .map_err(|e| format!("Failed to create {}: {e}", dest.display()))?;
        file.write_all(&bytes)
            .map_err(|e| format!("Failed to write {}: {e}", dest.display()))?;
    }

    // Remove marker on success
    let _ = fs::remove_file(&marker);

    tracing::info!("Model2Vec download complete");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_model_files_nonexistent() {
        assert!(!has_model_files(Path::new("/nonexistent/path")));
    }

    #[test]
    fn test_model_dir() {
        let dir = model_dir().unwrap();
        assert!(dir.ends_with("model2vec"));
        assert!(dir.to_string_lossy().contains(".codegraph"));
    }
}
