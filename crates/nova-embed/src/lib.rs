//! # Nova Embed
//!
//! Embedding generation service. Supports:
//! - Local MLX-based models (Qwen3-Embedding) for Apple Silicon
//! - OpenAI-compatible embedding APIs as fallback
//! - Batch processing with automatic chunking
//! - MinHash LSH for fast near-duplicate detection

pub mod client;
pub mod dedup;
pub mod chunker;
pub mod chunker_v2;

pub use client::EmbeddingService;
pub use dedup::DeduplicationEngine;
pub use chunker::TextChunker;
pub use chunker_v2::{NovelChunker, ChunkingConfigV2, ChunkV2, ContentLanguage};

#[cfg(test)]
mod tests;
