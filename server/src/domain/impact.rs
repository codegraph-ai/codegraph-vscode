//! Impact analysis — transport-agnostic.
//!
//! Extracts analyze_impact from MCP server.

use crate::ai_query::QueryEngine;
use crate::domain::node_props;
use codegraph::{CodeGraph, NodeId};
use serde::Serialize;
use tokio::sync::RwLock;

// ============================================================
// Response Types
// ============================================================

/// A symbol impacted by a change.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ImpactedSymbol {
    pub node_id: String,
    pub name: String,
    pub depth: u32,
    pub impact_type: String,
}

/// Result of `analyze_impact`.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ImpactResult {
    pub symbol_id: String,
    pub symbol_name: String,
    pub change_type: String,
    pub impacted: Vec<ImpactedSymbol>,
    pub total_impacted: usize,
    pub direct_impacted: usize,
    pub risk_level: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used_fallback: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_message: Option<String>,
}

// ============================================================
// Domain Function
// ============================================================

/// Analyze the blast radius of a change to a symbol.
///
/// `change_type` is one of: "modify" | "delete" | "rename"
///
/// Returns `ImpactResult` with impacted symbols, risk level, and optional fallback metadata.
pub(crate) async fn analyze_impact(
    graph: &RwLock<CodeGraph>,
    query_engine: &QueryEngine,
    start_node: NodeId,
    change_type: &str,
    used_fallback: bool,
    requested_line: Option<u32>,
) -> ImpactResult {
    let symbol_name = {
        let g = graph.read().await;
        g.get_node(start_node)
            .ok()
            .map(|n| node_props::name(n).to_string())
            .unwrap_or_default()
    };

    // Get all callers (things that depend on this symbol)
    let callers = query_engine.get_callers(start_node, 3).await;

    let impacted: Vec<ImpactedSymbol> = callers
        .iter()
        .map(|c| ImpactedSymbol {
            node_id: c.node_id.to_string(),
            name: c.symbol.name.clone(),
            depth: c.depth,
            impact_type: if c.depth == 1 {
                "direct".to_string()
            } else {
                "indirect".to_string()
            },
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

    let direct_impacted = callers.iter().filter(|c| c.depth == 1).count();
    let total_impacted = callers.len();

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

    ImpactResult {
        symbol_id: start_node.to_string(),
        symbol_name,
        change_type: change_type.to_string(),
        impacted,
        total_impacted,
        direct_impacted,
        risk_level: risk_level.to_string(),
        used_fallback: used_fallback_field,
        fallback_message,
    }
}
