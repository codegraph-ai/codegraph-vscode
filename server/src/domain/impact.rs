//! Impact analysis — transport-agnostic.
//!
//! Extracts analyze_impact from MCP server.

use crate::ai_query::QueryEngine;
use crate::domain::node_props;
use codegraph::{CodeGraph, NodeId};
use serde_json::Value;
use tokio::sync::RwLock;

// ============================================================
// Domain Function
// ============================================================

/// Analyze the blast radius of a change to a symbol.
///
/// `change_type` is one of: "modify" | "delete" | "rename"
///
/// Returns JSON with impacted symbols, risk level, and optional fallback metadata.
pub(crate) async fn analyze_impact(
    graph: &RwLock<CodeGraph>,
    query_engine: &QueryEngine,
    start_node: NodeId,
    change_type: &str,
    used_fallback: bool,
    requested_line: Option<u32>,
) -> Value {
    let symbol_name = {
        let g = graph.read().await;
        g.get_node(start_node)
            .ok()
            .map(|n| node_props::name(n).to_string())
            .unwrap_or_default()
    };

    // Get all callers (things that depend on this symbol)
    let callers = query_engine.get_callers(start_node, 3).await;

    let impacted: Vec<Value> = callers
        .iter()
        .map(|c| {
            serde_json::json!({
                "node_id": c.node_id.to_string(),
                "name": c.symbol.name,
                "depth": c.depth,
                "impact_type": if c.depth == 1 { "direct" } else { "indirect" },
            })
        })
        .collect();

    let risk_level = match (change_type, callers.len()) {
        ("delete", n) if n > 10 => "critical",
        ("delete", n) if n > 0 => "high",
        ("rename", n) if n > 10 => "high",
        ("rename", n) if n > 0 => "medium",
        ("modify", n) if n > 20 => "medium",
        ("modify", _) => "low",
        _ => "low",
    };

    let mut response = serde_json::json!({
        "symbol_id": start_node.to_string(),
        "symbol_name": symbol_name,
        "change_type": change_type,
        "impacted": impacted,
        "total_impacted": callers.len(),
        "direct_impacted": callers.iter().filter(|c| c.depth == 1).count(),
        "risk_level": risk_level,
    });

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
