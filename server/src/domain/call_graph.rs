//! Call graph traversal — transport-agnostic.
//!
//! Extracts get_call_graph from MCP server.

use crate::ai_query::QueryEngine;
use crate::domain::node_props;
use codegraph::{CodeGraph, NodeId};
use serde::Serialize;
use tokio::sync::RwLock;

// ============================================================
// Response Types
// ============================================================

/// A node in the call graph.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct CallGraphNode {
    pub id: String,
    pub name: String,
    pub depth: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,
}

/// An edge in the call graph.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct CallGraphEdge {
    pub from: String,
    pub to: String,
    #[serde(rename = "type")]
    pub edge_type: String,
}

/// Diagnostic information when no call relationships are found.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct CallGraphDiagnostic {
    pub node_found: bool,
    pub total_edges_in_graph: usize,
    pub note: String,
}

/// Result of `get_call_graph`.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct CallGraphResult {
    pub root: String,
    pub symbol_name: String,
    pub nodes: Vec<CallGraphNode>,
    pub edges: Vec<CallGraphEdge>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<CallGraphDiagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used_fallback: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_message: Option<String>,
}

// ============================================================
// Domain Function
// ============================================================

/// Build a call graph for a symbol.
///
/// `direction` is one of: "callers" | "callees" | "both"
///
/// `used_fallback` / `requested_line` add fallback metadata to the response.
pub(crate) async fn get_call_graph(
    graph: &RwLock<CodeGraph>,
    query_engine: &QueryEngine,
    start_node: NodeId,
    depth: u32,
    direction: &str,
    used_fallback: bool,
    requested_line: Option<u32>,
) -> CallGraphResult {
    // Get symbol name for response/fallback
    let symbol_name = {
        let g = graph.read().await;
        g.get_node(start_node)
            .ok()
            .map(|n| node_props::name(n).to_string())
            .unwrap_or_default()
    };

    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut seen = std::collections::HashSet::new();
    seen.insert(start_node);

    match direction {
        "callers" => {
            let callers = query_engine.get_callers(start_node, depth).await;
            for caller in callers {
                if seen.insert(caller.node_id) {
                    nodes.push(CallGraphNode {
                        id: caller.node_id.to_string(),
                        name: caller.symbol.name,
                        depth: caller.depth,
                        direction: None,
                    });
                    edges.push(CallGraphEdge {
                        from: caller.node_id.to_string(),
                        to: start_node.to_string(),
                        edge_type: "calls".to_string(),
                    });
                }
            }
        }
        "callees" => {
            let callees = query_engine.get_callees(start_node, depth).await;
            for callee in callees {
                if seen.insert(callee.node_id) {
                    nodes.push(CallGraphNode {
                        id: callee.node_id.to_string(),
                        name: callee.symbol.name,
                        depth: callee.depth,
                        direction: None,
                    });
                    edges.push(CallGraphEdge {
                        from: start_node.to_string(),
                        to: callee.node_id.to_string(),
                        edge_type: "calls".to_string(),
                    });
                }
            }
        }
        _ => {
            // Both directions
            let callers = query_engine.get_callers(start_node, depth).await;
            let callees = query_engine.get_callees(start_node, depth).await;

            for caller in callers {
                if seen.insert(caller.node_id) {
                    nodes.push(CallGraphNode {
                        id: caller.node_id.to_string(),
                        name: caller.symbol.name,
                        depth: caller.depth,
                        direction: Some("caller".to_string()),
                    });
                    edges.push(CallGraphEdge {
                        from: caller.node_id.to_string(),
                        to: start_node.to_string(),
                        edge_type: "calls".to_string(),
                    });
                }
            }

            for callee in callees {
                if seen.insert(callee.node_id) {
                    nodes.push(CallGraphNode {
                        id: callee.node_id.to_string(),
                        name: callee.symbol.name,
                        depth: callee.depth,
                        direction: Some("callee".to_string()),
                    });
                    edges.push(CallGraphEdge {
                        from: start_node.to_string(),
                        to: callee.node_id.to_string(),
                        edge_type: "calls".to_string(),
                    });
                }
            }
        }
    }

    let diagnostic = if nodes.is_empty() {
        let edge_count = {
            let g = graph.read().await;
            g.edge_count()
        };
        Some(CallGraphDiagnostic {
            node_found: true,
            total_edges_in_graph: edge_count,
            note: "No call relationships found. Call graph analysis depends on language \
                   parser support for extracting call edges. Some parsers may have \
                   limited call extraction capabilities."
                .to_string(),
        })
    } else {
        None
    };

    let (used_fallback_field, fallback_message) = if used_fallback {
        (
            Some(true),
            Some(format!(
                "No symbol at line {}. Using nearest symbol '{}' instead.",
                requested_line.unwrap_or(0),
                symbol_name
            )),
        )
    } else {
        (None, None)
    };

    CallGraphResult {
        root: start_node.to_string(),
        symbol_name,
        nodes,
        edges,
        diagnostic,
        used_fallback: used_fallback_field,
        fallback_message,
    }
}
