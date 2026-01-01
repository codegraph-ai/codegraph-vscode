# CodeGraph AI Agent Query Architecture

**Document Version:** 1.0  
**Created:** December 31, 2024  
**Author:** CodeGraph Team  
**Status:** Design Document - Approved for Implementation

---

## Executive Summary

This document defines the architecture for CodeGraph's AI agent query system, which provides fast, composable, graph-based code intelligence without semantic embeddings. Instead of building fuzzy semantic search, we leverage the key insight that **AI agents can synthesize precise queries from natural language**, allowing us to focus on fast, structured query primitives that AI agents can compose into complex workflows.

**Core Hypothesis:** When AI agents are the primary interface, they don't need fuzzy matching—they need fast, composable query primitives that return rich metadata.

**Key Metrics:**
- Query latency: < 10ms for simple queries, < 20ms for graph traversals
- Token reduction: 75-90% vs grep-based approaches
- AI query success rate: > 90% for common code exploration tasks

**Strategic Benefits:**
1. **10x faster than embeddings** (10ms vs 100ms queries)
2. **More defensible** (graph intelligence is unique IP, not commodity embeddings)
3. **More explainable** (structural reasons, not similarity scores)
4. **Plays to CodeGraph's strengths** (best-in-class code graph)

---

## Table of Contents

1. [Problem Statement](#1-problem-statement)
2. [Design Principles](#2-design-principles)
3. [Architecture Overview](#3-architecture-overview)
4. [Core Query Primitives](#4-core-query-primitives)
5. [Language Model Tools](#5-language-model-tools)
6. [Performance Architecture](#6-performance-architecture)
7. [Implementation Phases](#7-implementation-phases)
8. [Testing Strategy](#8-testing-strategy)
9. [Success Metrics](#9-success-metrics)
10. [Migration Path](#10-migration-path)

---

## 1. Problem Statement

### 1.1 Current State

AI agents exploring unfamiliar codebases face two problems:

**Problem 1: Token Explosion**
```
User: "How does authentication work?"

Current approach (grep-based):
→ grep -r "auth" → 847 matches
→ Read 50 files to find relevant code
→ Cost: 100K+ tokens
→ Time: 30+ seconds
→ Quality: Lots of noise (AUTH_TOKEN constants, comments, etc.)
```

**Problem 2: Semantic Search Limitations**
```
Semantic search approach:
→ Embed "authentication flow"
→ Compare to 10K function embeddings
→ Cost: 100ms query latency + 22MB model
→ Quality: Fuzzy (might miss relevant code with different terminology)
→ Explainability: "Similarity score 0.87" (not helpful)
```

### 1.2 Why AI Agents Change Everything

AI agents can decompose natural language into precise structural queries:

```
User: "How does authentication work?"

AI Agent Query Plan:
1. find_entry_points("HttpHandler") → POST /login, /register
2. get_call_graph(login_handler, depth=3) → auth flow
3. find_by_imports(["passport", "jwt"]) → auth libraries
4. get_symbol_info(verifyToken) → implementation details

Result:
→ Cost: 4 queries × 10ms = 40ms
→ Tokens: 3K (precise code snippets, not entire files)
→ Quality: Exact code paths, not fuzzy matches
→ Explainability: Clear structural reasons
```

**Key Insight:** The AI agent is already doing semantic understanding. We just need to provide fast, precise building blocks.

### 1.3 Success Criteria

A successful implementation will:
- ✅ Reduce query latency by 10x (vs embeddings)
- ✅ Reduce token consumption by 90% (vs grep)
- ✅ Increase AI agent success rate to >90% (measured by user satisfaction)
- ✅ Provide explainable results (structural reasons, not similarity scores)
- ✅ Scale to 100K+ LOC codebases with <100ms query latency

---

## 2. Design Principles

### 2.1 Core Principles

**Principle 1: Optimize for Composition**
- AI agents chain 5-10 queries per user question
- Each query must be < 10ms for acceptable UX
- Queries should be independently useful and composable

**Principle 2: Speed Over Fuzzy Matching**
- Graph traversal + text indexes are 10x faster than embeddings
- AI agents generate precise queries; fuzzy matching is unnecessary
- Target: Sub-10ms for simple queries, sub-20ms for graph queries

**Principle 3: Rich Metadata Over Similarity Scores**
- Return structured data: callers, callees, imports, tests, metrics
- AI agents reason about structure better than similarity scores
- Enable explainability: "Found because X calls Y and imports Z"

**Principle 4: Graph is the Source of Truth**
- Code relationships are explicit in the AST/graph
- Don't approximate with learned models what we know precisely
- Graph queries are deterministic and explainable

**Principle 5: Fail Fast, Return Partial**
- Set timeouts (5s max for any query)
- Return partial results if complete traversal exceeds timeout
- Clear error messages for debugging

**Principle 6: Zero External Dependencies**
- Pure Rust implementation (no Python, no model files)
- Consistent with CodeGraph's existing architecture
- Easy to deploy, debug, and maintain

### 2.2 Non-Goals

What we are **NOT** building:
- ❌ Fuzzy semantic search with embeddings
- ❌ Natural language query parsing (AI generates queries)
- ❌ ML-based ranking (graph structure provides signal)
- ❌ User-facing semantic search UI (AI agents are the interface)
- ❌ Vector databases or embedding storage

---

## 3. Architecture Overview

### 3.1 System Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    AI Agent (Claude, Copilot)           │
│  "Find authentication code and explain how it works"    │
└────────────────────────┬────────────────────────────────┘
                         │ Language Model Tools API
                         ▼
┌─────────────────────────────────────────────────────────┐
│              VS Code Extension (TypeScript)             │
│  ┌─────────────────────────────────────────────────┐   │
│  │  Language Model Tools (9 existing + 6 new)      │   │
│  └─────────────────────────────────────────────────┘   │
└────────────────────────┬────────────────────────────────┘
                         │ LSP Protocol
                         ▼
┌─────────────────────────────────────────────────────────┐
│               Rust LSP Server (tower-lsp)               │
│  ┌──────────────────────────────────────────────────┐  │
│  │         AI Agent Query Engine (NEW)              │  │
│  │  ┌───────────────┐  ┌──────────────────────┐    │  │
│  │  │ Text Index    │  │  Graph Query Engine  │    │  │
│  │  │ (BM25)        │  │  (Traversal, Filter) │    │  │
│  │  └───────────────┘  └──────────────────────┘    │  │
│  │  ┌───────────────────────────────────────────┐  │  │
│  │  │  Query Primitives                         │  │  │
│  │  │  - symbol_search                          │  │  │
│  │  │  - find_by_imports                        │  │  │
│  │  │  - find_entry_points                      │  │  │
│  │  │  - traverse_graph                         │  │  │
│  │  │  - get_callers/callees                    │  │  │
│  │  └───────────────────────────────────────────┘  │  │
│  └──────────────────────────────────────────────────┘  │
│  ┌──────────────────────────────────────────────────┐  │
│  │  CodeGraph Core (Existing)                       │  │
│  │  - Parser Registry                               │  │
│  │  - Graph Storage (RocksDB)                       │  │
│  │  - Symbol Index                                  │  │
│  └──────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────┘
```

### 3.2 Data Flow

**Example: "Find email validation code"**

```
1. User → AI Agent
   "Find email validation code"

2. AI Agent → Language Model Tools
   [
     codegraph_symbol_search(query="validate_email"),
     codegraph_find_by_imports(libraries=["re", "email"]),
     codegraph_find_by_signature(returnType="bool", namePattern=".*valid.*")
   ]

3. Language Model Tools → LSP Server
   Custom LSP requests:
   - codegraph/symbolSearch
   - codegraph/findByImports
   - codegraph/findBySignature

4. LSP Server → Query Engine
   - Text index lookup: "validate_email" → [node_123, node_456]
   - Import index lookup: "re" → [node_123, node_789]
   - Signature index lookup: bool + "valid" → [node_123, node_456, node_999]
   
5. Query Engine → Graph
   - Fetch metadata for matching nodes
   - Get callers/callees (1-hop)
   - Get import information
   - Calculate ranking scores

6. Results → AI Agent
   [
     {
       symbol: "validate_email",
       location: "auth/validators.py:45",
       signature: "def validate_email(email: str) -> bool",
       callers: ["register_user", "update_profile"],
       imports: ["re"],
       hasTests: true,
       complexity: 5,
       rankScore: 9.2,
       reason: "Symbol name exact match + imports 're' + has tests"
     },
     ...
   ]

7. AI Agent → User
   "Found `validate_email()` in auth/validators.py. This function:
   - Uses regex to validate email format
   - Called by registration and profile update flows
   - Has comprehensive test coverage
   
   Would you like me to explain the validation logic?"
```

### 3.3 Component Responsibilities

**Text Index** (New)
- Inverted index for symbol names, docstrings, comments
- BM25-style ranking algorithm
- Sub-5ms lookup for keyword queries
- Memory footprint: ~50 bytes per token occurrence

**Graph Query Engine** (New)
- Graph traversal with custom filters
- Multi-hop relationship queries
- Entry point detection (HTTP handlers, CLI commands, public APIs)
- Import-based discovery

**Query Primitives** (New)
- 7 core primitives for AI agent composition
- Input validation and sanitization
- Result ranking and filtering
- Timeout management (5s max)

**CodeGraph Core** (Existing)
- AST parsing via language-specific plugins
- Graph construction and storage
- Content-hash based caching
- Symbol indexing

---

## 4. Core Query Primitives

### 4.1 Primitive 1: symbol_search

**Purpose:** Fast text-based symbol search with BM25 ranking

**Signature:**
```rust
fn symbol_search(
    query: &str,
    options: SearchOptions
) -> Result<Vec<SymbolMatch>>;

struct SearchOptions {
    scope: SearchScope,          // Workspace | Module | File
    symbol_types: Vec<SymbolType>, // Function | Class | Variable | Module
    languages: Vec<Language>,     // Filter by language
    limit: usize,                // Max results (default 20)
    include_private: bool,       // Include private symbols (default false)
}

struct SymbolMatch {
    node_id: NodeId,
    symbol: SymbolInfo,
    score: f32,                  // BM25 score
    match_reason: MatchReason,   // Name | Docstring | Comment
}
```

**Implementation:**
```rust
impl TextIndex {
    fn search(&self, query: &str, options: &SearchOptions) -> Vec<SymbolMatch> {
        let tokens = self.tokenize(query);
        let mut scores: HashMap<NodeId, f32> = HashMap::new();
        
        // BM25 scoring
        for token in tokens {
            if let Some(postings) = self.inverted_index.get(&token) {
                for posting in postings {
                    // Filter by options
                    if !self.matches_filter(posting.node_id, options) {
                        continue;
                    }
                    
                    // BM25 formula
                    let idf = self.compute_idf(&token);
                    let tf = posting.term_frequency;
                    let doc_len = self.get_doc_length(posting.node_id);
                    let avg_doc_len = self.avg_document_length;
                    
                    let score = idf * (tf * (K1 + 1.0)) / 
                        (tf + K1 * (1.0 - B + B * (doc_len / avg_doc_len)));
                    
                    *scores.entry(posting.node_id).or_insert(0.0) += score * posting.weight;
                }
            }
        }
        
        // Rank and return
        let mut results: Vec<_> = scores.into_iter().collect();
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        results.truncate(options.limit);
        
        results.into_iter()
            .map(|(node_id, score)| self.to_symbol_match(node_id, score))
            .collect()
    }
}
```

**Performance Target:** < 5ms for 10K symbols

**Index Structure:**
```rust
struct TextIndex {
    // Token → List of (NodeId, term_frequency, field_weight)
    inverted_index: HashMap<String, Vec<Posting>>,
    
    // NodeId → Document length (for BM25)
    doc_lengths: HashMap<NodeId, f32>,
    
    // Average document length (for BM25)
    avg_document_length: f32,
    
    // Total number of documents
    total_docs: usize,
}

struct Posting {
    node_id: NodeId,
    term_frequency: f32,
    weight: f32,  // Symbol name: 3.0, Docstring: 2.0, Comment: 1.0
    position: usize,
}
```

**BM25 Constants:**
```rust
const K1: f32 = 1.2;  // Term frequency saturation
const B: f32 = 0.75;  // Length normalization
```

---

### 4.2 Primitive 2: find_by_imports

**Purpose:** Discover code by imported libraries/modules

**Signature:**
```rust
fn find_by_imports(
    libraries: &[&str],
    options: ImportSearchOptions
) -> Result<Vec<NodeId>>;

struct ImportSearchOptions {
    match_mode: ImportMatchMode,  // Exact | Prefix | Fuzzy
    scope: SearchScope,
    languages: Vec<Language>,
    include_transitive: bool,     // Include code that imports code that imports X
}

enum ImportMatchMode {
    Exact,    // import re
    Prefix,   // import email.* 
    Fuzzy,    // import re or regex or regular_expression
}
```

**Implementation:**
```rust
struct ImportIndex {
    // Library name → List of NodeIds that import it
    library_to_nodes: HashMap<String, Vec<NodeId>>,
    
    // NodeId → List of imported libraries
    node_to_libraries: HashMap<NodeId, Vec<String>>,
}

impl ImportIndex {
    fn find_by_imports(&self, libraries: &[&str], options: &ImportSearchOptions) -> Vec<NodeId> {
        let mut results = HashSet::new();
        
        for lib in libraries {
            let matching_libs = match options.match_mode {
                ImportMatchMode::Exact => {
                    vec![lib.to_string()]
                },
                ImportMatchMode::Prefix => {
                    self.library_to_nodes.keys()
                        .filter(|k| k.starts_with(lib))
                        .cloned()
                        .collect()
                },
                ImportMatchMode::Fuzzy => {
                    self.fuzzy_match_libraries(lib)
                }
            };
            
            for matching_lib in matching_libs {
                if let Some(nodes) = self.library_to_nodes.get(&matching_lib) {
                    results.extend(nodes.iter().cloned());
                }
            }
        }
        
        // Transitive imports if requested
        if options.include_transitive {
            let transitive = self.find_transitive_importers(&results);
            results.extend(transitive);
        }
        
        results.into_iter().collect()
    }
}
```

**Performance Target:** < 5ms (simple hash lookup)

**Example Usage:**
```rust
// Find all code that uses regex
find_by_imports(&["re", "regex", "regular-expression"], ImportSearchOptions {
    match_mode: ImportMatchMode::Fuzzy,
    scope: SearchScope::Workspace,
    languages: vec![Language::Python],
    include_transitive: false,
})

// Find all database access code
find_by_imports(&["sqlalchemy", "prisma", "diesel"], ImportSearchOptions {
    match_mode: ImportMatchMode::Exact,
    scope: SearchScope::Workspace,
    languages: vec![],
    include_transitive: true,  // Include code that calls DB code
})
```

---

### 4.3 Primitive 3: find_by_signature

**Purpose:** Pattern matching on function signatures

**Signature:**
```rust
fn find_by_signature(
    pattern: SignaturePattern
) -> Result<Vec<NodeId>>;

struct SignaturePattern {
    name_pattern: Option<String>,     // Regex pattern for function name
    return_type: Option<TypePattern>, // Return type pattern
    param_types: Vec<TypePattern>,    // Parameter type patterns
    param_count: Option<RangeInclusive<usize>>,
    modifiers: Vec<Modifier>,         // async, pub, static, etc.
}

enum TypePattern {
    Exact(String),           // bool
    Pattern(String),         // Result<*, *>
    Primitive(PrimitiveType), // bool, int, string
    Any,
}

enum Modifier {
    Public,
    Private,
    Static,
    Async,
    Const,
}
```

**Implementation:**
```rust
struct SignatureIndex {
    // Return type → List of functions
    return_type_index: HashMap<String, Vec<NodeId>>,
    
    // Parameter count → List of functions
    param_count_index: HashMap<usize, Vec<NodeId>>,
    
    // Modifier → List of functions
    modifier_index: HashMap<Modifier, Vec<NodeId>>,
}

impl SignatureIndex {
    fn find_by_signature(&self, pattern: &SignaturePattern) -> Vec<NodeId> {
        let mut candidates: Option<HashSet<NodeId>> = None;
        
        // Filter by return type
        if let Some(return_type) = &pattern.return_type {
            let nodes = self.find_by_return_type(return_type);
            candidates = Some(self.intersect(candidates, nodes));
        }
        
        // Filter by parameter count
        if let Some(param_count) = &pattern.param_count {
            let mut nodes = HashSet::new();
            for count in param_count.clone() {
                if let Some(funcs) = self.param_count_index.get(&count) {
                    nodes.extend(funcs.iter().cloned());
                }
            }
            candidates = Some(self.intersect(candidates, nodes));
        }
        
        // Filter by modifiers
        for modifier in &pattern.modifiers {
            if let Some(funcs) = self.modifier_index.get(modifier) {
                let nodes: HashSet<_> = funcs.iter().cloned().collect();
                candidates = Some(self.intersect(candidates, nodes));
            }
        }
        
        // Filter by name pattern (regex)
        if let Some(name_pattern) = &pattern.name_pattern {
            let regex = Regex::new(name_pattern).ok()?;
            candidates = candidates.map(|nodes| {
                nodes.into_iter()
                    .filter(|node| {
                        let symbol = self.get_symbol(*node);
                        regex.is_match(&symbol.name)
                    })
                    .collect()
            });
        }
        
        candidates.unwrap_or_default().into_iter().collect()
    }
}
```

**Performance Target:** < 10ms (multiple index lookups + regex)

**Example Usage:**
```rust
// Find all validators (return bool, name contains "valid")
find_by_signature(SignaturePattern {
    name_pattern: Some(".*valid.*".to_string()),
    return_type: Some(TypePattern::Primitive(PrimitiveType::Bool)),
    param_types: vec![],
    param_count: None,
    modifiers: vec![],
})

// Find all async public functions
find_by_signature(SignaturePattern {
    name_pattern: None,
    return_type: None,
    param_types: vec![],
    param_count: None,
    modifiers: vec![Modifier::Async, Modifier::Public],
})
```

---

### 4.4 Primitive 4: find_entry_points

**Purpose:** Detect architectural entry points (HTTP handlers, CLI commands, etc.)

**Signature:**
```rust
fn find_entry_points(
    entry_type: EntryType,
    options: EntryPointOptions
) -> Result<Vec<EntryPoint>>;

enum EntryType {
    HttpHandler,      // Express routes, FastAPI endpoints, etc.
    CliCommand,       // Main functions, CLI parsers
    PublicApi,        // Exported functions/classes
    EventHandler,     // Event listeners, callbacks
    TestEntry,        // Test functions
    Main,            // Program entry points
}

struct EntryPointOptions {
    scope: SearchScope,
    languages: Vec<Language>,
    framework: Option<String>,  // "express", "fastapi", "clap", etc.
}

struct EntryPoint {
    node_id: NodeId,
    entry_type: EntryType,
    route: Option<String>,      // "/api/users" for HTTP
    method: Option<HttpMethod>, // GET, POST, etc.
    description: Option<String>,
}
```

**Implementation:**
```rust
impl EntryPointDetector {
    fn find_entry_points(&self, entry_type: &EntryType, options: &EntryPointOptions) -> Vec<EntryPoint> {
        match entry_type {
            EntryType::HttpHandler => self.find_http_handlers(options),
            EntryType::CliCommand => self.find_cli_commands(options),
            EntryType::PublicApi => self.find_public_apis(options),
            EntryType::EventHandler => self.find_event_handlers(options),
            EntryType::TestEntry => self.find_test_entries(options),
            EntryType::Main => self.find_main_functions(options),
        }
    }
    
    fn find_http_handlers(&self, options: &EntryPointOptions) -> Vec<EntryPoint> {
        let mut results = Vec::new();
        
        // Pattern 1: Decorator-based (Python FastAPI, Flask)
        // @app.get("/users")
        // def get_users(): ...
        results.extend(self.find_by_decorator_pattern(&[
            "app.get", "app.post", "app.put", "app.delete",
            "route", "api_route"
        ]));
        
        // Pattern 2: Function call (Express.js)
        // app.get("/users", handler)
        results.extend(self.find_by_call_pattern(&[
            "app.get", "app.post", "router.get", "router.post"
        ]));
        
        // Pattern 3: Attribute-based (Rust Actix)
        // #[get("/users")]
        // async fn get_users() -> impl Responder
        results.extend(self.find_by_attribute_pattern(&[
            "get", "post", "put", "delete", "route"
        ]));
        
        results
    }
    
    fn find_cli_commands(&self, options: &EntryPointOptions) -> Vec<EntryPoint> {
        let mut results = Vec::new();
        
        // Pattern 1: Main function
        results.extend(self.find_by_name("main"));
        
        // Pattern 2: CLI framework decorators/attributes
        // @click.command()
        // #[derive(Parser)]
        results.extend(self.find_by_decorator_pattern(&[
            "click.command", "click.group", "clap"
        ]));
        
        // Pattern 3: Argument parsers
        results.extend(self.find_by_imports(&[
            "argparse", "click", "clap", "commander"
        ]));
        
        results
    }
    
    fn find_public_apis(&self, options: &EntryPointOptions) -> Vec<EntryPoint> {
        // Pattern 1: Exported symbols
        self.graph.nodes()
            .filter(|node| node.is_exported)
            .map(|node| EntryPoint {
                node_id: node.id,
                entry_type: EntryType::PublicApi,
                route: None,
                method: None,
                description: node.docstring.clone(),
            })
            .collect()
    }
}
```

**Performance Target:** < 20ms (complex pattern matching)

**Example Usage:**
```rust
// Find all HTTP endpoints
let handlers = find_entry_points(EntryType::HttpHandler, EntryPointOptions {
    scope: SearchScope::Workspace,
    languages: vec![Language::TypeScript, Language::Python],
    framework: None,
});
// → [POST /login, GET /users, POST /register, ...]

// Find CLI commands
let commands = find_entry_points(EntryType::CliCommand, EntryPointOptions {
    scope: SearchScope::Workspace,
    languages: vec![Language::Rust],
    framework: Some("clap".to_string()),
});
// → [main, deploy, test, build, ...]
```

---

### 4.5 Primitive 5: traverse_graph

**Purpose:** Custom graph traversal with filters

**Signature:**
```rust
fn traverse_graph(
    start: Vec<NodeId>,
    direction: Direction,
    depth: u32,
    filter: TraversalFilter
) -> Result<Vec<TraversalNode>>;

enum Direction {
    Outgoing,   // Follow calls/dependencies
    Incoming,   // Follow callers/dependents
    Both,       // Bidirectional
}

struct TraversalFilter {
    node_filter: Option<Box<dyn Fn(&CodeNode) -> bool>>,
    edge_filter: Option<Box<dyn Fn(&EdgeType) -> bool>>,
    stop_condition: Option<Box<dyn Fn(&CodeNode, u32) -> bool>>,
    max_nodes: usize,  // Safety limit
}

struct TraversalNode {
    node_id: NodeId,
    depth: u32,
    path: Vec<NodeId>,  // Path from start
    edge_type: EdgeType,
}
```

**Implementation:**
```rust
impl GraphQueryEngine {
    fn traverse_graph(
        &self,
        start: Vec<NodeId>,
        direction: Direction,
        depth: u32,
        filter: TraversalFilter
    ) -> Vec<TraversalNode> {
        let mut visited = HashSet::new();
        let mut results = Vec::new();
        let mut queue = VecDeque::new();
        
        // Initialize queue
        for node in start {
            queue.push_back((node, 0, vec![node]));
        }
        
        while let Some((current, current_depth, path)) = queue.pop_front() {
            // Check limits
            if results.len() >= filter.max_nodes {
                break;
            }
            
            if current_depth > depth {
                continue;
            }
            
            if !visited.insert(current) {
                continue;
            }
            
            let node = self.graph.get_node(current)?;
            
            // Apply node filter
            if let Some(node_filter) = &filter.node_filter {
                if !node_filter(node) {
                    continue;
                }
            }
            
            // Apply stop condition
            if let Some(stop_condition) = &filter.stop_condition {
                if stop_condition(node, current_depth) {
                    continue;
                }
            }
            
            results.push(TraversalNode {
                node_id: current,
                depth: current_depth,
                path: path.clone(),
                edge_type: EdgeType::Unknown,  // Set based on how we reached it
            });
            
            // Get neighbors based on direction
            let neighbors = match direction {
                Direction::Outgoing => self.graph.outgoing_edges(current),
                Direction::Incoming => self.graph.incoming_edges(current),
                Direction::Both => {
                    let mut out = self.graph.outgoing_edges(current);
                    out.extend(self.graph.incoming_edges(current));
                    out
                }
            };
            
            // Filter edges and add to queue
            for (neighbor, edge_type) in neighbors {
                if let Some(edge_filter) = &filter.edge_filter {
                    if !edge_filter(&edge_type) {
                        continue;
                    }
                }
                
                let mut new_path = path.clone();
                new_path.push(neighbor);
                queue.push_back((neighbor, current_depth + 1, new_path));
            }
        }
        
        results
    }
}
```

**Performance Target:** < 15ms for 3-hop traversal

**Example Usage:**
```rust
// Find all functions transitively called by authenticate()
traverse_graph(
    vec![authenticate_node],
    Direction::Outgoing,
    3,
    TraversalFilter {
        node_filter: Some(Box::new(|node| node.kind == SymbolType::Function)),
        edge_filter: Some(Box::new(|edge| matches!(edge, EdgeType::Calls))),
        stop_condition: None,
        max_nodes: 1000,
    }
)

// Find all code that depends on a module (transitive)
traverse_graph(
    vec![module_node],
    Direction::Incoming,
    5,
    TraversalFilter {
        node_filter: None,
        edge_filter: Some(Box::new(|edge| matches!(edge, EdgeType::Imports))),
        stop_condition: Some(Box::new(|node, depth| {
            // Stop at module boundaries
            depth > 0 && node.kind == SymbolType::Module
        })),
        max_nodes: 5000,
    }
)
```

---

### 4.6 Primitive 6: get_callers / get_callees

**Purpose:** Fast relationship queries (optimized traverse_graph)

**Signature:**
```rust
fn get_callers(node: NodeId, depth: u32) -> Result<Vec<CallerInfo>>;
fn get_callees(node: NodeId, depth: u32) -> Result<Vec<CalleeInfo>>;

struct CallerInfo {
    caller: NodeId,
    call_site: Location,  // Where the call happens
    call_context: String, // Line of code with the call
    depth: u32,
}

struct CalleeInfo {
    callee: NodeId,
    call_site: Location,
    depth: u32,
}
```

**Implementation:**
```rust
impl GraphQueryEngine {
    fn get_callers(&self, node: NodeId, depth: u32) -> Vec<CallerInfo> {
        // Optimized version of traverse_graph
        // Pre-computed caller index for O(1) lookup
        
        if depth == 1 {
            // Fast path: direct callers only
            return self.caller_index.get(&node)
                .map(|callers| {
                    callers.iter().map(|caller| CallerInfo {
                        caller: *caller,
                        call_site: self.get_call_site(*caller, node),
                        call_context: self.get_call_context(*caller, node),
                        depth: 1,
                    }).collect()
                })
                .unwrap_or_default();
        }
        
        // Multi-hop: BFS traversal
        let mut results = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back((node, 0));
        
        while let Some((current, current_depth)) = queue.pop_front() {
            if current_depth >= depth {
                continue;
            }
            
            if let Some(callers) = self.caller_index.get(&current) {
                for caller in callers {
                    if visited.insert(*caller) {
                        results.push(CallerInfo {
                            caller: *caller,
                            call_site: self.get_call_site(*caller, current),
                            call_context: self.get_call_context(*caller, current),
                            depth: current_depth + 1,
                        });
                        
                        if current_depth + 1 < depth {
                            queue.push_back((*caller, current_depth + 1));
                        }
                    }
                }
            }
        }
        
        results
    }
    
    fn get_callees(&self, node: NodeId, depth: u32) -> Vec<CalleeInfo> {
        // Similar implementation but for callees
        // Uses callee_index for O(1) lookup
        // ...
    }
}
```

**Performance Target:** 
- Depth 1: < 5ms (index lookup)
- Depth 3: < 15ms (BFS with cache)

**Index Structure:**
```rust
struct RelationshipIndex {
    // NodeId → Direct callers
    caller_index: HashMap<NodeId, Vec<NodeId>>,
    
    // NodeId → Direct callees
    callee_index: HashMap<NodeId, Vec<NodeId>>,
    
    // (Caller, Callee) → Call site location
    call_sites: HashMap<(NodeId, NodeId), Location>,
}
```

---

### 4.7 Primitive 7: get_symbol_info

**Purpose:** Retrieve rich metadata for a symbol

**Signature:**
```rust
fn get_symbol_info(node: NodeId) -> Result<SymbolInfo>;

struct SymbolInfo {
    // Basic info
    name: String,
    kind: SymbolType,
    location: Location,
    signature: Option<String>,
    
    // Documentation
    docstring: Option<String>,
    comments: Vec<String>,
    
    // Relationships (1-hop only for speed)
    callers: Vec<SymbolRef>,      // Direct callers
    callees: Vec<SymbolRef>,      // Direct callees
    dependencies: Vec<ModulePath>, // Direct imports
    dependents: Vec<ModulePath>,   // Who imports this
    
    // Code metrics
    complexity: Option<u32>,       // Cyclomatic complexity
    lines_of_code: usize,
    test_coverage: Option<f32>,
    
    // Quality signals
    has_tests: bool,
    is_public: bool,
    is_deprecated: bool,
    reference_count: usize,        // How many places reference this
    
    // Temporal data
    last_modified: Option<DateTime>,
    created: Option<DateTime>,
    author: Option<String>,
}

struct SymbolRef {
    node_id: NodeId,
    name: String,
    location: Location,
}
```

**Implementation:**
```rust
impl GraphQueryEngine {
    fn get_symbol_info(&self, node: NodeId) -> Result<SymbolInfo> {
        let node = self.graph.get_node(node)?;
        
        Ok(SymbolInfo {
            // Basic info (cached)
            name: node.name.clone(),
            kind: node.kind,
            location: node.location.clone(),
            signature: node.signature.clone(),
            
            // Documentation (cached)
            docstring: node.docstring.clone(),
            comments: node.comments.clone(),
            
            // Relationships (indexed)
            callers: self.get_direct_callers(node.id),
            callees: self.get_direct_callees(node.id),
            dependencies: self.get_direct_imports(node.id),
            dependents: self.get_direct_importers(node.id),
            
            // Metrics (pre-computed or cached)
            complexity: self.complexity_cache.get(&node.id).cloned(),
            lines_of_code: node.end_line - node.start_line + 1,
            test_coverage: self.coverage_cache.get(&node.id).cloned(),
            
            // Quality signals
            has_tests: self.has_tests(node.id),
            is_public: node.is_exported,
            is_deprecated: self.is_deprecated(node),
            reference_count: self.get_reference_count(node.id),
            
            // Temporal data (from git)
            last_modified: self.git_info.get_last_modified(&node.location.file),
            created: self.git_info.get_created(&node.location.file),
            author: self.git_info.get_author(&node.location.file),
        })
    }
}
```

**Performance Target:** < 5ms (mostly cache lookups)

---

## 5. Language Model Tools

### 5.1 Tool Specifications

Each Language Model Tool wraps one or more query primitives:

#### Tool 1: codegraph_symbol_search

```typescript
{
  name: "codegraph_symbol_search",
  description: `Search for code symbols by name, docstring, or comments.
  
Use this when you need to find functions, classes, or variables by name or keyword.

Examples:
- "Find functions with 'email' in the name"
- "Search for validation code"
- "Locate database connection logic"`,
  
  inputSchema: {
    type: "object",
    properties: {
      query: {
        type: "string",
        description: "Search keywords (e.g., 'validate email')"
      },
      scope: {
        type: "string",
        enum: ["workspace", "module", "file"],
        default: "workspace",
        description: "Search scope"
      },
      symbolTypes: {
        type: "array",
        items: {
          type: "string",
          enum: ["function", "class", "variable", "module"]
        },
        description: "Filter by symbol types"
      },
      languages: {
        type: "array",
        items: { type: "string" },
        description: "Filter by programming languages"
      },
      limit: {
        type: "number",
        default: 20,
        description: "Maximum results to return"
      }
    },
    required: ["query"]
  }
}
```

**Response Format:**
```typescript
{
  results: [
    {
      symbol: {
        name: "validate_email",
        kind: "function",
        location: { file: "auth/validators.py", line: 45 },
        signature: "def validate_email(email: str) -> bool"
      },
      score: 9.2,
      matchReason: "Symbol name exact match",
      context: {
        callers: ["register_user", "update_profile"],
        imports: ["re"],
        hasTests: true
      }
    }
  ],
  totalMatches: 1,
  queryTime: "4ms"
}
```

---

#### Tool 2: codegraph_find_by_imports

```typescript
{
  name: "codegraph_find_by_imports",
  description: `Find code that imports specific libraries or modules.

Use this to discover code that uses particular dependencies or frameworks.

Examples:
- "Find all code that uses SQLAlchemy"
- "Show me JWT authentication code"
- "Locate regex usage"`,
  
  inputSchema: {
    type: "object",
    properties: {
      libraries: {
        type: "array",
        items: { type: "string" },
        description: "Library names to search for (e.g., ['re', 'email', 'validator'])"
      },
      matchMode: {
        type: "string",
        enum: ["exact", "prefix", "fuzzy"],
        default: "exact",
        description: "How to match library names"
      },
      includeTransitive: {
        type: "boolean",
        default: false,
        description: "Include code that imports code that imports these libraries"
      },
      languages: {
        type: "array",
        items: { type: "string" }
      }
    },
    required: ["libraries"]
  }
}
```

---

#### Tool 3: codegraph_find_entry_points

```typescript
{
  name: "codegraph_find_entry_points",
  description: `Discover architectural entry points in the codebase.

Use this to understand how users/systems interact with the code.

Entry point types:
- http_handler: API endpoints (GET /users, POST /login)
- cli_command: Command-line interfaces
- public_api: Exported functions/classes
- test_entry: Test functions
- main: Program entry points`,
  
  inputSchema: {
    type: "object",
    properties: {
      entryType: {
        type: "string",
        enum: ["http_handler", "cli_command", "public_api", "test_entry", "main"],
        description: "Type of entry point to find"
      },
      framework: {
        type: "string",
        description: "Specific framework (e.g., 'express', 'fastapi', 'clap')"
      },
      languages: {
        type: "array",
        items: { type: "string" }
      }
    },
    required: ["entryType"]
  }
}
```

**Response Format:**
```typescript
{
  entryPoints: [
    {
      symbol: "login_handler",
      location: { file: "api/auth.ts", line: 23 },
      entryType: "http_handler",
      route: "/api/login",
      method: "POST",
      description: "Handles user authentication"
    },
    {
      symbol: "register_handler",
      location: { file: "api/auth.ts", line: 45 },
      entryType: "http_handler",
      route: "/api/register",
      method: "POST"
    }
  ],
  totalFound: 2
}
```

---

#### Tool 4: codegraph_traverse_graph

```typescript
{
  name: "codegraph_traverse_graph",
  description: `Traverse the code graph with custom filters.

Use this for advanced queries like:
- "Find all functions called by authenticate()"
- "Show everything that depends on this module"
- "Trace execution flow from entry point"`,
  
  inputSchema: {
    type: "object",
    properties: {
      startNodes: {
        type: "array",
        items: {
          type: "object",
          properties: {
            uri: { type: "string" },
            line: { type: "number" }
          }
        },
        description: "Starting points for traversal"
      },
      direction: {
        type: "string",
        enum: ["outgoing", "incoming", "both"],
        default: "outgoing",
        description: "Traversal direction (calls vs callers)"
      },
      depth: {
        type: "number",
        default: 3,
        minimum: 1,
        maximum: 10,
        description: "Maximum traversal depth"
      },
      filterSymbolTypes: {
        type: "array",
        items: { type: "string" },
        description: "Only include these symbol types"
      },
      maxNodes: {
        type: "number",
        default: 1000,
        description: "Safety limit on results"
      }
    },
    required: ["startNodes"]
  }
}
```

---

#### Tool 5: codegraph_get_detailed_info

```typescript
{
  name: "codegraph_get_detailed_info",
  description: `Get comprehensive information about a specific symbol.

Returns:
- Signature and documentation
- Callers and callees (1-hop)
- Import dependencies
- Code metrics (complexity, LOC)
- Test coverage
- Quality signals (has tests, reference count)
- Git information (last modified, author)`,
  
  inputSchema: {
    type: "object",
    properties: {
      uri: { type: "string" },
      line: { type: "number" },
      includeCallers: { type: "boolean", default: true },
      includeCallees: { type: "boolean", default: true },
      includeTests: { type: "boolean", default: true }
    },
    required: ["uri", "line"]
  }
}
```

---

#### Tool 6: codegraph_find_by_signature

```typescript
{
  name: "codegraph_find_by_signature",
  description: `Find functions by signature patterns.

Use this to locate functions with specific characteristics:
- Return type (e.g., all functions returning bool)
- Parameter count (e.g., functions with 2-3 parameters)
- Modifiers (e.g., async public functions)
- Name patterns (e.g., functions with 'validate' in name)`,
  
  inputSchema: {
    type: "object",
    properties: {
      namePattern: {
        type: "string",
        description: "Regex pattern for function name (e.g., '.*valid.*')"
      },
      returnType: {
        type: "string",
        description: "Expected return type (e.g., 'bool', 'Promise<*>')"
      },
      paramCount: {
        type: "object",
        properties: {
          min: { type: "number" },
          max: { type: "number" }
        },
        description: "Parameter count range"
      },
      modifiers: {
        type: "array",
        items: {
          type: "string",
          enum: ["public", "private", "static", "async", "const"]
        }
      }
    }
  }
}
```

---

### 5.2 Tool Usage Patterns

**Pattern 1: Discovery → Detail**
```
AI Agent Flow:
1. codegraph_symbol_search("authentication")
   → Find auth-related symbols
   
2. codegraph_get_detailed_info(top_result)
   → Get full context for most relevant symbol
   
3. codegraph_traverse_graph(start=top_result, direction="outgoing", depth=2)
   → Trace execution flow
```

**Pattern 2: Architecture Understanding**
```
AI Agent Flow:
1. codegraph_find_entry_points("http_handler")
   → Discover all API endpoints
   
2. For each endpoint:
   codegraph_traverse_graph(start=endpoint, depth=3)
   → Map request handling flow
   
3. codegraph_find_by_imports(["database_library"])
   → Identify data layer
```

**Pattern 3: Impact Analysis**
```
AI Agent Flow:
1. codegraph_get_detailed_info(target_function)
   → Get callers and callees
   
2. codegraph_traverse_graph(start=target, direction="incoming", depth=5)
   → Find all upstream dependencies
   
3. codegraph_find_by_imports([libraries_used_by_target])
   → Find code with similar dependencies (might be affected)
```

---

## 6. Performance Architecture

### 6.1 Indexing Strategy

**Index Types:**

1. **Text Index** (BM25 for symbol search)
   - Memory: ~50 bytes × 10K symbols × 10 tokens = ~5MB
   - Build time: < 5 seconds for 10K symbols
   - Query time: < 5ms

2. **Import Index** (Hash map)
   - Memory: ~100 bytes × 10K imports = ~1MB
   - Build time: Instant (built during parsing)
   - Query time: < 1ms (hash lookup)

3. **Signature Index** (Multiple hash maps)
   - Memory: ~2MB for 10K functions
   - Build time: < 2 seconds
   - Query time: < 5ms

4. **Relationship Index** (Caller/callee)
   - Memory: ~200 bytes × 10K relationships = ~2MB
   - Build time: Instant (built during graph construction)
   - Query time: < 1ms (hash lookup)

5. **Entry Point Patterns** (Regex + pattern matching)
   - Memory: Negligible (patterns only)
   - Build time: N/A
   - Query time: < 20ms (pattern matching on demand)

**Total Memory Overhead:** ~10-15MB for 10K symbol codebase

**Total Build Time:** < 10 seconds for complete index

---

### 6.2 Caching Strategy

**Multi-Level Cache:**

```rust
struct QueryCache {
    // L1: Hot query results (LRU, 100 entries)
    hot_cache: LruCache<QueryKey, QueryResult>,
    
    // L2: Symbol metadata (content-hash based)
    symbol_cache: HashMap<ContentHash, SymbolInfo>,
    
    // L3: Graph traversal results (TTL-based, 5 minutes)
    traversal_cache: TtlCache<TraversalKey, Vec<NodeId>>,
}

struct QueryKey {
    query_type: QueryType,
    params_hash: u64,  // Hash of query parameters
}
```

**Cache Invalidation:**

```rust
impl QueryCache {
    fn invalidate_on_file_change(&mut self, file_path: &Path) {
        // Invalidate affected nodes
        let affected_nodes = self.get_nodes_in_file(file_path);
        
        for node in affected_nodes {
            // Invalidate symbol metadata
            self.symbol_cache.remove(&node.content_hash);
            
            // Invalidate traversal results that include this node
            self.traversal_cache.retain(|_, result| {
                !result.contains(&node.id)
            });
        }
        
        // Invalidate hot cache entries for this file
        self.hot_cache.retain(|key, _| {
            !self.query_touches_file(key, file_path)
        });
    }
}
```

**Cache Metrics to Track:**
- Hit rate (target: > 80%)
- Average lookup time (target: < 1ms)
- Memory usage (target: < 100MB)
- Eviction frequency

---

### 6.3 Performance Budgets

**Per-Query Budgets:**

| Query Type | Target | Maximum | Notes |
|------------|--------|---------|-------|
| symbol_search | 5ms | 10ms | Text index lookup |
| find_by_imports | 3ms | 5ms | Hash table lookup |
| find_by_signature | 8ms | 15ms | Multiple index lookups + regex |
| find_entry_points | 15ms | 25ms | Pattern matching across codebase |
| traverse_graph (depth=1) | 5ms | 10ms | Single hop, indexed |
| traverse_graph (depth=3) | 15ms | 30ms | Multi-hop BFS |
| get_callers/callees | 5ms | 10ms | Indexed relationship lookup |
| get_symbol_info | 3ms | 5ms | Cache-first metadata retrieval |

**Timeout Policy:**
- All queries: 5 second hard timeout
- Return partial results if timeout exceeded
- Log slow queries for optimization

**Scaling Targets:**

| Codebase Size | Index Time | Query Time (p95) | Memory Usage |
|---------------|------------|------------------|--------------|
| 10K LOC (1K symbols) | < 5s | < 10ms | < 20MB |
| 100K LOC (10K symbols) | < 30s | < 20ms | < 100MB |
| 1M LOC (100K symbols) | < 5min | < 50ms | < 500MB |

---

### 6.4 Incremental Updates

**File Change Detection:**

```rust
struct IncrementalIndexer {
    file_watcher: FileWatcher,
    dirty_files: HashSet<PathBuf>,
    reindex_queue: VecDeque<PathBuf>,
}

impl IncrementalIndexer {
    async fn on_file_changed(&mut self, file: PathBuf) {
        // Mark file as dirty
        self.dirty_files.insert(file.clone());
        
        // Debounce: wait 500ms for additional changes
        tokio::time::sleep(Duration::from_millis(500)).await;
        
        if self.dirty_files.contains(&file) {
            self.reindex_file(file).await;
        }
    }
    
    async fn reindex_file(&mut self, file: PathBuf) {
        // Parse file
        let new_ast = self.parser.parse_file(&file)?;
        
        // Get old nodes for this file
        let old_nodes = self.graph.get_nodes_in_file(&file);
        
        // Compute diff
        let diff = self.compute_diff(&old_nodes, &new_ast);
        
        // Update indexes incrementally
        for removed_node in diff.removed {
            self.remove_from_indexes(removed_node);
        }
        
        for added_node in diff.added {
            self.add_to_indexes(added_node);
        }
        
        for modified_node in diff.modified {
            self.update_in_indexes(modified_node);
        }
        
        // Invalidate caches
        self.cache.invalidate_on_file_change(&file);
        
        self.dirty_files.remove(&file);
    }
}
```

**Incremental Update Performance:**
- Target: < 100ms for single file change
- Batch updates if multiple files changed within 500ms
- Background processing to avoid blocking queries

---

## 7. Implementation Phases

### Phase 1: Foundation (Weeks 1-2)

**Goals:**
- Build core query primitives
- Implement basic indexing
- Prove performance targets

**Deliverables:**

**Week 1: Text Index & Symbol Search**
```rust
// Day 1-2: Text indexing
struct TextIndex {
    inverted_index: HashMap<String, Vec<Posting>>,
    doc_lengths: HashMap<NodeId, f32>,
}

impl TextIndex {
    fn build(nodes: &[CodeNode]) -> Self;
    fn search(&self, query: &str) -> Vec<(NodeId, f32)>;
}

// Day 3-4: BM25 ranking
impl TextIndex {
    fn compute_bm25_score(&self, node: NodeId, query: &str) -> f32;
    fn rank_results(&self, results: Vec<NodeId>) -> Vec<(NodeId, f32)>;
}

// Day 5: Integration
fn symbol_search(query: &str, options: SearchOptions) -> Vec<SymbolMatch>;

// Tests
#[test]
fn test_symbol_search_exact_match();
#[test]
fn test_symbol_search_fuzzy_match();
#[test]
fn test_symbol_search_performance();  // < 5ms for 10K symbols
```

**Week 2: Relationship Indexes & Graph Queries**
```rust
// Day 1-2: Import indexing
struct ImportIndex {
    library_to_nodes: HashMap<String, Vec<NodeId>>,
    node_to_libraries: HashMap<NodeId, Vec<String>>,
}

impl ImportIndex {
    fn build(graph: &CodeGraph) -> Self;
    fn find_by_imports(&self, libraries: &[&str]) -> Vec<NodeId>;
}

// Day 3-4: Caller/callee indexing
struct RelationshipIndex {
    caller_index: HashMap<NodeId, Vec<NodeId>>,
    callee_index: HashMap<NodeId, Vec<NodeId>>,
    call_sites: HashMap<(NodeId, NodeId), Location>,
}

impl RelationshipIndex {
    fn get_callers(&self, node: NodeId, depth: u32) -> Vec<CallerInfo>;
    fn get_callees(&self, node: NodeId, depth: u32) -> Vec<CalleeInfo>;
}

// Day 5: Graph traversal
fn traverse_graph(
    start: Vec<NodeId>,
    direction: Direction,
    depth: u32,
    filter: TraversalFilter
) -> Vec<TraversalNode>;

// Tests
#[test]
fn test_find_by_imports();
#[test]
fn test_get_callers_single_hop();  // < 5ms
#[test]
fn test_get_callers_multi_hop();   // < 15ms
#[test]
fn test_traverse_graph_performance();
```

**Success Criteria:**
- ✅ All 7 query primitives implemented
- ✅ All primitives meet performance targets
- ✅ Unit tests pass with 95%+ coverage
- ✅ Benchmark on 10K symbol codebase

---

### Phase 2: LSP Integration (Week 3)

**Goals:**
- Expose primitives via LSP custom methods
- Implement caching
- Add timeout handling

**Deliverables:**

**LSP Custom Methods:**
```rust
// In server/src/handlers/ai_agent_queries.rs

async fn handle_symbol_search(
    state: &ServerState,
    params: SymbolSearchParams
) -> Result<SymbolSearchResponse> {
    // Check cache
    if let Some(cached) = state.cache.get(&params) {
        return Ok(cached);
    }
    
    // Execute query with timeout
    let result = timeout(
        Duration::from_secs(5),
        state.query_engine.symbol_search(params.query, params.options)
    ).await??;
    
    // Cache result
    state.cache.insert(params, result.clone());
    
    Ok(result)
}

async fn handle_find_by_imports(
    state: &ServerState,
    params: FindByImportsParams
) -> Result<FindByImportsResponse>;

async fn handle_find_entry_points(
    state: &ServerState,
    params: FindEntryPointsParams
) -> Result<FindEntryPointsResponse>;

async fn handle_traverse_graph(
    state: &ServerState,
    params: TraverseGraphParams
) -> Result<TraverseGraphResponse>;

async fn handle_get_detailed_info(
    state: &ServerState,
    params: GetDetailedInfoParams
) -> Result<SymbolInfo>;

async fn handle_find_by_signature(
    state: &ServerState,
    params: FindBySignatureParams
) -> Result<FindBySignatureResponse>;
```

**Cache Implementation:**
```rust
struct QueryCache {
    hot_cache: LruCache<QueryKey, QueryResult>,
    symbol_cache: HashMap<ContentHash, SymbolInfo>,
    traversal_cache: TtlCache<TraversalKey, Vec<NodeId>>,
}

impl QueryCache {
    fn new() -> Self {
        Self {
            hot_cache: LruCache::new(100),
            symbol_cache: HashMap::new(),
            traversal_cache: TtlCache::new(Duration::from_secs(300)), // 5 min TTL
        }
    }
}
```

**Success Criteria:**
- ✅ All custom LSP methods working
- ✅ Cache hit rate > 80% on typical workloads
- ✅ Timeout handling prevents hangs
- ✅ Integration tests pass

---

### Phase 3: Language Model Tools (Week 4)

**Goals:**
- Register tools with VS Code Language Model API
- Implement tool formatters
- Test with AI agents

**Deliverables:**

**Tool Registration:**
```typescript
// In src/tools/aiAgentTools.ts

export function registerAIAgentTools(
    context: vscode.ExtensionContext,
    client: LanguageClient
): vscode.Disposable[] {
    const tools = [
        registerSymbolSearchTool(client),
        registerFindByImportsTool(client),
        registerFindEntryPointsTool(client),
        registerTraverseGraphTool(client),
        registerGetDetailedInfoTool(client),
        registerFindBySignatureTool(client),
    ];
    
    console.log(`[CodeGraph] Registered ${tools.length} AI Agent query tools`);
    
    return tools;
}
```

**Tool Implementation Example:**
```typescript
function registerSymbolSearchTool(client: LanguageClient): vscode.Disposable {
    return vscode.lm.registerTool('codegraph_symbol_search', {
        async invoke(request: SymbolSearchRequest, token: vscode.CancellationToken) {
            // Call LSP server
            const response = await client.sendRequest(
                'codegraph/symbolSearch',
                request,
                token
            );
            
            // Format for AI consumption
            return formatSymbolSearchResponse(response);
        }
    });
}

function formatSymbolSearchResponse(response: SymbolSearchResponse): vscode.LanguageModelToolResult {
    // Format as structured data that AI can parse
    const text = `Found ${response.results.length} symbols:\n\n` +
        response.results.map(r => 
            `${r.symbol.name} (${r.symbol.kind}) at ${r.symbol.location.file}:${r.symbol.location.line}\n` +
            `  Score: ${r.score.toFixed(2)} - ${r.matchReason}\n` +
            `  Signature: ${r.symbol.signature || 'N/A'}\n` +
            `  Context: ${r.context.callers.length} callers, ${r.context.hasTests ? 'has tests' : 'no tests'}`
        ).join('\n\n');
    
    return new vscode.LanguageModelToolResult([
        new vscode.LanguageModelTextPart(text)
    ]);
}
```

**Testing with AI Agents:**
```typescript
// Manual test script
async function testWithAIAgent() {
    const agent = await vscode.lm.selectChatModels({ vendor: 'copilot' })[0];
    
    const messages = [
        vscode.LanguageModelChatMessage.User(
            "Find email validation code in this project"
        )
    ];
    
    const response = await agent.sendRequest(messages, {
        tools: ['codegraph_symbol_search', 'codegraph_find_by_imports']
    });
    
    // Verify AI agent called our tools
    console.log('Tool calls:', response.toolCalls);
    console.log('Response:', response.text);
}
```

**Success Criteria:**
- ✅ All 6 new tools registered
- ✅ Tools discoverable by AI agents
- ✅ AI agent successfully calls tools
- ✅ Formatted responses are parseable by AI
- ✅ Manual testing with Claude Code / GitHub Copilot

---

### Phase 4: Advanced Features (Week 5-6)

**Goals:**
- Signature-based search
- Entry point detection
- Performance optimization

**Week 5: Signature Search & Entry Points**
```rust
// Signature index
struct SignatureIndex {
    return_type_index: HashMap<String, Vec<NodeId>>,
    param_count_index: HashMap<usize, Vec<NodeId>>,
    modifier_index: HashMap<Modifier, Vec<NodeId>>,
}

// Entry point detection
impl EntryPointDetector {
    fn find_http_handlers(&self) -> Vec<EntryPoint>;
    fn find_cli_commands(&self) -> Vec<EntryPoint>;
    fn find_public_apis(&self) -> Vec<EntryPoint>;
}

// Pattern matching for decorators/attributes
impl PatternMatcher {
    fn find_by_decorator(&self, patterns: &[&str]) -> Vec<NodeId>;
    fn find_by_attribute(&self, patterns: &[&str]) -> Vec<NodeId>;
}
```

**Week 6: Performance Optimization**
```rust
// Benchmark suite
#[bench]
fn bench_symbol_search_10k_symbols(b: &mut Bencher);

#[bench]
fn bench_traverse_graph_depth_3(b: &mut Bencher);

#[bench]
fn bench_find_by_imports(b: &mut Bencher);

// Profiling
fn profile_query_performance() {
    let profiler = Profiler::new();
    
    profiler.measure("symbol_search", || {
        symbol_search("validate", SearchOptions::default())
    });
    
    profiler.measure("traverse_graph", || {
        traverse_graph(vec![node_id], Direction::Outgoing, 3, TraversalFilter::default())
    });
    
    profiler.report();
}

// Optimization targets
- Reduce allocations in hot paths
- Use binary search for sorted indexes
- Implement query plan optimization
- Add query result streaming for large results
```

**Success Criteria:**
- ✅ Signature search working
- ✅ Entry point detection for 3+ frameworks
- ✅ All queries meet performance budgets
- ✅ Benchmark suite in place

---

### Phase 5: Documentation & Polish (Week 7)

**Goals:**
- Comprehensive documentation
- Example workflows
- Video demos

**Deliverables:**

**Documentation:**
- API reference for all query primitives
- Tool usage examples
- Architecture diagrams
- Performance characteristics

**Example Workflows:**
```markdown
# AI Agent Workflow Examples

## Workflow 1: "How does authentication work?"

1. **Find entry points**
   ```typescript
   codegraph_find_entry_points({
     entryType: "http_handler",
     framework: "express"
   })
   ```
   Result: `/api/login`, `/api/register` handlers

2. **Trace execution flow**
   ```typescript
   codegraph_traverse_graph({
     startNodes: [{ uri: "api/auth.ts", line: 23 }],
     direction: "outgoing",
     depth: 3
   })
   ```
   Result: login → middleware → verifyToken → queryDB

3. **Get implementation details**
   ```typescript
   codegraph_get_detailed_info({
     uri: "auth/tokens.ts",
     line: 45
   })
   ```
   Result: verifyToken function signature, callers, tests

4. **AI Synthesis**
   "Authentication works by: (1) POST /api/login receives credentials,
   (2) authMiddleware intercepts request, (3) verifyToken validates JWT,
   (4) queryDB checks credentials, (5) returns session token..."

## Workflow 2: "Find unused code"

1. **Get all public APIs**
   ```typescript
   codegraph_find_entry_points({ entryType: "public_api" })
   ```

2. **For each public API, get callers**
   ```typescript
   codegraph_get_callers({ uri, line }, depth: 1)
   ```

3. **Identify functions with 0 callers**
   
4. **Verify not called via dynamic dispatch**
   ```typescript
   codegraph_symbol_search(functionName)
   ```
   Check if name appears in strings, reflection calls, etc.
```

**Video Demos:**
- "AI Agent explores unfamiliar codebase in 30 seconds"
- "Token reduction: 100K → 3K tokens"
- "Architecture understanding with entry point detection"

**Success Criteria:**
- ✅ Complete API documentation
- ✅ 5+ example workflows
- ✅ 3+ demo videos
- ✅ User guide published

---

## 8. Testing Strategy

### 8.1 Unit Tests

**Coverage Targets:**
- Query primitives: 95%+
- Indexes: 90%+
- LSP handlers: 85%+
- Tool formatters: 80%+

**Test Categories:**

```rust
// 1. Correctness tests
#[test]
fn test_symbol_search_finds_exact_match() {
    let index = build_test_index();
    let results = index.symbol_search("validate_email", &SearchOptions::default());
    assert_eq!(results[0].symbol.name, "validate_email");
}

// 2. Performance tests
#[test]
fn test_symbol_search_meets_performance_target() {
    let index = build_large_index(10_000);  // 10K symbols
    
    let start = Instant::now();
    let _results = index.symbol_search("validate", &SearchOptions::default());
    let elapsed = start.elapsed();
    
    assert!(elapsed < Duration::from_millis(5), "Expected < 5ms, got {:?}", elapsed);
}

// 3. Edge case tests
#[test]
fn test_traverse_graph_handles_cycles() {
    let graph = build_cyclic_graph();
    let results = traverse_graph(vec![node_a], Direction::Outgoing, 10, TraversalFilter::default());
    
    // Should not infinite loop
    assert!(results.len() < 100);
    
    // Should visit each node once
    let unique_nodes: HashSet<_> = results.iter().map(|r| r.node_id).collect();
    assert_eq!(unique_nodes.len(), results.len());
}

// 4. Timeout tests
#[test]
fn test_query_respects_timeout() {
    let index = build_pathological_index();  // Very slow query
    
    let result = timeout(
        Duration::from_secs(1),
        index.traverse_graph(vec![root], Direction::Outgoing, 100, TraversalFilter::default())
    ).await;
    
    assert!(result.is_err(), "Query should timeout");
}
```

---

### 8.2 Integration Tests

**Test Scenarios:**

```typescript
// Test 1: End-to-end tool invocation
test('AI agent can discover authentication code', async () => {
    const workspace = await loadTestWorkspace('auth-app');
    
    // Step 1: Find entry points
    const entryPoints = await callTool('codegraph_find_entry_points', {
        entryType: 'http_handler'
    });
    
    expect(entryPoints.results).toContainEqual(
        expect.objectContaining({
            route: '/api/login',
            method: 'POST'
        })
    );
    
    // Step 2: Trace flow
    const flow = await callTool('codegraph_traverse_graph', {
        startNodes: [{ uri: entryPoints.results[0].location.file, line: entryPoints.results[0].location.line }],
        direction: 'outgoing',
        depth: 3
    });
    
    // Should find auth middleware, token validation, database query
    expect(flow.nodes.map(n => n.symbol.name)).toContain('verifyToken');
    expect(flow.nodes.map(n => n.symbol.name)).toContain('queryDatabase');
});

// Test 2: Cache effectiveness
test('repeated queries use cache', async () => {
    const query = { query: 'validate_email' };
    
    // First call
    const start1 = Date.now();
    const result1 = await callTool('codegraph_symbol_search', query);
    const time1 = Date.now() - start1;
    
    // Second call (should hit cache)
    const start2 = Date.now();
    const result2 = await callTool('codegraph_symbol_search', query);
    const time2 = Date.now() - start2;
    
    expect(result1).toEqual(result2);
    expect(time2).toBeLessThan(time1 / 2);  // Cache should be 2x+ faster
});

// Test 3: Incremental updates
test('file changes trigger incremental reindex', async () => {
    const workspace = await loadTestWorkspace('small-project');
    
    // Initial state
    const results1 = await callTool('codegraph_symbol_search', { query: 'oldFunction' });
    expect(results1.results).toHaveLength(1);
    
    // Modify file
    await workspace.editFile('src/module.ts', content => 
        content.replace('oldFunction', 'newFunction')
    );
    
    // Wait for reindex
    await waitForReindex();
    
    // New query should reflect changes
    const results2 = await callTool('codegraph_symbol_search', { query: 'oldFunction' });
    expect(results2.results).toHaveLength(0);
    
    const results3 = await callTool('codegraph_symbol_search', { query: 'newFunction' });
    expect(results3.results).toHaveLength(1);
});
```

---

### 8.3 Performance Tests

**Benchmark Suite:**

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};

fn benchmark_symbol_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("symbol_search");
    
    for size in [100, 1_000, 10_000, 100_000].iter() {
        let index = build_index_with_size(*size);
        
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| {
                index.symbol_search(black_box("validate"), &SearchOptions::default())
            });
        });
    }
    
    group.finish();
}

fn benchmark_traverse_graph(c: &mut Criterion) {
    let mut group = c.benchmark_group("traverse_graph");
    
    for depth in [1, 2, 3, 5].iter() {
        let graph = build_test_graph();
        
        group.bench_with_input(BenchmarkId::from_parameter(depth), depth, |b, &d| {
            b.iter(|| {
                traverse_graph(
                    black_box(vec![root_node]),
                    Direction::Outgoing,
                    d,
                    TraversalFilter::default()
                )
            });
        });
    }
    
    group.finish();
}

criterion_group!(benches, benchmark_symbol_search, benchmark_traverse_graph);
criterion_main!(benches);
```

**Performance Regression Tests:**

```rust
#[test]
fn test_performance_regression() {
    let index = build_standard_10k_index();
    
    let benchmarks = vec![
        ("symbol_search", Duration::from_millis(5)),
        ("find_by_imports", Duration::from_millis(3)),
        ("get_callers_depth_1", Duration::from_millis(5)),
        ("traverse_graph_depth_3", Duration::from_millis(15)),
    ];
    
    for (name, target) in benchmarks {
        let elapsed = measure_query(name, &index);
        
        assert!(
            elapsed < target,
            "{} exceeded target: {:?} > {:?}",
            name, elapsed, target
        );
    }
}
```

---

### 8.4 AI Agent Integration Tests

**Real-world Scenarios:**

```typescript
// Test with actual AI agent
async function testAIAgentIntegration() {
    const testCases = [
        {
            prompt: "How does authentication work in this app?",
            expectedTools: ['codegraph_find_entry_points', 'codegraph_traverse_graph'],
            expectedMentions: ['login', 'verifyToken', 'authenticate']
        },
        {
            prompt: "Find all database access code",
            expectedTools: ['codegraph_find_by_imports', 'codegraph_symbol_search'],
            expectedMentions: ['database', 'query', 'connection']
        },
        {
            prompt: "Show me email validation logic",
            expectedTools: ['codegraph_symbol_search', 'codegraph_get_detailed_info'],
            expectedMentions: ['validate_email', 'regex', 'email']
        }
    ];
    
    for (const testCase of testCases) {
        const agent = await vscode.lm.selectChatModels()[0];
        
        const messages = [
            vscode.LanguageModelChatMessage.User(testCase.prompt)
        ];
        
        const response = await agent.sendRequest(messages, {
            tools: getAllCodeGraphTools()
        });
        
        // Verify AI called expected tools
        const calledTools = response.toolCalls.map(t => t.name);
        for (const expectedTool of testCase.expectedTools) {
            expect(calledTools).toContain(expectedTool);
        }
        
        // Verify response mentions key concepts
        for (const mention of testCase.expectedMentions) {
            expect(response.text.toLowerCase()).toContain(mention.toLowerCase());
        }
    }
}
```

---

## 9. Success Metrics

### 9.1 Technical Metrics

**Performance Metrics:**
- Query latency (p50, p95, p99)
  - Target p95: < 20ms for all queries
  - Target p99: < 50ms
- Cache hit rate
  - Target: > 80%
- Memory usage
  - Target: < 100MB for 100K LOC codebase
- Indexing time
  - Target: < 30s for 100K LOC
  - Target: < 100ms for incremental update

**Quality Metrics:**
- Query precision (relevant results / total results)
  - Target: > 90%
- Query recall (relevant results found / total relevant)
  - Target: > 85%
- AI agent success rate
  - Target: > 90% of queries answered correctly

**Reliability Metrics:**
- Error rate
  - Target: < 1%
- Timeout rate
  - Target: < 0.1%
- Crash rate
  - Target: < 0.01%

---

### 9.2 User Experience Metrics

**Efficiency Gains:**
- Token reduction vs grep-based search
  - Target: 75-90% reduction
- Number of tool calls per user question
  - Target: 3-5 calls average
  - Max: 10 calls for complex queries
- Time to answer
  - Target: < 5 seconds total for typical questions

**Adoption Metrics:**
- Tool invocation frequency
  - Target: 1000+ invocations/day at 1K users
- User retention
  - Target: 60%+ users active after 30 days
- AI agent integration
  - Target: Works with Claude Code, GitHub Copilot, and other agents

---

### 9.3 Measurement Infrastructure

**Telemetry Collection:**

```rust
struct QueryMetrics {
    query_type: QueryType,
    duration_ms: u64,
    result_count: usize,
    cache_hit: bool,
    error: Option<String>,
    timestamp: DateTime,
}

impl TelemetryCollector {
    fn record_query(&self, metrics: QueryMetrics) {
        // Log to structured format
        info!(
            query_type = ?metrics.query_type,
            duration_ms = metrics.duration_ms,
            result_count = metrics.result_count,
            cache_hit = metrics.cache_hit,
            "Query executed"
        );
        
        // Update aggregates
        self.update_percentiles(metrics.query_type, metrics.duration_ms);
        self.update_cache_stats(metrics.cache_hit);
    }
}
```

**Dashboard Metrics:**

```typescript
interface DashboardMetrics {
    // Performance
    queryLatencyP50: number;
    queryLatencyP95: number;
    queryLatencyP99: number;
    cacheHitRate: number;
    
    // Usage
    dailyActiveUsers: number;
    queriesPerUser: number;
    mostUsedTools: Array<{ tool: string; count: number }>;
    
    // Quality
    errorRate: number;
    timeoutRate: number;
    avgResultsPerQuery: number;
    
    // Efficiency
    avgTokensPerQuery: number;
    tokenReductionVsGrep: number;
    avgToolCallsPerQuestion: number;
}
```

---

## 10. Migration Path

### 10.1 From Semantic Search Plan

If previously planned semantic search implementation:

**Phase 1: Parallel Implementation (Week 1-2)**
- Build graph-based query system alongside existing code
- No breaking changes to existing features
- Feature flag: `enableAIAgentQueries`

**Phase 2: Comparison (Week 3)**
- A/B test both approaches
- Measure performance, accuracy, user satisfaction
- Collect feedback from early users

**Phase 3: Transition (Week 4)**
- If graph-based approach proves superior:
  - Make it the default
  - Deprecate semantic search (if implemented)
  - Provide migration guide

**Phase 4: Cleanup (Week 5+)**
- Remove deprecated code
- Optimize based on usage patterns
- Scale up infrastructure

---

### 10.2 Backwards Compatibility

**Existing Tools:**
- All 9 existing Language Model Tools remain unchanged
- New 6 tools are additive, not replacements

**Existing LSP Methods:**
- All standard LSP methods (goto_definition, references, etc.) unchanged
- Custom methods (getDependencyGraph, getCallGraph) remain supported
- New custom methods are additions

**Existing Features:**
- Graph visualization: Works as before
- Call hierarchy: Works as before
- Impact analysis: Works as before
- Chat participant: Enhanced with new query capabilities

---

### 10.3 Deployment Strategy

**Rollout Plan:**

**Week 1-2: Alpha (Internal Testing)**
- Deploy to dev environment
- Test with internal AI agents
- Fix critical bugs

**Week 3-4: Beta (Early Adopters)**
- Release to 10-20 beta users
- Collect feedback and metrics
- Performance tuning

**Week 5: Public Release**
- Publish to VS Code Marketplace
- Blog post and documentation
- Monitor adoption and errors

**Week 6+: Iteration**
- Optimize based on real-world usage
- Add features based on feedback
- Scale infrastructure

---

## Appendix A: Data Structures

### Core Data Structures

```rust
// Node representation
struct CodeNode {
    id: NodeId,
    name: String,
    kind: SymbolType,
    location: Location,
    signature: Option<String>,
    docstring: Option<String>,
    comments: Vec<String>,
    is_exported: bool,
    content_hash: ContentHash,
}

// Index structures
struct TextIndex {
    inverted_index: HashMap<String, Vec<Posting>>,
    doc_lengths: HashMap<NodeId, f32>,
    avg_document_length: f32,
    total_docs: usize,
}

struct Posting {
    node_id: NodeId,
    term_frequency: f32,
    weight: f32,
    position: usize,
}

struct ImportIndex {
    library_to_nodes: HashMap<String, Vec<NodeId>>,
    node_to_libraries: HashMap<NodeId, Vec<String>>,
}

struct RelationshipIndex {
    caller_index: HashMap<NodeId, Vec<NodeId>>,
    callee_index: HashMap<NodeId, Vec<NodeId>>,
    call_sites: HashMap<(NodeId, NodeId), Location>,
}

struct SignatureIndex {
    return_type_index: HashMap<String, Vec<NodeId>>,
    param_count_index: HashMap<usize, Vec<NodeId>>,
    modifier_index: HashMap<Modifier, Vec<NodeId>>,
}
```

---

## Appendix B: Performance Profiling

### Profiling Checklist

```bash
# 1. CPU profiling
cargo build --release
perf record --call-graph dwarf ./target/release/codegraph-lsp
perf report

# 2. Memory profiling
valgrind --tool=massif ./target/release/codegraph-lsp
ms_print massif.out.*

# 3. Benchmarking
cargo bench --bench query_primitives

# 4. Flame graph
cargo flamegraph --bench query_primitives

# 5. Query profiling
RUST_LOG=debug cargo run --release
# Look for slow query warnings
```

---

## Appendix C: Tool Invocation Examples

### Example 1: Authentication Discovery

```
User: "How does this app handle authentication?"

AI Agent Plan:
→ codegraph_find_entry_points({ entryType: "http_handler" })
→ codegraph_traverse_graph({ 
    startNodes: [POST /login handler],
    direction: "outgoing",
    depth: 3
  })
→ codegraph_get_detailed_info(verifyToken function)

Result:
"Authentication flow:
1. POST /api/login receives credentials
2. authMiddleware intercepts request  
3. verifyToken() validates JWT using jose library
4. queryDatabase() checks credentials against users table
5. Returns session token or 401 error

Key files:
- api/auth.ts (entry points)
- middleware/auth.ts (verification logic)
- db/users.ts (credential checking)"
```

### Example 2: Database Access Discovery

```
User: "Show me all database access code"

AI Agent Plan:
→ codegraph_find_by_imports({ 
    libraries: ["prisma", "sqlalchemy", "diesel", "pg"],
    matchMode: "fuzzy"
  })
→ For top 5 results:
    codegraph_get_detailed_info(each result)

Result:
"Found 12 files accessing the database:

Primary data access:
- db/users.ts: User CRUD operations (Prisma)
- db/posts.ts: Post queries (Prisma)  
- db/migrations/: Schema definitions

Helper utilities:
- db/connection.ts: Database connection pool
- db/seeds.ts: Test data generation

All database code uses Prisma ORM with PostgreSQL."
```

---

## Document History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | 2024-12-31 | CodeGraph Team | Initial draft |

---

