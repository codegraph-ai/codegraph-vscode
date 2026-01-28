//! Memory layer integration for CodeGraph LSP
//!
//! Provides persistent memory storage with semantic search for AI agent context.
//! Uses on-demand database opening to avoid lock conflicts between processes.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

// Import and re-export types from codegraph_memory
pub use codegraph_memory::{
    MemoryError, MemoryNode, MemorySearch, MemoryStore, SearchConfig, SearchResult, VectorEngine,
};

/// Memory manager for the LSP server
///
/// Opens the database on-demand for each operation and closes it immediately after.
/// This allows multiple processes (VS Code extension + Claude MCP) to share the same
/// database without lock conflicts.
pub struct MemoryManager {
    /// Path to workspace root (database lives at workspace/.codegraph/memory)
    workspace_path: Arc<RwLock<Option<PathBuf>>>,
    /// Path to extension root (for model discovery at extension/models/model2vec)
    extension_path: Option<PathBuf>,
    /// Cached vector engine (holds model, not DB - safe to keep)
    engine: Arc<RwLock<Option<Arc<VectorEngine>>>>,
}

impl MemoryManager {
    /// Create a new MemoryManager
    ///
    /// # Arguments
    /// * `extension_path` - Optional path to the VS Code extension root for model discovery
    pub fn new(extension_path: Option<PathBuf>) -> Self {
        Self {
            workspace_path: Arc::new(RwLock::new(None)),
            extension_path,
            engine: Arc::new(RwLock::new(None)),
        }
    }

    /// Initialize the memory manager with workspace path
    ///
    /// Sets up the workspace path and initializes the vector engine.
    /// Does NOT hold the database open - that happens on-demand per operation.
    ///
    /// # Arguments
    /// * `workspace_path` - Path to the workspace root
    pub async fn initialize(&self, workspace_path: &Path) -> Result<(), MemoryError> {
        tracing::info!("[MemoryManager::initialize] Starting initialization");
        tracing::info!(
            "[MemoryManager::initialize] Workspace path: {:?}",
            workspace_path
        );
        tracing::info!(
            "[MemoryManager::initialize] Extension path: {:?}",
            self.extension_path
        );

        // Create data directory for memory storage
        let data_dir = workspace_path.join(".codegraph").join("memory");
        tracing::info!(
            "[MemoryManager::initialize] Creating data directory: {:?}",
            data_dir
        );

        std::fs::create_dir_all(&data_dir).map_err(|e| {
            tracing::error!(
                "[MemoryManager::initialize] Failed to create data directory: {}",
                e
            );
            e
        })?;
        tracing::info!("[MemoryManager::initialize] Data directory created successfully");

        // Initialize vector engine with bundled model (cached, doesn't hold DB lock)
        tracing::info!("[MemoryManager::initialize] Initializing VectorEngine...");
        let engine = VectorEngine::new(self.extension_path.as_deref()).map_err(|e| {
            tracing::error!(
                "[MemoryManager::initialize] VectorEngine initialization failed: {:?}",
                e
            );
            e
        })?;
        tracing::info!("[MemoryManager::initialize] VectorEngine created successfully");

        // Store paths and engine for on-demand use
        *self.workspace_path.write().await = Some(workspace_path.to_path_buf());
        *self.engine.write().await = Some(Arc::new(engine));

        tracing::info!(
            "[MemoryManager::initialize] Memory manager initialized (on-demand DB mode)"
        );
        Ok(())
    }

    /// Check if memory manager is initialized
    pub async fn is_initialized(&self) -> bool {
        self.workspace_path.read().await.is_some() && self.engine.read().await.is_some()
    }

    /// Open a fresh MemoryStore for an operation
    ///
    /// The store is dropped when it goes out of scope, releasing the DB lock.
    async fn open_store(&self) -> Result<MemoryStore, MemoryError> {
        let workspace = self
            .workspace_path
            .read()
            .await
            .clone()
            .ok_or_else(|| MemoryError::Other("Memory manager not initialized".to_string()))?;

        let engine = self
            .engine
            .read()
            .await
            .clone()
            .ok_or_else(|| MemoryError::Other("Vector engine not initialized".to_string()))?;

        let data_dir = workspace.join(".codegraph").join("memory");
        MemoryStore::new(&data_dir, engine)
    }

    /// Store a memory node
    ///
    /// Opens DB, stores memory, closes DB.
    pub async fn put(&self, node: MemoryNode) -> Result<String, MemoryError> {
        let store = self.open_store().await?;
        store.put(node).await
    }

    /// Get a memory by ID
    ///
    /// Opens DB, retrieves memory, closes DB.
    pub async fn get(&self, id: &str) -> Result<Option<MemoryNode>, MemoryError> {
        let store = self.open_store().await?;
        Ok(store.get(id))
    }

    /// Search memories with hybrid search
    ///
    /// Opens DB, performs search, closes DB.
    pub async fn search(
        &self,
        query: &str,
        config: &SearchConfig,
        code_context: &[String],
    ) -> Result<Vec<SearchResult>, MemoryError> {
        let store = self.open_store().await?;
        let store = Arc::new(store);
        let search = MemorySearch::new(store)?;
        search.search(query, code_context, config)
    }

    /// Find memories linked to a code node
    pub async fn find_by_code_node(
        &self,
        code_node_id: &str,
    ) -> Result<Vec<MemoryNode>, MemoryError> {
        let store = self.open_store().await?;
        Ok(store.find_by_code_node(code_node_id))
    }

    /// Find memories with a specific tag
    pub async fn find_by_tag(&self, tag: &str) -> Result<Vec<MemoryNode>, MemoryError> {
        let store = self.open_store().await?;
        Ok(store.find_by_tag(tag))
    }

    /// Invalidate a memory (mark as no longer current)
    pub async fn invalidate(&self, id: &str, reason: &str) -> Result<(), MemoryError> {
        let store = self.open_store().await?;
        store.invalidate(id, reason)
    }

    /// Delete a memory permanently
    pub async fn delete(&self, id: &str) -> Result<bool, MemoryError> {
        let store = self.open_store().await?;
        store.delete(id)
    }

    /// Get all current (non-invalidated) memories
    pub async fn get_all_current(&self) -> Result<Vec<MemoryNode>, MemoryError> {
        let store = self.open_store().await?;
        Ok(store.get_all_current())
    }

    /// Get store statistics
    pub async fn stats(&self) -> Result<serde_json::Value, MemoryError> {
        let store = self.open_store().await?;
        Ok(store.stats())
    }

    /// Invalidate all memories linked to any of the given code node IDs
    ///
    /// Used for auto-invalidation when code changes.
    pub async fn invalidate_for_code_nodes(
        &self,
        node_ids: &[String],
        reason: &str,
    ) -> Result<Vec<(String, String)>, MemoryError> {
        if !self.is_initialized().await {
            return Ok(vec![]);
        }

        let store = self.open_store().await?;
        let mut invalidated = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();

        for node_id in node_ids {
            let memories = store.find_by_code_node(node_id);
            for memory in memories {
                let id_str = memory.id.to_string();
                // Avoid invalidating the same memory twice
                if seen_ids.insert(id_str.clone())
                    && memory.temporal.is_current()
                    && store.invalidate(&id_str, reason).is_ok()
                {
                    invalidated.push((id_str, memory.title.clone()));
                }
            }
        }

        if !invalidated.is_empty() {
            tracing::info!(
                "Auto-invalidated {} memories due to code changes: {}",
                invalidated.len(),
                reason
            );
        }

        Ok(invalidated)
    }

    /// Create a memory builder for convenience
    pub fn builder() -> codegraph_memory::MemoryNodeBuilder {
        MemoryNode::builder()
    }
}

// Re-export additional commonly used types for convenience
pub use codegraph_memory::{
    search::{MatchReason, MemoryKindFilter},
    CodeLink, IssueSeverity, LinkedNodeType, MemoryId, MemoryKind, MemoryNodeBuilder, MemorySource,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_manager_uninitialized() {
        let manager = MemoryManager::new(None);
        assert!(!manager.is_initialized().await);

        // Operations should fail when not initialized
        let result = manager.get("test-id").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    #[ignore = "requires model files"]
    async fn test_memory_manager_lifecycle() {
        use tempfile::TempDir;
        let temp_dir = TempDir::new().unwrap();
        let manager = MemoryManager::new(None);

        // Initialize
        manager.initialize(temp_dir.path()).await.unwrap();
        assert!(manager.is_initialized().await);

        // Create and store a memory
        let memory = MemoryManager::builder()
            .debug_context("Test problem", "Test solution")
            .title("Test Memory")
            .content("This is test content")
            .tag("test")
            .build()
            .unwrap();

        let id = manager.put(memory).await.unwrap();

        // Retrieve it
        let retrieved = manager.get(&id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().title, "Test Memory");

        // Search for it
        let config = SearchConfig::default();
        let results = manager.search("test problem", &config, &[]).await.unwrap();
        assert!(!results.is_empty());

        // Invalidate it
        manager.invalidate(&id, "testing").await.unwrap();
    }
}
