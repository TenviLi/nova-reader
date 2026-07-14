use async_trait::async_trait;
use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use nova_core::domain::book::*;
use nova_core::repo::book_repo::*;
use nova_core::{Error, Result};

pub struct PgBookRepository {
    pool: PgPool,
}

/// Stored file identity used while reconciling a library filesystem scan.
#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct BookFileRecord {
    pub file_hash: String,
    pub book_id: Uuid,
    pub file_path: String,
}

impl PgBookRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Find the deterministic first non-archived book with this file hash.
    ///
    /// `library_ids = None` is the unrestricted (admin) scope. An explicit
    /// empty scope cannot match anything and deliberately avoids a query.
    pub(crate) async fn find_non_archived_by_hash_in_libraries(
        &self,
        file_hash: &str,
        library_ids: Option<&[Uuid]>,
    ) -> Result<Option<Uuid>> {
        if matches!(library_ids, Some(ids) if ids.is_empty()) {
            return Ok(None);
        }

        let book_id = if let Some(library_ids) = library_ids {
            sqlx::query_scalar(
                r#"
                SELECT id
                FROM books
                WHERE file_hash = $1
                  AND status != 'archived'
                  AND library_id = ANY($2::uuid[])
                ORDER BY created_at, id
                LIMIT 1
                "#,
            )
            .bind(file_hash)
            .bind(library_ids)
            .fetch_optional(&self.pool)
            .await?
        } else {
            sqlx::query_scalar(
                r#"
                SELECT id
                FROM books
                WHERE file_hash = $1
                  AND status != 'archived'
                ORDER BY created_at, id
                LIMIT 1
                "#,
            )
            .bind(file_hash)
            .fetch_optional(&self.pool)
            .await?
        };

        Ok(book_id)
    }

    /// List all stored file identities in deterministic duplicate preference
    /// order (active records before archived records).
    pub(crate) async fn list_file_records_by_library(
        &self,
        library_id: Uuid,
    ) -> Result<Vec<BookFileRecord>> {
        let records = sqlx::query_as::<_, BookFileRecord>(
            r#"
            SELECT file_hash, id AS book_id, file_path
            FROM books
            WHERE library_id = $1
            ORDER BY (status = 'archived'), created_at, id
            "#,
        )
        .bind(library_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(records)
    }
}

#[async_trait]
impl BookRepository for PgBookRepository {
    async fn list(&self, filter: &BookFilter) -> Result<Paginated<Book>> {
        let offset = (filter.page - 1) * filter.per_page;

        // Build dynamic query based on filters
        let (order_col, order_dir) = match &filter.sort_by {
            BookSort::TitleAsc => ("title", "ASC"),
            BookSort::TitleDesc => ("title", "DESC"),
            BookSort::CreatedAtDesc => ("created_at", "DESC"),
            BookSort::CreatedAtAsc => ("created_at", "ASC"),
            BookSort::WordCountDesc => ("word_count", "DESC"),
            BookSort::WordCountAsc => ("word_count", "ASC"),
            _ => ("updated_at", "DESC"),
        };

        let mut count_query = QueryBuilder::<Postgres>::new("SELECT COUNT(*) FROM books");
        push_book_filters(&mut count_query, filter);
        let total: i64 = count_query
            .build_query_scalar()
            .fetch_one(&self.pool)
            .await?;

        let mut books_query = QueryBuilder::<Postgres>::new(
            r#"
            SELECT id, title, author, description, language, format, status, reading_status,
                   metadata, file_path, file_hash, file_size_bytes,
                   chapter_count, word_count, library_id,
                   created_at, updated_at, indexed_at
            FROM books
            "#,
        );
        push_book_filters(&mut books_query, filter);
        books_query
            .push(" ORDER BY ")
            .push(order_col)
            .push(" ")
            .push(order_dir)
            .push(", created_at DESC LIMIT ")
            .push_bind(filter.per_page)
            .push(" OFFSET ")
            .push_bind(offset);

        let books = books_query
            .build_query_as::<BookRow>()
            .fetch_all(&self.pool)
            .await?;

        Ok(Paginated {
            data: books.into_iter().map(Into::into).collect(),
            total,
            page: filter.page,
            per_page: filter.per_page,
        })
    }

    async fn get(&self, id: Uuid) -> Result<Book> {
        let row = sqlx::query_as::<_, BookRow>(
            r#"
            SELECT id, title, author, description, language, format, status, reading_status,
                   metadata, file_path, file_hash, file_size_bytes,
                   chapter_count, word_count, library_id,
                   created_at, updated_at, indexed_at
            FROM books WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| Error::NotFound {
            entity: "book",
            id: id.to_string(),
        })?;

        Ok(row.into())
    }

    async fn create(&self, input: &CreateBook) -> Result<Book> {
        let id = Uuid::now_v7();
        let metadata_json = serde_json::to_value(&BookMetadata::default()).unwrap_or_default();

        let row = sqlx::query_as::<_, BookRow>(
            r#"
            INSERT INTO books (id, title, author, language, format, status,
                              metadata, file_path, file_hash, file_size_bytes,
                              chapter_count, word_count, library_id)
            VALUES ($1, $2, $3, $4::language, $5::book_format, 'pending'::book_status,
                    $6, $7, $8, $9, 0, 0,
                    COALESCE($10, (SELECT id FROM libraries WHERE is_default = true LIMIT 1)))
            RETURNING id, title, author, description, language, format, status, reading_status,
                      metadata, file_path, file_hash, file_size_bytes,
                      chapter_count, word_count, library_id,
                      created_at, updated_at, indexed_at
            "#,
        )
        .bind(id)
        .bind(&input.title)
        .bind(&input.author)
        .bind(&input.language)
        .bind(&input.format)
        .bind(&metadata_json)
        .bind(&input.file_path)
        .bind(&input.file_hash)
        .bind(input.file_size_bytes)
        .bind(input.library_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.into())
    }

    async fn update_metadata(&self, id: Uuid, metadata: &BookMetadata) -> Result<Book> {
        let metadata_json = serde_json::to_value(metadata)
            .map_err(|e| Error::Internal(format!("serialize metadata: {e}")))?;

        let row = sqlx::query_as::<_, BookRow>(
            r#"
            UPDATE books SET metadata = $2, updated_at = NOW()
            WHERE id = $1
            RETURNING id, title, author, description, language, format, status, reading_status,
                      metadata, file_path, file_hash, file_size_bytes,
                      chapter_count, word_count, library_id,
                      created_at, updated_at, indexed_at
            "#,
        )
        .bind(id)
        .bind(&metadata_json)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| Error::NotFound {
            entity: "book",
            id: id.to_string(),
        })?;

        Ok(row.into())
    }

    async fn update_status(&self, id: Uuid, status: BookStatus) -> Result<()> {
        let result = sqlx::query(
            "UPDATE books SET status = $2::book_status, updated_at = NOW() WHERE id = $1",
        )
        .bind(id)
        .bind(&status)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(Error::NotFound {
                entity: "book",
                id: id.to_string(),
            });
        }
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<()> {
        sqlx::query(
            "UPDATE books SET status = 'archived'::book_status, updated_at = NOW() WHERE id = $1",
        )
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn exists_by_hash(&self, hash: &str) -> Result<Option<Uuid>> {
        let result: Option<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM books WHERE file_hash = $1 AND status != 'archived' LIMIT 1",
        )
        .bind(hash)
        .fetch_optional(&self.pool)
        .await?;

        Ok(result.map(|(id,)| id))
    }

    async fn recent(&self, limit: i64) -> Result<Vec<Book>> {
        let rows = sqlx::query_as::<_, BookRow>(
            r#"
            SELECT id, title, author, description, language, format, status,
                   metadata, file_path, file_hash, file_size_bytes,
                   chapter_count, word_count, library_id,
                   created_at, updated_at, indexed_at
            FROM books
            WHERE status NOT IN ('archived', 'duplicate')
            ORDER BY created_at DESC
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn count_by_status(&self, status: BookStatus) -> Result<i64> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM books WHERE status = $1::book_status")
                .bind(&status)
                .fetch_one(&self.pool)
                .await?;

        Ok(count)
    }

    async fn list_all(&self) -> Result<Vec<Book>> {
        let rows = sqlx::query_as::<_, BookRow>(
            "SELECT * FROM books WHERE status NOT IN ('archived', 'duplicate') ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }
}

fn push_book_filters<'a>(query: &mut QueryBuilder<'a, Postgres>, filter: &'a BookFilter) {
    query.push(" WHERE status != 'archived'");

    if let Some(search) = filter
        .search
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let pattern = format!("%{search}%");
        query
            .push(" AND (title ILIKE ")
            .push_bind(pattern.clone())
            .push(" OR author ILIKE ")
            .push_bind(pattern)
            .push(")");
    }

    if let Some(status) = filter.status {
        query.push(" AND status = ").push_bind(status);
    } else {
        query.push(" AND status != 'duplicate'");
    }
    if let Some(reading_status) = filter.reading_status {
        query
            .push(" AND reading_status = ")
            .push_bind(reading_status);
    }
    if let Some(language) = filter.language {
        query.push(" AND language = ").push_bind(language);
    }
    if let Some(format) = filter.format {
        query.push(" AND format = ").push_bind(format);
    }
    if let Some(library_id) = filter.library_id {
        query.push(" AND library_id = ").push_bind(library_id);
    } else if let Some(library_ids) = &filter.library_ids {
        query
            .push(" AND library_id = ANY(")
            .push_bind(library_ids)
            .push(")");
    }
    if let Some(series_id) = filter.series_id {
        query.push(" AND series_id = ").push_bind(series_id);
    }
}

/// Internal row mapping from SQLx.
#[derive(sqlx::FromRow)]
struct BookRow {
    id: Uuid,
    title: String,
    author: Option<String>,
    description: Option<String>,
    language: Language,
    format: BookFormat,
    status: BookStatus,
    reading_status: ReadingStatus,
    metadata: serde_json::Value,
    file_path: String,
    file_hash: String,
    file_size_bytes: i64,
    chapter_count: i32,
    word_count: i64,
    library_id: Option<Uuid>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    indexed_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<BookRow> for Book {
    fn from(row: BookRow) -> Self {
        let metadata: BookMetadata = serde_json::from_value(row.metadata).unwrap_or_default();
        Book {
            id: nova_core::Id::from_uuid(row.id),
            title: row.title,
            author: row.author,
            description: row.description,
            language: row.language,
            format: row.format,
            status: row.status,
            reading_status: row.reading_status,
            metadata,
            file_path: row.file_path,
            file_hash: row.file_hash,
            file_size_bytes: row.file_size_bytes,
            library_id: row.library_id,
            chapter_count: row.chapter_count,
            word_count: row.word_count,
            created_at: row.created_at,
            updated_at: row.updated_at,
            indexed_at: row.indexed_at,
        }
    }
}
