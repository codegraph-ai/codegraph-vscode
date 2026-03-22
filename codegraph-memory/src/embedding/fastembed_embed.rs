//! Fastembed wrapper for codegraph-memory
//!
//! Uses Jina Code V2 (768d) via ONNX Runtime for code-aware semantic embeddings.
//! Trained on 150M+ code Q&A and docstring-source pairs across 30 languages.

use crate::error::{MemoryError, Result};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use std::path::PathBuf;

/// Embedding dimension for Jina Code V2
pub(crate) const EMBEDDING_DIM: usize = 768;

/// ONNX Runtime version required by ort-sys 2.0.0-rc.9
#[cfg(target_os = "windows")]
const ORT_VERSION: &str = "1.20.0";

/// Fastembed-based text embedding model
pub(crate) struct FastembedEmbedding {
    model: TextEmbedding,
}

impl FastembedEmbedding {
    /// Create a new FastembedEmbedding with Jina Code V2
    ///
    /// The model is automatically downloaded to `cache_dir` on first use (~162MB quantized ONNX).
    /// On Windows, also ensures onnxruntime.dll is available (downloaded if needed).
    pub(crate) fn new(cache_dir: PathBuf) -> Result<Self> {
        // MUST set FASTEMBED_CACHE_DIR before InitOptions::new() — its Default impl
        // calls get_cache_dir() which falls back to ".fastembed_cache" in CWD.
        // Note: the env var is FASTEMBED_CACHE_DIR (not _PATH).
        unsafe { std::env::set_var("FASTEMBED_CACHE_DIR", &cache_dir) };

        // On Windows with ort-load-dynamic, ensure onnxruntime.dll is available
        #[cfg(target_os = "windows")]
        ensure_ort_dll(&cache_dir)?;

        let options = InitOptions::new(EmbeddingModel::JinaEmbeddingsV2BaseCode)
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

    /// Get the embedding dimension (768 for Jina Code V2)
    pub(crate) fn dimension(&self) -> usize {
        EMBEDDING_DIM
    }
}

/// Ensure onnxruntime.dll is present for ort-load-dynamic on Windows.
///
/// The ort crate with `load-dynamic` feature requires onnxruntime.dll at runtime.
/// This function checks for the DLL and downloads it from GitHub releases if missing.
/// Sets `ORT_DYLIB_PATH` so ort can find it.
#[cfg(target_os = "windows")]
fn ensure_ort_dll(cache_dir: &std::path::Path) -> Result<()> {
    let dll_dir = cache_dir.join("ort");
    let dll_path = dll_dir.join("onnxruntime.dll");

    // Already have it — just set the env var
    if dll_path.exists() {
        log::info!("ONNX Runtime DLL found at {}", dll_path.display());
        std::env::set_var("ORT_DYLIB_PATH", &dll_path);
        return Ok(());
    }

    log::info!(
        "ONNX Runtime DLL not found — downloading v{} (one-time setup)...",
        ORT_VERSION
    );

    std::fs::create_dir_all(&dll_dir)
        .map_err(|e| MemoryError::model(format!("Failed to create ORT cache dir: {e}")))?;

    // Download the official release zip
    let url = format!(
        "https://github.com/microsoft/onnxruntime/releases/download/v{ORT_VERSION}/onnxruntime-win-x64-{ORT_VERSION}.zip"
    );

    let response = ureq::get(&url)
        .call()
        .map_err(|e| MemoryError::model(format!("Failed to download ONNX Runtime: {e}")))?;

    // Stream to a temp file
    let zip_path = dll_dir.join("onnxruntime.zip");
    let mut zip_file = std::fs::File::create(&zip_path)
        .map_err(|e| MemoryError::model(format!("Failed to create temp zip: {e}")))?;
    std::io::copy(&mut response.into_reader(), &mut zip_file)
        .map_err(|e| MemoryError::model(format!("Failed to write zip: {e}")))?;
    drop(zip_file);

    // Extract onnxruntime.dll from the zip
    let zip_file = std::fs::File::open(&zip_path)
        .map_err(|e| MemoryError::model(format!("Failed to open zip: {e}")))?;
    let mut archive = zip::ZipArchive::new(zip_file)
        .map_err(|e| MemoryError::model(format!("Failed to read zip: {e}")))?;

    let dll_name_in_zip = format!("onnxruntime-win-x64-{ORT_VERSION}/lib/onnxruntime.dll");

    let mut dll_entry = archive.by_name(&dll_name_in_zip).map_err(|e| {
        MemoryError::model(format!(
            "onnxruntime.dll not found in zip at '{dll_name_in_zip}': {e}"
        ))
    })?;

    let mut out_file = std::fs::File::create(&dll_path)
        .map_err(|e| MemoryError::model(format!("Failed to create DLL file: {e}")))?;
    std::io::copy(&mut dll_entry, &mut out_file)
        .map_err(|e| MemoryError::model(format!("Failed to extract DLL: {e}")))?;

    // Clean up zip
    let _ = std::fs::remove_file(&zip_path);

    log::info!(
        "ONNX Runtime v{} DLL installed at {}",
        ORT_VERSION,
        dll_path.display()
    );
    std::env::set_var("ORT_DYLIB_PATH", &dll_path);

    Ok(())
}
