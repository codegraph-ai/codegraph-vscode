//! Embedding module for semantic search
//!
//! Uses Model2Vec for ultra-fast static embeddings (~8000 samples/sec).

mod discovery;
mod engine;
mod model2vec;

pub use discovery::find_model2vec_path;
pub use engine::VectorEngine;
pub use model2vec::{Model2VecConfig, Model2VecEmbedding};
