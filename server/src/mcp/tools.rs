//! MCP Tool Definitions
//!
//! Defines all 35 CodeGraph tools for the MCP protocol.

use super::protocol::{PropertySchema, Tool, ToolInputSchema};
use std::collections::HashMap;

/// Get all available CodeGraph tools
pub fn get_all_tools() -> Vec<Tool> {
    vec![
        // Analysis Tools (11)
        get_dependency_graph_tool(),
        get_call_graph_tool(),
        analyze_impact_tool(),
        get_ai_context_tool(),
        get_edit_context_tool(),
        get_curated_context_tool(),
        find_related_tests_tool(),
        get_symbol_info_tool(),
        analyze_complexity_tool(),
        find_unused_code_tool(),
        analyze_coupling_tool(),
        // Search Tools (5)
        symbol_search_tool(),
        find_by_imports_tool(),
        find_entry_points_tool(),
        traverse_graph_tool(),
        find_by_signature_tool(),
        // Navigation Tools (3)
        get_callers_tool(),
        get_callees_tool(),
        get_detailed_symbol_tool(),
        // Memory Tools (8)
        memory_store_tool(),
        memory_search_tool(),
        memory_get_tool(),
        memory_context_tool(),
        memory_invalidate_tool(),
        memory_list_tool(),
        memory_stats_tool(),
        mine_git_history_tool(),
        mine_git_file_tool(),
        search_git_history_tool(),
        // Cross-Project Tools (1)
        cross_project_search_tool(),
        // Code Similarity Tools (4)
        find_duplicates_tool(),
        find_similar_tool(),
        cluster_symbols_tool(),
        compare_symbols_tool(),
        // Admin Tools (1)
        reindex_workspace_tool(),
    ]
}

// Helper to create property schema
fn string_prop(description: &str) -> PropertySchema {
    PropertySchema {
        property_type: "string".to_string(),
        description: Some(description.to_string()),
        default: None,
        enum_values: None,
        items: None,
        minimum: None,
        maximum: None,
    }
}

fn number_prop(description: &str, default: Option<f64>) -> PropertySchema {
    PropertySchema {
        property_type: "number".to_string(),
        description: Some(description.to_string()),
        default: default.map(|v| serde_json::json!(v)),
        enum_values: None,
        items: None,
        minimum: None,
        maximum: None,
    }
}

fn boolean_prop(description: &str, default: bool) -> PropertySchema {
    PropertySchema {
        property_type: "boolean".to_string(),
        description: Some(description.to_string()),
        default: Some(serde_json::json!(default)),
        enum_values: None,
        items: None,
        minimum: None,
        maximum: None,
    }
}

fn enum_prop(description: &str, values: Vec<&str>, default: Option<&str>) -> PropertySchema {
    PropertySchema {
        property_type: "string".to_string(),
        description: Some(description.to_string()),
        default: default.map(|v| serde_json::json!(v)),
        enum_values: Some(values.into_iter().map(|s| s.to_string()).collect()),
        items: None,
        minimum: None,
        maximum: None,
    }
}

fn array_prop(description: &str, item_type: &str) -> PropertySchema {
    PropertySchema {
        property_type: "array".to_string(),
        description: Some(description.to_string()),
        default: None,
        enum_values: None,
        items: Some(Box::new(PropertySchema {
            property_type: item_type.to_string(),
            description: None,
            default: None,
            enum_values: None,
            items: None,
            minimum: None,
            maximum: None,
        })),
        minimum: None,
        maximum: None,
    }
}

// === Analysis Tools ===

fn get_dependency_graph_tool() -> Tool {
    let mut properties = HashMap::new();
    properties.insert(
        "uri".to_string(),
        string_prop("The file URI to analyze (e.g., file:///path/to/file.ts)"),
    );
    properties.insert(
        "depth".to_string(),
        number_prop(
            "How many levels of dependencies to traverse (1-10, default: 3)",
            Some(3.0),
        ),
    );
    properties.insert(
        "includeExternal".to_string(),
        boolean_prop(
            "Whether to include external dependencies from node_modules/packages",
            false,
        ),
    );
    properties.insert("direction".to_string(), enum_prop(
        "Direction to analyze: 'imports' (what this file uses), 'importedBy' (what uses this file), or 'both'",
        vec!["imports", "importedBy", "both"],
        Some("both"),
    ));
    properties.insert(
        "summary".to_string(),
        boolean_prop("Return a condensed summary for large graphs", false),
    );

    Tool {
        name: "codegraph_get_dependency_graph".to_string(),
        description: Some("Analyzes file import/dependency relationships. USE WHEN: understanding module architecture, finding circular dependencies, planning refactoring, or tracing import chains. Returns a graph of files connected by import edges. direction='imports' shows what this file depends on, 'importedBy' shows what depends on this file, 'both' shows full picture. depth controls how many levels to traverse (1=direct only). Requires uri parameter (file URI).".to_string()),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: Some(vec!["uri".to_string()]),
        },
    }
}

fn get_call_graph_tool() -> Tool {
    let mut properties = HashMap::new();
    properties.insert(
        "uri".to_string(),
        string_prop("The file URI containing the function"),
    );
    properties.insert(
        "line".to_string(),
        number_prop("Line number of the function (0-indexed)", None),
    );
    properties.insert(
        "depth".to_string(),
        number_prop("How many levels deep to traverse the call graph", Some(3.0)),
    );
    properties.insert(
        "direction".to_string(),
        enum_prop(
            "Direction: 'callers' (who calls this), 'callees' (what this calls), or 'both'",
            vec!["callers", "callees", "both"],
            Some("both"),
        ),
    );
    properties.insert(
        "summary".to_string(),
        boolean_prop("Return a condensed summary for large call graphs", false),
    );

    Tool {
        name: "codegraph_get_call_graph".to_string(),
        description: Some("Maps function call relationships showing callers and callees. USE WHEN: tracing execution flow, understanding function usage, finding dead code, or debugging. Returns nodes with name, type, file path, and line range for each caller/callee in the chain. Use depth to control how many levels to traverse. Requires uri and line parameters.".to_string()),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: Some(vec!["uri".to_string(), "line".to_string()]),
        },
    }
}

fn analyze_impact_tool() -> Tool {
    let mut properties = HashMap::new();
    properties.insert(
        "uri".to_string(),
        string_prop("The file URI containing the symbol"),
    );
    properties.insert(
        "line".to_string(),
        number_prop("Line number of the symbol (0-indexed)", None),
    );
    properties.insert(
        "changeType".to_string(),
        enum_prop(
            "Type of change to analyze",
            vec!["modify", "delete", "rename"],
            Some("modify"),
        ),
    );
    properties.insert(
        "summary".to_string(),
        boolean_prop(
            "Return a condensed summary when many impacts are found",
            false,
        ),
    );

    Tool {
        name: "codegraph_analyze_impact".to_string(),
        description: Some("Predicts blast radius of code changes before making them. USE WHEN: planning refactoring, renaming symbols, deleting code, or assessing risk. Returns: list of affected symbols (direct and transitive), risk assessment, and change type analysis. changeType affects the analysis: 'modify' shows callers/dependents, 'delete' shows all references that would break, 'rename' shows all sites needing updates. Requires uri and line parameters.".to_string()),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: Some(vec!["uri".to_string(), "line".to_string()]),
        },
    }
}

fn get_ai_context_tool() -> Tool {
    let mut properties = HashMap::new();
    properties.insert(
        "uri".to_string(),
        string_prop("The file URI to get context for"),
    );
    properties.insert(
        "line".to_string(),
        number_prop("Line number (0-indexed)", None),
    );
    properties.insert(
        "intent".to_string(),
        enum_prop(
            "What you plan to do with the context. Affects which related code is selected.",
            vec!["explain", "modify", "debug", "test"],
            Some("explain"),
        ),
    );
    properties.insert(
        "maxTokens".to_string(),
        number_prop("Maximum tokens of context to return", Some(4000.0)),
    );

    Tool {
        name: "codegraph_get_ai_context".to_string(),
        description: Some("Gathers comprehensive code context optimized for AI understanding. USE WHEN: explaining code, planning modifications, debugging issues, or writing tests. THIS IS YOUR PRIMARY TOOL for understanding unfamiliar code. Returns: primaryContext (full source code), relatedSymbols (full source or signature-only when budget is tight), imports (file-level module imports), siblingFunctions (other functions in same file with signatures), dependencies, architecture (module, layer, neighbors), and debugHints (complexity, branches, exception handlers, early returns — debug intent only). Intent controls prioritization: 'explain' = dependencies + callers + siblings, 'modify' = tests + callers, 'debug' = call chain + debug hints, 'test' = example tests + mockable dependencies. maxTokens controls budget — high-priority sections get full source, remaining budget fills with signature-only symbols for maximum coverage. Requires uri and line parameters.".to_string()),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: Some(vec!["uri".to_string(), "line".to_string()]),
        },
    }
}

fn get_edit_context_tool() -> Tool {
    let mut properties = HashMap::new();
    properties.insert(
        "uri".to_string(),
        string_prop("The file URI of the code being edited"),
    );
    properties.insert(
        "line".to_string(),
        number_prop("Line number being edited (0-indexed)", None),
    );
    properties.insert(
        "maxTokens".to_string(),
        number_prop(
            "Maximum tokens of context to return (default: 8000)",
            Some(8000.0),
        ),
    );

    Tool {
        name: "codegraph_get_edit_context".to_string(),
        description: Some("Assembles everything needed to edit code at a specific location in a single call. USE WHEN: you are about to modify, refactor, or fix code and need full context before making changes. PREFER THIS over codegraph_get_ai_context when you are about to write or modify code — it includes callers (impact), tests (what to update), and git history (recent context) that get_ai_context does not. Use get_ai_context instead when you only need to understand or explain code. Returns 5 sections: (1) symbol — full source code of the function/method at the given line, (2) callers — functions that call this symbol (to assess impact of changes), (3) tests — related test functions (to know what to update/run), (4) memories — relevant debug notes, architectural decisions, and known issues, (5) recentChanges — recent git commits that touched this file. EXAMPLE: Before modifying a function's signature, call this to see all callers that would break, tests that need updating, and whether someone recently changed this code. Token budget controls total context size with priority: symbol > callers > tests > memories > git history. Requires uri and line parameters.".to_string()),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: Some(vec!["uri".to_string(), "line".to_string()]),
        },
    }
}

fn get_curated_context_tool() -> Tool {
    let mut properties = HashMap::new();
    properties.insert(
        "query".to_string(),
        string_prop("Natural language description of the context needed, or a symbol/module name (e.g., 'authentication logic', 'error handling in the API layer', 'UserService')"),
    );
    properties.insert(
        "uri".to_string(),
        string_prop(
            "Optional file URI to anchor the search — results from this file are prioritized",
        ),
    );
    properties.insert(
        "maxTokens".to_string(),
        number_prop(
            "Maximum tokens of context to return (default: 8000)",
            Some(8000.0),
        ),
    );
    properties.insert(
        "maxSymbols".to_string(),
        number_prop(
            "Maximum number of primary symbols to include (default: 5)",
            Some(5.0),
        ),
    );

    Tool {
        name: "codegraph_get_curated_context".to_string(),
        description: Some("Discovers and assembles context across the entire codebase for a natural language query. USE WHEN: you need to understand a concept, pattern, or subsystem that spans multiple files — e.g., 'how does authentication work?', 'what handles database connections?', 'error handling patterns'. Unlike codegraph_get_ai_context (single symbol) or codegraph_get_edit_context (single location), this searches the whole codebase and curates cross-cutting context. Pipeline: (1) searches for relevant symbols matching query, (2) resolves full source code for top matches, (3) walks dependency graph to find related modules, (4) fetches relevant memories, (5) curates everything within token budget prioritized by relevance. EXAMPLE: query='authentication middleware' returns the auth middleware function source, its callers (routes using it), its dependencies (token verification, user lookup), and any architectural decisions or debug notes about auth.".to_string()),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: Some(vec!["query".to_string()]),
        },
    }
}

fn find_related_tests_tool() -> Tool {
    let mut properties = HashMap::new();
    properties.insert(
        "uri".to_string(),
        string_prop("The file URI to find tests for"),
    );
    properties.insert(
        "line".to_string(),
        number_prop("Line number (0-indexed)", Some(0.0)),
    );
    properties.insert(
        "limit".to_string(),
        number_prop("Maximum number of related tests to return", Some(10.0)),
    );

    Tool {
        name: "codegraph_find_related_tests".to_string(),
        description: Some("Discovers test files and functions that exercise specific code. USE WHEN: modifying code to know which tests to run/update, debugging to find test cases, or assessing test coverage. Returns tests array (name, id, relationship) for the target symbol, plus total count. Finds tests by tracing Calls edges to functions with test-like names (test_, _test). Requires uri and line parameters.".to_string()),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: Some(vec!["uri".to_string()]),
        },
    }
}

fn get_symbol_info_tool() -> Tool {
    let mut properties = HashMap::new();
    properties.insert(
        "uri".to_string(),
        string_prop("The file URI containing the symbol"),
    );
    properties.insert(
        "line".to_string(),
        number_prop("Line number of the symbol (0-indexed)", None),
    );
    properties.insert(
        "includeReferences".to_string(),
        boolean_prop(
            "Whether to include all references to the symbol. Can be slow on large workspaces.",
            false,
        ),
    );

    Tool {
        name: "codegraph_get_symbol_info".to_string(),
        description: Some("Gets quick metadata about any symbol (function, class, variable, type). USE WHEN: you need to quickly understand what a symbol is, check its signature, or see usage count. FASTER than codegraph_get_ai_context when you only need basic info. Returns: name, kind, signature, visibility, file path, line range, and properties (is_async, is_static, etc.). Set includeReferences=true to also get all reference locations (slower). Requires uri and line parameters.".to_string()),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: Some(vec!["uri".to_string(), "line".to_string()]),
        },
    }
}

fn analyze_complexity_tool() -> Tool {
    let mut properties = HashMap::new();
    properties.insert("uri".to_string(), string_prop("The file URI to analyze"));
    properties.insert(
        "line".to_string(),
        number_prop(
            "Optional line number to analyze a specific function (0-indexed)",
            None,
        ),
    );
    properties.insert(
        "threshold".to_string(),
        number_prop(
            "Complexity threshold for flagging (default: 10)",
            Some(10.0),
        ),
    );
    properties.insert(
        "summary".to_string(),
        boolean_prop("Return a condensed summary", false),
    );

    Tool {
        name: "codegraph_analyze_complexity".to_string(),
        description: Some("Measures code complexity metrics for refactoring decisions. USE WHEN: identifying functions that need simplification, reviewing code quality, or prioritizing technical debt. Returns cyclomatic complexity score per function, with name, line range, and file path. Scores >10 typically indicate refactoring candidates, >20 is high complexity. Use threshold to filter — only functions at or above the threshold are returned. Omit line to analyze all functions in a file. Returns: {functions:[{name, complexity, grade, node_id, line_start, line_end, details:{complexity_branches, complexity_loops, complexity_logical_ops, complexity_nesting, complexity_exceptions, complexity_early_returns, lines_of_code}}], summary:{total_functions, average_complexity, max_complexity, above_threshold, threshold, overall_grade}, recommendations:[]} Requires uri parameter. Optionally line for a specific function.".to_string()),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: Some(vec!["uri".to_string()]),
        },
    }
}

fn find_unused_code_tool() -> Tool {
    let mut properties = HashMap::new();
    properties.insert(
        "uri".to_string(),
        string_prop("The file URI to analyze (optional for workspace scope)"),
    );
    properties.insert(
        "scope".to_string(),
        enum_prop(
            "Scope of analysis: 'file', 'module', or 'workspace'",
            vec!["file", "module", "workspace"],
            Some("file"),
        ),
    );
    properties.insert(
        "includeTests".to_string(),
        boolean_prop("Whether to include test files in analysis", false),
    );
    properties.insert(
        "confidence".to_string(),
        number_prop(
            "Minimum confidence threshold (0-1) for reporting unused code",
            Some(0.7),
        ),
    );
    properties.insert(
        "summary".to_string(),
        boolean_prop("Return a condensed summary", false),
    );

    Tool {
        name: "codegraph_find_unused_code".to_string(),
        description: Some("Detects dead code that can be safely removed. USE WHEN: cleaning up codebase, reducing bundle size, or finding abandoned features. Returns unused_items array (name, type, confidence 0-1, is_public) plus summary (total_checked, unused_count). confidence filters results — higher values mean more certain the code is unused. LIMITATIONS: May flag entry points, event handlers, or dynamically-called code.".to_string()),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: None,
        },
    }
}

fn analyze_coupling_tool() -> Tool {
    let mut properties = HashMap::new();
    properties.insert("uri".to_string(), string_prop("The file URI to analyze"));
    properties.insert(
        "includeExternal".to_string(),
        boolean_prop(
            "Whether to include external dependencies in analysis",
            false,
        ),
    );
    properties.insert(
        "depth".to_string(),
        number_prop("Depth of dependency analysis", Some(2.0)),
    );
    properties.insert(
        "summary".to_string(),
        boolean_prop("Return a condensed summary", false),
    );

    Tool {
        name: "codegraph_analyze_coupling".to_string(),
        description: Some("Measures module coupling for architectural analysis. USE WHEN: evaluating module boundaries, planning decoupling refactoring, or assessing architectural health. Returns metrics: afferent_coupling (incoming), efferent_coupling (outgoing), instability (0.0=stable, 1.0=unstable), total_dependencies, total_connections, plus a dependency_graph with nodes and edges. High instability (>0.8) suggests fragile module. Requires uri parameter (file URI).".to_string()),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: Some(vec!["uri".to_string()]),
        },
    }
}

// === Search Tools ===

fn symbol_search_tool() -> Tool {
    let mut properties = HashMap::new();
    properties.insert(
        "query".to_string(),
        string_prop("Search query - can be a symbol name, partial name, or descriptive text"),
    );
    properties.insert(
        "symbolType".to_string(),
        enum_prop(
            "Filter results by symbol type",
            vec![
                "function",
                "class",
                "method",
                "variable",
                "interface",
                "type",
                "module",
                "any",
            ],
            Some("any"),
        ),
    );
    properties.insert(
        "limit".to_string(),
        number_prop("Maximum number of results to return", Some(20.0)),
    );
    properties.insert(
        "includePrivate".to_string(),
        boolean_prop("Include private/internal symbols in results", true),
    );
    properties.insert(
        "compact".to_string(),
        boolean_prop(
            "Compact mode: return minimal info (name, kind, location) without signatures/docstrings for smaller responses",
            false,
        ),
    );

    Tool {
        name: "codegraph_symbol_search".to_string(),
        description: Some("Searches codebase for symbols by name or pattern. USE WHEN: finding function/class implementations, exploring unfamiliar code, or locating specific functionality. THIS IS YOUR STARTING POINT when you don't know where code is located. Supports both exact name matching and natural language queries (e.g., 'function that validates email addresses'). Returns array of matches, each with: name, kind (function/class/method/variable/interface/type/module), file path, line range, signature, and docstring. Use compact=true for minimal output (name, kind, location only). symbolType filters by kind — use 'any' to search all types.".to_string()),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: Some(vec!["query".to_string()]),
        },
    }
}

fn find_by_imports_tool() -> Tool {
    let mut properties = HashMap::new();
    properties.insert(
        "moduleName".to_string(),
        string_prop(
            "Name of the module/package to search for (e.g., 'lodash', 'react', './utils')",
        ),
    );
    properties.insert(
        "matchMode".to_string(),
        enum_prop(
            "How to match the module name",
            vec!["exact", "prefix", "contains", "fuzzy"],
            Some("contains"),
        ),
    );
    properties.insert(
        "limit".to_string(),
        number_prop("Maximum number of results", Some(50.0)),
    );

    Tool {
        name: "codegraph_find_by_imports".to_string(),
        description: Some("Finds all files importing a specific module or package. USE WHEN: planning library migrations, finding all React component usages, or discovering internal module consumers. Pass the module name via `moduleName` param (e.g., 'vscode', 'lodash', 'react', './utils'). Returns array of files that import the specified module, with file path and import details. matchMode controls matching: 'exact' for full name, 'prefix' for starts-with, 'contains' for substring, 'fuzzy' for approximate matching.".to_string()),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: Some(vec!["moduleName".to_string()]),
        },
    }
}

fn find_entry_points_tool() -> Tool {
    let mut properties = HashMap::new();
    properties.insert(
        "entryType".to_string(),
        enum_prop(
            "Type of entry point to find. Default returns architectural entry points (main, http_handler, cli_command, event_handler). Use 'all' to include tests and public API. Use 'test' or 'public' for those specifically.",
            vec![
                "main",
                "http_handler",
                "cli_command",
                "event_handler",
                "test",
                "public",
                "all",
            ],
            None,
        ),
    );
    properties.insert(
        "framework".to_string(),
        string_prop("Filter by framework (e.g., 'express', 'fastapi', 'actix')"),
    );
    properties.insert(
        "limit".to_string(),
        number_prop("Maximum number of results", Some(50.0)),
    );
    properties.insert(
        "compact".to_string(),
        boolean_prop(
            "Compact mode: return minimal info (name, kind, location) without signatures/docstrings for smaller responses",
            false,
        ),
    );

    Tool {
        name: "codegraph_find_entry_points".to_string(),
        description: Some("Discovers application entry points and execution starting points. USE WHEN: understanding app architecture, tracing request flow, or finding where to start debugging. START HERE when exploring unfamiliar backend applications. Returns array of entry points with name, kind, file path, line range, and signature. Default returns architectural entry points only (main, HTTP handlers, CLI commands, event handlers). Use entryType='all' to include tests and public API, or 'test'/'public' for those specifically. Default limit 50. Use compact=true for minimal output.".to_string()),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: None,
        },
    }
}

fn traverse_graph_tool() -> Tool {
    let mut properties = HashMap::new();
    properties.insert(
        "uri".to_string(),
        string_prop("File URI. Use with line to identify the starting symbol."),
    );
    properties.insert(
        "line".to_string(),
        number_prop(
            "0-based line number of the symbol. Use with uri to identify the starting symbol.",
            None,
        ),
    );
    properties.insert(
        "startNodeId".to_string(),
        string_prop("Internal node ID from symbol_search. Alternative to uri+line."),
    );
    properties.insert(
        "direction".to_string(),
        enum_prop(
            "Direction to traverse edges",
            vec!["outgoing", "incoming", "both"],
            Some("outgoing"),
        ),
    );
    properties.insert(
        "edgeTypes".to_string(),
        array_prop(
            "Types of edges to follow (e.g., ['calls', 'imports'])",
            "string",
        ),
    );
    properties.insert(
        "nodeTypes".to_string(),
        array_prop("Filter results to specific node types", "string"),
    );
    properties.insert(
        "maxDepth".to_string(),
        number_prop("Maximum traversal depth", Some(3.0)),
    );
    properties.insert(
        "limit".to_string(),
        number_prop("Maximum number of nodes to return", Some(100.0)),
    );
    properties.insert(
        "summary".to_string(),
        boolean_prop("Return a condensed summary for large graphs", false),
    );

    Tool {
        name: "codegraph_traverse_graph".to_string(),
        description: Some("Advanced graph traversal for complex code exploration. USE WHEN: specialized analysis requiring custom traversal (not covered by get_callers/get_callees/get_dependency_graph). PREFER simpler tools for common cases. Returns nodes and edges discovered during traversal. edgeTypes filters which relationships to follow (e.g., ['calls', 'imports']). nodeTypes filters which node kinds appear in results. Identify start node via uri+line or startNodeId from symbol_search.".to_string()),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: None,
        },
    }
}

fn find_by_signature_tool() -> Tool {
    let mut properties = HashMap::new();
    properties.insert(
        "namePattern".to_string(),
        string_prop("Pattern to match function names (supports wildcards like 'get*', '*Handler')"),
    );
    properties.insert(
        "paramCount".to_string(),
        number_prop("Exact number of parameters", None),
    );
    properties.insert(
        "minParams".to_string(),
        number_prop("Minimum number of parameters", None),
    );
    properties.insert(
        "maxParams".to_string(),
        number_prop("Maximum number of parameters", None),
    );
    properties.insert(
        "returnType".to_string(),
        string_prop("Return type to match (e.g., 'Promise', 'Result<T>', 'void')"),
    );
    properties.insert(
        "modifiers".to_string(),
        array_prop(
            "Required modifiers (e.g., ['async'], ['static', 'public'])",
            "string",
        ),
    );
    properties.insert(
        "limit".to_string(),
        number_prop("Maximum number of results", Some(50.0)),
    );

    Tool {
        name: "codegraph_find_by_signature".to_string(),
        description: Some("Finds functions matching signature patterns. USE WHEN: searching by structural characteristics rather than names - parameter count, return types, or modifiers. namePattern supports wildcards: 'get*' matches getUser, getData; '*Handler' matches requestHandler. paramCount filters by exact count; use minParams/maxParams for ranges. returnType matches against the function's return type string. modifiers filters by async, static, public, private, etc.".to_string()),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: None,
        },
    }
}

// === Navigation Tools ===

fn get_callers_tool() -> Tool {
    let mut properties = HashMap::new();
    properties.insert(
        "uri".to_string(),
        string_prop("File URI. Use with line to identify the function."),
    );
    properties.insert(
        "line".to_string(),
        number_prop(
            "0-based line number of the function. Use with uri to identify the function.",
            None,
        ),
    );
    properties.insert(
        "nodeId".to_string(),
        string_prop("Internal node ID from symbol_search. Alternative to uri+line."),
    );
    properties.insert(
        "depth".to_string(),
        number_prop("Depth of caller chain to traverse (default: 1)", Some(1.0)),
    );

    Tool {
        name: "codegraph_get_callers".to_string(),
        description: Some("Finds all functions that call a target function (reverse call graph). USE WHEN: understanding function usage, finding all invocation sites, or assessing change impact. SIMPLER than traverse_graph for this common use case. Returns callers array with symbol name and node ID. Use depth>1 to trace the full caller chain (who calls the callers). Identify target via uri+line or nodeId from symbol_search.".to_string()),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: None,
        },
    }
}

fn get_callees_tool() -> Tool {
    let mut properties = HashMap::new();
    properties.insert(
        "uri".to_string(),
        string_prop("File URI. Use with line to identify the function."),
    );
    properties.insert(
        "line".to_string(),
        number_prop(
            "0-based line number of the function. Use with uri to identify the function.",
            None,
        ),
    );
    properties.insert(
        "nodeId".to_string(),
        string_prop("Internal node ID from symbol_search. Alternative to uri+line."),
    );
    properties.insert(
        "depth".to_string(),
        number_prop("Depth of callee chain to traverse (default: 1)", Some(1.0)),
    );

    Tool {
        name: "codegraph_get_callees".to_string(),
        description: Some("Finds all functions called by a target function (forward call graph). USE WHEN: understanding function dependencies, tracing execution flow, or analyzing what code a function touches. SIMPLER than traverse_graph for this common use case. Returns callees array with symbol name and node ID. Use depth>1 to trace the full callee chain (what those callees call). Identify target via uri+line or nodeId from symbol_search.".to_string()),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: None,
        },
    }
}

fn get_detailed_symbol_tool() -> Tool {
    let mut properties = HashMap::new();
    properties.insert(
        "uri".to_string(),
        string_prop("File URI. Use with line to identify the symbol."),
    );
    properties.insert(
        "line".to_string(),
        number_prop(
            "0-based line number of the symbol. Use with uri to identify the symbol.",
            None,
        ),
    );
    properties.insert(
        "nodeId".to_string(),
        string_prop("Internal node ID from symbol_search. Alternative to uri+line."),
    );
    properties.insert(
        "includeSource".to_string(),
        boolean_prop("Include full source code of the symbol", true),
    );
    properties.insert(
        "includeCallers".to_string(),
        boolean_prop("Include list of callers", true),
    );
    properties.insert(
        "includeCallees".to_string(),
        boolean_prop("Include list of callees", true),
    );

    Tool {
        name: "codegraph_get_detailed_symbol".to_string(),
        description: Some("Gets comprehensive symbol details including source code and relationships. USE WHEN: you need full context about a symbol — source code, callers, callees, complexity, and metadata together. MORE COMPLETE than get_symbol_info but heavier. Returns: symbol (name, kind, signature, visibility, uri, line_range, properties), source (full source code string), callers (array), callees (array). Toggle includeSource/includeCallers/includeCallees to control response size. Identify symbol via uri+line or nodeId.".to_string()),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: None,
        },
    }
}

// === Memory Tools ===

fn memory_store_tool() -> Tool {
    let mut properties = HashMap::new();
    properties.insert(
        "kind".to_string(),
        enum_prop(
            "Type of memory being stored",
            vec![
                "debug_context",
                "architectural_decision",
                "known_issue",
                "convention",
                "project_context",
            ],
            None,
        ),
    );
    properties.insert(
        "title".to_string(),
        string_prop("Short descriptive title for the memory"),
    );
    properties.insert(
        "content".to_string(),
        string_prop("Main content of the memory"),
    );
    properties.insert(
        "tags".to_string(),
        array_prop("Tags for categorization and search", "string"),
    );
    properties.insert(
        "confidence".to_string(),
        number_prop("Confidence level 0.0-1.0 (default: 1.0)", Some(1.0)),
    );
    properties.insert(
        "problem".to_string(),
        string_prop("For debug_context: describe the problem encountered"),
    );
    properties.insert(
        "solution".to_string(),
        string_prop("For debug_context: describe the solution found"),
    );
    properties.insert(
        "decision".to_string(),
        string_prop("For architectural_decision: the decision made"),
    );
    properties.insert(
        "rationale".to_string(),
        string_prop("For architectural_decision: reasoning behind the decision"),
    );
    properties.insert(
        "description".to_string(),
        string_prop("For known_issue/convention/project_context: detailed description"),
    );
    properties.insert(
        "severity".to_string(),
        enum_prop(
            "For known_issue: severity level",
            vec!["critical", "high", "medium", "low"],
            None,
        ),
    );

    Tool {
        name: "codegraph_memory_store".to_string(),
        description: Some("Persists knowledge for future sessions. USE WHEN: discovering important context worth remembering — debugging insights, architectural decisions, known issues, coding conventions, or project-specific knowledge. Returns the stored memory ID. Each kind has specific optional fields: debug_context uses problem+solution, architectural_decision uses decision+rationale, known_issue uses description+severity. Tags improve future retrieval.".to_string()),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: Some(vec!["kind".to_string(), "title".to_string(), "content".to_string()]),
        },
    }
}

fn memory_search_tool() -> Tool {
    let mut properties = HashMap::new();
    properties.insert(
        "query".to_string(),
        string_prop("Search query - supports natural language"),
    );
    properties.insert(
        "limit".to_string(),
        number_prop("Maximum results to return", Some(10.0)),
    );
    properties.insert("tags".to_string(), array_prop("Filter by tags", "string"));
    properties.insert(
        "kinds".to_string(),
        array_prop("Filter by memory kinds", "string"),
    );
    properties.insert(
        "currentOnly".to_string(),
        boolean_prop("Only return non-invalidated memories", true),
    );
    properties.insert(
        "codeContext".to_string(),
        array_prop("Code node IDs for proximity boosting", "string"),
    );

    Tool {
        name: "codegraph_memory_search".to_string(),
        description: Some("Searches memories with hybrid BM25 + semantic + graph proximity. USE WHEN: recalling past knowledge — previous debugging sessions, architectural decisions, known issues. ALWAYS SEARCH before starting complex tasks. Returns results array (id, title, content, kind, score, tags, created_at) sorted by relevance. Filter with kinds (debug_context, architectural_decision, known_issue, convention, project_context), tags, or codeContext (node IDs for proximity boosting). Set currentOnly=false to include invalidated memories.".to_string()),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: Some(vec!["query".to_string()]),
        },
    }
}

fn memory_get_tool() -> Tool {
    let mut properties = HashMap::new();
    properties.insert("id".to_string(), string_prop("Memory ID to retrieve"));

    Tool {
        name: "codegraph_memory_get".to_string(),
        description: Some("Retrieves full memory details by ID. USE WHEN: you have a memory ID from search results and need complete content.".to_string()),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: Some(vec!["id".to_string()]),
        },
    }
}

fn memory_context_tool() -> Tool {
    let mut properties = HashMap::new();
    properties.insert(
        "uri".to_string(),
        string_prop("File URI to find relevant memories for"),
    );
    properties.insert(
        "line".to_string(),
        number_prop("Optional line number for more specific context", None),
    );
    properties.insert(
        "character".to_string(),
        number_prop("Optional character position", None),
    );
    properties.insert(
        "limit".to_string(),
        number_prop("Maximum memories to return", Some(5.0)),
    );
    properties.insert(
        "kinds".to_string(),
        array_prop("Filter by memory kinds", "string"),
    );

    Tool {
        name: "codegraph_memory_context".to_string(),
        description: Some("Finds memories relevant to current code location. USE WHEN: starting work on a file/function to see past context. THIS SHOULD BE YOUR FIRST CALL when starting work on unfamiliar code. Returns memories array (id, title, content, kind, score, tags) ranked by relevance to the file/line. Optionally filter by kinds. Provide line for function-level precision.".to_string()),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: Some(vec!["uri".to_string()]),
        },
    }
}

fn memory_invalidate_tool() -> Tool {
    let mut properties = HashMap::new();
    properties.insert("id".to_string(), string_prop("Memory ID to invalidate"));

    Tool {
        name: "codegraph_memory_invalidate".to_string(),
        description: Some("Marks memory as outdated without deleting. USE WHEN: knowledge is superseded, bugs are fixed, decisions are reversed. Maintains history while preventing outdated info from surfacing.".to_string()),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: Some(vec!["id".to_string()]),
        },
    }
}

fn memory_list_tool() -> Tool {
    let mut properties = HashMap::new();
    properties.insert(
        "kinds".to_string(),
        array_prop("Filter by memory kinds", "string"),
    );
    properties.insert("tags".to_string(), array_prop("Filter by tags", "string"));
    properties.insert(
        "currentOnly".to_string(),
        boolean_prop("Only show non-invalidated memories", true),
    );
    properties.insert(
        "limit".to_string(),
        number_prop("Maximum memories to return", Some(50.0)),
    );
    properties.insert(
        "offset".to_string(),
        number_prop("Offset for pagination", Some(0.0)),
    );

    Tool {
        name: "codegraph_memory_list".to_string(),
        description: Some("Lists memories with filtering and pagination. USE WHEN: browsing available memories or auditing stored knowledge.".to_string()),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: None,
        },
    }
}

fn memory_stats_tool() -> Tool {
    Tool {
        name: "codegraph_memory_stats".to_string(),
        description: Some(
            "Get statistics about stored memories - counts by kind, total storage, etc."
                .to_string(),
        ),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: None,
            required: None,
        },
    }
}

fn mine_git_history_tool() -> Tool {
    let mut properties = HashMap::new();
    properties.insert(
        "maxCommits".to_string(),
        number_prop("Maximum number of commits to process", Some(500.0)),
    );
    properties.insert(
        "minConfidence".to_string(),
        number_prop(
            "Minimum confidence score (0-1) for creating memories",
            Some(0.7),
        ),
    );

    Tool {
        name: "codegraph_mine_git_history".to_string(),
        description: Some("Mines git history to create memories from commit messages and patterns. USE WHEN: setting up a new project to bootstrap knowledge from past commits.".to_string()),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: None,
        },
    }
}

fn mine_git_file_tool() -> Tool {
    let mut properties = HashMap::new();
    properties.insert(
        "uri".to_string(),
        string_prop("File URI to mine git history for"),
    );
    properties.insert(
        "maxCommits".to_string(),
        number_prop("Maximum number of commits to process", Some(100.0)),
    );
    properties.insert(
        "minConfidence".to_string(),
        number_prop(
            "Minimum confidence score (0-1) for creating memories",
            Some(0.7),
        ),
    );

    Tool {
        name: "codegraph_mine_git_history_for_file".to_string(),
        description: Some("Mines git history for a specific file to create memories. USE WHEN: wanting to understand the history and evolution of a particular file. Requires uri parameter (file URI).".to_string()),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: Some(vec!["uri".to_string()]),
        },
    }
}

fn search_git_history_tool() -> Tool {
    let mut properties = HashMap::new();
    properties.insert(
        "query".to_string(),
        string_prop("Natural language search query (e.g., 'authentication changes', 'bug fix in payment module', 'database migration')"),
    );
    properties.insert(
        "since".to_string(),
        string_prop("Optional time filter (e.g., '2 weeks ago', '2026-01-01', '3 months ago')"),
    );
    properties.insert(
        "maxResults".to_string(),
        number_prop("Maximum results to return (default: 10)", Some(10.0)),
    );

    Tool {
        name: "codegraph_search_git_history".to_string(),
        description: Some("Searches git commit history using both semantic matching and keyword search. USE WHEN: you need to understand what changed and why — e.g., 'what changed authentication last month?', 'when was the database schema modified?', 'who fixed the login bug?'. Combines two search strategies: (1) semantic search over git-mined memories (embedding-based, finds conceptually related commits even without keyword overlap), (2) keyword search via git log (catches commits not yet mined into memories). Results include commit hash, subject, author, date, affected files, and memory content when available. Use 'since' to scope results to a time range.".to_string()),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: Some(vec!["query".to_string()]),
        },
    }
}

// === Admin Tools ===

fn cross_project_search_tool() -> Tool {
    let mut properties = HashMap::new();
    properties.insert(
        "query".to_string(),
        string_prop("Symbol name or substring to search for across other indexed projects"),
    );
    properties.insert(
        "symbolType".to_string(),
        enum_prop(
            "Filter results by symbol type",
            vec![
                "function",
                "class",
                "method",
                "variable",
                "interface",
                "type",
                "module",
                "any",
            ],
            Some("any"),
        ),
    );
    properties.insert(
        "limit".to_string(),
        number_prop("Maximum number of results to return", Some(20.0)),
    );

    Tool {
        name: "codegraph_cross_project_search".to_string(),
        description: Some("Search for symbols across ALL other indexed projects in the shared graph database. USE WHEN: finding cross-repo dependencies, discovering API consumers in other services, or understanding how a symbol is used across the organization. Returns matches with project attribution (which project, file path, line numbers). Only searches projects that have been previously indexed by codegraph. Does NOT search the current project (use symbol_search for that).".to_string()),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: Some(vec!["query".to_string()]),
        },
    }
}

fn reindex_workspace_tool() -> Tool {
    Tool {
        name: "codegraph_reindex_workspace".to_string(),
        description: Some("Reindex the workspace to refresh the code graph. USE WHEN: the graph data is stale or parsers have been updated. This clears the existing graph and re-parses all files.".to_string()),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: None,
            required: None,
        },
    }
}

fn find_duplicates_tool() -> Tool {
    let mut properties = HashMap::new();
    properties.insert(
        "threshold".to_string(),
        number_prop(
            "Minimum similarity score (0.0-1.0) to consider a duplicate. Default 0.7 works well for code clones. Use 0.9+ for near-exact duplicates.",
            Some(0.7),
        ),
    );
    properties.insert(
        "limit".to_string(),
        number_prop("Maximum number of duplicate pairs to return", Some(20.0)),
    );
    properties.insert(
        "uri".to_string(),
        string_prop("Optional file URI to limit search scope to functions in matching files"),
    );

    Tool {
        name: "codegraph_find_duplicates".to_string(),
        description: Some("Detects duplicate and near-duplicate functions across the codebase using semantic code embeddings. USE WHEN: cleaning up code, finding copy-paste patterns, identifying refactoring opportunities, or auditing code quality. Returns pairs of similar functions ranked by similarity score. Works across languages — can find a Python function duplicated in Go. Threshold 0.7 catches renamed-variable clones, 0.9+ finds near-exact copies. Powered by Jina Code V2 embeddings trained on 150M+ code pairs.".to_string()),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: None,
        },
    }
}

fn find_similar_tool() -> Tool {
    let mut properties = HashMap::new();
    properties.insert(
        "uri".to_string(),
        string_prop("File URI containing the function to find similar code for"),
    );
    properties.insert(
        "line".to_string(),
        number_prop("Line number of the function (0-indexed)", None),
    );
    properties.insert(
        "nodeId".to_string(),
        string_prop("Node ID from symbol_search (alternative to uri+line)"),
    );
    properties.insert(
        "limit".to_string(),
        number_prop("Maximum number of similar functions to return", Some(10.0)),
    );

    Tool {
        name: "codegraph_find_similar".to_string(),
        description: Some("Finds functions most similar to a given function using semantic code embeddings. USE WHEN: checking if similar functionality already exists before writing new code, finding related implementations across the codebase, or discovering code that could be consolidated. Returns functions ranked by similarity score with file paths and signatures. Works across languages. Identify the target function via nodeId (from symbol_search results) OR uri+line.".to_string()),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: None, // either nodeId or uri+line
        },
    }
}

fn cluster_symbols_tool() -> Tool {
    let mut properties = HashMap::new();
    properties.insert(
        "threshold".to_string(),
        number_prop(
            "Similarity threshold for grouping (0.0-1.0). Higher = tighter clusters. Default 0.7.",
            Some(0.7),
        ),
    );
    properties.insert(
        "minClusterSize".to_string(),
        number_prop("Minimum number of functions to form a cluster", Some(2.0)),
    );
    properties.insert(
        "limit".to_string(),
        number_prop("Maximum number of clusters to return", Some(20.0)),
    );

    Tool {
        name: "codegraph_cluster_symbols".to_string(),
        description: Some("Groups functions into semantic clusters based on what they do (e.g., all database access functions, all error handlers, all auth checks). USE WHEN: understanding codebase architecture, finding related functionality, identifying patterns, or planning refactoring. Returns clusters ranked by size, each with members and their similarity scores. Powered by code-aware embeddings.".to_string()),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: None,
        },
    }
}

fn compare_symbols_tool() -> Tool {
    let mut properties = HashMap::new();
    properties.insert(
        "uriA".to_string(),
        string_prop("File URI of the first function"),
    );
    properties.insert(
        "lineA".to_string(),
        number_prop("Line number of the first function (0-indexed)", None),
    );
    properties.insert(
        "uriB".to_string(),
        string_prop("File URI of the second function"),
    );
    properties.insert(
        "lineB".to_string(),
        number_prop("Line number of the second function (0-indexed)", None),
    );
    properties.insert(
        "nodeIdA".to_string(),
        string_prop("Node ID of the first function (alternative to uriA+lineA)"),
    );
    properties.insert(
        "nodeIdB".to_string(),
        string_prop("Node ID of the second function (alternative to uriB+lineB)"),
    );

    Tool {
        name: "codegraph_compare_symbols".to_string(),
        description: Some("Compares two functions semantically and structurally. USE WHEN: reviewing duplicates from find_duplicates, deciding whether to merge similar code, or understanding how two functions relate. Returns: semantic similarity score, verdict (clone/similar/related/unrelated), structural comparison (complexity, lines, params), and shared callers/callees. Identify each function via nodeIdA/nodeIdB (strings from symbol_search) OR uriA+lineA / uriB+lineB.".to_string()),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_all_tools_count() {
        let tools = get_all_tools();
        // Analysis: 11, Search: 5, Navigation: 3, Memory: 10, Cross-Project: 1, Admin: 1 = 31 tools
        assert_eq!(tools.len(), 31, "Expected 31 tools, got {}", tools.len());
    }

    #[test]
    fn test_tools_have_required_fields() {
        for tool in get_all_tools() {
            assert!(!tool.name.is_empty(), "Tool name should not be empty");
            assert!(
                tool.description.is_some(),
                "Tool {} should have description",
                tool.name
            );
        }
    }

    #[test]
    fn test_tool_names_are_unique() {
        let tools = get_all_tools();
        let names: Vec<_> = tools.iter().map(|t| &t.name).collect();
        let unique_names: std::collections::HashSet<_> = names.iter().collect();
        assert_eq!(
            names.len(),
            unique_names.len(),
            "Tool names should be unique"
        );
    }
}
