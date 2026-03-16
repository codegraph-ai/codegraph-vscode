//! Module coupling analysis — transport-agnostic.
//!
//! Extracts analyze_coupling from MCP server. Uses domain::dependency_graph.

use crate::domain::dependency_graph::{self, DependencyGraphResult};
use codegraph::CodeGraph;
use serde::Serialize;

// ============================================================
// Response Types
// ============================================================

/// Coupling metrics for a file.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct CouplingMetrics {
    pub afferent_coupling: usize,
    pub efferent_coupling: usize,
    pub instability: f64,
    pub total_dependencies: usize,
    pub total_connections: usize,
}

/// Result of `analyze_coupling`.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct CouplingResult {
    pub uri: String,
    pub metrics: CouplingMetrics,
    pub dependency_graph: DependencyGraphResult,
}

// ============================================================
// Domain Function
// ============================================================

/// Compute coupling metrics for a file.
///
/// Calls `dependency_graph::get_dependency_graph` then computes
/// afferent/efferent/instability metrics from the result.
pub(crate) fn analyze_coupling(
    graph: &CodeGraph,
    file_path: &str,
    uri: &str,
    depth: usize,
) -> CouplingResult {
    let dep_graph = dependency_graph::get_dependency_graph(graph, file_path, depth, "both");

    let node_count = dep_graph.nodes.len();
    let edge_count = dep_graph.edges.len();

    // Preserve original logic: all edges have both "from" and "to" fields,
    // so afferent == efferent == edge_count (mirrors original JSON-navigation behavior).
    let afferent = dep_graph.edges.iter().filter(|e| !e.to.is_empty()).count();
    let efferent = dep_graph
        .edges
        .iter()
        .filter(|e| !e.from.is_empty())
        .count();

    let instability = if afferent + efferent > 0 {
        efferent as f64 / (afferent + efferent) as f64
    } else {
        0.0
    };

    CouplingResult {
        uri: uri.to_string(),
        metrics: CouplingMetrics {
            afferent_coupling: afferent,
            efferent_coupling: efferent,
            instability,
            total_dependencies: node_count,
            total_connections: edge_count,
        },
        dependency_graph: dep_graph,
    }
}
