use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::Id;
use super::book::BookId;
use super::chapter::ChapterId;

/// Reading progress tracked via CFI (Canonical Fragment Identifier).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadingProgress {
    pub id: Id<ReadingProgressMarker>,
    pub book_id: BookId,
    pub chapter_id: Option<ChapterId>,
    /// CFI string for atomic position tracking (e.g., "epubcfi(/6/4!/4/2:5)")
    pub cfi: Option<String>,
    /// Percentage progress (0.0 - 1.0)
    pub progress: f64,
    /// Current chapter index
    pub current_chapter: i32,
    /// Scroll position within current chapter (for non-EPUB)
    pub scroll_position: Option<f64>,
    /// Time spent reading in seconds
    pub reading_time_secs: i64,
    pub last_read_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy)]
pub struct ReadingProgressMarker;

/// A highlight or annotation made by the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Annotation {
    pub id: Id<AnnotationMarker>,
    pub book_id: BookId,
    pub chapter_id: ChapterId,
    /// CFI range for the highlighted text
    pub cfi_range: Option<String>,
    /// The highlighted text content
    pub selected_text: String,
    /// User's note about the highlight
    pub note: Option<String>,
    /// Color of the highlight
    pub color: HighlightColor,
    /// Character offsets within the chapter
    pub start_offset: i64,
    pub end_offset: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy)]
pub struct AnnotationMarker;

/// Highlight colors available to the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "highlight_color", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum HighlightColor {
    Yellow,
    Green,
    Blue,
    Pink,
    Purple,
    Orange,
}

/// Reading statistics aggregation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadingStats {
    pub total_books_read: i64,
    pub total_reading_time_secs: i64,
    pub total_words_read: i64,
    pub books_in_progress: i64,
    pub daily_average_minutes: f64,
    pub longest_streak_days: i32,
    pub current_streak_days: i32,
}
