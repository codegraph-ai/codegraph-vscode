//! Code Metrics Handler - Complexity and quality analysis for AI assistants.

use crate::backend::CodeGraphBackend;
use crate::handlers::ai_context::LocationInfo;
use codegraph::{CodeGraph, Direction, EdgeType, NodeId, NodeType};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::Url;

// Re-export domain complexity types and functions so existing call sites are unaffected.
pub(crate) use crate::domain::complexity::{analyze_file_complexity, ComplexityDetails};

// ==========================================
// Complexity Analysis Types
// ==========================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComplexityParams {
    pub uri: String,
    /// Specific line to analyze (optional, analyzes whole file if not provided)
    pub line: Option<u32>,
    /// Complexity threshold for recommendations (default: 10)
    pub threshold: Option<u32>,
    /// Include detailed metrics breakdown
    pub include_metrics: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComplexityResponse {
    pub functions: Vec<FunctionComplexity>,
    pub file_summary: FileSummary,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FunctionComplexity {
    pub name: String,
    pub complexity: u32,
    pub grade: char,
    pub location: LocationInfo,
    pub details: ComplexityDetails,
}

// LocationInfo is imported from ai_context module
// ComplexityDetails, FunctionComplexityEntry, ComplexityAnalysisResult re-exported from domain::complexity above.

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSummary {
    pub total_functions: u32,
    pub average_complexity: f64,
    pub max_complexity: u32,
    pub functions_above_threshold: u32,
    pub overall_grade: char,
}

// ==========================================
// Dead Code Detection Types
// ==========================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnusedCodeParams {
    /// Specific file to analyze (optional)
    pub uri: Option<String>,
    /// Scope of analysis: "file", "module", or "workspace"
    pub scope: String,
    /// Include test files in analysis
    pub include_tests: Option<bool>,
    /// Minimum confidence threshold (0.0-1.0)
    pub confidence: Option<f64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnusedCodeResponse {
    pub unused_items: Vec<UnusedItem>,
    pub summary: UnusedSummary,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnusedItem {
    /// Type of unused item: "function", "class", "import", "export", "variable"
    pub item_type: String,
    pub name: String,
    pub location: LocationInfo,
    /// Confidence that this is truly unused (0.0-1.0)
    pub confidence: f64,
    /// Explanation of why this is considered unused
    pub reason: String,
    /// Whether it's safe to remove without breaking external consumers
    pub safe_to_remove: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnusedSummary {
    pub total_items: u32,
    pub by_type: UnusedByType,
    pub safe_deletions: u32,
    pub estimated_lines_removable: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnusedByType {
    pub functions: u32,
    pub classes: u32,
    pub imports: u32,
    pub variables: u32,
}

// ==========================================
// Coupling Analysis Types
// ==========================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CouplingParams {
    pub uri: String,
    /// Include external dependencies in analysis
    pub include_external: Option<bool>,
    /// Depth of dependency analysis
    pub depth: Option<u32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CouplingResponse {
    pub coupling: CouplingMetrics,
    pub cohesion: CohesionMetrics,
    pub violations: Vec<ArchViolation>,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CouplingMetrics {
    /// Incoming dependencies (who depends on this module)
    pub afferent: u32,
    /// Outgoing dependencies (what this module depends on)
    pub efferent: u32,
    /// Instability: Ce / (Ca + Ce), 0 = stable, 1 = unstable
    pub instability: f64,
    /// List of modules that depend on this one
    pub dependents: Vec<String>,
    /// List of modules this one depends on
    pub dependencies: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CohesionMetrics {
    /// Cohesion score (0.0-1.0, higher is better)
    pub score: f64,
    /// Type of cohesion detected
    pub cohesion_type: String,
    /// Percentage of internal references vs external
    pub internal_reference_ratio: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchViolation {
    pub violation_type: String,
    pub severity: String,
    pub description: String,
    pub suggestion: String,
}

// ==========================================
// LSP Handlers
// ==========================================

impl CodeGraphBackend {
    /// LSP handler — delegates to shared `analyze_file_complexity()`.
    pub async fn handle_analyze_complexity(
        &self,
        params: ComplexityParams,
    ) -> Result<ComplexityResponse> {
        let threshold = params.threshold.unwrap_or(10);
        let graph = self.graph.read().await;
        let file_nodes = self.get_file_node_ids(&graph, &params.uri)?;
        let result = analyze_file_complexity(&graph, &file_nodes, params.line, threshold);

        let mut functions = Vec::new();
        for entry in &result.functions {
            let location = self
                .node_to_location(&graph, entry.node_id)
                .map_err(|_| tower_lsp::jsonrpc::Error::internal_error())?;
            functions.push(FunctionComplexity {
                name: entry.name.clone(),
                complexity: entry.complexity,
                grade: entry.grade,
                location: LocationInfo {
                    uri: location.uri.to_string(),
                    range: location.range,
                },
                details: entry.details.clone(),
            });
        }

        Ok(ComplexityResponse {
            functions,
            file_summary: FileSummary {
                total_functions: result.functions.len() as u32,
                average_complexity: result.average_complexity,
                max_complexity: result.max_complexity,
                functions_above_threshold: result.functions_above_threshold,
                overall_grade: result.overall_grade,
            },
            recommendations: result.recommendations,
        })
    }

    /// Resolve file URI to node IDs via symbol index.
    fn get_file_node_ids(&self, _graph: &CodeGraph, uri_str: &str) -> Result<Vec<NodeId>> {
        let uri = Url::parse(uri_str)
            .map_err(|_| tower_lsp::jsonrpc::Error::invalid_params("Invalid URI"))?;
        let path = uri
            .to_file_path()
            .map_err(|_| tower_lsp::jsonrpc::Error::invalid_params("Invalid file path"))?;
        Ok(self.symbol_index.get_file_symbols(&path))
    }

    /// Find unused code in the codebase — delegates to shared domain::unused_code.
    pub async fn handle_find_unused_code(
        &self,
        params: UnusedCodeParams,
    ) -> Result<UnusedCodeResponse> {
        let min_confidence = params.confidence.unwrap_or(0.7);
        let include_tests = params.include_tests.unwrap_or(false);

        // Resolve URI to file path if provided
        let path = if let Some(uri) = &params.uri {
            let uri_parsed = Url::parse(uri)
                .map_err(|_| tower_lsp::jsonrpc::Error::invalid_params("Invalid URI"))?;
            let file_path = uri_parsed
                .to_file_path()
                .map_err(|_| tower_lsp::jsonrpc::Error::invalid_params("Invalid file path"))?;
            Some(file_path.to_string_lossy().to_string())
        } else {
            None
        };

        let domain_params = crate::domain::unused_code::FindUnusedCodeParams {
            path,
            scope: params.scope.clone(),
            include_tests,
            confidence: min_confidence,
        };

        let graph = self.graph.read().await;
        let result =
            crate::domain::unused_code::find_unused_code(&graph, &self.query_engine, domain_params)
                .await;

        let mut unused_items: Vec<UnusedItem> = Vec::new();
        let mut functions_count = 0u32;
        let mut classes_count = 0u32;
        let imports_count = 0u32;
        let variables_count = 0u32;
        let mut total_lines = 0u32;

        for candidate in &result.candidates {
            let lines = candidate.line_end.saturating_sub(candidate.line_start) + 1;

            let item_type = match candidate.node_type {
                NodeType::Function => "function",
                NodeType::Class => "class",
                NodeType::Variable => "variable",
                _ => "type",
            };

            let reason = if candidate.is_public {
                "Exported but no internal callers or importers found"
            } else {
                "No callers or importers found in codebase"
            };

            if let Ok(location) = self.node_to_location(&graph, candidate.node_id) {
                unused_items.push(UnusedItem {
                    item_type: item_type.to_string(),
                    name: candidate.name.clone(),
                    location: LocationInfo {
                        uri: location.uri.to_string(),
                        range: location.range,
                    },
                    confidence: candidate.confidence,
                    reason: reason.to_string(),
                    safe_to_remove: !candidate.is_public && candidate.confidence > 0.8,
                });

                match candidate.node_type {
                    NodeType::Function => functions_count += 1,
                    NodeType::Class => classes_count += 1,
                    _ => {}
                }

                if candidate.confidence > 0.8 {
                    total_lines += lines;
                }
            }
        }

        // Sort by confidence descending
        unused_items.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let safe_deletions = unused_items.iter().filter(|i| i.safe_to_remove).count() as u32;

        Ok(UnusedCodeResponse {
            unused_items,
            summary: UnusedSummary {
                total_items: functions_count + classes_count + imports_count + variables_count,
                by_type: UnusedByType {
                    functions: functions_count,
                    classes: classes_count,
                    imports: imports_count,
                    variables: variables_count,
                },
                safe_deletions,
                estimated_lines_removable: total_lines,
            },
        })
    }

    /// Analyze module coupling and cohesion
    pub async fn handle_analyze_coupling(
        &self,
        params: CouplingParams,
    ) -> Result<CouplingResponse> {
        let uri = Url::parse(&params.uri)
            .map_err(|_| tower_lsp::jsonrpc::Error::invalid_params("Invalid URI"))?;

        let path = uri
            .to_file_path()
            .map_err(|_| tower_lsp::jsonrpc::Error::invalid_params("Invalid file path"))?;

        let graph = self.graph.read().await;
        let _include_external = params.include_external.unwrap_or(false);

        // Get all symbols in this file
        let file_symbols: HashSet<NodeId> = self
            .symbol_index
            .get_file_symbols(&path)
            .into_iter()
            .collect();

        let mut dependents: Vec<String> = Vec::new();
        let mut dependencies: Vec<String> = Vec::new();
        let mut internal_refs = 0u32;
        let mut external_refs = 0u32;

        // Analyze each symbol in the file
        for node_id in &file_symbols {
            // Outgoing edges (what we depend on)
            let outgoing = self.get_connected_edges(&graph, *node_id, Direction::Outgoing);
            for (_, target, edge_type) in outgoing {
                if edge_type == EdgeType::Imports || edge_type == EdgeType::Calls {
                    if file_symbols.contains(&target) {
                        internal_refs += 1;
                    } else {
                        external_refs += 1;
                        // Get the file path of the dependency
                        if let Ok(target_node) = graph.get_node(target) {
                            if let Some(dep_path) = target_node.properties.get_string("path") {
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

            // Incoming edges (who depends on us)
            let incoming = self.get_connected_edges(&graph, *node_id, Direction::Incoming);
            for (source, _, edge_type) in incoming {
                if (edge_type == EdgeType::Imports || edge_type == EdgeType::Calls)
                    && !file_symbols.contains(&source)
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

        // Determine cohesion type
        let cohesion_type = if internal_ratio > 0.7 {
            "functional"
        } else if internal_ratio > 0.4 {
            "sequential"
        } else {
            "coincidental"
        };

        // Generate recommendations
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
                suggestion: "Consider extracting dependencies to make module more focused"
                    .to_string(),
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

        Ok(CouplingResponse {
            coupling: CouplingMetrics {
                afferent,
                efferent,
                instability,
                dependents,
                dependencies,
            },
            cohesion: CohesionMetrics {
                score: internal_ratio,
                cohesion_type: cohesion_type.to_string(),
                internal_reference_ratio: internal_ratio,
            },
            violations,
            recommendations,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::complexity::{complexity_grade, file_grade};

    #[test]
    fn test_complexity_grade() {
        assert_eq!(complexity_grade(1), 'A');
        assert_eq!(complexity_grade(5), 'A');
        assert_eq!(complexity_grade(6), 'B');
        assert_eq!(complexity_grade(10), 'B');
        assert_eq!(complexity_grade(11), 'C');
        assert_eq!(complexity_grade(20), 'C');
        assert_eq!(complexity_grade(21), 'D');
        assert_eq!(complexity_grade(50), 'D');
        assert_eq!(complexity_grade(51), 'F');
    }

    #[test]
    fn test_file_grade() {
        assert_eq!(file_grade(3.0), 'A');
        assert_eq!(file_grade(8.0), 'B');
        assert_eq!(file_grade(12.0), 'C');
        assert_eq!(file_grade(20.0), 'D');
        assert_eq!(file_grade(30.0), 'F');
    }

    // ==========================================
    // Unused Code Detection Tests
    // ==========================================

    mod unused_code_tests {
        use super::*;
        use codegraph::{CodeGraph, PropertyMap, PropertyValue};
        use std::sync::Arc;
        use tokio::sync::RwLock;

        /// Create a minimal backend for testing
        fn create_test_backend_with_graph(graph: Arc<RwLock<CodeGraph>>) -> CodeGraphBackend {
            use crate::ai_query::QueryEngine;
            let query_engine = Arc::new(QueryEngine::new(Arc::clone(&graph)));
            CodeGraphBackend::new_for_test(graph, query_engine)
        }

        #[tokio::test]
        async fn test_function_with_call_edge_is_not_unused() {
            // Create a graph with two functions where one calls the other
            let graph = Arc::new(RwLock::new(
                CodeGraph::in_memory().expect("Failed to create graph"),
            ));

            let (called_func_id, _caller_func_id) = {
                let mut g = graph.write().await;

                // Create a function that will be called
                let mut called_props = PropertyMap::new();
                called_props.insert(
                    "name".to_string(),
                    PropertyValue::String("myFunction".to_string()),
                );
                called_props.insert(
                    "path".to_string(),
                    PropertyValue::String("/src/utils.ts".to_string()),
                );
                called_props.insert("line_start".to_string(), PropertyValue::Int(10));
                called_props.insert("line_end".to_string(), PropertyValue::Int(20));
                let called_id = g.add_node(NodeType::Function, called_props).unwrap();

                // Create a function that calls myFunction
                let mut caller_props = PropertyMap::new();
                caller_props.insert(
                    "name".to_string(),
                    PropertyValue::String("caller".to_string()),
                );
                caller_props.insert(
                    "path".to_string(),
                    PropertyValue::String("/src/main.ts".to_string()),
                );
                caller_props.insert("line_start".to_string(), PropertyValue::Int(5));
                caller_props.insert("line_end".to_string(), PropertyValue::Int(15));
                let caller_id = g.add_node(NodeType::Function, caller_props).unwrap();

                // Create call edge: caller -> called
                g.add_edge(caller_id, called_id, EdgeType::Calls, PropertyMap::new())
                    .unwrap();

                (called_id, caller_id)
            };

            let backend = create_test_backend_with_graph(graph.clone());

            // Build indexes so get_callers() works in the domain function
            backend.query_engine.build_indexes().await;

            // Add the called function to the symbol index
            backend.symbol_index.add_node_for_test(
                std::path::PathBuf::from("/src/utils.ts"),
                called_func_id,
                "myFunction",
                "Function",
                10,
                20,
            );

            // Query for unused code
            let params = UnusedCodeParams {
                uri: Some("file:///src/utils.ts".to_string()),
                scope: "file".to_string(),
                include_tests: Some(false),
                confidence: Some(0.0), // Accept all confidence levels
            };

            let result = backend.handle_find_unused_code(params).await.unwrap();

            // myFunction should NOT be in unused items since it's called
            let unused_names: Vec<&str> = result
                .unused_items
                .iter()
                .map(|item| item.name.as_str())
                .collect();
            assert!(
                !unused_names.contains(&"myFunction"),
                "Called function should not be reported as unused"
            );
        }

        #[tokio::test]
        async fn test_function_with_import_edge_is_not_unused() {
            // Create a graph where a class is imported by another file
            let graph = Arc::new(RwLock::new(
                CodeGraph::in_memory().expect("Failed to create graph"),
            ));

            let imported_class_id = {
                let mut g = graph.write().await;

                // Create a class that will be imported
                let mut class_props = PropertyMap::new();
                class_props.insert(
                    "name".to_string(),
                    PropertyValue::String("MyClass".to_string()),
                );
                class_props.insert(
                    "path".to_string(),
                    PropertyValue::String("/src/myClass.ts".to_string()),
                );
                class_props.insert("line_start".to_string(), PropertyValue::Int(1));
                class_props.insert("line_end".to_string(), PropertyValue::Int(50));
                let class_id = g.add_node(NodeType::Class, class_props).unwrap();

                // Create a file node that imports the class
                let mut file_props = PropertyMap::new();
                file_props.insert(
                    "name".to_string(),
                    PropertyValue::String("extension".to_string()),
                );
                file_props.insert(
                    "path".to_string(),
                    PropertyValue::String("/src/extension.ts".to_string()),
                );
                let file_id = g.add_node(NodeType::CodeFile, file_props).unwrap();

                // Create import edge: extension.ts -> MyClass
                let mut import_props = PropertyMap::new();
                import_props.insert(
                    "imported_symbol".to_string(),
                    PropertyValue::String("MyClass".to_string()),
                );
                g.add_edge(file_id, class_id, EdgeType::Imports, import_props)
                    .unwrap();

                class_id
            };

            let backend = create_test_backend_with_graph(graph.clone());

            // Add the class to the symbol index
            backend.symbol_index.add_node_for_test(
                std::path::PathBuf::from("/src/myClass.ts"),
                imported_class_id,
                "MyClass",
                "Class",
                1,
                50,
            );

            // Query for unused code
            let params = UnusedCodeParams {
                uri: Some("file:///src/myClass.ts".to_string()),
                scope: "file".to_string(),
                include_tests: Some(false),
                confidence: Some(0.0), // Accept all confidence levels
            };

            let result = backend.handle_find_unused_code(params).await.unwrap();

            // MyClass should NOT be in unused items since it's imported
            let unused_names: Vec<&str> = result
                .unused_items
                .iter()
                .map(|item| item.name.as_str())
                .collect();
            assert!(
                !unused_names.contains(&"MyClass"),
                "Imported class should not be reported as unused. Found: {unused_names:?}"
            );
        }

        #[tokio::test]
        async fn test_truly_unused_function_is_reported() {
            // Create a graph with a function that has no incoming edges
            let graph = Arc::new(RwLock::new(
                CodeGraph::in_memory().expect("Failed to create graph"),
            ));

            let unused_func_id = {
                let mut g = graph.write().await;

                // Create a function with no callers or importers
                let mut func_props = PropertyMap::new();
                func_props.insert(
                    "name".to_string(),
                    PropertyValue::String("unusedHelper".to_string()),
                );
                func_props.insert(
                    "path".to_string(),
                    PropertyValue::String("/src/helpers.ts".to_string()),
                );
                func_props.insert("line_start".to_string(), PropertyValue::Int(5));
                func_props.insert("line_end".to_string(), PropertyValue::Int(15));
                g.add_node(NodeType::Function, func_props).unwrap()
            };

            let backend = create_test_backend_with_graph(graph.clone());

            // Add the function to the symbol index
            backend.symbol_index.add_node_for_test(
                std::path::PathBuf::from("/src/helpers.ts"),
                unused_func_id,
                "unusedHelper",
                "Function",
                5,
                15,
            );

            // Query for unused code
            let params = UnusedCodeParams {
                uri: Some("file:///src/helpers.ts".to_string()),
                scope: "file".to_string(),
                include_tests: Some(false),
                confidence: Some(0.0), // Accept all confidence levels
            };

            let result = backend.handle_find_unused_code(params).await.unwrap();

            // unusedHelper SHOULD be in unused items since it has no edges
            let unused_names: Vec<&str> = result
                .unused_items
                .iter()
                .map(|item| item.name.as_str())
                .collect();
            assert!(
                unused_names.contains(&"unusedHelper"),
                "Truly unused function should be reported as unused. Found: {unused_names:?}"
            );
        }

        #[tokio::test]
        async fn test_function_with_both_call_and_import_is_not_unused() {
            // Create a graph where a function is both called and imported
            let graph = Arc::new(RwLock::new(
                CodeGraph::in_memory().expect("Failed to create graph"),
            ));

            let popular_func_id = {
                let mut g = graph.write().await;

                // Create a popular function
                let mut func_props = PropertyMap::new();
                func_props.insert(
                    "name".to_string(),
                    PropertyValue::String("popularFunction".to_string()),
                );
                func_props.insert(
                    "path".to_string(),
                    PropertyValue::String("/src/utils.ts".to_string()),
                );
                func_props.insert("line_start".to_string(), PropertyValue::Int(1));
                func_props.insert("line_end".to_string(), PropertyValue::Int(10));
                let func_id = g.add_node(NodeType::Function, func_props).unwrap();

                // Create a file that imports it
                let mut file_props = PropertyMap::new();
                file_props.insert(
                    "name".to_string(),
                    PropertyValue::String("consumer1".to_string()),
                );
                let file_id = g.add_node(NodeType::CodeFile, file_props).unwrap();
                g.add_edge(file_id, func_id, EdgeType::Imports, PropertyMap::new())
                    .unwrap();

                // Create another function that calls it
                let mut caller_props = PropertyMap::new();
                caller_props.insert(
                    "name".to_string(),
                    PropertyValue::String("caller".to_string()),
                );
                let caller_id = g.add_node(NodeType::Function, caller_props).unwrap();
                g.add_edge(caller_id, func_id, EdgeType::Calls, PropertyMap::new())
                    .unwrap();

                func_id
            };

            let backend = create_test_backend_with_graph(graph.clone());

            // Add the function to the symbol index
            backend.symbol_index.add_node_for_test(
                std::path::PathBuf::from("/src/utils.ts"),
                popular_func_id,
                "popularFunction",
                "Function",
                1,
                10,
            );

            // Query for unused code
            let params = UnusedCodeParams {
                uri: Some("file:///src/utils.ts".to_string()),
                scope: "file".to_string(),
                include_tests: Some(false),
                confidence: Some(0.0),
            };

            let result = backend.handle_find_unused_code(params).await.unwrap();

            let unused_names: Vec<&str> = result
                .unused_items
                .iter()
                .map(|item| item.name.as_str())
                .collect();
            assert!(
                !unused_names.contains(&"popularFunction"),
                "Function with both call and import edges should not be unused"
            );
        }
    }
}
