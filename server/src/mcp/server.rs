//! MCP Server Implementation
//!
//! Handles MCP protocol requests and routes them to CodeGraph functionality.

use super::protocol::*;
use super::resources::get_all_resources;
use super::tools::get_all_tools;
use super::transport::AsyncStdioTransport;
use crate::ai_query::QueryEngine;
use crate::domain::node_props;
use crate::git_mining::{GitExecutor, GitMiner, MiningConfig};
use crate::memory::{self, MemoryManager};
use crate::parser_registry::ParserRegistry;
use codegraph::{CodeGraph, NamespacedBackend, RocksDBBackend, StorageBackend};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

const PROTOCOL_VERSION: &str = "2024-11-05";
const SERVER_NAME: &str = "codegraph";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// MCP Backend - wraps CodeGraph components for MCP access
#[derive(Clone)]
pub struct McpBackend {
    pub graph: Arc<RwLock<CodeGraph>>,
    pub parsers: Arc<ParserRegistry>,
    pub query_engine: Arc<QueryEngine>,
    pub memory_manager: Arc<MemoryManager>,
    pub workspace_folders: Vec<PathBuf>,
    /// Project slug used as namespace in the shared graph database
    pub project_slug: String,
}

impl McpBackend {
    /// Create a new MCP backend for the given workspace.
    ///
    /// Starts with a fresh in-memory graph (re-indexes all files on startup).
    /// After indexing, persists to the shared database at `~/.codegraph/graph.db`
    /// (namespaced by project slug) for cross-project access.
    pub fn new(workspace: PathBuf) -> Self {
        let slug = memory::project_slug(&workspace);
        tracing::info!("Project slug: {}", slug);

        let graph = Arc::new(RwLock::new(
            CodeGraph::in_memory().expect("Failed to create in-memory graph"),
        ));

        // Resolve extension path from binary location for model discovery
        // In dev: target/debug/codegraph-lsp -> project root (go up 3 levels)
        // In prod: extension/bin/codegraph-lsp -> extension root (go up 2 levels)
        let extension_path = std::env::current_exe().ok().and_then(|exe| {
            let exe_dir = exe.parent()?;
            // Check if we're in target/debug or target/release
            if exe_dir.ends_with("debug") || exe_dir.ends_with("release") {
                // Dev environment: go up to project root (target -> project)
                exe_dir.parent()?.parent().map(|p| p.to_path_buf())
            } else {
                // Prod environment: assume bin/ -> extension root
                exe_dir.parent().map(|p| p.to_path_buf())
            }
        });

        tracing::info!("Extension path for models: {:?}", extension_path);

        Self {
            query_engine: Arc::new(QueryEngine::new(Arc::clone(&graph))),
            graph,
            parsers: Arc::new(ParserRegistry::new()),
            memory_manager: Arc::new(MemoryManager::new(extension_path)),
            workspace_folders: vec![workspace],
            project_slug: slug,
        }
    }

    /// Open the shared graph database with project-scoped namespacing.
    ///
    /// Opens RocksDB at `~/.codegraph/graph.db`, wraps with NamespacedBackend,
    /// loads all data into in-memory caches, then detaches storage to release
    /// the database lock. Used for cross-project graph access (T1-4).
    fn open_persistent_graph(slug: &str) -> Result<CodeGraph, String> {
        let db_path = memory::shared_graph_db_path().map_err(|e| format!("{e}"))?;

        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create ~/.codegraph: {e}"))?;
        }

        let rocks =
            RocksDBBackend::open(&db_path).map_err(|e| format!("Failed to open graph.db: {e}"))?;
        let namespaced = NamespacedBackend::new(Box::new(rocks), slug);
        let mut graph = CodeGraph::with_backend(Box::new(namespaced))
            .map_err(|e| format!("Failed to load graph: {e}"))?;

        // Detach to release the RocksDB lock — all data is now in memory
        graph
            .detach_storage()
            .map_err(|e| format!("Failed to detach storage: {e}"))?;

        Ok(graph)
    }

    /// Persist the current graph state to the shared database.
    ///
    /// Opens RocksDB briefly, writes registry entry + all data with namespace prefix, then closes.
    fn persist_graph(&self, graph: &CodeGraph) -> Result<(), String> {
        let db_path = memory::shared_graph_db_path().map_err(|e| format!("{e}"))?;

        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create ~/.codegraph: {e}"))?;
        }

        let mut rocks = RocksDBBackend::open(&db_path)
            .map_err(|e| format!("Failed to open graph.db for persist: {e}"))?;

        // Write project registry entry (un-namespaced, global key)
        let workspace_path = self
            .workspace_folders
            .first()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let registry_value = serde_json::json!({
            "slug": self.project_slug,
            "workspace": workspace_path,
            "node_count": graph.node_count(),
            "edge_count": graph.edge_count(),
            "last_indexed": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        });
        let registry_key = format!("_registry:{}", self.project_slug);
        rocks
            .put(
                registry_key.as_bytes(),
                registry_value.to_string().as_bytes(),
            )
            .map_err(|e| format!("Failed to write registry: {e}"))?;

        // Write graph data with namespace prefix
        let namespaced = NamespacedBackend::new(Box::new(rocks), &self.project_slug);

        graph
            .persist_to(Box::new(namespaced))
            .map_err(|e| format!("Failed to persist graph: {e}"))?;

        tracing::info!(
            "Persisted {} nodes, {} edges to graph.db (namespace: {})",
            graph.node_count(),
            graph.edge_count(),
            self.project_slug
        );
        Ok(())
    }

    /// List all projects indexed in the shared graph database.
    ///
    /// Scans `_registry:*` keys to discover project metadata without loading graphs.
    fn list_indexed_projects() -> Result<Vec<serde_json::Value>, String> {
        let db_path = memory::shared_graph_db_path().map_err(|e| format!("{e}"))?;

        if !db_path.exists() {
            return Ok(vec![]);
        }

        let rocks =
            RocksDBBackend::open(&db_path).map_err(|e| format!("Failed to open graph.db: {e}"))?;

        let entries = rocks
            .scan_prefix(b"_registry:")
            .map_err(|e| format!("Failed to scan registry: {e}"))?;

        let mut projects = Vec::new();
        for (_key, value) in entries {
            if let Ok(metadata) = serde_json::from_slice::<serde_json::Value>(&value) {
                projects.push(metadata);
            }
        }

        Ok(projects)
    }

    /// Search for symbols across all other indexed projects.
    ///
    /// Opens each project's graph from the shared DB (excluding the current project),
    /// searches for matching symbols by name substring, and returns aggregated results.
    fn cross_project_search(
        &self,
        query: &str,
        symbol_type: Option<&str>,
        limit: usize,
    ) -> Result<serde_json::Value, String> {
        let projects = Self::list_indexed_projects()?;
        let query_lower = query.to_lowercase();

        let mut all_results = Vec::new();
        let mut searched_projects = Vec::new();

        for project in &projects {
            let slug = project.get("slug").and_then(|v| v.as_str()).unwrap_or("");
            let workspace = project
                .get("workspace")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            // Skip the current project
            if slug == self.project_slug {
                continue;
            }

            // Open the project graph from shared DB
            let graph = match Self::open_persistent_graph(slug) {
                Ok(g) => g,
                Err(e) => {
                    tracing::warn!("Failed to open project {}: {}", slug, e);
                    continue;
                }
            };

            searched_projects.push(serde_json::json!({
                "slug": slug,
                "workspace": workspace,
                "node_count": graph.node_count(),
            }));

            // Search nodes by name substring match
            let type_filter: Option<codegraph::NodeType> = symbol_type.and_then(|st| match st {
                "function" | "method" => Some(codegraph::NodeType::Function),
                "class" => Some(codegraph::NodeType::Class),
                "variable" => Some(codegraph::NodeType::Variable),
                "interface" => Some(codegraph::NodeType::Interface),
                "type" => Some(codegraph::NodeType::Type),
                "module" => Some(codegraph::NodeType::Module),
                _ => None,
            });

            for (_id, node) in graph.iter_nodes() {
                if all_results.len() >= limit {
                    break;
                }

                // Apply type filter
                if let Some(ref tf) = type_filter {
                    if &node.node_type != tf {
                        continue;
                    }
                }

                // Skip CodeFile nodes
                if node.node_type == codegraph::NodeType::CodeFile {
                    continue;
                }

                let name = node_props::name(node);
                if !name.to_lowercase().contains(&query_lower) {
                    continue;
                }

                let file_path = node_props::path(node);
                let line_start = node_props::line_start(node);
                let line_end = node_props::line_end(node);
                let signature = node.properties.get_string("signature").unwrap_or("");

                let mut result = serde_json::json!({
                    "name": name,
                    "kind": format!("{}", node.node_type),
                    "project": slug,
                    "project_workspace": workspace,
                    "file": file_path,
                    "line_start": line_start,
                    "line_end": line_end,
                });

                if !signature.is_empty() {
                    result["signature"] = serde_json::Value::String(signature.to_string());
                }
                if let Some(route) = node.properties.get_string("route") {
                    result["route"] = serde_json::Value::String(route.to_string());
                    if let Some(method) = node.properties.get_string("http_method") {
                        result["http_method"] = serde_json::Value::String(method.to_string());
                    }
                }

                all_results.push(result);
            }
        }

        Ok(serde_json::json!({
            "query": query,
            "current_project": self.project_slug,
            "searched_projects": searched_projects,
            "results": all_results,
            "total": all_results.len(),
        }))
    }

    /// Index the workspace
    pub async fn index_workspace(&self) -> usize {
        let mut total = 0;
        for folder in &self.workspace_folders {
            total += self.index_directory(folder).await;

            // Initialize memory manager with workspace path
            if let Err(e) = self.memory_manager.initialize(folder).await {
                tracing::warn!("Failed to initialize memory manager: {:?}", e);
            }
        }

        // Resolve cross-file imports and calls before building indexes
        {
            let mut graph = self.graph.write().await;
            crate::watcher::GraphUpdater::resolve_cross_file_imports(&mut graph);
        }

        // Detect runtime dependencies: HTTP routes and client calls
        {
            let mut graph = self.graph.write().await;
            let routes = crate::runtime_deps::detect_route_handlers(&mut graph);
            let clients = crate::runtime_deps::detect_http_client_calls(&mut graph);
            if routes > 0 || clients > 0 {
                let edges = crate::runtime_deps::create_runtime_call_edges(&mut graph);
                tracing::info!(
                    "Runtime deps: {} routes, {} clients, {} edges",
                    routes,
                    clients,
                    edges
                );
            }
        }

        // Persist graph to shared database
        {
            let graph = self.graph.read().await;
            if let Err(e) = self.persist_graph(&graph) {
                tracing::warn!("Failed to persist graph: {}", e);
            }
        }

        // Build query engine indexes
        self.query_engine.build_indexes().await;

        // Share vector engine with query engine for semantic symbol search
        if let Some(engine) = self.memory_manager.get_vector_engine().await {
            self.query_engine.set_vector_engine(engine).await;
            self.query_engine.build_symbol_vectors().await;
            tracing::info!("Semantic symbol search initialized for MCP");
        }

        total
    }

    /// Index a directory recursively
    async fn index_directory(&self, dir: &std::path::Path) -> usize {
        use std::fs;

        let mut indexed_count = 0;
        let supported_extensions = self.parsers.supported_extensions();

        tracing::info!("Indexing directory: {:?}", dir);
        tracing::debug!("Supported extensions: {:?}", supported_extensions);

        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();

                // Skip hidden files/directories and common exclusions
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with('.')
                        || name == "node_modules"
                        || name == "target"
                        || name == "__pycache__"
                        || name == ".git"
                        || name == "dist"
                        || name == "build"
                        || name == "out"
                        || name == "vendor"
                        || name == "DerivedData"
                        || name == "tmp"
                    {
                        continue;
                    }
                }

                if path.is_dir() {
                    indexed_count += Box::pin(self.index_directory(&path)).await;
                } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    // supported_extensions includes the dot (e.g., ".rs"), but path.extension() doesn't
                    let ext_with_dot = format!(".{}", ext);
                    let is_supported = supported_extensions
                        .iter()
                        .any(|e| *e == ext || *e == ext_with_dot);
                    if is_supported {
                        match self.index_file(&path).await {
                            Ok(()) => {
                                tracing::debug!("Indexed file: {:?}", path);
                                indexed_count += 1;
                            }
                            Err(e) => {
                                tracing::warn!("Failed to index {:?}: {}", path, e);
                            }
                        }
                    }
                }
            }
        }

        indexed_count
    }

    /// Index a single file
    async fn index_file(&self, path: &std::path::Path) -> Result<(), String> {
        let mut graph = self.graph.write().await;

        // The parser directly modifies the graph
        match self.parsers.parse_file(path, &mut graph) {
            Ok(_file_info) => Ok(()),
            Err(e) => Err(format!("{:?}", e)),
        }
    }
}

/// MCP Server - handles protocol messages
pub struct McpServer {
    backend: McpBackend,
    initialized: bool,
}

impl McpServer {
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            backend: McpBackend::new(workspace),
            initialized: false,
        }
    }

    /// Run the MCP server event loop
    pub async fn run(&mut self) -> std::io::Result<()> {
        let mut transport = AsyncStdioTransport::new();

        tracing::info!("MCP server starting...");

        loop {
            match transport.read_request().await {
                Ok(Some(request)) => {
                    let response = self.handle_request(request).await;
                    transport.write_response(&response).await?;
                }
                Ok(None) => {
                    // Empty line, keep reading
                    continue;
                }
                Err(e) => {
                    if e.kind() == std::io::ErrorKind::UnexpectedEof {
                        tracing::info!("Client disconnected");
                        break;
                    }
                    let response = JsonRpcResponse::error(
                        None,
                        JsonRpcError::parse_error(format!("Parse error: {}", e)),
                    );
                    transport.write_response(&response).await?;
                }
            }
        }

        Ok(())
    }

    /// Handle a JSON-RPC request
    async fn handle_request(&mut self, request: JsonRpcRequest) -> JsonRpcResponse {
        tracing::debug!("Handling request: {}", request.method);

        match request.method.as_str() {
            "initialize" => self.handle_initialize(request.id, request.params).await,
            "initialized" => {
                // Notification, no response needed but we return success
                JsonRpcResponse::success(request.id, Value::Null)
            }
            "ping" => {
                JsonRpcResponse::success(request.id, serde_json::to_value(PingResult {}).unwrap())
            }
            "tools/list" => self.handle_tools_list(request.id).await,
            "tools/call" => self.handle_tools_call(request.id, request.params).await,
            "resources/list" => self.handle_resources_list(request.id).await,
            "resources/read" => self.handle_resources_read(request.id, request.params).await,
            "notifications/cancelled" => {
                // Notification, no response needed
                JsonRpcResponse::success(request.id, Value::Null)
            }
            _ => {
                JsonRpcResponse::error(request.id, JsonRpcError::method_not_found(&request.method))
            }
        }
    }

    async fn handle_initialize(
        &mut self,
        id: Option<Value>,
        params: Option<Value>,
    ) -> JsonRpcResponse {
        let init_params: InitializeParams = params
            .map(|p| serde_json::from_value(p).unwrap_or_default())
            .unwrap_or_default();

        // If the client provides roots, use them as workspace folders.
        // This allows a globally-configured MCP server to index the
        // correct project without per-project .mcp.json or --workspace.
        if let Some(roots) = &init_params.roots {
            let root_paths: Vec<PathBuf> = roots
                .iter()
                .filter_map(|r| {
                    r.uri
                        .strip_prefix("file://")
                        .map(PathBuf::from)
                        .or_else(|| {
                            // Accept bare paths too
                            let p = PathBuf::from(&r.uri);
                            if p.is_absolute() {
                                Some(p)
                            } else {
                                None
                            }
                        })
                })
                .filter(|p| p.is_dir())
                .collect();

            if !root_paths.is_empty() {
                tracing::info!(
                    "Using {} workspace root(s) from client: {:?}",
                    root_paths.len(),
                    root_paths
                );
                self.backend.workspace_folders = root_paths;
                // Recompute project slug from first root
                self.backend.project_slug =
                    crate::memory::project_slug(&self.backend.workspace_folders[0]);
            }
        }

        if let Some(ref client_info) = init_params.client_info {
            tracing::info!(
                "Client: {} {}",
                client_info.name,
                client_info.version.as_deref().unwrap_or("(unknown)")
            );
        }

        // Index the workspace
        tracing::info!("Indexing workspace: {:?}", self.backend.workspace_folders);
        let indexed = self.backend.index_workspace().await;
        tracing::info!("Indexed {} files", indexed);

        self.initialized = true;

        let result = InitializeResult {
            protocol_version: PROTOCOL_VERSION.to_string(),
            capabilities: ServerCapabilities {
                experimental: None,
                logging: Some(LoggingCapability {}),
                prompts: None,
                resources: Some(ResourcesCapability {
                    subscribe: Some(false),
                    list_changed: Some(false),
                }),
                tools: Some(ToolsCapability {
                    list_changed: Some(false),
                }),
            },
            server_info: ServerInfo {
                name: SERVER_NAME.to_string(),
                version: Some(SERVER_VERSION.to_string()),
            },
        };

        JsonRpcResponse::success(id, serde_json::to_value(result).unwrap())
    }

    async fn handle_tools_list(&self, id: Option<Value>) -> JsonRpcResponse {
        let result = ToolsListResult {
            tools: get_all_tools(),
        };
        JsonRpcResponse::success(id, serde_json::to_value(result).unwrap())
    }

    async fn handle_tools_call(&self, id: Option<Value>, params: Option<Value>) -> JsonRpcResponse {
        let params: ToolCallParams = match params {
            Some(p) => match serde_json::from_value(p) {
                Ok(p) => p,
                Err(e) => {
                    return JsonRpcResponse::error(
                        id,
                        JsonRpcError::invalid_params(format!("Invalid params: {}", e)),
                    );
                }
            },
            None => {
                return JsonRpcResponse::error(id, JsonRpcError::invalid_params("Missing params"));
            }
        };

        match self.execute_tool(&params.name, params.arguments).await {
            Ok(result) => {
                let tool_result = ToolCallResult {
                    content: vec![ToolResultContent::Text {
                        text: serde_json::to_string_pretty(&result)
                            .unwrap_or_else(|_| result.to_string()),
                    }],
                    is_error: None,
                };
                JsonRpcResponse::success(id, serde_json::to_value(tool_result).unwrap())
            }
            Err(e) => {
                let tool_result = ToolCallResult {
                    content: vec![ToolResultContent::Text {
                        text: format!("Error: {}", e),
                    }],
                    is_error: Some(true),
                };
                JsonRpcResponse::success(id, serde_json::to_value(tool_result).unwrap())
            }
        }
    }

    async fn handle_resources_list(&self, id: Option<Value>) -> JsonRpcResponse {
        let result = ResourcesListResult {
            resources: get_all_resources(),
        };
        JsonRpcResponse::success(id, serde_json::to_value(result).unwrap())
    }

    async fn handle_resources_read(
        &self,
        id: Option<Value>,
        params: Option<Value>,
    ) -> JsonRpcResponse {
        let params: ResourceReadParams = match params {
            Some(p) => match serde_json::from_value(p) {
                Ok(p) => p,
                Err(e) => {
                    return JsonRpcResponse::error(
                        id,
                        JsonRpcError::invalid_params(format!("Invalid params: {}", e)),
                    );
                }
            },
            None => {
                return JsonRpcResponse::error(id, JsonRpcError::invalid_params("Missing params"));
            }
        };

        match super::resources::read_resource(
            &params.uri,
            Arc::clone(&self.backend.graph),
            &self.backend.memory_manager,
            &self.backend.workspace_folders,
        )
        .await
        {
            Some(result) => JsonRpcResponse::success(id, serde_json::to_value(result).unwrap()),
            None => JsonRpcResponse::error(
                id,
                JsonRpcError::invalid_params(format!("Resource not found: {}", params.uri)),
            ),
        }
    }

    /// Execute a tool by name - delegates to query engine and other components
    async fn execute_tool(&self, name: &str, args: Option<Value>) -> Result<Value, String> {
        let args = args.unwrap_or(Value::Object(serde_json::Map::new()));

        match name {
            // ==================== Search Tools ====================
            "codegraph_symbol_search" => {
                let query = args
                    .get("query")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing 'query' parameter")?;
                let limit = args
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize)
                    .unwrap_or(20);
                let compact = args
                    .get("compact")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                // Parse symbolType filter
                let symbol_types: Vec<crate::ai_query::SymbolType> = args
                    .get("symbolType")
                    .or_else(|| args.get("symbol_type"))
                    .and_then(|v| {
                        // Accept either a single string or "any"
                        v.as_str().and_then(|s| {
                            if s == "any" {
                                None
                            } else {
                                Self::parse_symbol_type(s).map(|st| vec![st])
                            }
                        })
                    })
                    .unwrap_or_default();

                let include_private = args
                    .get("includePrivate")
                    .or_else(|| args.get("include_private"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);

                let options = crate::ai_query::SearchOptions::new()
                    .with_limit(limit)
                    .with_compact(compact)
                    .with_symbol_types(symbol_types)
                    .with_include_private(include_private);
                let mut result = self
                    .backend
                    .query_engine
                    .symbol_search(query, &options)
                    .await;

                // Deduplicate by node_id
                let mut seen = std::collections::HashSet::new();
                result.results.retain(|m| seen.insert(m.node_id));

                Ok(serde_json::to_value(result).map_err(|e| e.to_string())?)
            }

            "codegraph_find_entry_points" => {
                let entry_type = args
                    .get("entryType")
                    .or_else(|| args.get("entry_type"))
                    .and_then(|v| v.as_str());

                let entry_types = match entry_type {
                    Some("http") | Some("http_handler") | Some("HttpHandler") => {
                        vec![crate::ai_query::EntryType::HttpHandler]
                    }
                    Some("cli") | Some("cli_command") | Some("CliCommand") => {
                        vec![crate::ai_query::EntryType::CliCommand]
                    }
                    Some("public") | Some("public_api") | Some("PublicApi") => {
                        vec![crate::ai_query::EntryType::PublicApi]
                    }
                    Some("event") | Some("event_handler") | Some("EventHandler") => {
                        vec![crate::ai_query::EntryType::EventHandler]
                    }
                    Some("test") | Some("TestEntry") => vec![crate::ai_query::EntryType::TestEntry],
                    Some("main") | Some("Main") => vec![crate::ai_query::EntryType::Main],
                    Some("all") | None => vec![
                        crate::ai_query::EntryType::HttpHandler,
                        crate::ai_query::EntryType::CliCommand,
                        crate::ai_query::EntryType::PublicApi,
                        crate::ai_query::EntryType::Main,
                        crate::ai_query::EntryType::EventHandler,
                        crate::ai_query::EntryType::TestEntry,
                    ],
                    _ => vec![
                        crate::ai_query::EntryType::HttpHandler,
                        crate::ai_query::EntryType::CliCommand,
                        crate::ai_query::EntryType::PublicApi,
                        crate::ai_query::EntryType::Main,
                    ],
                };

                let compact = args
                    .get("compact")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let limit = args
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize);

                let result = self
                    .backend
                    .query_engine
                    .find_entry_points_opts(&entry_types, compact, limit)
                    .await;

                // Deduplicate by node_id
                let mut seen = std::collections::HashSet::new();
                let deduped: Vec<_> = result
                    .into_iter()
                    .filter(|e| seen.insert(e.node_id))
                    .collect();

                Ok(serde_json::to_value(deduped).map_err(|e| e.to_string())?)
            }

            "codegraph_find_by_imports" => {
                let module_name = args
                    .get("moduleName")
                    .or_else(|| args.get("module_name"))
                    .and_then(|v| v.as_str());
                let libraries = args
                    .get("libraries")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let match_mode_str = args
                    .get("matchMode")
                    .or_else(|| args.get("match_mode"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("contains");
                let limit = args
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize)
                    .unwrap_or(50);

                // Determine which library to search for
                let library = if let Some(name) = module_name {
                    name.to_string()
                } else if let Some(first) = libraries.first() {
                    first.clone()
                } else {
                    return Err("Missing 'moduleName' or 'libraries' parameter".to_string());
                };

                let match_mode = match match_mode_str {
                    "exact" => crate::ai_query::ImportMatchMode::Exact,
                    "prefix" => crate::ai_query::ImportMatchMode::Prefix,
                    _ => crate::ai_query::ImportMatchMode::Fuzzy,
                };

                let options = crate::ai_query::ImportSearchOptions {
                    match_mode,
                    ..Default::default()
                };

                let result = self
                    .backend
                    .query_engine
                    .find_by_imports(&library, &options)
                    .await;

                // Deduplicate by node_id and apply limit
                let mut seen = std::collections::HashSet::new();
                let deduped: Vec<_> = result
                    .into_iter()
                    .filter(|m| seen.insert(m.node_id))
                    .take(limit)
                    .collect();

                Ok(serde_json::to_value(deduped).map_err(|e| e.to_string())?)
            }

            "codegraph_find_by_signature" => {
                let name_pattern = args
                    .get("namePattern")
                    .or_else(|| args.get("name_pattern"))
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let return_type = args
                    .get("returnType")
                    .or_else(|| args.get("return_type"))
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let exact_param_count = args
                    .get("paramCount")
                    .or_else(|| args.get("param_count"))
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize);
                let min_params = args
                    .get("minParams")
                    .or_else(|| args.get("min_params"))
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize);
                let max_params = args
                    .get("maxParams")
                    .or_else(|| args.get("max_params"))
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize);
                let modifiers = args
                    .get("modifiers")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let limit = args
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize);

                // param_count is Option<(min, max)>
                let param_count = if let Some(exact) = exact_param_count {
                    Some((exact, exact))
                } else if min_params.is_some() || max_params.is_some() {
                    Some((min_params.unwrap_or(0), max_params.unwrap_or(usize::MAX)))
                } else {
                    None
                };

                let pattern = crate::ai_query::SignaturePattern {
                    name_pattern,
                    return_type,
                    param_count,
                    modifiers,
                };

                let result = self
                    .backend
                    .query_engine
                    .find_by_signature(&pattern, limit)
                    .await;

                // Deduplicate by node_id
                let mut seen = std::collections::HashSet::new();
                let deduped: Vec<_> = result
                    .into_iter()
                    .filter(|m| seen.insert(m.node_id))
                    .collect();

                Ok(serde_json::to_value(deduped).map_err(|e| e.to_string())?)
            }

            // ==================== Graph Traversal Tools ====================
            "codegraph_get_callers" => {
                let uri = args.get("uri").and_then(|v| v.as_str());
                let line = args.get("line").and_then(|v| v.as_u64()).map(|v| v as u32);
                let node_id = args
                    .get("nodeId")
                    .or_else(|| args.get("node_id"))
                    .and_then(|v| v.as_str());
                let depth = args
                    .get("depth")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32)
                    .unwrap_or(1);

                // Use fallback for uri+line, exact match for node_id
                let (start_node, used_fallback) = if let Some(id_str) = node_id {
                    (parse_node_id(id_str), false)
                } else if let (Some(u), Some(l)) = (uri, line) {
                    match self.find_nearest_node_with_fallback(u, l).await {
                        Some((id, fallback)) => (Some(id), fallback),
                        None => (None, false),
                    }
                } else {
                    (None, false)
                };

                if let Some(start) = start_node {
                    let result = crate::domain::callers::get_callers(
                        &self.backend.graph,
                        &self.backend.query_engine,
                        start,
                        depth,
                        used_fallback,
                        line,
                    )
                    .await;
                    Ok(serde_json::to_value(&result).unwrap_or_default())
                } else {
                    Ok(serde_json::json!({
                        "callers": [],
                        "message": "Could not find starting node. Provide either nodeId or uri+line."
                    }))
                }
            }

            "codegraph_get_callees" => {
                let uri = args.get("uri").and_then(|v| v.as_str());
                let line = args.get("line").and_then(|v| v.as_u64()).map(|v| v as u32);
                let node_id = args
                    .get("nodeId")
                    .or_else(|| args.get("node_id"))
                    .and_then(|v| v.as_str());
                let depth = args
                    .get("depth")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32)
                    .unwrap_or(1);

                // Use fallback for uri+line, exact match for node_id
                let (start_node, used_fallback) = if let Some(id_str) = node_id {
                    (parse_node_id(id_str), false)
                } else if let (Some(u), Some(l)) = (uri, line) {
                    match self.find_nearest_node_with_fallback(u, l).await {
                        Some((id, fallback)) => (Some(id), fallback),
                        None => (None, false),
                    }
                } else {
                    (None, false)
                };

                if let Some(start) = start_node {
                    let result = crate::domain::callers::get_callees(
                        &self.backend.graph,
                        &self.backend.query_engine,
                        start,
                        depth,
                        used_fallback,
                        line,
                    )
                    .await;
                    Ok(serde_json::to_value(&result).unwrap_or_default())
                } else {
                    Ok(serde_json::json!({
                        "callees": [],
                        "message": "Could not find starting node. Provide either nodeId or uri+line."
                    }))
                }
            }

            "codegraph_traverse_graph" => {
                let uri = args.get("uri").and_then(|v| v.as_str());
                let line = args.get("line").and_then(|v| v.as_u64()).map(|v| v as u32);
                let node_id = args
                    .get("startNodeId")
                    .or_else(|| args.get("nodeId"))
                    .or_else(|| args.get("node_id"))
                    .and_then(|v| v.as_str());
                let direction_str = args
                    .get("direction")
                    .and_then(|v| v.as_str())
                    .unwrap_or("outgoing");
                let max_depth = args
                    .get("maxDepth")
                    .or_else(|| args.get("max_depth"))
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32)
                    .unwrap_or(3);
                let limit = args
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize)
                    .unwrap_or(100);

                // Use fallback for uri+line, exact match for node_id
                let (start_node, used_fallback) = if let Some(id_str) = node_id {
                    (parse_node_id(id_str), false)
                } else if let (Some(u), Some(l)) = (uri, line) {
                    match self.find_nearest_node_with_fallback(u, l).await {
                        Some((id, fallback)) => (Some(id), fallback),
                        None => (None, false),
                    }
                } else {
                    (None, false)
                };

                // Parse edgeTypes filter
                let edge_types: Vec<String> = args
                    .get("edgeTypes")
                    .or_else(|| args.get("edge_types"))
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();

                // Parse nodeTypes filter
                let node_types: Vec<crate::ai_query::SymbolType> = args
                    .get("nodeTypes")
                    .or_else(|| args.get("node_types"))
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str())
                            .filter_map(Self::parse_symbol_type)
                            .collect()
                    })
                    .unwrap_or_default();

                let summary = args
                    .get("summary")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                if let Some(start) = start_node {
                    let direction = match direction_str {
                        "incoming" => crate::ai_query::TraversalDirection::Incoming,
                        "both" => crate::ai_query::TraversalDirection::Both,
                        _ => crate::ai_query::TraversalDirection::Outgoing,
                    };

                    let filter = crate::ai_query::TraversalFilter {
                        symbol_types: node_types,
                        edge_types,
                        max_nodes: limit,
                    };

                    let result = self
                        .backend
                        .query_engine
                        .traverse_graph(start, direction, max_depth, &filter)
                        .await;

                    if summary {
                        let node_count = result.len();
                        let edge_types_seen: Vec<String> = result
                            .iter()
                            .filter(|n| !n.edge_type.is_empty())
                            .map(|n| n.edge_type.clone())
                            .collect::<std::collections::HashSet<_>>()
                            .into_iter()
                            .collect();
                        Ok(serde_json::json!({
                            "summary": {
                                "node_count": node_count,
                                "max_depth": max_depth,
                                "direction": direction_str,
                                "edge_types_seen": edge_types_seen,
                            }
                        }))
                    } else {
                        // Add fallback metadata if used
                        let mut response =
                            serde_json::to_value(result).map_err(|e| e.to_string())?;
                        if used_fallback {
                            if let Some(obj) = response.as_object_mut() {
                                let symbol_name = {
                                    let graph = self.backend.graph.read().await;
                                    graph
                                        .get_node(start)
                                        .ok()
                                        .and_then(|n| {
                                            n.properties.get_string("name").map(|s| s.to_string())
                                        })
                                        .unwrap_or_default()
                                };
                                obj.insert("used_fallback".to_string(), serde_json::json!(true));
                                obj.insert(
                                    "fallback_message".to_string(),
                                    serde_json::json!(format!(
                                        "No symbol at line {}. Using nearest symbol '{}' instead.",
                                        line.unwrap_or(0),
                                        symbol_name
                                    )),
                                );
                            }
                        }
                        Ok(response)
                    }
                } else {
                    Ok(serde_json::json!({
                        "nodes": [],
                        "edges": [],
                        "message": "Could not find starting node. Provide either startNodeId or uri+line."
                    }))
                }
            }

            "codegraph_get_symbol_info" => {
                let uri = args.get("uri").and_then(|v| v.as_str());
                let line = args.get("line").and_then(|v| v.as_u64()).map(|v| v as u32);
                let node_id = args
                    .get("nodeId")
                    .or_else(|| args.get("node_id"))
                    .and_then(|v| v.as_str());
                let include_refs = args
                    .get("includeReferences")
                    .or_else(|| args.get("include_references"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                // Use fallback for uri+line, exact match for node_id
                let (target_node, used_fallback) = if let Some(id_str) = node_id {
                    (parse_node_id(id_str), false)
                } else if let (Some(u), Some(l)) = (uri, line) {
                    match self.find_nearest_node_with_fallback(u, l).await {
                        Some((id, fallback)) => (Some(id), fallback),
                        None => (None, false),
                    }
                } else {
                    (None, false)
                };

                if let Some(node_id) = target_node {
                    match crate::domain::symbol_info::get_symbol_info(
                        &self.backend.graph,
                        &self.backend.query_engine,
                        node_id,
                        include_refs,
                        used_fallback,
                        line,
                    )
                    .await
                    {
                        Some(response) => Ok(serde_json::to_value(&response).unwrap_or_default()),
                        None => Ok(serde_json::json!({
                            "error": "Symbol not found"
                        })),
                    }
                } else {
                    Ok(serde_json::json!({
                        "error": "Could not find symbol. Provide either nodeId or uri+line."
                    }))
                }
            }

            "codegraph_get_detailed_symbol" => {
                let uri = args.get("uri").and_then(|v| v.as_str());
                let line = args.get("line").and_then(|v| v.as_u64()).map(|v| v as u32);
                let node_id = args
                    .get("nodeId")
                    .or_else(|| args.get("node_id"))
                    .and_then(|v| v.as_str());
                let include_source = args
                    .get("includeSource")
                    .or_else(|| args.get("include_source"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                let include_callers = args
                    .get("includeCallers")
                    .or_else(|| args.get("include_callers"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                let include_callees = args
                    .get("includeCallees")
                    .or_else(|| args.get("include_callees"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);

                // Use fallback for uri+line, exact match for node_id
                let (target_node, used_fallback) = if let Some(id_str) = node_id {
                    (parse_node_id(id_str), false)
                } else if let (Some(u), Some(l)) = (uri, line) {
                    match self.find_nearest_node_with_fallback(u, l).await {
                        Some((id, fallback)) => (Some(id), fallback),
                        None => (None, false),
                    }
                } else {
                    (None, false)
                };

                if let Some(node_id) = target_node {
                    let result = crate::domain::symbol_info::get_detailed_symbol(
                        &self.backend.graph,
                        &self.backend.query_engine,
                        node_id,
                        include_source,
                        include_callers,
                        include_callees,
                        used_fallback,
                        line,
                    )
                    .await;
                    Ok(serde_json::to_value(&result).unwrap_or_default())
                } else {
                    Ok(serde_json::json!({
                        "error": "Could not find symbol. Provide either nodeId or uri+line."
                    }))
                }
            }

            // ==================== Dependency Analysis Tools ====================
            "codegraph_get_dependency_graph" => {
                let uri = args
                    .get("uri")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing 'uri' parameter")?;
                let depth = args
                    .get("depth")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize)
                    .unwrap_or(3);
                let direction = args
                    .get("direction")
                    .and_then(|v| v.as_str())
                    .unwrap_or("both");
                let _include_external = args
                    .get("includeExternal")
                    .or_else(|| args.get("include_external"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let summary = args
                    .get("summary")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let typed_result = {
                    let url = tower_lsp::lsp_types::Url::parse(uri)
                        .map_err(|_| "Invalid URI".to_string())?;
                    let path = url
                        .to_file_path()
                        .map_err(|_| "Invalid file path".to_string())?;
                    let path_str = path.to_string_lossy().to_string();
                    let graph = self.backend.graph.read().await;
                    crate::domain::dependency_graph::get_dependency_graph(
                        &graph, &path_str, depth, direction,
                    )
                };

                if summary {
                    let node_count = typed_result.nodes.len();
                    let edge_count = typed_result.edges.len();
                    Ok(serde_json::json!({
                        "summary": {
                            "node_count": node_count,
                            "edge_count": edge_count,
                            "depth": depth,
                            "direction": direction,
                        }
                    }))
                } else {
                    Ok(serde_json::to_value(&typed_result).unwrap_or_default())
                }
            }

            "codegraph_get_call_graph" => {
                let uri = args
                    .get("uri")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing 'uri' parameter")?;
                let line = args
                    .get("line")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32)
                    .unwrap_or(0);
                let depth = args
                    .get("depth")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32)
                    .unwrap_or(3);
                let direction = args
                    .get("direction")
                    .and_then(|v| v.as_str())
                    .unwrap_or("both");
                let summary = args
                    .get("summary")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let (start_node, used_fallback) =
                    match self.find_nearest_node_with_fallback(uri, line).await {
                        Some((id, fallback)) => (Some(id), fallback),
                        None => (None, false),
                    };

                let result = match start_node {
                    Some(start) => {
                        let typed = crate::domain::call_graph::get_call_graph(
                            &self.backend.graph,
                            &self.backend.query_engine,
                            start,
                            depth,
                            direction,
                            used_fallback,
                            Some(line),
                        )
                        .await;
                        serde_json::to_value(&typed).unwrap_or_default()
                    }
                    None => serde_json::json!({
                        "nodes": [],
                        "edges": [],
                        "message": "Could not find symbol at location"
                    }),
                };

                if summary {
                    let caller_count = result
                        .get("callers")
                        .and_then(|v| v.as_array())
                        .map(|a| a.len())
                        .unwrap_or(0);
                    let callee_count = result
                        .get("callees")
                        .and_then(|v| v.as_array())
                        .map(|a| a.len())
                        .unwrap_or(0);
                    let symbol = result
                        .get("symbol")
                        .cloned()
                        .unwrap_or(serde_json::json!(null));
                    Ok(serde_json::json!({
                        "symbol": symbol,
                        "summary": {
                            "caller_count": caller_count,
                            "callee_count": callee_count,
                            "depth": depth,
                            "direction": direction,
                        }
                    }))
                } else {
                    Ok(result)
                }
            }

            "codegraph_analyze_impact" => {
                let uri = args
                    .get("uri")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing 'uri' parameter")?;
                let line = args
                    .get("line")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32)
                    .unwrap_or(0);
                let change_type = args
                    .get("changeType")
                    .or_else(|| args.get("change_type"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("modify");
                let summary = args
                    .get("summary")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let (start_node, used_fallback) =
                    match self.find_nearest_node_with_fallback(uri, line).await {
                        Some((id, fallback)) => (Some(id), fallback),
                        None => (None, false),
                    };

                let result = match start_node {
                    Some(start) => {
                        let typed = crate::domain::impact::analyze_impact(
                            &self.backend.graph,
                            &self.backend.query_engine,
                            start,
                            change_type,
                            used_fallback,
                            Some(line),
                            Some(&self.backend.project_slug),
                        )
                        .await;
                        serde_json::to_value(&typed).unwrap_or_default()
                    }
                    None => serde_json::json!({
                        "impacted": [],
                        "risk_level": "unknown",
                        "message": "Could not find symbol at location"
                    }),
                };

                if summary {
                    let total_impacted = result
                        .get("total_impacted")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let direct_impacted = result
                        .get("direct_impacted")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let risk_level = result
                        .get("risk_level")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    let symbol_name = result
                        .get("symbol_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let symbol_id = result
                        .get("symbol_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    Ok(serde_json::json!({
                        "symbol": symbol_name,
                        "symbol_id": symbol_id,
                        "summary": {
                            "total_impacted": total_impacted,
                            "direct_impacted": direct_impacted,
                            "risk_level": risk_level,
                            "change_type": change_type,
                        }
                    }))
                } else {
                    Ok(result)
                }
            }

            "codegraph_analyze_coupling" => {
                let uri = args
                    .get("uri")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing 'uri' parameter")?;
                let depth = args
                    .get("depth")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize)
                    .unwrap_or(2);
                let summary = args
                    .get("summary")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let result = {
                    let url = tower_lsp::lsp_types::Url::parse(uri)
                        .map_err(|_| "Invalid URI".to_string())?;
                    let path = url
                        .to_file_path()
                        .map_err(|_| "Invalid file path".to_string())?;
                    let path_str = path.to_string_lossy().to_string();
                    let graph = self.backend.graph.read().await;
                    let typed =
                        crate::domain::coupling::analyze_coupling(&graph, &path_str, uri, depth);
                    serde_json::to_value(&typed).unwrap_or_default()
                };

                if summary {
                    // Return just metrics, omit the full dependency_graph
                    let metrics = result
                        .get("metrics")
                        .cloned()
                        .unwrap_or(serde_json::json!({}));
                    Ok(serde_json::json!({
                        "uri": uri,
                        "metrics": metrics,
                    }))
                } else {
                    Ok(result)
                }
            }

            // ==================== Analysis Tools ====================
            "codegraph_get_ai_context" => {
                let uri = args
                    .get("uri")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing 'uri' parameter")?;
                let line = args
                    .get("line")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32)
                    .unwrap_or(0);
                let intent = args
                    .get("intent")
                    .or_else(|| args.get("context_type"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("explain");
                let max_tokens = args
                    .get("maxTokens")
                    .or_else(|| args.get("max_tokens"))
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize)
                    .unwrap_or(4000);

                let url =
                    tower_lsp::lsp_types::Url::parse(uri).map_err(|_| "Invalid URI".to_string())?;
                let path = url
                    .to_file_path()
                    .map_err(|_| "Invalid file path".to_string())?;
                let path_str = path.to_string_lossy().to_string();

                let graph = self.backend.graph.read().await;
                let result = crate::domain::ai_context::get_ai_context(
                    &graph, &path_str, line, intent, max_tokens,
                )
                .ok_or_else(|| {
                    format!("No symbols found in '{uri}'. Try indexing the workspace first.")
                })?;

                Ok(serde_json::to_value(result).map_err(|e| e.to_string())?)
            }

            "codegraph_get_edit_context" => {
                let uri = args
                    .get("uri")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing 'uri' parameter")?;
                let line = args
                    .get("line")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32)
                    .ok_or("Missing 'line' parameter")?;
                let max_tokens = args
                    .get("maxTokens")
                    .or_else(|| args.get("max_tokens"))
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize)
                    .unwrap_or(8000);

                let file_path = tower_lsp::lsp_types::Url::parse(uri)
                    .ok()
                    .and_then(|u| u.to_file_path().ok())
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();
                let result = crate::domain::edit_context::get_edit_context(
                    &self.backend.graph,
                    &self.backend.query_engine,
                    &self.backend.memory_manager,
                    &self.backend.workspace_folders,
                    &file_path,
                    uri,
                    line,
                    max_tokens,
                )
                .await;
                Ok(match result {
                    Ok(ctx) => serde_json::to_value(&ctx).unwrap_or_default(),
                    Err(e) => serde_json::to_value(&e).unwrap_or_default(),
                })
            }

            "codegraph_get_curated_context" => {
                let query = args
                    .get("query")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing 'query' parameter")?;
                let uri = args.get("uri").and_then(|v| v.as_str());
                let max_tokens = args
                    .get("maxTokens")
                    .or_else(|| args.get("max_tokens"))
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize)
                    .unwrap_or(8000);
                let max_symbols = args
                    .get("maxSymbols")
                    .or_else(|| args.get("max_symbols"))
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize)
                    .unwrap_or(5);

                let anchor_path: Option<String> = uri.and_then(|u| {
                    tower_lsp::lsp_types::Url::parse(u)
                        .ok()
                        .and_then(|parsed| parsed.to_file_path().ok())
                        .map(|p| p.to_string_lossy().to_string())
                });
                let result = crate::domain::curated_context::get_curated_context(
                    &self.backend.graph,
                    &self.backend.query_engine,
                    &self.backend.memory_manager,
                    query,
                    anchor_path.as_deref(),
                    max_tokens,
                    max_symbols,
                )
                .await;
                Ok(match result {
                    Ok(ctx) => serde_json::to_value(&ctx).unwrap_or_default(),
                    Err(e) => serde_json::to_value(&e).unwrap_or_default(),
                })
            }

            "codegraph_find_related_tests" => {
                let uri = args
                    .get("uri")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing 'uri' parameter")?;
                let line = args
                    .get("line")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32)
                    .unwrap_or(0);
                let limit = args
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize)
                    .unwrap_or(10);

                // Resolve file path
                let url = match tower_lsp::lsp_types::Url::parse(uri) {
                    Ok(u) => u,
                    Err(_) => {
                        return Ok(serde_json::json!({
                            "tests": [],
                            "message": "Invalid URI"
                        }))
                    }
                };
                let file_path = match url.to_file_path() {
                    Ok(p) => p,
                    Err(_) => {
                        return Ok(serde_json::json!({
                            "tests": [],
                            "message": "Invalid file path"
                        }))
                    }
                };
                let path_str = file_path.to_string_lossy().to_string();

                // Resolve target node (with fallback to nearest symbol)
                let (target_node_id, used_fallback, symbol_name) =
                    match self.find_nearest_node_with_fallback(uri, line).await {
                        Some((id, fallback)) => {
                            let name = {
                                let graph = self.backend.graph.read().await;
                                graph
                                    .get_node(id)
                                    .ok()
                                    .map(|n| node_props::name(n).to_string())
                                    .unwrap_or_default()
                            };
                            (Some(id), fallback, name)
                        }
                        None => (None, false, String::new()),
                    };

                let params = crate::domain::related_tests::FindRelatedTestsParams {
                    path: path_str.clone(),
                    target_node_id,
                    limit,
                };

                let graph = self.backend.graph.read().await;
                let result = crate::domain::related_tests::find_related_tests(
                    &graph,
                    &self.backend.query_engine,
                    params,
                )
                .await;

                let tests: Vec<_> = result
                    .tests
                    .iter()
                    .map(|t| {
                        serde_json::json!({
                            "name": t.name,
                            "id": t.node_id.to_string(),
                            "relationship": t.relationship,
                        })
                    })
                    .collect();

                let mut response = if let Some(target_id) = target_node_id {
                    serde_json::json!({
                        "target_id": target_id.to_string(),
                        "symbol_name": symbol_name,
                        "tests": tests,
                        "total": tests.len(),
                    })
                } else {
                    serde_json::json!({
                        "file": path_str,
                        "tests": tests,
                        "total": tests.len(),
                    })
                };

                if used_fallback {
                    if let Some(obj) = response.as_object_mut() {
                        obj.insert("used_fallback".to_string(), serde_json::json!(true));
                        obj.insert(
                            "fallback_message".to_string(),
                            serde_json::json!(format!(
                                "No symbol at line {}. Using nearest symbol '{}' instead.",
                                line, symbol_name
                            )),
                        );
                    }
                }

                Ok(serde_json::to_value(response).map_err(|e| e.to_string())?)
            }

            "codegraph_analyze_complexity" => {
                let uri = args
                    .get("uri")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing 'uri' parameter")?;
                let line = args.get("line").and_then(|v| v.as_u64()).map(|v| v as u32);
                let threshold = args
                    .get("threshold")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32)
                    .unwrap_or(10);
                let summary_only = args
                    .get("summary")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let url =
                    tower_lsp::lsp_types::Url::parse(uri).map_err(|_| "Invalid URI".to_string())?;
                let path = url
                    .to_file_path()
                    .map_err(|_| "Invalid file path".to_string())?;
                let graph = self.backend.graph.read().await;
                let path_str = path.to_string_lossy().to_string();
                let file_nodes = graph
                    .query()
                    .property("path", path_str)
                    .execute()
                    .unwrap_or_default();
                let result = crate::handlers::metrics::analyze_file_complexity(
                    &graph,
                    &file_nodes,
                    line,
                    threshold,
                );

                let functions: Vec<serde_json::Value> = result
                    .functions
                    .iter()
                    .map(|f| {
                        serde_json::json!({
                            "name": f.name,
                            "complexity": f.complexity,
                            "grade": f.grade.to_string(),
                            "node_id": f.node_id.to_string(),
                            "line_start": f.line_start,
                            "line_end": f.line_end,
                            "details": {
                                "complexity_branches": f.details.complexity_branches,
                                "complexity_loops": f.details.complexity_loops,
                                "complexity_logical_ops": f.details.complexity_logical_ops,
                                "complexity_nesting": f.details.complexity_nesting,
                                "complexity_exceptions": f.details.complexity_exceptions,
                                "complexity_early_returns": f.details.complexity_early_returns,
                                "lines_of_code": f.details.lines_of_code,
                            }
                        })
                    })
                    .collect();

                let summary = serde_json::json!({
                    "total_functions": result.functions.len(),
                    "average_complexity": result.average_complexity,
                    "max_complexity": result.max_complexity,
                    "above_threshold": result.functions_above_threshold,
                    "threshold": result.threshold,
                    "overall_grade": result.overall_grade.to_string(),
                });

                if summary_only {
                    Ok(serde_json::json!({ "summary": summary }))
                } else if functions.is_empty() {
                    Ok(serde_json::json!({
                        "functions": [],
                        "summary": summary,
                        "recommendations": [],
                        "note": "No functions found in this file. This may indicate: (1) the language parser doesn't extract function-level details for this file type, (2) the file doesn't contain any functions, or (3) the workspace needs to be re-indexed."
                    }))
                } else {
                    Ok(serde_json::json!({
                        "functions": functions,
                        "summary": summary,
                        "recommendations": result.recommendations,
                    }))
                }
            }

            "codegraph_find_unused_code" => {
                let uri = args.get("uri").and_then(|v| v.as_str());
                let scope = args
                    .get("scope")
                    .and_then(|v| v.as_str())
                    .unwrap_or("file")
                    .to_string();
                let include_tests = args
                    .get("includeTests")
                    .or_else(|| args.get("include_tests"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let confidence = args
                    .get("confidence")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.7);

                // Resolve URI to file path
                let path = if let Some(uri_str) = uri {
                    match tower_lsp::lsp_types::Url::parse(uri_str)
                        .ok()
                        .and_then(|u| u.to_file_path().ok())
                    {
                        Some(p) => Some(p.to_string_lossy().to_string()),
                        None => return Ok(serde_json::json!({"error": "Invalid URI"})),
                    }
                } else {
                    None
                };

                let params = crate::domain::unused_code::FindUnusedCodeParams {
                    path,
                    scope: scope.clone(),
                    include_tests,
                    confidence,
                };

                let graph = self.backend.graph.read().await;
                let result = crate::domain::unused_code::find_unused_code(
                    &graph,
                    &self.backend.query_engine,
                    params,
                )
                .await;

                let unused: Vec<_> = result
                    .candidates
                    .iter()
                    .map(|c| {
                        serde_json::json!({
                            "name": c.name,
                            "node_id": c.node_id.to_string(),
                            "type": format!("{:?}", c.node_type),
                            "confidence": c.confidence,
                            "is_public": c.is_public,
                        })
                    })
                    .collect();

                Ok(serde_json::json!({
                    "unused_items": unused,
                    "summary": {
                        "total_checked": result.total_checked,
                        "unused_count": result.candidates.len(),
                        "scope": result.scope,
                        "min_confidence": result.min_confidence,
                    }
                }))
            }

            // ==================== Memory Tools ====================
            "codegraph_memory_search" => {
                let query = args
                    .get("query")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing 'query' parameter")?;
                let limit = args
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize)
                    .unwrap_or(10);
                let current_only = args
                    .get("currentOnly")
                    .or_else(|| args.get("current_only"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                let kinds = Self::parse_kinds_filter(&args);
                let tags = Self::parse_tags_filter(&args);

                let config = crate::memory::SearchConfig {
                    limit,
                    current_only,
                    kinds,
                    tags,
                    ..Default::default()
                };

                let results = self
                    .backend
                    .memory_manager
                    .search(query, &config, &[])
                    .await
                    .map_err(|e| format!("Memory search failed: {:?}", e))?;

                // Deduplicate by title and commit hash (git-mined commits create duplicates)
                let mut seen_titles = std::collections::HashSet::new();
                let mut seen_commits = std::collections::HashSet::new();
                let results_json: Vec<serde_json::Value> = results
                    .iter()
                    .filter(|r| {
                        // Skip if commit hash already seen
                        if let crate::memory::MemorySource::GitHistory { ref commit_hash } =
                            r.memory.source
                        {
                            if !seen_commits.insert(commit_hash.clone()) {
                                return false;
                            }
                        }
                        seen_titles.insert(r.memory.title.clone())
                    })
                    .map(|r| {
                        serde_json::json!({
                            "id": r.memory.id,
                            "title": r.memory.title,
                            "content": r.memory.content,
                            "kind": r.memory.kind.discriminant_name(),
                            "score": r.score,
                            "created_at": r.memory.temporal.created_at.to_rfc3339(),
                            "tags": r.memory.tags,
                        })
                    })
                    .collect();

                Ok(serde_json::json!({
                    "results": results_json,
                    "total": results_json.len()
                }))
            }

            "codegraph_memory_stats" => {
                let result = self
                    .backend
                    .memory_manager
                    .stats()
                    .await
                    .map_err(|e| format!("Failed to get memory stats: {:?}", e))?;

                Ok(result)
            }

            "codegraph_memory_store" => {
                let kind = args
                    .get("kind")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing 'kind' parameter")?;
                let title = args
                    .get("title")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing 'title' parameter")?;
                let content = args
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing 'content' parameter")?;
                let tags = args
                    .get("tags")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();

                let memory = self.build_memory_node(kind, title, content, &tags, &args)?;

                let id = self
                    .backend
                    .memory_manager
                    .put(memory)
                    .await
                    .map_err(|e| format!("Failed to store memory: {:?}", e))?;

                Ok(serde_json::json!({
                    "id": id,
                    "status": "stored"
                }))
            }

            "codegraph_memory_get" => {
                let id = args
                    .get("id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing 'id' parameter")?;

                let result = self
                    .backend
                    .memory_manager
                    .get(id)
                    .await
                    .map_err(|e| format!("Failed to get memory: {:?}", e))?;

                match result {
                    Some(memory) => Ok(serde_json::json!({
                        "id": memory.id,
                        "title": memory.title,
                        "content": memory.content,
                        "kind": memory.kind.discriminant_name(),
                        "tags": memory.tags,
                        "created_at": memory.temporal.created_at.to_rfc3339(),
                        "invalidated": memory.temporal.invalid_at.is_some(),
                    })),
                    None => Ok(serde_json::json!({
                        "error": "Memory not found"
                    })),
                }
            }

            "codegraph_memory_context" => {
                let uri = args
                    .get("uri")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing 'uri' parameter")?;
                let limit = args
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize)
                    .unwrap_or(5);

                // Find code nodes at the given location and search for related memories
                let url =
                    tower_lsp::lsp_types::Url::parse(uri).map_err(|_| "Invalid URI".to_string())?;
                let path = url
                    .to_file_path()
                    .map_err(|_| "Invalid file path".to_string())?;
                let path_str = path.to_string_lossy().to_string();

                // Search for memories related to this file
                let current_only = args
                    .get("currentOnly")
                    .or_else(|| args.get("current_only"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                let kinds = Self::parse_kinds_filter(&args);
                let tags = Self::parse_tags_filter(&args);
                let config = crate::memory::SearchConfig {
                    limit,
                    current_only,
                    kinds,
                    tags,
                    ..Default::default()
                };

                let results = self
                    .backend
                    .memory_manager
                    .search(&path_str, &config, &[])
                    .await
                    .map_err(|e| format!("Memory search failed: {:?}", e))?;

                // Deduplicate by title and commit hash (git-mined commits create duplicates)
                let mut seen_titles = std::collections::HashSet::new();
                let mut seen_commits = std::collections::HashSet::new();
                let results_json: Vec<serde_json::Value> = results
                    .iter()
                    .filter(|r| {
                        if let crate::memory::MemorySource::GitHistory { ref commit_hash } =
                            r.memory.source
                        {
                            if !seen_commits.insert(commit_hash.clone()) {
                                return false;
                            }
                        }
                        seen_titles.insert(r.memory.title.clone())
                    })
                    .map(|r| {
                        serde_json::json!({
                            "id": r.memory.id,
                            "title": r.memory.title,
                            "content": r.memory.content,
                            "kind": r.memory.kind.discriminant_name(),
                            "score": r.score,
                            "tags": r.memory.tags,
                        })
                    })
                    .collect();

                Ok(serde_json::json!({
                    "uri": uri,
                    "memories": results_json,
                    "total": results_json.len()
                }))
            }

            "codegraph_memory_invalidate" => {
                let id = args
                    .get("id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing 'id' parameter")?;

                // Try to invalidate — idempotent: re-invalidating an already-invalidated
                // memory succeeds silently (returns "already_invalidated" status).
                match self
                    .backend
                    .memory_manager
                    .invalidate(id, "Invalidated via MCP")
                    .await
                {
                    Ok(()) => Ok(serde_json::json!({
                        "id": id,
                        "status": "invalidated"
                    })),
                    Err(e) => {
                        let err_str = format!("{:?}", e);
                        // If the memory doesn't exist in the primary index, check if it's
                        // already invalidated (visible via get_all_memories with currentOnly=false)
                        if err_str.contains("not found") || err_str.contains("Not found") {
                            // Check if it exists as an invalidated memory
                            let all_memories = self
                                .backend
                                .memory_manager
                                .get_all_memories(false)
                                .await
                                .unwrap_or_default();
                            let is_already_invalidated =
                                all_memories.iter().any(|m| m.id.to_string() == id);
                            if is_already_invalidated {
                                Ok(serde_json::json!({
                                    "id": id,
                                    "status": "already_invalidated"
                                }))
                            } else {
                                Err(format!("Memory not found: {}", id))
                            }
                        } else {
                            Err(format!("Failed to invalidate memory: {}", err_str))
                        }
                    }
                }
            }

            "codegraph_memory_list" => {
                let current_only = args
                    .get("currentOnly")
                    .or_else(|| args.get("current_only"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                let limit = args
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize)
                    .unwrap_or(50);
                let offset = args
                    .get("offset")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize)
                    .unwrap_or(0);
                let kinds = Self::parse_kinds_filter(&args);
                let tags = Self::parse_tags_filter(&args);

                let all_memories = self
                    .backend
                    .memory_manager
                    .get_all_memories(current_only)
                    .await
                    .map_err(|e| format!("Failed to list memories: {:?}", e))?;

                // Apply kinds/tags filters and deduplicate by title + commit hash
                let mut seen_titles = std::collections::HashSet::new();
                let mut seen_commits = std::collections::HashSet::new();
                let filtered: Vec<&crate::memory::MemoryNode> = all_memories
                    .iter()
                    .filter(|m| {
                        if !kinds.is_empty()
                            && !kinds.iter().any(|k| Self::kind_matches_filter(k, &m.kind))
                        {
                            return false;
                        }
                        if !tags.is_empty() && !tags.iter().any(|t| m.tags.contains(t)) {
                            return false;
                        }
                        // Deduplicate by commit hash (git-mined commits create duplicates)
                        if let crate::memory::MemorySource::GitHistory { ref commit_hash } =
                            m.source
                        {
                            if !seen_commits.insert(commit_hash.clone()) {
                                return false;
                            }
                        }
                        seen_titles.insert(m.title.clone())
                    })
                    .collect();

                let total = filtered.len();
                let memories_json: Vec<serde_json::Value> = filtered
                    .into_iter()
                    .skip(offset)
                    .take(limit)
                    .map(|m| {
                        serde_json::json!({
                            "id": m.id,
                            "title": m.title,
                            "kind": m.kind.discriminant_name(),
                            "tags": m.tags,
                            "created_at": m.temporal.created_at.to_rfc3339(),
                            "invalidated": m.temporal.invalid_at.is_some(),
                        })
                    })
                    .collect();

                Ok(serde_json::json!({
                    "memories": memories_json,
                    "total": total,
                    "offset": offset,
                    "limit": limit,
                }))
            }

            // ==================== Git Mining Tools ====================
            "codegraph_mine_git_history" => {
                let max_commits = args
                    .get("maxCommits")
                    .or_else(|| args.get("max_commits"))
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize)
                    .unwrap_or(500);
                let min_confidence = args
                    .get("minConfidence")
                    .or_else(|| args.get("min_confidence"))
                    .and_then(|v| v.as_f64())
                    .map(|v| v as f32)
                    .unwrap_or(0.7);

                let repo_path = self
                    .backend
                    .workspace_folders
                    .first()
                    .ok_or("No workspace folder available for git mining")?;

                let miner = GitMiner::new(repo_path).map_err(|e| e.to_string())?;
                let config = MiningConfig {
                    max_commits,
                    min_confidence,
                    ..MiningConfig::default()
                };

                match miner
                    .mine_repository(&self.backend.memory_manager, &self.backend.graph, &config)
                    .await
                {
                    Ok(result) => Ok(serde_json::json!({
                        "status": "success",
                        "commits_processed": result.commits_processed,
                        "memories_created": result.memories_created,
                        "commits_skipped": result.commits_skipped,
                        "memory_ids": result.memory_ids,
                        "warnings": result.warnings
                    })),
                    Err(e) => Ok(serde_json::json!({
                        "status": "error",
                        "message": e.to_string()
                    })),
                }
            }

            "codegraph_mine_git_history_for_file" => {
                let uri = args
                    .get("uri")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing 'uri' parameter")?;
                let max_commits = args
                    .get("maxCommits")
                    .or_else(|| args.get("max_commits"))
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize)
                    .unwrap_or(100);
                let min_confidence = args
                    .get("minConfidence")
                    .or_else(|| args.get("min_confidence"))
                    .and_then(|v| v.as_f64())
                    .map(|v| v as f32)
                    .unwrap_or(0.7);

                let file_path = match tower_lsp::lsp_types::Url::parse(uri) {
                    Ok(url) => match url.to_file_path() {
                        Ok(p) => p,
                        Err(_) => {
                            return Ok(serde_json::json!({
                                "status": "error",
                                "message": "Invalid file URI"
                            }))
                        }
                    },
                    Err(_) => std::path::PathBuf::from(uri),
                };

                let repo_path = self
                    .backend
                    .workspace_folders
                    .first()
                    .ok_or("No workspace folder available for git mining")?;

                let miner = GitMiner::new(repo_path).map_err(|e| e.to_string())?;
                let config = MiningConfig {
                    max_commits,
                    min_confidence,
                    ..MiningConfig::default()
                };

                match miner
                    .mine_file(
                        &file_path,
                        &self.backend.memory_manager,
                        &self.backend.graph,
                        &config,
                    )
                    .await
                {
                    Ok(result) => Ok(serde_json::json!({
                        "status": "success",
                        "uri": uri,
                        "commits_processed": result.commits_processed,
                        "memories_created": result.memories_created,
                        "commits_skipped": result.commits_skipped,
                        "memory_ids": result.memory_ids,
                        "warnings": result.warnings
                    })),
                    Err(e) => Ok(serde_json::json!({
                        "status": "error",
                        "uri": uri,
                        "message": e.to_string()
                    })),
                }
            }

            "codegraph_search_git_history" => {
                let query = args
                    .get("query")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing 'query' parameter")?;
                let since = args.get("since").and_then(|v| v.as_str());
                let max_results = args
                    .get("maxResults")
                    .or_else(|| args.get("max_results"))
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize)
                    .unwrap_or(10);

                let result = self.search_git_history(query, since, max_results).await;
                Ok(serde_json::to_value(result).map_err(|e| e.to_string())?)
            }

            // ==================== Cross-Project Tools ====================
            "codegraph_cross_project_search" => {
                let query = args
                    .get("query")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing 'query' parameter")?;
                let limit = args
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize)
                    .unwrap_or(20);
                let symbol_type = args
                    .get("symbolType")
                    .or_else(|| args.get("symbol_type"))
                    .and_then(|v| v.as_str())
                    .and_then(|s| if s == "any" { None } else { Some(s) });

                // Run cross-project search in blocking task (opens RocksDB)
                let backend = self.backend.clone();
                let query_owned = query.to_string();
                let symbol_type_owned = symbol_type.map(|s| s.to_string());

                let result = tokio::task::spawn_blocking(move || {
                    backend.cross_project_search(&query_owned, symbol_type_owned.as_deref(), limit)
                })
                .await
                .map_err(|e| format!("Task failed: {e}"))??;

                Ok(result)
            }

            // ==================== Admin Tools ====================
            "codegraph_reindex_workspace" => {
                tracing::info!("Reindexing workspace...");

                // Clear the graph
                {
                    let mut graph = self.backend.graph.write().await;
                    *graph = codegraph::CodeGraph::in_memory()
                        .map_err(|e| format!("Failed to create new graph: {}", e))?;
                }

                // Reindex the workspace
                let indexed = self.backend.index_workspace().await;
                tracing::info!("Reindexed {} files", indexed);

                // Rebuild AI query engine indexes
                self.backend.query_engine.build_indexes().await;

                Ok(serde_json::json!({
                    "status": "success",
                    "message": format!("Reindexed {} files", indexed),
                    "files_indexed": indexed
                }))
            }

            // ==================== Unknown Tool ====================
            _ => Err(format!("Unknown tool: {}", name)),
        }
    }

    /// Find a node at location with broader fallback, returning whether fallback was used.
    ///
    /// Strategy:
    /// 1. First try exact match (line within symbol's range)
    /// 2. If no exact match, find the closest symbol in the file (no distance limit)
    ///
    /// Returns (node_id, used_fallback) where used_fallback is true if not an exact match.
    async fn find_nearest_node_with_fallback(
        &self,
        uri: &str,
        line: u32,
    ) -> Option<(codegraph::NodeId, bool)> {
        let url = tower_lsp::lsp_types::Url::parse(uri).ok()?;
        let path = url.to_file_path().ok()?;
        let path_str = path.to_string_lossy().to_string();
        let graph = self.backend.graph.read().await;
        crate::domain::node_resolution::find_nearest_node(&graph, &path_str, line)
    }

    /// Search git history using semantic (memory embeddings) + keyword (git log --grep) matching.
    async fn search_git_history(
        &self,
        query: &str,
        since: Option<&str>,
        max_results: usize,
    ) -> serde_json::Value {
        let start_time = std::time::Instant::now();
        let mut results = Vec::new();
        let mut seen_hashes = std::collections::HashSet::new();

        // Strategy 1: Semantic search via memory embeddings (git-mined memories)
        let config = crate::memory::SearchConfig {
            limit: max_results,
            current_only: false, // include invalidated — historical context matters
            ..Default::default()
        };
        // Minimum similarity threshold — filter out low-relevance semantic results
        const MIN_SIMILARITY: f32 = 0.5;

        if let Ok(mem_results) = self
            .backend
            .memory_manager
            .search(query, &config, &[])
            .await
        {
            for r in &mem_results {
                // Skip low-confidence results
                if r.score < MIN_SIMILARITY {
                    continue;
                }
                if let crate::memory::MemorySource::GitHistory { ref commit_hash } = r.memory.source
                {
                    if seen_hashes.insert(commit_hash.clone()) {
                        results.push(serde_json::json!({
                            "hash": &commit_hash[..8.min(commit_hash.len())],
                            "fullHash": commit_hash,
                            "subject": r.memory.title.trim_start_matches("[Git] "),
                            "content": r.memory.content,
                            "kind": r.memory.kind.discriminant_name(),
                            "score": r.score,
                            "source": "semantic",
                        }));
                    }
                }
            }
        }

        // Strategy 2: Keyword search via git log --grep
        if results.len() < max_results {
            let workspace = self.backend.workspace_folders.first().cloned();
            let query_owned = query.to_string();
            let since_owned = since.map(|s| s.to_string());
            let remaining = max_results.saturating_sub(results.len());

            if let Some(ws) = workspace {
                let git_results = tokio::task::spawn_blocking(move || {
                    let executor = GitExecutor::new(&ws).ok()?;
                    let mut cmd = std::process::Command::new("git");
                    cmd.current_dir(&ws);
                    cmd.args([
                        "log",
                        "--format=%H%x00%s%x00%an%x00%ai",
                        &format!("--grep={}", query_owned),
                        "-i", // case-insensitive
                        &format!("-n{}", remaining * 2),
                    ]);
                    if let Some(ref since_str) = since_owned {
                        cmd.arg(format!("--since={}", since_str));
                    }
                    cmd.arg("--");
                    let output = cmd.output().ok()?;
                    if !output.status.success() {
                        return None;
                    }
                    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

                    // Also get affected files for each commit
                    let commits: Vec<(String, String, String, String, Vec<String>)> = stdout
                        .lines()
                        .filter(|l| !l.is_empty())
                        .take(remaining)
                        .filter_map(|line| {
                            let parts: Vec<&str> = line.split('\0').collect();
                            if parts.len() >= 4 {
                                let files = executor
                                    .show_files(parts[0])
                                    .unwrap_or_default()
                                    .into_iter()
                                    .take(10)
                                    .collect();
                                Some((
                                    parts[0].to_string(),
                                    parts[1].to_string(),
                                    parts[2].to_string(),
                                    parts[3].to_string(),
                                    files,
                                ))
                            } else {
                                None
                            }
                        })
                        .collect();
                    Some(commits)
                })
                .await
                .ok()
                .flatten()
                .unwrap_or_default();

                for (hash, subject, author, date, files) in git_results {
                    if seen_hashes.insert(hash.clone()) {
                        results.push(serde_json::json!({
                            "hash": &hash[..8.min(hash.len())],
                            "fullHash": hash,
                            "subject": subject,
                            "author": author,
                            "date": date,
                            "files": files,
                            "source": "keyword",
                        }));
                    }
                }
            }
        }

        // Also add --since results from git log without --grep if we need more
        // (covers time-scoped browsing like "what changed last week?")
        if let Some(since_str) = since.filter(|_| results.len() < max_results) {
            let workspace = self.backend.workspace_folders.first().cloned();
            let since_owned = since_str.to_string();
            let remaining = max_results.saturating_sub(results.len());
            let seen = seen_hashes.clone();

            if let Some(ws) = workspace {
                let time_results = tokio::task::spawn_blocking(move || {
                    let mut cmd = std::process::Command::new("git");
                    cmd.current_dir(&ws);
                    cmd.args([
                        "log",
                        "--format=%H%x00%s%x00%an%x00%ai",
                        &format!("--since={}", since_owned),
                        &format!("-n{}", remaining * 2),
                        "--",
                    ]);
                    let output = cmd.output().ok()?;
                    if !output.status.success() {
                        return None;
                    }
                    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                    let commits: Vec<(String, String, String, String)> = stdout
                        .lines()
                        .filter(|l| !l.is_empty())
                        .filter_map(|line| {
                            let parts: Vec<&str> = line.split('\0').collect();
                            if parts.len() >= 4 {
                                let hash = parts[0].to_string();
                                if seen.contains(&hash) {
                                    return None;
                                }
                                Some((
                                    hash,
                                    parts[1].to_string(),
                                    parts[2].to_string(),
                                    parts[3].to_string(),
                                ))
                            } else {
                                None
                            }
                        })
                        .take(remaining)
                        .collect();
                    Some(commits)
                })
                .await
                .ok()
                .flatten()
                .unwrap_or_default();

                for (hash, subject, author, date) in time_results {
                    if seen_hashes.insert(hash.clone()) {
                        results.push(serde_json::json!({
                            "hash": &hash[..8.min(hash.len())],
                            "fullHash": hash,
                            "subject": subject,
                            "author": author,
                            "date": date,
                            "source": "time_range",
                        }));
                    }
                }
            }
        }

        let query_time = start_time.elapsed().as_millis() as u64;

        serde_json::json!({
            "query": query,
            "since": since,
            "results": results,
            "metadata": {
                "total": results.len(),
                "queryTime": query_time,
                "semanticMatches": results.iter().filter(|r| r.get("source").and_then(|s| s.as_str()) == Some("semantic")).count(),
                "keywordMatches": results.iter().filter(|r| r.get("source").and_then(|s| s.as_str()) == Some("keyword")).count(),
            }
        })
    }

    /// Build a memory node from parameters
    fn build_memory_node(
        &self,
        kind: &str,
        title: &str,
        content: &str,
        tags: &[String],
        args: &Value,
    ) -> Result<crate::memory::MemoryNode, String> {
        let mut builder = crate::memory::MemoryNodeBuilder::new()
            .title(title)
            .content(content);

        for tag in tags {
            builder = builder.tag(tag);
        }

        // Set kind-specific fields
        builder = match kind {
            "debug_context" => {
                let problem = args
                    .get("problem")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown problem");
                let solution = args
                    .get("solution")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown solution");
                builder.debug_context(problem, solution)
            }
            "architectural_decision" => {
                let decision = args
                    .get("decision")
                    .and_then(|v| v.as_str())
                    .unwrap_or(title);
                let rationale = args
                    .get("rationale")
                    .and_then(|v| v.as_str())
                    .unwrap_or(content);
                builder.architectural_decision(decision, rationale)
            }
            "known_issue" => {
                let description = args
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or(content);
                let severity = args
                    .get("severity")
                    .and_then(|v| v.as_str())
                    .unwrap_or("medium");
                let severity_enum = match severity {
                    "critical" => crate::memory::IssueSeverity::Critical,
                    "high" => crate::memory::IssueSeverity::High,
                    "low" => crate::memory::IssueSeverity::Low,
                    _ => crate::memory::IssueSeverity::Medium,
                };
                builder.known_issue(description, severity_enum)
            }
            "convention" => {
                let name = args.get("name").and_then(|v| v.as_str()).unwrap_or(title);
                let description = args
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or(content);
                builder.convention(name, description)
            }
            "project_context" => {
                let topic = args.get("topic").and_then(|v| v.as_str()).unwrap_or(title);
                let description = args
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or(content);
                builder.project_context(topic, description)
            }
            _ => {
                return Err(format!(
                    "Unknown memory kind: {}. Use: debug_context, architectural_decision, known_issue, convention, project_context",
                    kind
                ));
            }
        };

        builder
            .build()
            .map_err(|e| format!("Failed to build memory: {:?}", e))
    }

    /// Parse a string into a SymbolType
    fn parse_symbol_type(s: &str) -> Option<crate::ai_query::SymbolType> {
        match s.to_lowercase().as_str() {
            "function" | "method" => Some(crate::ai_query::SymbolType::Function),
            "class" | "struct" => Some(crate::ai_query::SymbolType::Class),
            "variable" | "constant" => Some(crate::ai_query::SymbolType::Variable),
            "module" | "namespace" => Some(crate::ai_query::SymbolType::Module),
            "interface" | "trait" => Some(crate::ai_query::SymbolType::Interface),
            "type" | "enum" => Some(crate::ai_query::SymbolType::Type),
            _ => None,
        }
    }

    /// Parse `kinds` filter from MCP args into MemoryKindFilter vec
    fn parse_kinds_filter(args: &serde_json::Value) -> Vec<crate::memory::MemoryKindFilter> {
        args.get("kinds")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .filter_map(Self::parse_kind_str)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Parse `tags` filter from MCP args
    fn parse_tags_filter(args: &serde_json::Value) -> Vec<String> {
        args.get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Parse a kind string into a MemoryKindFilter
    fn parse_kind_str(s: &str) -> Option<crate::memory::MemoryKindFilter> {
        match s {
            "debug_context" | "DebugContext" => Some(crate::memory::MemoryKindFilter::DebugContext),
            "architectural_decision" | "ArchitecturalDecision" => {
                Some(crate::memory::MemoryKindFilter::ArchitecturalDecision)
            }
            "known_issue" | "KnownIssue" => Some(crate::memory::MemoryKindFilter::KnownIssue),
            "convention" | "Convention" => Some(crate::memory::MemoryKindFilter::Convention),
            "project_context" | "ProjectContext" => {
                Some(crate::memory::MemoryKindFilter::ProjectContext)
            }
            _ => None,
        }
    }

    /// Check if a MemoryKindFilter matches a MemoryKind
    fn kind_matches_filter(
        filter: &crate::memory::MemoryKindFilter,
        kind: &crate::memory::MemoryKind,
    ) -> bool {
        matches!(
            (filter, kind),
            (
                crate::memory::MemoryKindFilter::ArchitecturalDecision,
                crate::memory::MemoryKind::ArchitecturalDecision { .. }
            ) | (
                crate::memory::MemoryKindFilter::DebugContext,
                crate::memory::MemoryKind::DebugContext { .. }
            ) | (
                crate::memory::MemoryKindFilter::KnownIssue,
                crate::memory::MemoryKind::KnownIssue { .. }
            ) | (
                crate::memory::MemoryKindFilter::Convention,
                crate::memory::MemoryKind::Convention { .. }
            ) | (
                crate::memory::MemoryKindFilter::ProjectContext,
                crate::memory::MemoryKind::ProjectContext { .. }
            )
        )
    }
}

/// Parse a string into a NodeId
fn parse_node_id(s: &str) -> Option<codegraph::NodeId> {
    // NodeId is u64 in codegraph
    s.parse::<codegraph::NodeId>().ok()
}
