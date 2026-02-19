//! LSP Backend Implementation
//!
//! This module implements the Language Server Protocol for CodeGraph.

use crate::ai_query::QueryEngine;
use crate::cache::QueryCache;
use crate::error::{LspError, LspResult};
use crate::index::SymbolIndex;
use crate::memory::MemoryManager;
use crate::parser_registry::ParserRegistry;
use crate::watcher::{FileWatcher, GraphUpdater};
use codegraph::{CodeGraph, Direction, EdgeType, NodeId, NodeType};
use codegraph_parser_api::FileInfo;
use dashmap::DashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

/// CodeGraph Language Server backend.
pub struct CodeGraphBackend {
    /// LSP client for sending notifications.
    pub client: Client,

    /// The code graph database.
    pub graph: Arc<RwLock<CodeGraph>>,

    /// Parser registry for all supported languages.
    pub parsers: Arc<ParserRegistry>,

    /// File cache: URI -> FileInfo.
    pub file_cache: Arc<DashMap<Url, FileInfo>>,

    /// Query cache for performance.
    pub query_cache: Arc<QueryCache>,

    /// Symbol index for fast lookups.
    pub symbol_index: Arc<SymbolIndex>,

    /// AI Agent Query Engine for fast code exploration.
    pub query_engine: Arc<QueryEngine>,

    /// Memory manager for persistent AI context.
    pub memory_manager: Arc<MemoryManager>,

    /// Workspace folders
    pub workspace_folders: Arc<RwLock<Vec<std::path::PathBuf>>>,

    /// File system watcher for incremental updates.
    file_watcher: Arc<Mutex<Option<FileWatcher>>>,
}

impl CodeGraphBackend {
    /// Create a new CodeGraph backend.
    pub fn new(client: Client) -> Self {
        let graph = Arc::new(RwLock::new(
            CodeGraph::in_memory().expect("Failed to create in-memory graph"),
        ));

        Self {
            client,
            query_engine: Arc::new(QueryEngine::new(Arc::clone(&graph))),
            graph,
            parsers: Arc::new(ParserRegistry::new()),
            file_cache: Arc::new(DashMap::new()),
            query_cache: Arc::new(QueryCache::new(1000)),
            symbol_index: Arc::new(SymbolIndex::new()),
            memory_manager: Arc::new(MemoryManager::new(None)),
            workspace_folders: Arc::new(RwLock::new(Vec::new())),
            file_watcher: Arc::new(Mutex::new(None)),
        }
    }

    /// Create a backend for testing with a pre-configured graph and query engine.
    /// This allows tests to inject their own graph state without needing a real LSP client.
    #[cfg(test)]
    pub fn new_for_test(graph: Arc<RwLock<CodeGraph>>, query_engine: Arc<QueryEngine>) -> Self {
        use tower_lsp::LspService;

        // Create a dummy client for testing
        let (service, _socket) = LspService::new(Self::new);
        let dummy_client = service.inner().client.clone();

        Self {
            client: dummy_client,
            query_engine,
            graph,
            parsers: Arc::new(ParserRegistry::new()),
            file_cache: Arc::new(DashMap::new()),
            query_cache: Arc::new(QueryCache::new(1000)),
            symbol_index: Arc::new(SymbolIndex::new()),
            memory_manager: Arc::new(MemoryManager::new(None)),
            workspace_folders: Arc::new(RwLock::new(Vec::new())),
            file_watcher: Arc::new(Mutex::new(None)),
        }
    }

    /// Start the file watcher for the given workspace folders.
    pub async fn start_file_watcher(&self, folders: &[PathBuf]) {
        // Create the file watcher
        match FileWatcher::new(
            Arc::clone(&self.graph),
            Arc::clone(&self.parsers),
            self.client.clone(),
            Arc::clone(&self.memory_manager),
        ) {
            Ok(mut watcher) => {
                // Start watching each folder
                for folder in folders {
                    if let Err(e) = watcher.watch(folder) {
                        self.client
                            .log_message(
                                MessageType::WARNING,
                                format!("Failed to watch {}: {}", folder.display(), e),
                            )
                            .await;
                    } else {
                        self.client
                            .log_message(
                                MessageType::INFO,
                                format!("Watching folder: {}", folder.display()),
                            )
                            .await;
                    }
                }

                // Store the watcher
                *self.file_watcher.lock().await = Some(watcher);
            }
            Err(e) => {
                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!("Failed to create file watcher: {e}"),
                    )
                    .await;
            }
        }
    }

    /// Remove all nodes associated with a file from the graph.
    ///
    /// Also auto-invalidates any memories linked to the removed nodes.
    async fn remove_file_from_graph(&self, path: &std::path::Path) {
        let path_str = path.to_string_lossy().to_string();
        let node_id_strings: Vec<String>;

        // Scope the graph lock to avoid holding it across await
        {
            let mut graph = self.graph.write().await;

            // Query for all nodes with this file path using the query builder
            if let Ok(nodes) = graph.query().property("path", path_str.clone()).execute() {
                // Collect node IDs as strings for memory invalidation
                node_id_strings = nodes.iter().map(|n| n.to_string()).collect();

                // Delete the nodes from the graph
                for node_id in nodes {
                    let _ = graph.delete_node(node_id);
                }
            } else {
                node_id_strings = Vec::new();
            }
        }

        // Auto-invalidate memories linked to these nodes (after releasing graph lock)
        if !node_id_strings.is_empty() {
            let reason = format!("Code changed: {}", path_str);
            if let Err(e) = self
                .memory_manager
                .invalidate_for_code_nodes(&node_id_strings, &reason)
                .await
            {
                tracing::warn!("Failed to invalidate memories for {}: {}", path_str, e);
            }
        }

        // Invalidate caches
        self.query_cache.invalidate_file(&path.to_path_buf());
        self.symbol_index.remove_file(path);
    }

    /// Index all supported files in a directory
    pub fn index_directory<'a>(
        &'a self,
        dir: &'a std::path::Path,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = usize> + Send + 'a>> {
        Box::pin(async move {
            use std::fs;

            let mut indexed_count = 0;
            let supported_extensions = self.parsers.supported_extensions();

            tracing::info!("Indexing directory: {:?}", dir);

            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();

                    // Skip hidden files and directories
                    if let Some(name) = path.file_name() {
                        if name.to_string_lossy().starts_with('.') {
                            continue;
                        }
                    }

                    if path.is_dir() {
                        // Skip common directories
                        let dir_name = path.file_name().unwrap().to_string_lossy();
                        if matches!(
                            dir_name.as_ref(),
                            "node_modules" | "target" | "dist" | "build" | ".git" | "__pycache__"
                        ) {
                            continue;
                        }

                        // Recursively index subdirectories
                        indexed_count += self.index_directory(&path).await;
                    } else if path.is_file() {
                        // Check if file has supported extension
                        if let Some(ext) = path.extension() {
                            let ext_str = ext.to_string_lossy();
                            if supported_extensions
                                .iter()
                                .any(|&e| e.trim_start_matches('.') == ext_str)
                            {
                                // Parse the file using parse_file (which updates metrics)
                                if let Some(parser) = self.parsers.parser_for_path(&path) {
                                    let mut graph = self.graph.write().await;
                                    match parser.parse_file(&path, &mut graph) {
                                        Ok(file_info) => {
                                            self.symbol_index.add_file(
                                                path.clone(),
                                                &file_info,
                                                &graph,
                                            );
                                            self.file_cache.insert(
                                                Url::from_file_path(&path).unwrap(),
                                                file_info,
                                            );
                                            indexed_count += 1;
                                        }
                                        Err(e) => {
                                            tracing::warn!("Failed to parse {:?}: {}", path, e);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            indexed_count
        })
    }

    /// Find node at the given position.
    /// Position line is 0-indexed (LSP style), but we convert to 1-indexed for graph.
    pub fn find_node_at_position(
        &self,
        graph: &CodeGraph,
        path: &std::path::Path,
        position: Position,
    ) -> LspResult<Option<NodeId>> {
        // LSP positions are 0-indexed, our index stores 1-indexed
        let line = (position.line + 1) as i64;
        let col = position.character as i64;

        tracing::info!(
            "find_node_at_position: path={:?}, LSP position={}:{}, converted={}:{}",
            path,
            position.line,
            position.character,
            line,
            col
        );

        // Try the symbol index first (faster)
        if let Some(node_id) = self
            .symbol_index
            .find_at_position(path, line as u32, col as u32)
        {
            tracing::info!("Found node in symbol index: {:?}", node_id);
            return Ok(Some(node_id));
        }

        tracing::info!("Not found in symbol index, trying graph query");

        // Fallback: Get all nodes from the symbol index for this file and check positions
        // The graph stores line_start/line_end (not start_line/end_line)
        let file_symbols = self.symbol_index.get_file_symbols(path);

        tracing::info!(
            "Symbol index returned {} symbols for {:?}",
            file_symbols.len(),
            path
        );

        for node_id in file_symbols {
            if let Ok(node) = graph.get_node(node_id) {
                // Parsers use line_start/line_end (not start_line/end_line)
                let start_line = node.properties.get_int("line_start").unwrap_or(0);
                let end_line = node.properties.get_int("line_end").unwrap_or(0);
                // Column info not available from most parsers, default to full line
                let start_col = node.properties.get_int("col_start").unwrap_or(0);
                let end_col = node.properties.get_int("col_end").unwrap_or(i64::MAX);

                tracing::info!(
                    "Checking node {:?} '{}' at {}:{} to {}:{}",
                    node_id,
                    node.properties.get_string("name").unwrap_or(""),
                    start_line,
                    start_col,
                    end_line,
                    end_col
                );

                if line >= start_line && line <= end_line {
                    if line == start_line && col < start_col {
                        continue;
                    }
                    if line == end_line && col > end_col {
                        continue;
                    }
                    tracing::info!("Found matching node: {:?}", node_id);
                    return Ok(Some(node_id));
                }
            }
        }

        tracing::warn!("No node found at position {}:{} in {:?}", line, col, path);
        Ok(None)
    }

    /// Find the nearest symbol to a position, or the first symbol in the file.
    /// This is used as a fallback when no symbol is found at the exact cursor position.
    /// Returns (node_id, was_fallback) where was_fallback indicates if this was not an exact match.
    pub fn find_nearest_node(
        &self,
        graph: &CodeGraph,
        path: &std::path::Path,
        position: Position,
    ) -> LspResult<Option<(NodeId, bool)>> {
        // First try exact position match
        if let Some(node_id) = self.find_node_at_position(graph, path, position)? {
            return Ok(Some((node_id, false)));
        }

        // LSP positions are 0-indexed, our index stores 1-indexed
        let target_line = (position.line + 1) as i64;

        // Get all symbols in the file
        let file_symbols = self.symbol_index.get_file_symbols(path);

        if file_symbols.is_empty() {
            tracing::info!("No symbols in file {:?}", path);
            return Ok(None);
        }

        // Find the nearest symbol by line distance
        let mut best_match: Option<(NodeId, i64)> = None;

        for node_id in file_symbols {
            if let Ok(node) = graph.get_node(node_id) {
                let start_line = node.properties.get_int("line_start").unwrap_or(0);
                let end_line = node.properties.get_int("line_end").unwrap_or(0);

                // Calculate distance - prefer symbols that start after cursor (looking ahead)
                // or symbols that contain the cursor line
                let distance = if target_line >= start_line && target_line <= end_line {
                    // Cursor is within this symbol's range
                    0
                } else if start_line > target_line {
                    // Symbol starts after cursor - prefer these (looking forward)
                    start_line - target_line
                } else {
                    // Symbol ends before cursor - less preferred
                    (target_line - end_line) + 1000 // Add penalty for looking backward
                };

                if best_match.is_none() || distance < best_match.unwrap().1 {
                    best_match = Some((node_id, distance));
                }
            }
        }

        if let Some((node_id, _)) = best_match {
            if let Ok(node) = graph.get_node(node_id) {
                let name = node.properties.get_string("name").unwrap_or("unknown");
                tracing::info!(
                    "Fallback: found nearest symbol '{}' for position {}:{} in {:?}",
                    name,
                    target_line,
                    position.character,
                    path
                );
            }
            return Ok(Some((node_id, true)));
        }

        Ok(None)
    }

    /// Find all edges connected to a node.
    pub fn get_connected_edges(
        &self,
        graph: &CodeGraph,
        node_id: NodeId,
        direction: Direction,
    ) -> Vec<(NodeId, NodeId, EdgeType)> {
        let mut edges = Vec::new();

        // Get neighbors in the specified direction
        let neighbors = match graph.get_neighbors(node_id, direction) {
            Ok(n) => n,
            Err(_) => return edges,
        };

        for neighbor_id in neighbors {
            // Get edges between this node and the neighbor
            let (source, target) = match direction {
                Direction::Outgoing => (node_id, neighbor_id),
                Direction::Incoming => (neighbor_id, node_id),
                Direction::Both => {
                    // Try both directions
                    if let Ok(edge_ids) = graph.get_edges_between(node_id, neighbor_id) {
                        for edge_id in edge_ids {
                            if let Ok(edge) = graph.get_edge(edge_id) {
                                edges.push((edge.source_id, edge.target_id, edge.edge_type));
                            }
                        }
                    }
                    if let Ok(edge_ids) = graph.get_edges_between(neighbor_id, node_id) {
                        for edge_id in edge_ids {
                            if let Ok(edge) = graph.get_edge(edge_id) {
                                edges.push((edge.source_id, edge.target_id, edge.edge_type));
                            }
                        }
                    }
                    continue;
                }
            };

            if let Ok(edge_ids) = graph.get_edges_between(source, target) {
                for edge_id in edge_ids {
                    if let Ok(edge) = graph.get_edge(edge_id) {
                        edges.push((edge.source_id, edge.target_id, edge.edge_type));
                    }
                }
            }
        }

        edges
    }

    /// Find the definition node for a reference.
    fn find_definition_for_reference(
        &self,
        graph: &CodeGraph,
        ref_node_id: NodeId,
    ) -> LspResult<Option<NodeId>> {
        let edges = self.get_connected_edges(graph, ref_node_id, Direction::Outgoing);

        for (_, target, edge_type) in edges {
            match edge_type {
                EdgeType::Calls | EdgeType::References | EdgeType::Imports => {
                    return Ok(Some(target));
                }
                _ => continue,
            }
        }

        Ok(None)
    }

    /// Convert a node to an LSP Location.
    pub fn node_to_location(&self, graph: &CodeGraph, node_id: NodeId) -> LspResult<Location> {
        let node = graph
            .get_node(node_id)
            .map_err(|e| LspError::Graph(e.to_string()))?;

        // Try to get path from node properties first, fallback to symbol index
        let path_str = match node.properties.get_string("path") {
            Some(p) => p.to_string(),
            None => {
                // Fallback: look up file path from symbol index
                match self.symbol_index.find_file_for_node(node_id) {
                    Some(path_buf) => path_buf.to_string_lossy().to_string(),
                    None => {
                        let node_name = node.properties.get_string("name").unwrap_or("<unnamed>");
                        let node_type = format!("{}", node.node_type);
                        tracing::warn!(
                            "Cannot determine file path for node {}: {} '{}' (not in symbol index)",
                            node_id,
                            node_type,
                            node_name
                        );
                        return Err(LspError::NodeNotFound(format!(
                            "Cannot determine file path for {node_type} '{node_name}'"
                        )));
                    }
                }
            }
        };

        // Support both property name conventions (line_start or start_line)
        let start_line = node
            .properties
            .get_int("line_start")
            .or_else(|| node.properties.get_int("start_line"))
            .unwrap_or(1) as u32;
        let start_col = node
            .properties
            .get_int("col_start")
            .or_else(|| node.properties.get_int("start_col"))
            .unwrap_or(0) as u32;
        let end_line = node
            .properties
            .get_int("line_end")
            .or_else(|| node.properties.get_int("end_line"))
            .unwrap_or(start_line as i64) as u32;
        let end_col = node
            .properties
            .get_int("col_end")
            .or_else(|| node.properties.get_int("end_col"))
            .unwrap_or(0) as u32;

        // Convert to 0-indexed
        let start_line = start_line.saturating_sub(1);
        let end_line = end_line.saturating_sub(1);

        Ok(Location {
            uri: Url::from_file_path(&path_str)
                .map_err(|_| LspError::InvalidUri(path_str.clone()))?,
            range: Range {
                start: Position {
                    line: start_line,
                    character: start_col,
                },
                end: Position {
                    line: end_line,
                    character: end_col,
                },
            },
        })
    }

    /// Get the source code for a node.
    pub async fn get_node_source_code(&self, node_id: NodeId) -> LspResult<Option<String>> {
        let graph = self.graph.read().await;
        let node = graph
            .get_node(node_id)
            .map_err(|e| LspError::Graph(e.to_string()))?;

        // Try to get source from node properties first
        if let Some(source) = node.properties.get_string("source") {
            return Ok(Some(source.to_string()));
        }

        // Try to get path from node properties, fallback to symbol index
        let path = match node.properties.get_string("path") {
            Some(p) => PathBuf::from(p),
            None => {
                // Fallback: look up file path from symbol index
                match self.symbol_index.find_file_for_node(node_id) {
                    Some(p) => p,
                    None => return Ok(None),
                }
            }
        };

        // Try to read from file
        if path.exists() {
            // Support both property name conventions
            let start_line = node
                .properties
                .get_int("line_start")
                .or_else(|| node.properties.get_int("start_line"))
                .unwrap_or(1) as usize;
            let end_line = node
                .properties
                .get_int("line_end")
                .or_else(|| node.properties.get_int("end_line"))
                .unwrap_or(start_line as i64) as usize;

            if let Ok(content) = tokio::fs::read_to_string(&path).await {
                let lines: Vec<&str> = content.lines().collect();
                if start_line > 0 && end_line <= lines.len() {
                    let source: String = lines[start_line - 1..end_line].join("\n");
                    return Ok(Some(source));
                }
            }
        }

        Ok(None)
    }

    /// Helper to get a string property from a node
    #[allow(dead_code)]
    fn get_node_string_property(
        &self,
        graph: &CodeGraph,
        node_id: NodeId,
        key: &str,
    ) -> Option<String> {
        graph
            .get_node(node_id)
            .ok()?
            .properties
            .get_string(key)
            .map(|s| s.to_string())
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for CodeGraphBackend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        tracing::info!("Initializing CodeGraph LSP server");

        // Extract extension path from initialization options
        let extension_path = params.initialization_options.and_then(|opts| {
            opts.get("extensionPath")
                .and_then(|v| v.as_str())
                .map(std::path::PathBuf::from)
        });

        if let Some(path) = extension_path {
            tracing::info!(
                "[LSP::initialize] Extension path received: {}",
                path.display()
            );
            tracing::info!(
                "[LSP::initialize] Model path should be: {}/models/model2vec/",
                path.display()
            );

            // Update memory manager with extension path by replacing it
            // Safety: We're replacing the Arc contents during initialization before any use
            let new_manager = Arc::new(MemoryManager::new(Some(path.clone())));
            let self_mut = self as *const Self as *mut Self;
            unsafe {
                (*self_mut).memory_manager = new_manager;
            }
            tracing::info!("[LSP::initialize] MemoryManager updated with extension path");
        } else {
            tracing::error!(
                "[LSP::initialize] CRITICAL: No extension path provided in initialization options!"
            );
            tracing::warn!("[LSP::initialize] Memory features will require MODEL2VEC_PATH or ~/.codegraph/models/model2vec");
        }

        // Store workspace folders
        if let Some(folders) = params.workspace_folders {
            let mut workspace_folders = self.workspace_folders.write().await;
            for folder in folders {
                if let Ok(path) = folder.uri.to_file_path() {
                    tracing::info!("Workspace folder: {}", path.display());
                    workspace_folders.push(path);
                }
            }
        }

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::FULL),
                        save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                            include_text: Some(true),
                        })),
                        ..Default::default()
                    },
                )),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                call_hierarchy_provider: Some(CallHierarchyServerCapability::Simple(true)),
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec![
                        "codegraph.getDependencyGraph".to_string(),
                        "codegraph.getCallGraph".to_string(),
                        "codegraph.analyzeImpact".to_string(),
                        "codegraph.getParserMetrics".to_string(),
                        "codegraph.reindexWorkspace".to_string(),
                        "codegraph.getAIContext".to_string(),
                        "codegraph.findRelatedTests".to_string(),
                        "codegraph.getNodeLocation".to_string(),
                        "codegraph.getWorkspaceSymbols".to_string(),
                        "codegraph.analyzeComplexity".to_string(),
                        "codegraph.findUnusedCode".to_string(),
                        "codegraph.analyzeCoupling".to_string(),
                        // AI Agent Query Primitives
                        "codegraph.symbolSearch".to_string(),
                        "codegraph.findByImports".to_string(),
                        "codegraph.findEntryPoints".to_string(),
                        "codegraph.traverseGraph".to_string(),
                        "codegraph.getCallers".to_string(),
                        "codegraph.getCallees".to_string(),
                        "codegraph.getDetailedSymbolInfo".to_string(),
                        "codegraph.findBySignature".to_string(),
                        // Memory Layer Commands
                        "codegraph.memoryStore".to_string(),
                        "codegraph.memorySearch".to_string(),
                        "codegraph.memoryGet".to_string(),
                        "codegraph.memoryInvalidate".to_string(),
                        "codegraph.memoryList".to_string(),
                        "codegraph.memoryUpdate".to_string(),
                        "codegraph.memoryContext".to_string(),
                        "codegraph.memoryStats".to_string(),
                        // Git mining commands
                        "codegraph.mineGitHistory".to_string(),
                        "codegraph.mineGitHistoryForFile".to_string(),
                    ],
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                }),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "CodeGraph LSP server initialized")
            .await;

        // Index workspace folders
        let folders = self.workspace_folders.read().await.clone();
        let mut total_indexed = 0;

        for folder in &folders {
            let count = self.index_directory(folder).await;
            total_indexed += count;
            self.client
                .log_message(
                    MessageType::INFO,
                    format!("Indexed {} files from {}", count, folder.display()),
                )
                .await;
        }

        self.client
            .log_message(
                MessageType::INFO,
                format!("Total files indexed: {total_indexed}"),
            )
            .await;

        // Resolve cross-file imports after all files are indexed
        {
            let mut graph = self.graph.write().await;
            GraphUpdater::resolve_cross_file_imports(&mut graph);
        }
        self.client
            .log_message(MessageType::INFO, "Cross-file imports resolved")
            .await;

        // Build AI query engine indexes
        self.query_engine.build_indexes().await;
        self.client
            .log_message(MessageType::INFO, "AI query engine indexes built")
            .await;

        // Initialize memory store for persistent AI context
        if let Some(first_folder) = folders.first() {
            tracing::info!(
                "Starting memory store initialization for workspace: {}",
                first_folder.display()
            );
            self.client
                .log_message(
                    MessageType::INFO,
                    format!(
                        "[DEBUG] Initializing memory store for: {}",
                        first_folder.display()
                    ),
                )
                .await;

            match self.memory_manager.initialize(first_folder).await {
                Ok(_) => {
                    tracing::info!("Memory store initialization succeeded");
                    self.client
                        .log_message(MessageType::INFO, "✓ Memory store initialized successfully")
                        .await;
                }
                Err(e) => {
                    tracing::error!("Memory store initialization failed: {:?}", e);
                    self.client
                        .log_message(
                            MessageType::ERROR,
                            format!("✗ Failed to initialize memory store: {}. Memory features will be disabled.", e),
                        )
                        .await;
                }
            }
        } else {
            tracing::warn!("No workspace folders available for memory initialization");
            self.client
                .log_message(
                    MessageType::WARNING,
                    "[DEBUG] No workspace folders found - memory store not initialized",
                )
                .await;
        }

        // Start file watcher for incremental updates
        if !folders.is_empty() {
            self.start_file_watcher(&folders).await;
        }
    }

    async fn shutdown(&self) -> Result<()> {
        tracing::info!("Shutting down CodeGraph LSP server");
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;

        tracing::info!("did_open called for: {}", uri);

        let path = match uri.to_file_path() {
            Ok(p) => p,
            Err(_) => {
                tracing::warn!("Invalid URI: {}", uri);
                return;
            }
        };

        if let Some(parser) = self.parsers.parser_for_path(&path) {
            tracing::info!("Parser found for: {:?}", path);
            let mut graph = self.graph.write().await;

            match parser.parse_source(&text, &path, &mut graph) {
                Ok(file_info) => {
                    tracing::info!("Parse succeeded for: {:?}", path);

                    // Resolve cross-file imports after parsing
                    GraphUpdater::resolve_cross_file_imports(&mut graph);

                    // Update symbol index
                    self.symbol_index.add_file(path.clone(), &file_info, &graph);

                    // Update file cache
                    self.file_cache.insert(uri.clone(), file_info);

                    self.client
                        .log_message(MessageType::INFO, format!("Indexed: {uri}"))
                        .await;
                }
                Err(e) => {
                    tracing::error!("Parse failed for {:?}: {}", path, e);
                    self.client
                        .log_message(MessageType::ERROR, format!("Parse error in {uri}: {e}"))
                        .await;
                }
            }
        } else {
            tracing::warn!("No parser found for: {:?}", path);
        }
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;

        let path = match uri.to_file_path() {
            Ok(p) => p,
            Err(_) => return,
        };

        // Get the full text (assuming full sync mode)
        if let Some(change) = params.content_changes.into_iter().next() {
            if let Some(parser) = self.parsers.parser_for_path(&path) {
                // Remove old entries
                self.remove_file_from_graph(&path).await;

                // Re-parse with new content
                let mut graph = self.graph.write().await;
                if let Ok(file_info) = parser.parse_source(&change.text, &path, &mut graph) {
                    // Resolve cross-file imports after parsing
                    GraphUpdater::resolve_cross_file_imports(&mut graph);

                    self.symbol_index.add_file(path.clone(), &file_info, &graph);
                    self.file_cache.insert(uri, file_info);
                }
            }
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri;

        let path = match uri.to_file_path() {
            Ok(p) => p,
            Err(_) => return,
        };

        if let Some(parser) = self.parsers.parser_for_path(&path) {
            if let Some(text) = params.text {
                self.remove_file_from_graph(&path).await;

                let mut graph = self.graph.write().await;
                if let Ok(file_info) = parser.parse_source(&text, &path, &mut graph) {
                    // Resolve cross-file imports after parsing
                    GraphUpdater::resolve_cross_file_imports(&mut graph);

                    self.symbol_index.add_file(path.clone(), &file_info, &graph);
                    self.file_cache.insert(uri, file_info);
                }
            }
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        // Keep in graph for cross-file references, but remove from file cache
        self.file_cache.remove(&params.text_document.uri);
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let path = uri
            .to_file_path()
            .map_err(|_| tower_lsp::jsonrpc::Error::invalid_params("Invalid URI"))?;

        let graph = self.graph.read().await;

        // Find node at the given position
        let node_id = match self.find_node_at_position(&graph, &path, position)? {
            Some(id) => id,
            None => return Ok(None),
        };

        // Check if this is a reference - find its definition
        if let Some(def_node_id) = self.find_definition_for_reference(&graph, node_id)? {
            tracing::info!(
                "goto_definition: found reference node {} -> definition node {}",
                node_id,
                def_node_id
            );
            match self.node_to_location(&graph, def_node_id) {
                Ok(location) => return Ok(Some(GotoDefinitionResponse::Scalar(location))),
                Err(e) => {
                    // Log the error but try to return the source location as fallback
                    tracing::warn!(
                        "Failed to get definition location: {}, trying source node",
                        e
                    );
                    // Fall through to try source node
                }
            }
        }

        // Already at definition (or fallback if definition lookup failed)
        let location = self.node_to_location(&graph, node_id)?;
        Ok(Some(GotoDefinitionResponse::Scalar(location)))
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let include_declaration = params.context.include_declaration;

        let path = uri
            .to_file_path()
            .map_err(|_| tower_lsp::jsonrpc::Error::invalid_params("Invalid URI"))?;

        let graph = self.graph.read().await;

        // Find node at position
        let node_id = match self.find_node_at_position(&graph, &path, position)? {
            Some(id) => id,
            None => return Ok(None),
        };

        // Find the definition
        let def_node_id = self
            .find_definition_for_reference(&graph, node_id)?
            .unwrap_or(node_id);

        let mut locations = Vec::new();

        // Include declaration if requested
        if include_declaration {
            if let Ok(loc) = self.node_to_location(&graph, def_node_id) {
                locations.push(loc);
            }
        }

        // Find all incoming edges (references to this definition)
        let edges = self.get_connected_edges(&graph, def_node_id, Direction::Incoming);

        for (source, _, _) in edges {
            if let Ok(loc) = self.node_to_location(&graph, source) {
                locations.push(loc);
            }
        }

        if locations.is_empty() {
            Ok(None)
        } else {
            Ok(Some(locations))
        }
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let path = uri
            .to_file_path()
            .map_err(|_| tower_lsp::jsonrpc::Error::invalid_params("Invalid URI"))?;

        let graph = self.graph.read().await;

        let node_id = match self.find_node_at_position(&graph, &path, position)? {
            Some(id) => id,
            None => return Ok(None),
        };

        let node = graph
            .get_node(node_id)
            .map_err(|_| tower_lsp::jsonrpc::Error::internal_error())?;

        // Build hover content
        let name = node.properties.get_string("name").unwrap_or("").to_string();
        let kind = format!("{}", node.node_type);
        let signature = node
            .properties
            .get_string("signature")
            .unwrap_or("")
            .to_string();
        let doc = node.properties.get_string("doc").map(|s| s.to_string());
        let def_path = node.properties.get_string("path").unwrap_or("").to_string();

        // Count references
        let ref_count = self
            .get_connected_edges(&graph, node_id, Direction::Incoming)
            .len();

        let mut content = format!("**{kind}** `{name}`");

        if !signature.is_empty() {
            content.push_str(&format!("\n\n```\n{signature}\n```"));
        }

        if let Some(doc) = doc {
            content.push_str(&format!("\n\n{doc}"));
        }

        content.push_str(&format!(
            "\n\n---\n\n**Defined in:** {def_path}\n**References:** {ref_count}"
        ));

        Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: content,
            }),
            range: None,
        }))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = &params.text_document.uri;

        let path = uri
            .to_file_path()
            .map_err(|_| tower_lsp::jsonrpc::Error::invalid_params("Invalid URI"))?;

        let graph = self.graph.read().await;

        // Get all symbols in this file
        let node_ids = self.symbol_index.get_file_symbols(&path);

        let mut symbols = Vec::new();

        for node_id in node_ids {
            if let Ok(node) = graph.get_node(node_id) {
                let name = node.properties.get_string("name").unwrap_or("").to_string();
                let kind = match node.node_type {
                    NodeType::Function => SymbolKind::FUNCTION,
                    NodeType::Class => SymbolKind::CLASS,
                    NodeType::Interface => SymbolKind::INTERFACE,
                    NodeType::Module => SymbolKind::MODULE,
                    NodeType::Variable => SymbolKind::VARIABLE,
                    NodeType::Type => SymbolKind::TYPE_PARAMETER,
                    NodeType::CodeFile => SymbolKind::FILE,
                    _ => SymbolKind::VARIABLE,
                };

                if let Ok(location) = self.node_to_location(&graph, node_id) {
                    #[allow(deprecated)]
                    symbols.push(SymbolInformation {
                        name,
                        kind,
                        tags: None,
                        deprecated: None,
                        location,
                        container_name: None,
                    });
                }
            }
        }

        if symbols.is_empty() {
            Ok(None)
        } else {
            Ok(Some(DocumentSymbolResponse::Flat(symbols)))
        }
    }

    async fn prepare_call_hierarchy(
        &self,
        params: CallHierarchyPrepareParams,
    ) -> Result<Option<Vec<CallHierarchyItem>>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let path = uri
            .to_file_path()
            .map_err(|_| tower_lsp::jsonrpc::Error::invalid_params("Invalid URI"))?;

        let graph = self.graph.read().await;

        let node_id = match self.find_node_at_position(&graph, &path, position)? {
            Some(id) => id,
            None => return Ok(None),
        };

        let node = graph
            .get_node(node_id)
            .map_err(|_| tower_lsp::jsonrpc::Error::internal_error())?;

        // Only functions can have call hierarchies
        if node.node_type != NodeType::Function {
            return Ok(None);
        }

        let name = node.properties.get_string("name").unwrap_or("").to_string();
        let location = self.node_to_location(&graph, node_id)?;

        Ok(Some(vec![CallHierarchyItem {
            name,
            kind: SymbolKind::FUNCTION,
            tags: None,
            detail: node
                .properties
                .get_string("signature")
                .map(|s| s.to_string()),
            uri: location.uri,
            range: location.range,
            selection_range: location.range,
            data: Some(serde_json::json!({ "nodeId": node_id.to_string() })),
        }]))
    }

    async fn incoming_calls(
        &self,
        params: CallHierarchyIncomingCallsParams,
    ) -> Result<Option<Vec<CallHierarchyIncomingCall>>> {
        let node_id = self.extract_node_id_from_item(&params.item)?;

        let graph = self.graph.read().await;

        let edges = self.get_connected_edges(&graph, node_id, Direction::Incoming);

        let mut calls = Vec::new();

        for (source, _, edge_type) in edges {
            if edge_type == EdgeType::Calls {
                if let Ok(node) = graph.get_node(source) {
                    let name = node.properties.get_string("name").unwrap_or("").to_string();

                    if let Ok(location) = self.node_to_location(&graph, source) {
                        calls.push(CallHierarchyIncomingCall {
                            from: CallHierarchyItem {
                                name,
                                kind: SymbolKind::FUNCTION,
                                tags: None,
                                detail: node
                                    .properties
                                    .get_string("signature")
                                    .map(|s| s.to_string()),
                                uri: location.uri.clone(),
                                range: location.range,
                                selection_range: location.range,
                                data: Some(serde_json::json!({ "nodeId": source.to_string() })),
                            },
                            from_ranges: vec![location.range],
                        });
                    }
                }
            }
        }

        if calls.is_empty() {
            Ok(None)
        } else {
            Ok(Some(calls))
        }
    }

    async fn outgoing_calls(
        &self,
        params: CallHierarchyOutgoingCallsParams,
    ) -> Result<Option<Vec<CallHierarchyOutgoingCall>>> {
        let node_id = self.extract_node_id_from_item(&params.item)?;

        let graph = self.graph.read().await;

        let edges = self.get_connected_edges(&graph, node_id, Direction::Outgoing);

        let mut calls = Vec::new();

        for (_, target, edge_type) in edges {
            if edge_type == EdgeType::Calls {
                if let Ok(node) = graph.get_node(target) {
                    let name = node.properties.get_string("name").unwrap_or("").to_string();

                    if let Ok(location) = self.node_to_location(&graph, target) {
                        calls.push(CallHierarchyOutgoingCall {
                            to: CallHierarchyItem {
                                name,
                                kind: SymbolKind::FUNCTION,
                                tags: None,
                                detail: node
                                    .properties
                                    .get_string("signature")
                                    .map(|s| s.to_string()),
                                uri: location.uri.clone(),
                                range: location.range,
                                selection_range: location.range,
                                data: Some(serde_json::json!({ "nodeId": target.to_string() })),
                            },
                            from_ranges: vec![location.range],
                        });
                    }
                }
            }
        }

        if calls.is_empty() {
            Ok(None)
        } else {
            Ok(Some(calls))
        }
    }

    async fn execute_command(
        &self,
        params: ExecuteCommandParams,
    ) -> Result<Option<serde_json::Value>> {
        tracing::info!("Executing command: {}", params.command);

        match params.command.as_str() {
            "codegraph.getDependencyGraph" => {
                let args = params.arguments.first().ok_or_else(|| {
                    tower_lsp::jsonrpc::Error::invalid_params("Missing arguments")
                })?;
                let params: crate::handlers::DependencyGraphParams =
                    serde_json::from_value(args.clone()).map_err(|e| {
                        tower_lsp::jsonrpc::Error::invalid_params(format!("Invalid params: {e}"))
                    })?;
                let response = self.handle_get_dependency_graph(params).await?;
                Ok(Some(serde_json::to_value(response).unwrap()))
            }

            "codegraph.getCallGraph" => {
                let args = params.arguments.first().ok_or_else(|| {
                    tower_lsp::jsonrpc::Error::invalid_params("Missing arguments")
                })?;
                let params: crate::handlers::CallGraphParams = serde_json::from_value(args.clone())
                    .map_err(|e| {
                        tower_lsp::jsonrpc::Error::invalid_params(format!("Invalid params: {e}"))
                    })?;
                let response = self.handle_get_call_graph(params).await?;
                Ok(Some(serde_json::to_value(response).unwrap()))
            }

            "codegraph.analyzeImpact" => {
                let args = params.arguments.first().ok_or_else(|| {
                    tower_lsp::jsonrpc::Error::invalid_params("Missing arguments")
                })?;
                let params: crate::handlers::ImpactAnalysisParams =
                    serde_json::from_value(args.clone()).map_err(|e| {
                        tower_lsp::jsonrpc::Error::invalid_params(format!("Invalid params: {e}"))
                    })?;
                let response = self.handle_analyze_impact(params).await?;
                Ok(Some(serde_json::to_value(response).unwrap()))
            }

            "codegraph.getParserMetrics" => {
                let response = self
                    .handle_get_parser_metrics(crate::handlers::ParserMetricsParams {
                        language: None,
                    })
                    .await?;
                Ok(Some(serde_json::to_value(response).unwrap()))
            }

            "codegraph.reindexWorkspace" => {
                // Clear graph and caches
                {
                    let mut graph = self.graph.write().await;
                    *graph = CodeGraph::in_memory().expect("Failed to create graph");
                }
                self.symbol_index.clear();
                self.file_cache.clear();
                self.query_cache.invalidate_all();

                self.client
                    .log_message(MessageType::INFO, "Reindexing workspace...")
                    .await;

                // Index all workspace folders
                let workspace_folders = self.workspace_folders.read().await.clone();
                let mut total_indexed = 0;

                for folder in workspace_folders {
                    tracing::info!("Indexing folder: {:?}", folder);
                    let count = self.index_directory(&folder).await;
                    total_indexed += count;
                }

                // Rebuild AI query engine indexes
                self.query_engine.build_indexes().await;

                self.client
                    .log_message(
                        MessageType::INFO,
                        format!("Workspace reindexed: {total_indexed} files"),
                    )
                    .await;

                Ok(None)
            }

            "codegraph.getAIContext" => {
                let args = params.arguments.first().ok_or_else(|| {
                    tower_lsp::jsonrpc::Error::invalid_params("Missing arguments")
                })?;
                let params: crate::handlers::AIContextParams = serde_json::from_value(args.clone())
                    .map_err(|e| {
                        tower_lsp::jsonrpc::Error::invalid_params(format!("Invalid params: {e}"))
                    })?;
                let response = self.handle_get_ai_context(params).await?;
                Ok(Some(serde_json::to_value(response).unwrap()))
            }

            "codegraph.findRelatedTests" => {
                let args = params.arguments.first().ok_or_else(|| {
                    tower_lsp::jsonrpc::Error::invalid_params("Missing arguments")
                })?;
                let params: crate::handlers::RelatedTestsParams =
                    serde_json::from_value(args.clone()).map_err(|e| {
                        tower_lsp::jsonrpc::Error::invalid_params(format!("Invalid params: {e}"))
                    })?;
                let response = self.handle_find_related_tests(params).await?;
                Ok(Some(serde_json::to_value(response).unwrap()))
            }

            "codegraph.getNodeLocation" => {
                let args = params.arguments.first().ok_or_else(|| {
                    tower_lsp::jsonrpc::Error::invalid_params("Missing arguments")
                })?;
                let params: crate::handlers::GetNodeLocationParams =
                    serde_json::from_value(args.clone()).map_err(|e| {
                        tower_lsp::jsonrpc::Error::invalid_params(format!("Invalid params: {e}"))
                    })?;
                let response = self.handle_get_node_location(params).await?;
                Ok(Some(serde_json::to_value(response).unwrap()))
            }

            "codegraph.getWorkspaceSymbols" => {
                let args = params.arguments.first().ok_or_else(|| {
                    tower_lsp::jsonrpc::Error::invalid_params("Missing arguments")
                })?;
                let params: crate::handlers::WorkspaceSymbolsParams =
                    serde_json::from_value(args.clone()).map_err(|e| {
                        tower_lsp::jsonrpc::Error::invalid_params(format!("Invalid params: {e}"))
                    })?;
                let response = self.handle_get_workspace_symbols(params).await?;
                Ok(Some(serde_json::to_value(response).unwrap()))
            }

            "codegraph.analyzeComplexity" => {
                let args = params.arguments.first().ok_or_else(|| {
                    tower_lsp::jsonrpc::Error::invalid_params("Missing arguments")
                })?;
                let params: crate::handlers::ComplexityParams =
                    serde_json::from_value(args.clone()).map_err(|e| {
                        tower_lsp::jsonrpc::Error::invalid_params(format!("Invalid params: {e}"))
                    })?;
                let response = self.handle_analyze_complexity(params).await?;
                Ok(Some(serde_json::to_value(response).unwrap()))
            }

            "codegraph.findUnusedCode" => {
                let args = params.arguments.first().ok_or_else(|| {
                    tower_lsp::jsonrpc::Error::invalid_params("Missing arguments")
                })?;
                let params: crate::handlers::UnusedCodeParams =
                    serde_json::from_value(args.clone()).map_err(|e| {
                        tower_lsp::jsonrpc::Error::invalid_params(format!("Invalid params: {e}"))
                    })?;
                let response = self.handle_find_unused_code(params).await?;
                Ok(Some(serde_json::to_value(response).unwrap()))
            }

            "codegraph.analyzeCoupling" => {
                let args = params.arguments.first().ok_or_else(|| {
                    tower_lsp::jsonrpc::Error::invalid_params("Missing arguments")
                })?;
                let params: crate::handlers::CouplingParams = serde_json::from_value(args.clone())
                    .map_err(|e| {
                        tower_lsp::jsonrpc::Error::invalid_params(format!("Invalid params: {e}"))
                    })?;
                let response = self.handle_analyze_coupling(params).await?;
                Ok(Some(serde_json::to_value(response).unwrap()))
            }

            // AI Agent Query Primitives
            "codegraph.symbolSearch" => {
                let args = params.arguments.first().ok_or_else(|| {
                    tower_lsp::jsonrpc::Error::invalid_params("Missing arguments")
                })?;
                let params: crate::handlers::SymbolSearchParams =
                    serde_json::from_value(args.clone()).map_err(|e| {
                        tower_lsp::jsonrpc::Error::invalid_params(format!("Invalid params: {e}"))
                    })?;
                let response = self.handle_symbol_search(params).await?;
                Ok(Some(serde_json::to_value(response).unwrap()))
            }

            "codegraph.findByImports" => {
                let args = params.arguments.first().ok_or_else(|| {
                    tower_lsp::jsonrpc::Error::invalid_params("Missing arguments")
                })?;
                let params: crate::handlers::FindByImportsParams =
                    serde_json::from_value(args.clone()).map_err(|e| {
                        tower_lsp::jsonrpc::Error::invalid_params(format!("Invalid params: {e}"))
                    })?;
                let response = self.handle_find_by_imports(params).await?;
                Ok(Some(serde_json::to_value(response).unwrap()))
            }

            "codegraph.findEntryPoints" => {
                let args = params.arguments.first().ok_or_else(|| {
                    tower_lsp::jsonrpc::Error::invalid_params("Missing arguments")
                })?;
                let params: crate::handlers::FindEntryPointsParams =
                    serde_json::from_value(args.clone()).map_err(|e| {
                        tower_lsp::jsonrpc::Error::invalid_params(format!("Invalid params: {e}"))
                    })?;
                let response = self.handle_find_entry_points(params).await?;
                Ok(Some(serde_json::to_value(response).unwrap()))
            }

            "codegraph.traverseGraph" => {
                let args = params.arguments.first().ok_or_else(|| {
                    tower_lsp::jsonrpc::Error::invalid_params("Missing arguments")
                })?;
                let params: crate::handlers::TraverseGraphParams =
                    serde_json::from_value(args.clone()).map_err(|e| {
                        tower_lsp::jsonrpc::Error::invalid_params(format!("Invalid params: {e}"))
                    })?;
                let response = self.handle_traverse_graph(params).await?;
                Ok(Some(serde_json::to_value(response).unwrap()))
            }

            "codegraph.getCallers" => {
                let args = params.arguments.first().ok_or_else(|| {
                    tower_lsp::jsonrpc::Error::invalid_params("Missing arguments")
                })?;
                let params: crate::handlers::GetCallersParams =
                    serde_json::from_value(args.clone()).map_err(|e| {
                        tower_lsp::jsonrpc::Error::invalid_params(format!("Invalid params: {e}"))
                    })?;
                let response = self.handle_get_callers(params).await?;
                Ok(Some(serde_json::to_value(response).unwrap()))
            }

            "codegraph.getCallees" => {
                let args = params.arguments.first().ok_or_else(|| {
                    tower_lsp::jsonrpc::Error::invalid_params("Missing arguments")
                })?;
                let params: crate::handlers::GetCallersParams =
                    serde_json::from_value(args.clone()).map_err(|e| {
                        tower_lsp::jsonrpc::Error::invalid_params(format!("Invalid params: {e}"))
                    })?;
                let response = self.handle_get_callees(params).await?;
                Ok(Some(serde_json::to_value(response).unwrap()))
            }

            "codegraph.getDetailedSymbolInfo" => {
                let args = params.arguments.first().ok_or_else(|| {
                    tower_lsp::jsonrpc::Error::invalid_params("Missing arguments")
                })?;
                let params: crate::handlers::GetDetailedInfoParams =
                    serde_json::from_value(args.clone()).map_err(|e| {
                        tower_lsp::jsonrpc::Error::invalid_params(format!("Invalid params: {e}"))
                    })?;
                let response = self.handle_get_detailed_symbol_info(params).await?;
                Ok(Some(serde_json::to_value(response).unwrap()))
            }

            "codegraph.findBySignature" => {
                let args = params.arguments.first().ok_or_else(|| {
                    tower_lsp::jsonrpc::Error::invalid_params("Missing arguments")
                })?;
                let params: crate::handlers::FindBySignatureParams =
                    serde_json::from_value(args.clone()).map_err(|e| {
                        tower_lsp::jsonrpc::Error::invalid_params(format!("Invalid params: {e}"))
                    })?;
                let response = self.handle_find_by_signature(params).await?;
                Ok(Some(serde_json::to_value(response).unwrap()))
            }

            // Memory Layer Commands
            "codegraph.memoryStore" => {
                let args = params.arguments.first().ok_or_else(|| {
                    tower_lsp::jsonrpc::Error::invalid_params("Missing arguments")
                })?;
                let params: crate::handlers::MemoryStoreParams =
                    serde_json::from_value(args.clone()).map_err(|e| {
                        tower_lsp::jsonrpc::Error::invalid_params(format!("Invalid params: {e}"))
                    })?;
                let response = self.handle_memory_store(params).await?;
                Ok(Some(serde_json::to_value(response).unwrap()))
            }

            "codegraph.memorySearch" => {
                let args = params.arguments.first().ok_or_else(|| {
                    tower_lsp::jsonrpc::Error::invalid_params("Missing arguments")
                })?;
                let params: crate::handlers::MemorySearchParams =
                    serde_json::from_value(args.clone()).map_err(|e| {
                        tower_lsp::jsonrpc::Error::invalid_params(format!("Invalid params: {e}"))
                    })?;
                let response = self.handle_memory_search(params).await?;
                Ok(Some(serde_json::to_value(response).unwrap()))
            }

            "codegraph.memoryGet" => {
                let args = params.arguments.first().ok_or_else(|| {
                    tower_lsp::jsonrpc::Error::invalid_params("Missing arguments")
                })?;
                let params: crate::handlers::MemoryGetParams = serde_json::from_value(args.clone())
                    .map_err(|e| {
                        tower_lsp::jsonrpc::Error::invalid_params(format!("Invalid params: {e}"))
                    })?;
                let response = self.handle_memory_get(params).await?;
                Ok(Some(serde_json::to_value(response).unwrap()))
            }

            "codegraph.memoryInvalidate" => {
                let args = params.arguments.first().ok_or_else(|| {
                    tower_lsp::jsonrpc::Error::invalid_params("Missing arguments")
                })?;
                let params: crate::handlers::MemoryInvalidateParams =
                    serde_json::from_value(args.clone()).map_err(|e| {
                        tower_lsp::jsonrpc::Error::invalid_params(format!("Invalid params: {e}"))
                    })?;
                let response = self.handle_memory_invalidate(params).await?;
                Ok(Some(serde_json::to_value(response).unwrap()))
            }

            "codegraph.memoryList" => {
                let args = params
                    .arguments
                    .first()
                    .cloned()
                    .unwrap_or(serde_json::json!({}));
                let params: crate::handlers::MemoryListParams = serde_json::from_value(args)
                    .map_err(|e| {
                        tower_lsp::jsonrpc::Error::invalid_params(format!("Invalid params: {e}"))
                    })?;
                let response = self.handle_memory_list(params).await?;
                Ok(Some(serde_json::to_value(response).unwrap()))
            }

            "codegraph.memoryUpdate" => {
                let args = params.arguments.first().ok_or_else(|| {
                    tower_lsp::jsonrpc::Error::invalid_params("Missing arguments")
                })?;
                let params: crate::handlers::MemoryUpdateParams =
                    serde_json::from_value(args.clone()).map_err(|e| {
                        tower_lsp::jsonrpc::Error::invalid_params(format!("Invalid params: {e}"))
                    })?;
                let response = self.handle_memory_update(params).await?;
                Ok(Some(serde_json::to_value(response).unwrap()))
            }

            "codegraph.memoryContext" => {
                let args = params.arguments.first().ok_or_else(|| {
                    tower_lsp::jsonrpc::Error::invalid_params("Missing arguments")
                })?;
                let params: crate::handlers::MemoryContextParams =
                    serde_json::from_value(args.clone()).map_err(|e| {
                        tower_lsp::jsonrpc::Error::invalid_params(format!("Invalid params: {e}"))
                    })?;
                let response = self.handle_memory_context(params).await?;
                Ok(Some(serde_json::to_value(response).unwrap()))
            }

            "codegraph.memoryStats" => {
                let response = self.handle_memory_stats().await?;
                Ok(Some(response))
            }

            // Git mining commands
            "codegraph.mineGitHistory" => {
                let args = params.arguments.first().ok_or_else(|| {
                    tower_lsp::jsonrpc::Error::invalid_params("Missing arguments")
                })?;
                let params: serde_json::Value = serde_json::from_value(args.clone())
                    .map_err(|e| tower_lsp::jsonrpc::Error::invalid_params(e.to_string()))?;
                let response = self.handle_mine_git_history(params).await?;
                Ok(Some(response))
            }

            "codegraph.mineGitHistoryForFile" => {
                let args = params.arguments.first().ok_or_else(|| {
                    tower_lsp::jsonrpc::Error::invalid_params("Missing arguments")
                })?;
                let file_params: serde_json::Value = serde_json::from_value(args.clone())
                    .map_err(|e| tower_lsp::jsonrpc::Error::invalid_params(e.to_string()))?;
                let response = self.handle_mine_git_history_for_file(file_params).await?;
                Ok(Some(response))
            }

            _ => Err(tower_lsp::jsonrpc::Error::method_not_found()),
        }
    }
}

impl CodeGraphBackend {
    /// Extract node ID from CallHierarchyItem data.
    fn extract_node_id_from_item(&self, item: &CallHierarchyItem) -> Result<NodeId> {
        let data = item
            .data
            .as_ref()
            .ok_or_else(|| tower_lsp::jsonrpc::Error::invalid_params("Missing data"))?;

        let node_id_str = data
            .get("nodeId")
            .and_then(|v| v.as_str())
            .ok_or_else(|| tower_lsp::jsonrpc::Error::invalid_params("Missing nodeId"))?;

        node_id_str
            .parse::<NodeId>()
            .map_err(|_| tower_lsp::jsonrpc::Error::invalid_params("Invalid nodeId"))
    }
}

// ==========================================
// Memory Layer Handlers
// ==========================================

impl CodeGraphBackend {
    /// Store a new memory in the memory layer.
    pub async fn handle_memory_store(
        &self,
        params: crate::handlers::MemoryStoreParams,
    ) -> Result<crate::handlers::MemoryStoreResponse> {
        use crate::memory::{LinkedNodeType, MemoryNode};

        // Parse the memory kind using the builder pattern for convenience
        let mut builder = MemoryNode::builder();

        builder = match params.kind.as_str() {
            "debug_context" => {
                let problem = params
                    .kind_data
                    .get("problem")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let solution = params
                    .kind_data
                    .get("solution")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                builder.debug_context(problem, solution)
            }
            "architectural_decision" => {
                let decision = params
                    .kind_data
                    .get("decision")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let rationale = params
                    .kind_data
                    .get("rationale")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                builder.architectural_decision(decision, rationale)
            }
            "known_issue" => {
                let description = params
                    .kind_data
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let severity = match params
                    .kind_data
                    .get("severity")
                    .and_then(|v| v.as_str())
                    .unwrap_or("medium")
                {
                    "critical" => crate::memory::IssueSeverity::Critical,
                    "high" => crate::memory::IssueSeverity::High,
                    "medium" => crate::memory::IssueSeverity::Medium,
                    "low" => crate::memory::IssueSeverity::Low,
                    _ => crate::memory::IssueSeverity::Medium,
                };
                builder.known_issue(description, severity)
            }
            "convention" => {
                let name = params
                    .kind_data
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let description = params
                    .kind_data
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                builder.convention(name, description)
            }
            "project_context" => {
                let topic = params
                    .kind_data
                    .get("topic")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let description = params
                    .kind_data
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                builder.project_context(topic, description)
            }
            _ => {
                return Err(tower_lsp::jsonrpc::Error::invalid_params(format!(
                    "Unknown memory kind: {}",
                    params.kind
                )))
            }
        };

        // Set title and content
        builder = builder.title(&params.title).content(&params.content);

        // Add tags
        for tag in params.tags {
            builder = builder.tag(&tag);
        }

        // Add code links
        for link in params.code_links {
            let node_type = match link.node_type.as_str() {
                "function" => LinkedNodeType::Function,
                "class" => LinkedNodeType::Class,
                "module" => LinkedNodeType::Module,
                "file" => LinkedNodeType::File,
                "variable" => LinkedNodeType::Variable,
                "import" => LinkedNodeType::Import,
                "interface" => LinkedNodeType::Interface,
                "trait" => LinkedNodeType::Trait,
                _ => LinkedNodeType::Function, // Default fallback
            };
            builder = builder.link_to_code(link.node_id, node_type);
        }

        // Set confidence if provided
        if let Some(conf) = params.confidence {
            builder = builder.confidence(conf);
        }

        let memory = builder.build().map_err(|e| {
            tower_lsp::jsonrpc::Error::invalid_params(format!("Failed to build memory: {e}"))
        })?;

        // Store the memory
        let id = self
            .memory_manager
            .put(memory)
            .await
            .map_err(|_| tower_lsp::jsonrpc::Error::internal_error())?;

        Ok(crate::handlers::MemoryStoreResponse { id, success: true })
    }

    /// Search memories using hybrid search.
    pub async fn handle_memory_search(
        &self,
        params: crate::handlers::MemorySearchParams,
    ) -> Result<crate::handlers::MemorySearchResponse> {
        use crate::memory::{MemoryKindFilter, SearchConfig};

        // Build search config
        let mut config = SearchConfig {
            limit: params.limit,
            current_only: params.current_only,
            ..Default::default()
        };

        // Set tag filter (tags is Vec, not Option)
        if !params.tags.is_empty() {
            config.tags = params.tags;
        }

        // Set kind filter (kinds is Vec, not Option)
        if !params.kinds.is_empty() {
            config.kinds = params
                .kinds
                .iter()
                .filter_map(|k| match k.as_str() {
                    "debug_context" => Some(MemoryKindFilter::DebugContext),
                    "architectural_decision" => Some(MemoryKindFilter::ArchitecturalDecision),
                    "known_issue" => Some(MemoryKindFilter::KnownIssue),
                    "convention" => Some(MemoryKindFilter::Convention),
                    "project_context" => Some(MemoryKindFilter::ProjectContext),
                    _ => None,
                })
                .collect();
        }

        // Perform search
        let results = self
            .memory_manager
            .search(&params.query, &config, &params.code_context)
            .await
            .map_err(|_| tower_lsp::jsonrpc::Error::internal_error())?;

        let total = results.len();
        let search_results: Vec<crate::handlers::MemorySearchResult> = results
            .into_iter()
            .map(|r| {
                let kind_str = match &r.memory.kind {
                    crate::memory::MemoryKind::DebugContext { .. } => "debug_context",
                    crate::memory::MemoryKind::ArchitecturalDecision { .. } => {
                        "architectural_decision"
                    }
                    crate::memory::MemoryKind::KnownIssue { .. } => "known_issue",
                    crate::memory::MemoryKind::Convention { .. } => "convention",
                    crate::memory::MemoryKind::ProjectContext { .. } => "project_context",
                };

                crate::handlers::MemorySearchResult {
                    id: r.memory.id.to_string(),
                    kind: kind_str.to_string(),
                    title: r.memory.title.clone(),
                    content: r.memory.content.clone(),
                    tags: r.memory.tags.clone(),
                    score: r.score,
                    is_current: r.memory.is_current(),
                }
            })
            .collect();

        Ok(crate::handlers::MemorySearchResponse {
            results: search_results,
            total,
        })
    }

    /// Get a memory by ID.
    pub async fn handle_memory_get(
        &self,
        params: crate::handlers::MemoryGetParams,
    ) -> Result<Option<crate::handlers::MemoryGetResponse>> {
        let memory = self
            .memory_manager
            .get(&params.id)
            .await
            .map_err(|_| tower_lsp::jsonrpc::Error::internal_error())?;

        let response = memory.map(|m| {
            let kind_json = match &m.kind {
                crate::memory::MemoryKind::DebugContext {
                    problem_description,
                    root_cause,
                    solution,
                    symptoms,
                    related_errors,
                } => {
                    serde_json::json!({
                        "type": "debug_context",
                        "problem_description": problem_description,
                        "root_cause": root_cause,
                        "solution": solution,
                        "symptoms": symptoms,
                        "related_errors": related_errors
                    })
                }
                crate::memory::MemoryKind::ArchitecturalDecision {
                    decision,
                    rationale,
                    alternatives_considered,
                    stakeholders,
                } => {
                    serde_json::json!({
                        "type": "architectural_decision",
                        "decision": decision,
                        "rationale": rationale,
                        "alternatives_considered": alternatives_considered,
                        "stakeholders": stakeholders
                    })
                }
                crate::memory::MemoryKind::KnownIssue {
                    description,
                    severity,
                    workaround,
                    tracking_id,
                } => {
                    serde_json::json!({
                        "type": "known_issue",
                        "description": description,
                        "severity": format!("{:?}", severity).to_lowercase(),
                        "workaround": workaround,
                        "tracking_id": tracking_id
                    })
                }
                crate::memory::MemoryKind::Convention {
                    name,
                    description,
                    pattern,
                    anti_pattern,
                } => {
                    serde_json::json!({
                        "type": "convention",
                        "name": name,
                        "description": description,
                        "pattern": pattern,
                        "anti_pattern": anti_pattern
                    })
                }
                crate::memory::MemoryKind::ProjectContext {
                    topic,
                    description,
                    tags,
                } => {
                    serde_json::json!({
                        "type": "project_context",
                        "topic": topic,
                        "description": description,
                        "tags": tags
                    })
                }
            };

            let code_links: Vec<crate::handlers::CodeLinkResponse> = m
                .code_links
                .iter()
                .map(|link| crate::handlers::CodeLinkResponse {
                    node_id: link.node_id.clone(),
                    node_type: format!("{:?}", link.node_type).to_lowercase(),
                })
                .collect();

            crate::handlers::MemoryGetResponse {
                id: m.id.to_string(),
                kind: kind_json,
                title: m.title.clone(),
                content: m.content.clone(),
                tags: m.tags.clone(),
                code_links,
                confidence: m.confidence,
                is_current: m.is_current(),
                created_at: m.temporal.created_at.to_rfc3339(),
                valid_from: m.temporal.valid_at.to_rfc3339().into(),
            }
        });

        Ok(response)
    }

    /// Invalidate a memory by ID.
    pub async fn handle_memory_invalidate(
        &self,
        params: crate::handlers::MemoryInvalidateParams,
    ) -> Result<crate::handlers::MemoryInvalidateResponse> {
        self.memory_manager
            .invalidate(&params.id, "Invalidated via LSP command")
            .await
            .map_err(|_| tower_lsp::jsonrpc::Error::internal_error())?;

        Ok(crate::handlers::MemoryInvalidateResponse { success: true })
    }

    /// List memories with optional filters.
    pub async fn handle_memory_list(
        &self,
        params: crate::handlers::MemoryListParams,
    ) -> Result<crate::handlers::MemoryListResponse> {
        // Get all current memories
        let all_memories = self
            .memory_manager
            .get_all_current()
            .await
            .map_err(|_| tower_lsp::jsonrpc::Error::internal_error())?;

        // Apply filters
        let filtered: Vec<_> = all_memories
            .into_iter()
            .filter(|m| {
                // Filter by current_only
                if params.current_only && !m.is_current() {
                    return false;
                }

                // Filter by kinds
                if !params.kinds.is_empty() {
                    let kind_str = match &m.kind {
                        crate::memory::MemoryKind::DebugContext { .. } => "debug_context",
                        crate::memory::MemoryKind::ArchitecturalDecision { .. } => {
                            "architectural_decision"
                        }
                        crate::memory::MemoryKind::KnownIssue { .. } => "known_issue",
                        crate::memory::MemoryKind::Convention { .. } => "convention",
                        crate::memory::MemoryKind::ProjectContext { .. } => "project_context",
                    };
                    if !params.kinds.contains(&kind_str.to_string()) {
                        return false;
                    }
                }

                // Filter by tags
                if !params.tags.is_empty() && !params.tags.iter().any(|t| m.tags.contains(t)) {
                    return false;
                }

                true
            })
            .collect();

        let total = filtered.len();
        let has_more = params.offset + params.limit < total;

        // Apply pagination
        let paginated: Vec<crate::handlers::MemorySearchResult> = filtered
            .into_iter()
            .skip(params.offset)
            .take(params.limit)
            .map(|m| {
                let kind_str = match &m.kind {
                    crate::memory::MemoryKind::DebugContext { .. } => "debug_context",
                    crate::memory::MemoryKind::ArchitecturalDecision { .. } => {
                        "architectural_decision"
                    }
                    crate::memory::MemoryKind::KnownIssue { .. } => "known_issue",
                    crate::memory::MemoryKind::Convention { .. } => "convention",
                    crate::memory::MemoryKind::ProjectContext { .. } => "project_context",
                };

                crate::handlers::MemorySearchResult {
                    id: m.id.to_string(),
                    kind: kind_str.to_string(),
                    title: m.title.clone(),
                    content: m.content.clone(),
                    tags: m.tags.clone(),
                    score: m.confidence,
                    is_current: m.is_current(),
                }
            })
            .collect();

        Ok(crate::handlers::MemoryListResponse {
            memories: paginated,
            total,
            has_more,
        })
    }

    /// Update an existing memory.
    pub async fn handle_memory_update(
        &self,
        params: crate::handlers::MemoryUpdateParams,
    ) -> Result<crate::handlers::MemoryUpdateResponse> {
        use crate::memory::{CodeLink, LinkedNodeType};

        // Get existing memory
        let existing = self
            .memory_manager
            .get(&params.id)
            .await
            .map_err(|_| tower_lsp::jsonrpc::Error::internal_error())?;

        let Some(mut memory) = existing else {
            return Ok(crate::handlers::MemoryUpdateResponse {
                success: false,
                memory: None,
            });
        };

        // Apply updates
        if let Some(title) = params.title {
            memory.title = title;
        }
        if let Some(content) = params.content {
            memory.content = content;
        }
        if let Some(tags) = params.tags {
            memory.tags = tags;
        }
        if let Some(confidence) = params.confidence {
            memory.confidence = confidence;
        }

        // Handle code link additions
        for link_param in params.add_code_links {
            let node_type = match link_param.node_type.as_str() {
                "function" => LinkedNodeType::Function,
                "class" => LinkedNodeType::Class,
                "module" => LinkedNodeType::Module,
                "file" => LinkedNodeType::File,
                "variable" => LinkedNodeType::Variable,
                "import" => LinkedNodeType::Import,
                "interface" => LinkedNodeType::Interface,
                "trait" => LinkedNodeType::Trait,
                _ => LinkedNodeType::Function, // Default fallback
            };
            memory
                .code_links
                .push(CodeLink::new(link_param.node_id, node_type));
        }

        // Handle code link removals
        for node_id in params.remove_code_links {
            memory.code_links.retain(|link| link.node_id != node_id);
        }

        // Clear embeddings so they get regenerated
        memory.embedding = None;

        // Store updated memory
        let id = self
            .memory_manager
            .put(memory)
            .await
            .map_err(|_| tower_lsp::jsonrpc::Error::internal_error())?;

        // Get the updated memory for response
        let updated = self
            .memory_manager
            .get(&id)
            .await
            .map_err(|_| tower_lsp::jsonrpc::Error::internal_error())?;

        let response_memory = updated.map(|m| {
            let kind_json = match &m.kind {
                crate::memory::MemoryKind::DebugContext {
                    problem_description,
                    root_cause,
                    solution,
                    symptoms,
                    related_errors,
                } => {
                    serde_json::json!({
                        "type": "debug_context",
                        "problem_description": problem_description,
                        "root_cause": root_cause,
                        "solution": solution,
                        "symptoms": symptoms,
                        "related_errors": related_errors
                    })
                }
                crate::memory::MemoryKind::ArchitecturalDecision {
                    decision,
                    rationale,
                    alternatives_considered,
                    stakeholders,
                } => {
                    serde_json::json!({
                        "type": "architectural_decision",
                        "decision": decision,
                        "rationale": rationale,
                        "alternatives_considered": alternatives_considered,
                        "stakeholders": stakeholders
                    })
                }
                crate::memory::MemoryKind::KnownIssue {
                    description,
                    severity,
                    workaround,
                    tracking_id,
                } => {
                    serde_json::json!({
                        "type": "known_issue",
                        "description": description,
                        "severity": format!("{:?}", severity).to_lowercase(),
                        "workaround": workaround,
                        "tracking_id": tracking_id
                    })
                }
                crate::memory::MemoryKind::Convention {
                    name,
                    description,
                    pattern,
                    anti_pattern,
                } => {
                    serde_json::json!({
                        "type": "convention",
                        "name": name,
                        "description": description,
                        "pattern": pattern,
                        "anti_pattern": anti_pattern
                    })
                }
                crate::memory::MemoryKind::ProjectContext {
                    topic,
                    description,
                    tags,
                } => {
                    serde_json::json!({
                        "type": "project_context",
                        "topic": topic,
                        "description": description,
                        "tags": tags
                    })
                }
            };

            let code_links: Vec<crate::handlers::CodeLinkResponse> = m
                .code_links
                .iter()
                .map(|link| crate::handlers::CodeLinkResponse {
                    node_id: link.node_id.clone(),
                    node_type: format!("{:?}", link.node_type).to_lowercase(),
                })
                .collect();

            crate::handlers::MemoryGetResponse {
                id: m.id.to_string(),
                kind: kind_json,
                title: m.title.clone(),
                content: m.content.clone(),
                tags: m.tags.clone(),
                code_links,
                confidence: m.confidence,
                is_current: m.is_current(),
                created_at: m.temporal.created_at.to_rfc3339(),
                valid_from: m.temporal.valid_at.to_rfc3339().into(),
            }
        });

        Ok(crate::handlers::MemoryUpdateResponse {
            success: true,
            memory: response_memory,
        })
    }

    /// Get memories relevant to a code context.
    pub async fn handle_memory_context(
        &self,
        params: crate::handlers::MemoryContextParams,
    ) -> Result<crate::handlers::MemoryContextResponse> {
        use crate::memory::SearchConfig;

        // Parse URI and find code context
        let uri = Url::parse(&params.uri)
            .map_err(|_| tower_lsp::jsonrpc::Error::invalid_params("Invalid URI"))?;

        let path = uri
            .to_file_path()
            .map_err(|_| tower_lsp::jsonrpc::Error::invalid_params("Invalid file path"))?;

        // Build code context from the graph
        let mut code_context = Vec::new();
        let graph = self.graph.read().await;

        // Get file node
        let path_str = path.to_string_lossy().to_string();
        if let Ok(file_nodes) = graph.query().property("path", path_str).execute() {
            for node_id in file_nodes {
                code_context.push(node_id.to_string());
            }
        }

        // If position provided, find node at position
        if let Some(pos) = params.position {
            let position = Position {
                line: pos.line,
                character: pos.character,
            };
            if let Ok(Some(node_id)) = self.find_node_at_position(&graph, &path, position) {
                code_context.push(node_id.to_string());
            }
        }

        drop(graph);

        // Search with code context for graph proximity scoring
        let mut config = SearchConfig {
            limit: params.limit,
            current_only: true,
            ..Default::default()
        };

        // Set kind filter if provided (kinds is Vec, not Option)
        if !params.kinds.is_empty() {
            config.kinds = params
                .kinds
                .iter()
                .filter_map(|k| match k.as_str() {
                    "debug_context" => Some(crate::memory::MemoryKindFilter::DebugContext),
                    "architectural_decision" => {
                        Some(crate::memory::MemoryKindFilter::ArchitecturalDecision)
                    }
                    "known_issue" => Some(crate::memory::MemoryKindFilter::KnownIssue),
                    "convention" => Some(crate::memory::MemoryKindFilter::Convention),
                    "project_context" => Some(crate::memory::MemoryKindFilter::ProjectContext),
                    _ => None,
                })
                .collect();
        }

        // Use file name as query for semantic relevance
        let query = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let results = self
            .memory_manager
            .search(&query, &config, &code_context)
            .await
            .map_err(|_| tower_lsp::jsonrpc::Error::internal_error())?;

        let memories: Vec<crate::handlers::ContextMemory> = results
            .into_iter()
            .map(|r| {
                let kind_str = match &r.memory.kind {
                    crate::memory::MemoryKind::DebugContext { .. } => "debug_context",
                    crate::memory::MemoryKind::ArchitecturalDecision { .. } => {
                        "architectural_decision"
                    }
                    crate::memory::MemoryKind::KnownIssue { .. } => "known_issue",
                    crate::memory::MemoryKind::Convention { .. } => "convention",
                    crate::memory::MemoryKind::ProjectContext { .. } => "project_context",
                };

                let reason = r
                    .match_reasons
                    .first()
                    .map(|mr| format!("{:?}", mr))
                    .unwrap_or_else(|| "Related to code context".to_string());

                crate::handlers::ContextMemory {
                    id: r.memory.id.to_string(),
                    kind: kind_str.to_string(),
                    title: r.memory.title.clone(),
                    content: r.memory.content.clone(),
                    tags: r.memory.tags.clone(),
                    relevance_score: r.score,
                    relevance_reason: reason,
                }
            })
            .collect();

        Ok(crate::handlers::MemoryContextResponse { memories })
    }

    /// Get memory store statistics.
    pub async fn handle_memory_stats(&self) -> Result<serde_json::Value> {
        let stats = self
            .memory_manager
            .stats()
            .await
            .map_err(|_| tower_lsp::jsonrpc::Error::internal_error())?;

        Ok(stats)
    }

    /// Mine git history and create memories from relevant commits.
    pub async fn handle_mine_git_history(
        &self,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        use crate::git_mining::{GitMiner, MiningConfig};

        // Get workspace folder
        let workspace_folders = self.workspace_folders.read().await;
        let workspace_path = workspace_folders
            .first()
            .ok_or_else(tower_lsp::jsonrpc::Error::invalid_request)?;

        // Parse configuration from params
        let max_commits = params
            .get("maxCommits")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(500);

        let min_confidence = params
            .get("minConfidence")
            .and_then(|v| v.as_f64())
            .map(|v| v as f32)
            .unwrap_or(0.7);

        let mine_bug_fixes = params
            .get("mineBugFixes")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let mine_arch_decisions = params
            .get("mineArchDecisions")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let mine_breaking_changes = params
            .get("mineBreakingChanges")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let mine_reverts = params
            .get("mineReverts")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let mine_features = params
            .get("mineFeatures")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let mine_deprecations = params
            .get("mineDeprecations")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let include_hotspots = params
            .get("includeHotspots")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let include_coupling = params
            .get("includeCoupling")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let config = MiningConfig {
            max_commits,
            min_confidence,
            mine_bug_fixes,
            mine_arch_decisions,
            mine_breaking_changes,
            mine_reverts,
            mine_features,
            mine_deprecations,
            grep_patterns: vec![
                "fix:".to_string(),
                "bug:".to_string(),
                "BREAKING".to_string(),
                "revert".to_string(),
                "arch:".to_string(),
                "adr:".to_string(),
                "feat:".to_string(),
                "deprecate".to_string(),
            ],
        };

        // Create miner and run
        let miner = GitMiner::new(workspace_path)
            .map_err(|_| tower_lsp::jsonrpc::Error::invalid_request())?;

        let mut result = miner
            .mine_repository(&self.memory_manager, &self.graph, &config)
            .await
            .map_err(|e| {
                tracing::error!("Git mining failed: {}", e);
                tower_lsp::jsonrpc::Error::internal_error()
            })?;

        // Detect hotspots if requested
        let mut hotspots_created = 0;
        if include_hotspots {
            match miner.detect_hotspots(10).await {
                Ok(hotspots) => {
                    for hotspot in hotspots.iter().take(20) {
                        // Create memory for each hotspot
                        let memory = codegraph_memory::MemoryNode::builder()
                            .project_context(
                                format!("High-activity file: {}", hotspot.file_path),
                                format!(
                                    "Modified {} times across {} commits. This file shows high churn, \
                                     indicating active development or potential complexity.",
                                    hotspot.change_count, hotspot.unique_commits
                                ),
                            )
                            .title(format!("Hotspot: {}", hotspot.file_path))
                            .content(format!(
                                "**Change Count:** {}\n**Unique Commits:** {}\n**Recent Changes:**\n{}",
                                hotspot.change_count,
                                hotspot.unique_commits,
                                hotspot.recent_changes.join("\n- ")
                            ))
                            .tag("hotspot")
                            .tag("git-mined")
                            .confidence(0.7)
                            .build()
                            .ok();

                        if let Some(m) = memory {
                            if let Ok(id) = self.memory_manager.put(m).await {
                                result.memory_ids.push(id);
                                hotspots_created += 1;
                            }
                        }
                    }
                }
                Err(e) => {
                    result
                        .warnings
                        .push(format!("Failed to detect hotspots: {}", e));
                }
            }
        }

        // Detect coupling if requested
        let mut couplings_created = 0;
        if include_coupling {
            match miner.detect_coupling(0.7).await {
                Ok(couplings) => {
                    for coupling in couplings.iter().take(15) {
                        // Create convention memory for strong couplings
                        let memory = codegraph_memory::MemoryNode::builder()
                            .convention(
                                format!("Co-change: {} ↔ {}", coupling.file_a, coupling.file_b),
                                format!(
                                    "These files change together {:.0}% of the time ({} of {} changes). \
                                     When modifying one, consider checking the other.",
                                    coupling.coupling_strength * 100.0,
                                    coupling.co_change_count,
                                    coupling.total_changes
                                ),
                            )
                            .title(format!("Coupling: {} ↔ {}", 
                                coupling.file_a.split('/').next_back().unwrap_or(&coupling.file_a),
                                coupling.file_b.split('/').next_back().unwrap_or(&coupling.file_b)))
                            .content(format!(
                                "**File A:** {}\n**File B:** {}\n**Coupling Strength:** {:.1}%\n\
                                 **Co-changes:** {} out of {} total changes",
                                coupling.file_a,
                                coupling.file_b,
                                coupling.coupling_strength * 100.0,
                                coupling.co_change_count,
                                coupling.total_changes
                            ))
                            .tag("coupling")
                            .tag("git-mined")
                            .confidence(coupling.coupling_strength)
                            .build()
                            .ok();

                        if let Some(m) = memory {
                            if let Ok(id) = self.memory_manager.put(m).await {
                                result.memory_ids.push(id);
                                couplings_created += 1;
                            }
                        }
                    }
                }
                Err(e) => {
                    result
                        .warnings
                        .push(format!("Failed to detect coupling: {}", e));
                }
            }
        }

        Ok(serde_json::json!({
            "commitsProcessed": result.commits_processed,
            "memoriesCreated": result.memories_created + hotspots_created + couplings_created,
            "commitsSkipped": result.commits_skipped,
            "memoryIds": result.memory_ids,
            "warnings": result.warnings,
            "hotspotsDetected": hotspots_created,
            "couplingsDetected": couplings_created,
        }))
    }

    /// Mine git history for a specific file.
    pub async fn handle_mine_git_history_for_file(
        &self,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        use crate::git_mining::{GitMiner, MiningConfig};

        // Get file path from params
        let uri_str = params
            .get("uri")
            .and_then(|v| v.as_str())
            .ok_or_else(|| tower_lsp::jsonrpc::Error::invalid_params("Missing uri parameter"))?;

        let file_path = Url::parse(uri_str)
            .ok()
            .and_then(|url| url.to_file_path().ok())
            .ok_or_else(|| tower_lsp::jsonrpc::Error::invalid_params("Invalid file URI"))?;

        // Get workspace folder
        let workspace_folders = self.workspace_folders.read().await;
        let workspace_path = workspace_folders
            .first()
            .ok_or_else(tower_lsp::jsonrpc::Error::invalid_request)?;

        // Parse configuration from params
        let max_commits = params
            .get("maxCommits")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(100);

        let config = MiningConfig {
            max_commits,
            min_confidence: 0.7,
            ..Default::default()
        };

        // Create miner and run for specific file
        let miner = GitMiner::new(workspace_path)
            .map_err(|_| tower_lsp::jsonrpc::Error::invalid_request())?;

        let result = miner
            .mine_file(&file_path, &self.memory_manager, &self.graph, &config)
            .await
            .map_err(|e| {
                tracing::error!("Git mining for file failed: {}", e);
                tower_lsp::jsonrpc::Error::internal_error()
            })?;

        Ok(serde_json::json!({
            "file": file_path.to_string_lossy(),
            "commitsProcessed": result.commits_processed,
            "memoriesCreated": result.memories_created,
            "commitsSkipped": result.commits_skipped,
            "memoryIds": result.memory_ids,
            "warnings": result.warnings
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codegraph::{NodeType, PropertyMap};
    use std::path::Path;
    use tempfile::TempDir;

    /// Helper to create a test backend with an empty graph
    fn create_test_backend() -> CodeGraphBackend {
        let graph = Arc::new(RwLock::new(
            CodeGraph::in_memory().expect("Failed to create graph"),
        ));
        let query_engine = Arc::new(QueryEngine::new(Arc::clone(&graph)));
        CodeGraphBackend::new_for_test(graph, query_engine)
    }

    /// Helper to create a backend with a pre-populated graph
    async fn create_backend_with_nodes() -> (CodeGraphBackend, NodeId, NodeId) {
        use codegraph::PropertyValue;

        let graph = Arc::new(RwLock::new(
            CodeGraph::in_memory().expect("Failed to create graph"),
        ));

        let (func1_id, func2_id) = {
            let mut g = graph.write().await;

            // Create a function node
            let mut props1 = PropertyMap::new();
            props1.insert(
                "name".to_string(),
                PropertyValue::String("test_function".to_string()),
            );
            props1.insert(
                "path".to_string(),
                PropertyValue::String("/test/file.rs".to_string()),
            );
            props1.insert("line_start".to_string(), PropertyValue::Int(10));
            props1.insert("line_end".to_string(), PropertyValue::Int(20));
            props1.insert("col_start".to_string(), PropertyValue::Int(0));
            props1.insert("col_end".to_string(), PropertyValue::Int(50));
            props1.insert(
                "signature".to_string(),
                PropertyValue::String("fn test_function() -> bool".to_string()),
            );
            let func1_id = g.add_node(NodeType::Function, props1).unwrap();

            // Create another function that calls the first
            let mut props2 = PropertyMap::new();
            props2.insert(
                "name".to_string(),
                PropertyValue::String("caller_function".to_string()),
            );
            props2.insert(
                "path".to_string(),
                PropertyValue::String("/test/file.rs".to_string()),
            );
            props2.insert("line_start".to_string(), PropertyValue::Int(30));
            props2.insert("line_end".to_string(), PropertyValue::Int(40));
            props2.insert("col_start".to_string(), PropertyValue::Int(0));
            props2.insert("col_end".to_string(), PropertyValue::Int(50));
            props2.insert(
                "signature".to_string(),
                PropertyValue::String("fn caller_function()".to_string()),
            );
            let func2_id = g.add_node(NodeType::Function, props2).unwrap();

            // Create a call edge from func2 to func1
            g.add_edge(func2_id, func1_id, EdgeType::Calls, PropertyMap::new())
                .unwrap();

            (func1_id, func2_id)
        };

        let query_engine = Arc::new(QueryEngine::new(Arc::clone(&graph)));
        let backend = CodeGraphBackend::new_for_test(graph, query_engine);

        (backend, func1_id, func2_id)
    }

    /// Helper to add a function node to the symbol index
    fn add_func_to_index(
        backend: &CodeGraphBackend,
        path: &Path,
        func_id: NodeId,
        name: &str,
        start_line: u32,
        end_line: u32,
    ) {
        backend.symbol_index.add_node_for_test(
            path.to_path_buf(),
            func_id,
            name,
            "Function",
            start_line,
            end_line,
        );
    }

    #[test]
    fn test_backend_creation() {
        let backend = create_test_backend();
        assert!(backend.file_cache.is_empty());
    }

    #[tokio::test]
    async fn test_find_node_at_position_empty_graph() {
        let backend = create_test_backend();
        let graph = backend.graph.read().await;
        let path = Path::new("/test/file.rs");
        let position = Position {
            line: 10,
            character: 5,
        };

        let result = backend.find_node_at_position(&graph, path, position);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_find_node_at_position_with_indexed_symbol() {
        let (backend, func_id, _) = create_backend_with_nodes().await;

        // Add the node to the symbol index
        let path = Path::new("/test/file.rs");
        add_func_to_index(&backend, path, func_id, "test_function", 10, 20);

        let graph = backend.graph.read().await;
        // Position within the function (line 15, 0-indexed is 14, but we add 1 = 15)
        let position = Position {
            line: 14, // 0-indexed, will be converted to 15 (1-indexed)
            character: 10,
        };

        let result = backend.find_node_at_position(&graph, path, position);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some(func_id));
    }

    #[tokio::test]
    async fn test_find_nearest_node_exact_match() {
        let (backend, func_id, _) = create_backend_with_nodes().await;

        // Add the node to the symbol index
        let path = Path::new("/test/file.rs");
        add_func_to_index(&backend, path, func_id, "test_function", 10, 20);

        let graph = backend.graph.read().await;
        let position = Position {
            line: 14,
            character: 10,
        };

        let result = backend.find_nearest_node(&graph, path, position);
        assert!(result.is_ok());
        let (node_id, was_fallback) = result.unwrap().unwrap();
        assert_eq!(node_id, func_id);
        assert!(!was_fallback);
    }

    #[tokio::test]
    async fn test_find_nearest_node_fallback() {
        let (backend, func_id, _) = create_backend_with_nodes().await;

        // Add the node to the symbol index
        let path = Path::new("/test/file.rs");
        add_func_to_index(&backend, path, func_id, "test_function", 10, 20);

        let graph = backend.graph.read().await;
        // Position before the function
        let position = Position {
            line: 0,
            character: 0,
        };

        let result = backend.find_nearest_node(&graph, path, position);
        assert!(result.is_ok());
        let (node_id, was_fallback) = result.unwrap().unwrap();
        assert_eq!(node_id, func_id);
        assert!(was_fallback);
    }

    #[tokio::test]
    async fn test_get_connected_edges_outgoing() {
        let (backend, func1_id, func2_id) = create_backend_with_nodes().await;
        let graph = backend.graph.read().await;

        // func2 calls func1, so outgoing edges from func2 should include func1
        let edges = backend.get_connected_edges(&graph, func2_id, Direction::Outgoing);
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].0, func2_id); // source
        assert_eq!(edges[0].1, func1_id); // target
        assert_eq!(edges[0].2, EdgeType::Calls);
    }

    #[tokio::test]
    async fn test_get_connected_edges_incoming() {
        let (backend, func1_id, func2_id) = create_backend_with_nodes().await;
        let graph = backend.graph.read().await;

        // func1 is called by func2, so incoming edges to func1 should include func2
        let edges = backend.get_connected_edges(&graph, func1_id, Direction::Incoming);
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].0, func2_id); // source
        assert_eq!(edges[0].1, func1_id); // target
        assert_eq!(edges[0].2, EdgeType::Calls);
    }

    #[tokio::test]
    async fn test_get_connected_edges_both() {
        let (backend, func1_id, _func2_id) = create_backend_with_nodes().await;
        let graph = backend.graph.read().await;

        // Get both directions for func1
        let edges = backend.get_connected_edges(&graph, func1_id, Direction::Both);
        assert_eq!(edges.len(), 1);
    }

    #[tokio::test]
    async fn test_node_to_location() {
        let (backend, func_id, _) = create_backend_with_nodes().await;
        let graph = backend.graph.read().await;

        let result = backend.node_to_location(&graph, func_id);
        assert!(result.is_ok());

        let location = result.unwrap();
        assert!(location.uri.to_string().contains("file.rs"));
        // Line 10 (1-indexed) becomes 9 (0-indexed)
        assert_eq!(location.range.start.line, 9);
        assert_eq!(location.range.end.line, 19);
    }

    #[tokio::test]
    async fn test_node_to_location_missing_path() {
        use codegraph::PropertyValue;
        let backend = create_test_backend();

        let node_id = {
            let mut graph = backend.graph.write().await;
            let mut props = PropertyMap::new();
            props.insert(
                "name".to_string(),
                PropertyValue::String("orphan_node".to_string()),
            );
            // No path property set
            graph.add_node(NodeType::Function, props).unwrap()
        };

        let graph = backend.graph.read().await;
        let result = backend.node_to_location(&graph, node_id);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_node_source_code_from_property() {
        use codegraph::PropertyValue;
        let backend = create_test_backend();

        let node_id = {
            let mut graph = backend.graph.write().await;
            let mut props = PropertyMap::new();
            props.insert(
                "name".to_string(),
                PropertyValue::String("test_func".to_string()),
            );
            props.insert(
                "source".to_string(),
                PropertyValue::String("fn test_func() { }".to_string()),
            );
            graph.add_node(NodeType::Function, props).unwrap()
        };

        let result = backend.get_node_source_code(node_id).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some("fn test_func() { }".to_string()));
    }

    #[tokio::test]
    async fn test_get_node_source_code_from_file() {
        use codegraph::PropertyValue;
        let backend = create_test_backend();
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.rs");

        // Write test file
        std::fs::write(&file_path, "line 1\nline 2\nline 3\nline 4\nline 5").unwrap();

        let node_id = {
            let mut graph = backend.graph.write().await;
            let mut props = PropertyMap::new();
            props.insert(
                "name".to_string(),
                PropertyValue::String("test_func".to_string()),
            );
            props.insert(
                "path".to_string(),
                PropertyValue::String(file_path.to_str().unwrap().to_string()),
            );
            props.insert("line_start".to_string(), PropertyValue::Int(2));
            props.insert("line_end".to_string(), PropertyValue::Int(4));
            graph.add_node(NodeType::Function, props).unwrap()
        };

        let result = backend.get_node_source_code(node_id).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some("line 2\nline 3\nline 4".to_string()));
    }

    #[tokio::test]
    async fn test_extract_node_id_from_item_valid() {
        let backend = create_test_backend();

        let item = CallHierarchyItem {
            name: "test".to_string(),
            kind: SymbolKind::FUNCTION,
            tags: None,
            detail: None,
            uri: Url::parse("file:///test.rs").unwrap(),
            range: Range::default(),
            selection_range: Range::default(),
            data: Some(serde_json::json!({ "nodeId": "123" })),
        };

        let result = backend.extract_node_id_from_item(&item);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 123);
    }

    #[tokio::test]
    async fn test_extract_node_id_from_item_missing_data() {
        let backend = create_test_backend();

        let item = CallHierarchyItem {
            name: "test".to_string(),
            kind: SymbolKind::FUNCTION,
            tags: None,
            detail: None,
            uri: Url::parse("file:///test.rs").unwrap(),
            range: Range::default(),
            selection_range: Range::default(),
            data: None,
        };

        let result = backend.extract_node_id_from_item(&item);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_extract_node_id_from_item_invalid_node_id() {
        let backend = create_test_backend();

        let item = CallHierarchyItem {
            name: "test".to_string(),
            kind: SymbolKind::FUNCTION,
            tags: None,
            detail: None,
            uri: Url::parse("file:///test.rs").unwrap(),
            range: Range::default(),
            selection_range: Range::default(),
            data: Some(serde_json::json!({ "nodeId": "not_a_number" })),
        };

        let result = backend.extract_node_id_from_item(&item);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_cache_operations() {
        let backend = create_test_backend();
        let uri = Url::parse("file:///test.rs").unwrap();

        assert!(backend.file_cache.is_empty());

        // Simulate inserting a file info
        let file_info = FileInfo {
            file_path: "/test.rs".into(),
            file_id: 1,
            functions: vec![],
            classes: vec![],
            traits: vec![],
            imports: vec![],
            parse_time: std::time::Duration::from_millis(0),
            line_count: 0,
            byte_count: 0,
        };
        backend.file_cache.insert(uri.clone(), file_info);

        assert!(!backend.file_cache.is_empty());
        assert!(backend.file_cache.contains_key(&uri));

        // Remove
        backend.file_cache.remove(&uri);
        assert!(backend.file_cache.is_empty());
    }

    #[tokio::test]
    async fn test_query_cache_invalidation() {
        let backend = create_test_backend();

        // Add something to cache
        let path = PathBuf::from("/test/file.rs");
        backend.query_cache.set_definition(path.clone(), 0, 0, 123);

        // Verify it's cached
        let cached = backend.query_cache.get_definition(&path, 0, 0);
        assert!(cached.is_some());
        assert_eq!(cached.unwrap(), 123);

        // Invalidate
        backend.query_cache.invalidate_file(&path);

        // Verify it's gone
        let cached = backend.query_cache.get_definition(&path, 0, 0);
        assert!(cached.is_none());
    }

    #[tokio::test]
    async fn test_symbol_index_integration() {
        let (backend, func_id, _) = create_backend_with_nodes().await;
        let path = Path::new("/test/file.rs");

        // Add to symbol index using test helper
        add_func_to_index(&backend, path, func_id, "test_function", 10, 20);

        // Search by name
        let results = backend.symbol_index.search_by_name("test_function");
        assert!(!results.is_empty());
        assert!(results.contains(&func_id));

        // Get file symbols
        let file_symbols = backend.symbol_index.get_file_symbols(path);
        assert!(!file_symbols.is_empty());

        // Remove file
        backend.symbol_index.remove_file(path);
        let file_symbols = backend.symbol_index.get_file_symbols(path);
        assert!(file_symbols.is_empty());
    }
}
