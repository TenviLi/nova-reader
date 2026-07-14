use async_trait::async_trait;
use chrono::NaiveDate;
use uuid::Uuid;

use crate::domain::reading::*;
use crate::domain::stats::*;
use crate::Result;

#[async_trait]
pub trait ReadingRepository: Send + Sync {
    // ─── Progress ───────────────────────────────────────────────────

    /// Get reading progress for a book.
    async fn get_progress(&self, book_id: Uuid) -> Result<Option<ReadingProgress>>;

    /// Save/update reading progress (upsert).
    async fn save_progress(&self, progress: &SaveProgress) -> Result<ReadingProgress>;

    /// Get all books currently being read (progress > 0, not finished).
    async fn currently_reading(&self, limit: i64) -> Result<Vec<ReadingProgress>>;

    /// Get recently finished books.
    async fn recently_finished(&self, limit: i64) -> Result<Vec<ReadingProgress>>;

    // ─── Annotations ────────────────────────────────────────────────

    /// Get all annotations for a book.
    async fn annotations_for_book(&self, book_id: Uuid) -> Result<Vec<Annotation>>;

    /// Get annotations for a specific chapter.
    async fn annotations_for_chapter(
        &self,
        book_id: Uuid,
        chapter_id: Uuid,
    ) -> Result<Vec<Annotation>>;

    /// Create an annotation.
    async fn create_annotation(&self, input: &CreateAnnotation) -> Result<Annotation>;

    /// Update an annotation's note.
    async fn update_annotation(
        &self,
        id: Uuid,
        note: Option<&str>,
        color: Option<HighlightColor>,
    ) -> Result<Annotation>;

    /// Delete an annotation.
    async fn delete_annotation(&self, id: Uuid) -> Result<()>;

    /// Count total annotations for a user.
    async fn annotation_count(&self) -> Result<i64>;

    // ─── Sessions ───────────────────────────────────────────────────

    /// Record a reading session.
    async fn record_session(&self, session: &CreateSession) -> Result<ReadingSession>;

    /// Get reading sessions for a date range.
    async fn sessions_between(
        &self,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<ReadingSession>>;

    /// Get daily stats for a date range (for heatmap).
    async fn daily_stats(&self, start: NaiveDate, end: NaiveDate)
        -> Result<Vec<DailyReadingStats>>;

    // ─── Goals ──────────────────────────────────────────────────────

    /// Get active reading goals.
    async fn active_goals(&self) -> Result<Vec<ReadingGoal>>;

    /// Create a reading goal.
    async fn create_goal(
        &self,
        goal_type: GoalType,
        target: i64,
        period: GoalPeriod,
    ) -> Result<ReadingGoal>;

    /// Update goal progress.
    async fn update_goal_progress(&self, id: Uuid, progress: i64) -> Result<()>;
}

/// Input for saving progress.
#[derive(Debug, Clone)]
pub struct SaveProgress {
    pub book_id: Uuid,
    pub chapter_id: Option<Uuid>,
    pub cfi: Option<String>,
    pub progress: f64,
    pub current_chapter: i32,
    pub scroll_position: Option<f64>,
    pub reading_time_secs: i64,
}

/// Input for creating an annotation.
#[derive(Debug, Clone)]
pub struct CreateAnnotation {
    pub book_id: Uuid,
    pub chapter_id: Uuid,
    pub cfi_range: Option<String>,
    pub selected_text: String,
    pub note: Option<String>,
    pub color: HighlightColor,
    pub start_offset: i64,
    pub end_offset: i64,
}

/// Input for creating a reading session.
#[derive(Debug, Clone)]
pub struct CreateSession {
    pub book_id: Uuid,
    pub start_chapter: i32,
    pub end_chapter: i32,
    pub words_read: i64,
    pub duration_secs: i64,
    pub device: Option<String>,
}
