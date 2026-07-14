use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::Id;
use super::book::BookId;

/// Marker for reading session IDs.
#[derive(Debug, Clone, Copy)]
pub struct ReadingSessionMarker;
pub type ReadingSessionId = Id<ReadingSessionMarker>;

/// A discrete reading session (like Immich's activity log).
/// Tracks start/end time, pages/words read — for analytics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadingSession {
    pub id: ReadingSessionId,
    pub book_id: BookId,
    /// Which chapter the user was on when they started
    pub start_chapter: i32,
    /// Which chapter the user was on when they stopped
    pub end_chapter: i32,
    /// Words read during this session
    pub words_read: i64,
    /// Duration in seconds
    pub duration_secs: i64,
    /// Pages read (estimated from word count / ~300 words per page)
    pub pages_read: i32,
    /// Device/client identifier (for multi-device tracking)
    pub device: Option<String>,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
}

/// Daily reading aggregate (for heatmap/streak visualization).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyReadingStats {
    pub date: NaiveDate,
    pub total_minutes: i32,
    pub total_words: i64,
    pub sessions_count: i32,
    pub books_read: Vec<BookId>,
}

/// Reading goals set by the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadingGoal {
    pub id: Id<ReadingGoalMarker>,
    pub goal_type: GoalType,
    /// Target value (pages, minutes, books, etc.)
    pub target: i64,
    /// Current progress toward the goal
    pub progress: i64,
    /// Period (daily, weekly, monthly, yearly)
    pub period: GoalPeriod,
    /// Is this goal currently active?
    pub active: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy)]
pub struct ReadingGoalMarker;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "goal_type", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum GoalType {
    /// Number of books to finish
    BooksFinished,
    /// Minutes of reading time
    ReadingMinutes,
    /// Words read
    WordsRead,
    /// Pages read
    PagesRead,
    /// Maintain a daily reading streak
    DailyStreak,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "goal_period", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum GoalPeriod {
    Daily,
    Weekly,
    Monthly,
    Yearly,
    AllTime,
}
