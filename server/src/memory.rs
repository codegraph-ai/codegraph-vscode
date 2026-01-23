//! Memory layer integration for CodeGraph LSP
//!
//! Provides persistent memory storage with semantic search for AI agent context.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

// Import and re-export types from codegraph_memory
pub use codegraph_memory::{
    MemoryError, MemoryNode, MemorySearch, MemoryStore, SearchConfig, SearchResult, VectorEngine,
};

/// Memory manager for the LSP server
///
/// Wraps `MemoryStore` and provides async methods suitable for the LSP runtime.
/// Handles initialization, storage, search, and invalidation of memory nodes.
pub struct MemoryManager {
    store: Arc<RwLock<Option<Arc<MemoryStore>>>>,
    extension_path: Option<PathBuf>,
}

impl MemoryManager {
    /// Create a new MemoryManager
    ///
    /// # Arguments
    /// * `extension_path` - Optional path to the VS Code extension root for model discovery
    pub fn new(extension_path: Option<PathBuf>) -> Self {
        Self {
            store: Arc::new(RwLock::new(None)),
            extension_path,
        }
    }

    /// Initialize the memory store
    ///
    /// Called during LSP initialization with the workspace path.
    /// Creates the data directory and initializes the vector engine and memory store.
    ///
    /// # Arguments
    /// * `workspace_path` - Path to the workspace root
    ///
    /// # Errors
    /// Returns error if directory creation, model loading, or store initialization fails.
    pub async fn initialize(&self, workspace_path: &Path) -> Result<(), MemoryError> {
        tracing::info!("[MemoryManager::initialize] Starting initialization");
        tracing::info!("[MemoryManager::initialize] Workspace path: {:?}", workspace_path);
        tracing::info!("[MemoryManager::initialize] Extension path: {:?}", self.extension_path);
        
        // Create data directory for memory storage
        let data_dir = workspace_path.join(".codegraph").join("memory");
        tracing::info!("[MemoryManager::initialize] Creating data directory: {:?}", data_dir);
        
        std::fs::create_dir_all(&data_dir)
            .map_err(|e| {
                tracing::error!("[MemoryManager::initialize] Failed to create data directory: {}", e);
                e
            })?;
        tracing::info!("[MemoryManager::initialize] Data directory created successfully");

        // Initialize vector engine with bundled model
        tracing::info!("[MemoryManager::initialize] Initializing VectorEngine...");
        let engine = VectorEngine::new(self.extension_path.as_deref())
            .map_err(|e| {
                tracing::error!("[MemoryManager::initialize] VectorEngine initialization failed: {:?}", e);
                e
            })?;
        tracing::info!("[MemoryManager::initialize] VectorEngine created successfully");
        let engine = Arc::new(engine);

        // Initialize memory store
        tracing::info!("[MemoryManager::initialize] Creating MemoryStore...");
        let store = MemoryStore::new(&data_dir, engine)
            .map_err(|e| {
                tracing::error!("[MemoryManager::initialize] MemoryStore creation failed: {:?}", e);
                e
            })?;
        tracing::info!("[MemoryManager::initialize] MemoryStore created successfully");
        let store = Arc::new(store);

        tracing::info!("[MemoryManager::initialize] Memory store fully initialized at {:?}", data_dir);

        *self.store.write().await = Some(store);
        tracing::info!("[MemoryManager::initialize] Store reference updated - initialization complete");
        Ok(())
    }

    /// Check if memory store is initialized
    pub async fn is_initialized(&self) -> bool {
        self.store.read().await.is_some()
    }

    /// Store a memory node
    ///
    /// Generates embeddings automatically if not present and persists to storage.
    ///
    /// # Arguments
    /// * `node` - The memory node to store
    ///
    /// # Returns
    /// The ID of the stored memory as a string
    pub async fn put(&self, node: MemoryNode) -> Result<String, MemoryError> {
        let store = self.get_store().await?;
        store.put(node).await
    }

    /// Get a memory by ID
    ///
    /// # Arguments
    /// * `id` - The memory ID as a string
    ///
    /// # Returns
    /// The memory node if found, None otherwise
    pub async fn get(&self, id: &str) -> Result<Option<MemoryNode>, MemoryError> {
        let store = self.get_store().await?;
        Ok(store.get(id))
    }

    /// Search memories with hybrid search
    ///
    /// Combines BM25 text search, semantic similarity, and graph proximity
    /// for comprehensive memory retrieval.
    ///
    /// # Arguments
    /// * `query` - The search query text
    /// * `config` - Search configuration (limits, weights, filters)
    /// * `code_context` - List of code node IDs for graph proximity scoring
    ///
    /// # Returns
    /// Vector of search results sorted by relevance score
    pub async fn search(
        &self,
        query: &str,
        config: &SearchConfig,
        code_context: &[String],
    ) -> Result<Vec<SearchResult>, MemoryError> {
        let store = self.get_store().await?;
        let search = MemorySearch::new(store)?;
        search.search(query, code_context, config)
    }

    /// Find memories linked to a code node
    ///
    /// # Arguments
    /// * `code_node_id` - The ID of the code graph node
    ///
    /// # Returns
    /// Vector of memory nodes that reference the given code node
    pub async fn find_by_code_node(
        &self,
        code_node_id: &str,
    ) -> Result<Vec<MemoryNode>, MemoryError> {
        let store = self.get_store().await?;
        Ok(store.find_by_code_node(code_node_id))
    }

    /// Find memories with a specific tag
    ///
    /// # Arguments
    /// * `tag` - The tag to search for
    ///
    /// # Returns
    /// Vector of memory nodes that have the specified tag
    pub async fn find_by_tag(&self, tag: &str) -> Result<Vec<MemoryNode>, MemoryError> {
        let store = self.get_store().await?;
        Ok(store.find_by_tag(tag))
    }

    /// Invalidate a memory (mark as no longer current)
    ///
    /// The memory is not deleted but marked with an invalidation timestamp.
    /// Invalidated memories are excluded from searches by default.
    ///
    /// # Arguments
    /// * `id` - The memory ID to invalidate
    /// * `reason` - Human-readable reason for invalidation
    pub async fn invalidate(&self, id: &str, reason: &str) -> Result<(), MemoryError> {
        let store = self.get_store().await?;
        store.invalidate(id, reason)
    }

    /// Delete a memory permanently
    ///
    /// # Arguments
    /// * `id` - The memory ID to delete
    ///
    /// # Returns
    /// true if the memory was deleted, false if it didn't exist
    pub async fn delete(&self, id: &str) -> Result<bool, MemoryError> {
        let store = self.get_store().await?;
        store.delete(id)
    }

    /// Get all current (non-invalidated) memories
    pub async fn get_all_current(&self) -> Result<Vec<MemoryNode>, MemoryError> {
        let store = self.get_store().await?;
        Ok(store.get_all_current())
    }

    /// Get store statistics
    pub async fn stats(&self) -> Result<serde_json::Value, MemoryError> {
        let store = self.get_store().await?;
        Ok(store.stats())
    }

    /// Invalidate all memories linked to any of the given code node IDs
    ///
    /// Used for auto-invalidation when code changes. Returns the count of
    /// invalidated memories and their IDs for logging.
    ///
    /// # Arguments
    /// * `node_ids` - List of code graph node IDs that have changed
    /// * `reason` - Human-readable reason for invalidation
    ///
    /// # Returns
    /// Vector of (memory_id, memory_title) pairs that were invalidated
    pub async fn invalidate_for_code_nodes(
        &self,
        node_ids: &[String],
        reason: &str,
    ) -> Result<Vec<(String, String)>, MemoryError> {
        if !self.is_initialized().await {
            return Ok(vec![]);
        }

        let store = self.get_store().await?;
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

    /// Get the underlying store, returning error if not initialized
    async fn get_store(&self) -> Result<Arc<MemoryStore>, MemoryError> {
        self.store
            .read()
            .await
            .clone()
            .ok_or_else(|| MemoryError::Other("Memory store not initialized".to_string()))
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
