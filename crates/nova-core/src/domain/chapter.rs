use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::Id;
use super::book::BookId;

/// Marker type for Chapter IDs.
#[derive(Debug, Clone, Copy)]
pub struct ChapterMarker;
pub type ChapterId = Id<ChapterMarker>;

/// A single chapter within a book.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chapter {
    pub id: ChapterId,
    pub book_id: BookId,
    pub index: i32,
    pub title: Option<String>,
    pub content: String,
    pub word_count: i32,
    pub created_at: DateTime<Utc>,
}

/// A text chunk for embedding and RAG purposes.
/// Chapters are split into overlapping chunks of ~512 tokens.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextChunk {
    pub id: Id<TextChunkMarker>,
    pub chapter_id: ChapterId,
    pub book_id: BookId,
    pub chunk_index: i32,
    pub content: String,
    pub token_count: i32,
    /// Character offset within the chapter
    pub start_offset: i64,
    pub end_offset: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy)]
pub struct TextChunkMarker;

/// Parameters for chunking configuration.
#[derive(Debug, Clone)]
pub struct ChunkingConfig {
    /// Target chunk size in tokens
    pub chunk_size: usize,
    /// Overlap between consecutive chunks in tokens
    pub overlap: usize,
    /// Minimum chunk size (don't create tiny trailing chunks)
    pub min_chunk_size: usize,
}

impl Default for ChunkingConfig {
    fn default() -> Self {
        Self {
            chunk_size: 512,
            overlap: 64,
            min_chunk_size: 100,
        }
    }
}
