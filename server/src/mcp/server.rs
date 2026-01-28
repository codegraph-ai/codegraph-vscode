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

                let result = self
                    .backend
                    .query_engine
                    .find_entry_points(&entry_types)
                    .await;

                Ok(serde_json::to_value(result).map_err(|e| e.to_string())?)
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

                Ok(serde_json::to_value(result).map_err(|e| e.to_string())?)
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
                    .find_by_signature(&pattern)
                    .await;

                Ok(serde_json::to_value(result).map_err(|e| e.to_string())?)
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

                let start_node = if let Some(id_str) = node_id {
                    parse_node_id(id_str)
                } else if let (Some(u), Some(l)) = (uri, line) {
                    self.find_node_at_location(u, l).await
                } else {
                    None
                };

                if let Some(start) = start_node {
                    let direction = match direction_str {
                        "incoming" => crate::ai_query::TraversalDirection::Incoming,
                        "both" => crate::ai_query::TraversalDirection::Both,
                        _ => crate::ai_query::TraversalDirection::Outgoing,
                    };

                    let filter = crate::ai_query::TraversalFilter {
                        symbol_types: vec![],
                        max_nodes: limit,
                    };

                    let result = self
                        .backend
                        .query_engine
                        .traverse_graph(start, direction, max_depth, &filter)
                        .await;
                    Ok(serde_json::to_value(result).map_err(|e| e.to_string())?)
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

                let target_node = if let Some(id_str) = node_id {
                    parse_node_id(id_str)
                } else if let (Some(u), Some(l)) = (uri, line) {
                    self.find_node_at_location(u, l).await
                } else {
                    None
                };

                if let Some(node_id) = target_node {
                    let result = self.backend.query_engine.get_symbol_info(node_id).await;
                    match result {
                        Some(info) => Ok(serde_json::to_value(info).map_err(|e| e.to_string())?),
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

                let target_node = if let Some(id_str) = node_id {
                    parse_node_id(id_str)
                } else if let (Some(u), Some(l)) = (uri, line) {
                    self.find_node_at_location(u, l).await
                } else {
                    None
                };

                if let Some(node_id) = target_node {
                    let mut result = serde_json::Map::new();

                    // Get basic symbol info
                    if let Some(info) = self.backend.query_engine.get_symbol_info(node_id).await {
                        result.insert(
                            "symbol".to_string(),
                            serde_json::to_value(&info).unwrap_or(Value::Null),
                        );
                    }

                    // Get source code if requested
                    if include_source {
                        if let Some(source) = self.get_symbol_source(node_id).await {
                            result.insert("source".to_string(), Value::String(source));
                        }
                    }

                    // Get callers if requested
                    if include_callers {
                        let callers = self.backend.query_engine.get_callers(node_id, 1).await;
                        result.insert(
                            "callers".to_string(),
                            serde_json::to_value(&callers).unwrap_or(Value::Array(vec![])),
                        );
                    }

                    // Get callees if requested
                    if include_callees {
                        let callees = self.backend.query_engine.get_callees(node_id, 1).await;
                        result.insert(
                            "callees".to_string(),
                            serde_json::to_value(&callees).unwrap_or(Value::Array(vec![])),
                        );
                    }

                    Ok(Value::Object(result))
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
                let include_external = args
                    .get("includeExternal")
                    .or_else(|| args.get("include_external"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let result = self
                    .get_dependency_graph(uri, depth, direction, include_external)
                    .await;
                Ok(serde_json::to_value(result).map_err(|e| e.to_string())?)
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

                let result = self.get_call_graph(uri, line, depth, direction).await;
                Ok(serde_json::to_value(result).map_err(|e| e.to_string())?)
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

                let result = self.analyze_impact(uri, line, change_type).await;
                Ok(serde_json::to_value(result).map_err(|e| e.to_string())?)
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

                let result = self.analyze_coupling(uri, depth).await;
                Ok(serde_json::to_value(result).map_err(|e| e.to_string())?)
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

                let result = self.get_ai_context(uri, line, intent, max_tokens).await;
                Ok(serde_json::to_value(result).map_err(|e| e.to_string())?)
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

                let result = self.find_related_tests(uri, line, limit).await;
                Ok(serde_json::to_value(result).map_err(|e| e.to_string())?)
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

                let result = self.analyze_complexity(uri, line, threshold).await;
                Ok(serde_json::to_value(result).map_err(|e| e.to_string())?)
            }

            "codegraph_find_unused_code" => {
                let uri = args.get("uri").and_then(|v| v.as_str());
                let scope = args
                    .get("scope")
                    .and_then(|v| v.as_str())
                    .unwrap_or("file");
                let include_tests = args
                    .get("includeTests")
                    .or_else(|| args.get("include_tests"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let confidence = args
                    .get("confidence")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.7);

                let result = self
                    .find_unused_code(uri, scope, include_tests, confidence)
                    .await;
                Ok(serde_json::to_value(result).map_err(|e| e.to_string())?)
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
                        "kind": format!("{:?}", memory.kind),
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
                let url = tower_lsp::lsp_types::Url::parse(uri)
                    .map_err(|_| "Invalid URI".to_string())?;
                let path = url
                    .to_file_path()
                    .map_err(|_| "Invalid file path".to_string())?;
                let path_str = path.to_string_lossy().to_string();

                // Search for memories related to this file
                let config = crate::memory::SearchConfig {
                    limit,
                    ..Default::default()
                };

                let results = self
                    .backend
                    .memory_manager
                    .search(&path_str, &config, &[])
                    .await
                    .map_err(|e| format!("Memory search failed: {:?}", e))?;

                let results_json: Vec<serde_json::Value> = results
                    .iter()
                    .map(|r| {
                        serde_json::json!({
                            "id": r.memory.id,
                            "title": r.memory.title,
                            "content": r.memory.content,
                            "kind": format!("{:?}", r.memory.kind),
                            "score": r.score,
                            "tags": r.memory.tags,
                        })
                    })
                    .collect();

                Ok(serde_json::json!({
                    "uri": uri,
                    "memories": results_json,
                    "total": results.len()
                }))
            }

            "codegraph_memory_invalidate" => {
                let id = args
                    .get("id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing 'id' parameter")?;

                self.backend
                    .memory_manager
                    .invalidate(id, "Invalidated via MCP")
                    .await
                    .map_err(|e| format!("Failed to invalidate memory: {:?}", e))?;

                Ok(serde_json::json!({
                    "id": id,
                    "status": "invalidated"
                }))
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

                let memories = if current_only {
                    self.backend
                        .memory_manager
                        .get_all_current()
                        .await
                        .map_err(|e| format!("Failed to list memories: {:?}", e))?
                } else {
                    // For now, just return current memories
                    self.backend
                        .memory_manager
                        .get_all_current()
                        .await
                        .map_err(|e| format!("Failed to list memories: {:?}", e))?
                };

                let memories_json: Vec<serde_json::Value> = memories
                    .iter()
                    .take(limit)
                    .map(|m| {
                        serde_json::json!({
                            "id": m.id,
                            "title": m.title,
                            "kind": format!("{:?}", m.kind),
                            "tags": m.tags,
                            "created_at": m.temporal.created_at.to_rfc3339(),
                        })
                    })
                    .collect();

                Ok(serde_json::json!({
                    "memories": memories_json,
                    "total": memories.len()
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
                    .unwrap_or(0.7);

                // Git mining is a complex operation - return status for now
                Ok(serde_json::json!({
                    "status": "not_fully_implemented",
                    "message": "Git history mining requires additional setup. Use the VS Code extension for full git mining capabilities.",
                    "requested_max_commits": max_commits,
                    "requested_min_confidence": min_confidence
                }))
            }

            "codegraph_mine_git_file" => {
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

                Ok(serde_json::json!({
                    "status": "not_fully_implemented",
                    "message": "File-specific git mining requires additional setup. Use the VS Code extension for full git mining capabilities.",
                    "uri": uri,
                    "requested_max_commits": max_commits
                }))
            }

            // ==================== Unknown Tool ====================
            _ => Err(format!("Unknown tool: {}", name)),
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

    /// Get source code for a symbol
    async fn get_symbol_source(&self, node_id: codegraph::NodeId) -> Option<String> {
        let graph = self.backend.graph.read().await;
        let node = graph.get_node(node_id).ok()?;

        let path = node.properties.get_string("path")?;
        let start_line = node
            .properties
            .get_int("line_start")
            .or_else(|| node.properties.get_int("start_line"))? as usize;
        let end_line = node
            .properties
            .get_int("line_end")
            .or_else(|| node.properties.get_int("end_line"))? as usize;

        // Read the file and extract lines
        let content = std::fs::read_to_string(path).ok()?;
        let lines: Vec<&str> = content.lines().collect();

        if start_line > 0 && end_line <= lines.len() {
            Some(lines[start_line - 1..end_line].join("\n"))
        } else {
            None
        }
    }

    /// Get dependency graph for a file
    async fn get_dependency_graph(
        &self,
        uri: &str,
        depth: usize,
        direction: &str,
        _include_external: bool,
    ) -> serde_json::Value {
        use std::collections::{HashSet, VecDeque};

        let url = match tower_lsp::lsp_types::Url::parse(uri) {
            Ok(u) => u,
            Err(_) => {
                return serde_json::json!({
                    "nodes": [],
                    "edges": [],
                    "error": "Invalid URI"
                })
            }
        };

        let path = match url.to_file_path() {
            Ok(p) => p,
            Err(_) => {
                return serde_json::json!({
                    "nodes": [],
                    "edges": [],
                    "error": "Invalid file path"
                })
            }
        };

        let graph = self.backend.graph.read().await;
        let path_str = path.to_string_lossy().to_string();

        // Find nodes in this file
        let file_nodes = match graph.query().property("path", path_str).execute() {
            Ok(nodes) => nodes,
            Err(_) => {
                return serde_json::json!({
                    "nodes": [],
                    "edges": []
                })
            }
        };

        if file_nodes.is_empty() {
            return serde_json::json!({
                "nodes": [],
                "edges": []
            });
        }

        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut nodes = Vec::new();
        let mut edges = Vec::new();

        // Start BFS from file nodes
        for node_id in &file_nodes {
            queue.push_back((*node_id, 0usize));
            visited.insert(*node_id);
        }

        while let Some((node_id, current_depth)) = queue.pop_front() {
            if current_depth > depth {
                continue;
            }

            if let Ok(node) = graph.get_node(node_id) {
                let name = node
                    .properties
                    .get_string("name")
                    .unwrap_or_default();
                let node_type = format!("{:?}", node.node_type);

                nodes.push(serde_json::json!({
                    "id": node_id.to_string(),
                    "name": name,
                    "type": node_type,
                }));

                // Get neighbors based on direction
                let directions: Vec<codegraph::Direction> = match direction {
                    "imports" => vec![codegraph::Direction::Outgoing],
                    "importedBy" => vec![codegraph::Direction::Incoming],
                    _ => vec![codegraph::Direction::Outgoing, codegraph::Direction::Incoming],
                };

                for dir in directions {
                    if let Ok(neighbors) = graph.get_neighbors(node_id, dir) {
                        for neighbor_id in neighbors {
                            let (from, to) = match dir {
                                codegraph::Direction::Outgoing => (node_id, neighbor_id),
                                codegraph::Direction::Incoming => (neighbor_id, node_id),
                                codegraph::Direction::Both => (node_id, neighbor_id),
                            };

                            edges.push(serde_json::json!({
                                "from": from.to_string(),
                                "to": to.to_string(),
                                "type": "depends_on",
                            }));

                            if !visited.contains(&neighbor_id) && current_depth < depth {
                                visited.insert(neighbor_id);
                                queue.push_back((neighbor_id, current_depth + 1));
                            }
                        }
                    }
                }
            }
        }

        serde_json::json!({
            "nodes": nodes,
            "edges": edges
        })
    }

    /// Get call graph for a function
    async fn get_call_graph(
        &self,
        uri: &str,
        line: u32,
        depth: u32,
        direction: &str,
    ) -> serde_json::Value {
        let start_node = self.find_node_at_location(uri, line).await;

        let start = match start_node {
            Some(id) => id,
            None => {
                return serde_json::json!({
                    "nodes": [],
                    "edges": [],
                    "message": "Could not find symbol at location"
                })
            }
        };

        let mut nodes = Vec::new();
        let mut edges = Vec::new();

        // Get callers and/or callees based on direction
        match direction {
            "callers" => {
                let callers = self.backend.query_engine.get_callers(start, depth).await;
                for caller in callers {
                    nodes.push(serde_json::json!({
                        "id": caller.node_id.to_string(),
                        "name": caller.symbol.name,
                        "depth": caller.depth,
                    }));
                    edges.push(serde_json::json!({
                        "from": caller.node_id.to_string(),
                        "to": start.to_string(),
                        "type": "calls",
                    }));
                }
            }
            "callees" => {
                let callees = self.backend.query_engine.get_callees(start, depth).await;
                for callee in callees {
                    nodes.push(serde_json::json!({
                        "id": callee.node_id.to_string(),
                        "name": callee.symbol.name,
                        "depth": callee.depth,
                    }));
                    edges.push(serde_json::json!({
                        "from": start.to_string(),
                        "to": callee.node_id.to_string(),
                        "type": "calls",
                    }));
                }
            }
            _ => {
                // Both directions
                let callers = self.backend.query_engine.get_callers(start, depth).await;
                let callees = self.backend.query_engine.get_callees(start, depth).await;

                for caller in callers {
                    nodes.push(serde_json::json!({
                        "id": caller.node_id.to_string(),
                        "name": caller.symbol.name,
                        "depth": caller.depth,
                        "direction": "caller",
                    }));
                    edges.push(serde_json::json!({
                        "from": caller.node_id.to_string(),
                        "to": start.to_string(),
                        "type": "calls",
                    }));
                }

                for callee in callees {
                    nodes.push(serde_json::json!({
                        "id": callee.node_id.to_string(),
                        "name": callee.symbol.name,
                        "depth": callee.depth,
                        "direction": "callee",
                    }));
                    edges.push(serde_json::json!({
                        "from": start.to_string(),
                        "to": callee.node_id.to_string(),
                        "type": "calls",
                    }));
                }
            }
        }

        serde_json::json!({
            "root": start.to_string(),
            "nodes": nodes,
            "edges": edges
        })
    }

    /// Analyze impact of changes to a symbol
    async fn analyze_impact(&self, uri: &str, line: u32, change_type: &str) -> serde_json::Value {
        let start_node = self.find_node_at_location(uri, line).await;

        let start = match start_node {
            Some(id) => id,
            None => {
                return serde_json::json!({
                    "impacted": [],
                    "risk_level": "unknown",
                    "message": "Could not find symbol at location"
                })
            }
        };

        // Get all callers (things that depend on this)
        let callers = self.backend.query_engine.get_callers(start, 3).await;

        let impacted: Vec<serde_json::Value> = callers
            .iter()
            .map(|c| {
                serde_json::json!({
                    "node_id": c.node_id.to_string(),
                    "name": c.symbol.name,
                    "depth": c.depth,
                    "impact_type": if c.depth == 1 { "direct" } else { "indirect" },
                })
            })
            .collect();

        let risk_level = match (change_type, callers.len()) {
            ("delete", n) if n > 10 => "critical",
            ("delete", n) if n > 0 => "high",
            ("rename", n) if n > 10 => "high",
            ("rename", n) if n > 0 => "medium",
            ("modify", n) if n > 20 => "medium",
            ("modify", _) => "low",
            _ => "low",
        };

        serde_json::json!({
            "symbol_id": start.to_string(),
            "change_type": change_type,
            "impacted": impacted,
            "total_impacted": callers.len(),
            "direct_impacted": callers.iter().filter(|c| c.depth == 1).count(),
            "risk_level": risk_level,
        })
    }

    /// Analyze coupling for a file
    async fn analyze_coupling(&self, uri: &str, depth: usize) -> serde_json::Value {
        let dep_graph = self.get_dependency_graph(uri, depth, "both", false).await;

        let nodes = dep_graph
            .get("nodes")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        let edges = dep_graph
            .get("edges")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);

        // Simple coupling metrics
        let afferent = dep_graph
            .get("edges")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter(|e| e.get("to").is_some()).count())
            .unwrap_or(0);

        let efferent = dep_graph
            .get("edges")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter(|e| e.get("from").is_some()).count())
            .unwrap_or(0);

        let instability = if afferent + efferent > 0 {
            efferent as f64 / (afferent + efferent) as f64
        } else {
            0.0
        };

        serde_json::json!({
            "uri": uri,
            "metrics": {
                "afferent_coupling": afferent,
                "efferent_coupling": efferent,
                "instability": instability,
                "total_dependencies": nodes,
                "total_connections": edges,
            },
            "dependency_graph": dep_graph,
        })
    }

    /// Get AI context for a location
    async fn get_ai_context(
        &self,
        uri: &str,
        line: u32,
        intent: &str,
        max_tokens: usize,
    ) -> serde_json::Value {
        let node_id = self.find_node_at_location(uri, line).await;

        let target = match node_id {
            Some(id) => id,
            None => {
                return serde_json::json!({
                    "error": "Could not find symbol at location",
                    "uri": uri,
                    "line": line
                })
            }
        };

        let graph = self.backend.graph.read().await;
        let node = match graph.get_node(target) {
            Ok(n) => n,
            Err(_) => {
                return serde_json::json!({
                    "error": "Could not load node"
                })
            }
        };

        let name = node
            .properties
            .get_string("name")
            .unwrap_or_default();
        let node_type = format!("{:?}", node.node_type);

        // Get source code
        let source = self.get_symbol_source(target).await;

        // Get related symbols based on intent
        let (callers, callees) = match intent {
            "explain" => {
                let callers = self.backend.query_engine.get_callers(target, 1).await;
                let callees = self.backend.query_engine.get_callees(target, 1).await;
                (callers, callees)
            }
            "modify" | "debug" => {
                let callers = self.backend.query_engine.get_callers(target, 2).await;
                let callees = self.backend.query_engine.get_callees(target, 2).await;
                (callers, callees)
            }
            "test" => {
                let callees = self.backend.query_engine.get_callees(target, 1).await;
                (vec![], callees)
            }
            _ => (vec![], vec![]),
        };

        // Estimate tokens (rough approximation)
        let source_tokens = source.as_ref().map(|s| s.len() / 4).unwrap_or(0);
        let context_tokens = (callers.len() + callees.len()) * 50;
        let total_tokens = source_tokens + context_tokens;

        serde_json::json!({
            "symbol": {
                "id": target.to_string(),
                "name": name,
                "type": node_type,
            },
            "source": source,
            "callers": callers.iter().take(10).map(|c| serde_json::json!({
                "name": c.symbol.name,
                "id": c.node_id.to_string(),
            })).collect::<Vec<_>>(),
            "callees": callees.iter().take(10).map(|c| serde_json::json!({
                "name": c.symbol.name,
                "id": c.node_id.to_string(),
            })).collect::<Vec<_>>(),
            "intent": intent,
            "estimated_tokens": total_tokens.min(max_tokens),
            "truncated": total_tokens > max_tokens,
        })
    }

    /// Find related tests for a symbol
    async fn find_related_tests(&self, uri: &str, line: u32, limit: usize) -> serde_json::Value {
        let node_id = self.find_node_at_location(uri, line).await;

        let target = match node_id {
            Some(id) => id,
            None => {
                return serde_json::json!({
                    "tests": [],
                    "message": "Could not find symbol at location"
                })
            }
        };

        // Find test entry points that call this symbol
        let entry_types = vec![crate::ai_query::EntryType::TestEntry];
        let tests = self
            .backend
            .query_engine
            .find_entry_points(&entry_types)
            .await;

        // Filter tests that might be related (by checking if they call the target)
        let mut related_tests = Vec::new();

        for test in tests.iter().take(limit * 2) {
            let callees = self
                .backend
                .query_engine
                .get_callees(test.node_id, 3)
                .await;
            if callees.iter().any(|c| c.node_id == target) {
                related_tests.push(serde_json::json!({
                    "name": test.symbol.name,
                    "id": test.node_id.to_string(),
                    "relationship": "calls_target",
                }));
                if related_tests.len() >= limit {
                    break;
                }
            }
        }

        serde_json::json!({
            "target_id": target.to_string(),
            "tests": related_tests,
            "total": related_tests.len(),
        })
    }

    /// Analyze complexity for a file
    async fn analyze_complexity(
        &self,
        uri: &str,
        line: Option<u32>,
        threshold: u32,
    ) -> serde_json::Value {
        let url = match tower_lsp::lsp_types::Url::parse(uri) {
            Ok(u) => u,
            Err(_) => {
                return serde_json::json!({
                    "error": "Invalid URI"
                })
            }
        };

        let path = match url.to_file_path() {
            Ok(p) => p,
            Err(_) => {
                return serde_json::json!({
                    "error": "Invalid file path"
                })
            }
        };

        let graph = self.backend.graph.read().await;
        let path_str = path.to_string_lossy().to_string();

        let file_nodes = match graph.query().property("path", path_str).execute() {
            Ok(nodes) => nodes,
            Err(_) => {
                return serde_json::json!({
                    "functions": [],
                    "summary": {}
                })
            }
        };

        let mut functions = Vec::new();
        let mut total_complexity = 0u32;
        let mut max_complexity = 0u32;
        let mut above_threshold = 0u32;

        for node_id in file_nodes {
            if let Ok(node) = graph.get_node(node_id) {
                // Only analyze functions
                if node.node_type != codegraph::NodeType::Function {
                    continue;
                }

                // Check line filter
                if let Some(target_line) = line {
                    let start = node
                        .properties
                        .get_int("line_start")
                        .unwrap_or(0) as u32;
                    let end = node.properties.get_int("line_end").unwrap_or(0) as u32;
                    if target_line < start || target_line > end {
                        continue;
                    }
                }

                let name = node
                    .properties
                    .get_string("name")
                    .unwrap_or_default();

                // Simple complexity estimation based on edges and properties
                let callees = self.backend.query_engine.get_callees(node_id, 1).await;
                let complexity = (callees.len() as u32).max(1);

                total_complexity += complexity;
                max_complexity = max_complexity.max(complexity);
                if complexity > threshold {
                    above_threshold += 1;
                }

                let grade = match complexity {
                    0..=5 => 'A',
                    6..=10 => 'B',
                    11..=20 => 'C',
                    21..=50 => 'D',
                    _ => 'F',
                };

                functions.push(serde_json::json!({
                    "name": name,
                    "complexity": complexity,
                    "grade": grade.to_string(),
                    "node_id": node_id.to_string(),
                }));
            }
        }

        let avg_complexity = if !functions.is_empty() {
            total_complexity as f64 / functions.len() as f64
        } else {
            0.0
        };

        serde_json::json!({
            "functions": functions,
            "summary": {
                "total_functions": functions.len(),
                "average_complexity": avg_complexity,
                "max_complexity": max_complexity,
                "above_threshold": above_threshold,
                "threshold": threshold,
            }
        })
    }

    /// Find unused code
    async fn find_unused_code(
        &self,
        uri: Option<&str>,
        scope: &str,
        _include_tests: bool,
        confidence: f64,
    ) -> serde_json::Value {
        let graph = self.backend.graph.read().await;

        let nodes_to_check: Vec<codegraph::NodeId> = if let Some(uri) = uri {
            let url = match tower_lsp::lsp_types::Url::parse(uri) {
                Ok(u) => u,
                Err(_) => return serde_json::json!({"error": "Invalid URI"}),
            };
            let path = match url.to_file_path() {
                Ok(p) => p,
                Err(_) => return serde_json::json!({"error": "Invalid file path"}),
            };
            let path_str = path.to_string_lossy().to_string();
            graph
                .query()
                .property("path", path_str)
                .execute()
                .unwrap_or_default()
        } else if scope == "workspace" {
            // Get all nodes (limited for performance)
            graph
                .query()
                .node_type(codegraph::NodeType::Function)
                .execute()
                .unwrap_or_default()
                .into_iter()
                .take(1000)
                .collect()
        } else {
            vec![]
        };

        let mut unused = Vec::new();
        let total_checked = nodes_to_check.len();

        for node_id in nodes_to_check {
            if let Ok(node) = graph.get_node(node_id) {
                // Skip non-function types
                if node.node_type != codegraph::NodeType::Function {
                    continue;
                }

                // Check if anyone calls this
                let callers = self.backend.query_engine.get_callers(node_id, 1).await;

                if callers.is_empty() {
                    let name = node
                        .properties
                        .get_string("name")
                        .unwrap_or_default();

                    // Skip entry points and test functions
                    if name.starts_with("test_")
                        || name.starts_with("main")
                        || name.contains("handler")
                    {
                        continue;
                    }

                    let is_public = node
                        .properties
                        .get_bool("is_public")
                        .unwrap_or(false);

                    // Higher confidence for private functions
                    let item_confidence = if is_public { 0.5 } else { 0.9 };

                    if item_confidence >= confidence {
                        unused.push(serde_json::json!({
                            "name": name,
                            "node_id": node_id.to_string(),
                            "type": format!("{:?}", node.node_type),
                            "confidence": item_confidence,
                            "is_public": is_public,
                        }));
                    }
                }
            }
        }

        serde_json::json!({
            "unused_items": unused,
            "summary": {
                "total_checked": total_checked,
                "unused_count": unused.len(),
                "scope": scope,
                "min_confidence": confidence,
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
                let name = args
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or(title);
                let description = args
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or(content);
                builder.convention(name, description)
            }
            "project_context" => {
                let topic = args
                    .get("topic")
                    .and_then(|v| v.as_str())
                    .unwrap_or(title);
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

        builder.build().map_err(|e| format!("Failed to build memory: {:?}", e))
    }
}

/// Parse a string into a NodeId
fn parse_node_id(s: &str) -> Option<codegraph::NodeId> {
    // NodeId is u64 in codegraph
    s.parse::<codegraph::NodeId>().ok()
}
