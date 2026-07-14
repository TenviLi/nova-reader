use async_trait::async_trait;
use chrono::NaiveDate;

use crate::domain::reading::ReadingStats;
use crate::domain::stats::DailyReadingStats;
use crate::Result;

/// System-wide statistics and dashboard data.
#[async_trait]
pub trait StatsRepository: Send + Sync {
    /// Get overall reading statistics.
    async fn reading_stats(&self) -> Result<ReadingStats>;

    /// Get library statistics (total books, formats breakdown, languages breakdown).
    async fn library_stats(&self) -> Result<LibraryStats>;

    /// Get daily reading data for heatmap visualization.
    async fn reading_heatmap(&self, start: NaiveDate, end: NaiveDate) -> Result<Vec<DailyReadingStats>>;

    /// Get format distribution.
    async fn format_distribution(&self) -> Result<Vec<FormatCount>>;

    /// Get language distribution.
    async fn language_distribution(&self) -> Result<Vec<LanguageCount>>;

    /// Get storage usage breakdown.
    async fn storage_usage(&self) -> Result<StorageUsage>;
}

/// Library-level aggregate statistics.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LibraryStats {
    pub total_books: i64,
    pub total_series: i64,
    pub total_chapters: i64,
    pub total_words: i64,
    pub total_file_size_bytes: i64,
    pub total_entities: i64,
    pub total_annotations: i64,
    pub books_by_status: Vec<StatusCount>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StatusCount {
    pub status: String,
    pub count: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FormatCount {
    pub format: String,
    pub count: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LanguageCount {
    pub language: String,
    pub count: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StorageUsage {
    pub books_bytes: i64,
    pub database_bytes: i64,
    pub vectors_bytes: i64,
    pub total_bytes: i64,
}
