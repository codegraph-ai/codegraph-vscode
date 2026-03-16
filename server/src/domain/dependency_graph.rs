//! Dependency graph traversal — transport-agnostic.
//!
//! Extracts get_dependency_graph from MCP server. Synchronous (takes &CodeGraph).

use crate::domain::node_props;
use codegraph::{CodeGraph, Direction, NodeId};
use serde_json::Value;
use std::collections::HashSet;

// ============================================================
// Domain Function
// ============================================================

/// Build a file-level dependency graph using BFS from the given file path.
///
/// `direction` is one of: "imports" | "importedBy" | "both"
///
/// Returns JSON `{"nodes": [...], "edges": [...]}` or an error shape.
pub(crate) fn get_dependency_graph(
    graph: &CodeGraph,
    file_path: &str,
    depth: usize,
    direction: &str,
) -> Value {
    // Find the file node
    let start_node = match codegraph::helpers::find_file_by_path(graph, file_path) {
        Ok(Some(id)) => id,
        _ => {
            return serde_json::json!({
                "nodes": [],
                "edges": []
            })
        }
    };

    let bfs_direction = match direction {
        "imports" => Direction::Outgoing,
        "importedBy" => Direction::Incoming,
        _ => Direction::Both,
    };

    let mut reachable_set: HashSet<NodeId> = HashSet::new();
    reachable_set.insert(start_node);
    if let Ok(reachable) = graph.bfs(start_node, bfs_direction, Some(depth)) {
        reachable_set.extend(reachable);
    }

    // Build response nodes
    let mut nodes = Vec::new();
    for &node_id in &reachable_set {
        if let Ok(node) = graph.get_node(node_id) {
            let name = node_props::name(node);
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
    let mut seen_edges: HashSet<(NodeId, NodeId)> = HashSet::new();

    let edge_directions: Vec<Direction> = match direction {
        "imports" => vec![Direction::Outgoing],
        "importedBy" => vec![Direction::Incoming],
        _ => vec![Direction::Outgoing, Direction::Incoming],
    };

    for &node_id in &reachable_set {
        for &dir in &edge_directions {
            if let Ok(neighbors) = graph.get_neighbors(node_id, dir) {
                for neighbor_id in neighbors {
                    if !reachable_set.contains(&neighbor_id) {
                        continue;
                    }
                    let (from, to) = match dir {
                        Direction::Outgoing => (node_id, neighbor_id),
                        Direction::Incoming => (neighbor_id, node_id),
                        Direction::Both => (node_id, neighbor_id),
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
