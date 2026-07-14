use async_trait::async_trait;
use chrono::NaiveDate;
use sqlx::PgPool;

use nova_core::domain::reading::ReadingStats;
use nova_core::domain::stats::DailyReadingStats;
use nova_core::repo::stats_repo::*;
use nova_core::Result;

pub struct PgStatsRepository {
    pool: PgPool,
}

impl PgStatsRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl StatsRepository for PgStatsRepository {
    async fn reading_stats(&self) -> Result<ReadingStats> {
        let row = sqlx::query_as::<_, ReadingStatsRow>(
            r#"
            SELECT
                (SELECT COUNT(*) FROM reading_progress WHERE progress >= 1.0) as total_books_read,
                (SELECT COALESCE(SUM(reading_time_secs), 0) FROM reading_progress) as total_reading_time_secs,
                (SELECT COALESCE(SUM(b.word_count), 0)
                 FROM reading_progress rp JOIN books b ON rp.book_id = b.id
                 WHERE rp.progress >= 1.0) as total_words_read,
                (SELECT COUNT(*) FROM reading_progress WHERE progress > 0.0 AND progress < 1.0) as books_in_progress
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(ReadingStats {
            total_books_read: row.total_books_read,
            total_reading_time_secs: row.total_reading_time_secs,
            total_words_read: row.total_words_read,
            books_in_progress: row.books_in_progress,
            daily_average_minutes: 0.0, // computed client-side
            longest_streak_days: 0,
            current_streak_days: 0,
        })
    }

    async fn library_stats(&self) -> Result<LibraryStats> {
        let total_books: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM books WHERE status != 'archived'")
                .fetch_one(&self.pool)
                .await?;

        let total_series: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM series")
            .fetch_one(&self.pool)
            .await
            .unwrap_or(0);

        let total_chapters: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chapters")
            .fetch_one(&self.pool)
            .await?;

        let total_words: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(word_count), 0) FROM books WHERE status = 'ready'",
        )
        .fetch_one(&self.pool)
        .await?;

        let total_file_size: i64 =
            sqlx::query_scalar("SELECT COALESCE(SUM(file_size_bytes), 0) FROM books")
                .fetch_one(&self.pool)
                .await?;

        let total_entities: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entities")
            .fetch_one(&self.pool)
            .await
            .unwrap_or(0);

        let total_annotations: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM annotations")
            .fetch_one(&self.pool)
            .await
            .unwrap_or(0);

        let status_rows: Vec<(String, i64)> =
            sqlx::query_as("SELECT status::text, COUNT(*) FROM books GROUP BY status")
                .fetch_all(&self.pool)
                .await?;

        let books_by_status = status_rows
            .into_iter()
            .map(|(status, count)| StatusCount { status, count })
            .collect();

        Ok(LibraryStats {
            total_books,
            total_series,
            total_chapters,
            total_words,
            total_file_size_bytes: total_file_size,
            total_entities,
            total_annotations,
            books_by_status,
        })
    }

    async fn reading_heatmap(
        &self,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<DailyReadingStats>> {
        let rows = sqlx::query_as::<_, DailyRow>(
            r#"
            SELECT started_at::date as date,
                   (SUM(duration_secs) / 60)::int as total_minutes,
                   SUM(words_read) as total_words,
                   COUNT(*)::int as sessions_count
            FROM reading_sessions
            WHERE started_at::date >= $1 AND started_at::date <= $2
            GROUP BY started_at::date
            ORDER BY date
            "#,
        )
        .bind(start)
        .bind(end)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| DailyReadingStats {
                date: r.date,
                total_minutes: r.total_minutes,
                total_words: r.total_words,
                sessions_count: r.sessions_count,
                books_read: vec![],
            })
            .collect())
    }

    async fn format_distribution(&self) -> Result<Vec<FormatCount>> {
        let rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT format::text, COUNT(*) FROM books WHERE status != 'archived' GROUP BY format ORDER BY COUNT(*) DESC",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(format, count)| FormatCount { format, count })
            .collect())
    }

    async fn language_distribution(&self) -> Result<Vec<LanguageCount>> {
        let rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT language::text, COUNT(*) FROM books WHERE status != 'archived' GROUP BY language ORDER BY COUNT(*) DESC",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(language, count)| LanguageCount { language, count })
            .collect())
    }

    async fn storage_usage(&self) -> Result<StorageUsage> {
        let books_bytes: i64 =
            sqlx::query_scalar("SELECT COALESCE(SUM(file_size_bytes), 0) FROM books")
                .fetch_one(&self.pool)
                .await?;

        // Database size from pg_database_size
        let db_bytes: i64 = sqlx::query_scalar("SELECT pg_database_size(current_database())")
            .fetch_one(&self.pool)
            .await
            .unwrap_or(0);

        Ok(StorageUsage {
            books_bytes,
            database_bytes: db_bytes,
            vectors_bytes: 0, // Would need Qdrant API call
            total_bytes: books_bytes + db_bytes,
        })
    }
}

#[derive(sqlx::FromRow)]
struct ReadingStatsRow {
    total_books_read: i64,
    total_reading_time_secs: i64,
    total_words_read: i64,
    books_in_progress: i64,
}

#[derive(sqlx::FromRow)]
struct DailyRow {
    date: NaiveDate,
    total_minutes: i32,
    total_words: i64,
    sessions_count: i32,
}
