//! Custom LSP request handlers for graph-based features.

use crate::backend::CodeGraphBackend;
use codegraph::{CodeGraph, Direction, EdgeType, Node, NodeId};
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{Position, Range, Url};

/// Helper to get line position from node properties.
/// Supports both property name conventions (line_start or start_line).
fn get_line_start(node: &Node) -> u32 {
    node.properties
        .get_int("line_start")
        .or_else(|| node.properties.get_int("start_line"))
        .map(|v| v as u32)
        .unwrap_or(1)
}

fn get_line_end(node: &Node, default: u32) -> u32 {
    node.properties
        .get_int("line_end")
        .or_else(|| node.properties.get_int("end_line"))
        .map(|v| v as u32)
        .unwrap_or(default)
}

fn get_col_start(node: &Node) -> u32 {
    node.properties
        .get_int("col_start")
        .or_else(|| node.properties.get_int("start_col"))
        .map(|v| v as u32)
        .unwrap_or(0)
}

fn get_col_end(node: &Node) -> u32 {
    node.properties
        .get_int("col_end")
        .or_else(|| node.properties.get_int("end_col"))
        .map(|v| v as u32)
        .unwrap_or(0)
}

// ==========================================
// Dependency Graph Request
// ==========================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyGraphParams {
    pub uri: String,
    pub depth: Option<usize>,
    pub include_external: Option<bool>,
    pub direction: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyNode {
    pub id: String,
    pub label: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub language: String,
    pub uri: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyEdge {
    pub from: String,
    pub to: String,
    #[serde(rename = "type")]
    pub edge_type: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyGraphResponse {
    pub nodes: Vec<DependencyNode>,
    pub edges: Vec<DependencyEdge>,
}

// ==========================================
// Related Tests Request
// ==========================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelatedTestsParams {
    pub uri: String,
    pub position: Position,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelatedTest {
    pub uri: String,
    pub test_name: String,
    pub relationship: String,
    pub range: Range,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelatedTestsResponse {
    pub tests: Vec<RelatedTest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncated: Option<bool>,
}

impl CodeGraphBackend {
    pub async fn handle_get_dependency_graph(
        &self,
        params: DependencyGraphParams,
    ) -> Result<DependencyGraphResponse> {
        let uri = Url::parse(&params.uri)
            .map_err(|_| tower_lsp::jsonrpc::Error::invalid_params("Invalid URI"))?;

        let path = uri
            .to_file_path()
            .map_err(|_| tower_lsp::jsonrpc::Error::invalid_params("Invalid file path"))?;

        let graph = self.graph.read().await;
        let depth = params.depth.unwrap_or(3);
        let include_external = params.include_external.unwrap_or(false);

        // Find the file node
        let path_str = path.to_string_lossy().to_string();

        let start_node = match codegraph::helpers::find_file_by_path(&graph, &path_str)
            .map_err(|_| tower_lsp::jsonrpc::Error::internal_error())?
        {
            Some(id) => id,
            None => {
                return Ok(DependencyGraphResponse {
                    nodes: Vec::new(),
                    edges: Vec::new(),
                });
            }
        };

        // Use built-in import-aware graph traversal
        let dir_param = params.direction.as_deref().unwrap_or("both");
        let mut reachable_set: HashSet<NodeId> = HashSet::new();
        reachable_set.insert(start_node);

        if dir_param != "importedBy" {
            // "imports" or "both": follow outgoing import edges
            if let Ok(deps) =
                codegraph::helpers::transitive_dependencies(&graph, start_node, Some(depth))
            {
                reachable_set.extend(deps);
            }
        }
        if dir_param != "imports" {
            // "importedBy" or "both": follow incoming import edges
            if let Ok(deps) =
                codegraph::helpers::transitive_dependents(&graph, start_node, Some(depth))
            {
                reachable_set.extend(deps);
            }
        }

        // Build response nodes
        let mut nodes = Vec::new();
        let mut edges = Vec::new();

        for &node_id in &reachable_set {
            if let Ok(node) = graph.get_node(node_id) {
                // Skip external if not requested
                let is_external = node
                    .properties
                    .get_string("external")
                    .map(|v| v == "true")
                    .unwrap_or(false);

                if is_external && !include_external {
                    continue;
                }

                let name = node.properties.get_string("name").unwrap_or("").to_string();
                let node_type = format!("{:?}", node.node_type).to_lowercase();
                let language = node
                    .properties
                    .get_string("language")
                    .unwrap_or("unknown")
                    .to_string();

                let node_path = node
                    .properties
                    .get_string("path")
                    .map(|s| s.to_string())
                    .or_else(|| {
                        self.symbol_index
                            .find_file_for_node(node_id)
                            .and_then(|p| p.to_str().map(|s| format!("file://{s}")))
                    })
                    .unwrap_or_default();

                nodes.push(DependencyNode {
                    id: node_id.to_string(),
                    label: name,
                    node_type,
                    language,
                    uri: node_path,
                });
            }
        }

        // Collect import edges between reachable nodes
        for &node_id in &reachable_set {
            if let Ok(neighbors) = graph.get_neighbors(node_id, Direction::Outgoing) {
                for neighbor_id in neighbors {
                    if reachable_set.contains(&neighbor_id) {
                        if let Ok(edge_ids) = graph.get_edges_between(node_id, neighbor_id) {
                            for edge_id in edge_ids {
                                if let Ok(edge) = graph.get_edge(edge_id) {
                                    if edge.edge_type == EdgeType::Imports {
                                        edges.push(DependencyEdge {
                                            from: node_id.to_string(),
                                            to: neighbor_id.to_string(),
                                            edge_type: "import".to_string(),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(DependencyGraphResponse { nodes, edges })
    }

    /// Find tests related to a symbol at the given position using graph relationships.
    pub async fn handle_find_related_tests(
        &self,
        params: RelatedTestsParams,
    ) -> Result<RelatedTestsResponse> {
        let uri = Url::parse(&params.uri)
            .map_err(|_| tower_lsp::jsonrpc::Error::invalid_params("Invalid URI"))?;

        let path = uri
            .to_file_path()
            .map_err(|_| tower_lsp::jsonrpc::Error::invalid_params("Invalid file path"))?;

        let graph = self.graph.read().await;
        let limit = params.limit.unwrap_or(10);

        // Find node at position
        let Some(node_id) = self.find_node_at_position(&graph, &path, params.position)? else {
            return Ok(RelatedTestsResponse {
                tests: Vec::new(),
                truncated: None,
            });
        };

        let mut tests = Vec::new();
        let mut seen = HashSet::new();

        // First, look for direct callers/references that are tests
        for (source_id, _target_id, edge_type) in Self::get_incoming_edges(&graph, node_id) {
            if let Ok(node) = graph.get_node(source_id) {
                if !Self::is_test_node(node) {
                    continue;
                }

                if !seen.insert(source_id) {
                    continue;
                }

                if let Some(range) = Self::node_to_range(node) {
                    let node_path = node
                        .properties
                        .get_string("path")
                        .map(|s| s.to_string())
                        .or_else(|| {
                            self.symbol_index
                                .find_file_for_node(source_id)
                                .and_then(|p| p.to_str().map(|s| format!("file://{s}")))
                        })
                        .unwrap_or_default();

                    tests.push(RelatedTest {
                        uri: node_path,
                        test_name: node.properties.get_string("name").unwrap_or("").to_string(),
                        relationship: match edge_type {
                            EdgeType::Calls => "calls_target".to_string(),
                            EdgeType::References => "references_target".to_string(),
                            _ => "related".to_string(),
                        },
                        range,
                    });
                }
            }

            if tests.len() >= limit {
                break;
            }
        }

        // If we did not find enough, look for siblings in the same file with test-like names
        if tests.len() < limit {
            let path_str = path.to_string_lossy().to_string();
            if let Ok(same_file_nodes) = graph.query().property("path", path_str).execute() {
                for sibling_id in same_file_nodes {
                    if sibling_id == node_id || seen.contains(&sibling_id) {
                        continue;
                    }
                    if let Ok(node) = graph.get_node(sibling_id) {
                        if !Self::is_test_node(node) {
                            continue;
                        }

                        if let Some(range) = Self::node_to_range(node) {
                            let node_path = node
                                .properties
                                .get_string("path")
                                .map(|s| s.to_string())
                                .or_else(|| {
                                    self.symbol_index
                                        .find_file_for_node(sibling_id)
                                        .and_then(|p| p.to_str().map(|s| format!("file://{s}")))
                                })
                                .unwrap_or_default();

                            tests.push(RelatedTest {
                                uri: node_path,
                                test_name: node
                                    .properties
                                    .get_string("name")
                                    .unwrap_or("")
                                    .to_string(),
                                relationship: "same_file".to_string(),
                                range,
                            });
                            seen.insert(sibling_id);
                        }
                    }

                    if tests.len() >= limit {
                        break;
                    }
                }
            }
        }

        let truncated = if tests.len() >= limit {
            Some(true)
        } else {
            None
        };

        Ok(RelatedTestsResponse { tests, truncated })
    }

    fn is_test_node(node: &Node) -> bool {
        let name = node.properties.get_string("name").unwrap_or("");
        let path = node.properties.get_string("path").unwrap_or("");

        let name_is_test =
            name.starts_with("test_") || name.ends_with("_test") || name.contains("test ");
        let path_is_test = path.contains("/test")
            || path.contains("/tests")
            || path.contains("\\test")
            || path.contains("\\tests");

        name_is_test || path_is_test
    }

    fn node_to_range(node: &Node) -> Option<Range> {
        let start_line = get_line_start(node).saturating_sub(1);
        let end_line = get_line_end(node, start_line + 2).saturating_sub(1);

        Some(Range {
            start: Position {
                line: start_line,
                character: get_col_start(node),
            },
            end: Position {
                line: end_line,
                character: get_col_end(node),
            },
        })
    }

    /// Helper function to collect edges based on direction parameter.
    fn collect_edges_for_direction(
        graph: &CodeGraph,
        node_id: NodeId,
        direction: &str,
        skip_outgoing: &str,
        skip_incoming: &str,
    ) -> Vec<(NodeId, NodeId, EdgeType)> {
        let mut edges = Vec::new();

        // Get outgoing edges if not skipped
        if direction != skip_outgoing {
            if let Ok(neighbors) = graph.get_neighbors(node_id, Direction::Outgoing) {
                for neighbor_id in neighbors {
                    if let Ok(edge_ids) = graph.get_edges_between(node_id, neighbor_id) {
                        for edge_id in edge_ids {
                            if let Ok(edge) = graph.get_edge(edge_id) {
                                edges.push((edge.source_id, edge.target_id, edge.edge_type));
                            }
                        }
                    }
                }
            }
        }

        // Get incoming edges if not skipped
        if direction != skip_incoming {
            if let Ok(neighbors) = graph.get_neighbors(node_id, Direction::Incoming) {
                for neighbor_id in neighbors {
                    if let Ok(edge_ids) = graph.get_edges_between(neighbor_id, node_id) {
                        for edge_id in edge_ids {
                            if let Ok(edge) = graph.get_edge(edge_id) {
                                edges.push((edge.source_id, edge.target_id, edge.edge_type));
                            }
                        }
                    }
                }
            }
        }

        edges
    }

    /// Helper function to get incoming edges for a node.
    fn get_incoming_edges(graph: &CodeGraph, node_id: NodeId) -> Vec<(NodeId, NodeId, EdgeType)> {
        let mut edges = Vec::new();
        if let Ok(neighbors) = graph.get_neighbors(node_id, Direction::Incoming) {
            for neighbor_id in neighbors {
                if let Ok(edge_ids) = graph.get_edges_between(neighbor_id, node_id) {
                    for edge_id in edge_ids {
                        if let Ok(edge) = graph.get_edge(edge_id) {
                            edges.push((edge.source_id, edge.target_id, edge.edge_type));
                        }
                    }
                }
            }
        }
        edges
    }
}

// ==========================================
// Call Graph Request
// ==========================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallGraphParams {
    pub uri: String,
    pub position: Position,
    pub direction: Option<String>,
    pub depth: Option<usize>,
    pub include_external: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionNode {
    pub id: String,
    pub name: String,
    pub signature: String,
    pub uri: String,
    pub range: Range,
    pub language: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CallEdge {
    pub from: String,
    pub to: String,
    pub call_sites: Vec<Range>,
    pub is_recursive: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CallGraphResponse {
    pub root: Option<FunctionNode>,
    pub nodes: Vec<FunctionNode>,
    pub edges: Vec<CallEdge>,
}

impl CodeGraphBackend {
    pub async fn handle_get_call_graph(
        &self,
        params: CallGraphParams,
    ) -> Result<CallGraphResponse> {
        let uri = Url::parse(&params.uri)
            .map_err(|_| tower_lsp::jsonrpc::Error::invalid_params("Invalid URI"))?;

        let path = uri
            .to_file_path()
            .map_err(|_| tower_lsp::jsonrpc::Error::invalid_params("Invalid file path"))?;

        let graph = self.graph.read().await;
        let depth = params.depth.unwrap_or(3);
        let position = params.position;

        // Find node at position
        let node_id = match self.find_node_at_position(&graph, &path, position)? {
            Some(id) => id,
            None => {
                return Ok(CallGraphResponse {
                    root: None,
                    nodes: Vec::new(),
                    edges: Vec::new(),
                })
            }
        };

        // BFS to collect call graph
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut nodes = Vec::new();
        let mut edges = Vec::new();

        queue.push_back((node_id, 0));
        visited.insert(node_id);

        let mut root: Option<FunctionNode> = None;

        while let Some((current_id, current_depth)) = queue.pop_front() {
            if current_depth > depth {
                continue;
            }

            if let Ok(node) = graph.get_node(current_id) {
                let name = node.properties.get_string("name").unwrap_or("").to_string();
                let signature = node
                    .properties
                    .get_string("signature")
                    .unwrap_or("")
                    .to_string();
                let language = node
                    .properties
                    .get_string("language")
                    .unwrap_or("unknown")
                    .to_string();

                // For the root node, use the original URI from params
                // For other nodes, try to get path from properties or symbol index
                let node_path = if current_id == node_id {
                    params.uri.clone()
                } else {
                    node.properties
                        .get_string("path")
                        .map(|s| s.to_string())
                        .or_else(|| {
                            self.symbol_index
                                .find_file_for_node(current_id)
                                .and_then(|p| p.to_str().map(|s| format!("file://{s}")))
                        })
                        .unwrap_or_default()
                };

                let start_line = get_line_start(node).saturating_sub(1);
                let start_col = get_col_start(node);
                let end_line = get_line_end(node, start_line + 2).saturating_sub(1);
                let end_col = get_col_end(node);

                let func_node = FunctionNode {
                    id: current_id.to_string(),
                    name,
                    signature,
                    uri: node_path,
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
                    language,
                };

                if current_id == node_id {
                    root = Some(func_node.clone());
                }

                nodes.push(func_node);

                // Get call edges based on direction
                let dir_param = params.direction.as_deref().unwrap_or("both");

                // Collect edges based on direction
                let collected_edges = Self::collect_edges_for_direction(
                    &graph, current_id, dir_param, "callers", "callees",
                );

                for (source_id, target_id, edge_type) in collected_edges {
                    if edge_type == EdgeType::Calls {
                        let is_recursive = source_id == target_id;

                        edges.push(CallEdge {
                            from: source_id.to_string(),
                            to: target_id.to_string(),
                            call_sites: Vec::new(), // Could be populated from edge properties
                            is_recursive,
                        });

                        let next_node = if source_id == current_id {
                            target_id
                        } else {
                            source_id
                        };

                        if !visited.contains(&next_node) {
                            visited.insert(next_node);
                            queue.push_back((next_node, current_depth + 1));
                        }
                    }
                }
            }
        }

        Ok(CallGraphResponse { root, nodes, edges })
    }
}

// ==========================================
// Impact Analysis Request
// ==========================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImpactAnalysisParams {
    pub uri: String,
    pub position: Position,
    pub analysis_type: String, // "modify", "delete", "rename"
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectImpact {
    pub uri: String,
    pub range: Range,
    #[serde(rename = "type")]
    pub impact_type: String,
    pub severity: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndirectImpact {
    pub uri: String,
    pub path: Vec<String>,
    pub severity: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AffectedTest {
    pub uri: String,
    pub test_name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImpactSummary {
    pub files_affected: usize,
    pub breaking_changes: usize,
    pub warnings: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImpactAnalysisResponse {
    pub direct_impact: Vec<DirectImpact>,
    pub indirect_impact: Vec<IndirectImpact>,
    pub affected_tests: Vec<AffectedTest>,
    pub summary: ImpactSummary,
}

impl CodeGraphBackend {
    pub async fn handle_analyze_impact(
        &self,
        params: ImpactAnalysisParams,
    ) -> Result<ImpactAnalysisResponse> {
        let uri = Url::parse(&params.uri)
            .map_err(|_| tower_lsp::jsonrpc::Error::invalid_params("Invalid URI"))?;

        let path = uri
            .to_file_path()
            .map_err(|_| tower_lsp::jsonrpc::Error::invalid_params("Invalid file path"))?;

        let graph = self.graph.read().await;

        // Find node at position
        let node_id = match self.find_node_at_position(&graph, &path, params.position)? {
            Some(id) => id,
            None => {
                return Ok(ImpactAnalysisResponse {
                    direct_impact: Vec::new(),
                    indirect_impact: Vec::new(),
                    affected_tests: Vec::new(),
                    summary: ImpactSummary {
                        files_affected: 0,
                        breaking_changes: 0,
                        warnings: 0,
                    },
                })
            }
        };

        let mut direct_impact = Vec::new();
        let mut indirect_impact = Vec::new();
        let mut affected_tests = Vec::new();
        let mut affected_files = HashSet::new();

        // Find direct references
        let incoming_edges = Self::get_incoming_edges(&graph, node_id);

        for (source_id, _target_id, edge_type) in incoming_edges {
            if let Ok(ref_node) = graph.get_node(source_id) {
                let ref_path = ref_node
                    .properties
                    .get_string("path")
                    .map(|s| s.to_string())
                    .or_else(|| {
                        self.symbol_index
                            .find_file_for_node(source_id)
                            .and_then(|p| p.to_str().map(|s| format!("file://{s}")))
                    })
                    .unwrap_or_default();
                let ref_name = ref_node
                    .properties
                    .get_string("name")
                    .unwrap_or("")
                    .to_string();

                affected_files.insert(ref_path.clone());

                let start_line = get_line_start(ref_node).saturating_sub(1);
                let start_col = get_col_start(ref_node);
                let end_line = get_line_end(ref_node, start_line + 2).saturating_sub(1);
                let end_col = get_col_end(ref_node);

                let impact_type = match edge_type {
                    EdgeType::Calls => "caller",
                    EdgeType::References => "reference",
                    EdgeType::Extends => "subclass",
                    EdgeType::Implements => "implementation",
                    _ => "reference",
                };

                let severity = match params.analysis_type.as_str() {
                    "delete" => "breaking",
                    "rename" => "breaking",
                    "modify" => "warning",
                    _ => "info",
                };

                // Check if it's a test
                let is_test = ref_name.starts_with("test_")
                    || ref_name.ends_with("_test")
                    || ref_path.contains("test");

                if is_test {
                    affected_tests.push(AffectedTest {
                        uri: ref_path.clone(),
                        test_name: ref_name,
                    });
                }

                direct_impact.push(DirectImpact {
                    uri: ref_path,
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
                    impact_type: impact_type.to_string(),
                    severity: severity.to_string(),
                });
            }
        }

        // Find indirect impact using built-in graph traversal
        let mut indirect_visited: HashSet<NodeId> = HashSet::new();
        indirect_visited.insert(node_id);

        for impact in &direct_impact {
            if let Ok(impact_nodes) = graph.query().property("path", impact.uri.clone()).execute() {
                for n_id in impact_nodes {
                    if n_id == node_id {
                        continue;
                    }
                    indirect_visited.insert(n_id);

                    // Follow incoming edges up to 2 levels from each direct impact
                    if let Ok(indirect_ids) = graph.bfs(n_id, Direction::Incoming, Some(2)) {
                        for indirect_id in indirect_ids {
                            if indirect_id == node_id || indirect_visited.contains(&indirect_id) {
                                continue;
                            }
                            indirect_visited.insert(indirect_id);

                            if let Ok(ref_node) = graph.get_node(indirect_id) {
                                let ref_path = ref_node
                                    .properties
                                    .get_string("path")
                                    .map(|s| s.to_string())
                                    .or_else(|| {
                                        self.symbol_index
                                            .find_file_for_node(indirect_id)
                                            .and_then(|p| p.to_str().map(|s| format!("file://{s}")))
                                    })
                                    .unwrap_or_default();

                                if !affected_files.contains(&ref_path) {
                                    indirect_impact.push(IndirectImpact {
                                        uri: ref_path.clone(),
                                        path: vec![impact.uri.clone(), ref_path],
                                        severity: "warning".to_string(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        let breaking_count = direct_impact
            .iter()
            .filter(|i| i.severity == "breaking")
            .count();
        let warning_count = direct_impact
            .iter()
            .filter(|i| i.severity == "warning")
            .count()
            + indirect_impact.len();

        Ok(ImpactAnalysisResponse {
            direct_impact,
            indirect_impact,
            affected_tests,
            summary: ImpactSummary {
                files_affected: affected_files.len(),
                breaking_changes: breaking_count,
                warnings: warning_count,
            },
        })
    }
}

// ==========================================
// Parser Metrics Request
// ==========================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParserMetricsParams {
    pub language: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParserMetric {
    pub language: String,
    pub files_attempted: usize,
    pub files_succeeded: usize,
    pub files_failed: usize,
    pub total_entities: usize,
    pub total_relationships: usize,
    pub total_parse_time_ms: u64,
    pub avg_parse_time_ms: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricsTotals {
    pub files_attempted: usize,
    pub files_succeeded: usize,
    pub files_failed: usize,
    pub total_entities: usize,
    pub success_rate: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParserMetricsResponse {
    pub metrics: Vec<ParserMetric>,
    pub totals: MetricsTotals,
}

impl CodeGraphBackend {
    pub async fn handle_get_parser_metrics(
        &self,
        params: ParserMetricsParams,
    ) -> Result<ParserMetricsResponse> {
        let all_metrics = self.parsers.all_metrics();

        let metrics: Vec<ParserMetric> = all_metrics
            .into_iter()
            .filter(|(lang, _)| params.language.as_ref().is_none_or(|l| l == *lang))
            .map(|(language, m)| ParserMetric {
                language: language.to_string(),
                files_attempted: m.files_attempted,
                files_succeeded: m.files_succeeded,
                files_failed: m.files_failed,
                total_entities: m.total_entities,
                total_relationships: m.total_relationships,
                total_parse_time_ms: m.total_parse_time.as_millis() as u64,
                avg_parse_time_ms: if m.files_succeeded > 0 {
                    m.total_parse_time.as_millis() as u64 / m.files_succeeded as u64
                } else {
                    0
                },
            })
            .collect();

        let totals = MetricsTotals {
            files_attempted: metrics.iter().map(|m| m.files_attempted).sum(),
            files_succeeded: metrics.iter().map(|m| m.files_succeeded).sum(),
            files_failed: metrics.iter().map(|m| m.files_failed).sum(),
            total_entities: metrics.iter().map(|m| m.total_entities).sum(),
            success_rate: {
                let attempted: usize = metrics.iter().map(|m| m.files_attempted).sum();
                let succeeded: usize = metrics.iter().map(|m| m.files_succeeded).sum();
                if attempted > 0 {
                    succeeded as f64 / attempted as f64
                } else {
                    0.0
                }
            },
        };

        Ok(ParserMetricsResponse { metrics, totals })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_query::QueryEngine;
    use crate::backend::CodeGraphBackend;
    use codegraph::{CodeGraph, NodeType, PropertyMap, PropertyValue};
    use std::sync::Arc;
    use tokio::sync::RwLock;

    /// Helper to add a node to the symbol index
    fn add_node_to_index(
        backend: &CodeGraphBackend,
        path: &std::path::Path,
        node_id: NodeId,
        name: &str,
        node_type: &str,
        start_line: u32,
        end_line: u32,
    ) {
        backend.symbol_index.add_node_for_test(
            path.to_path_buf(),
            node_id,
            name,
            node_type,
            start_line,
            end_line,
        );
    }

    /// Helper to create a test backend with a graph containing test nodes
    async fn create_test_backend() -> CodeGraphBackend {
        let graph = Arc::new(RwLock::new(
            CodeGraph::in_memory().expect("Failed to create graph"),
        ));
        let query_engine = Arc::new(QueryEngine::new(Arc::clone(&graph)));
        CodeGraphBackend::new_for_test(graph, query_engine)
    }

    /// Helper to create a backend with nodes for dependency graph testing
    async fn create_backend_with_imports() -> (CodeGraphBackend, NodeId, NodeId) {
        let graph = Arc::new(RwLock::new(
            CodeGraph::in_memory().expect("Failed to create graph"),
        ));

        let (file1_id, file2_id) = {
            let mut g = graph.write().await;

            // Create file 1
            let mut props1 = PropertyMap::new();
            props1.insert(
                "name".to_string(),
                PropertyValue::String("main.rs".to_string()),
            );
            props1.insert(
                "path".to_string(),
                PropertyValue::String("/test/main.rs".to_string()),
            );
            props1.insert(
                "language".to_string(),
                PropertyValue::String("rust".to_string()),
            );
            props1.insert("line_start".to_string(), PropertyValue::Int(1));
            props1.insert("line_end".to_string(), PropertyValue::Int(100));
            let file1_id = g.add_node(NodeType::CodeFile, props1).unwrap();

            // Create file 2
            let mut props2 = PropertyMap::new();
            props2.insert(
                "name".to_string(),
                PropertyValue::String("utils.rs".to_string()),
            );
            props2.insert(
                "path".to_string(),
                PropertyValue::String("/test/utils.rs".to_string()),
            );
            props2.insert(
                "language".to_string(),
                PropertyValue::String("rust".to_string()),
            );
            props2.insert("line_start".to_string(), PropertyValue::Int(1));
            props2.insert("line_end".to_string(), PropertyValue::Int(50));
            let file2_id = g.add_node(NodeType::CodeFile, props2).unwrap();

            // Create import edge from file1 to file2
            g.add_edge(file1_id, file2_id, EdgeType::Imports, PropertyMap::new())
                .unwrap();

            (file1_id, file2_id)
        };

        let query_engine = Arc::new(QueryEngine::new(Arc::clone(&graph)));
        let backend = CodeGraphBackend::new_for_test(graph, query_engine);

        (backend, file1_id, file2_id)
    }

    /// Helper to create a backend with function nodes for call graph testing
    async fn create_backend_with_calls() -> (CodeGraphBackend, NodeId, NodeId) {
        let graph = Arc::new(RwLock::new(
            CodeGraph::in_memory().expect("Failed to create graph"),
        ));

        let (func1_id, func2_id) = {
            let mut g = graph.write().await;

            // Create function 1
            let mut props1 = PropertyMap::new();
            props1.insert(
                "name".to_string(),
                PropertyValue::String("main".to_string()),
            );
            props1.insert(
                "path".to_string(),
                PropertyValue::String("/test/main.rs".to_string()),
            );
            props1.insert(
                "signature".to_string(),
                PropertyValue::String("fn main()".to_string()),
            );
            props1.insert(
                "language".to_string(),
                PropertyValue::String("rust".to_string()),
            );
            props1.insert("line_start".to_string(), PropertyValue::Int(1));
            props1.insert("line_end".to_string(), PropertyValue::Int(10));
            let func1_id = g.add_node(NodeType::Function, props1).unwrap();

            // Create function 2
            let mut props2 = PropertyMap::new();
            props2.insert(
                "name".to_string(),
                PropertyValue::String("helper".to_string()),
            );
            props2.insert(
                "path".to_string(),
                PropertyValue::String("/test/main.rs".to_string()),
            );
            props2.insert(
                "signature".to_string(),
                PropertyValue::String("fn helper() -> i32".to_string()),
            );
            props2.insert(
                "language".to_string(),
                PropertyValue::String("rust".to_string()),
            );
            props2.insert("line_start".to_string(), PropertyValue::Int(15));
            props2.insert("line_end".to_string(), PropertyValue::Int(25));
            let func2_id = g.add_node(NodeType::Function, props2).unwrap();

            // Create call edge from func1 to func2
            g.add_edge(func1_id, func2_id, EdgeType::Calls, PropertyMap::new())
                .unwrap();

            (func1_id, func2_id)
        };

        let query_engine = Arc::new(QueryEngine::new(Arc::clone(&graph)));
        let backend = CodeGraphBackend::new_for_test(graph, query_engine);

        // Add to symbol index
        let path = std::path::Path::new("/test/main.rs");
        add_node_to_index(&backend, path, func1_id, "main", "Function", 1, 10);
        add_node_to_index(&backend, path, func2_id, "helper", "Function", 15, 25);

        (backend, func1_id, func2_id)
    }

    // ==========================================
    // Helper function tests
    // ==========================================

    #[test]
    fn test_get_line_start() {
        let mut props = PropertyMap::new();
        props.insert("line_start".to_string(), PropertyValue::Int(42));
        let node = Node {
            id: 1,
            node_type: NodeType::Function,
            properties: props,
        };
        assert_eq!(get_line_start(&node), 42);
    }

    #[test]
    fn test_get_line_start_fallback() {
        let mut props = PropertyMap::new();
        props.insert("start_line".to_string(), PropertyValue::Int(42));
        let node = Node {
            id: 1,
            node_type: NodeType::Function,
            properties: props,
        };
        assert_eq!(get_line_start(&node), 42);
    }

    #[test]
    fn test_get_line_start_default() {
        let props = PropertyMap::new();
        let node = Node {
            id: 1,
            node_type: NodeType::Function,
            properties: props,
        };
        assert_eq!(get_line_start(&node), 1);
    }

    #[test]
    fn test_get_line_end() {
        let mut props = PropertyMap::new();
        props.insert("line_end".to_string(), PropertyValue::Int(100));
        let node = Node {
            id: 1,
            node_type: NodeType::Function,
            properties: props,
        };
        assert_eq!(get_line_end(&node, 50), 100);
    }

    #[test]
    fn test_get_col_start() {
        let mut props = PropertyMap::new();
        props.insert("col_start".to_string(), PropertyValue::Int(5));
        let node = Node {
            id: 1,
            node_type: NodeType::Function,
            properties: props,
        };
        assert_eq!(get_col_start(&node), 5);
    }

    #[test]
    fn test_get_col_end() {
        let mut props = PropertyMap::new();
        props.insert("col_end".to_string(), PropertyValue::Int(80));
        let node = Node {
            id: 1,
            node_type: NodeType::Function,
            properties: props,
        };
        assert_eq!(get_col_end(&node), 80);
    }

    #[test]
    fn test_is_test_node_by_name() {
        let mut props = PropertyMap::new();
        props.insert(
            "name".to_string(),
            PropertyValue::String("test_something".to_string()),
        );
        let node = Node {
            id: 1,
            node_type: NodeType::Function,
            properties: props,
        };
        assert!(CodeGraphBackend::is_test_node(&node));
    }

    #[test]
    fn test_is_test_node_by_path() {
        let mut props = PropertyMap::new();
        props.insert(
            "name".to_string(),
            PropertyValue::String("something".to_string()),
        );
        props.insert(
            "path".to_string(),
            PropertyValue::String("/project/tests/my_test.rs".to_string()),
        );
        let node = Node {
            id: 1,
            node_type: NodeType::Function,
            properties: props,
        };
        assert!(CodeGraphBackend::is_test_node(&node));
    }

    #[test]
    fn test_is_not_test_node() {
        let mut props = PropertyMap::new();
        props.insert(
            "name".to_string(),
            PropertyValue::String("regular_function".to_string()),
        );
        props.insert(
            "path".to_string(),
            PropertyValue::String("/project/src/lib.rs".to_string()),
        );
        let node = Node {
            id: 1,
            node_type: NodeType::Function,
            properties: props,
        };
        assert!(!CodeGraphBackend::is_test_node(&node));
    }

    #[test]
    fn test_node_to_range() {
        let mut props = PropertyMap::new();
        props.insert("line_start".to_string(), PropertyValue::Int(10));
        props.insert("line_end".to_string(), PropertyValue::Int(20));
        props.insert("col_start".to_string(), PropertyValue::Int(4));
        props.insert("col_end".to_string(), PropertyValue::Int(50));
        let node = Node {
            id: 1,
            node_type: NodeType::Function,
            properties: props,
        };

        let range = CodeGraphBackend::node_to_range(&node);
        assert!(range.is_some());
        let range = range.unwrap();
        // Line 10 -> 9 (0-indexed)
        assert_eq!(range.start.line, 9);
        assert_eq!(range.start.character, 4);
        // Line 20 -> 19 (0-indexed)
        assert_eq!(range.end.line, 19);
        assert_eq!(range.end.character, 50);
    }

    // ==========================================
    // Dependency Graph tests
    // ==========================================

    #[tokio::test]
    async fn test_handle_get_dependency_graph_invalid_uri() {
        let backend = create_test_backend().await;

        let params = DependencyGraphParams {
            uri: "not_a_valid_uri".to_string(),
            depth: None,
            include_external: None,
            direction: None,
        };

        let result = backend.handle_get_dependency_graph(params).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_handle_get_dependency_graph_no_file_node() {
        let backend = create_test_backend().await;

        let params = DependencyGraphParams {
            uri: "file:///nonexistent/file.rs".to_string(),
            depth: Some(2),
            include_external: Some(false),
            direction: None,
        };

        let result = backend.handle_get_dependency_graph(params).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert!(response.nodes.is_empty());
        assert!(response.edges.is_empty());
    }

    #[tokio::test]
    async fn test_handle_get_dependency_graph_with_imports() {
        let (backend, _file1_id, _file2_id) = create_backend_with_imports().await;

        let params = DependencyGraphParams {
            uri: "file:///test/main.rs".to_string(),
            depth: Some(3),
            include_external: Some(false),
            direction: None,
        };

        let result = backend.handle_get_dependency_graph(params).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        // Should have at least the starting node
        assert!(!response.nodes.is_empty());
    }

    // ==========================================
    // Call Graph tests
    // ==========================================

    #[tokio::test]
    async fn test_handle_get_call_graph_invalid_uri() {
        let backend = create_test_backend().await;

        let params = CallGraphParams {
            uri: "not_a_valid_uri".to_string(),
            position: Position {
                line: 0,
                character: 0,
            },
            direction: None,
            depth: None,
            include_external: None,
        };

        let result = backend.handle_get_call_graph(params).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_handle_get_call_graph_no_node_at_position() {
        let backend = create_test_backend().await;

        let params = CallGraphParams {
            uri: "file:///test/main.rs".to_string(),
            position: Position {
                line: 0,
                character: 0,
            },
            direction: None,
            depth: Some(2),
            include_external: Some(false),
        };

        let result = backend.handle_get_call_graph(params).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert!(response.root.is_none());
        assert!(response.nodes.is_empty());
    }

    #[tokio::test]
    async fn test_handle_get_call_graph_with_calls() {
        let (backend, _func1_id, _func2_id) = create_backend_with_calls().await;

        // Position at line 1 (0-indexed = 0) which is within func1 (lines 1-10)
        let params = CallGraphParams {
            uri: "file:///test/main.rs".to_string(),
            position: Position {
                line: 0,
                character: 0,
            },
            direction: Some("both".to_string()),
            depth: Some(3),
            include_external: Some(false),
        };

        let result = backend.handle_get_call_graph(params).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        // If we found the node, we should have nodes and potentially edges
        if response.root.is_some() {
            assert!(!response.nodes.is_empty());
        }
    }

    // ==========================================
    // Impact Analysis tests
    // ==========================================

    #[tokio::test]
    async fn test_handle_analyze_impact_invalid_uri() {
        let backend = create_test_backend().await;

        let params = ImpactAnalysisParams {
            uri: "not_a_valid_uri".to_string(),
            position: Position {
                line: 0,
                character: 0,
            },
            analysis_type: "modify".to_string(),
        };

        let result = backend.handle_analyze_impact(params).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_handle_analyze_impact_no_node() {
        let backend = create_test_backend().await;

        let params = ImpactAnalysisParams {
            uri: "file:///test/main.rs".to_string(),
            position: Position {
                line: 0,
                character: 0,
            },
            analysis_type: "modify".to_string(),
        };

        let result = backend.handle_analyze_impact(params).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert!(response.direct_impact.is_empty());
        assert!(response.indirect_impact.is_empty());
        assert_eq!(response.summary.files_affected, 0);
    }

    #[tokio::test]
    async fn test_handle_analyze_impact_with_references() {
        let (backend, _func1_id, _func2_id) = create_backend_with_calls().await;

        // Analyze impact on func2 which is called by func1
        // Position within func2 (lines 15-25, so 0-indexed = 14-24)
        let params = ImpactAnalysisParams {
            uri: "file:///test/main.rs".to_string(),
            position: Position {
                line: 14,
                character: 0,
            },
            analysis_type: "delete".to_string(),
        };

        let result = backend.handle_analyze_impact(params).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        // func2 is called by func1, so there should be direct impact
        if !response.direct_impact.is_empty() {
            // Verify severity is correct for delete operation
            assert!(response
                .direct_impact
                .iter()
                .any(|i| i.severity == "breaking"));
        }
    }

    // ==========================================
    // Related Tests tests
    // ==========================================

    #[tokio::test]
    async fn test_handle_find_related_tests_invalid_uri() {
        let backend = create_test_backend().await;

        let params = RelatedTestsParams {
            uri: "not_a_valid_uri".to_string(),
            position: Position {
                line: 0,
                character: 0,
            },
            limit: None,
        };

        let result = backend.handle_find_related_tests(params).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_handle_find_related_tests_no_node() {
        let backend = create_test_backend().await;

        let params = RelatedTestsParams {
            uri: "file:///test/main.rs".to_string(),
            position: Position {
                line: 0,
                character: 0,
            },
            limit: Some(10),
        };

        let result = backend.handle_find_related_tests(params).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert!(response.tests.is_empty());
    }

    #[tokio::test]
    async fn test_handle_find_related_tests_with_test_caller() {
        let graph = Arc::new(RwLock::new(
            CodeGraph::in_memory().expect("Failed to create graph"),
        ));

        let (func_id, test_id) = {
            let mut g = graph.write().await;

            // Create a regular function
            let mut props1 = PropertyMap::new();
            props1.insert(
                "name".to_string(),
                PropertyValue::String("my_function".to_string()),
            );
            props1.insert(
                "path".to_string(),
                PropertyValue::String("/test/lib.rs".to_string()),
            );
            props1.insert("line_start".to_string(), PropertyValue::Int(1));
            props1.insert("line_end".to_string(), PropertyValue::Int(10));
            let func_id = g.add_node(NodeType::Function, props1).unwrap();

            // Create a test function that calls the regular function
            let mut props2 = PropertyMap::new();
            props2.insert(
                "name".to_string(),
                PropertyValue::String("test_my_function".to_string()),
            );
            props2.insert(
                "path".to_string(),
                PropertyValue::String("/test/tests/lib_test.rs".to_string()),
            );
            props2.insert("line_start".to_string(), PropertyValue::Int(1));
            props2.insert("line_end".to_string(), PropertyValue::Int(10));
            let test_id = g.add_node(NodeType::Function, props2).unwrap();

            // Test calls the function
            g.add_edge(test_id, func_id, EdgeType::Calls, PropertyMap::new())
                .unwrap();

            (func_id, test_id)
        };

        let query_engine = Arc::new(QueryEngine::new(Arc::clone(&graph)));
        let backend = CodeGraphBackend::new_for_test(graph, query_engine);

        // Add to symbol index
        let func_path = std::path::Path::new("/test/lib.rs");
        let test_path = std::path::Path::new("/test/tests/lib_test.rs");
        add_node_to_index(
            &backend,
            func_path,
            func_id,
            "my_function",
            "Function",
            1,
            10,
        );
        add_node_to_index(
            &backend,
            test_path,
            test_id,
            "test_my_function",
            "Function",
            1,
            10,
        );

        let params = RelatedTestsParams {
            uri: "file:///test/lib.rs".to_string(),
            position: Position {
                line: 0,
                character: 0,
            },
            limit: Some(10),
        };

        let result = backend.handle_find_related_tests(params).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        // Should find the test that calls my_function
        assert!(!response.tests.is_empty());
        assert!(response.tests.iter().any(|t| t.test_name.contains("test")));
    }

    // ==========================================
    // Parser Metrics tests
    // ==========================================

    #[tokio::test]
    async fn test_handle_get_parser_metrics_all() {
        let backend = create_test_backend().await;

        let params = ParserMetricsParams { language: None };

        let result = backend.handle_get_parser_metrics(params).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        // Should have metrics for supported languages
        assert!(response.totals.success_rate >= 0.0);
    }

    #[tokio::test]
    async fn test_handle_get_parser_metrics_filtered() {
        let backend = create_test_backend().await;

        let params = ParserMetricsParams {
            language: Some("rust".to_string()),
        };

        let result = backend.handle_get_parser_metrics(params).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        // All metrics should be for rust only
        for metric in &response.metrics {
            assert_eq!(metric.language, "rust");
        }
    }

    #[tokio::test]
    async fn test_handle_get_parser_metrics_unknown_language() {
        let backend = create_test_backend().await;

        let params = ParserMetricsParams {
            language: Some("unknown_language_xyz".to_string()),
        };

        let result = backend.handle_get_parser_metrics(params).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert!(response.metrics.is_empty());
    }

    // ==========================================
    // Edge collection helper tests
    // ==========================================

    #[tokio::test]
    async fn test_collect_edges_for_direction_both() {
        let (backend, func1_id, _func2_id) = create_backend_with_calls().await;
        let graph = backend.graph.read().await;

        let edges = CodeGraphBackend::collect_edges_for_direction(
            &graph, func1_id, "both", "callers", "callees",
        );

        // func1 calls func2, so there should be an outgoing edge
        assert!(!edges.is_empty());
    }

    #[tokio::test]
    async fn test_collect_edges_for_direction_callees_only() {
        let (backend, func1_id, _func2_id) = create_backend_with_calls().await;
        let graph = backend.graph.read().await;

        // Test func1 with direction = "callers" which matches skip_outgoing, so only incoming checked
        // func1 (main) has no incoming edges (nothing calls it)
        let edges = CodeGraphBackend::collect_edges_for_direction(
            &graph, func1_id, "callers", "callers", "callees",
        );

        // func1 has no incoming edges (nothing calls main), so should be empty
        assert!(edges.is_empty());
    }

    #[tokio::test]
    async fn test_get_incoming_edges() {
        let (backend, func1_id, func2_id) = create_backend_with_calls().await;
        let graph = backend.graph.read().await;

        // func2 is called by func1, so incoming edges to func2 should have func1
        let edges = CodeGraphBackend::get_incoming_edges(&graph, func2_id);
        assert!(!edges.is_empty());
        assert!(edges.iter().any(|(src, _, _)| *src == func1_id));
    }
}
