//! MCP Tool Definitions
//!
//! Defines all 27 CodeGraph tools for the MCP protocol.

use super::protocol::{PropertySchema, Tool, ToolInputSchema};
use std::collections::HashMap;

/// Get all available CodeGraph tools
pub fn get_all_tools() -> Vec<Tool> {
    vec![
        // Analysis Tools (9)
        get_dependency_graph_tool(),
        get_call_graph_tool(),
        analyze_impact_tool(),
        get_ai_context_tool(),
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
        description: Some("Analyzes file import/dependency relationships. USE WHEN: understanding module architecture, finding circular dependencies, planning refactoring, or tracing import chains.".to_string()),
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
        "character".to_string(),
        number_prop("Character position in the line (0-indexed)", Some(0.0)),
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
        description: Some("Maps function call relationships showing callers and callees. USE WHEN: tracing execution flow, understanding function usage, finding dead code, or debugging.".to_string()),
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
        "character".to_string(),
        number_prop("Character position (0-indexed)", Some(0.0)),
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
        description: Some("Predicts blast radius of code changes before making them. USE WHEN: planning refactoring, renaming symbols, deleting code, or assessing risk.".to_string()),
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
        "character".to_string(),
        number_prop("Character position (0-indexed)", Some(0.0)),
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
        description: Some("Gathers comprehensive code context optimized for AI understanding. USE WHEN: explaining code, planning modifications, debugging issues, or writing tests. THIS IS YOUR PRIMARY TOOL for understanding unfamiliar code.".to_string()),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: Some(vec!["uri".to_string(), "line".to_string()]),
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
        description: Some("Discovers test files and functions that exercise specific code. USE WHEN: modifying code to know which tests to run/update, debugging to find test cases, or assessing test coverage.".to_string()),
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
        "character".to_string(),
        number_prop("Character position (0-indexed)", Some(0.0)),
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
        description: Some("Gets quick metadata about any symbol (function, class, variable, type). USE WHEN: you need to quickly understand what a symbol is, check its signature, or see usage count. FASTER than codegraph_get_ai_context when you only need basic info.".to_string()),
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
        description: Some("Measures code complexity metrics for refactoring decisions. USE WHEN: identifying functions that need simplification, reviewing code quality, or prioritizing technical debt. Scores >10 typically indicate refactoring candidates.".to_string()),
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
        description: Some("Detects dead code that can be safely removed. USE WHEN: cleaning up codebase, reducing bundle size, or finding abandoned features. LIMITATIONS: May flag entry points, event handlers, or dynamically-called code.".to_string()),
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
        description: Some("Measures module coupling for architectural analysis. USE WHEN: evaluating module boundaries, planning decoupling refactoring, or assessing architectural health. High instability (>0.8) suggests fragile module.".to_string()),
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

    Tool {
        name: "codegraph_symbol_search".to_string(),
        description: Some("Searches codebase for symbols by name or pattern. USE WHEN: finding function/class implementations, exploring unfamiliar code, or locating specific functionality. THIS IS YOUR STARTING POINT when you don't know where code is located.".to_string()),
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
        description: Some("Finds all files importing a specific module or package. USE WHEN: planning library migrations, finding all React component usages, or discovering internal module consumers.".to_string()),
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
            "Type of entry point to find",
            vec![
                "main",
                "http_handler",
                "cli_command",
                "event_handler",
                "test",
                "all",
            ],
            Some("all"),
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

    Tool {
        name: "codegraph_find_entry_points".to_string(),
        description: Some("Discovers application entry points and execution starting points. USE WHEN: understanding app architecture, tracing request flow, or finding where to start debugging. START HERE when exploring unfamiliar backend applications.".to_string()),
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

    Tool {
        name: "codegraph_traverse_graph".to_string(),
        description: Some("Advanced graph traversal for complex code exploration. USE WHEN: specialized analysis requiring custom traversal (not covered by get_callers/get_callees/get_dependency_graph). PREFER simpler tools for common cases.".to_string()),
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
        description: Some("Finds functions matching signature patterns. USE WHEN: searching by structural characteristics rather than names - parameter count, return types, or modifiers.".to_string()),
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
        description: Some("Finds all functions that call a target function (reverse call graph). USE WHEN: understanding function usage, finding all invocation sites, or assessing change impact. SIMPLER than traverse_graph for this common use case.".to_string()),
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
        description: Some("Finds all functions called by a target function (forward call graph). USE WHEN: understanding function dependencies, tracing execution flow, or analyzing what code a function touches. SIMPLER than traverse_graph for this common use case.".to_string()),
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
        description: Some("Gets comprehensive symbol details including source code and relationships. USE WHEN: you need full context about a symbol - source code, callers, callees, complexity, and metadata together. MORE COMPLETE than get_symbol_info but heavier.".to_string()),
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
        description: Some("Persists knowledge for future sessions. USE WHEN: discovering important context worth remembering - debugging insights, architectural decisions, known issues, coding conventions, or project-specific knowledge.".to_string()),
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
        description: Some("Searches memories with hybrid BM25 + semantic + graph proximity. USE WHEN: recalling past knowledge - previous debugging sessions, architectural decisions, known issues. ALWAYS SEARCH before starting complex tasks.".to_string()),
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
        description: Some("Finds memories relevant to current code location. USE WHEN: starting work on a file/function to see past context. THIS SHOULD BE YOUR FIRST CALL when starting work on unfamiliar code.".to_string()),
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
        name: "codegraph_mine_git_file".to_string(),
        description: Some("Mines git history for a specific file to create memories. USE WHEN: wanting to understand the history and evolution of a particular file.".to_string()),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: Some(vec!["uri".to_string()]),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_all_tools_count() {
        let tools = get_all_tools();
        // Analysis: 9, Search: 5, Navigation: 3, Memory: 9 = 26 tools
        assert_eq!(tools.len(), 26, "Expected 26 tools, got {}", tools.len());
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
