use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::chapter::*;
use crate::Result;

#[async_trait]
pub trait ChapterRepository: Send + Sync {
    /// Get all chapters for a book (ordered by index).
    async fn list_by_book(&self, book_id: Uuid) -> Result<Vec<Chapter>>;

    /// Get a single chapter by ID.
    async fn get(&self, id: Uuid) -> Result<Chapter>;

    /// Get a chapter by book ID and index.
    async fn get_by_index(&self, book_id: Uuid, index: i32) -> Result<Chapter>;

    /// Bulk insert chapters for a book (replaces existing).
    async fn replace_all(&self, book_id: Uuid, chapters: &[CreateChapter]) -> Result<Vec<Chapter>>;

    /// Get total chapter count for a book.
    async fn count(&self, book_id: Uuid) -> Result<i64>;

    /// Get chapter word counts (for progress visualization).
    async fn word_counts(&self, book_id: Uuid) -> Result<Vec<(i32, i32)>>;
}

/// Input for creating a chapter.
#[derive(Debug, Clone)]
pub struct CreateChapter {
    pub index: i32,
    pub title: Option<String>,
    pub content: String,
    pub word_count: i32,
}
