use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::book::*;
use crate::Result;

/// Filtering/sorting parameters for book listing.
#[derive(Debug, Clone, Default)]
pub struct BookFilter {
    pub search: Option<String>,
    pub status: Option<BookStatus>,
    pub reading_status: Option<ReadingStatus>,
    pub language: Option<Language>,
    pub format: Option<BookFormat>,
    pub library_id: Option<Uuid>,
    pub library_ids: Option<Vec<Uuid>>,
    pub series_id: Option<Uuid>,
    pub sort_by: BookSort,
    pub page: i64,
    pub per_page: i64,
}

#[derive(Debug, Clone, Default)]
pub enum BookSort {
    #[default]
    UpdatedAtDesc,
    UpdatedAtAsc,
    TitleAsc,
    TitleDesc,
    CreatedAtDesc,
    CreatedAtAsc,
    WordCountDesc,
    WordCountAsc,
}

/// Paginated result container.
#[derive(Debug, Clone)]
pub struct Paginated<T> {
    pub data: Vec<T>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}

impl<T> Default for Paginated<T> {
    fn default() -> Self {
        Self {
            data: Vec::new(),
            total: 0,
            page: 1,
            per_page: 20,
        }
    }
}

impl<T> Paginated<T> {
    pub fn total_pages(&self) -> i64 {
        (self.total + self.per_page - 1) / self.per_page
    }
}

#[async_trait]
pub trait BookRepository: Send + Sync {
    /// List books with filtering and pagination.
    async fn list(&self, filter: &BookFilter) -> Result<Paginated<Book>>;

    /// Get a single book by ID.
    async fn get(&self, id: Uuid) -> Result<Book>;

    /// Create a new book entry.
    async fn create(&self, book: &CreateBook) -> Result<Book>;

    /// Update a book's metadata.
    async fn update_metadata(&self, id: Uuid, metadata: &BookMetadata) -> Result<Book>;

    /// Update book processing status.
    async fn update_status(&self, id: Uuid, status: BookStatus) -> Result<()>;

    /// Soft-delete a book.
    async fn delete(&self, id: Uuid) -> Result<()>;

    /// Check if a file hash already exists (dedup).
    async fn exists_by_hash(&self, hash: &str) -> Result<Option<Uuid>>;

    /// Get recently added books.
    async fn recent(&self, limit: i64) -> Result<Vec<Book>>;

    /// Count books by status.
    async fn count_by_status(&self, status: BookStatus) -> Result<i64>;

    /// List all books (lightweight fields only, for operations like dedup).
    /// Returns all non-archived books.
    async fn list_all(&self) -> Result<Vec<Book>>;
}
