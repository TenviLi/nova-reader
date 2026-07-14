use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use nova_core::domain::series::*;
use nova_core::repo::book_repo::Paginated;
use nova_core::repo::series_repo::{SeriesFilter, SeriesRepository};
use nova_core::{Error, Result};

pub struct PgSeriesRepository {
    pool: PgPool,
}

impl PgSeriesRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SeriesRepository for PgSeriesRepository {
    async fn list(&self, filter: &SeriesFilter) -> Result<Paginated<Series>> {
        let offset = (filter.page - 1) * filter.per_page;

        let mut conditions = vec!["1=1".to_string()];
        if let Some(ref q) = filter.search {
            conditions.push(format!(
                "(s.name ILIKE '%{}%' OR s.original_name ILIKE '%{}%')",
                q.replace('\'', "''"),
                q.replace('\'', "''")
            ));
        }
        if let Some(ref status) = filter.status {
            let status_str = format!("{:?}", status).to_lowercase();
            conditions.push(format!("s.status::text = '{}'", status_str));
        }
        if let Some(ref library_id) = filter.library_id {
            conditions.push(format!("s.library_id = '{}'", library_id));
        }
        let where_clause = conditions.join(" AND ");

        let count_query = format!("SELECT COUNT(*) FROM series s WHERE {}", where_clause);
        let total: i64 = sqlx::query_scalar(&count_query)
            .fetch_one(&self.pool)
            .await?;

        let query_str = format!(
            r#"
            SELECT s.id, s.library_id, s.name, s.sort_name, s.original_name,
                   s.alternate_names, s.description, s.folder_path,
                   s.status::text as status_text,
                   s.book_count, s.total_word_count, s.cover_path,
                   s.metadata,
                   s.created_at, s.updated_at
            FROM series s
            WHERE {}
            ORDER BY s.name ASC
            LIMIT {} OFFSET {}
            "#,
            where_clause, filter.per_page, offset
        );

        let rows = sqlx::query_as::<_, SeriesRow>(&query_str)
            .fetch_all(&self.pool)
            .await?;

        Ok(Paginated {
            data: rows.into_iter().map(Into::into).collect(),
            total,
            page: filter.page,
            per_page: filter.per_page,
        })
    }

    async fn get(&self, id: Uuid) -> Result<Series> {
        let row = sqlx::query_as::<_, SeriesRow>(
            r#"
            SELECT s.id, s.library_id, s.name, s.sort_name, s.original_name,
                   s.alternate_names, s.description, s.folder_path,
                   s.status::text as status_text,
                   s.book_count, s.total_word_count, s.cover_path,
                   s.metadata,
                   s.created_at, s.updated_at
            FROM series s
            WHERE s.id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| Error::NotFound {
            entity: "series",
            id: id.to_string(),
        })?;

        Ok(row.into())
    }

    async fn get_or_create_by_path(
        &self,
        library_id: Uuid,
        folder_path: &str,
        name: &str,
    ) -> Result<Series> {
        let id = Uuid::now_v7();

        let row = sqlx::query_as::<_, SeriesRow>(
            r#"
            WITH ins AS (
                INSERT INTO series (id, library_id, name, sort_name, folder_path, status, book_count, total_word_count)
                VALUES ($1, $2, $3, $3, $4, 'unknown'::series_status, 0, 0)
                ON CONFLICT (library_id, folder_path) DO UPDATE SET updated_at = NOW()
                RETURNING id, library_id, name, sort_name, original_name,
                          alternate_names, description, folder_path,
                          status::text as status_text,
                          book_count, total_word_count, cover_path,
                          metadata, created_at, updated_at
            )
            SELECT * FROM ins
            "#,
        )
        .bind(id)
        .bind(library_id)
        .bind(name)
        .bind(folder_path)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.into())
    }

    async fn update_metadata(&self, id: Uuid, metadata: &SeriesMetadata) -> Result<Series> {
        let json = serde_json::to_value(metadata)
            .map_err(|e| Error::Internal(format!("serialize metadata: {}", e)))?;

        sqlx::query("UPDATE series SET metadata = $1, updated_at = NOW() WHERE id = $2")
            .bind(&json)
            .bind(id)
            .execute(&self.pool)
            .await?;

        self.get(id).await
    }

    async fn update_status(&self, id: Uuid, status: SeriesStatus) -> Result<()> {
        let status_str = format!("{:?}", status).to_lowercase();
        sqlx::query(
            "UPDATE series SET status = $1::series_status, updated_at = NOW() WHERE id = $2",
        )
        .bind(&status_str)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn add_book(&self, series_id: Uuid, book_id: Uuid, sort_order: f64) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO series_books (series_id, book_id, sort_order)
            VALUES ($1, $2, $3)
            ON CONFLICT (series_id, book_id) DO UPDATE SET sort_order = $3
            "#,
        )
        .bind(series_id)
        .bind(book_id)
        .bind(sort_order)
        .execute(&self.pool)
        .await?;

        // Update book_count and total_word_count
        sqlx::query(
            r#"
            UPDATE series SET
                book_count = (SELECT COUNT(*)::int FROM series_books WHERE series_id = $1),
                total_word_count = (SELECT COALESCE(SUM(b.word_count), 0) FROM series_books sb JOIN books b ON b.id = sb.book_id WHERE sb.series_id = $1),
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(series_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn remove_book(&self, series_id: Uuid, book_id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM series_books WHERE series_id = $1 AND book_id = $2")
            .bind(series_id)
            .bind(book_id)
            .execute(&self.pool)
            .await?;

        // Update counts
        sqlx::query(
            r#"
            UPDATE series SET
                book_count = (SELECT COUNT(*)::int FROM series_books WHERE series_id = $1),
                total_word_count = (SELECT COALESCE(SUM(b.word_count), 0) FROM series_books sb JOIN books b ON b.id = sb.book_id WHERE sb.series_id = $1),
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(series_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn list_books(&self, series_id: Uuid) -> Result<Vec<SeriesBook>> {
        let rows: Vec<(Uuid, Uuid, f64, Option<String>)> = sqlx::query_as(
            r#"
            SELECT sb.series_id, sb.book_id, sb.sort_order, sb.volume_label
            FROM series_books sb
            WHERE sb.series_id = $1
            ORDER BY sb.sort_order ASC
            "#,
        )
        .bind(series_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(sid, bid, order, label)| SeriesBook {
                series_id: nova_core::Id::from_uuid(sid),
                book_id: nova_core::Id::from_uuid(bid),
                sort_order: order,
                volume_label: label,
            })
            .collect())
    }

    async fn delete(&self, id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM series WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

fn parse_status(s: &str) -> SeriesStatus {
    match s {
        "ongoing" => SeriesStatus::Ongoing,
        "completed" => SeriesStatus::Completed,
        "hiatus" => SeriesStatus::Hiatus,
        "cancelled" => SeriesStatus::Cancelled,
        _ => SeriesStatus::Unknown,
    }
}

// ─── Row mapping ────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct SeriesRow {
    id: Uuid,
    library_id: Uuid,
    name: String,
    sort_name: String,
    original_name: Option<String>,
    alternate_names: Option<serde_json::Value>,
    description: Option<String>,
    folder_path: String,
    status_text: Option<String>,
    book_count: i32,
    total_word_count: i64,
    cover_path: Option<String>,
    metadata: Option<serde_json::Value>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<SeriesRow> for Series {
    fn from(row: SeriesRow) -> Self {
        let metadata: SeriesMetadata = row
            .metadata
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();

        let alternate_names: Vec<String> = row
            .alternate_names
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();

        Series {
            id: nova_core::Id::from_uuid(row.id),
            library_id: nova_core::Id::from_uuid(row.library_id),
            name: row.name,
            sort_name: row.sort_name,
            original_name: row.original_name,
            alternate_names,
            description: row.description,
            folder_path: row.folder_path,
            status: parse_status(row.status_text.as_deref().unwrap_or("unknown")),
            book_count: row.book_count,
            total_word_count: row.total_word_count,
            cover_path: row.cover_path,
            metadata,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}
