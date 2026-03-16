//! Module coupling analysis — transport-agnostic.
//!
//! Extracts analyze_coupling from MCP server. Uses domain::dependency_graph.

use crate::domain::dependency_graph;
use codegraph::CodeGraph;
use serde_json::Value;

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
) -> Value {
    let dep_graph = dependency_graph::get_dependency_graph(graph, file_path, depth, "both");

    let node_count = dep_graph
        .get("nodes")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    let edge_count = dep_graph
        .get("edges")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);

    // Simple coupling metrics based on edge count
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
            "total_dependencies": node_count,
            "total_connections": edge_count,
        },
        "dependency_graph": dep_graph,
    })
}
