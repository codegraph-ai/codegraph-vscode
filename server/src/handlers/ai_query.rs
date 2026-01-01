//! AI Agent Query Handlers
//!
//! This module implements the LSP handlers for AI agent query primitives.
//! These are exposed via workspace/executeCommand and provide fast,
//! composable query primitives for AI agents to explore codebases.

use crate::ai_query::{
    EntryType, ImportMatchMode, ImportSearchOptions, SearchOptions, SignaturePattern, SymbolType,
    TraversalDirection, TraversalFilter,
};
use crate::backend::CodeGraphBackend;
use codegraph::NodeId;
use serde::{Deserialize, Serialize};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::Url;

// ==========================================
// Symbol Search Request
// ==========================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolSearchParams {
    /// Search query keywords
    pub query: String,
    /// Search scope: "workspace", "module", or "file"
    #[serde(default)]
    pub scope: Option<String>,
    /// Filter by symbol types: "function", "class", "variable", "module", "interface", "type"
    #[serde(default)]
    pub symbol_types: Option<Vec<String>>,
    /// Maximum number of results
    #[serde(default)]
    pub limit: Option<usize>,
    /// Include private symbols (default: false)
    #[serde(default)]
    pub include_private: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolSearchResult {
    pub results: Vec<SymbolMatchResponse>,
    pub total_matches: usize,
    pub query_time_ms: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolMatchResponse {
    pub node_id: String,
    pub symbol: SymbolInfoResponse,
    pub score: f32,
    pub match_reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolInfoResponse {
    pub name: String,
    pub kind: String,
    pub location: SymbolLocationResponse,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docstring: Option<String>,
    pub is_public: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolLocationResponse {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

// ==========================================
// Find By Imports Request
// ==========================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindByImportsParams {
    /// Libraries/modules to search for
    pub libraries: Vec<String>,
    /// Match mode: "exact", "prefix", or "fuzzy"
    #[serde(default)]
    pub match_mode: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FindByImportsResponse {
    pub results: Vec<SymbolMatchResponse>,
    pub query_time_ms: u64,
}

// ==========================================
// Find Entry Points Request
// ==========================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindEntryPointsParams {
    /// Entry type: "http_handler", "cli_command", "public_api", "event_handler", "test_entry", "main"
    #[serde(default)]
    pub entry_type: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FindEntryPointsResponse {
    pub entry_points: Vec<EntryPointResponse>,
    pub total_found: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryPointResponse {
    pub node_id: String,
    pub entry_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub symbol: SymbolInfoResponse,
}

// ==========================================
// Traverse Graph Request
// ==========================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraverseGraphParams {
    /// Starting point: either node_id or uri+line
    #[serde(default)]
    pub start_node_id: Option<String>,
    #[serde(default)]
    pub uri: Option<String>,
    #[serde(default)]
    pub line: Option<u32>,
    /// Traversal direction: "outgoing", "incoming", or "both"
    #[serde(default)]
    pub direction: Option<String>,
    /// Maximum traversal depth
    #[serde(default)]
    pub depth: Option<u32>,
    /// Filter by symbol types
    #[serde(default)]
    pub filter_symbol_types: Option<Vec<String>>,
    /// Maximum nodes to return
    #[serde(default)]
    pub max_nodes: Option<usize>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraverseGraphResponse {
    pub nodes: Vec<TraversalNodeResponse>,
    pub query_time_ms: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraversalNodeResponse {
    pub node_id: String,
    pub depth: u32,
    pub path: Vec<String>,
    pub edge_type: String,
    pub symbol: SymbolInfoResponse,
}

// ==========================================
// Get Callers/Callees Request
// ==========================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetCallersParams {
    /// Node ID or uri+line
    #[serde(default)]
    pub node_id: Option<String>,
    #[serde(default)]
    pub uri: Option<String>,
    #[serde(default)]
    pub line: Option<u32>,
    /// Depth of caller chain (default: 1)
    #[serde(default)]
    pub depth: Option<u32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetCallersResponse {
    pub callers: Vec<CallInfoResponse>,
    pub query_time_ms: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CallInfoResponse {
    pub node_id: String,
    pub symbol: SymbolInfoResponse,
    pub call_site: SymbolLocationResponse,
    pub depth: u32,
}

// ==========================================
// Get Detailed Symbol Info Request
// ==========================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetDetailedInfoParams {
    #[serde(default)]
    pub node_id: Option<String>,
    #[serde(default)]
    pub uri: Option<String>,
    #[serde(default)]
    pub line: Option<u32>,
    #[serde(default)]
    pub include_callers: Option<bool>,
    #[serde(default)]
    pub include_callees: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetailedSymbolResponse {
    pub symbol: SymbolInfoResponse,
    pub callers: Vec<CallInfoResponse>,
    pub callees: Vec<CallInfoResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub complexity: Option<u32>,
    pub lines_of_code: usize,
    pub is_public: bool,
    pub is_deprecated: bool,
    pub reference_count: usize,
}

// ==========================================
// Find By Signature Request
// ==========================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindBySignatureParams {
    /// Regex pattern for function name
    #[serde(default)]
    pub name_pattern: Option<String>,
    /// Expected return type
    #[serde(default)]
    pub return_type: Option<String>,
    /// Parameter count range: { min, max }
    #[serde(default)]
    pub param_count: Option<ParamCountRange>,
    /// Required modifiers: ["async", "public", "static", "const"]
    #[serde(default)]
    pub modifiers: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParamCountRange {
    pub min: usize,
    pub max: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FindBySignatureResponse {
    pub results: Vec<SymbolMatchResponse>,
    pub query_time_ms: u64,
}

// ==========================================
// Handler Implementations
// ==========================================

impl CodeGraphBackend {
    /// Handle symbol search request
    pub async fn handle_symbol_search(
        &self,
        params: SymbolSearchParams,
    ) -> Result<SymbolSearchResult> {
        // Build search options
        let mut options = SearchOptions::new();

        if let Some(limit) = params.limit {
            options = options.with_limit(limit);
        }

        if params.include_private.unwrap_or(false) {
            options = options.include_private();
        }

        if let Some(types) = params.symbol_types {
            let symbol_types: Vec<SymbolType> = types
                .iter()
                .filter_map(|t| match t.as_str() {
                    "function" => Some(SymbolType::Function),
                    "class" => Some(SymbolType::Class),
                    "variable" => Some(SymbolType::Variable),
                    "module" => Some(SymbolType::Module),
                    "interface" => Some(SymbolType::Interface),
                    "type" => Some(SymbolType::Type),
                    _ => None,
                })
                .collect();
            options = options.with_symbol_types(symbol_types);
        }

        let result = self
            .query_engine
            .symbol_search(&params.query, &options)
            .await;

        let results = result
            .results
            .into_iter()
            .map(|m| SymbolMatchResponse {
                node_id: m.node_id.to_string(),
                symbol: symbol_info_to_response(&m.symbol),
                score: m.score,
                match_reason: m.match_reason,
            })
            .collect();

        Ok(SymbolSearchResult {
            results,
            total_matches: result.total_matches,
            query_time_ms: result.query_time_ms,
        })
    }

    /// Handle find by imports request
    pub async fn handle_find_by_imports(
        &self,
        params: FindByImportsParams,
    ) -> Result<FindByImportsResponse> {
        let start = std::time::Instant::now();

        let match_mode = match params.match_mode.as_deref() {
            Some("prefix") => ImportMatchMode::Prefix,
            Some("fuzzy") => ImportMatchMode::Fuzzy,
            _ => ImportMatchMode::Exact,
        };

        let options = ImportSearchOptions::new().with_match_mode(match_mode);

        // Search for each library
        let mut all_results = Vec::new();
        for library in &params.libraries {
            let results = self.query_engine.find_by_imports(library, &options).await;
            all_results.extend(results);
        }

        let results = all_results
            .into_iter()
            .map(|m| SymbolMatchResponse {
                node_id: m.node_id.to_string(),
                symbol: symbol_info_to_response(&m.symbol),
                score: m.score,
                match_reason: m.match_reason,
            })
            .collect();

        Ok(FindByImportsResponse {
            results,
            query_time_ms: start.elapsed().as_millis() as u64,
        })
    }

    /// Handle find entry points request
    pub async fn handle_find_entry_points(
        &self,
        params: FindEntryPointsParams,
    ) -> Result<FindEntryPointsResponse> {
        let entry_types: Vec<EntryType> = if let Some(et) = params.entry_type {
            match et.as_str() {
                "http_handler" => vec![EntryType::HttpHandler],
                "cli_command" => vec![EntryType::CliCommand],
                "public_api" => vec![EntryType::PublicApi],
                "event_handler" => vec![EntryType::EventHandler],
                "test_entry" => vec![EntryType::TestEntry],
                "main" => vec![EntryType::Main],
                _ => vec![],
            }
        } else {
            vec![]
        };

        let results = self.query_engine.find_entry_points(&entry_types).await;
        let total_found = results.len();

        let entry_points = results
            .into_iter()
            .map(|ep| EntryPointResponse {
                node_id: ep.node_id.to_string(),
                entry_type: format!("{:?}", ep.entry_type).to_lowercase(),
                route: ep.route,
                method: ep.method,
                description: ep.description,
                symbol: symbol_info_to_response(&ep.symbol),
            })
            .collect();

        Ok(FindEntryPointsResponse {
            entry_points,
            total_found,
        })
    }

    /// Handle traverse graph request
    pub async fn handle_traverse_graph(
        &self,
        params: TraverseGraphParams,
    ) -> Result<TraverseGraphResponse> {
        let start = std::time::Instant::now();

        // Resolve start node
        let start_node = self
            .resolve_node_id(&params.start_node_id, &params.uri, &params.line)
            .await?;

        let direction = match params.direction.as_deref() {
            Some("incoming") => TraversalDirection::Incoming,
            Some("both") => TraversalDirection::Both,
            _ => TraversalDirection::Outgoing,
        };

        let depth = params.depth.unwrap_or(3);

        let mut filter = TraversalFilter::new();
        if let Some(max) = params.max_nodes {
            filter = filter.with_max_nodes(max);
        }
        if let Some(types) = params.filter_symbol_types {
            let symbol_types: Vec<SymbolType> = types
                .iter()
                .filter_map(|t| match t.as_str() {
                    "function" => Some(SymbolType::Function),
                    "class" => Some(SymbolType::Class),
                    "variable" => Some(SymbolType::Variable),
                    "module" => Some(SymbolType::Module),
                    "interface" => Some(SymbolType::Interface),
                    "type" => Some(SymbolType::Type),
                    _ => None,
                })
                .collect();
            filter = filter.with_symbol_types(symbol_types);
        }

        let results = self
            .query_engine
            .traverse_graph(start_node, direction, depth, &filter)
            .await;

        let nodes = results
            .into_iter()
            .map(|n| TraversalNodeResponse {
                node_id: n.node_id.to_string(),
                depth: n.depth,
                path: n.path.iter().map(|id| id.to_string()).collect(),
                edge_type: n.edge_type,
                symbol: symbol_info_to_response(&n.symbol),
            })
            .collect();

        Ok(TraverseGraphResponse {
            nodes,
            query_time_ms: start.elapsed().as_millis() as u64,
        })
    }

    /// Handle get callers request
    pub async fn handle_get_callers(&self, params: GetCallersParams) -> Result<GetCallersResponse> {
        let start = std::time::Instant::now();

        let node_id = self
            .resolve_node_id(&params.node_id, &params.uri, &params.line)
            .await?;
        let depth = params.depth.unwrap_or(1);

        let results = self.query_engine.get_callers(node_id, depth).await;

        let callers = results
            .into_iter()
            .map(|c| CallInfoResponse {
                node_id: c.node_id.to_string(),
                symbol: symbol_info_to_response(&c.symbol),
                call_site: SymbolLocationResponse {
                    file: c.call_site.file,
                    line: c.call_site.line,
                    column: c.call_site.column,
                    end_line: c.call_site.end_line,
                    end_column: c.call_site.end_column,
                },
                depth: c.depth,
            })
            .collect();

        Ok(GetCallersResponse {
            callers,
            query_time_ms: start.elapsed().as_millis() as u64,
        })
    }

    /// Handle get callees request
    pub async fn handle_get_callees(&self, params: GetCallersParams) -> Result<GetCallersResponse> {
        let start = std::time::Instant::now();

        let node_id = self
            .resolve_node_id(&params.node_id, &params.uri, &params.line)
            .await?;
        let depth = params.depth.unwrap_or(1);

        let results = self.query_engine.get_callees(node_id, depth).await;

        let callers = results
            .into_iter()
            .map(|c| CallInfoResponse {
                node_id: c.node_id.to_string(),
                symbol: symbol_info_to_response(&c.symbol),
                call_site: SymbolLocationResponse {
                    file: c.call_site.file,
                    line: c.call_site.line,
                    column: c.call_site.column,
                    end_line: c.call_site.end_line,
                    end_column: c.call_site.end_column,
                },
                depth: c.depth,
            })
            .collect();

        Ok(GetCallersResponse {
            callers,
            query_time_ms: start.elapsed().as_millis() as u64,
        })
    }

    /// Handle get detailed info request
    pub async fn handle_get_detailed_symbol_info(
        &self,
        params: GetDetailedInfoParams,
    ) -> Result<DetailedSymbolResponse> {
        let node_id = self
            .resolve_node_id(&params.node_id, &params.uri, &params.line)
            .await?;

        let info = self
            .query_engine
            .get_symbol_info(node_id)
            .await
            .ok_or_else(|| tower_lsp::jsonrpc::Error::invalid_params("Symbol not found"))?;

        let include_callers = params.include_callers.unwrap_or(true);
        let include_callees = params.include_callees.unwrap_or(true);

        let callers = if include_callers {
            info.callers
                .into_iter()
                .map(|c| CallInfoResponse {
                    node_id: c.node_id.to_string(),
                    symbol: symbol_info_to_response(&c.symbol),
                    call_site: SymbolLocationResponse {
                        file: c.call_site.file,
                        line: c.call_site.line,
                        column: c.call_site.column,
                        end_line: c.call_site.end_line,
                        end_column: c.call_site.end_column,
                    },
                    depth: c.depth,
                })
                .collect()
        } else {
            Vec::new()
        };

        let callees = if include_callees {
            info.callees
                .into_iter()
                .map(|c| CallInfoResponse {
                    node_id: c.node_id.to_string(),
                    symbol: symbol_info_to_response(&c.symbol),
                    call_site: SymbolLocationResponse {
                        file: c.call_site.file,
                        line: c.call_site.line,
                        column: c.call_site.column,
                        end_line: c.call_site.end_line,
                        end_column: c.call_site.end_column,
                    },
                    depth: c.depth,
                })
                .collect()
        } else {
            Vec::new()
        };

        Ok(DetailedSymbolResponse {
            symbol: symbol_info_to_response(&info.symbol),
            callers,
            callees,
            complexity: info.complexity,
            lines_of_code: info.lines_of_code,
            is_public: info.is_public,
            is_deprecated: info.is_deprecated,
            reference_count: info.reference_count,
        })
    }

    /// Handle find by signature request
    pub async fn handle_find_by_signature(
        &self,
        params: FindBySignatureParams,
    ) -> Result<FindBySignatureResponse> {
        let start = std::time::Instant::now();

        // Build signature pattern from params
        let pattern = SignaturePattern {
            name_pattern: params.name_pattern,
            return_type: params.return_type,
            param_count: params.param_count.map(|r| (r.min, r.max)),
            modifiers: params.modifiers.unwrap_or_default(),
        };

        let results = self.query_engine.find_by_signature(&pattern).await;

        let response_results = results
            .into_iter()
            .map(|m| SymbolMatchResponse {
                node_id: m.node_id.to_string(),
                symbol: symbol_info_to_response(&m.symbol),
                score: m.score,
                match_reason: m.match_reason,
            })
            .collect();

        Ok(FindBySignatureResponse {
            results: response_results,
            query_time_ms: start.elapsed().as_millis() as u64,
        })
    }

    /// Helper to resolve a node ID from either direct ID or uri+line
    async fn resolve_node_id(
        &self,
        node_id: &Option<String>,
        uri: &Option<String>,
        line: &Option<u32>,
    ) -> Result<NodeId> {
        if let Some(id_str) = node_id {
            return id_str
                .parse::<NodeId>()
                .map_err(|_| tower_lsp::jsonrpc::Error::invalid_params("Invalid node ID"));
        }

        let uri_str = uri.as_ref().ok_or_else(|| {
            tower_lsp::jsonrpc::Error::invalid_params("Must provide node_id or uri+line")
        })?;

        let url = Url::parse(uri_str)
            .map_err(|_| tower_lsp::jsonrpc::Error::invalid_params("Invalid URI"))?;

        let path = url
            .to_file_path()
            .map_err(|_| tower_lsp::jsonrpc::Error::invalid_params("Invalid file path"))?;

        let line_num = line.unwrap_or(1);
        let position = tower_lsp::lsp_types::Position {
            line: line_num.saturating_sub(1), // LSP is 0-indexed
            character: 0,
        };

        let graph = self.graph.read().await;
        self.find_node_at_position(&graph, &path, position)?
            .ok_or_else(|| tower_lsp::jsonrpc::Error::invalid_params("No symbol found at position"))
    }
}

/// Helper to convert internal SymbolInfo to response format
fn symbol_info_to_response(info: &crate::ai_query::SymbolInfo) -> SymbolInfoResponse {
    SymbolInfoResponse {
        name: info.name.clone(),
        kind: info.kind.clone(),
        location: SymbolLocationResponse {
            file: info.location.file.clone(),
            line: info.location.line,
            column: info.location.column,
            end_line: info.location.end_line,
            end_column: info.location.end_column,
        },
        signature: info.signature.clone(),
        docstring: info.docstring.clone(),
        is_public: info.is_public,
    }
}
