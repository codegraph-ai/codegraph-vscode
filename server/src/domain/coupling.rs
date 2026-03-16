//! Module coupling analysis — transport-agnostic.
//!
//! Extracts analyze_coupling from MCP server. Uses domain::dependency_graph.

use crate::domain::dependency_graph::{self, DependencyGraphResult};
use codegraph::{CodeGraph, Direction, EdgeType, NodeId};
use serde::Serialize;
use std::collections::HashSet;

// ============================================================
// Response Types — MCP (dependency-graph based)
// ============================================================

/// Coupling metrics for a file.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct CouplingMetrics {
    pub afferent_coupling: usize,
    pub efferent_coupling: usize,
    pub instability: f64,
    pub total_dependencies: usize,
    pub total_connections: usize,
}

/// Result of `analyze_coupling`.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct CouplingResult {
    pub uri: String,
    pub metrics: CouplingMetrics,
    pub dependency_graph: DependencyGraphResult,
}

// ============================================================
// Response Types — LSP (symbol-level analysis)
// ============================================================

/// Coupling metrics derived from symbol-level edge traversal.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct FileCouplingMetrics {
    /// Incoming dependencies (how many external modules depend on this file)
    pub afferent: u32,
    /// Outgoing dependencies (how many external modules this file depends on)
    pub efferent: u32,
    /// Instability: Ce / (Ca + Ce), 0 = stable, 1 = unstable
    pub instability: f64,
    /// Names of modules that depend on this file
    pub dependents: Vec<String>,
    /// Names of modules this file depends on
    pub dependencies: Vec<String>,
}

/// Cohesion metrics for a file.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct FileCohesionMetrics {
    /// Cohesion score (0.0-1.0, higher is better)
    pub score: f64,
    /// Cohesion type: "functional", "sequential", or "coincidental"
    pub cohesion_type: String,
    /// Percentage of references that stay within the file
    pub internal_reference_ratio: f64,
}

/// An architectural violation detected during coupling analysis.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ArchViolation {
    pub violation_type: String,
    pub severity: String,
    pub description: String,
    pub suggestion: String,
}

/// Full coupling analysis result for LSP handlers.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct FileCouplingResult {
    pub coupling: FileCouplingMetrics,
    pub cohesion: FileCohesionMetrics,
    pub violations: Vec<ArchViolation>,
    pub recommendations: Vec<String>,
}

// ============================================================
// Domain Functions
// ============================================================

/// Compute coupling metrics for a file (MCP path).
///
/// Calls `dependency_graph::get_dependency_graph` then computes
/// afferent/efferent/instability metrics from the result.
pub(crate) fn analyze_coupling(
    graph: &CodeGraph,
    file_path: &str,
    uri: &str,
    depth: usize,
) -> CouplingResult {
    let dep_graph = dependency_graph::get_dependency_graph(graph, file_path, depth, "both");

    let node_count = dep_graph.nodes.len();
    let edge_count = dep_graph.edges.len();

    // Preserve original logic: all edges have both "from" and "to" fields,
    // so afferent == efferent == edge_count (mirrors original JSON-navigation behavior).
    let afferent = dep_graph.edges.iter().filter(|e| !e.to.is_empty()).count();
    let efferent = dep_graph
        .edges
        .iter()
        .filter(|e| !e.from.is_empty())
        .count();

    let instability = if afferent + efferent > 0 {
        efferent as f64 / (afferent + efferent) as f64
    } else {
        0.0
    };

    CouplingResult {
        uri: uri.to_string(),
        metrics: CouplingMetrics {
            afferent_coupling: afferent,
            efferent_coupling: efferent,
            instability,
            total_dependencies: node_count,
            total_connections: edge_count,
        },
        dependency_graph: dep_graph,
    }
}

/// Full coupling analysis for a file using its pre-resolved symbol nodes (LSP path).
///
/// Takes a set of node IDs for all symbols defined in the file (resolved via symbol_index).
/// Walks incoming/outgoing edges for each symbol to compute coupling, cohesion,
/// architectural violations, and recommendations.
pub(crate) fn analyze_coupling_for_file(
    graph: &CodeGraph,
    file_symbols: HashSet<NodeId>,
) -> FileCouplingResult {
    let mut dependents: Vec<String> = Vec::new();
    let mut dependencies: Vec<String> = Vec::new();
    let mut internal_refs = 0u32;
    let mut external_refs = 0u32;

    for &node_id in &file_symbols {
        // Outgoing edges — what symbols in this file call or import
        let out_neighbors = match graph.get_neighbors(node_id, Direction::Outgoing) {
            Ok(n) => n,
            Err(_) => continue,
        };
        for target in out_neighbors {
            if let Ok(edge_ids) = graph.get_edges_between(node_id, target) {
                for edge_id in edge_ids {
                    if let Ok(edge) = graph.get_edge(edge_id) {
                        if edge.edge_type == EdgeType::Imports || edge.edge_type == EdgeType::Calls
                        {
                            if file_symbols.contains(&target) {
                                internal_refs += 1;
                            } else {
                                external_refs += 1;
                                if let Ok(target_node) = graph.get_node(target) {
                                    if let Some(dep_path) =
                                        target_node.properties.get_string("path")
                                    {
                                        let dep_name = std::path::Path::new(dep_path)
                                            .file_stem()
                                            .and_then(|s| s.to_str())
                                            .unwrap_or("unknown")
                                            .to_string();
                                        if !dependencies.contains(&dep_name) {
                                            dependencies.push(dep_name);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Incoming edges — who calls or imports symbols in this file
        let in_neighbors = match graph.get_neighbors(node_id, Direction::Incoming) {
            Ok(n) => n,
            Err(_) => continue,
        };
        for source in in_neighbors {
            if file_symbols.contains(&source) {
                continue;
            }
            if let Ok(edge_ids) = graph.get_edges_between(source, node_id) {
                for edge_id in edge_ids {
                    if let Ok(edge) = graph.get_edge(edge_id) {
                        if edge.edge_type == EdgeType::Imports || edge.edge_type == EdgeType::Calls
                        {
                            if let Ok(source_node) = graph.get_node(source) {
                                if let Some(src_path) = source_node.properties.get_string("path") {
                                    let src_name = std::path::Path::new(src_path)
                                        .file_stem()
                                        .and_then(|s| s.to_str())
                                        .unwrap_or("unknown")
                                        .to_string();
                                    if !dependents.contains(&src_name) {
                                        dependents.push(src_name);
                                    }
                                }
                            }
                            break; // one matching edge is enough to register the dependent
                        }
                    }
                }
            }
        }
    }

    let afferent = dependents.len() as u32;
    let efferent = dependencies.len() as u32;
    let instability = if afferent + efferent > 0 {
        efferent as f64 / (afferent + efferent) as f64
    } else {
        0.0
    };

    let total_refs = internal_refs + external_refs;
    let internal_ratio = if total_refs > 0 {
        internal_refs as f64 / total_refs as f64
    } else {
        1.0
    };

    let cohesion_type = if internal_ratio > 0.7 {
        "functional"
    } else if internal_ratio > 0.4 {
        "sequential"
    } else {
        "coincidental"
    };

    let mut recommendations = Vec::new();
    let mut violations = Vec::new();

    if instability > 0.8 {
        recommendations.push(
            "High instability - this module depends on many others. Consider reducing dependencies."
                .to_string(),
        );
    }

    if instability < 0.2 && efferent > 5 {
        violations.push(ArchViolation {
            violation_type: "stable_dependency".to_string(),
            severity: "warning".to_string(),
            description: "Stable module has many outgoing dependencies".to_string(),
            suggestion: "Consider extracting dependencies to make module more focused".to_string(),
        });
    }

    if internal_ratio < 0.3 {
        recommendations.push(
            "Low cohesion - functions in this module don't reference each other much. Consider splitting."
                .to_string(),
        );
    }

    if afferent > 10 {
        recommendations.push(format!(
            "Many modules ({afferent}) depend on this one. Changes here have wide impact."
        ));
    }

    FileCouplingResult {
        coupling: FileCouplingMetrics {
            afferent,
            efferent,
            instability,
            dependents,
            dependencies,
        },
        cohesion: FileCohesionMetrics {
            score: internal_ratio,
            cohesion_type: cohesion_type.to_string(),
            internal_reference_ratio: internal_ratio,
        },
        violations,
        recommendations,
    }
}
