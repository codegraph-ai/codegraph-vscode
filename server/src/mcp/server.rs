//! MCP Server Implementation
//!
//! Handles MCP protocol requests and routes them to CodeGraph functionality.

use super::protocol::*;
use super::resources::get_all_resources;
use super::tools::get_all_tools;
use super::transport::AsyncStdioTransport;
use crate::ai_query::QueryEngine;
use crate::memory::MemoryManager;
use crate::parser_registry::ParserRegistry;
use codegraph::CodeGraph;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

const PROTOCOL_VERSION: &str = "2024-11-05";
const SERVER_NAME: &str = "codegraph";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// MCP Backend - wraps CodeGraph components for MCP access
pub struct McpBackend {
    pub graph: Arc<RwLock<CodeGraph>>,
    pub parsers: Arc<ParserRegistry>,
    pub query_engine: Arc<QueryEngine>,
    pub memory_manager: Arc<MemoryManager>,
    pub workspace_folders: Vec<PathBuf>,
}

impl McpBackend {
    /// Create a new MCP backend for the given workspace
    pub fn new(workspace: PathBuf) -> Self {
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
        }
    }

    /// Index the workspace
    pub async fn index_workspace(&self) -> usize {
        let mut total = 0;
        for folder in &self.workspace_folders {
            total += self.index_directory(folder).await;

            // Initialize memory manager with workspace path
            // Note: Uses .codegraph/memory which may conflict with LSP if both run simultaneously
            if let Err(e) = self.memory_manager.initialize(folder).await {
                tracing::warn!("Failed to initialize memory manager: {:?}", e);
            }
        }

        // Build query engine indexes
        self.query_engine.build_indexes().await;

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
                    // EOF or empty line - continue
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
        let _params: InitializeParams = params
            .map(|p| serde_json::from_value(p).unwrap_or_default())
            .unwrap_or_default();

        // Index the workspace
        tracing::info!("Indexing workspace...");
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

    /// Execute a tool by name - delegates to query engine
    async fn execute_tool(&self, name: &str, args: Option<Value>) -> Result<Value, String> {
        let args = args.unwrap_or(Value::Object(serde_json::Map::new()));

        // For now, we'll provide a simplified implementation that uses the query engine
        // directly. A full implementation would create a shim CodeGraphBackend.
        match name {
            // Search tools - these use the query engine directly
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

                let options = crate::ai_query::SearchOptions::new().with_limit(limit);
                let result = self
                    .backend
                    .query_engine
                    .symbol_search(query, &options)
                    .await;

                Ok(serde_json::to_value(result).map_err(|e| e.to_string())?)
            }

            "codegraph_find_entry_points" => {
                let entry_type = args
                    .get("entryType")
                    .or_else(|| args.get("entry_type"))
                    .and_then(|v| v.as_str());

                // Convert string to entry types
                let entry_types = match entry_type {
                    Some("http") | Some("HttpHandler") => {
                        vec![crate::ai_query::EntryType::HttpHandler]
                    }
                    Some("cli") | Some("CliCommand") => {
                        vec![crate::ai_query::EntryType::CliCommand]
                    }
                    Some("public") | Some("PublicApi") => {
                        vec![crate::ai_query::EntryType::PublicApi]
                    }
                    Some("event") | Some("EventHandler") => {
                        vec![crate::ai_query::EntryType::EventHandler]
                    }
                    Some("test") | Some("TestEntry") => vec![crate::ai_query::EntryType::TestEntry],
                    Some("main") | Some("Main") => vec![crate::ai_query::EntryType::Main],
                    _ => vec![
                        crate::ai_query::EntryType::HttpHandler,
                        crate::ai_query::EntryType::CliCommand,
                        crate::ai_query::EntryType::PublicApi,
                        crate::ai_query::EntryType::Main,
                    ],
                };

                let result = self
                    .backend
                    .query_engine
                    .find_entry_points(&entry_types)
                    .await;

                Ok(serde_json::to_value(result).map_err(|e| e.to_string())?)
            }

            // Graph traversal - get_callers
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

                // Try to find starting node
                let start_node = if let Some(id_str) = node_id {
                    parse_node_id(id_str)
                } else if let (Some(u), Some(l)) = (uri, line) {
                    self.find_node_at_location(u, l).await
                } else {
                    None
                };

                if let Some(start) = start_node {
                    let result = self.backend.query_engine.get_callers(start, depth).await;
                    Ok(serde_json::to_value(result).map_err(|e| e.to_string())?)
                } else {
                    Ok(serde_json::json!({
                        "callers": [],
                        "message": "Could not find starting node. Provide either nodeId or uri+line."
                    }))
                }
            }

            // Graph traversal - get_callees
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

                // Try to find starting node
                let start_node = if let Some(id_str) = node_id {
                    parse_node_id(id_str)
                } else if let (Some(u), Some(l)) = (uri, line) {
                    self.find_node_at_location(u, l).await
                } else {
                    None
                };

                if let Some(start) = start_node {
                    let result = self.backend.query_engine.get_callees(start, depth).await;
                    Ok(serde_json::to_value(result).map_err(|e| e.to_string())?)
                } else {
                    Ok(serde_json::json!({
                        "callees": [],
                        "message": "Could not find starting node. Provide either nodeId or uri+line."
                    }))
                }
            }

            // Memory tools
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

                let config = crate::memory::SearchConfig {
                    limit,
                    ..Default::default()
                };

                let results = self
                    .backend
                    .memory_manager
                    .search(query, &config, &[])
                    .await
                    .map_err(|e| format!("Memory search failed: {:?}", e))?;

                // Convert to JSON-serializable format
                let results_json: Vec<serde_json::Value> = results
                    .iter()
                    .map(|r| {
                        serde_json::json!({
                            "id": r.memory.id,
                            "title": r.memory.title,
                            "content": r.memory.content,
                            "kind": format!("{:?}", r.memory.kind),
                            "score": r.score,
                            "created_at": r.memory.temporal.created_at.to_rfc3339(),
                            "tags": r.memory.tags,
                        })
                    })
                    .collect();

                Ok(serde_json::json!({
                    "results": results_json,
                    "total": results.len()
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

            // For other tools, return a helpful message
            _ => {
                // Check if it's a known tool that's not yet implemented
                let known_tools = [
                    "codegraph_get_dependency_graph",
                    "codegraph_get_call_graph",
                    "codegraph_analyze_impact",
                    "codegraph_get_ai_context",
                    "codegraph_find_related_tests",
                    "codegraph_get_symbol_info",
                    "codegraph_analyze_complexity",
                    "codegraph_find_unused_code",
                    "codegraph_analyze_coupling",
                    "codegraph_find_by_imports",
                    "codegraph_traverse_graph",
                    "codegraph_find_by_signature",
                    "codegraph_get_detailed_symbol",
                    "codegraph_memory_store",
                    "codegraph_memory_get",
                    "codegraph_memory_context",
                    "codegraph_memory_invalidate",
                    "codegraph_memory_list",
                    "codegraph_mine_git_history",
                    "codegraph_mine_git_file",
                ];

                if known_tools.contains(&name) {
                    Ok(serde_json::json!({
                        "status": "not_implemented",
                        "tool": name,
                        "message": format!("Tool '{}' is defined but not yet implemented in MCP mode. Use symbol_search, find_entry_points, get_callers, get_callees, or memory_search instead.", name)
                    }))
                } else {
                    Err(format!("Unknown tool: {}", name))
                }
            }
        }
    }

    /// Helper to find a node at a given location
    async fn find_node_at_location(&self, uri: &str, line: u32) -> Option<codegraph::NodeId> {
        let url = tower_lsp::lsp_types::Url::parse(uri).ok()?;
        let path = url.to_file_path().ok()?;
        let path_str = path.to_string_lossy().to_string();

        let graph = self.backend.graph.read().await;
        let nodes = graph.query().property("path", path_str).execute().ok()?;

        // Find closest node to the given line
        for node_id in nodes {
            if let Ok(node) = graph.get_node(node_id) {
                let start_line = node
                    .properties
                    .get_int("line_start")
                    .or_else(|| node.properties.get_int("start_line"))
                    .unwrap_or(0) as u32;
                let end_line = node
                    .properties
                    .get_int("line_end")
                    .or_else(|| node.properties.get_int("end_line"))
                    .unwrap_or(start_line as i64) as u32;

                if line >= start_line && line <= end_line {
                    return Some(node_id);
                }
            }
        }

        None
    }
}

/// Parse a string into a NodeId
fn parse_node_id(s: &str) -> Option<codegraph::NodeId> {
    // NodeId is u64 in codegraph
    s.parse::<codegraph::NodeId>().ok()
}
