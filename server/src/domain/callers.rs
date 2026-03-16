//! Caller/callee graph traversal — transport-agnostic.
//!
//! Extracts the shared pattern from MCP get_callers/get_callees handlers.

use crate::ai_query::QueryEngine;
use crate::domain::node_props;
use codegraph::{CodeGraph, NodeId};
use serde_json::Value;
use tokio::sync::RwLock;

// ============================================================
// Domain Functions
// ============================================================

/// Get all callers of a symbol at the given depth.
///
/// Returns JSON matching the MCP codegraph_get_callers response shape.
pub(crate) async fn get_callers(
    graph: &RwLock<CodeGraph>,
    query_engine: &QueryEngine,
    node_id: NodeId,
    depth: u32,
    used_fallback: bool,
    requested_line: Option<u32>,
) -> Value {
    let symbol_name = {
        let g = graph.read().await;
        g.get_node(node_id)
            .ok()
            .map(|n| node_props::name(n).to_string())
            .unwrap_or_default()
    };

    let result = query_engine.get_callers(node_id, depth).await;

    if result.is_empty() {
        let edge_count = {
            let g = graph.read().await;
            g.edge_count()
        };
        let mut response = serde_json::json!({
            "callers": [],
            "diagnostic": {
                "node_found": true,
                "node_id": node_id,
                "symbol_name": symbol_name,
                "total_edges_in_graph": edge_count,
                "note": "No callers found. This may indicate: (1) the function is not called \
                         anywhere, (2) the language parser doesn't extract call relationships, \
                         or (3) indexes need to be rebuilt."
            }
        });
        add_fallback_fields(&mut response, used_fallback, requested_line, &symbol_name);
        response
    } else {
        let callers_json = serde_json::to_value(&result).unwrap_or(Value::Array(vec![]));
        let mut obj = serde_json::Map::new();
        obj.insert("callers".to_string(), callers_json);
        obj.insert("symbol_name".to_string(), serde_json::json!(symbol_name));
        let mut response = Value::Object(obj);
        add_fallback_fields(&mut response, used_fallback, requested_line, &symbol_name);
        response
    }
}

/// Get all callees of a symbol at the given depth.
///
/// Returns JSON matching the MCP codegraph_get_callees response shape.
pub(crate) async fn get_callees(
    graph: &RwLock<CodeGraph>,
    query_engine: &QueryEngine,
    node_id: NodeId,
    depth: u32,
    used_fallback: bool,
    requested_line: Option<u32>,
) -> Value {
    let symbol_name = {
        let g = graph.read().await;
        g.get_node(node_id)
            .ok()
            .map(|n| node_props::name(n).to_string())
            .unwrap_or_default()
    };

    let result = query_engine.get_callees(node_id, depth).await;

    if result.is_empty() {
        let edge_count = {
            let g = graph.read().await;
            g.edge_count()
        };
        let mut response = serde_json::json!({
            "callees": [],
            "diagnostic": {
                "node_found": true,
                "node_id": node_id,
                "symbol_name": symbol_name,
                "total_edges_in_graph": edge_count,
                "note": "No callees found. This may indicate: (1) the function doesn't call \
                         other functions, (2) the language parser doesn't extract call \
                         relationships, or (3) indexes need to be rebuilt."
            }
        });
        add_fallback_fields(&mut response, used_fallback, requested_line, &symbol_name);
        response
    } else {
        let callees_json = serde_json::to_value(&result).unwrap_or(Value::Array(vec![]));
        let mut obj = serde_json::Map::new();
        obj.insert("callees".to_string(), callees_json);
        obj.insert("symbol_name".to_string(), serde_json::json!(symbol_name));
        let mut response = Value::Object(obj);
        add_fallback_fields(&mut response, used_fallback, requested_line, &symbol_name);
        response
    }
}

// ============================================================
// Private Helpers
// ============================================================

fn add_fallback_fields(
    response: &mut Value,
    used_fallback: bool,
    requested_line: Option<u32>,
    symbol_name: &str,
) {
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
}
