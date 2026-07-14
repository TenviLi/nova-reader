//! # Nova Search
//!
//! Hybrid search engine implementing:
//! - Keyword search via Meilisearch
//! - Dense vector search via Qdrant
//! - Sparse BM25 search via Qdrant
//! - Reciprocal Rank Fusion (RRF) for result merging
//! - Graph-based multi-hop reasoning via Neo4j

pub mod embedding;
pub mod hybrid;
pub mod meilisearch;
pub mod qdrant;
pub mod rrf;

pub use hybrid::HybridSearchEngine;
