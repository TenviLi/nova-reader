use async_trait::async_trait;
use chrono::NaiveDate;
use sqlx::PgPool;
use uuid::Uuid;

use nova_core::domain::reading::*;
use nova_core::domain::stats::*;
use nova_core::repo::reading_repo::*;
use nova_core::{Error, Result};

pub struct PgReadingRepository {
    pool: PgPool,
}

impl PgReadingRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ReadingRepository for PgReadingRepository {
    async fn get_progress(&self, book_id: Uuid) -> Result<Option<ReadingProgress>> {
        let row = sqlx::query_as::<_, ProgressRow>(
            r#"
            SELECT id, book_id, chapter_id, cfi, progress, current_chapter,
                   scroll_position, reading_time_secs, last_read_at, created_at
            FROM reading_progress WHERE book_id = $1
            "#,
        )
        .bind(book_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(Into::into))
    }

    async fn save_progress(&self, input: &SaveProgress) -> Result<ReadingProgress> {
        let id = Uuid::now_v7();
        let row = sqlx::query_as::<_, ProgressRow>(
            r#"
            INSERT INTO reading_progress (id, book_id, chapter_id, cfi, progress,
                                         current_chapter, scroll_position, reading_time_secs)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (book_id) DO UPDATE SET
                chapter_id = EXCLUDED.chapter_id,
                cfi = EXCLUDED.cfi,
                progress = EXCLUDED.progress,
                current_chapter = EXCLUDED.current_chapter,
                scroll_position = EXCLUDED.scroll_position,
                reading_time_secs = reading_progress.reading_time_secs + EXCLUDED.reading_time_secs,
                last_read_at = NOW()
            RETURNING id, book_id, chapter_id, cfi, progress, current_chapter,
                      scroll_position, reading_time_secs, last_read_at, created_at
            "#,
        )
        .bind(id)
        .bind(input.book_id)
        .bind(input.chapter_id)
        .bind(&input.cfi)
        .bind(input.progress)
        .bind(input.current_chapter)
        .bind(input.scroll_position)
        .bind(input.reading_time_secs)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.into())
    }

    async fn currently_reading(&self, limit: i64) -> Result<Vec<ReadingProgress>> {
        let rows = sqlx::query_as::<_, ProgressRow>(
            r#"
            SELECT id, book_id, chapter_id, cfi, progress, current_chapter,
                   scroll_position, reading_time_secs, last_read_at, created_at
            FROM reading_progress
            WHERE progress > 0.0 AND progress < 1.0
            ORDER BY last_read_at DESC
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn recently_finished(&self, limit: i64) -> Result<Vec<ReadingProgress>> {
        let rows = sqlx::query_as::<_, ProgressRow>(
            r#"
            SELECT id, book_id, chapter_id, cfi, progress, current_chapter,
                   scroll_position, reading_time_secs, last_read_at, created_at
            FROM reading_progress
            WHERE progress >= 1.0
            ORDER BY last_read_at DESC
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn annotations_for_book(&self, book_id: Uuid) -> Result<Vec<Annotation>> {
        let rows = sqlx::query_as::<_, AnnotationRow>(
            r#"
            SELECT id, book_id, chapter_id, cfi_range, selected_text, note,
                   color, start_offset, end_offset, created_at, updated_at
            FROM annotations
            WHERE book_id = $1
            ORDER BY start_offset ASC
            "#,
        )
        .bind(book_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn annotations_for_chapter(
        &self,
        book_id: Uuid,
        chapter_id: Uuid,
    ) -> Result<Vec<Annotation>> {
        let rows = sqlx::query_as::<_, AnnotationRow>(
            r#"
            SELECT id, book_id, chapter_id, cfi_range, selected_text, note,
                   color, start_offset, end_offset, created_at, updated_at
            FROM annotations
            WHERE book_id = $1 AND chapter_id = $2
            ORDER BY start_offset ASC
            "#,
        )
        .bind(book_id)
        .bind(chapter_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn create_annotation(&self, input: &CreateAnnotation) -> Result<Annotation> {
        let id = Uuid::now_v7();
        let row = sqlx::query_as::<_, AnnotationRow>(
            r#"
            INSERT INTO annotations (id, book_id, chapter_id, cfi_range, selected_text,
                                    note, color, start_offset, end_offset)
            VALUES ($1, $2, $3, $4, $5, $6, $7::highlight_color, $8, $9)
            RETURNING id, book_id, chapter_id, cfi_range, selected_text, note,
                      color, start_offset, end_offset, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(input.book_id)
        .bind(input.chapter_id)
        .bind(&input.cfi_range)
        .bind(&input.selected_text)
        .bind(&input.note)
        .bind(&input.color)
        .bind(input.start_offset)
        .bind(input.end_offset)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.into())
    }

    async fn update_annotation(
        &self,
        id: Uuid,
        note: Option<&str>,
        color: Option<HighlightColor>,
    ) -> Result<Annotation> {
        let row = sqlx::query_as::<_, AnnotationRow>(
            r#"
            UPDATE annotations
            SET note = COALESCE($2, note),
                color = COALESCE($3::highlight_color, color),
                updated_at = NOW()
            WHERE id = $1
            RETURNING id, book_id, chapter_id, cfi_range, selected_text, note,
                      color, start_offset, end_offset, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(note)
        .bind(color)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| Error::NotFound {
            entity: "annotation",
            id: id.to_string(),
        })?;

        Ok(row.into())
    }

    async fn delete_annotation(&self, id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM annotations WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn annotation_count(&self) -> Result<i64> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM annotations")
            .fetch_one(&self.pool)
            .await?;
        Ok(count)
    }

    async fn record_session(&self, input: &CreateSession) -> Result<ReadingSession> {
        let id = Uuid::now_v7();
        let pages = (input.words_read as f64 / 300.0).ceil() as i32;

        let row = sqlx::query_as::<_, SessionRow>(
            r#"
            INSERT INTO reading_sessions (id, book_id, start_chapter, end_chapter,
                                         words_read, duration_secs, pages_read, device)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING id, book_id, start_chapter, end_chapter, words_read,
                      duration_secs, pages_read, device, started_at, ended_at
            "#,
        )
        .bind(id)
        .bind(input.book_id)
        .bind(input.start_chapter)
        .bind(input.end_chapter)
        .bind(input.words_read)
        .bind(input.duration_secs)
        .bind(pages)
        .bind(&input.device)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.into())
    }

    async fn sessions_between(
        &self,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<ReadingSession>> {
        let rows = sqlx::query_as::<_, SessionRow>(
            r#"
            SELECT id, book_id, start_chapter, end_chapter, words_read,
                   duration_secs, pages_read, device, started_at, ended_at
            FROM reading_sessions
            WHERE started_at::date >= $1 AND started_at::date <= $2
            ORDER BY started_at DESC
            "#,
        )
        .bind(start)
        .bind(end)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn daily_stats(
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
            ORDER BY date ASC
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

    async fn active_goals(&self) -> Result<Vec<ReadingGoal>> {
        // Goals table may not exist yet in migration — return empty for now
        Ok(vec![])
    }

    async fn create_goal(
        &self,
        _goal_type: GoalType,
        _target: i64,
        _period: GoalPeriod,
    ) -> Result<ReadingGoal> {
        Err(Error::Internal("Goals not yet implemented".into()))
    }

    async fn update_goal_progress(&self, _id: Uuid, _progress: i64) -> Result<()> {
        Ok(())
    }
}

// ─── Row types ──────────────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct ProgressRow {
    id: Uuid,
    book_id: Uuid,
    chapter_id: Option<Uuid>,
    cfi: Option<String>,
    progress: f64,
    current_chapter: i32,
    scroll_position: Option<f64>,
    reading_time_secs: i64,
    last_read_at: chrono::DateTime<chrono::Utc>,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl From<ProgressRow> for ReadingProgress {
    fn from(row: ProgressRow) -> Self {
        ReadingProgress {
            id: nova_core::Id::from_uuid(row.id),
            book_id: nova_core::Id::from_uuid(row.book_id),
            chapter_id: row.chapter_id.map(nova_core::Id::from_uuid),
            cfi: row.cfi,
            progress: row.progress,
            current_chapter: row.current_chapter,
            scroll_position: row.scroll_position,
            reading_time_secs: row.reading_time_secs,
            last_read_at: row.last_read_at,
            created_at: row.created_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct AnnotationRow {
    id: Uuid,
    book_id: Uuid,
    chapter_id: Uuid,
    cfi_range: Option<String>,
    selected_text: String,
    note: Option<String>,
    color: HighlightColor,
    start_offset: i64,
    end_offset: i64,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<AnnotationRow> for Annotation {
    fn from(row: AnnotationRow) -> Self {
        Annotation {
            id: nova_core::Id::from_uuid(row.id),
            book_id: nova_core::Id::from_uuid(row.book_id),
            chapter_id: nova_core::Id::from_uuid(row.chapter_id),
            cfi_range: row.cfi_range,
            selected_text: row.selected_text,
            note: row.note,
            color: row.color,
            start_offset: row.start_offset,
            end_offset: row.end_offset,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct SessionRow {
    id: Uuid,
    book_id: Uuid,
    start_chapter: i32,
    end_chapter: i32,
    words_read: i64,
    duration_secs: i64,
    pages_read: i32,
    device: Option<String>,
    started_at: chrono::DateTime<chrono::Utc>,
    ended_at: chrono::DateTime<chrono::Utc>,
}

impl From<SessionRow> for ReadingSession {
    fn from(row: SessionRow) -> Self {
        ReadingSession {
            id: nova_core::Id::from_uuid(row.id),
            book_id: nova_core::Id::from_uuid(row.book_id),
            start_chapter: row.start_chapter,
            end_chapter: row.end_chapter,
            words_read: row.words_read,
            duration_secs: row.duration_secs,
            pages_read: row.pages_read,
            device: row.device,
            started_at: row.started_at,
            ended_at: row.ended_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct DailyRow {
    date: NaiveDate,
    total_minutes: i32,
    total_words: i64,
    sessions_count: i32,
}
