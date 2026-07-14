use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::Id;

/// Marker type for Book IDs.
#[derive(Debug, Clone, Copy)]
pub struct BookMarker;
pub type BookId = Id<BookMarker>;

/// The primary representation of a book/novel in the system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Book {
    pub id: BookId,
    pub title: String,
    pub author: Option<String>,
    pub description: Option<String>,
    pub language: Language,
    pub format: BookFormat,
    pub status: BookStatus,
    pub reading_status: ReadingStatus,
    pub metadata: BookMetadata,
    pub file_path: String,
    pub file_hash: String,
    pub file_size_bytes: i64,
    pub library_id: Option<uuid::Uuid>,
    pub chapter_count: i32,
    pub word_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub indexed_at: Option<DateTime<Utc>>,
}

/// Supported source file formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "book_format", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum BookFormat {
    Txt,
    Epub,
    Pdf,
    Doc,
    Docx,
    #[serde(rename = "md")]
    Markdown,
    Html,
}

impl BookFormat {
    /// Infer format from file extension.
    #[must_use]
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "txt" => Some(Self::Txt),
            "epub" => Some(Self::Epub),
            "pdf" => Some(Self::Pdf),
            "doc" => Some(Self::Doc),
            "docx" => Some(Self::Docx),
            "md" | "markdown" => Some(Self::Markdown),
            "html" | "htm" => Some(Self::Html),
            _ => None,
        }
    }
}

/// Book processing lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "book_status", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum BookStatus {
    /// File detected, awaiting processing
    Pending,
    /// Currently being parsed and chunked
    Processing,
    /// Parsed and indexed, ready to read
    Ready,
    /// Marked as duplicate (soft-deleted)
    Duplicate,
    /// Processing failed
    Failed,
    /// Archived by user
    Archived,
}

/// User-facing reading status (orthogonal to processing status).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "reading_status", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ReadingStatus {
    /// Not started yet
    Unread,
    /// Currently reading
    Reading,
    /// Finished the book
    Completed,
    /// Paused
    OnHold,
    /// Dropped / abandoned
    Dropped,
}

/// Primary language of the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "language", rename_all = "lowercase")]
pub enum Language {
    #[serde(rename = "zh")]
    Chinese,
    #[serde(rename = "en")]
    English,
    #[serde(rename = "ja")]
    Japanese,
    #[serde(rename = "ko")]
    Korean,
    #[serde(rename = "unknown")]
    Unknown,
}

/// Extended metadata extracted from the book.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BookMetadata {
    /// Genre tags (e.g., "玄幻", "都市", "科幻")
    #[serde(default)]
    pub genres: Vec<String>,

    /// User-defined tags
    #[serde(default)]
    pub tags: Vec<String>,

    /// Series name if part of a series
    pub series: Option<String>,

    /// Volume number within series
    pub volume: Option<i32>,

    /// Publisher information
    pub publisher: Option<String>,

    /// Publication year
    pub year: Option<i32>,

    /// Cover image path (relative to library)
    pub cover_path: Option<String>,

    /// ISBN if available
    pub isbn: Option<String>,

    /// Custom key-value metadata
    #[serde(default)]
    pub extra: serde_json::Value,
}

/// Criteria for creating a new book entry.
#[derive(Debug, Clone)]
pub struct CreateBook {
    pub title: String,
    pub author: Option<String>,
    pub language: Language,
    pub format: BookFormat,
    pub file_path: String,
    pub file_hash: String,
    pub file_size_bytes: i64,
    pub library_id: Option<uuid::Uuid>,
}
