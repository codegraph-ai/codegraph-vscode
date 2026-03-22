//! Embedding module for semantic search
//!
//! Uses fastembed with Jina Code V2 (768d) for code-aware semantic embeddings.

mod engine;
mod fastembed_embed;

pub use engine::VectorEngine;
