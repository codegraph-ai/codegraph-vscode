//! Call graph traversal — transport-agnostic.
//!
//! Extracts get_call_graph from MCP server.

use crate::ai_query::QueryEngine;
use crate::domain::node_props;
use codegraph::{CodeGraph, NodeId};
use serde_json::Value;
use tokio::sync::RwLock;

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
) -> Value {
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
                    nodes.push(serde_json::json!({
                        "id": caller.node_id.to_string(),
                        "name": caller.symbol.name,
                        "depth": caller.depth,
                    }));
                    edges.push(serde_json::json!({
                        "from": caller.node_id.to_string(),
                        "to": start_node.to_string(),
                        "type": "calls",
                    }));
                }
            }
        }
        "callees" => {
            let callees = query_engine.get_callees(start_node, depth).await;
            for callee in callees {
                if seen.insert(callee.node_id) {
                    nodes.push(serde_json::json!({
                        "id": callee.node_id.to_string(),
                        "name": callee.symbol.name,
                        "depth": callee.depth,
                    }));
                    edges.push(serde_json::json!({
                        "from": start_node.to_string(),
                        "to": callee.node_id.to_string(),
                        "type": "calls",
                    }));
                }
            }
        }
        _ => {
            // Both directions
            let callers = query_engine.get_callers(start_node, depth).await;
            let callees = query_engine.get_callees(start_node, depth).await;

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
                        "to": start_node.to_string(),
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
                        "from": start_node.to_string(),
                        "to": callee.node_id.to_string(),
                        "type": "calls",
                    }));
                }
            }
        }
    }

    let mut response = if nodes.is_empty() {
        let edge_count = {
            let g = graph.read().await;
            g.edge_count()
        };
        serde_json::json!({
            "root": start_node.to_string(),
            "symbol_name": symbol_name,
            "nodes": nodes,
            "edges": edges,
            "diagnostic": {
                "node_found": true,
                "total_edges_in_graph": edge_count,
                "note": "No call relationships found. Call graph analysis depends on language \
                         parser support for extracting call edges. Some parsers may have \
                         limited call extraction capabilities."
            }
        })
    } else {
        serde_json::json!({
            "root": start_node.to_string(),
            "symbol_name": symbol_name,
            "nodes": nodes,
            "edges": edges
        })
    };

    if used_fallback {
        if let Some(obj) = response.as_object_mut() {
            obj.insert("used_fallback".to_string(), serde_json::json!(true));
            obj.insert(
                "fallback_message".to_string(),
                serde_json::json!(format!(
                    "No symbol at line {}. Using nearest symbol '{}' instead.",
                    requested_line.unwrap_or(0),
                    symbol_name
                )),
            );
        }
    }

    response
}
