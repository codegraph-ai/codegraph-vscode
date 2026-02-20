//! AI Query Engine
//!
//! Main query engine that provides fast, composable query primitives for AI agents.
//! Integrates with CodeGraph for graph-based code intelligence.

use super::primitives::{
    truncate_string, CallInfo, DetailedSymbolInfo, EntryPoint, EntryType, ImportMatchMode,
    ImportSearchOptions, SearchOptions, SignaturePattern, SymbolInfo, SymbolLocation, SymbolMatch,
    SymbolSearchResult, SymbolType, TraversalDirection, TraversalFilter, TraversalNode,
    MAX_SIGNATURE_LENGTH,
};
use super::text_index::{TextIndex, TextIndexBuilder};
use codegraph::{CodeGraph, Direction, EdgeType, NodeId, NodeType};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

/// AI Query Engine for fast code exploration.
pub struct QueryEngine {
    /// Reference to the code graph
    graph: Arc<RwLock<CodeGraph>>,
    /// Text index for fast symbol search
    text_index: Arc<RwLock<TextIndex>>,
    /// Import index: library name -> importing files
    import_index: Arc<RwLock<HashMap<String, Vec<NodeId>>>>,
    /// Caller index: function -> list of callers
    caller_index: Arc<RwLock<HashMap<NodeId, Vec<NodeId>>>>,
    /// Callee index: function -> list of callees
    callee_index: Arc<RwLock<HashMap<NodeId, Vec<NodeId>>>>,
}

impl QueryEngine {
    /// Create a new query engine with the given graph.
    pub fn new(graph: Arc<RwLock<CodeGraph>>) -> Self {
        Self {
            graph,
            text_index: Arc::new(RwLock::new(TextIndex::new())),
            import_index: Arc::new(RwLock::new(HashMap::new())),
            caller_index: Arc::new(RwLock::new(HashMap::new())),
            callee_index: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Build indexes from the current graph state.
    /// Should be called after initial parsing or reindexing.
    pub async fn build_indexes(&self) {
        let graph = self.graph.read().await;

        // Build text index
        let mut text_builder = TextIndexBuilder::new();
        let mut import_map: HashMap<String, Vec<NodeId>> = HashMap::new();
        let mut caller_map: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
        let mut callee_map: HashMap<NodeId, Vec<NodeId>> = HashMap::new();

        // Iterate over all nodes using iter_nodes()
        for (node_id, node) in graph.iter_nodes() {
            let name = node.properties.get_string("name").unwrap_or("").to_string();
            let docstring = node.properties.get_string("doc").map(|s| s.to_string());

            // Add to text index
            text_builder.add_document(node_id, &name, docstring.as_deref(), &[]);

            // Build import index from Imports edges
            if let Ok(neighbors) = graph.get_neighbors(node_id, Direction::Outgoing) {
                for neighbor_id in neighbors {
                    if let Ok(edges) = graph.get_edges_between(node_id, neighbor_id) {
                        for edge_id in edges {
                            if let Ok(edge) = graph.get_edge(edge_id) {
                                match edge.edge_type {
                                    EdgeType::Imports => {
                                        // Get the imported module name
                                        if let Ok(target_node) = graph.get_node(neighbor_id) {
                                            let module_name = target_node
                                                .properties
                                                .get_string("name")
                                                .unwrap_or("")
                                                .to_string();
                                            if !module_name.is_empty() {
                                                import_map
                                                    .entry(module_name)
                                                    .or_default()
                                                    .push(node_id);
                                            }
                                        }
                                    }
                                    EdgeType::Calls => {
                                        // Build caller/callee indexes
                                        callee_map.entry(node_id).or_default().push(neighbor_id);
                                        caller_map.entry(neighbor_id).or_default().push(node_id);
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
        }

        // Store built indexes
        *self.text_index.write().await = text_builder.build();
        *self.import_index.write().await = import_map;
        *self.caller_index.write().await = caller_map;
        *self.callee_index.write().await = callee_map;
    }

    /// Search for symbols by name, docstring, or comments.
    /// Returns results sorted by BM25 relevance.
    pub async fn symbol_search(&self, query: &str, options: &SearchOptions) -> SymbolSearchResult {
        let start = Instant::now();

        let text_index = self.text_index.read().await;
        let graph = self.graph.read().await;

        let text_results = text_index.search(query, options.limit * 2); // Get more than limit for filtering
        let total_matches = text_results.len();

        let mut results = Vec::new();
        for text_result in &text_results {
            if let Ok(node) = graph.get_node(text_result.node_id) {
                // Apply symbol type filter
                if !options.symbol_types.is_empty() {
                    let node_type_matches = options.symbol_types.iter().any(|st| {
                        matches!(
                            (st, &node.node_type),
                            (SymbolType::Function, NodeType::Function)
                                | (SymbolType::Class, NodeType::Class)
                                | (SymbolType::Variable, NodeType::Variable)
                                | (SymbolType::Module, NodeType::Module)
                                | (SymbolType::Interface, NodeType::Interface)
                                | (SymbolType::Type, NodeType::Type)
                        )
                    });
                    if !node_type_matches {
                        continue;
                    }
                }

                // Build symbol info (use compact mode if requested)
                let symbol_info =
                    self.node_to_symbol_info_opts(&graph, text_result.node_id, options.compact);
                if let Some(symbol) = symbol_info {
                    // Apply visibility filter
                    if !options.include_private && !symbol.is_public {
                        continue;
                    }

                    results.push(SymbolMatch {
                        node_id: text_result.node_id,
                        symbol,
                        score: text_result.score,
                        match_reason: format!("{:?}", text_result.match_reason),
                    });
                }
            }

            if results.len() >= options.limit {
                break;
            }
        }

        let query_time_ms = start.elapsed().as_millis() as u64;

        SymbolSearchResult {
            results,
            total_matches,
            query_time_ms,
        }
    }

    /// Find code by imported libraries/modules.
    pub async fn find_by_imports(
        &self,
        library: &str,
        options: &ImportSearchOptions,
    ) -> Vec<SymbolMatch> {
        let import_index = self.import_index.read().await;
        let graph = self.graph.read().await;

        let mut matching_nodes = Vec::new();

        match options.match_mode {
            ImportMatchMode::Exact => {
                if let Some(nodes) = import_index.get(library) {
                    matching_nodes.extend(nodes.iter().copied());
                }
            }
            ImportMatchMode::Prefix => {
                for (module, nodes) in import_index.iter() {
                    if module.starts_with(library) {
                        matching_nodes.extend(nodes.iter().copied());
                    }
                }
            }
            ImportMatchMode::Fuzzy => {
                let library_lower = library.to_lowercase();
                for (module, nodes) in import_index.iter() {
                    if module.to_lowercase().contains(&library_lower) {
                        matching_nodes.extend(nodes.iter().copied());
                    }
                }
            }
        }

        // Convert to SymbolMatch
        matching_nodes
            .into_iter()
            .filter_map(|node_id| {
                self.node_to_symbol_info(&graph, node_id)
                    .map(|symbol| SymbolMatch {
                        node_id,
                        symbol,
                        score: 1.0, // No ranking for import-based search
                        match_reason: format!("imports {library}"),
                    })
            })
            .collect()
    }

    /// Get direct callers of a function.
    pub async fn get_callers(&self, node_id: NodeId, depth: u32) -> Vec<CallInfo> {
        let caller_index = self.caller_index.read().await;
        let graph = self.graph.read().await;

        self.get_call_chain(&graph, &caller_index, node_id, depth)
    }

    /// Get direct callees of a function.
    pub async fn get_callees(&self, node_id: NodeId, depth: u32) -> Vec<CallInfo> {
        let callee_index = self.callee_index.read().await;
        let graph = self.graph.read().await;

        self.get_call_chain(&graph, &callee_index, node_id, depth)
    }

    /// Traverse the graph from a starting node with filters.
    pub async fn traverse_graph(
        &self,
        start_node: NodeId,
        direction: TraversalDirection,
        max_depth: u32,
        filter: &TraversalFilter,
    ) -> Vec<TraversalNode> {
        let graph = self.graph.read().await;
        let mut results = Vec::new();
        let mut visited = HashSet::new();
        let mut queue: VecDeque<(NodeId, u32, Vec<NodeId>, String)> = VecDeque::new();

        queue.push_back((start_node, 0, vec![start_node], String::new()));
        visited.insert(start_node);

        while let Some((current, depth, path, incoming_edge_type)) = queue.pop_front() {
            if depth > max_depth || results.len() >= filter.max_nodes {
                break;
            }

            // Skip the start node in results
            if depth > 0 {
                if let Ok(node) = graph.get_node(current) {
                    // Apply type filter
                    if !filter.symbol_types.is_empty() {
                        let type_matches = filter.symbol_types.iter().any(|st| {
                            matches!(
                                (st, &node.node_type),
                                (SymbolType::Function, NodeType::Function)
                                    | (SymbolType::Class, NodeType::Class)
                                    | (SymbolType::Variable, NodeType::Variable)
                                    | (SymbolType::Module, NodeType::Module)
                                    | (SymbolType::Interface, NodeType::Interface)
                                    | (SymbolType::Type, NodeType::Type)
                            )
                        });
                        if !type_matches {
                            continue;
                        }
                    }

                    if let Some(symbol) = self.node_to_symbol_info(&graph, current) {
                        results.push(TraversalNode {
                            node_id: current,
                            depth,
                            path: path.clone(),
                            edge_type: incoming_edge_type.clone(),
                            symbol,
                        });
                    }
                }
            }

            // Get neighbors based on direction
            let codegraph_direction = match direction {
                TraversalDirection::Outgoing => Direction::Outgoing,
                TraversalDirection::Incoming => Direction::Incoming,
                TraversalDirection::Both => Direction::Both,
            };

            if let Ok(neighbors) = graph.get_neighbors(current, codegraph_direction) {
                for neighbor in neighbors {
                    if !visited.contains(&neighbor) {
                        visited.insert(neighbor);
                        let mut new_path = path.clone();
                        new_path.push(neighbor);

                        // Resolve the edge type between current and neighbor
                        let edge_type_str = graph
                            .get_edges_between(current, neighbor)
                            .ok()
                            .and_then(|edges| edges.into_iter().next())
                            .and_then(|eid| graph.get_edge(eid).ok())
                            .map(|e| e.edge_type.to_string())
                            .unwrap_or_default();

                        queue.push_back((neighbor, depth + 1, new_path, edge_type_str));
                    }
                }
            }
        }

        results
    }

    /// Get detailed information about a symbol.
    pub async fn get_symbol_info(&self, node_id: NodeId) -> Option<DetailedSymbolInfo> {
        let graph = self.graph.read().await;
        let caller_index = self.caller_index.read().await;
        let callee_index = self.callee_index.read().await;

        let node = graph.get_node(node_id).ok()?;
        let symbol = self.node_to_symbol_info(&graph, node_id)?;

        // Get callers and callees
        let callers = self.get_call_chain(&graph, &caller_index, node_id, 1);
        let callees = self.get_call_chain(&graph, &callee_index, node_id, 1);

        // Count references
        let reference_count = graph
            .get_neighbors(node_id, Direction::Incoming)
            .map(|n| n.len())
            .unwrap_or(0);

        // Get complexity if available
        let complexity = node.properties.get_int("complexity").map(|c| c as u32);

        // Get lines of code
        let lines_of_code = {
            let start_line = node.properties.get_int("line_start").unwrap_or(0);
            let end_line = node.properties.get_int("line_end").unwrap_or(start_line);
            (end_line - start_line + 1) as usize
        };

        // Check if public
        let is_public = node
            .properties
            .get_bool("is_public")
            .or_else(|| node.properties.get_bool("exported"))
            .unwrap_or(true);

        // Check if deprecated
        let is_deprecated = node.properties.get_bool("deprecated").unwrap_or(false);

        // Collect dependencies (outgoing import edges)
        let mut dependencies = Vec::new();
        if let Ok(neighbors) = graph.get_neighbors(node_id, Direction::Outgoing) {
            for neighbor_id in neighbors {
                if let Ok(edges) = graph.get_edges_between(node_id, neighbor_id) {
                    let is_import = edges.iter().any(|eid| {
                        graph.get_edge(*eid).is_ok_and(|e| {
                            matches!(e.edge_type, EdgeType::Imports | EdgeType::ImportsFrom)
                        })
                    });
                    if is_import {
                        if let Ok(target) = graph.get_node(neighbor_id) {
                            let name = target
                                .properties
                                .get_string("name")
                                .unwrap_or("")
                                .to_string();
                            if !name.is_empty() && !dependencies.contains(&name) {
                                dependencies.push(name);
                            }
                        }
                    }
                }
            }
        }

        // Collect dependents (incoming import edges)
        let mut dependents = Vec::new();
        if let Ok(neighbors) = graph.get_neighbors(node_id, Direction::Incoming) {
            for neighbor_id in neighbors {
                if let Ok(edges) = graph.get_edges_between(neighbor_id, node_id) {
                    let is_import = edges.iter().any(|eid| {
                        graph.get_edge(*eid).is_ok_and(|e| {
                            matches!(e.edge_type, EdgeType::Imports | EdgeType::ImportsFrom)
                        })
                    });
                    if is_import {
                        if let Ok(source) = graph.get_node(neighbor_id) {
                            let name = source
                                .properties
                                .get_string("name")
                                .unwrap_or("")
                                .to_string();
                            if !name.is_empty() && !dependents.contains(&name) {
                                dependents.push(name);
                            }
                        }
                    }
                }
            }
        }

        // Detect test associations by checking if any caller is a test node
        let has_tests = callers.iter().any(|caller| {
            graph.get_node(caller.node_id).is_ok_and(|n| {
                let name = n.properties.get_string("name").unwrap_or("");
                let path = n.properties.get_string("path").unwrap_or("");
                name.starts_with("test_")
                    || name.ends_with("_test")
                    || name.contains("test ")
                    || path.contains("/test")
                    || path.contains("/tests")
            })
        });

        Some(DetailedSymbolInfo {
            symbol,
            callers,
            callees,
            dependencies,
            dependents,
            complexity,
            lines_of_code,
            has_tests,
            is_public,
            is_deprecated,
            reference_count,
        })
    }

    /// Find functions by signature patterns.
    pub async fn find_by_signature(
        &self,
        pattern: &SignaturePattern,
        limit: Option<usize>,
    ) -> Vec<SymbolMatch> {
        let graph = self.graph.read().await;
        let mut results = Vec::new();

        // Compile regex if name pattern is provided
        let name_regex = pattern
            .name_pattern
            .as_ref()
            .and_then(|p| regex::Regex::new(p).ok());

        // Iterate over all function nodes using iter_nodes()
        for (node_id, node) in graph.iter_nodes() {
            // Only check functions
            if node.node_type != NodeType::Function {
                continue;
            }

            let name = node.properties.get_string("name").unwrap_or("");

            // Check name pattern
            if let Some(ref regex) = name_regex {
                if !regex.is_match(name) {
                    continue;
                }
            }

            // Check return type
            if let Some(ref expected_return) = pattern.return_type {
                let actual_return = node.properties.get_string("return_type").unwrap_or("");
                if !self.type_matches(actual_return, expected_return) {
                    continue;
                }
            }

            // Check parameter count
            if let Some((min, max)) = pattern.param_count {
                let param_count = node.properties.get_int("param_count").unwrap_or(0) as usize;
                if param_count < min || param_count > max {
                    continue;
                }
            }

            // Check modifiers
            if !pattern.modifiers.is_empty() {
                let mut all_modifiers_match = true;
                for modifier in &pattern.modifiers {
                    let has_modifier = match modifier.as_str() {
                        "async" => node.properties.get_bool("is_async").unwrap_or(false),
                        "public" | "pub" => node
                            .properties
                            .get_bool("is_public")
                            .or_else(|| node.properties.get_bool("exported"))
                            .unwrap_or(false),
                        "private" => !node
                            .properties
                            .get_bool("is_public")
                            .or_else(|| node.properties.get_bool("exported"))
                            .unwrap_or(true),
                        "static" => node.properties.get_bool("is_static").unwrap_or(false),
                        "const" => node.properties.get_bool("is_const").unwrap_or(false),
                        _ => false,
                    };
                    if !has_modifier {
                        all_modifiers_match = false;
                        break;
                    }
                }
                if !all_modifiers_match {
                    continue;
                }
            }

            // Build symbol info for matching function
            if let Some(symbol) = self.node_to_symbol_info(&graph, node_id) {
                let match_reason = self.build_signature_match_reason(pattern);
                results.push(SymbolMatch {
                    node_id,
                    symbol,
                    score: 1.0, // All matches are equally relevant for signature search
                    match_reason,
                });

                // Check limit and return early if reached
                if let Some(max) = limit {
                    if results.len() >= max {
                        return results;
                    }
                }
            }
        }

        results
    }

    /// Check if actual type matches expected type pattern.
    fn type_matches(&self, actual: &str, expected: &str) -> bool {
        // Handle exact match
        if actual == expected {
            return true;
        }

        // Handle primitive type aliases
        let actual_normalized = match actual.to_lowercase().as_str() {
            "boolean" => "bool",
            "integer" | "int" | "i32" | "i64" => "int",
            "string" | "str" | "&str" => "string",
            "void" | "()" | "none" | "null" => "void",
            _ => actual,
        };

        let expected_normalized = match expected.to_lowercase().as_str() {
            "boolean" => "bool",
            "integer" | "int" | "i32" | "i64" => "int",
            "string" | "str" | "&str" => "string",
            "void" | "()" | "none" | "null" => "void",
            _ => expected,
        };

        if actual_normalized == expected_normalized {
            return true;
        }

        // Handle wildcard patterns (e.g., "Result<*, *>")
        if expected.contains('*') {
            let pattern = expected.replace('*', ".*");
            if let Ok(regex) = regex::Regex::new(&format!("^{pattern}$")) {
                return regex.is_match(actual);
            }
        }

        false
    }

    /// Build a human-readable match reason for signature search.
    fn build_signature_match_reason(&self, pattern: &SignaturePattern) -> String {
        let mut parts = Vec::new();

        if let Some(ref name_pattern) = pattern.name_pattern {
            parts.push(format!("name matches /{name_pattern}/"));
        }
        if let Some(ref return_type) = pattern.return_type {
            parts.push(format!("returns {return_type}"));
        }
        if let Some((min, max)) = pattern.param_count {
            if min == max {
                parts.push(format!("{min} parameters"));
            } else {
                parts.push(format!("{min}-{max} parameters"));
            }
        }
        if !pattern.modifiers.is_empty() {
            let mods = pattern.modifiers.join(", ");
            parts.push(format!("modifiers: {mods}"));
        }

        if parts.is_empty() {
            "Signature match".to_string()
        } else {
            parts.join(", ")
        }
    }

    /// Find entry points in the codebase.
    pub async fn find_entry_points(&self, entry_types: &[EntryType]) -> Vec<EntryPoint> {
        self.find_entry_points_opts(entry_types, false, None).await
    }

    /// Find entry points with compact option and optional limit.
    pub async fn find_entry_points_opts(
        &self,
        entry_types: &[EntryType],
        compact: bool,
        limit: Option<usize>,
    ) -> Vec<EntryPoint> {
        let graph = self.graph.read().await;
        let mut results = Vec::new();

        // Iterate over all nodes using iter_nodes()
        for (node_id, node) in graph.iter_nodes() {
            // Only check functions
            if node.node_type != NodeType::Function {
                continue;
            }

            let name = node.properties.get_string("name").unwrap_or("");

            // Detect entry type
            let entry_type = self.detect_entry_type(node, name);

            if let Some(et) = entry_type {
                // Filter by requested entry types
                if entry_types.is_empty() || entry_types.contains(&et) {
                    if let Some(symbol) = self.node_to_symbol_info_opts(&graph, node_id, compact) {
                        // In compact mode, also truncate description
                        let description = if compact {
                            None
                        } else {
                            node.properties
                                .get_string("doc")
                                .map(|s| truncate_string(s, MAX_SIGNATURE_LENGTH))
                        };

                        results.push(EntryPoint {
                            node_id,
                            entry_type: et,
                            route: node.properties.get_string("route").map(|s| s.to_string()),
                            method: node
                                .properties
                                .get_string("http_method")
                                .map(|s| s.to_string()),
                            description,
                            symbol,
                        });

                        // Check limit and return early if reached
                        if let Some(max) = limit {
                            if results.len() >= max {
                                return results;
                            }
                        }
                    }
                }
            }
        }

        results
    }

    // Helper methods

    /// Convert a node to SymbolInfo with default options (truncated signatures)
    fn node_to_symbol_info(&self, graph: &CodeGraph, node_id: NodeId) -> Option<SymbolInfo> {
        self.node_to_symbol_info_opts(graph, node_id, false)
    }

    /// Convert a node to SymbolInfo with options
    /// - compact: if true, omit signature and docstring entirely
    /// - if false, truncate signature to MAX_SIGNATURE_LENGTH
    fn node_to_symbol_info_opts(
        &self,
        graph: &CodeGraph,
        node_id: NodeId,
        compact: bool,
    ) -> Option<SymbolInfo> {
        let node = graph.get_node(node_id).ok()?;

        let name = node.properties.get_string("name")?.to_string();
        let kind = format!("{}", node.node_type);

        // Support both property name conventions
        let line = node
            .properties
            .get_int("line_start")
            .or_else(|| node.properties.get_int("start_line"))
            .unwrap_or(1) as u32;
        let column = node
            .properties
            .get_int("col_start")
            .or_else(|| node.properties.get_int("start_col"))
            .unwrap_or(0) as u32;
        let end_line = node
            .properties
            .get_int("line_end")
            .or_else(|| node.properties.get_int("end_line"))
            .unwrap_or(line as i64) as u32;
        let end_column = node
            .properties
            .get_int("col_end")
            .or_else(|| node.properties.get_int("end_col"))
            .unwrap_or(0) as u32;

        let file = node.properties.get_string("path").unwrap_or("").to_string();

        let location = SymbolLocation {
            file,
            line,
            column,
            end_line,
            end_column,
        };

        // In compact mode, omit signature and docstring
        // Otherwise, truncate signature to prevent huge responses
        let (signature, docstring) = if compact {
            (None, None)
        } else {
            let sig = node
                .properties
                .get_string("signature")
                .map(|s| truncate_string(s, MAX_SIGNATURE_LENGTH));
            let doc = node
                .properties
                .get_string("doc")
                .map(|s| truncate_string(s, MAX_SIGNATURE_LENGTH));
            (sig, doc)
        };

        let is_public = node
            .properties
            .get_bool("is_public")
            .or_else(|| node.properties.get_bool("exported"))
            .unwrap_or(true);

        Some(SymbolInfo {
            name,
            kind,
            location,
            signature,
            docstring,
            is_public,
        })
    }

    fn get_call_chain(
        &self,
        graph: &CodeGraph,
        index: &HashMap<NodeId, Vec<NodeId>>,
        start: NodeId,
        max_depth: u32,
    ) -> Vec<CallInfo> {
        let mut results = Vec::new();
        let mut visited = HashSet::new();
        let mut queue: VecDeque<(NodeId, u32)> = VecDeque::new();

        if let Some(direct) = index.get(&start) {
            for &node_id in direct {
                queue.push_back((node_id, 1));
            }
        }

        while let Some((current, depth)) = queue.pop_front() {
            if depth > max_depth || visited.contains(&current) {
                continue;
            }
            visited.insert(current);

            if let Some(symbol) = self.node_to_symbol_info(graph, current) {
                results.push(CallInfo {
                    node_id: current,
                    symbol: symbol.clone(),
                    call_site: symbol.location.clone(),
                    depth,
                });
            }

            // Continue to next depth if needed
            if depth < max_depth {
                if let Some(next_level) = index.get(&current) {
                    for &node_id in next_level {
                        if !visited.contains(&node_id) {
                            queue.push_back((node_id, depth + 1));
                        }
                    }
                }
            }
        }

        results
    }

    fn detect_entry_type(&self, node: &codegraph::Node, name: &str) -> Option<EntryType> {
        let name_lower = name.to_lowercase();

        // Check for HTTP handlers
        if node.properties.get_string("route").is_some()
            || node.properties.get_string("http_method").is_some()
        {
            return Some(EntryType::HttpHandler);
        }

        // Check for main function
        if name == "main" || name == "__main__" {
            return Some(EntryType::Main);
        }

        // Check for test functions
        if name_lower.starts_with("test_")
            || name_lower.starts_with("test")
            || node.properties.get_bool("is_test").unwrap_or(false)
        {
            return Some(EntryType::TestEntry);
        }

        // Check for CLI commands
        if name_lower.contains("command")
            || name_lower.contains("cli")
            || node.properties.get_bool("is_cli").unwrap_or(false)
        {
            return Some(EntryType::CliCommand);
        }

        // Check for event handlers
        if name_lower.starts_with("on_")
            || name_lower.starts_with("handle_")
            || name_lower.ends_with("_handler")
            || name_lower.ends_with("_callback")
        {
            return Some(EntryType::EventHandler);
        }

        // Check for public API (exported functions)
        if node.properties.get_bool("exported").unwrap_or(false)
            || node.properties.get_bool("is_public").unwrap_or(false)
        {
            return Some(EntryType::PublicApi);
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codegraph::PropertyMap;

    async fn create_test_engine() -> (QueryEngine, Arc<RwLock<CodeGraph>>) {
        let graph = Arc::new(RwLock::new(
            CodeGraph::in_memory().expect("Failed to create in-memory graph"),
        ));
        let engine = QueryEngine::new(Arc::clone(&graph));
        (engine, graph)
    }

    #[tokio::test]
    async fn test_engine_creation() {
        let (engine, _) = create_test_engine().await;
        // Engine should be created successfully
        let text_index = engine.text_index.read().await;
        assert_eq!(text_index.document_count(), 0);
    }

    #[tokio::test]
    async fn test_symbol_search_empty() {
        let (engine, _) = create_test_engine().await;

        let results = engine.symbol_search("test", &SearchOptions::new()).await;

        assert_eq!(results.results.len(), 0);
        assert_eq!(results.total_matches, 0);
    }

    #[tokio::test]
    async fn test_symbol_search_with_data() {
        let (engine, graph) = create_test_engine().await;

        // Add a function node to the graph
        {
            let mut g = graph.write().await;
            let mut props = PropertyMap::new();
            props.insert(
                "name".to_string(),
                codegraph::PropertyValue::String("validateEmail".to_string()),
            );
            props.insert(
                "path".to_string(),
                codegraph::PropertyValue::String("/src/test.rs".to_string()),
            );
            props.insert("line_start".to_string(), codegraph::PropertyValue::Int(10));
            props.insert("line_end".to_string(), codegraph::PropertyValue::Int(20));

            let node_id = g
                .add_node(NodeType::Function, props)
                .expect("Failed to add node");

            // Node ID is always valid (can be 0 for first node)
            let _ = node_id;
        }

        // Build indexes
        engine.build_indexes().await;

        // Search should find the function
        let results = engine
            .symbol_search("validate", &SearchOptions::new())
            .await;

        assert_eq!(results.results.len(), 1);
        assert_eq!(results.results[0].symbol.name, "validateEmail");
    }

    #[tokio::test]
    async fn test_symbol_search_with_type_filter() {
        let (engine, graph) = create_test_engine().await;

        // Add a function and a class
        {
            let mut g = graph.write().await;

            let mut func_props = PropertyMap::new();
            func_props.insert(
                "name".to_string(),
                codegraph::PropertyValue::String("processData".to_string()),
            );
            func_props.insert(
                "path".to_string(),
                codegraph::PropertyValue::String("/src/test.rs".to_string()),
            );
            func_props.insert("line_start".to_string(), codegraph::PropertyValue::Int(1));
            g.add_node(NodeType::Function, func_props)
                .expect("Failed to add function");

            let mut class_props = PropertyMap::new();
            class_props.insert(
                "name".to_string(),
                codegraph::PropertyValue::String("DataProcessor".to_string()),
            );
            class_props.insert(
                "path".to_string(),
                codegraph::PropertyValue::String("/src/test.rs".to_string()),
            );
            class_props.insert("line_start".to_string(), codegraph::PropertyValue::Int(10));
            g.add_node(NodeType::Class, class_props)
                .expect("Failed to add class");
        }

        engine.build_indexes().await;

        // Search with function type filter
        let options = SearchOptions::new().with_symbol_types(vec![SymbolType::Function]);
        let results = engine.symbol_search("data", &options).await;

        assert_eq!(results.results.len(), 1);
        assert_eq!(results.results[0].symbol.kind, "Function");
    }

    #[tokio::test]
    async fn test_traverse_graph() {
        let (engine, graph) = create_test_engine().await;

        // Create a simple call chain: A -> B -> C
        let (a, b, c);
        {
            let mut g = graph.write().await;

            let mut props_a = PropertyMap::new();
            props_a.insert(
                "name".to_string(),
                codegraph::PropertyValue::String("functionA".to_string()),
            );
            props_a.insert(
                "path".to_string(),
                codegraph::PropertyValue::String("/src/test.rs".to_string()),
            );
            props_a.insert("line_start".to_string(), codegraph::PropertyValue::Int(1));
            a = g
                .add_node(NodeType::Function, props_a)
                .expect("Failed to add node");

            let mut props_b = PropertyMap::new();
            props_b.insert(
                "name".to_string(),
                codegraph::PropertyValue::String("functionB".to_string()),
            );
            props_b.insert(
                "path".to_string(),
                codegraph::PropertyValue::String("/src/test.rs".to_string()),
            );
            props_b.insert("line_start".to_string(), codegraph::PropertyValue::Int(10));
            b = g
                .add_node(NodeType::Function, props_b)
                .expect("Failed to add node");

            let mut props_c = PropertyMap::new();
            props_c.insert(
                "name".to_string(),
                codegraph::PropertyValue::String("functionC".to_string()),
            );
            props_c.insert(
                "path".to_string(),
                codegraph::PropertyValue::String("/src/test.rs".to_string()),
            );
            props_c.insert("line_start".to_string(), codegraph::PropertyValue::Int(20));
            c = g
                .add_node(NodeType::Function, props_c)
                .expect("Failed to add node");

            // A calls B, B calls C
            g.add_edge(a, b, EdgeType::Calls, PropertyMap::new())
                .expect("Failed to add edge");
            g.add_edge(b, c, EdgeType::Calls, PropertyMap::new())
                .expect("Failed to add edge");
        }

        engine.build_indexes().await;

        // Traverse from A with depth 2
        let filter = TraversalFilter::new().with_max_nodes(100);
        let results = engine
            .traverse_graph(a, TraversalDirection::Outgoing, 2, &filter)
            .await;

        // Should find B and C
        assert_eq!(results.len(), 2);
        let names: Vec<&str> = results.iter().map(|r| r.symbol.name.as_str()).collect();
        assert!(names.contains(&"functionB"));
        assert!(names.contains(&"functionC"));
    }

    #[tokio::test]
    async fn test_get_callers() {
        let (engine, graph) = create_test_engine().await;

        let (a, b, c);
        {
            let mut g = graph.write().await;

            let mut props_a = PropertyMap::new();
            props_a.insert(
                "name".to_string(),
                codegraph::PropertyValue::String("caller1".to_string()),
            );
            props_a.insert(
                "path".to_string(),
                codegraph::PropertyValue::String("/src/test.rs".to_string()),
            );
            props_a.insert("line_start".to_string(), codegraph::PropertyValue::Int(1));
            a = g
                .add_node(NodeType::Function, props_a)
                .expect("Failed to add node");

            let mut props_b = PropertyMap::new();
            props_b.insert(
                "name".to_string(),
                codegraph::PropertyValue::String("caller2".to_string()),
            );
            props_b.insert(
                "path".to_string(),
                codegraph::PropertyValue::String("/src/test.rs".to_string()),
            );
            props_b.insert("line_start".to_string(), codegraph::PropertyValue::Int(10));
            b = g
                .add_node(NodeType::Function, props_b)
                .expect("Failed to add node");

            let mut props_c = PropertyMap::new();
            props_c.insert(
                "name".to_string(),
                codegraph::PropertyValue::String("target".to_string()),
            );
            props_c.insert(
                "path".to_string(),
                codegraph::PropertyValue::String("/src/test.rs".to_string()),
            );
            props_c.insert("line_start".to_string(), codegraph::PropertyValue::Int(20));
            c = g
                .add_node(NodeType::Function, props_c)
                .expect("Failed to add node");

            // A and B both call C
            g.add_edge(a, c, EdgeType::Calls, PropertyMap::new())
                .expect("Failed to add edge");
            g.add_edge(b, c, EdgeType::Calls, PropertyMap::new())
                .expect("Failed to add edge");
        }

        engine.build_indexes().await;

        // Get callers of C
        let callers = engine.get_callers(c, 1).await;

        assert_eq!(callers.len(), 2);
        let caller_names: Vec<&str> = callers.iter().map(|c| c.symbol.name.as_str()).collect();
        assert!(caller_names.contains(&"caller1"));
        assert!(caller_names.contains(&"caller2"));
    }

    #[tokio::test]
    async fn test_find_entry_points() {
        let (engine, graph) = create_test_engine().await;

        {
            let mut g = graph.write().await;

            // Main function
            let mut main_props = PropertyMap::new();
            main_props.insert(
                "name".to_string(),
                codegraph::PropertyValue::String("main".to_string()),
            );
            main_props.insert(
                "path".to_string(),
                codegraph::PropertyValue::String("/src/main.rs".to_string()),
            );
            main_props.insert("line_start".to_string(), codegraph::PropertyValue::Int(1));
            g.add_node(NodeType::Function, main_props)
                .expect("Failed to add main");

            // Test function
            let mut test_props = PropertyMap::new();
            test_props.insert(
                "name".to_string(),
                codegraph::PropertyValue::String("test_something".to_string()),
            );
            test_props.insert(
                "path".to_string(),
                codegraph::PropertyValue::String("/src/test.rs".to_string()),
            );
            test_props.insert("line_start".to_string(), codegraph::PropertyValue::Int(10));
            g.add_node(NodeType::Function, test_props)
                .expect("Failed to add test");

            // Regular function (not an entry point)
            let mut helper_props = PropertyMap::new();
            helper_props.insert(
                "name".to_string(),
                codegraph::PropertyValue::String("helper".to_string()),
            );
            helper_props.insert(
                "path".to_string(),
                codegraph::PropertyValue::String("/src/lib.rs".to_string()),
            );
            helper_props.insert("line_start".to_string(), codegraph::PropertyValue::Int(20));
            g.add_node(NodeType::Function, helper_props)
                .expect("Failed to add helper");
        }

        engine.build_indexes().await;

        // Find main entry points
        let mains = engine.find_entry_points(&[EntryType::Main]).await;
        assert_eq!(mains.len(), 1);
        assert_eq!(mains[0].symbol.name, "main");

        // Find test entry points
        let tests = engine.find_entry_points(&[EntryType::TestEntry]).await;
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].symbol.name, "test_something");
    }

    #[tokio::test]
    async fn test_get_symbol_info() {
        let (engine, graph) = create_test_engine().await;

        let node_id;
        {
            let mut g = graph.write().await;
            let mut props = PropertyMap::new();
            props.insert(
                "name".to_string(),
                codegraph::PropertyValue::String("processData".to_string()),
            );
            props.insert(
                "path".to_string(),
                codegraph::PropertyValue::String("/src/lib.rs".to_string()),
            );
            props.insert("line_start".to_string(), codegraph::PropertyValue::Int(10));
            props.insert("line_end".to_string(), codegraph::PropertyValue::Int(25));
            props.insert(
                "doc".to_string(),
                codegraph::PropertyValue::String("Processes input data".to_string()),
            );
            props.insert(
                "is_public".to_string(),
                codegraph::PropertyValue::Bool(true),
            );

            node_id = g
                .add_node(NodeType::Function, props)
                .expect("Failed to add node");
        }

        engine.build_indexes().await;

        let info = engine.get_symbol_info(node_id).await;

        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.symbol.name, "processData");
        assert_eq!(info.lines_of_code, 16); // 25 - 10 + 1
        assert!(info.is_public);
    }

    #[tokio::test]
    async fn test_query_performance() {
        let (engine, graph) = create_test_engine().await;

        // Add 1000 nodes using camelCase so tokens are properly split
        {
            let mut g = graph.write().await;
            for i in 0..1000 {
                let mut props = PropertyMap::new();
                // Use functionXXX format like "functionProcess0" so "function" is a separate token
                props.insert(
                    "name".to_string(),
                    codegraph::PropertyValue::String(format!("functionProcess{i}")),
                );
                props.insert(
                    "path".to_string(),
                    codegraph::PropertyValue::String("/src/test.rs".to_string()),
                );
                props.insert(
                    "line_start".to_string(),
                    codegraph::PropertyValue::Int(i as i64),
                );

                g.add_node(NodeType::Function, props)
                    .expect("Failed to add node");
            }
        }

        engine.build_indexes().await;

        // Search should complete quickly (< 10ms)
        let start = Instant::now();
        let results = engine
            .symbol_search("function", &SearchOptions::new())
            .await;
        let duration = start.elapsed();

        assert!(
            duration.as_millis() < 10,
            "Search took too long: {duration:?}"
        );
        assert!(!results.results.is_empty());
    }

    // ==========================================
    // find_by_signature tests
    // ==========================================

    #[tokio::test]
    async fn test_find_by_signature_name_pattern() {
        let (engine, graph) = create_test_engine().await;

        {
            let mut g = graph.write().await;

            // Add functions with different names
            for (name, is_async) in [
                ("getUserById", false),
                ("getOrderById", false),
                ("createUser", true),
                ("deleteUser", false),
                ("processData", false),
            ] {
                let mut props = PropertyMap::new();
                props.insert(
                    "name".to_string(),
                    codegraph::PropertyValue::String(name.to_string()),
                );
                props.insert(
                    "path".to_string(),
                    codegraph::PropertyValue::String("/src/api.rs".to_string()),
                );
                props.insert("line_start".to_string(), codegraph::PropertyValue::Int(1));
                props.insert(
                    "is_async".to_string(),
                    codegraph::PropertyValue::Bool(is_async),
                );
                g.add_node(NodeType::Function, props)
                    .expect("Failed to add node");
            }
        }

        engine.build_indexes().await;

        // Search for functions matching "get.*ById" pattern
        let pattern = SignaturePattern {
            name_pattern: Some("get.*ById".to_string()),
            return_type: None,
            param_count: None,
            modifiers: vec![],
        };

        let results = engine.find_by_signature(&pattern, None).await;

        assert_eq!(results.len(), 2);
        let names: Vec<&str> = results.iter().map(|r| r.symbol.name.as_str()).collect();
        assert!(names.contains(&"getUserById"));
        assert!(names.contains(&"getOrderById"));
    }

    #[tokio::test]
    async fn test_find_by_signature_return_type() {
        let (engine, graph) = create_test_engine().await;

        {
            let mut g = graph.write().await;

            // Add functions with different return types
            for (name, return_type) in [
                ("getString", "String"),
                ("getInt", "i32"),
                ("getBool", "bool"),
                ("getResult", "Result<String, Error>"),
            ] {
                let mut props = PropertyMap::new();
                props.insert(
                    "name".to_string(),
                    codegraph::PropertyValue::String(name.to_string()),
                );
                props.insert(
                    "path".to_string(),
                    codegraph::PropertyValue::String("/src/lib.rs".to_string()),
                );
                props.insert("line_start".to_string(), codegraph::PropertyValue::Int(1));
                props.insert(
                    "return_type".to_string(),
                    codegraph::PropertyValue::String(return_type.to_string()),
                );
                g.add_node(NodeType::Function, props)
                    .expect("Failed to add node");
            }
        }

        engine.build_indexes().await;

        // Search for functions returning String
        let pattern = SignaturePattern {
            name_pattern: None,
            return_type: Some("String".to_string()),
            param_count: None,
            modifiers: vec![],
        };

        let results = engine.find_by_signature(&pattern, None).await;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].symbol.name, "getString");
    }

    #[tokio::test]
    async fn test_find_by_signature_return_type_normalized() {
        let (engine, graph) = create_test_engine().await;

        {
            let mut g = graph.write().await;

            // Add functions with equivalent return types
            for (name, return_type) in [
                ("fn1", "boolean"),
                ("fn2", "bool"),
                ("fn3", "void"),
                ("fn4", "()"),
            ] {
                let mut props = PropertyMap::new();
                props.insert(
                    "name".to_string(),
                    codegraph::PropertyValue::String(name.to_string()),
                );
                props.insert(
                    "path".to_string(),
                    codegraph::PropertyValue::String("/src/lib.rs".to_string()),
                );
                props.insert("line_start".to_string(), codegraph::PropertyValue::Int(1));
                props.insert(
                    "return_type".to_string(),
                    codegraph::PropertyValue::String(return_type.to_string()),
                );
                g.add_node(NodeType::Function, props)
                    .expect("Failed to add node");
            }
        }

        engine.build_indexes().await;

        // Search for boolean (should match both "boolean" and "bool")
        let pattern = SignaturePattern {
            name_pattern: None,
            return_type: Some("bool".to_string()),
            param_count: None,
            modifiers: vec![],
        };

        let results = engine.find_by_signature(&pattern, None).await;

        assert_eq!(results.len(), 2);
        let names: Vec<&str> = results.iter().map(|r| r.symbol.name.as_str()).collect();
        assert!(names.contains(&"fn1"));
        assert!(names.contains(&"fn2"));

        // Search for void (should match "void" and "()")
        let pattern = SignaturePattern {
            name_pattern: None,
            return_type: Some("void".to_string()),
            param_count: None,
            modifiers: vec![],
        };

        let results = engine.find_by_signature(&pattern, None).await;

        assert_eq!(results.len(), 2);
        let names: Vec<&str> = results.iter().map(|r| r.symbol.name.as_str()).collect();
        assert!(names.contains(&"fn3"));
        assert!(names.contains(&"fn4"));
    }

    #[tokio::test]
    async fn test_find_by_signature_param_count() {
        let (engine, graph) = create_test_engine().await;

        {
            let mut g = graph.write().await;

            // Add functions with different param counts
            for (name, param_count) in [
                ("noParams", 0),
                ("oneParam", 1),
                ("twoParams", 2),
                ("threeParams", 3),
                ("manyParams", 5),
            ] {
                let mut props = PropertyMap::new();
                props.insert(
                    "name".to_string(),
                    codegraph::PropertyValue::String(name.to_string()),
                );
                props.insert(
                    "path".to_string(),
                    codegraph::PropertyValue::String("/src/lib.rs".to_string()),
                );
                props.insert("line_start".to_string(), codegraph::PropertyValue::Int(1));
                props.insert(
                    "param_count".to_string(),
                    codegraph::PropertyValue::Int(param_count),
                );
                g.add_node(NodeType::Function, props)
                    .expect("Failed to add node");
            }
        }

        engine.build_indexes().await;

        // Search for functions with 1-2 parameters
        let pattern = SignaturePattern {
            name_pattern: None,
            return_type: None,
            param_count: Some((1, 2)),
            modifiers: vec![],
        };

        let results = engine.find_by_signature(&pattern, None).await;

        assert_eq!(results.len(), 2);
        let names: Vec<&str> = results.iter().map(|r| r.symbol.name.as_str()).collect();
        assert!(names.contains(&"oneParam"));
        assert!(names.contains(&"twoParams"));

        // Search for functions with exactly 0 parameters
        let pattern = SignaturePattern {
            name_pattern: None,
            return_type: None,
            param_count: Some((0, 0)),
            modifiers: vec![],
        };

        let results = engine.find_by_signature(&pattern, None).await;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].symbol.name, "noParams");
    }

    #[tokio::test]
    async fn test_find_by_signature_modifiers() {
        let (engine, graph) = create_test_engine().await;

        {
            let mut g = graph.write().await;

            // Add functions with different modifiers
            let configs = [
                ("syncPublic", false, true, false),
                ("asyncPublic", true, true, false),
                ("syncPrivate", false, false, false),
                ("asyncPrivate", true, false, false),
                ("staticFunc", false, true, true),
            ];

            for (name, is_async, is_public, is_static) in configs {
                let mut props = PropertyMap::new();
                props.insert(
                    "name".to_string(),
                    codegraph::PropertyValue::String(name.to_string()),
                );
                props.insert(
                    "path".to_string(),
                    codegraph::PropertyValue::String("/src/lib.rs".to_string()),
                );
                props.insert("line_start".to_string(), codegraph::PropertyValue::Int(1));
                props.insert(
                    "is_async".to_string(),
                    codegraph::PropertyValue::Bool(is_async),
                );
                props.insert(
                    "is_public".to_string(),
                    codegraph::PropertyValue::Bool(is_public),
                );
                props.insert(
                    "is_static".to_string(),
                    codegraph::PropertyValue::Bool(is_static),
                );
                g.add_node(NodeType::Function, props)
                    .expect("Failed to add node");
            }
        }

        engine.build_indexes().await;

        // Search for async functions
        let pattern = SignaturePattern {
            name_pattern: None,
            return_type: None,
            param_count: None,
            modifiers: vec!["async".to_string()],
        };

        let results = engine.find_by_signature(&pattern, None).await;

        assert_eq!(results.len(), 2);
        let names: Vec<&str> = results.iter().map(|r| r.symbol.name.as_str()).collect();
        assert!(names.contains(&"asyncPublic"));
        assert!(names.contains(&"asyncPrivate"));

        // Search for public async functions
        let pattern = SignaturePattern {
            name_pattern: None,
            return_type: None,
            param_count: None,
            modifiers: vec!["async".to_string(), "public".to_string()],
        };

        let results = engine.find_by_signature(&pattern, None).await;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].symbol.name, "asyncPublic");

        // Search for static functions
        let pattern = SignaturePattern {
            name_pattern: None,
            return_type: None,
            param_count: None,
            modifiers: vec!["static".to_string()],
        };

        let results = engine.find_by_signature(&pattern, None).await;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].symbol.name, "staticFunc");
    }

    #[tokio::test]
    async fn test_find_by_signature_combined_filters() {
        let (engine, graph) = create_test_engine().await;

        {
            let mut g = graph.write().await;

            // Add various functions
            let configs = [
                ("getUserById", "User", 1, true, true),
                ("getOrderById", "Order", 1, true, false),
                ("fetchUserData", "User", 2, true, true),
                ("createUser", "User", 3, false, true),
                ("processRequest", "Response", 1, true, true),
            ];

            for (name, return_type, param_count, is_async, is_public) in configs {
                let mut props = PropertyMap::new();
                props.insert(
                    "name".to_string(),
                    codegraph::PropertyValue::String(name.to_string()),
                );
                props.insert(
                    "path".to_string(),
                    codegraph::PropertyValue::String("/src/api.rs".to_string()),
                );
                props.insert("line_start".to_string(), codegraph::PropertyValue::Int(1));
                props.insert(
                    "return_type".to_string(),
                    codegraph::PropertyValue::String(return_type.to_string()),
                );
                props.insert(
                    "param_count".to_string(),
                    codegraph::PropertyValue::Int(param_count),
                );
                props.insert(
                    "is_async".to_string(),
                    codegraph::PropertyValue::Bool(is_async),
                );
                props.insert(
                    "is_public".to_string(),
                    codegraph::PropertyValue::Bool(is_public),
                );
                g.add_node(NodeType::Function, props)
                    .expect("Failed to add node");
            }
        }

        engine.build_indexes().await;

        // Search for async public functions returning User with 1 param
        let pattern = SignaturePattern {
            name_pattern: None,
            return_type: Some("User".to_string()),
            param_count: Some((1, 1)),
            modifiers: vec!["async".to_string(), "public".to_string()],
        };

        let results = engine.find_by_signature(&pattern, None).await;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].symbol.name, "getUserById");
    }

    #[tokio::test]
    async fn test_find_by_signature_no_matches() {
        let (engine, graph) = create_test_engine().await;

        {
            let mut g = graph.write().await;
            let mut props = PropertyMap::new();
            props.insert(
                "name".to_string(),
                codegraph::PropertyValue::String("someFunction".to_string()),
            );
            props.insert(
                "path".to_string(),
                codegraph::PropertyValue::String("/src/lib.rs".to_string()),
            );
            props.insert("line_start".to_string(), codegraph::PropertyValue::Int(1));
            g.add_node(NodeType::Function, props)
                .expect("Failed to add node");
        }

        engine.build_indexes().await;

        // Search for a pattern that won't match
        let pattern = SignaturePattern {
            name_pattern: Some("nonexistent.*".to_string()),
            return_type: None,
            param_count: None,
            modifiers: vec![],
        };

        let results = engine.find_by_signature(&pattern, None).await;

        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_find_by_signature_only_matches_functions() {
        let (engine, graph) = create_test_engine().await;

        {
            let mut g = graph.write().await;

            // Add a function
            let mut func_props = PropertyMap::new();
            func_props.insert(
                "name".to_string(),
                codegraph::PropertyValue::String("myFunction".to_string()),
            );
            func_props.insert(
                "path".to_string(),
                codegraph::PropertyValue::String("/src/lib.rs".to_string()),
            );
            func_props.insert("line_start".to_string(), codegraph::PropertyValue::Int(1));
            g.add_node(NodeType::Function, func_props)
                .expect("Failed to add function");

            // Add a class with similar name
            let mut class_props = PropertyMap::new();
            class_props.insert(
                "name".to_string(),
                codegraph::PropertyValue::String("myClass".to_string()),
            );
            class_props.insert(
                "path".to_string(),
                codegraph::PropertyValue::String("/src/lib.rs".to_string()),
            );
            class_props.insert("line_start".to_string(), codegraph::PropertyValue::Int(10));
            g.add_node(NodeType::Class, class_props)
                .expect("Failed to add class");

            // Add a variable with similar name
            let mut var_props = PropertyMap::new();
            var_props.insert(
                "name".to_string(),
                codegraph::PropertyValue::String("myVariable".to_string()),
            );
            var_props.insert(
                "path".to_string(),
                codegraph::PropertyValue::String("/src/lib.rs".to_string()),
            );
            var_props.insert("line_start".to_string(), codegraph::PropertyValue::Int(20));
            g.add_node(NodeType::Variable, var_props)
                .expect("Failed to add variable");
        }

        engine.build_indexes().await;

        // Search with pattern matching all "my*"
        let pattern = SignaturePattern {
            name_pattern: Some("my.*".to_string()),
            return_type: None,
            param_count: None,
            modifiers: vec![],
        };

        let results = engine.find_by_signature(&pattern, None).await;

        // Should only match the function
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].symbol.name, "myFunction");
        assert_eq!(results[0].symbol.kind, "Function");
    }

    #[tokio::test]
    async fn test_find_by_signature_match_reason() {
        let (engine, graph) = create_test_engine().await;

        {
            let mut g = graph.write().await;
            let mut props = PropertyMap::new();
            props.insert(
                "name".to_string(),
                codegraph::PropertyValue::String("testFunc".to_string()),
            );
            props.insert(
                "path".to_string(),
                codegraph::PropertyValue::String("/src/lib.rs".to_string()),
            );
            props.insert("line_start".to_string(), codegraph::PropertyValue::Int(1));
            props.insert(
                "return_type".to_string(),
                codegraph::PropertyValue::String("bool".to_string()),
            );
            props.insert("param_count".to_string(), codegraph::PropertyValue::Int(2));
            props.insert("is_async".to_string(), codegraph::PropertyValue::Bool(true));
            g.add_node(NodeType::Function, props)
                .expect("Failed to add node");
        }

        engine.build_indexes().await;

        let pattern = SignaturePattern {
            name_pattern: Some("test.*".to_string()),
            return_type: Some("bool".to_string()),
            param_count: Some((2, 2)),
            modifiers: vec!["async".to_string()],
        };

        let results = engine.find_by_signature(&pattern, None).await;

        assert_eq!(results.len(), 1);
        let match_reason = &results[0].match_reason;
        assert!(match_reason.contains("name matches /test.*/"));
        assert!(match_reason.contains("returns bool"));
        assert!(match_reason.contains("2 parameters"));
        assert!(match_reason.contains("modifiers: async"));
    }

    #[tokio::test]
    async fn test_type_matches_wildcard() {
        let (engine, graph) = create_test_engine().await;

        {
            let mut g = graph.write().await;

            for (name, return_type) in [
                ("fn1", "Result<String, Error>"),
                ("fn2", "Result<i32, Error>"),
                ("fn3", "Option<String>"),
            ] {
                let mut props = PropertyMap::new();
                props.insert(
                    "name".to_string(),
                    codegraph::PropertyValue::String(name.to_string()),
                );
                props.insert(
                    "path".to_string(),
                    codegraph::PropertyValue::String("/src/lib.rs".to_string()),
                );
                props.insert("line_start".to_string(), codegraph::PropertyValue::Int(1));
                props.insert(
                    "return_type".to_string(),
                    codegraph::PropertyValue::String(return_type.to_string()),
                );
                g.add_node(NodeType::Function, props)
                    .expect("Failed to add node");
            }
        }

        engine.build_indexes().await;

        // Search for Result<*, Error> pattern
        let pattern = SignaturePattern {
            name_pattern: None,
            return_type: Some("Result<*, Error>".to_string()),
            param_count: None,
            modifiers: vec![],
        };

        let results = engine.find_by_signature(&pattern, None).await;

        assert_eq!(results.len(), 2);
        let names: Vec<&str> = results.iter().map(|r| r.symbol.name.as_str()).collect();
        assert!(names.contains(&"fn1"));
        assert!(names.contains(&"fn2"));
    }

    // ==========================================
    // detect_entry_type tests
    // ==========================================

    #[tokio::test]
    async fn test_detect_entry_type_http_handler() {
        let (engine, graph) = create_test_engine().await;

        {
            let mut g = graph.write().await;
            let mut props = PropertyMap::new();
            props.insert(
                "name".to_string(),
                codegraph::PropertyValue::String("getUsers".to_string()),
            );
            props.insert(
                "path".to_string(),
                codegraph::PropertyValue::String("/src/handlers.rs".to_string()),
            );
            props.insert("line_start".to_string(), codegraph::PropertyValue::Int(1));
            props.insert(
                "route".to_string(),
                codegraph::PropertyValue::String("/api/users".to_string()),
            );
            props.insert(
                "http_method".to_string(),
                codegraph::PropertyValue::String("GET".to_string()),
            );
            g.add_node(NodeType::Function, props)
                .expect("Failed to add node");
        }

        engine.build_indexes().await;

        let results = engine.find_entry_points(&[EntryType::HttpHandler]).await;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].symbol.name, "getUsers");
        assert!(matches!(results[0].entry_type, EntryType::HttpHandler));
        assert_eq!(results[0].route, Some("/api/users".to_string()));
        assert_eq!(results[0].method, Some("GET".to_string()));
    }

    #[tokio::test]
    async fn test_detect_entry_type_cli_command() {
        let (engine, graph) = create_test_engine().await;

        {
            let mut g = graph.write().await;

            // CLI command by name
            let mut props = PropertyMap::new();
            props.insert(
                "name".to_string(),
                codegraph::PropertyValue::String("runCommand".to_string()),
            );
            props.insert(
                "path".to_string(),
                codegraph::PropertyValue::String("/src/cli.rs".to_string()),
            );
            props.insert("line_start".to_string(), codegraph::PropertyValue::Int(1));
            g.add_node(NodeType::Function, props)
                .expect("Failed to add node");

            // CLI command by property
            let mut props2 = PropertyMap::new();
            props2.insert(
                "name".to_string(),
                codegraph::PropertyValue::String("execute".to_string()),
            );
            props2.insert(
                "path".to_string(),
                codegraph::PropertyValue::String("/src/cli.rs".to_string()),
            );
            props2.insert("line_start".to_string(), codegraph::PropertyValue::Int(10));
            props2.insert("is_cli".to_string(), codegraph::PropertyValue::Bool(true));
            g.add_node(NodeType::Function, props2)
                .expect("Failed to add node");
        }

        engine.build_indexes().await;

        let results = engine.find_entry_points(&[EntryType::CliCommand]).await;

        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_detect_entry_type_event_handler() {
        let (engine, graph) = create_test_engine().await;

        {
            let mut g = graph.write().await;

            // Use names that match the actual detection patterns:
            // - starts_with("on_")
            // - starts_with("handle_")
            // - ends_with("_handler")
            // - ends_with("_callback")
            // Note: Avoid "cli" in names as it triggers CliCommand detection first
            for name in [
                "on_submit",
                "handle_submit",
                "button_handler",
                "data_callback",
            ] {
                let mut props = PropertyMap::new();
                props.insert(
                    "name".to_string(),
                    codegraph::PropertyValue::String(name.to_string()),
                );
                props.insert(
                    "path".to_string(),
                    codegraph::PropertyValue::String("/src/events.rs".to_string()),
                );
                props.insert("line_start".to_string(), codegraph::PropertyValue::Int(1));
                g.add_node(NodeType::Function, props)
                    .expect("Failed to add node");
            }
        }

        engine.build_indexes().await;

        let results = engine.find_entry_points(&[EntryType::EventHandler]).await;

        assert_eq!(results.len(), 4);

        // Verify all expected patterns are detected
        let names: Vec<&str> = results.iter().map(|r| r.symbol.name.as_str()).collect();
        assert!(
            names.contains(&"on_submit"),
            "on_submit should be detected (starts with on_)"
        );
        assert!(
            names.contains(&"handle_submit"),
            "handle_submit should be detected (starts with handle_)"
        );
        assert!(
            names.contains(&"button_handler"),
            "button_handler should be detected (ends with _handler)"
        );
        assert!(
            names.contains(&"data_callback"),
            "data_callback should be detected (ends with _callback)"
        );
    }

    #[tokio::test]
    async fn test_find_entry_points_all_types() {
        let (engine, graph) = create_test_engine().await;

        {
            let mut g = graph.write().await;

            // Main
            let mut main_props = PropertyMap::new();
            main_props.insert(
                "name".to_string(),
                codegraph::PropertyValue::String("main".to_string()),
            );
            main_props.insert(
                "path".to_string(),
                codegraph::PropertyValue::String("/src/main.rs".to_string()),
            );
            main_props.insert("line_start".to_string(), codegraph::PropertyValue::Int(1));
            g.add_node(NodeType::Function, main_props)
                .expect("Failed to add main");

            // Test
            let mut test_props = PropertyMap::new();
            test_props.insert(
                "name".to_string(),
                codegraph::PropertyValue::String("test_something".to_string()),
            );
            test_props.insert(
                "path".to_string(),
                codegraph::PropertyValue::String("/src/tests.rs".to_string()),
            );
            test_props.insert("line_start".to_string(), codegraph::PropertyValue::Int(10));
            g.add_node(NodeType::Function, test_props)
                .expect("Failed to add test");

            // Public API
            let mut api_props = PropertyMap::new();
            api_props.insert(
                "name".to_string(),
                codegraph::PropertyValue::String("someApi".to_string()),
            );
            api_props.insert(
                "path".to_string(),
                codegraph::PropertyValue::String("/src/lib.rs".to_string()),
            );
            api_props.insert("line_start".to_string(), codegraph::PropertyValue::Int(20));
            api_props.insert("exported".to_string(), codegraph::PropertyValue::Bool(true));
            g.add_node(NodeType::Function, api_props)
                .expect("Failed to add api");
        }

        engine.build_indexes().await;

        // Find all entry points (empty filter)
        let results = engine.find_entry_points(&[]).await;

        // Should find main, test, and public API
        assert!(results.len() >= 3);
    }
}
