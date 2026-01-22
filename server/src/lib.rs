//! CodeGraph LSP Server Library
//!
//! This crate implements a Language Server Protocol (LSP) server for CodeGraph,
//! providing cross-language code intelligence through graph-based analysis.

pub mod ai_query;
pub mod backend;
pub mod cache;
pub mod custom_requests;
pub mod error;
pub mod git_mining;
pub mod handlers;
pub mod index;
pub mod memory;
pub mod parser_registry;
pub mod watcher;

pub use backend::CodeGraphBackend;
pub use error::LspError;
pub use git_mining::{GitMiner, MiningConfig, MiningResult};
pub use memory::MemoryManager;
pub use parser_registry::ParserRegistry;
