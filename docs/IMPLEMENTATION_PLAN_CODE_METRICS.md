# Implementation Plan: Code Quality Metrics (Phase 1)

**Priority:** HIGH
**Estimated Effort:** 3-4 weeks
**Status:** Planning

---

## Overview

This plan covers the implementation of code quality metrics tools as outlined in Phase 1 of the CodeGraph roadmap. These features will add significant value for code analysis, refactoring decisions, and technical debt tracking.

---

## Phase 1.1: Cyclomatic Complexity Analysis

### Tool: `codegraph_analyze_complexity`

**Goal:** Provide function-level complexity metrics with actionable insights.

### Implementation Steps

#### Step 1: Rust LSP Handler (server/src/handlers/metrics.rs) - NEW FILE

```rust
//! Code Metrics Handler - Complexity and quality analysis

use crate::backend::CodeGraphBackend;
use codegraph::{NodeId, NodeType};
use serde::{Deserialize, Serialize};
use tower_lsp::jsonrpc::Result;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComplexityParams {
    pub uri: String,
    pub line: Option<u32>,           // Specific function (optional)
    pub threshold: Option<u32>,      // Complexity threshold (default: 10)
    pub include_metrics: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComplexityResponse {
    pub functions: Vec<FunctionComplexity>,
    pub file_summary: FileSummary,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionComplexity {
    pub name: String,
    pub complexity: u32,
    pub grade: char,                  // A, B, C, D, F
    pub location: LocationInfo,
    pub details: ComplexityDetails,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComplexityDetails {
    pub branches: u32,               // if/else/switch
    pub loops: u32,                  // for/while/loop
    pub conditions: u32,             // && / ||
    pub nesting_depth: u32,
    pub lines_of_code: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSummary {
    pub total_functions: u32,
    pub average_complexity: f64,
    pub max_complexity: u32,
    pub functions_above_threshold: u32,
}
```

**Complexity Calculation Algorithm:**
```
Cyclomatic Complexity = E - N + 2P

Where:
- E = Number of edges in control flow graph
- N = Number of nodes
- P = Number of connected components (usually 1)

Simplified McCabe formula for functions:
CC = 1 + (if_count + for_count + while_count + case_count + catch_count + && + ||)
```

**Grading Scale:**
- A: 1-5 (Simple)
- B: 6-10 (Moderate)
- C: 11-20 (Complex)
- D: 21-50 (Very Complex)
- F: 50+ (Untestable)

#### Step 2: AST Analysis for Complexity

Since the parsers use tree-sitter, we need to traverse AST nodes to count control flow elements.

**Add to parser_registry.rs or new metrics module:**

```rust
pub fn calculate_complexity(source: &str, language: &str) -> ComplexityDetails {
    // Get tree-sitter parser for language
    // Traverse AST counting:
    // - if_statement, else_clause
    // - for_statement, while_statement, loop_statement
    // - switch_statement, case_clause
    // - try_statement, catch_clause
    // - binary_expression with && or ||
    // - ternary_expression (? :)

    // Track nesting depth during traversal
}
```

#### Step 3: Register LSP Command (server/src/custom_requests.rs)

Add to the match statement:
```rust
"codegraph/analyzeComplexity" => {
    let params: ComplexityParams = serde_json::from_value(params)?;
    let response = self.handle_analyze_complexity(params).await?;
    serde_json::to_value(response).map_err(|_| Error::internal_error())
}
```

#### Step 4: TypeScript Types (src/types.ts)

```typescript
export interface ComplexityParams {
    uri: string;
    line?: number;
    threshold?: number;
    includeMetrics?: boolean;
}

export interface ComplexityResponse {
    functions: FunctionComplexity[];
    fileSummary: FileSummary;
    recommendations: string[];
}

export interface FunctionComplexity {
    name: string;
    complexity: number;
    grade: 'A' | 'B' | 'C' | 'D' | 'F';
    location: LocationInfo;
    details: {
        branches: number;
        loops: number;
        conditions: number;
        nestingDepth: number;
        linesOfCode: number;
    };
}
```

#### Step 5: Language Model Tool (src/ai/toolManager.ts)

```typescript
private registerComplexityTool(): vscode.Disposable {
    return vscode.lm.registerTool('codegraph_analyze_complexity', {
        invoke: async (options, token) => {
            const input = options.input as {
                uri?: string;
                threshold?: number;
            };

            const uri = input.uri || this.getActiveFileUri();
            if (!uri) {
                return new vscode.LanguageModelToolResult([
                    new vscode.LanguageModelTextPart('No file specified or active.')
                ]);
            }

            const response = await this.sendRequestWithRetry<ComplexityResponse>(
                'codegraph.analyzeComplexity',
                { uri, threshold: input.threshold ?? 10 },
                token
            );

            return new vscode.LanguageModelToolResult([
                new vscode.LanguageModelTextPart(this.formatComplexityResponse(response))
            ]);
        },
        prepareInvocation: async (options) => ({
            invocationMessage: `Analyzing code complexity...`
        })
    });
}

private formatComplexityResponse(response: ComplexityResponse): string {
    const lines: string[] = [
        '## Code Complexity Analysis\n',
        `**File Summary:**`,
        `- Total Functions: ${response.fileSummary.totalFunctions}`,
        `- Average Complexity: ${response.fileSummary.averageComplexity.toFixed(1)}`,
        `- Max Complexity: ${response.fileSummary.maxComplexity}`,
        `- Functions Above Threshold: ${response.fileSummary.functionsAboveThreshold}`,
        ''
    ];

    if (response.functions.length > 0) {
        lines.push('**Functions:**\n');
        for (const fn of response.functions) {
            lines.push(`### ${fn.name} (Grade: ${fn.grade}, CC: ${fn.complexity})`);
            lines.push(`- Location: Line ${fn.location.range.start.line + 1}`);
            lines.push(`- Branches: ${fn.details.branches}`);
            lines.push(`- Loops: ${fn.details.loops}`);
            lines.push(`- Nesting: ${fn.details.nestingDepth}`);
            lines.push('');
        }
    }

    if (response.recommendations.length > 0) {
        lines.push('**Recommendations:**');
        for (const rec of response.recommendations) {
            lines.push(`- ${rec}`);
        }
    }

    return lines.join('\n');
}
```

#### Step 6: Tool Schema Definition

```typescript
{
    name: 'codegraph_analyze_complexity',
    description: 'Analyze cyclomatic complexity of functions in a file. Returns complexity scores, grades (A-F), and refactoring recommendations. Use this to identify complex code that may need simplification.',
    inputSchema: {
        type: 'object' as const,
        properties: {
            uri: {
                type: 'string',
                description: 'File URI to analyze. If not provided, uses the active file.'
            },
            threshold: {
                type: 'number',
                description: 'Complexity threshold for recommendations (default: 10)'
            }
        }
    }
}
```

---

## Phase 1.2: Dead Code Detection

### Tool: `codegraph_find_unused_code`

**Goal:** Identify unreferenced functions, imports, and exports.

### Implementation Steps

#### Step 1: Rust Handler (server/src/handlers/metrics.rs)

```rust
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnusedCodeParams {
    pub uri: Option<String>,
    pub scope: String,              // "file" | "module" | "workspace"
    pub include_tests: Option<bool>,
    pub confidence: Option<f64>,    // Min confidence threshold
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
    pub item_type: String,          // "function" | "class" | "import" | "export" | "variable"
    pub name: String,
    pub location: LocationInfo,
    pub confidence: f64,
    pub reason: String,
    pub safe_to_remove: bool,
}
```

**Detection Algorithm:**
1. Query graph for all function/class/variable nodes
2. For each node, check incoming edges (Direction::Incoming)
3. If no callers/references exist → potentially unused
4. Check for:
   - Exported symbols (may be used externally)
   - Entry points (main, exported functions)
   - Test files (may reference production code)
5. Calculate confidence based on scope analysis

#### Step 2: Graph Traversal Logic

```rust
impl CodeGraphBackend {
    pub async fn handle_find_unused_code(&self, params: UnusedCodeParams) -> Result<UnusedCodeResponse> {
        let graph = self.graph.read().await;
        let mut unused_items = Vec::new();

        // Get all function nodes in scope
        let query = graph.query()
            .node_type(NodeType::Function);

        if let Some(uri) = &params.uri {
            query.property("path", uri);
        }

        for node_id in query.execute()? {
            let node = graph.get_node(node_id)?;
            let incoming = graph.get_neighbors(node_id, Direction::Incoming)?;

            // Check if node has any callers
            let has_callers = incoming.iter().any(|&neighbor_id| {
                graph.get_edges_between(neighbor_id, node_id)
                    .map(|edges| edges.iter().any(|e| {
                        graph.get_edge(*e).map(|edge| edge.edge_type == EdgeType::Calls).unwrap_or(false)
                    }))
                    .unwrap_or(false)
            });

            if !has_callers {
                // Check if exported or entry point
                let name = node.properties.get_string("name").unwrap_or("");
                let is_exported = node.properties.get_bool("exported").unwrap_or(false);
                let is_entry = Self::is_entry_point(name);

                let confidence = if is_exported { 0.5 } else if is_entry { 0.3 } else { 0.9 };
                let safe_to_remove = !is_exported && !is_entry && confidence > 0.8;

                unused_items.push(UnusedItem {
                    item_type: "function".to_string(),
                    name: name.to_string(),
                    location: self.node_to_location_info(&graph, node_id)?,
                    confidence,
                    reason: if is_exported {
                        "Exported but no internal callers found".to_string()
                    } else {
                        "No callers found in codebase".to_string()
                    },
                    safe_to_remove,
                });
            }
        }

        Ok(UnusedCodeResponse {
            unused_items,
            summary: UnusedSummary { /* ... */ },
        })
    }
}
```

---

## Phase 1.3: Code Duplication Detection

### Tool: `codegraph_find_duplicates`

**Goal:** AST-based duplicate code detection with similarity scoring.

### Implementation Approach

This is more complex and requires:
1. **AST Fingerprinting**: Hash structural patterns, ignoring identifiers
2. **Similarity Matching**: Compare fingerprints across functions
3. **Clone Classification**:
   - Type-1: Exact copies
   - Type-2: Renamed identifiers
   - Type-3: Modified structure

**Simplified MVP Approach:**
- Compare function bodies by normalized AST structure
- Use tree-sitter to extract and normalize
- Hash and compare for quick detection

---

## Phase 1.4: Coupling & Cohesion Metrics

### Tool: `codegraph_analyze_coupling`

**Goal:** Module-level dependency analysis with architecture metrics.

### Metrics to Calculate

1. **Afferent Coupling (Ca)**: Incoming dependencies (who depends on this)
2. **Efferent Coupling (Ce)**: Outgoing dependencies (what this depends on)
3. **Instability (I)**: Ce / (Ca + Ce) — 0 = stable, 1 = unstable
4. **Abstractness (A)**: Abstract types / Total types
5. **Distance from Main Sequence**: |A + I - 1|

### Implementation

```rust
#[derive(Debug, Serialize)]
pub struct CouplingMetrics {
    pub afferent: u32,
    pub efferent: u32,
    pub instability: f64,
    pub cohesion_score: f64,
    pub violations: Vec<ArchViolation>,
}

impl CodeGraphBackend {
    pub async fn handle_analyze_coupling(&self, params: CouplingParams) -> Result<CouplingResponse> {
        let graph = self.graph.read().await;

        // Get module node
        let module_nodes = graph.query()
            .node_type(NodeType::Module)
            .property("path", &params.uri)
            .execute()?;

        if let Some(module_id) = module_nodes.first() {
            // Count incoming edges (afferent)
            let incoming = graph.get_neighbors(*module_id, Direction::Incoming)?;
            let afferent = incoming.len() as u32;

            // Count outgoing edges (efferent)
            let outgoing = graph.get_neighbors(*module_id, Direction::Outgoing)?;
            let efferent = outgoing.len() as u32;

            // Calculate instability
            let instability = if afferent + efferent > 0 {
                efferent as f64 / (afferent + efferent) as f64
            } else {
                0.0
            };

            Ok(CouplingResponse {
                coupling: CouplingMetrics {
                    afferent,
                    efferent,
                    instability,
                    cohesion_score: self.calculate_cohesion(&graph, *module_id)?,
                    violations: vec![],
                },
                recommendations: self.generate_coupling_recommendations(instability),
            })
        } else {
            Err(Error::invalid_params("Module not found"))
        }
    }
}
```

---

## Implementation Order

### Week 1: Foundation
- [ ] Create `server/src/handlers/metrics.rs` module
- [ ] Add complexity calculation for TypeScript/JavaScript
- [ ] Register `codegraph/analyzeComplexity` command
- [ ] Add TypeScript types and LM tool registration

### Week 2: Complexity Tool Complete
- [ ] Add complexity calculation for Python, Rust, Go
- [ ] Implement grading and recommendations
- [ ] Add unit tests for complexity calculation
- [ ] Integration test with VS Code

### Week 3: Dead Code Detection
- [ ] Implement `codegraph_find_unused_code`
- [ ] Handle edge cases (exports, entry points, tests)
- [ ] Add confidence scoring
- [ ] Register LM tool

### Week 4: Coupling Analysis
- [ ] Implement `codegraph_analyze_coupling`
- [ ] Calculate afferent/efferent coupling
- [ ] Add instability and cohesion metrics
- [ ] Integration testing and documentation

---

## Testing Strategy

### Unit Tests (Rust)
```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_complexity_simple_function() {
        let source = "function foo() { return 1; }";
        let complexity = calculate_complexity(source, "typescript");
        assert_eq!(complexity.cyclomatic, 1);
        assert_eq!(complexity.grade, 'A');
    }

    #[test]
    fn test_complexity_with_branches() {
        let source = r#"
            function foo(x) {
                if (x > 0) {
                    return 1;
                } else if (x < 0) {
                    return -1;
                } else {
                    return 0;
                }
            }
        "#;
        let complexity = calculate_complexity(source, "typescript");
        assert_eq!(complexity.cyclomatic, 3); // 1 + 2 branches
    }
}
```

### Integration Tests (TypeScript)
```typescript
describe('Complexity Analysis', () => {
    it('should analyze file complexity', async () => {
        const response = await client.sendRequest('codegraph/analyzeComplexity', {
            uri: 'file:///test/complex.ts',
            threshold: 10
        });

        expect(response.functions).toHaveLength(3);
        expect(response.fileSummary.averageComplexity).toBeGreaterThan(0);
    });
});
```

---

## Success Criteria

1. **Complexity Analysis**
   - Accurately calculates cyclomatic complexity
   - Provides actionable grades (A-F)
   - Generates useful recommendations

2. **Dead Code Detection**
   - Identifies truly unused code with >90% accuracy
   - Minimizes false positives for exports
   - Provides safe-to-remove confidence

3. **Coupling Metrics**
   - Correctly calculates Ca/Ce/I metrics
   - Identifies architectural violations
   - Provides refactoring guidance

4. **AI Integration**
   - All tools discoverable by AI agents
   - Clear, formatted output for LLM consumption
   - Token-efficient responses

---

## Files to Create/Modify

### New Files
- `server/src/handlers/metrics.rs` - Metrics handler implementations
- `server/src/complexity.rs` - Complexity calculation algorithms
- `src/ai/tools/complexity.ts` - Tool-specific formatting (optional)

### Modified Files
- `server/src/handlers/mod.rs` - Export metrics module
- `server/src/custom_requests.rs` - Register new commands
- `src/ai/toolManager.ts` - Register new LM tools
- `src/types.ts` - Add TypeScript interfaces
- `package.json` - Add commands if needed

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Complexity calculation accuracy | Start with simple McCabe formula, validate against known tools |
| Performance on large files | Cache results, use incremental analysis |
| False positives in dead code | Conservative confidence scoring, export awareness |
| Tree-sitter language differences | Abstract language-specific patterns behind trait |

---

## Dependencies

- `tree-sitter` - Already available via parser crates
- `codegraph` 0.1.1 - Graph traversal
- `codegraph-parser-api` 0.1.1 - Parser trait
- VS Code LM API 1.90+ - Tool registration

---

**Document Version:** 1.0
**Created:** December 14, 2025
**Author:** Implementation Planning
