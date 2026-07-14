use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::series::*;
use crate::Result;
use super::book_repo::Paginated;

#[derive(Debug, Clone, Default)]
pub struct SeriesFilter {
    pub library_id: Option<Uuid>,
    pub status: Option<SeriesStatus>,
    pub search: Option<String>,
    pub page: i64,
    pub per_page: i64,
}

#[async_trait]
pub trait SeriesRepository: Send + Sync {
    /// List series with filtering.
    async fn list(&self, filter: &SeriesFilter) -> Result<Paginated<Series>>;

    /// Get a single series by ID.
    async fn get(&self, id: Uuid) -> Result<Series>;

    /// Get or create a series by folder path within a library.
    async fn get_or_create_by_path(
        &self,
        library_id: Uuid,
        folder_path: &str,
        name: &str,
    ) -> Result<Series>;

    /// Update series metadata.
    async fn update_metadata(&self, id: Uuid, metadata: &SeriesMetadata) -> Result<Series>;

    /// Update series status.
    async fn update_status(&self, id: Uuid, status: SeriesStatus) -> Result<()>;

    /// Add a book to a series at a given position.
    async fn add_book(&self, series_id: Uuid, book_id: Uuid, sort_order: f64) -> Result<()>;

    /// Remove a book from a series.
    async fn remove_book(&self, series_id: Uuid, book_id: Uuid) -> Result<()>;

    /// Get all books in a series (ordered).
    async fn list_books(&self, series_id: Uuid) -> Result<Vec<SeriesBook>>;

    /// Delete a series (books are NOT deleted).
    async fn delete(&self, id: Uuid) -> Result<()>;
}
