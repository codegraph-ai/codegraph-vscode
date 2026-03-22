//! Custom LSP requests for CodeGraph-specific features.
//!
//! Tower-LSP handles custom requests through the request method on LanguageServer trait.

use crate::backend::CodeGraphBackend;
use crate::handlers::*;
use crate::watcher::GraphUpdater;
use serde_json::Value;
use tower_lsp::jsonrpc::{Error, Result};

/// Custom request handler dispatcher
impl CodeGraphBackend {
    pub async fn handle_custom_request(&self, method: &str, params: Value) -> Result<Value> {
        match method {
            "codegraph/getDependencyGraph" => {
                let params: DependencyGraphParams = serde_json::from_value(params)
                    .map_err(|e| Error::invalid_params(format!("Invalid params: {e}")))?;
                let response = self.handle_get_dependency_graph(params).await?;
                serde_json::to_value(response).map_err(|_| Error::internal_error())
            }

            "codegraph/getCallGraph" => {
                let params: CallGraphParams = serde_json::from_value(params)
                    .map_err(|e| Error::invalid_params(format!("Invalid params: {e}")))?;
                let response = self.handle_get_call_graph(params).await?;
                serde_json::to_value(response).map_err(|_| Error::internal_error())
            }

            "codegraph/analyzeImpact" => {
                let params: ImpactAnalysisParams = serde_json::from_value(params)
                    .map_err(|e| Error::invalid_params(format!("Invalid params: {e}")))?;
                let response = self.handle_analyze_impact(params).await?;
                serde_json::to_value(response).map_err(|_| Error::internal_error())
            }

            "codegraph/getParserMetrics" => {
                let params: ParserMetricsParams = serde_json::from_value(params)
                    .map_err(|e| Error::invalid_params(format!("Invalid params: {e}")))?;
                let response = self.handle_get_parser_metrics(params).await?;
                serde_json::to_value(response).map_err(|_| Error::internal_error())
            }

            "codegraph/reindexWorkspace" => {
                let total_indexed = self.handle_reindex_workspace().await?;
                serde_json::to_value(serde_json::json!({
                    "status": "success",
                    "message": format!("Workspace reindexed: {total_indexed} files"),
                    "files_indexed": total_indexed
                }))
                .map_err(|_| Error::internal_error())
            }

            "codegraph/getAIContext" => {
                let params: AIContextParams = serde_json::from_value(params)
                    .map_err(|e| Error::invalid_params(format!("Invalid params: {e}")))?;
                let response = self.handle_get_ai_context(params).await?;
                serde_json::to_value(response).map_err(|_| Error::internal_error())
            }

            "codegraph/findRelatedTests" => {
                let params: RelatedTestsParams = serde_json::from_value(params)
                    .map_err(|e| Error::invalid_params(format!("Invalid params: {e}")))?;
                let response = self.handle_find_related_tests(params).await?;
                serde_json::to_value(response).map_err(|_| Error::internal_error())
            }

            "codegraph/getNodeLocation" => {
                let params: GetNodeLocationParams = serde_json::from_value(params)
                    .map_err(|e| Error::invalid_params(format!("Invalid params: {e}")))?;
                let response = self.handle_get_node_location(params).await?;
                serde_json::to_value(response).map_err(|_| Error::internal_error())
            }

            "codegraph/getWorkspaceSymbols" => {
                let params: WorkspaceSymbolsParams = serde_json::from_value(params)
                    .map_err(|e| Error::invalid_params(format!("Invalid params: {e}")))?;
                let response = self.handle_get_workspace_symbols(params).await?;
                serde_json::to_value(response).map_err(|_| Error::internal_error())
            }

            "codegraph/analyzeComplexity" => {
                let params: ComplexityParams = serde_json::from_value(params)
                    .map_err(|e| Error::invalid_params(format!("Invalid params: {e}")))?;
                let response = self.handle_analyze_complexity(params).await?;
                serde_json::to_value(response).map_err(|_| Error::internal_error())
            }

            "codegraph/findUnusedCode" => {
                let params: UnusedCodeParams = serde_json::from_value(params)
                    .map_err(|e| Error::invalid_params(format!("Invalid params: {e}")))?;
                let response = self.handle_find_unused_code(params).await?;
                serde_json::to_value(response).map_err(|_| Error::internal_error())
            }

            "codegraph/analyzeCoupling" => {
                let params: CouplingParams = serde_json::from_value(params)
                    .map_err(|e| Error::invalid_params(format!("Invalid params: {e}")))?;
                let response = self.handle_analyze_coupling(params).await?;
                serde_json::to_value(response).map_err(|_| Error::internal_error())
            }

            // AI Agent Query Primitives
            "codegraph/symbolSearch" => {
                let params: SymbolSearchParams = serde_json::from_value(params)
                    .map_err(|e| Error::invalid_params(format!("Invalid params: {e}")))?;
                let response = self.handle_symbol_search(params).await?;
                serde_json::to_value(response).map_err(|_| Error::internal_error())
            }

            "codegraph/findByImports" => {
                let params: FindByImportsParams = serde_json::from_value(params)
                    .map_err(|e| Error::invalid_params(format!("Invalid params: {e}")))?;
                let response = self.handle_find_by_imports(params).await?;
                serde_json::to_value(response).map_err(|_| Error::internal_error())
            }

            "codegraph/findEntryPoints" => {
                let params: FindEntryPointsParams = serde_json::from_value(params)
                    .map_err(|e| Error::invalid_params(format!("Invalid params: {e}")))?;
                let response = self.handle_find_entry_points(params).await?;
                serde_json::to_value(response).map_err(|_| Error::internal_error())
            }

            "codegraph/traverseGraph" => {
                let params: TraverseGraphParams = serde_json::from_value(params)
                    .map_err(|e| Error::invalid_params(format!("Invalid params: {e}")))?;
                let response = self.handle_traverse_graph(params).await?;
                serde_json::to_value(response).map_err(|_| Error::internal_error())
            }

            "codegraph/getCallers" => {
                let params: GetCallersParams = serde_json::from_value(params)
                    .map_err(|e| Error::invalid_params(format!("Invalid params: {e}")))?;
                let response = self.handle_get_callers(params).await?;
                serde_json::to_value(response).map_err(|_| Error::internal_error())
            }

            "codegraph/getCallees" => {
                let params: GetCallersParams = serde_json::from_value(params)
                    .map_err(|e| Error::invalid_params(format!("Invalid params: {e}")))?;
                let response = self.handle_get_callees(params).await?;
                serde_json::to_value(response).map_err(|_| Error::internal_error())
            }

            "codegraph/getDetailedSymbolInfo" => {
                let params: GetDetailedInfoParams = serde_json::from_value(params)
                    .map_err(|e| Error::invalid_params(format!("Invalid params: {e}")))?;
                let response = self.handle_get_detailed_symbol_info(params).await?;
                serde_json::to_value(response).map_err(|_| Error::internal_error())
            }

            "codegraph/findBySignature" => {
                let params: FindBySignatureParams = serde_json::from_value(params)
                    .map_err(|e| Error::invalid_params(format!("Invalid params: {e}")))?;
                let response = self.handle_find_by_signature(params).await?;
                serde_json::to_value(response).map_err(|_| Error::internal_error())
            }

            "codegraph/findDuplicates" => {
                let threshold = params
                    .get("threshold")
                    .and_then(|v| v.as_f64())
                    .map(|v| v as f32)
                    .unwrap_or(0.7);
                let limit = params
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize)
                    .unwrap_or(20);
                let uri_filter = params.get("uri").and_then(|v| v.as_str());

                let result = self
                    .query_engine
                    .find_duplicates(threshold, limit, uri_filter)
                    .await;

                serde_json::to_value(result).map_err(|_| Error::internal_error())
            }

            "codegraph/findSimilar" => {
                let node_id = if let Some(id_str) = params.get("nodeId").and_then(|v| v.as_str()) {
                    id_str
                        .parse::<codegraph::NodeId>()
                        .map_err(|_| Error::invalid_params("Invalid nodeId"))?
                } else if let Some(uri) = params.get("uri").and_then(|v| v.as_str()) {
                    let line = params
                        .get("line")
                        .and_then(|v| v.as_u64())
                        .map(|v| v as u32)
                        .unwrap_or(0);
                    let path = uri.strip_prefix("file://").unwrap_or(uri);
                    let graph = self.graph.read().await;
                    let mut best: Option<(codegraph::NodeId, u32)> = None;
                    for (nid, node) in graph.iter_nodes() {
                        if node.node_type != codegraph::NodeType::Function {
                            continue;
                        }
                        let node_path = crate::domain::node_props::path(node);
                        if !node_path.ends_with(path) && !path.ends_with(&node_path) {
                            continue;
                        }
                        let ls = crate::domain::node_props::line_start(node);
                        let le = crate::domain::node_props::line_end(node);
                        if line >= ls && line <= le {
                            let span = le - ls;
                            if best.is_none() || span < best.unwrap().1 {
                                best = Some((nid, span));
                            }
                        }
                    }
                    best.map(|(id, _)| id)
                        .ok_or_else(|| Error::invalid_params("Could not find symbol at location"))?
                } else {
                    return Err(Error::invalid_params("Missing 'uri' or 'nodeId' parameter"));
                };

                let limit = params
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize)
                    .unwrap_or(10);

                let result = self.query_engine.find_similar(node_id, limit).await;

                serde_json::to_value(result).map_err(|_| Error::internal_error())
            }

            "codegraph/clusterSymbols" => {
                let threshold = params
                    .get("threshold")
                    .and_then(|v| v.as_f64())
                    .map(|v| v as f32)
                    .unwrap_or(0.7);
                let min_cluster_size = params
                    .get("minClusterSize")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize)
                    .unwrap_or(2);
                let limit = params
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize)
                    .unwrap_or(20);

                let result = self
                    .query_engine
                    .cluster_symbols(threshold, min_cluster_size, limit)
                    .await;

                serde_json::to_value(result).map_err(|_| Error::internal_error())
            }

            "codegraph/compareSymbols" => {
                let node_a = if let Some(id_str) = params.get("nodeIdA").and_then(|v| v.as_str()) {
                    id_str.parse::<codegraph::NodeId>()
                        .map_err(|_| Error::invalid_params("Invalid nodeIdA"))?
                } else {
                    let uri = params.get("uriA").and_then(|v| v.as_str())
                        .ok_or_else(|| Error::invalid_params("Missing 'uriA' or 'nodeIdA'"))?;
                    let line = params.get("lineA").and_then(|v| v.as_u64()).map(|v| v as u32).unwrap_or(0);
                    let path = uri.strip_prefix("file://").unwrap_or(uri);
                    let graph = self.graph.read().await;
                    let mut best: Option<(codegraph::NodeId, u32)> = None;
                    for (nid, node) in graph.iter_nodes() {
                        if node.node_type != codegraph::NodeType::Function { continue; }
                        let np = crate::domain::node_props::path(node);
                        if !np.ends_with(path) && !path.ends_with(&np) { continue; }
                        let ls = crate::domain::node_props::line_start(node);
                        let le = crate::domain::node_props::line_end(node);
                        if line >= ls && line <= le {
                            let span = le - ls;
                            if best.is_none() || span < best.unwrap().1 { best = Some((nid, span)); }
                        }
                    }
                    best.map(|(id, _)| id)
                        .ok_or_else(|| Error::invalid_params("Could not find symbol A"))?
                };

                let node_b = if let Some(id_str) = params.get("nodeIdB").and_then(|v| v.as_str()) {
                    id_str.parse::<codegraph::NodeId>()
                        .map_err(|_| Error::invalid_params("Invalid nodeIdB"))?
                } else {
                    let uri = params.get("uriB").and_then(|v| v.as_str())
                        .ok_or_else(|| Error::invalid_params("Missing 'uriB' or 'nodeIdB'"))?;
                    let line = params.get("lineB").and_then(|v| v.as_u64()).map(|v| v as u32).unwrap_or(0);
                    let path = uri.strip_prefix("file://").unwrap_or(uri);
                    let graph = self.graph.read().await;
                    let mut best: Option<(codegraph::NodeId, u32)> = None;
                    for (nid, node) in graph.iter_nodes() {
                        if node.node_type != codegraph::NodeType::Function { continue; }
                        let np = crate::domain::node_props::path(node);
                        if !np.ends_with(path) && !path.ends_with(&np) { continue; }
                        let ls = crate::domain::node_props::line_start(node);
                        let le = crate::domain::node_props::line_end(node);
                        if line >= ls && line <= le {
                            let span = le - ls;
                            if best.is_none() || span < best.unwrap().1 { best = Some((nid, span)); }
                        }
                    }
                    best.map(|(id, _)| id)
                        .ok_or_else(|| Error::invalid_params("Could not find symbol B"))?
                };

                let result = self.query_engine.compare_symbols(node_a, node_b).await
                    .ok_or_else(|| Error::invalid_params("Could not compare symbols"))?;

                serde_json::to_value(result).map_err(|_| Error::internal_error())
            }

            "codegraph/indexDirectory" => self.handle_index_directory(params).await,

            "codegraph/updateConfiguration" => self.handle_update_configuration(params).await,

            _ => Err(Error::method_not_found()),
        }
    }

    /// Handle reindex workspace request
    async fn handle_reindex_workspace(&self) -> Result<usize> {
        tracing::info!("Reindexing workspace");

        // Clear current graph and indexes
        {
            let mut graph = self.graph.write().await;
            *graph = codegraph::CodeGraph::in_memory().expect("Failed to create in-memory graph");
        }
        self.symbol_index.clear();
        self.file_cache.clear();

        self.client
            .log_message(
                tower_lsp::lsp_types::MessageType::INFO,
                "Clearing indexes...",
            )
            .await;

        // Re-index all workspace folders
        let folders = self.workspace_folders.read().await.clone();
        let mut total_indexed = 0;

        for folder in &folders {
            let count = self.index_directory(folder).await;
            total_indexed += count;
            self.client
                .log_message(
                    tower_lsp::lsp_types::MessageType::INFO,
                    format!("Reindexed {} files from {}", count, folder.display()),
                )
                .await;
        }

        // Resolve cross-file imports after all files are indexed
        {
            let mut graph = self.graph.write().await;
            GraphUpdater::resolve_cross_file_imports(&mut graph);
        }

        // Rebuild AI query engine indexes
        self.query_engine.build_indexes().await;

        self.client
            .log_message(
                tower_lsp::lsp_types::MessageType::INFO,
                format!("Workspace reindexed: {total_indexed} files"),
            )
            .await;

        Ok(total_indexed)
    }

    async fn handle_index_directory(&self, params: Value) -> Result<Value> {
        let paths: Vec<String> = params
            .get("paths")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        if paths.is_empty() {
            return Err(Error::invalid_params("Missing or empty 'paths' array"));
        }

        tracing::info!("Indexing directories: {:?}", paths);
        let mut total_indexed = 0;

        for path_str in &paths {
            let path = std::path::PathBuf::from(path_str);
            if !path.is_dir() {
                tracing::warn!("Skipping non-directory path: {}", path_str);
                continue;
            }
            let count = self.index_directory(&path).await;
            total_indexed += count;
            self.client
                .log_message(
                    tower_lsp::lsp_types::MessageType::INFO,
                    format!("Indexed {} files from {}", count, path.display()),
                )
                .await;
        }

        // Resolve cross-file imports after all files are indexed
        {
            let mut graph = self.graph.write().await;
            GraphUpdater::resolve_cross_file_imports(&mut graph);
        }

        // Rebuild AI query engine indexes
        self.query_engine.build_indexes().await;

        // Start or extend file watcher for the newly indexed directories
        let indexed_paths: Vec<std::path::PathBuf> =
            paths.iter().map(std::path::PathBuf::from).collect();
        self.watch_directories(&indexed_paths).await;

        self.client
            .log_message(
                tower_lsp::lsp_types::MessageType::INFO,
                format!(
                    "Index complete: {total_indexed} files from {} directories (watching for changes)",
                    paths.len()
                ),
            )
            .await;

        Ok(serde_json::json!({ "indexed": total_indexed }))
    }

    async fn handle_update_configuration(&self, params: Value) -> Result<Value> {
        use crate::backend::CodeGraphConfig;

        let new_config: CodeGraphConfig = serde_json::from_value(params)
            .map_err(|e| Error::invalid_params(format!("Invalid configuration: {e}")))?;

        tracing::info!("Updating configuration: {:?}", new_config);

        {
            let mut config = self.config.write().await;
            *config = new_config;
        }

        self.client
            .log_message(
                tower_lsp::lsp_types::MessageType::INFO,
                "Configuration updated".to_string(),
            )
            .await;

        Ok(Value::Null)
    }
}
