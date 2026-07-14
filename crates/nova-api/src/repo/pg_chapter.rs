use async_trait::async_trait;
use futures::TryStreamExt;
use sqlx::PgPool;
use uuid::Uuid;

use nova_core::domain::chapter::*;
use nova_core::repo::chapter_repo::*;
use nova_core::{Error, Result};

pub struct PgChapterRepository {
    pool: PgPool,
}

/// Chapter content eligible for downstream search and RAG indexes.
///
/// This is intentionally an API-internal projection: callers should not need
/// to know how confirmed duplicate resolutions suppress redundant chapters.
#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct SearchableChapterRecord {
    pub chapter_index: i32,
    pub title: String,
    pub content: String,
}

/// Parsed chapter content awaiting its first atomic database publication.
///
/// The import route owns parsing, while this repository owns the transaction,
/// lock protocol, offsets, chapter rows, and final book status transition.
#[derive(Debug, Clone)]
pub(crate) struct ImportedChapter {
    pub(crate) title: String,
    pub(crate) content: String,
}

impl PgChapterRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// List chapters that should be present in search/vector indexes.
    ///
    /// Unique chapters from a secondary version remain searchable. Only
    /// source-verified, strictly redundant chapter mappings are omitted by the
    /// database policy function.
    pub(crate) async fn list_searchable_by_book(
        &self,
        book_id: Uuid,
    ) -> Result<Vec<SearchableChapterRecord>> {
        let chapters = sqlx::query_as::<_, SearchableChapterRecord>(
            r#"
            SELECT chapter_index, COALESCE(title, '') AS title, content
            FROM chapters
            WHERE book_id = $1 AND NOT dedup_chapter_is_redundant(id)
            ORDER BY chapter_index
            "#,
        )
        .bind(book_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(chapters)
    }

    /// Hash the ordered source snapshot without materializing the whole book.
    pub(crate) async fn book_source_content_hash(&self, book_id: Uuid) -> Result<String> {
        let mut rows = sqlx::query_as::<_, (i32, String)>(
            "SELECT chapter_index, content FROM chapters WHERE book_id = $1 ORDER BY chapter_index",
        )
        .bind(book_id)
        .fetch(&self.pool);
        let mut hasher = nova_ingest::dedup::SourceContentHasher::new();
        while let Some((chapter_index, content)) = rows.try_next().await? {
            hasher.update(chapter_index, &content);
        }
        Ok(hasher.finalize().to_hex())
    }

    /// Insert the initial chapter set and mark the book ready atomically.
    ///
    /// All dedup-sensitive advisory locks are acquired before any book or
    /// chapter row lock. A failed chapter insert therefore cannot leave a book
    /// marked ready with a partial chapter set.
    pub(crate) async fn insert_imported_and_mark_ready(
        &self,
        book_id: Uuid,
        chapters: &[ImportedChapter],
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        sqlx::query("SELECT lock_novel_dedup_global_barrier()")
            .execute(&mut *tx)
            .await?;
        let library_scope: String = sqlx::query_scalar(
            "SELECT COALESCE(library_id::text, '__all_libraries__') FROM books WHERE id = $1",
        )
        .bind(book_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| Error::NotFound {
            entity: "book",
            id: book_id.to_string(),
        })?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(library_scope)
            .execute(&mut *tx)
            .await?;
        sqlx::query("SELECT lock_novel_dedup_books($1)")
            .bind(vec![book_id])
            .execute(&mut *tx)
            .await?;

        let mut total_words = 0_i64;
        let mut offset = 0_i64;
        for (index, chapter) in chapters.iter().enumerate() {
            let chapter_index = i32::try_from(index)
                .map_err(|_| Error::Validation("chapter index exceeds i32".into()))?;
            let word_count = i32::try_from(
                chapter
                    .content
                    .chars()
                    .filter(|character| !character.is_whitespace())
                    .count(),
            )
            .unwrap_or(i32::MAX);
            total_words = total_words.saturating_add(i64::from(word_count));
            let content_bytes = i64::try_from(chapter.content.len()).unwrap_or(i64::MAX);
            let end_offset = offset.saturating_add(content_bytes);

            sqlx::query(
                r#"INSERT INTO chapters
                   (id, book_id, "index", title, content, word_count,
                    start_offset, end_offset, chapter_index)
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $3)"#,
            )
            .bind(Uuid::now_v7())
            .bind(book_id)
            .bind(chapter_index)
            .bind(&chapter.title)
            .bind(&chapter.content)
            .bind(word_count)
            .bind(offset)
            .bind(end_offset)
            .execute(&mut *tx)
            .await?;
            offset = end_offset;
        }

        let chapter_count = i32::try_from(chapters.len())
            .map_err(|_| Error::Validation("chapter count exceeds i32".into()))?;
        sqlx::query(
            r#"UPDATE books
               SET status = 'ready', chapter_count = $2, word_count = $3,
                   updated_at = NOW()
               WHERE id = $1"#,
        )
        .bind(book_id)
        .bind(chapter_count)
        .bind(total_words)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }
}

#[async_trait]
impl ChapterRepository for PgChapterRepository {
    async fn list_by_book(&self, book_id: Uuid) -> Result<Vec<Chapter>> {
        let rows = sqlx::query_as::<_, ChapterRow>(
            r#"
            SELECT id, book_id, index, title, content, word_count, created_at
            FROM chapters
            WHERE book_id = $1
            ORDER BY index ASC
            "#,
        )
        .bind(book_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn get(&self, id: Uuid) -> Result<Chapter> {
        let row = sqlx::query_as::<_, ChapterRow>(
            "SELECT id, book_id, index, title, content, word_count, created_at FROM chapters WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| Error::NotFound {
            entity: "chapter",
            id: id.to_string(),
        })?;

        Ok(row.into())
    }

    async fn get_by_index(&self, book_id: Uuid, index: i32) -> Result<Chapter> {
        let row = sqlx::query_as::<_, ChapterRow>(
            r#"
            SELECT id, book_id, index, title, content, word_count, created_at
            FROM chapters WHERE book_id = $1 AND index = $2
            "#,
        )
        .bind(book_id)
        .bind(index)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| Error::NotFound {
            entity: "chapter",
            id: format!("{book_id}:{index}"),
        })?;

        Ok(row.into())
    }

    async fn replace_all(&self, book_id: Uuid, chapters: &[CreateChapter]) -> Result<Vec<Chapter>> {
        let mut tx = self.pool.begin().await?;

        // These are the transaction's first locks. Acquiring the global,
        // enqueue-scope, and per-book barriers before the book row makes the
        // later chapter DELETE/INSERT triggers reentrant and deadlock-free.
        sqlx::query("SELECT lock_novel_dedup_global_barrier()")
            .execute(&mut *tx)
            .await?;
        let library_scope: String = sqlx::query_scalar(
            "SELECT COALESCE(library_id::text, '__all_libraries__') FROM books WHERE id = $1",
        )
        .bind(book_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| Error::NotFound {
            entity: "book",
            id: book_id.to_string(),
        })?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(library_scope)
            .execute(&mut *tx)
            .await?;
        sqlx::query("SELECT lock_novel_dedup_books($1)")
            .bind(vec![book_id])
            .execute(&mut *tx)
            .await?;

        // Serialize compare-and-replace for this book. Without this lock an
        // identical retry can read old rows while another transaction replaces
        // them, then return chapter IDs that were deleted before it committed.
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM books WHERE id = $1 FOR UPDATE")
            .bind(book_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| Error::NotFound {
                entity: "book",
                id: book_id.to_string(),
            })?;

        let existing = sqlx::query_as::<_, ChapterRow>(
            r#"SELECT id, book_id, index, title, content, word_count, created_at
               FROM chapters WHERE book_id = $1 ORDER BY index"#,
        )
        .bind(book_id)
        .fetch_all(&mut *tx)
        .await?;
        let chapter_count = i32::try_from(chapters.len()).unwrap_or(i32::MAX);
        let total_words: i64 = chapters
            .iter()
            .map(|chapter| i64::from(chapter.word_count))
            .sum();

        // Parser retries commonly submit the same chapter set. Preserve IDs and
        // downstream references, and do not manufacture a dedup rescan for an
        // input whose identity and content are unchanged.
        if chapter_replacement_is_identical(&existing, chapters) {
            sqlx::query(
                r#"UPDATE books
                   SET chapter_count = $2, word_count = $3, updated_at = NOW()
                   WHERE id = $1
                     AND (chapter_count IS DISTINCT FROM $2 OR word_count IS DISTINCT FROM $3)"#,
            )
            .bind(book_id)
            .bind(chapter_count)
            .bind(total_words)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            return Ok(existing.into_iter().map(Into::into).collect());
        }

        // Delete existing chapters
        sqlx::query("DELETE FROM chapters WHERE book_id = $1")
            .bind(book_id)
            .execute(&mut *tx)
            .await?;

        // Insert new chapters
        let mut results = Vec::with_capacity(chapters.len());
        for ch in chapters {
            let id = Uuid::now_v7();
            let row = sqlx::query_as::<_, ChapterRow>(
                r#"
                INSERT INTO chapters (id, book_id, index, chapter_index, title, content, word_count)
                VALUES ($1, $2, $3, $3, $4, $5, $6)
                RETURNING id, book_id, index, title, content, word_count, created_at
                "#,
            )
            .bind(id)
            .bind(book_id)
            .bind(ch.index)
            .bind(&ch.title)
            .bind(&ch.content)
            .bind(ch.word_count)
            .fetch_one(&mut *tx)
            .await?;

            results.push(row.into());
        }

        // Update book chapter count and total word count
        sqlx::query(
            "UPDATE books SET chapter_count = $2, word_count = $3, updated_at = NOW() WHERE id = $1",
        )
        .bind(book_id)
        .bind(chapter_count)
        .bind(total_words)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(results)
    }

    async fn count(&self, book_id: Uuid) -> Result<i64> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chapters WHERE book_id = $1")
            .bind(book_id)
            .fetch_one(&self.pool)
            .await?;
        Ok(count)
    }

    async fn word_counts(&self, book_id: Uuid) -> Result<Vec<(i32, i32)>> {
        let rows: Vec<(i32, i32)> = sqlx::query_as(
            "SELECT index, word_count FROM chapters WHERE book_id = $1 ORDER BY index",
        )
        .bind(book_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }
}

#[derive(sqlx::FromRow)]
struct ChapterRow {
    id: Uuid,
    book_id: Uuid,
    index: i32,
    title: Option<String>,
    content: String,
    word_count: i32,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl From<ChapterRow> for Chapter {
    fn from(row: ChapterRow) -> Self {
        Chapter {
            id: nova_core::Id::from_uuid(row.id),
            book_id: nova_core::Id::from_uuid(row.book_id),
            index: row.index,
            title: row.title,
            content: row.content,
            word_count: row.word_count,
            created_at: row.created_at,
        }
    }
}

fn chapter_replacement_is_identical(
    existing: &[ChapterRow],
    replacement: &[CreateChapter],
) -> bool {
    existing.len() == replacement.len()
        && existing.iter().zip(replacement).all(|(stored, incoming)| {
            stored.index == incoming.index
                && stored.title == incoming.title
                && stored.content == incoming.content
                && stored.word_count == incoming.word_count
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stored(content: &str) -> ChapterRow {
        ChapterRow {
            id: Uuid::now_v7(),
            book_id: Uuid::now_v7(),
            index: 0,
            title: Some("Chapter".to_string()),
            content: content.to_string(),
            word_count: 1,
            created_at: chrono::Utc::now(),
        }
    }

    fn incoming(content: &str) -> CreateChapter {
        CreateChapter {
            index: 0,
            title: Some("Chapter".to_string()),
            content: content.to_string(),
            word_count: 1,
        }
    }

    #[test]
    fn identical_parser_retry_is_a_noop_but_content_change_is_not() {
        assert!(chapter_replacement_is_identical(
            &[stored("same")],
            &[incoming("same")]
        ));
        assert!(!chapter_replacement_is_identical(
            &[stored("before")],
            &[incoming("after")]
        ));
    }
}
