use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::Id;
use super::book::BookId;
use super::library::LibraryId;

/// Marker type for Series IDs.
#[derive(Debug, Clone, Copy)]
pub struct SeriesMarker;
pub type SeriesId = Id<SeriesMarker>;

/// A series groups multiple books/volumes in reading order.
/// Automatically detected from folder structure (Komga/Kavita pattern).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Series {
    pub id: SeriesId,
    pub library_id: LibraryId,
    pub name: String,
    /// Sortable name (e.g., "斗破苍穹" → "doupocangqiong")
    pub sort_name: String,
    /// Original language name
    pub original_name: Option<String>,
    /// Localized/alternate names
    #[serde(default)]
    pub alternate_names: Vec<String>,
    pub description: Option<String>,
    /// Path to the series folder relative to library root
    pub folder_path: String,
    pub status: SeriesStatus,
    pub book_count: i32,
    pub total_word_count: i64,
    /// Cover image path
    pub cover_path: Option<String>,
    pub metadata: SeriesMetadata,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Series publication/reading status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "series_status", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum SeriesStatus {
    Ongoing,
    Completed,
    Hiatus,
    Cancelled,
    Unknown,
}

/// Rich metadata for a series (Jellyfin/Kavita inspired).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SeriesMetadata {
    /// Genre tags (e.g., 玄幻, 仙侠, 都市, 科幻, 悬疑, 言情)
    #[serde(default)]
    pub genres: Vec<String>,
    /// User-defined tags
    #[serde(default)]
    pub tags: Vec<String>,
    /// Age rating (e.g., "全年龄", "15+", "18+")
    pub age_rating: Option<String>,
    /// Original publication platform (起点, 纵横, 番茄, etc.)
    pub publisher: Option<String>,
    /// Year of first publication
    pub year: Option<i32>,
    /// Community rating (0.0 - 10.0)
    pub rating: Option<f64>,
    /// User's personal rating
    pub user_rating: Option<f64>,
    /// Total chapter count (if known from source)
    pub total_chapters: Option<i32>,
    /// Release frequency
    pub release_schedule: Option<String>,
    /// Synopsis / blurb
    pub summary: Option<String>,
    /// Web novel URL (original source, for reference only)
    pub source_url: Option<String>,
    /// Custom metadata fields
    #[serde(default)]
    pub extra: serde_json::Value,
}

/// A book's position within a series.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeriesBook {
    pub series_id: SeriesId,
    pub book_id: BookId,
    /// Volume/position number within series (1-indexed)
    pub sort_order: f64,
    /// Display label (e.g., "第一卷", "V01", "上册")
    pub volume_label: Option<String>,
}
