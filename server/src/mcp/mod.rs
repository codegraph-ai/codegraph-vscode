//! MCP (Model Context Protocol) Server Module
//!
//! This module implements an MCP server for CodeGraph, allowing AI clients
//! like Claude Desktop, Cursor, and Cline to interact with the code graph.
//!
//! ## Usage
//!
//! ```bash
//! codegraph-lsp --mcp --workspace /path/to/project
//! ```
//!
//! The MCP server communicates via stdio using JSON-RPC 2.0.

pub mod protocol;
pub mod resources;
pub mod server;
pub mod tools;
pub mod transport;

pub use protocol::*;
pub use server::McpServer;
