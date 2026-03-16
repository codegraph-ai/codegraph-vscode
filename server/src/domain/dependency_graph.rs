//! Dependency graph traversal — transport-agnostic.
//!
//! Extracts get_dependency_graph from MCP server. Synchronous (takes &CodeGraph).

use crate::domain::node_props;
use codegraph::{CodeGraph, Direction, NodeId};
use serde::Serialize;
use std::collections::HashSet;

// ============================================================
// Response Types
// ============================================================

/// A node in the dependency graph.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct GraphNode {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub node_type: String,
}

/// An edge in the dependency graph.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct GraphEdge {
    pub from: String,
    pub to: String,
    #[serde(rename = "type")]
    pub edge_type: String,
}

/// Result of `get_dependency_graph`.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct DependencyGraphResult {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

// ============================================================
// Domain Function
// ============================================================

/// Build a file-level dependency graph using BFS from the given file path.
///
/// `direction` is one of: "imports" | "importedBy" | "both"
///
/// Returns `DependencyGraphResult` with nodes and edges, or empty arrays on error.
pub(crate) fn get_dependency_graph(
    graph: &CodeGraph,
    file_path: &str,
    depth: usize,
    direction: &str,
) -> DependencyGraphResult {
    // Find the file node
    let start_node = match codegraph::helpers::find_file_by_path(graph, file_path) {
        Ok(Some(id)) => id,
        _ => {
            return DependencyGraphResult {
                nodes: vec![],
                edges: vec![],
            }
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
            let name = node_props::name(node).to_string();
            let node_type = format!("{:?}", node.node_type);
            nodes.push(GraphNode {
                id: node_id.to_string(),
                name,
                node_type,
            });
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
                        edges.push(GraphEdge {
                            from: from.to_string(),
                            to: to.to_string(),
                            edge_type: "depends_on".to_string(),
                        });
                    }
                }
            }
        }
    }

    DependencyGraphResult { nodes, edges }
}
