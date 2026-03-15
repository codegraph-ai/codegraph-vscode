//! Domain layer — transport-agnostic business logic.
//!
//! This module contains the core domain operations shared by both the LSP
//! and MCP transports. Functions here know nothing about JSON-RPC, tower-lsp,
//! or MCP protocol types.

pub(crate) mod complexity;
pub(crate) mod node_props;
pub(crate) mod node_resolution;
pub(crate) mod source_code;
