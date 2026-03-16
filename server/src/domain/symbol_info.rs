//! Symbol info assembly — transport-agnostic.
//!
//! Extracts get_symbol_info / get_detailed_symbol from MCP server.

use crate::ai_query::QueryEngine;
use crate::domain::source_code;
use codegraph::{CodeGraph, NodeId};
use serde_json::Value;
use tokio::sync::RwLock;

// ============================================================
// Domain Functions
// ============================================================

/// Get basic symbol info with optional fallback metadata.
///
/// Wraps query_engine.get_symbol_info() and optionally strips references
/// or adds fallback fields.
pub(crate) async fn get_symbol_info(
    _graph: &RwLock<CodeGraph>,
    query_engine: &QueryEngine,
    node_id: NodeId,
    include_refs: bool,
    used_fallback: bool,
    requested_line: Option<u32>,
) -> Option<Value> {
    let info = query_engine.get_symbol_info(node_id).await?;

    let mut response = serde_json::to_value(&info).ok()?;

    if used_fallback {
        let name = &info.symbol.name;
        if let Some(obj) = response.as_object_mut() {
            obj.insert("used_fallback".to_string(), serde_json::json!(true));
            obj.insert(
                "fallback_message".to_string(),
                serde_json::json!(format!(
                    "No symbol at line {}. Using nearest symbol '{}' instead.",
                    requested_line.unwrap_or(0),
                    name
                )),
            );
        }
    }

    if !include_refs {
        if let Some(obj) = response.as_object_mut() {
            obj.remove("callers");
            obj.remove("callees");
            obj.remove("dependencies");
            obj.remove("dependents");
        }
    }

    Some(response)
}

/// Get detailed symbol info: basic info + optional source + callers + callees.
///
/// Returns ad-hoc JSON. Shape matches the MCP codegraph_get_detailed_symbol response.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn get_detailed_symbol(
    graph: &RwLock<CodeGraph>,
    query_engine: &QueryEngine,
    node_id: NodeId,
    include_source: bool,
    include_callers: bool,
    include_callees: bool,
    used_fallback: bool,
    requested_line: Option<u32>,
) -> Value {
    let mut result = serde_json::Map::new();

    // Get basic symbol info
    let symbol_name = if let Some(info) = query_engine.get_symbol_info(node_id).await {
        let name = info.symbol.name.clone();
        result.insert(
            "symbol".to_string(),
            serde_json::to_value(&info).unwrap_or(Value::Null),
        );
        name
    } else {
        String::new()
    };

    // Add fallback metadata if used
    if used_fallback {
        result.insert("used_fallback".to_string(), serde_json::json!(true));
        result.insert(
            "fallback_message".to_string(),
            serde_json::json!(format!(
                "No symbol at line {}. Using nearest symbol '{}' instead.",
                requested_line.unwrap_or(0),
                symbol_name
            )),
        );
    }

    // Get source code if requested
    if include_source {
        let g = graph.read().await;
        if let Some(src) = source_code::get_symbol_source(&g, node_id) {
            result.insert("source".to_string(), Value::String(src));
        }
    }

    // Get callers if requested
    if include_callers {
        let callers = query_engine.get_callers(node_id, 1).await;
        result.insert(
            "callers".to_string(),
            serde_json::to_value(&callers).unwrap_or(Value::Array(vec![])),
        );
    }

    // Get callees if requested
    if include_callees {
        let callees = query_engine.get_callees(node_id, 1).await;
        result.insert(
            "callees".to_string(),
            serde_json::to_value(&callees).unwrap_or(Value::Array(vec![])),
        );
    }

    Value::Object(result)
}
