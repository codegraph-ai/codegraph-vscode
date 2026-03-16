//! Edit context assembly — transport-agnostic.
//!
//! Extracts get_edit_context from MCP server.
//! Composes: symbol source, callers, tests, memories, recent git changes.

use crate::ai_query::{EntryType, QueryEngine};
use crate::domain::{node_props, source_code};
use crate::git_mining::GitExecutor;
use crate::memory::MemoryManager;
use codegraph::{CodeGraph, NodeId, NodeType};
use serde_json::Value;
use std::path::PathBuf;
use tokio::sync::RwLock;

// ============================================================
// Domain Function
// ============================================================

/// Assemble comprehensive edit context for a file + line in a single call.
///
/// `file_path` should be the resolved filesystem path (not a URI).
/// `uri` is used in the response for location references.
///
/// Composes: symbol source, callers, tests, memories, and recent git changes.
/// Token budget is allocated with priority: symbol > callers > tests > memories > git.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn get_edit_context(
    graph: &RwLock<CodeGraph>,
    query_engine: &QueryEngine,
    memory_manager: &MemoryManager,
    workspace_folders: &[PathBuf],
    file_path: &str,
    uri: &str,
    line: u32,
    max_tokens: usize,
) -> Value {
    use crate::domain::node_resolution;

    let start_time = std::time::Instant::now();

    // --- Resolve target symbol ---
    let (target, used_fallback) = {
        let g = graph.read().await;
        match node_resolution::find_nearest_node(&g, file_path, line) {
            Some(result) => result,
            None => {
                return serde_json::json!({
                    "error": "No symbols found at this location. Try indexing the workspace first.",
                    "uri": uri,
                    "line": line
                });
            }
        }
    };

    // Extract symbol metadata
    let (name, node_type, language, sym_path, line_start, line_end) = {
        let g = graph.read().await;
        let node = match g.get_node(target) {
            Ok(n) => n,
            Err(_) => return serde_json::json!({ "error": "Could not load node" }),
        };
        let name = node_props::name(node).to_string();
        let node_type = format!("{:?}", node.node_type).to_lowercase();
        let language = node
            .properties
            .get_string("language")
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                node.properties
                    .get_string("path")
                    .and_then(|p| {
                        std::path::Path::new(p)
                            .extension()
                            .and_then(|e| e.to_str())
                            .map(|e| e.to_string())
                    })
                    .unwrap_or_else(|| "unknown".to_string())
            });
        let sym_path = node_props::path(node).to_string();
        let line_start = node_props::line_start(node) as i64;
        let line_end = {
            let e = node_props::line_end(node) as i64;
            if e == 0 {
                line_start
            } else {
                e
            }
        };
        (name, node_type, language, sym_path, line_start, line_end)
    };

    // --- Section 1: Symbol source code (budget: up to 30%) ---
    let source_code_str = {
        let g = graph.read().await;
        source_code::get_symbol_source(&g, target)
            .unwrap_or_else(|| "<source not available>".to_string())
    };
    let source_tokens = source_code_str.len() / 4;
    let mut budget_remaining = max_tokens.saturating_sub(source_tokens);

    let symbol = serde_json::json!({
        "name": name,
        "type": node_type,
        "code": source_code_str,
        "language": language,
        "location": {
            "uri": uri,
            "range": {
                "start": { "line": line_start, "character": 0 },
                "end": { "line": line_end, "character": 0 },
            }
        }
    });

    // --- Section 2: Callers (budget: up to 25% of original) ---
    let caller_budget = max_tokens / 4;
    let callers_json = {
        let callers = query_engine.get_callers(target, 1).await;
        let mut caller_tokens_used = 0usize;
        let mut caller_list = Vec::new();

        for caller in callers.iter().take(10) {
            if caller_tokens_used >= caller_budget {
                break;
            }
            let code = {
                let g = graph.read().await;
                source_code::get_symbol_source(&g, caller.node_id)
            };
            let code_tokens = code.as_ref().map(|c| c.len() / 4).unwrap_or(0);
            caller_tokens_used += code_tokens;

            caller_list.push(serde_json::json!({
                "name": caller.symbol.name,
                "code": code,
                "file": caller.symbol.location.file,
                "line": caller.symbol.location.line,
            }));
        }
        budget_remaining = budget_remaining.saturating_sub(caller_tokens_used);
        caller_list
    };

    // --- Section 3: Related tests (budget: up to 20% of original) ---
    let test_budget = max_tokens / 5;
    let tests_json = {
        let mut test_list = Vec::new();
        let mut test_tokens_used = 0usize;
        let mut seen_ids = std::collections::HashSet::<NodeId>::new();
        seen_ids.insert(target);

        // Stage 1: Tests that call target
        let entry_types = vec![EntryType::TestEntry];
        let tests = query_engine.find_entry_points(&entry_types).await;

        for test in tests.iter().take(20) {
            if test_list.len() >= 5 || test_tokens_used >= test_budget {
                break;
            }
            let callees = query_engine.get_callees(test.node_id, 3).await;
            if callees.iter().any(|c| c.node_id == target) && seen_ids.insert(test.node_id) {
                let code = {
                    let g = graph.read().await;
                    source_code::get_symbol_source(&g, test.node_id)
                };
                let code_tokens = code.as_ref().map(|c| c.len() / 4).unwrap_or(0);
                test_tokens_used += code_tokens;

                test_list.push(serde_json::json!({
                    "name": test.symbol.name,
                    "relationship": "calls_target",
                    "code": code,
                }));
            }
        }

        // Stage 2: Same-file test functions (if room)
        if test_list.len() < 5 {
            let g = graph.read().await;
            if let Ok(file_nodes) = g.query().property("path", sym_path.clone()).execute() {
                for node_id in file_nodes {
                    if test_list.len() >= 5 || test_tokens_used >= test_budget {
                        break;
                    }
                    if !seen_ids.insert(node_id) {
                        continue;
                    }
                    if let Ok(node) = g.get_node(node_id) {
                        if node.node_type == NodeType::Function
                            && crate::domain::unused_code::is_test_node(node)
                        {
                            let test_name = node_props::name(node).to_string();
                            test_list.push(serde_json::json!({
                                "name": test_name,
                                "relationship": "same_file",
                            }));
                        }
                    }
                }
            }
        }

        budget_remaining = budget_remaining.saturating_sub(test_tokens_used);
        test_list
    };

    // --- Section 4: Memories (budget: up to 15% of original) ---
    let memories_json = {
        let search_query = if sym_path.is_empty() {
            name.clone()
        } else {
            sym_path.clone()
        };

        let config = crate::memory::SearchConfig {
            limit: 5,
            current_only: true,
            ..Default::default()
        };

        match memory_manager.search(&search_query, &config, &[]).await {
            Ok(results) => {
                let memory_budget = max_tokens * 15 / 100;
                let mut mem_tokens_used = 0usize;
                let mut mem_list = Vec::new();
                let mut seen_titles = std::collections::HashSet::new();

                for r in &results {
                    if mem_tokens_used >= memory_budget {
                        break;
                    }
                    if !seen_titles.insert(r.memory.title.clone()) {
                        continue;
                    }
                    let content_tokens = r.memory.content.len() / 4;
                    mem_tokens_used += content_tokens;

                    mem_list.push(serde_json::json!({
                        "id": r.memory.id,
                        "title": r.memory.title,
                        "content": r.memory.content,
                        "kind": r.memory.kind.discriminant_name(),
                        "score": r.score,
                    }));
                }

                budget_remaining = budget_remaining.saturating_sub(mem_tokens_used);
                mem_list
            }
            Err(_) => Vec::new(),
        }
    };

    // --- Section 5: Recent git changes (budget: up to 10% of original) ---
    let git_json = {
        match workspace_folders.first().cloned() {
            Some(ws) => {
                let file_path_clone = sym_path.clone();
                let git_result = tokio::task::spawn_blocking(move || {
                    let executor = GitExecutor::new(&ws).ok()?;
                    let log_output = executor
                        .log(
                            "%H%x00%s%x00%an%x00%ai",
                            Some(10),
                            Some(std::path::Path::new(&file_path_clone)),
                        )
                        .ok()?;

                    let commits: Vec<Value> = log_output
                        .lines()
                        .filter(|l| !l.is_empty())
                        .take(5)
                        .filter_map(|line| {
                            let parts: Vec<&str> = line.split('\0').collect();
                            if parts.len() >= 4 {
                                Some(serde_json::json!({
                                    "hash": &parts[0][..8.min(parts[0].len())],
                                    "subject": parts[1],
                                    "author": parts[2],
                                    "date": parts[3],
                                }))
                            } else {
                                None
                            }
                        })
                        .collect();
                    Some(commits)
                })
                .await
                .ok()
                .flatten()
                .unwrap_or_default();
                git_result
            }
            None => Vec::new(),
        }
    };

    let query_time = start_time.elapsed().as_millis() as u64;
    let total_tokens = max_tokens.saturating_sub(budget_remaining);

    let mut response = serde_json::json!({
        "symbol": symbol,
        "callers": callers_json,
        "tests": tests_json,
        "memories": memories_json,
        "recentChanges": git_json,
        "metadata": {
            "totalTokens": total_tokens,
            "maxTokens": max_tokens,
            "queryTime": query_time,
            "sections": {
                "symbol": !source_code_str.is_empty(),
                "callers": !callers_json.is_empty(),
                "tests": !tests_json.is_empty(),
                "memories": !memories_json.is_empty(),
                "recentChanges": !git_json.is_empty(),
            }
        }
    });

    if used_fallback {
        if let Some(obj) = response.get_mut("metadata").and_then(|m| m.as_object_mut()) {
            obj.insert("usedFallback".to_string(), serde_json::json!(true));
            obj.insert(
                "fallbackMessage".to_string(),
                serde_json::json!(format!(
                    "No symbol at line {}. Using nearest symbol '{}' instead.",
                    line, name
                )),
            );
        }
    }

    response
}
