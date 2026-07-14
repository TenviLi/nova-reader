use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::library::*;
use crate::Result;
use super::book_repo::Paginated;

#[derive(Debug, Clone, Default)]
pub struct LibraryFilter {
    pub search: Option<String>,
    pub page: i64,
    pub per_page: i64,
}

#[async_trait]
pub trait LibraryRepository: Send + Sync {
    /// List all libraries.
    async fn list(&self, filter: &LibraryFilter) -> Result<Paginated<Library>>;

    /// Get a single library by ID.
    async fn get(&self, id: Uuid) -> Result<Library>;

    /// Create a new library (validates path exists).
    async fn create(&self, input: &CreateLibrary) -> Result<Library>;

    /// Update library configuration.
    async fn update(&self, id: Uuid, input: &UpdateLibrary) -> Result<Library>;

    /// Delete a library (does NOT delete files on disk).
    async fn delete(&self, id: Uuid) -> Result<()>;

    /// Update scan timestamp and stats after a scan completes.
    async fn update_scan_result(&self, id: Uuid, book_count: i64, total_size: i64) -> Result<()>;

    /// Get all shelves.
    async fn list_shelves(&self) -> Result<Vec<Shelf>>;

    /// Get all collections.
    async fn list_collections(&self) -> Result<Vec<Collection>>;

    /// Create a collection.
    async fn create_collection(&self, name: &str, description: Option<&str>) -> Result<Collection>;

    /// Add books to a collection.
    async fn add_to_collection(&self, collection_id: Uuid, book_ids: &[Uuid]) -> Result<()>;

    /// Remove books from a collection.
    async fn remove_from_collection(&self, collection_id: Uuid, book_ids: &[Uuid]) -> Result<()>;
}

/// Input for creating a library.
#[derive(Debug, Clone)]
pub struct CreateLibrary {
    pub name: String,
    pub root_path: String,
    pub description: Option<String>,
    pub auto_scan: bool,
    pub scan_interval_secs: i64,
    pub include_extensions: Vec<String>,
    pub exclude_patterns: Vec<String>,
}

/// Input for updating a library.
#[derive(Debug, Clone)]
pub struct UpdateLibrary {
    pub name: Option<String>,
    pub description: Option<String>,
    pub auto_scan: Option<bool>,
    pub scan_interval_secs: Option<i64>,
    pub include_extensions: Option<Vec<String>>,
    pub exclude_patterns: Option<Vec<String>>,
}
