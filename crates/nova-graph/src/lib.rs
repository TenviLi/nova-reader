//! # Nova Graph
//!
//! Agentic GraphRAG engine built on Neo4j.
//! Supports entity extraction, relationship mapping, community detection
//! (Leiden algorithm), and multi-hop reasoning over knowledge graphs.

pub mod agentic;
pub mod entity;
pub mod neo4j;
pub mod community;
pub mod leiden;
pub mod sync;

pub use agentic::AgenticGraphRag;
pub use neo4j::Neo4jClient;
pub use sync::sync_book_to_neo4j;
pub use leiden::{
    Graph as LeidenGraph, LeidenConfig, CommunityResult,
    detect_communities, detect_hierarchical, extract_book_graph,
    store_communities, extract_community_data,
};

#[cfg(test)]
mod tests;
