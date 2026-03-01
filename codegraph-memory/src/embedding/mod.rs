//! Embedding module for semantic search
//!
//! Uses fastembed with BGE-Small-EN-v1.5 (384d) for semantic embeddings.

mod engine;
mod fastembed_embed;

pub use engine::VectorEngine;
