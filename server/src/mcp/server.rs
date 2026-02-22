//! MCP Server Implementation
//!
//! Handles MCP protocol requests and routes them to CodeGraph functionality.

use super::protocol::*;
use super::resources::get_all_resources;
use super::tools::get_all_tools;
use super::transport::AsyncStdioTransport;
use crate::ai_query::QueryEngine;
use crate::git_mining::{GitMiner, MiningConfig};
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

            // Ensure embedding model is available (auto-downloads on first run)
            if let Err(e) = crate::model_download::ensure_model_downloaded() {
                tracing::warn!("Model download failed: {}", e);
            }

            // Initialize memory manager with workspace path
            // Note: Uses .codegraph/memory which may conflict with LSP if both run simultaneously
            if let Err(e) = self.memory_manager.initialize(folder).await {
                tracing::warn!("Failed to initialize memory manager: {:?}", e);
            }
        }

        // Resolve cross-file imports and calls before building indexes
        {
            let mut graph = self.graph.write().await;
            crate::watcher::GraphUpdater::resolve_cross_file_imports(&mut graph);
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
                    let result = self.backend.query_engine.get_callers(start, depth).await;

                    // Get symbol name for fallback message
                    let symbol_name = {
                        let graph = self.backend.graph.read().await;
                        graph
                            .get_node(start)
                            .ok()
                            .and_then(|n| n.properties.get_string("name").map(|s| s.to_string()))
                            .unwrap_or_default()
                    };

                    if result.is_empty() {
                        // Return diagnostic info when no callers found
                        let graph = self.backend.graph.read().await;
                        let edge_count = graph.edge_count();
                        let mut response = serde_json::json!({
                            "callers": [],
                            "diagnostic": {
                                "node_found": true,
                                "node_id": start,
                                "symbol_name": symbol_name,
                                "total_edges_in_graph": edge_count,
                                "note": "No callers found. This may indicate: (1) the function is not called anywhere, (2) the language parser doesn't extract call relationships, or (3) indexes need to be rebuilt."
                            }
                        });
                        // Add fallback metadata if used
                        if used_fallback {
                            if let Some(obj) = response.as_object_mut() {
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
                    } else {
                        let response = serde_json::to_value(&result).map_err(|e| e.to_string())?;
                        // Wrap in object with callers key and add fallback metadata
                        let mut obj = serde_json::Map::new();
                        obj.insert("callers".to_string(), response);
                        obj.insert("symbol_name".to_string(), serde_json::json!(symbol_name));
                        if used_fallback {
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
                        Ok(serde_json::Value::Object(obj))
                    }
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
                    let result = self.backend.query_engine.get_callees(start, depth).await;

                    // Get symbol name for fallback message
                    let symbol_name = {
                        let graph = self.backend.graph.read().await;
                        graph
                            .get_node(start)
                            .ok()
                            .and_then(|n| n.properties.get_string("name").map(|s| s.to_string()))
                            .unwrap_or_default()
                    };

                    if result.is_empty() {
                        // Return diagnostic info when no callees found
                        let graph = self.backend.graph.read().await;
                        let edge_count = graph.edge_count();
                        let mut response = serde_json::json!({
                            "callees": [],
                            "diagnostic": {
                                "node_found": true,
                                "node_id": start,
                                "symbol_name": symbol_name,
                                "total_edges_in_graph": edge_count,
                                "note": "No callees found. This may indicate: (1) the function doesn't call other functions, (2) the language parser doesn't extract call relationships, or (3) indexes need to be rebuilt."
                            }
                        });
                        // Add fallback metadata if used
                        if used_fallback {
                            if let Some(obj) = response.as_object_mut() {
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
                    } else {
                        // Wrap in object with callees key and add fallback metadata
                        let mut obj = serde_json::Map::new();
                        obj.insert(
                            "callees".to_string(),
                            serde_json::to_value(&result).map_err(|e| e.to_string())?,
                        );
                        obj.insert("symbol_name".to_string(), serde_json::json!(symbol_name));
                        if used_fallback {
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
                        Ok(serde_json::Value::Object(obj))
                    }
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

                    // Add fallback metadata if used
                    let mut response = serde_json::to_value(result).map_err(|e| e.to_string())?;
                    if used_fallback {
                        if let Some(obj) = response.as_object_mut() {
                            // Get symbol name for fallback message
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
                    let result = self.backend.query_engine.get_symbol_info(node_id).await;
                    match result {
                        Some(info) => {
                            let mut response =
                                serde_json::to_value(&info).map_err(|e| e.to_string())?;
                            // Add fallback metadata if used
                            if used_fallback {
                                if let Some(obj) = response.as_object_mut() {
                                    obj.insert(
                                        "used_fallback".to_string(),
                                        serde_json::json!(true),
                                    );
                                    obj.insert(
                                        "fallback_message".to_string(),
                                        serde_json::json!(format!(
                                            "No symbol at line {}. Using nearest symbol '{}' instead.",
                                            line.unwrap_or(0),
                                            info.symbol.name
                                        )),
                                    );
                                }
                            }
                            // Optionally include references (callers are already in DetailedSymbolInfo)
                            if include_refs && info.callers.is_empty() {
                                // Only note if explicitly requested but none found
                                if let Some(obj) = response.as_object_mut() {
                                    obj.insert(
                                        "references_note".to_string(),
                                        serde_json::json!("No references found. Check 'callers' field for call sites."),
                                    );
                                }
                            }
                            Ok(response)
                        }
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
                    let mut result = serde_json::Map::new();

                    // Get basic symbol info
                    let symbol_name = if let Some(info) =
                        self.backend.query_engine.get_symbol_info(node_id).await
                    {
                        let name = info.symbol.name.clone();
                        result.insert(
                            "symbol".to_string(),
                            serde_json::to_value(&info).unwrap_or(Value::Null),
                        );
                        name
                    } else {
                        String::new()
                    };

                    // Add fallback metadata if used
                    if used_fallback {
                        result.insert("used_fallback".to_string(), serde_json::json!(true));
                        result.insert(
                            "fallback_message".to_string(),
                            serde_json::json!(format!(
                                "No symbol at line {}. Using nearest symbol '{}' instead.",
                                line.unwrap_or(0),
                                symbol_name
                            )),
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
                let summary = args
                    .get("summary")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let result = self
                    .get_dependency_graph(uri, depth, direction, include_external)
                    .await;

                if summary {
                    let node_count = result
                        .get("nodes")
                        .and_then(|v| v.as_array())
                        .map(|a| a.len())
                        .unwrap_or(0);
                    let edge_count = result
                        .get("edges")
                        .and_then(|v| v.as_array())
                        .map(|a| a.len())
                        .unwrap_or(0);
                    Ok(serde_json::json!({
                        "summary": {
                            "node_count": node_count,
                            "edge_count": edge_count,
                            "depth": depth,
                            "direction": direction,
                        }
                    }))
                } else {
                    Ok(serde_json::to_value(result).map_err(|e| e.to_string())?)
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

                let result = self.get_call_graph(uri, line, depth, direction).await;

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
                    Ok(serde_json::to_value(result).map_err(|e| e.to_string())?)
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

                let result = self.analyze_impact(uri, line, change_type).await;

                if summary {
                    let affected_count = result
                        .get("affected_files")
                        .and_then(|v| v.as_array())
                        .map(|a| a.len())
                        .unwrap_or(0);
                    let risk_score = result
                        .get("risk_score")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    let symbol = result
                        .get("symbol")
                        .cloned()
                        .unwrap_or(serde_json::json!(null));
                    Ok(serde_json::json!({
                        "symbol": symbol,
                        "summary": {
                            "affected_file_count": affected_count,
                            "risk_score": risk_score,
                            "change_type": change_type,
                        }
                    }))
                } else {
                    Ok(serde_json::to_value(result).map_err(|e| e.to_string())?)
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

                let result = self.analyze_coupling(uri, depth).await;

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
                    Ok(serde_json::to_value(result).map_err(|e| e.to_string())?)
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
                let summary = args
                    .get("summary")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let result = self.analyze_complexity(uri, line, threshold).await;

                if summary {
                    // Return just the summary stats, omit per-function details
                    let summary_data = result
                        .get("summary")
                        .cloned()
                        .unwrap_or(serde_json::json!({}));
                    Ok(serde_json::json!({
                        "summary": summary_data,
                    }))
                } else {
                    Ok(serde_json::to_value(result).map_err(|e| e.to_string())?)
                }
            }

            "codegraph_find_unused_code" => {
                let uri = args.get("uri").and_then(|v| v.as_str());
                let scope = args.get("scope").and_then(|v| v.as_str()).unwrap_or("file");
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
                            "kind": format!("{:?}", r.memory.kind),
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
                let url =
                    tower_lsp::lsp_types::Url::parse(uri).map_err(|_| "Invalid URI".to_string())?;
                let path = url
                    .to_file_path()
                    .map_err(|_| "Invalid file path".to_string())?;
                let path_str = path.to_string_lossy().to_string();

                // Search for memories related to this file
                let kinds = Self::parse_kinds_filter(&args);
                let tags = Self::parse_tags_filter(&args);
                let config = crate::memory::SearchConfig {
                    limit,
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
                            "kind": format!("{:?}", r.memory.kind),
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

                // Check if memory exists before invalidating
                let exists = self
                    .backend
                    .memory_manager
                    .get(id)
                    .await
                    .map_err(|e| format!("Failed to check memory: {:?}", e))?;

                if exists.is_none() {
                    return Ok(serde_json::json!({"error": "Memory not found", "id": id}));
                }

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
                            "kind": format!("{:?}", m.kind),
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
        let nodes = graph.query().property("path", path_str).execute().ok()?;

        if nodes.is_empty() {
            return None;
        }

        // Strategy 1: Exact match (line within symbol range)
        for node_id in &nodes {
            if let Ok(node) = graph.get_node(*node_id) {
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
                    return Some((*node_id, false)); // Exact match, no fallback
                }
            }
        }

        // Strategy 2: Find nearest symbol (no distance limit)
        // Prefer symbols that start after cursor (looking forward)
        let mut best_match: Option<(codegraph::NodeId, i64)> = None;

        for node_id in &nodes {
            if let Ok(node) = graph.get_node(*node_id) {
                let start_line = node
                    .properties
                    .get_int("line_start")
                    .or_else(|| node.properties.get_int("start_line"))
                    .unwrap_or(0);
                let end_line = node
                    .properties
                    .get_int("line_end")
                    .or_else(|| node.properties.get_int("end_line"))
                    .unwrap_or(start_line);

                let target_line = line as i64;

                // Calculate distance - prefer symbols after cursor
                let distance = if start_line > target_line {
                    // Symbol starts after cursor - prefer these
                    start_line - target_line
                } else {
                    // Symbol ends before cursor - add penalty for looking backward
                    (target_line - end_line) + 1000
                };

                if best_match.is_none() || distance < best_match.unwrap().1 {
                    best_match = Some((*node_id, distance));
                }
            }
        }

        best_match.map(|(id, _)| (id, true)) // Fallback was used
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
        use std::collections::HashSet;

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

        // Find the file node
        let start_node = match codegraph::helpers::find_file_by_path(&graph, &path_str) {
            Ok(Some(id)) => id,
            _ => {
                return serde_json::json!({
                    "nodes": [],
                    "edges": []
                })
            }
        };

        // Use built-in BFS for dependency traversal
        let bfs_direction = match direction {
            "imports" => codegraph::Direction::Outgoing,
            "importedBy" => codegraph::Direction::Incoming,
            _ => codegraph::Direction::Both,
        };

        let mut reachable_set: HashSet<codegraph::NodeId> = HashSet::new();
        reachable_set.insert(start_node);
        if let Ok(reachable) = graph.bfs(start_node, bfs_direction, Some(depth)) {
            reachable_set.extend(reachable);
        }

        // Build response nodes
        let mut nodes = Vec::new();
        for &node_id in &reachable_set {
            if let Ok(node) = graph.get_node(node_id) {
                let name = node.properties.get_string("name").unwrap_or_default();
                let node_type = format!("{:?}", node.node_type);
                nodes.push(serde_json::json!({
                    "id": node_id.to_string(),
                    "name": name,
                    "type": node_type,
                }));
            }
        }

        // Collect edges between reachable nodes
        let mut edges = Vec::new();
        let mut seen_edges: HashSet<(codegraph::NodeId, codegraph::NodeId)> = HashSet::new();

        let edge_directions = match direction {
            "imports" => vec![codegraph::Direction::Outgoing],
            "importedBy" => vec![codegraph::Direction::Incoming],
            _ => vec![
                codegraph::Direction::Outgoing,
                codegraph::Direction::Incoming,
            ],
        };

        for &node_id in &reachable_set {
            for &dir in &edge_directions {
                if let Ok(neighbors) = graph.get_neighbors(node_id, dir) {
                    for neighbor_id in neighbors {
                        if !reachable_set.contains(&neighbor_id) {
                            continue;
                        }
                        let (from, to) = match dir {
                            codegraph::Direction::Outgoing => (node_id, neighbor_id),
                            codegraph::Direction::Incoming => (neighbor_id, node_id),
                            codegraph::Direction::Both => (node_id, neighbor_id),
                        };
                        if seen_edges.insert((from, to)) {
                            edges.push(serde_json::json!({
                                "from": from.to_string(),
                                "to": to.to_string(),
                                "type": "depends_on",
                            }));
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
        // Use fallback for better symbol discovery
        let (start, used_fallback) = match self.find_nearest_node_with_fallback(uri, line).await {
            Some((id, fallback)) => (id, fallback),
            None => {
                return serde_json::json!({
                    "nodes": [],
                    "edges": [],
                    "message": "Could not find symbol at location"
                })
            }
        };

        // Get symbol name for fallback message
        let symbol_name = {
            let graph = self.backend.graph.read().await;
            graph
                .get_node(start)
                .ok()
                .and_then(|n| n.properties.get_string("name").map(|s| s.to_string()))
                .unwrap_or_default()
        };

        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        let mut seen = std::collections::HashSet::new();
        seen.insert(start); // Don't include start node in results

        // Get callers and/or callees based on direction
        match direction {
            "callers" => {
                let callers = self.backend.query_engine.get_callers(start, depth).await;
                for caller in callers {
                    if seen.insert(caller.node_id) {
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
            }
            "callees" => {
                let callees = self.backend.query_engine.get_callees(start, depth).await;
                for callee in callees {
                    if seen.insert(callee.node_id) {
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
            }
            _ => {
                // Both directions
                let callers = self.backend.query_engine.get_callers(start, depth).await;
                let callees = self.backend.query_engine.get_callees(start, depth).await;

                for caller in callers {
                    if seen.insert(caller.node_id) {
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
                }

                for callee in callees {
                    if seen.insert(callee.node_id) {
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
        }

        // Build base response
        let mut response = if nodes.is_empty() {
            let graph = self.backend.graph.read().await;
            let edge_count = graph.edge_count();
            serde_json::json!({
                "root": start.to_string(),
                "symbol_name": symbol_name,
                "nodes": nodes,
                "edges": edges,
                "diagnostic": {
                    "node_found": true,
                    "total_edges_in_graph": edge_count,
                    "note": "No call relationships found. Call graph analysis depends on language parser support for extracting call edges. Some parsers may have limited call extraction capabilities."
                }
            })
        } else {
            serde_json::json!({
                "root": start.to_string(),
                "symbol_name": symbol_name,
                "nodes": nodes,
                "edges": edges
            })
        };

        // Add fallback metadata if used
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

        response
    }

    /// Analyze impact of changes to a symbol
    async fn analyze_impact(&self, uri: &str, line: u32, change_type: &str) -> serde_json::Value {
        // Use fallback for better symbol discovery
        let (start, used_fallback) = match self.find_nearest_node_with_fallback(uri, line).await {
            Some((id, fallback)) => (id, fallback),
            None => {
                return serde_json::json!({
                    "impacted": [],
                    "risk_level": "unknown",
                    "message": "Could not find symbol at location"
                })
            }
        };

        // Get symbol name for fallback message
        let symbol_name = {
            let graph = self.backend.graph.read().await;
            graph
                .get_node(start)
                .ok()
                .and_then(|n| n.properties.get_string("name").map(|s| s.to_string()))
                .unwrap_or_default()
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

        let mut response = serde_json::json!({
            "symbol_id": start.to_string(),
            "symbol_name": symbol_name,
            "change_type": change_type,
            "impacted": impacted,
            "total_impacted": callers.len(),
            "direct_impacted": callers.iter().filter(|c| c.depth == 1).count(),
            "risk_level": risk_level,
        });

        // Add fallback metadata if used
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

        response
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
        let start_time = std::time::Instant::now();

        // Resolve target symbol
        let (target, used_fallback) = match self.find_nearest_node_with_fallback(uri, line).await {
            Some(result) => result,
            None => {
                return serde_json::json!({
                    "error": "No symbols found in file. Try indexing the workspace first.",
                    "uri": uri,
                    "line": line
                })
            }
        };

        // Get primary context (name, type, language, source code)
        let primary_context = {
            let graph = self.backend.graph.read().await;
            let node = match graph.get_node(target) {
                Ok(n) => n,
                Err(_) => return serde_json::json!({ "error": "Could not load node" }),
            };

            let name = node
                .properties
                .get_string("name")
                .unwrap_or_default()
                .to_string();
            let node_type = format!("{:?}", node.node_type).to_lowercase();
            let language = node
                .properties
                .get_string("language")
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    node.properties
                        .get_string("path")
                        .and_then(|p| {
                            std::path::Path::new(p)
                                .extension()
                                .and_then(|e| e.to_str())
                                .map(|e| e.to_string())
                        })
                        .unwrap_or_else(|| "unknown".to_string())
                });
            let path = node
                .properties
                .get_string("path")
                .unwrap_or_default()
                .to_string();
            let line_start = node
                .properties
                .get_int("line_start")
                .or_else(|| node.properties.get_int("start_line"))
                .unwrap_or(0);
            let line_end = node
                .properties
                .get_int("line_end")
                .or_else(|| node.properties.get_int("end_line"))
                .unwrap_or(line_start);

            (name, node_type, language, path, line_start, line_end)
        };
        let (name, node_type, language, _path, line_start, line_end) = primary_context;

        // Get source code
        let source_code = self
            .get_symbol_source(target)
            .await
            .unwrap_or_else(|| "<source not available>".to_string());

        // Token budget
        let mut budget_used: usize = source_code.len() / 4;
        let budget_remaining = max_tokens.saturating_sub(budget_used);

        // Build primary context JSON
        let primary = serde_json::json!({
            "type": node_type,
            "name": name,
            "code": source_code,
            "language": language,
            "location": {
                "uri": uri,
                "range": {
                    "start": { "line": line_start, "character": 0 },
                    "end": { "line": line_end, "character": 0 },
                }
            }
        });

        // Get intent-specific related symbols, dependencies, usage examples
        let (related_symbols, dependencies, usage_examples, architecture) = {
            let graph = self.backend.graph.read().await;
            let mut remaining_budget = budget_remaining;

            let related =
                self.get_intent_related_symbols(&graph, target, intent, &mut remaining_budget);

            let deps = self.get_node_dependencies(&graph, target);

            let usage = self.get_node_usage_examples(&graph, target, &name, &mut remaining_budget);

            let arch = self.get_node_architecture(&graph, target);

            budget_used = max_tokens.saturating_sub(remaining_budget);

            (related, deps, usage, arch)
        };

        let query_time = start_time.elapsed().as_millis() as u64;

        let mut response = serde_json::json!({
            "primaryContext": primary,
            "relatedSymbols": related_symbols,
            "dependencies": dependencies,
            "usageExamples": usage_examples,
            "architecture": architecture,
            "metadata": {
                "totalTokens": budget_used,
                "queryTime": query_time,
            }
        });

        // Add fallback metadata if used
        if used_fallback {
            if let Some(obj) = response.get_mut("metadata").and_then(|m| m.as_object_mut()) {
                obj.insert("usedFallback".to_string(), serde_json::json!(true));
                obj.insert(
                    "fallbackMessage".to_string(),
                    serde_json::json!(format!(
                        "No symbol at line {}. Using nearest symbol '{}' instead.",
                        line, name
                    )),
                );
            }
        }

        response
    }

    /// Get intent-specific related symbols with source code and relevance scores.
    fn get_intent_related_symbols(
        &self,
        graph: &CodeGraph,
        node_id: codegraph::NodeId,
        intent: &str,
        budget: &mut usize,
    ) -> Vec<serde_json::Value> {
        use codegraph::{Direction, EdgeType};
        let mut symbols = Vec::new();
        let mut seen = std::collections::HashSet::new();
        seen.insert(node_id); // Don't include the target itself

        let outgoing = self.get_edges(graph, node_id, Direction::Outgoing);
        let incoming = self.get_edges(graph, node_id, Direction::Incoming);

        match intent {
            "explain" => {
                // Priority 1: Direct dependencies (things this symbol uses)
                for (_, target, _) in outgoing.iter().take(5) {
                    if *budget == 0 {
                        break;
                    }
                    if seen.insert(*target) {
                        if let Some(sym) =
                            self.make_related_symbol(graph, *target, "uses", 1.0, budget)
                        {
                            symbols.push(sym);
                        }
                    }
                }
                // Priority 2: Direct callers
                for (source, _, _) in incoming
                    .iter()
                    .filter(|(_, _, t)| *t == EdgeType::Calls)
                    .take(3)
                {
                    if *budget == 0 {
                        break;
                    }
                    if seen.insert(*source) {
                        if let Some(sym) =
                            self.make_related_symbol(graph, *source, "called_by", 0.8, budget)
                        {
                            symbols.push(sym);
                        }
                    }
                }
                // Priority 3: Parent type (for methods)
                for (source, _, _) in incoming.iter().filter(|(_, _, t)| *t == EdgeType::Extends) {
                    if *budget == 0 {
                        break;
                    }
                    if seen.insert(*source) {
                        if let Some(sym) =
                            self.make_related_symbol(graph, *source, "inherits", 0.9, budget)
                        {
                            symbols.push(sym);
                        }
                    }
                }
            }
            "modify" => {
                // Priority 1: Tests for this symbol
                for (source, _, _) in incoming
                    .iter()
                    .filter(|(_, _, t)| *t == EdgeType::Calls)
                    .take(5)
                {
                    if *budget == 0 {
                        break;
                    }
                    if seen.insert(*source) {
                        if let Ok(n) = graph.get_node(*source) {
                            let n_name = n.properties.get_string("name").unwrap_or("");
                            if n_name.starts_with("test_") || n_name.ends_with("_test") {
                                if let Some(sym) =
                                    self.make_related_symbol(graph, *source, "tests", 1.0, budget)
                                {
                                    symbols.push(sym);
                                }
                            }
                        }
                    }
                }
                // Priority 2: All non-test callers
                for (source, _, _) in incoming
                    .iter()
                    .filter(|(_, _, t)| *t == EdgeType::Calls)
                    .take(5)
                {
                    if *budget == 0 {
                        break;
                    }
                    if seen.insert(*source) {
                        if let Ok(n) = graph.get_node(*source) {
                            let n_name = n.properties.get_string("name").unwrap_or("");
                            if !n_name.starts_with("test_") && !n_name.ends_with("_test") {
                                if let Some(sym) = self.make_related_symbol(
                                    graph,
                                    *source,
                                    "called_by",
                                    0.9,
                                    budget,
                                ) {
                                    symbols.push(sym);
                                }
                            }
                        }
                    }
                }
            }
            "debug" => {
                // Call chain up to entry point (uses seen for dedup)
                let mut current = node_id;
                let mut depth = 0;

                while depth < 5 && *budget > 0 {
                    let cur_incoming = self.get_edges(graph, current, Direction::Incoming);
                    let caller = cur_incoming
                        .iter()
                        .filter(|(_, _, t)| *t == EdgeType::Calls)
                        .find(|(source, _, _)| !seen.contains(source));

                    if let Some((source, _, _)) = caller {
                        seen.insert(*source);
                        let relevance = 1.0 - (depth as f64 * 0.1);
                        let relationship = format!("call_chain_depth_{depth}");
                        if let Some(sym) = self.make_related_symbol(
                            graph,
                            *source,
                            &relationship,
                            relevance,
                            budget,
                        ) {
                            symbols.push(sym);
                        }
                        current = *source;
                        depth += 1;
                    } else {
                        break;
                    }
                }
                // Data dependencies
                for (_, target, _) in outgoing.iter().take(3) {
                    if *budget == 0 {
                        break;
                    }
                    if seen.insert(*target) {
                        if let Some(sym) =
                            self.make_related_symbol(graph, *target, "data_flow", 0.8, budget)
                        {
                            symbols.push(sym);
                        }
                    }
                }
            }
            "test" => {
                // Existing tests as examples
                for (source, _, _) in incoming
                    .iter()
                    .filter(|(_, _, t)| *t == EdgeType::Calls)
                    .take(3)
                {
                    if *budget == 0 {
                        break;
                    }
                    if seen.insert(*source) {
                        if let Ok(n) = graph.get_node(*source) {
                            let n_name = n.properties.get_string("name").unwrap_or("");
                            if n_name.starts_with("test_") || n_name.ends_with("_test") {
                                if let Some(sym) = self.make_related_symbol(
                                    graph,
                                    *source,
                                    "example_test",
                                    0.9,
                                    budget,
                                ) {
                                    symbols.push(sym);
                                }
                            }
                        }
                    }
                }
                // Dependencies that might need mocking
                for (_, target, _) in outgoing.iter().take(3) {
                    if *budget == 0 {
                        break;
                    }
                    if seen.insert(*target) {
                        if let Some(sym) = self.make_related_symbol(
                            graph,
                            *target,
                            "dependency_to_mock",
                            0.7,
                            budget,
                        ) {
                            symbols.push(sym);
                        }
                    }
                }
            }
            _ => {}
        }

        symbols
    }

    /// Create a related symbol JSON object, consuming token budget.
    fn make_related_symbol(
        &self,
        graph: &CodeGraph,
        node_id: codegraph::NodeId,
        relationship: &str,
        relevance: f64,
        budget: &mut usize,
    ) -> Option<serde_json::Value> {
        let node = graph.get_node(node_id).ok()?;
        let name = node
            .properties
            .get_string("name")
            .unwrap_or_default()
            .to_string();
        let path = node
            .properties
            .get_string("path")
            .unwrap_or_default()
            .to_string();
        let line_start = node
            .properties
            .get_int("line_start")
            .or_else(|| node.properties.get_int("start_line"))
            .unwrap_or(0);
        let line_end = node
            .properties
            .get_int("line_end")
            .or_else(|| node.properties.get_int("end_line"))
            .unwrap_or(line_start);

        // Read source code
        let code = if !path.is_empty() && line_start > 0 {
            std::fs::read_to_string(&path).ok().and_then(|content| {
                let lines: Vec<&str> = content.lines().collect();
                let start = (line_start as usize).saturating_sub(1);
                let end = (line_end as usize).min(lines.len());
                if start < end {
                    Some(lines[start..end].join("\n"))
                } else {
                    None
                }
            })
        } else {
            None
        };

        let code = code.unwrap_or_default();
        let tokens = code.len() / 4;

        if tokens > 0 && *budget < tokens {
            return None; // Not enough budget
        }
        *budget = budget.saturating_sub(tokens);

        Some(serde_json::json!({
            "name": name,
            "relationship": relationship,
            "code": code,
            "location": {
                "uri": if path.starts_with('/') { format!("file://{path}") } else { path },
                "range": {
                    "start": { "line": line_start, "character": 0 },
                    "end": { "line": line_end, "character": 0 },
                }
            },
            "relevanceScore": relevance,
        }))
    }

    /// Get edges for a node in a given direction.
    fn get_edges(
        &self,
        graph: &CodeGraph,
        node_id: codegraph::NodeId,
        direction: codegraph::Direction,
    ) -> Vec<(codegraph::NodeId, codegraph::NodeId, codegraph::EdgeType)> {
        let neighbors = match graph.get_neighbors(node_id, direction) {
            Ok(n) => n,
            Err(_) => return Vec::new(),
        };

        let mut edges = Vec::new();
        for neighbor_id in neighbors {
            let (source, target) = match direction {
                codegraph::Direction::Outgoing => (node_id, neighbor_id),
                codegraph::Direction::Incoming => (neighbor_id, node_id),
                codegraph::Direction::Both => {
                    // Try both directions for edges
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

    /// Get import dependencies for a node.
    fn get_node_dependencies(
        &self,
        graph: &CodeGraph,
        node_id: codegraph::NodeId,
    ) -> Vec<serde_json::Value> {
        let outgoing = self.get_edges(graph, node_id, codegraph::Direction::Outgoing);
        outgoing
            .iter()
            .filter(|(_, _, t)| *t == codegraph::EdgeType::Imports)
            .take(10)
            .filter_map(|(_, target, _)| {
                let dep_node = graph.get_node(*target).ok()?;
                let name = dep_node.properties.get_string("name")?.to_string();
                Some(serde_json::json!({
                    "name": name,
                    "type": "import",
                    "code": null,
                }))
            })
            .collect()
    }

    /// Get usage examples — callers that demonstrate how this symbol is used.
    fn get_node_usage_examples(
        &self,
        graph: &CodeGraph,
        node_id: codegraph::NodeId,
        target_name: &str,
        budget: &mut usize,
    ) -> serde_json::Value {
        let incoming = self.get_edges(graph, node_id, codegraph::Direction::Incoming);
        let usages: Vec<_> = incoming
            .iter()
            .filter(|(_, _, t)| {
                *t == codegraph::EdgeType::Calls || *t == codegraph::EdgeType::References
            })
            .collect();

        let mut examples = Vec::new();
        for (source, _, _) in usages.iter().take(3) {
            if *budget == 0 {
                break;
            }
            let usage_node = match graph.get_node(*source) {
                Ok(n) => n,
                Err(_) => continue,
            };
            let usage_name = usage_node.properties.get_string("name").unwrap_or("");
            if usage_name.starts_with("test_") || usage_name.ends_with("_test") {
                continue;
            }

            let path = usage_node
                .properties
                .get_string("path")
                .unwrap_or_default()
                .to_string();
            let line_start = usage_node
                .properties
                .get_int("line_start")
                .or_else(|| usage_node.properties.get_int("start_line"))
                .unwrap_or(0);
            let line_end = usage_node
                .properties
                .get_int("line_end")
                .or_else(|| usage_node.properties.get_int("end_line"))
                .unwrap_or(line_start);

            let code = if !path.is_empty() && line_start > 0 {
                std::fs::read_to_string(&path).ok().and_then(|content| {
                    let lines: Vec<&str> = content.lines().collect();
                    let start = (line_start as usize).saturating_sub(1);
                    let end = (line_end as usize).min(lines.len());
                    if start < end {
                        Some(lines[start..end].join("\n"))
                    } else {
                        None
                    }
                })
            } else {
                None
            };

            if let Some(code) = code {
                let tokens = code.len() / 4;
                if *budget < tokens {
                    break;
                }
                *budget = budget.saturating_sub(tokens);

                let description = Self::generate_usage_description(usage_name, target_name, &code);
                examples.push(serde_json::json!({
                    "code": code,
                    "location": {
                        "uri": if path.starts_with('/') { format!("file://{path}") } else { path },
                        "range": {
                            "start": { "line": line_start, "character": 0 },
                            "end": { "line": line_end, "character": 0 },
                        }
                    },
                    "description": description,
                }));
            }
        }

        if examples.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::json!(examples)
        }
    }

    /// Get architecture/layer information for a node.
    fn get_node_architecture(
        &self,
        graph: &CodeGraph,
        node_id: codegraph::NodeId,
    ) -> serde_json::Value {
        let node = match graph.get_node(node_id) {
            Ok(n) => n,
            Err(_) => return serde_json::Value::Null,
        };

        let path_str = node
            .properties
            .get_string("path")
            .unwrap_or_default()
            .to_string();
        if path_str.is_empty() {
            return serde_json::Value::Null;
        }

        let module = std::path::Path::new(&path_str)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        let layer = Self::detect_layer(&path_str);

        // Get neighbor modules
        let mut neighbors = std::collections::HashSet::new();
        let outgoing = self.get_edges(graph, node_id, codegraph::Direction::Outgoing);
        let incoming = self.get_edges(graph, node_id, codegraph::Direction::Incoming);

        for (source, target, _) in outgoing.iter().chain(incoming.iter()) {
            let other_id = if *source == node_id { *target } else { *source };
            if let Ok(other_node) = graph.get_node(other_id) {
                if let Some(other_path) = other_node.properties.get_string("path") {
                    if let Some(other_module) = std::path::Path::new(other_path)
                        .file_stem()
                        .and_then(|s| s.to_str())
                    {
                        if other_module != module {
                            neighbors.insert(other_module.to_string());
                        }
                    }
                }
            }
        }

        serde_json::json!({
            "module": module,
            "layer": layer,
            "neighbors": neighbors.into_iter().collect::<Vec<_>>(),
        })
    }

    /// Detect architectural layer from file path.
    fn detect_layer(path: &str) -> Option<String> {
        let path_lower = path.to_lowercase();

        let layer_patterns: &[(&[&str], &str)] = &[
            (
                &[
                    "controllers",
                    "controller",
                    "routes",
                    "router",
                    "endpoints",
                    "api/",
                ],
                "controller",
            ),
            (
                &["views", "view", "templates", "pages", "components", "ui/"],
                "presentation",
            ),
            (&["handlers", "handler"], "handler"),
            (
                &[
                    "services",
                    "service",
                    "usecases",
                    "use_cases",
                    "application/",
                ],
                "service",
            ),
            (&["commands", "command"], "command"),
            (&["queries", "query"], "query"),
            (
                &["models", "model", "entities", "entity", "domain/"],
                "domain",
            ),
            (&["aggregates", "aggregate"], "aggregate"),
            (&["value_objects", "valueobjects"], "value_object"),
            (&["repositories", "repository", "repos"], "repository"),
            (&["database", "db/", "persistence"], "persistence"),
            (
                &["adapters", "adapter", "infrastructure/"],
                "infrastructure",
            ),
            (&["clients", "client"], "client"),
            (&["providers", "provider"], "provider"),
            (&["middleware", "middlewares"], "middleware"),
            (&["utils", "util", "helpers", "helper", "lib/"], "utility"),
            (&["config", "configuration", "settings"], "configuration"),
            (&["types", "interfaces", "contracts"], "contract"),
            (&["tests", "test", "__tests__", "spec", "specs"], "test"),
            (&["fixtures", "mocks", "stubs"], "test_support"),
        ];

        for (patterns, layer) in layer_patterns {
            for pattern in *patterns {
                if path_lower.contains(pattern) {
                    return Some(layer.to_string());
                }
            }
        }

        // Fallback: infer from file name
        let file_name = std::path::Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();

        if file_name.ends_with("controller") || file_name.ends_with("_controller") {
            return Some("controller".to_string());
        }
        if file_name.ends_with("service") || file_name.ends_with("_service") {
            return Some("service".to_string());
        }
        if file_name.ends_with("repository")
            || file_name.ends_with("_repository")
            || file_name.ends_with("repo")
        {
            return Some("repository".to_string());
        }
        if file_name.ends_with("model")
            || file_name.ends_with("_model")
            || file_name.ends_with("entity")
        {
            return Some("domain".to_string());
        }
        if file_name.ends_with("handler") || file_name.ends_with("_handler") {
            return Some("handler".to_string());
        }
        if file_name.ends_with("middleware") {
            return Some("middleware".to_string());
        }
        if file_name.starts_with("test_")
            || file_name.ends_with("_test")
            || file_name.ends_with(".test")
            || file_name.ends_with(".spec")
        {
            return Some("test".to_string());
        }

        None
    }

    /// Generate a helpful description for a usage example.
    fn generate_usage_description(caller_name: &str, target_name: &str, code: &str) -> String {
        let is_async = code.contains("await") || code.contains("async");
        let is_error_handling =
            code.contains("try") || code.contains("catch") || code.contains("?");
        let is_conditional =
            code.contains("if") || code.contains("match") || code.contains("switch");

        let mut parts = Vec::new();
        if !caller_name.is_empty() {
            parts.push(format!("`{caller_name}` calls `{target_name}`"));
        } else {
            parts.push(format!("Usage of `{target_name}`"));
        }
        if is_async {
            parts.push("(async)".to_string());
        }
        if is_error_handling {
            parts.push("with error handling".to_string());
        }
        if is_conditional {
            parts.push("conditionally".to_string());
        }

        parts.join(" ")
    }

    /// Find related tests for a symbol or file.
    ///
    /// Strategy:
    /// 1. If a target symbol is found, search for test entry points that call it.
    /// 2. Search for test functions in the same file.
    /// 3. Search for test functions in adjacent test files (foo.test.ts, tests/foo.rs, etc).
    async fn find_related_tests(&self, uri: &str, line: u32, limit: usize) -> serde_json::Value {
        let url = match tower_lsp::lsp_types::Url::parse(uri) {
            Ok(u) => u,
            Err(_) => {
                return serde_json::json!({
                    "tests": [],
                    "message": "Invalid URI"
                })
            }
        };
        let file_path = match url.to_file_path() {
            Ok(p) => p,
            Err(_) => {
                return serde_json::json!({
                    "tests": [],
                    "message": "Invalid file path"
                })
            }
        };
        let path_str = file_path.to_string_lossy().to_string();

        // Try to find the target symbol (optional — test discovery works without it)
        let (target, used_fallback, symbol_name) =
            match self.find_nearest_node_with_fallback(uri, line).await {
                Some((id, fallback)) => {
                    let name = {
                        let graph = self.backend.graph.read().await;
                        graph
                            .get_node(id)
                            .ok()
                            .and_then(|n| n.properties.get_string("name").map(|s| s.to_string()))
                            .unwrap_or_default()
                    };
                    (Some(id), fallback, name)
                }
                None => (None, false, String::new()),
            };

        let mut related_tests = Vec::new();
        let mut seen_ids = std::collections::HashSet::<codegraph::NodeId>::new();

        // Stage 1: If we have a target, find test entry points that call it
        if let Some(target_id) = target {
            seen_ids.insert(target_id);
            let entry_types = vec![crate::ai_query::EntryType::TestEntry];
            let tests = self
                .backend
                .query_engine
                .find_entry_points(&entry_types)
                .await;

            for test in tests.iter().take(limit * 2) {
                if related_tests.len() >= limit {
                    break;
                }
                let callees = self.backend.query_engine.get_callees(test.node_id, 3).await;
                if callees.iter().any(|c| c.node_id == target_id)
                    && seen_ids.insert(test.node_id)
                {
                    related_tests.push(serde_json::json!({
                        "name": test.symbol.name,
                        "id": test.node_id.to_string(),
                        "relationship": "calls_target",
                    }));
                }
            }
        }

        // Stage 2: Find test functions in the same file
        if related_tests.len() < limit {
            let graph = self.backend.graph.read().await;
            if let Ok(file_nodes) = graph.query().property("path", path_str.clone()).execute() {
                for node_id in file_nodes {
                    if !seen_ids.insert(node_id) || related_tests.len() >= limit {
                        continue;
                    }
                    if let Ok(node) = graph.get_node(node_id) {
                        if node.node_type != codegraph::NodeType::Function {
                            continue;
                        }
                        if Self::is_mcp_test_node(node) {
                            let test_name =
                                node.properties.get_string("name").unwrap_or("").to_string();
                            related_tests.push(serde_json::json!({
                                "name": test_name,
                                "id": node_id.to_string(),
                                "relationship": "same_file",
                            }));
                        }
                    }
                }
            }
        }

        // Stage 3: Find test functions in adjacent test files
        if related_tests.len() < limit {
            let graph = self.backend.graph.read().await;
            let test_path_patterns = Self::generate_test_path_patterns(&path_str);
            for test_path in &test_path_patterns {
                if related_tests.len() >= limit {
                    break;
                }
                if let Ok(test_nodes) = graph.query().property("path", test_path.as_str()).execute()
                {
                    for node_id in test_nodes {
                        if !seen_ids.insert(node_id) || related_tests.len() >= limit {
                            continue;
                        }
                        if let Ok(node) = graph.get_node(node_id) {
                            if node.node_type != codegraph::NodeType::Function {
                                continue;
                            }
                            if Self::is_mcp_test_node(node) {
                                let test_name =
                                    node.properties.get_string("name").unwrap_or("").to_string();
                                related_tests.push(serde_json::json!({
                                    "name": test_name,
                                    "id": node_id.to_string(),
                                    "relationship": "adjacent_file",
                                }));
                            }
                        }
                    }
                }
            }
        }

        let mut response = if let Some(target_id) = target {
            serde_json::json!({
                "target_id": target_id.to_string(),
                "symbol_name": symbol_name,
                "tests": related_tests,
                "total": related_tests.len(),
            })
        } else {
            serde_json::json!({
                "file": path_str,
                "tests": related_tests,
                "total": related_tests.len(),
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

        response
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
                    let start = node.properties.get_int("line_start").unwrap_or(0) as u32;
                    let end = node.properties.get_int("line_end").unwrap_or(0) as u32;
                    if target_line < start || target_line > end {
                        continue;
                    }
                }

                let name = node.properties.get_string("name").unwrap_or_default();

                // Read AST-based complexity from node properties (populated by upstream parsers)
                let complexity = node.properties.get_int("complexity").unwrap_or(1) as u32;

                let grade = node
                    .properties
                    .get_string("complexity_grade")
                    .and_then(|s| s.chars().next())
                    .unwrap_or(match complexity {
                        0..=5 => 'A',
                        6..=10 => 'B',
                        11..=20 => 'C',
                        21..=50 => 'D',
                        _ => 'F',
                    });

                total_complexity += complexity;
                max_complexity = max_complexity.max(complexity);
                if complexity > threshold {
                    above_threshold += 1;
                }

                functions.push(serde_json::json!({
                    "name": name,
                    "complexity": complexity,
                    "grade": grade.to_string(),
                    "node_id": node_id.to_string(),
                    "details": {
                        "branches": node.properties.get_int("complexity_branches").unwrap_or(0),
                        "loops": node.properties.get_int("complexity_loops").unwrap_or(0),
                        "logical_operators": node.properties.get_int("complexity_logical_ops").unwrap_or(0),
                        "nesting_depth": node.properties.get_int("complexity_nesting").unwrap_or(0),
                    }
                }));
            }
        }

        let avg_complexity = if !functions.is_empty() {
            total_complexity as f64 / functions.len() as f64
        } else {
            0.0
        };

        // Include diagnostic note when no functions found
        if functions.is_empty() {
            serde_json::json!({
                "functions": functions,
                "summary": {
                    "total_functions": 0,
                    "average_complexity": 0.0,
                    "max_complexity": 0,
                    "above_threshold": 0,
                    "threshold": threshold,
                },
                "note": "No functions found in this file. This may indicate: (1) the language parser doesn't extract function-level details for this file type, (2) the file doesn't contain any functions, or (3) the workspace needs to be re-indexed."
            })
        } else {
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
    }

    /// Find unused code
    async fn find_unused_code(
        &self,
        uri: Option<&str>,
        scope: &str,
        include_tests: bool,
        confidence: f64,
    ) -> serde_json::Value {
        let graph = self.backend.graph.read().await;

        // Collect candidate nodes based on scope
        let mut nodes_to_check: Vec<codegraph::NodeId> = if let Some(uri) = uri {
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
        } else if scope == "workspace" || scope == "module" {
            // Gather functions, classes, variables, types, interfaces
            let mut all = Vec::new();
            for node_type in &[
                codegraph::NodeType::Function,
                codegraph::NodeType::Class,
                codegraph::NodeType::Variable,
                codegraph::NodeType::Type,
                codegraph::NodeType::Interface,
            ] {
                if let Ok(ids) = graph.query().node_type(*node_type).execute() {
                    all.extend(ids);
                }
            }
            // Exclude build output directories to avoid counting compiled duplicates
            all.retain(|&node_id| {
                graph
                    .get_node(node_id)
                    .map(|node| {
                        let path = node.properties.get_string("path").unwrap_or("");
                        !Self::is_build_output_path(path)
                    })
                    .unwrap_or(true)
            });
            all.into_iter().take(2000).collect()
        } else {
            vec![]
        };

        // When include_tests is false, filter out test nodes from the checked set
        if !include_tests {
            nodes_to_check.retain(|&node_id| {
                graph
                    .get_node(node_id)
                    .map(|node| !Self::is_mcp_test_node(node))
                    .unwrap_or(true)
            });
        }

        let mut unused = Vec::new();
        let total_checked = nodes_to_check.len();

        for node_id in nodes_to_check {
            if let Ok(node) = graph.get_node(node_id) {
                // Skip structural node types (files, modules)
                if node.node_type == codegraph::NodeType::CodeFile
                    || node.node_type == codegraph::NodeType::Module
                {
                    continue;
                }

                // Skip type definitions — interfaces and type aliases are structural,
                // they're referenced by type system, not "called"
                if node.node_type == codegraph::NodeType::Interface
                    || node.node_type == codegraph::NodeType::Type
                {
                    continue;
                }

                let name = node.properties.get_string("name").unwrap_or_default();

                // Skip anonymous/synthetic names
                if name == "arrow_function"
                    || name.is_empty()
                    || name == "anonymous"
                    || name == "constructor"
                {
                    continue;
                }

                // Skip well-known entry points and lifecycle hooks
                if Self::is_framework_entry_point(name) {
                    continue;
                }

                // Skip well-known trait impl methods (called by Rust/language framework dispatch)
                if Self::is_trait_impl_method(name) {
                    continue;
                }

                // Check for callers (via Calls edges)
                let callers = self.backend.query_engine.get_callers(node_id, 1).await;

                // When include_tests is false, filter out callers that are test functions
                let effective_callers = if !include_tests {
                    callers
                        .iter()
                        .filter(|c| {
                            graph
                                .get_node(c.node_id)
                                .map(|n| !Self::is_mcp_test_node(n))
                                .unwrap_or(true)
                        })
                        .count()
                } else {
                    callers.len()
                };

                // Check for usage edges (excluding structural Contains/Defines edges)
                let has_usage_edge = graph
                    .get_neighbors(node_id, codegraph::Direction::Incoming)
                    .map(|neighbors| {
                        neighbors.iter().any(|&neighbor_id| {
                            // When include_tests is false, skip test callers
                            if !include_tests {
                                if let Ok(n) = graph.get_node(neighbor_id) {
                                    if Self::is_mcp_test_node(n) {
                                        return false;
                                    }
                                }
                            }
                            graph
                                .get_edges_between(neighbor_id, node_id)
                                .unwrap_or_default()
                                .iter()
                                .any(|&edge_id| {
                                    graph
                                        .get_edge(edge_id)
                                        .map(|e| {
                                            matches!(
                                                e.edge_type,
                                                codegraph::EdgeType::Imports
                                                    | codegraph::EdgeType::ImportsFrom
                                                    | codegraph::EdgeType::References
                                                    | codegraph::EdgeType::Uses
                                                    | codegraph::EdgeType::Invokes
                                                    | codegraph::EdgeType::Instantiates
                                            )
                                        })
                                        .unwrap_or(false)
                                })
                        })
                    })
                    .unwrap_or(false);

                if effective_callers == 0 && !has_usage_edge {
                    let is_public = node
                        .properties
                        .get_bool("is_public")
                        .or_else(|| node.properties.get_bool("exported"))
                        .unwrap_or(false);
                    let visibility = node.properties.get_string("visibility").unwrap_or_default();
                    let is_exported = is_public || visibility == "public" || visibility == "pub";

                    // Confidence scoring with heuristic pattern detection
                    let item_confidence = Self::compute_unused_confidence(name, is_exported, node);

                    if item_confidence >= confidence {
                        unused.push(serde_json::json!({
                            "name": name,
                            "node_id": node_id.to_string(),
                            "type": format!("{:?}", node.node_type),
                            "confidence": item_confidence,
                            "is_public": is_exported,
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

    /// Check if a node is a test function
    fn is_mcp_test_node(node: &codegraph::Node) -> bool {
        // Check is_test property (set by Rust parser for #[test] functions)
        if node.properties.get_bool("is_test").unwrap_or(false) {
            return true;
        }

        let name = node.properties.get_string("name").unwrap_or("");
        let path = node.properties.get_string("path").unwrap_or("");

        let name_is_test = name.starts_with("test_")
            || name.ends_with("_test")
            || name.contains("test ")
            || name.starts_with("Test");

        let path_is_test = path.contains("/test")
            || path.contains("/tests")
            || path.contains("\\test")
            || path.contains("\\tests")
            || path.contains(".test.")
            || path.contains(".spec.")
            || path.contains("_test.");

        name_is_test || path_is_test
    }

    /// Generate candidate test file paths for a source file.
    /// Given `/src/foo.ts`, generates patterns like `/src/foo.test.ts`, `/src/foo.spec.ts`,
    /// `/src/tests/foo.ts`, `/src/__tests__/foo.ts`, `/src/foo_test.rs`, etc.
    fn generate_test_path_patterns(source_path: &str) -> Vec<String> {
        let path = std::path::Path::new(source_path);
        let stem = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s,
            None => return vec![],
        };
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let dir = path
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        let mut patterns = Vec::new();

        if !ext.is_empty() {
            // Adjacent test files: foo.test.ts, foo.spec.ts
            patterns.push(format!("{dir}/{stem}.test.{ext}"));
            patterns.push(format!("{dir}/{stem}.spec.{ext}"));
            // Rust/Go convention: foo_test.rs
            patterns.push(format!("{dir}/{stem}_test.{ext}"));
            // Subdirectory conventions: tests/foo.ts, __tests__/foo.ts, test/foo.ts
            patterns.push(format!("{dir}/tests/{stem}.{ext}"));
            patterns.push(format!("{dir}/__tests__/{stem}.{ext}"));
            patterns.push(format!("{dir}/test/{stem}.{ext}"));
            // Test file with _test suffix in subdirectory
            patterns.push(format!("{dir}/tests/{stem}_test.{ext}"));
        }

        patterns
    }

    /// Check if a path is inside a build output directory.
    /// Handles both absolute (/Users/.../out/foo.js) and relative (out/foo.js) paths.
    fn is_build_output_path(path: &str) -> bool {
        const EXCLUDED_DIRS: &[&str] = &["out", "dist", "target", "node_modules", "build"];
        path.split(['/', '\\'])
            .any(|component| EXCLUDED_DIRS.contains(&component))
    }

    /// Check if a name is a well-known framework entry point or lifecycle hook
    fn is_framework_entry_point(name: &str) -> bool {
        matches!(
            name,
            // Rust/general
            "main"
            // JS test frameworks
            | "it"
            | "describe"
            | "beforeEach"
            | "afterEach"
            | "beforeAll"
            | "afterAll"
            // VS Code extension API
            | "activate"
            | "deactivate"
            // VS Code TreeDataProvider / WebviewProvider
            | "getTreeItem"
            | "getChildren"
            | "getParent"
            | "resolveTreeItem"
            | "resolveWebviewView"
            // VS Code Disposable
            | "dispose"
            | "refresh"
            // LSP protocol methods (called by LSP framework dispatch)
            | "initialized"
            | "shutdown"
            | "did_open"
            | "did_change"
            | "did_save"
            | "did_close"
            | "goto_definition"
            | "references"
            | "hover"
            | "document_symbol"
            | "prepare_call_hierarchy"
            | "incoming_calls"
            | "outgoing_calls"
            | "execute_command"
            | "completion"
            | "code_action"
            | "code_lens"
            | "formatting"
            | "rename"
            | "did_change_configuration"
        )
    }

    /// Check if a name is a well-known trait impl method (Rust/JS framework dispatch)
    fn is_trait_impl_method(name: &str) -> bool {
        matches!(
            name,
            // Rust std trait impls
            "default"
                | "fmt"
                | "from"
                | "into"
                | "clone"
                | "clone_from"
                | "eq"
                | "ne"
                | "partial_cmp"
                | "cmp"
                | "hash"
                | "drop"
                | "deref"
                | "deref_mut"
                | "as_ref"
                | "as_mut"
                | "try_from"
                | "try_into"
                | "from_str"
                | "to_string"
                | "next"
                | "size_hint"
                // Serde
                | "serialize"
                | "deserialize"
                | "visit_str"
                | "visit_map"
                | "visit_seq"
                | "expecting"
                // Iterator/IntoIterator
                | "into_iter"
                | "from_iter"
                // Display/Debug/Error
                | "source"
                | "description"
                // JS built-ins called by runtime
                | "toString"
                | "valueOf"
                | "toJSON"
                | "Symbol.iterator"
                | "[Symbol.iterator]"
        )
    }

    /// Compute confidence score for an unused code candidate.
    /// Lower confidence = more likely a false positive.
    fn compute_unused_confidence(name: &str, is_exported: bool, _node: &codegraph::Node) -> f64 {
        // Dynamic dispatch patterns — very likely called at runtime
        if name.contains("handler")
            || name.contains("Handler")
            || name.contains("callback")
            || name.contains("Callback")
            || name.contains("listener")
            || name.contains("Listener")
            || name.contains("middleware")
            || name.contains("Middleware")
        {
            return 0.2;
        }

        // MCP tool builder functions (called via collected vec, not direct call edges)
        if name.ends_with("_tool") {
            return 0.1;
        }

        // Serde default functions (referenced by #[serde(default = "...")] attribute)
        if name.starts_with("default_") {
            return 0.1;
        }

        // Event handler patterns (on_click, on_change, handleSubmit, etc.)
        if name.starts_with("on_")
            || name.starts_with("on") && name.chars().nth(2).is_some_and(|c| c.is_uppercase())
        {
            return 0.2;
        }
        if name.starts_with("handle") && name.chars().nth(6).is_some_and(|c| c.is_uppercase()) {
            return 0.2;
        }

        // Exported symbols — might be used by consumers outside the indexed workspace
        if is_exported {
            return 0.5;
        }

        // Private/unexported symbols with no callers — very likely unused
        0.9
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
