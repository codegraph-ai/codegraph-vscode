//! Impact analysis — transport-agnostic.
//!
//! Extracts analyze_impact from MCP server.

use crate::ai_query::QueryEngine;
use crate::domain::node_props;
use codegraph::{CodeGraph, Direction, EdgeType, NodeId};
use serde::Serialize;
use std::collections::HashSet;
use tokio::sync::RwLock;

// ============================================================
// Response Types
// ============================================================

/// A symbol directly impacted by a change (depth = 1, all edge types).
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ImpactedSymbol {
    pub node_id: String,
    pub name: String,
    pub depth: u32,
    /// Semantic impact type: "caller", "reference", "subclass", "implementation".
    pub impact_type: String,
    /// File path on disk (empty string if unknown).
    pub path: String,
    pub line_start: u32,
    pub line_end: u32,
    pub col_start: u32,
    pub col_end: u32,
    /// "breaking" | "warning" | "info"
    pub severity: String,
    /// Whether the impacted symbol is a test function or lives in a test file.
    pub is_test: bool,
    /// Raw edge type string for debugging (e.g. "Calls", "References").
    pub edge_type_str: String,
}

/// An indirect impact item (reached via 2-level BFS from direct impacts).
#[derive(Debug, Clone, Serialize)]
pub(crate) struct IndirectImpactItem {
    pub node_id: String,
    pub path: String,
    /// Chain of paths from the changed symbol to this item (for display).
    pub via_path: Vec<String>,
    pub severity: String,
}

/// Result of `analyze_impact`.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ImpactResult {
    pub symbol_id: String,
    pub symbol_name: String,
    pub change_type: String,
    /// Direct impacts (all incoming edge types, depth = 1).
    pub impacted: Vec<ImpactedSymbol>,
    /// Indirect impacts (2-level BFS from direct impact nodes).
    pub indirect_impacted: Vec<IndirectImpactItem>,
    pub total_impacted: usize,
    pub direct_impacted: usize,
    pub risk_level: String,
    pub files_affected: usize,
    pub breaking_changes: usize,
    pub warnings: usize,
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
/// Computes direct impact from all incoming edge types (not just calls), then BFS
/// 2 levels from each direct impact for indirect impact. Uses `query_engine.get_callers()`
/// for risk-level calculation (broader caller count at depth 3).
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

    // Use query_engine for risk assessment (callers to depth 3, calls edges only)
    let all_callers = query_engine.get_callers(start_node, 3).await;

    // Compute direct impacts via graph reads (all incoming edge types)
    let mut direct_impacts: Vec<(NodeId, ImpactedSymbol)> = Vec::new();
    let mut affected_files: HashSet<String> = HashSet::new();

    {
        let g = graph.read().await;
        if let Ok(neighbors) = g.get_neighbors(start_node, Direction::Incoming) {
            for source_id in neighbors {
                if let Ok(edge_ids) = g.get_edges_between(source_id, start_node) {
                    for edge_id in edge_ids {
                        if let Ok(edge) = g.get_edge(edge_id) {
                            let impact_type = match edge.edge_type {
                                EdgeType::Calls => "caller",
                                EdgeType::References => "reference",
                                EdgeType::Extends => "subclass",
                                EdgeType::Implements => "implementation",
                                _ => "reference",
                            };
                            let severity = match change_type {
                                "delete" | "rename" => "breaking",
                                "modify" => "warning",
                                _ => "info",
                            };
                            if let Ok(ref_node) = g.get_node(source_id) {
                                let name = node_props::name(ref_node).to_string();
                                let path = node_props::path(ref_node).to_string();
                                let line_start = node_props::line_start(ref_node);
                                let line_end = node_props::line_end(ref_node);
                                let col_start =
                                    node_props::col_start_from_props(&ref_node.properties);
                                let col_end =
                                    node_props::col_end_from_props(&ref_node.properties);
                                let is_test =
                                    crate::domain::unused_code::is_test_node(ref_node);
                                let edge_type_str = format!("{:?}", edge.edge_type);
                                affected_files.insert(path.clone());
                                direct_impacts.push((
                                    source_id,
                                    ImpactedSymbol {
                                        node_id: source_id.to_string(),
                                        name,
                                        depth: 1,
                                        impact_type: impact_type.to_string(),
                                        path,
                                        line_start,
                                        line_end,
                                        col_start,
                                        col_end,
                                        severity: severity.to_string(),
                                        is_test,
                                        edge_type_str,
                                    },
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    // Compute indirect impacts via BFS from each direct impact node
    let mut indirect_impacted: Vec<IndirectImpactItem> = Vec::new();
    let mut indirect_visited: HashSet<NodeId> = HashSet::new();
    indirect_visited.insert(start_node);
    for &(id, _) in &direct_impacts {
        indirect_visited.insert(id);
    }

    {
        let g = graph.read().await;
        for &(direct_id, ref impact) in &direct_impacts {
            if let Ok(indirect_ids) = g.bfs(direct_id, Direction::Incoming, Some(2)) {
                for indirect_id in indirect_ids {
                    if indirect_visited.contains(&indirect_id) {
                        continue;
                    }
                    indirect_visited.insert(indirect_id);
                    if let Ok(ref_node) = g.get_node(indirect_id) {
                        let ref_path = node_props::path(ref_node).to_string();
                        if !affected_files.contains(&ref_path) {
                            indirect_impacted.push(IndirectImpactItem {
                                node_id: indirect_id.to_string(),
                                path: ref_path.clone(),
                                via_path: vec![impact.path.clone(), ref_path],
                                severity: "warning".to_string(),
                            });
                        }
                    }
                }
            }
        }
    }

    let impacted: Vec<ImpactedSymbol> =
        direct_impacts.into_iter().map(|(_, sym)| sym).collect();

    let direct_impacted = impacted.len();
    let total_impacted = direct_impacted + indirect_impacted.len();
    let breaking_changes = impacted
        .iter()
        .filter(|i| i.severity == "breaking")
        .count();
    let warnings = impacted.iter().filter(|i| i.severity == "warning").count()
        + indirect_impacted.len();

    // Use all_callers (depth 3) for risk_level to account for transitive call exposure
    let risk_level = match (change_type, all_callers.len()) {
        ("delete", n) if n > 10 => "critical",
        ("delete", n) if n > 0 => "high",
        ("rename", n) if n > 10 => "high",
        ("rename", n) if n > 0 => "medium",
        ("modify", n) if n > 20 => "medium",
        ("modify", _) => "low",
        _ => "low",
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

    ImpactResult {
        symbol_id: start_node.to_string(),
        symbol_name,
        change_type: change_type.to_string(),
        impacted,
        indirect_impacted,
        total_impacted,
        direct_impacted,
        risk_level: risk_level.to_string(),
        files_affected: affected_files.len(),
        breaking_changes,
        warnings,
        used_fallback: used_fallback_field,
        fallback_message,
    }
}
