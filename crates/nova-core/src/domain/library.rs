use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::Id;

/// Marker type for Library IDs.
#[derive(Debug, Clone, Copy)]
pub struct LibraryMarker;
pub type LibraryId = Id<LibraryMarker>;

/// A library represents a monitored root directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Library {
    pub id: LibraryId,
    pub name: String,
    pub root_path: String,
    pub scan_interval_secs: i64,
    pub auto_scan: bool,
    pub book_count: i64,
    pub total_size_bytes: i64,
    pub last_scanned_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Collection — a user-curated grouping of books.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collection {
    pub id: Id<CollectionMarker>,
    pub name: String,
    pub description: Option<String>,
    pub cover_path: Option<String>,
    pub book_count: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy)]
pub struct CollectionMarker;

/// A shelf for organizing books (like Kavita's reading lists).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shelf {
    pub id: Id<ShelfMarker>,
    pub name: String,
    pub description: Option<String>,
    pub is_ordered: bool,
    pub book_count: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy)]
pub struct ShelfMarker;
