use std::str::FromStr;

use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use nova_core::{
    domain::dedup::{
        DuplicateCandidateKind, DuplicatePairEvidence, DuplicateRelation, DuplicateReviewStatus,
    },
    Error, Result,
};

const PAIR_SELECT: &str = r#"
    SELECT COUNT(*) OVER() AS total_count,
           p.id, p.relation, p.review_status AS status, p.confidence,
           p.shared_chapters, p.coverage_a, p.coverage_b,
           p.character_coverage_a, p.character_coverage_b,
           p.longest_contiguous_run, p.order_score, p.contained_book_id,
           p.recommended_primary_id, p.semantic_score, p.evidence,
           p.created_at, p.updated_at,
           a.id AS book_a_id, a.title AS book_a_title, a.author AS book_a_author,
           a.format::text AS book_a_format, a.file_size_bytes AS book_a_file_size,
           a.word_count AS book_a_word_count, a.chapter_count AS book_a_chapter_count,
           a.cover_path AS book_a_cover_path,
           b.id AS book_b_id, b.title AS book_b_title, b.author AS book_b_author,
           b.format::text AS book_b_format, b.file_size_bytes AS book_b_file_size,
           b.word_count AS book_b_word_count, b.chapter_count AS book_b_chapter_count,
           b.cover_path AS book_b_cover_path
    FROM duplicate_pairs p
    JOIN books a ON a.id = p.book_a_id
    JOIN books b ON b.id = p.book_b_id
"#;

/// PostgreSQL persistence boundary for the duplicate-review API.
pub struct PgDuplicateRepository {
    pub(crate) pool: PgPool,
}

impl PgDuplicateRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn scan(&self, id: Uuid) -> Result<Option<DuplicateScanRecord>> {
        let scan = sqlx::query_as!(
            DuplicateScanRecord,
            r#"SELECT id, library_id, task_id, include_semantic, algorithm_version,
                      status, progress, progress_message, books_total, books_processed,
                      chapters_processed, candidates_found, pairs_found, exact_pairs,
                      contained_pairs, semantic_pairs, error_message, started_at,
                      completed_at, created_at
               FROM dedup_scan_runs
               WHERE id = $1"#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(scan)
    }

    pub async fn latest_scan(
        &self,
        library_id: Option<Uuid>,
    ) -> Result<Option<DuplicateScanRecord>> {
        let scan = sqlx::query_as!(
            DuplicateScanRecord,
            r#"SELECT id, library_id, task_id, include_semantic, algorithm_version,
                      status, progress, progress_message, books_total, books_processed,
                      chapters_processed, candidates_found, pairs_found, exact_pairs,
                      contained_pairs, semantic_pairs, error_message, started_at,
                      completed_at, created_at
               FROM dedup_scan_runs
               WHERE ($1::uuid IS NULL OR library_id = $1)
               ORDER BY created_at DESC
               LIMIT 1"#,
            library_id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(scan)
    }

    pub async fn latest_scan_in_libraries(
        &self,
        library_ids: &[Uuid],
    ) -> Result<Option<DuplicateScanRecord>> {
        let scan = sqlx::query_as!(
            DuplicateScanRecord,
            r#"SELECT id, library_id, task_id, include_semantic, algorithm_version,
                      status, progress, progress_message, books_total, books_processed,
                      chapters_processed, candidates_found, pairs_found, exact_pairs,
                      contained_pairs, semantic_pairs, error_message, started_at,
                      completed_at, created_at
               FROM dedup_scan_runs
               WHERE library_id = ANY($1)
               ORDER BY created_at DESC
               LIMIT 1"#,
            library_ids
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(scan)
    }

    pub async fn list_pairs(
        &self,
        filter: DuplicatePairFilter<'_>,
    ) -> Result<Vec<DuplicatePairRecord>> {
        let mut builder = QueryBuilder::<Postgres>::new(PAIR_SELECT);
        builder.push(" WHERE p.stale = FALSE");
        if let Some(ids) = filter.visible_library_ids {
            builder
                .push(" AND a.library_id = ANY(")
                .push_bind(ids)
                .push(") AND b.library_id = ANY(")
                .push_bind(ids)
                .push(")");
        }
        if let Some(library_id) = filter.library_id {
            builder
                .push(" AND (a.library_id = ")
                .push_bind(library_id)
                .push(" OR b.library_id = ")
                .push_bind(library_id)
                .push(")");
        }
        match filter.candidate_kind {
            Some(DuplicateCandidateKind::Content) => {
                builder.push(" AND p.relation <> 'semantic_relation'");
            }
            Some(DuplicateCandidateKind::Semantic) => {
                builder.push(" AND p.relation = 'semantic_relation'");
            }
            None => {}
        }
        if let Some(relation) = filter.relation {
            builder
                .push(" AND p.relation = ")
                .push_bind(relation.as_str());
        }
        if let Some(status) = filter.status {
            builder
                .push(" AND p.review_status = ")
                .push_bind(status.as_str());
        }
        builder
            .push(" ORDER BY p.confidence DESC, p.updated_at DESC LIMIT ")
            .push_bind(filter.limit)
            .push(" OFFSET ")
            .push_bind(filter.offset);

        let rows = builder
            .build_query_as::<RawDuplicatePairRecord>()
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter()
            .map(DuplicatePairRecord::try_from)
            .collect()
    }

    pub async fn pair(&self, id: Uuid) -> Result<Option<DuplicatePairRecord>> {
        let row = sqlx::query_as!(
            RawDuplicatePairRecord,
            r#"SELECT COUNT(*) OVER() AS "total_count!",
                      p.id, p.relation, p.review_status AS status, p.confidence,
                      p.shared_chapters, p.coverage_a, p.coverage_b,
                      p.character_coverage_a, p.character_coverage_b,
                      p.longest_contiguous_run, p.order_score, p.contained_book_id,
                      p.recommended_primary_id, p.semantic_score, p.evidence,
                      p.created_at, p.updated_at,
                      a.id AS book_a_id, a.title AS book_a_title, a.author AS book_a_author,
                      a.format::text AS "book_a_format!", a.file_size_bytes AS book_a_file_size,
                      a.word_count AS book_a_word_count, a.chapter_count AS book_a_chapter_count,
                      a.cover_path AS book_a_cover_path,
                      b.id AS book_b_id, b.title AS book_b_title, b.author AS book_b_author,
                      b.format::text AS "book_b_format!", b.file_size_bytes AS book_b_file_size,
                      b.word_count AS book_b_word_count, b.chapter_count AS book_b_chapter_count,
                      b.cover_path AS book_b_cover_path
               FROM duplicate_pairs p
               JOIN books a ON a.id = p.book_a_id
               JOIN books b ON b.id = p.book_b_id
               WHERE p.id = $1 AND p.stale = FALSE"#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;

        row.map(DuplicatePairRecord::try_from).transpose()
    }

    pub async fn chapter_matches(
        &self,
        pair_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<DuplicateChapterMatchPage> {
        let total = sqlx::query_scalar!(
            r#"SELECT COUNT(*) AS "count!"
               FROM duplicate_chapter_matches
               WHERE pair_id = $1"#,
            pair_id
        )
        .fetch_one(&self.pool)
        .await?;

        let indices = sqlx::query!(
            r#"SELECT
                 COALESCE(
                   array_agg(DISTINCT chapter_a_index ORDER BY chapter_a_index)
                     FILTER (WHERE chapter_a_index IS NOT NULL),
                   ARRAY[]::integer[]
                 ) AS "matched_indices_a!",
                 COALESCE(
                   array_agg(DISTINCT chapter_b_index ORDER BY chapter_b_index)
                     FILTER (WHERE chapter_b_index IS NOT NULL),
                   ARRAY[]::integer[]
                 ) AS "matched_indices_b!"
               FROM duplicate_chapter_matches
               WHERE pair_id = $1"#,
            pair_id
        )
        .fetch_one(&self.pool)
        .await?;

        let items = sqlx::query_as!(
            DuplicateChapterMatchRecord,
            r#"SELECT m.id, m.match_type, m.similarity, m.shared_fingerprints,
                      m.alignment_group, m.segment_ordinal,
                      m.chapter_a_start, m.chapter_a_end,
                      m.chapter_b_start, m.chapter_b_end, m.matched_chars,
                      m.chapter_a_id, m.chapter_a_index, ca.title AS chapter_a_title,
                      m.chapter_b_id, m.chapter_b_index, cb.title AS chapter_b_title
               FROM duplicate_chapter_matches m
               LEFT JOIN chapters ca ON ca.id = m.chapter_a_id
               LEFT JOIN chapters cb ON cb.id = m.chapter_b_id
               WHERE m.pair_id = $1
               ORDER BY m.chapter_a_index NULLS LAST, m.chapter_b_index NULLS LAST
               LIMIT $2 OFFSET $3"#,
            pair_id,
            limit,
            offset
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(DuplicateChapterMatchPage {
            total,
            matched_indices_a: indices.matched_indices_a,
            matched_indices_b: indices.matched_indices_b,
            items,
        })
    }

    pub async fn pair_book_ids(&self, pair_id: Uuid) -> Result<Option<(Uuid, Uuid)>> {
        let row = sqlx::query!(
            r#"SELECT book_a_id, book_b_id
               FROM duplicate_pairs
               WHERE id = $1"#,
            pair_id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|record| (record.book_a_id, record.book_b_id)))
    }

    pub async fn work_member_ids_for_books(&self, book_ids: &[Uuid]) -> Result<Vec<Uuid>> {
        let member_ids = sqlx::query_scalar!(
            r#"SELECT DISTINCT member.id AS "id!"
               FROM books pair_book
               JOIN books member ON member.work_id = pair_book.work_id
               WHERE pair_book.id = ANY($1) AND pair_book.work_id IS NOT NULL"#,
            book_ids
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(member_ids)
    }

    pub async fn book_versions(
        &self,
        book_id: Uuid,
        visible_library_ids: Option<&[Uuid]>,
    ) -> Result<BookVersionsRecord> {
        let work_id = sqlx::query_scalar!("SELECT work_id FROM books WHERE id = $1", book_id)
            .fetch_one(&self.pool)
            .await?;

        let mut builder = QueryBuilder::<Postgres>::new(
            r#"SELECT b.id, b.title, b.author, b.format::text, b.status::text,
                      b.chapter_count, b.word_count, b.file_size_bytes, b.cover_path,
                      b.created_at, (w.primary_book_id = b.id) AS is_primary
               FROM books b
               LEFT JOIN book_works w ON w.id = b.work_id
               WHERE "#,
        );
        if let Some(work_id) = work_id {
            builder.push("b.work_id = ").push_bind(work_id);
        } else {
            builder.push("b.id = ").push_bind(book_id);
        }
        if let Some(ids) = visible_library_ids {
            builder
                .push(" AND b.library_id = ANY(")
                .push_bind(ids)
                .push(")");
        }
        builder.push(" ORDER BY is_primary DESC, b.word_count DESC");
        let versions = builder
            .build_query_as::<BookVersionRecord>()
            .fetch_all(&self.pool)
            .await?;

        let work = match (work_id, visible_library_ids) {
            // A restricted caller must never receive metadata derived from a
            // version outside their visible libraries. A single visible
            // member is intentionally indistinguishable from an ungrouped
            // book at the work level.
            (Some(work_id), Some(_)) => restricted_work_from_versions(work_id, &versions),
            (Some(work_id), None) => {
                sqlx::query_as!(
                    BookWorkRecord,
                    r#"SELECT id, canonical_title, canonical_author, primary_book_id
                   FROM book_works
                   WHERE id = $1"#,
                    work_id
                )
                .fetch_optional(&self.pool)
                .await?
            }
            (None, _) => None,
        };

        Ok(BookVersionsRecord {
            work_id,
            work,
            versions,
        })
    }

    pub async fn chapter_diff_source(
        &self,
        pair_id: Uuid,
        match_id: Uuid,
    ) -> Result<Option<DuplicateChapterDiffSource>> {
        let source = sqlx::query_as!(
            DuplicateChapterDiffSource,
            r#"SELECT p.book_a_id, p.book_b_id,
                      ca.id AS chapter_a_id, cb.id AS chapter_b_id,
                      ca.title AS chapter_a_title, cb.title AS chapter_b_title,
                      ca.content AS content_a, cb.content AS content_b,
                      m.chapter_a_start, m.chapter_a_end,
                      m.chapter_b_start, m.chapter_b_end
               FROM duplicate_chapter_matches m
               JOIN duplicate_pairs p ON p.id = m.pair_id
               JOIN chapters ca ON ca.id = m.chapter_a_id
               JOIN chapters cb ON cb.id = m.chapter_b_id
               WHERE m.id = $1 AND m.pair_id = $2 AND p.stale = FALSE"#,
            match_id,
            pair_id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(source)
    }
}

fn restricted_work_from_versions(
    work_id: Uuid,
    versions: &[BookVersionRecord],
) -> Option<BookWorkRecord> {
    if versions.len() < 2 {
        return None;
    }

    let visible_primary = versions
        .iter()
        .find(|version| version.is_primary == Some(true));
    let canonical_source = visible_primary.or_else(|| versions.first())?;

    Some(BookWorkRecord {
        id: work_id,
        canonical_title: canonical_source.title.clone(),
        canonical_author: canonical_source.author.clone(),
        primary_book_id: visible_primary.map(|version| version.id),
    })
}

#[derive(Debug, Clone)]
pub struct DuplicateScanRecord {
    pub id: Uuid,
    pub library_id: Option<Uuid>,
    pub task_id: Option<Uuid>,
    pub include_semantic: bool,
    pub algorithm_version: i32,
    pub status: String,
    pub progress: i16,
    pub progress_message: Option<String>,
    pub books_total: i32,
    pub books_processed: i32,
    pub chapters_processed: i32,
    pub candidates_found: i32,
    pub pairs_found: i32,
    pub exact_pairs: i32,
    pub contained_pairs: i32,
    pub semantic_pairs: i32,
    pub error_message: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

pub struct DuplicatePairFilter<'a> {
    pub visible_library_ids: Option<&'a [Uuid]>,
    pub library_id: Option<Uuid>,
    pub candidate_kind: Option<DuplicateCandidateKind>,
    pub relation: Option<DuplicateRelation>,
    pub status: Option<DuplicateReviewStatus>,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug)]
pub struct DuplicatePairRecord {
    pub total_count: i64,
    pub id: Uuid,
    pub relation: DuplicateRelation,
    pub status: DuplicateReviewStatus,
    pub confidence: f64,
    pub shared_chapters: i32,
    pub coverage_a: f64,
    pub coverage_b: f64,
    pub character_coverage_a: f64,
    pub character_coverage_b: f64,
    pub longest_contiguous_run: i32,
    pub order_score: f64,
    pub contained_book_id: Option<Uuid>,
    pub recommended_primary_id: Option<Uuid>,
    pub semantic_score: Option<f64>,
    pub evidence: DuplicatePairEvidence,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub book_a_id: Uuid,
    pub book_a_title: String,
    pub book_a_author: Option<String>,
    pub book_a_format: String,
    pub book_a_file_size: i64,
    pub book_a_word_count: i64,
    pub book_a_chapter_count: i32,
    pub book_a_cover_path: Option<String>,
    pub book_b_id: Uuid,
    pub book_b_title: String,
    pub book_b_author: Option<String>,
    pub book_b_format: String,
    pub book_b_file_size: i64,
    pub book_b_word_count: i64,
    pub book_b_chapter_count: i32,
    pub book_b_cover_path: Option<String>,
}

#[derive(Debug, FromRow)]
struct RawDuplicatePairRecord {
    total_count: i64,
    id: Uuid,
    relation: String,
    status: String,
    confidence: f64,
    shared_chapters: i32,
    coverage_a: f64,
    coverage_b: f64,
    character_coverage_a: f64,
    character_coverage_b: f64,
    longest_contiguous_run: i32,
    order_score: f64,
    contained_book_id: Option<Uuid>,
    recommended_primary_id: Option<Uuid>,
    semantic_score: Option<f64>,
    evidence: serde_json::Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    book_a_id: Uuid,
    book_a_title: String,
    book_a_author: Option<String>,
    book_a_format: String,
    book_a_file_size: i64,
    book_a_word_count: i64,
    book_a_chapter_count: i32,
    book_a_cover_path: Option<String>,
    book_b_id: Uuid,
    book_b_title: String,
    book_b_author: Option<String>,
    book_b_format: String,
    book_b_file_size: i64,
    book_b_word_count: i64,
    book_b_chapter_count: i32,
    book_b_cover_path: Option<String>,
}

impl TryFrom<RawDuplicatePairRecord> for DuplicatePairRecord {
    type Error = Error;

    fn try_from(row: RawDuplicatePairRecord) -> Result<Self> {
        let relation = DuplicateRelation::from_str(&row.relation).map_err(|error| {
            Error::Internal(format!(
                "invalid duplicate relation persisted in database: {error}"
            ))
        })?;
        let status = DuplicateReviewStatus::from_str(&row.status).map_err(|error| {
            Error::Internal(format!(
                "invalid duplicate review status persisted in database: {error}"
            ))
        })?;
        let evidence = decode_duplicate_pair_evidence(row.evidence)?;

        Ok(Self {
            total_count: row.total_count,
            id: row.id,
            relation,
            status,
            confidence: row.confidence,
            shared_chapters: row.shared_chapters,
            coverage_a: row.coverage_a,
            coverage_b: row.coverage_b,
            character_coverage_a: row.character_coverage_a,
            character_coverage_b: row.character_coverage_b,
            longest_contiguous_run: row.longest_contiguous_run,
            order_score: row.order_score,
            contained_book_id: row.contained_book_id,
            recommended_primary_id: row.recommended_primary_id,
            semantic_score: row.semantic_score,
            evidence,
            created_at: row.created_at,
            updated_at: row.updated_at,
            book_a_id: row.book_a_id,
            book_a_title: row.book_a_title,
            book_a_author: row.book_a_author,
            book_a_format: row.book_a_format,
            book_a_file_size: row.book_a_file_size,
            book_a_word_count: row.book_a_word_count,
            book_a_chapter_count: row.book_a_chapter_count,
            book_a_cover_path: row.book_a_cover_path,
            book_b_id: row.book_b_id,
            book_b_title: row.book_b_title,
            book_b_author: row.book_b_author,
            book_b_format: row.book_b_format,
            book_b_file_size: row.book_b_file_size,
            book_b_word_count: row.book_b_word_count,
            book_b_chapter_count: row.book_b_chapter_count,
            book_b_cover_path: row.book_b_cover_path,
        })
    }
}

pub(crate) fn decode_duplicate_pair_evidence(
    evidence: serde_json::Value,
) -> Result<DuplicatePairEvidence> {
    serde_json::from_value(evidence).map_err(|error| {
        Error::Internal(format!(
            "invalid duplicate evidence persisted in database: {error}"
        ))
    })
}

#[derive(Debug)]
pub struct DuplicateChapterMatchRecord {
    pub id: Uuid,
    pub match_type: String,
    pub similarity: f64,
    pub shared_fingerprints: i32,
    pub alignment_group: Option<i32>,
    pub segment_ordinal: Option<i32>,
    pub chapter_a_start: Option<i32>,
    pub chapter_a_end: Option<i32>,
    pub chapter_b_start: Option<i32>,
    pub chapter_b_end: Option<i32>,
    pub matched_chars: i32,
    pub chapter_a_id: Option<Uuid>,
    pub chapter_a_index: Option<i32>,
    pub chapter_a_title: Option<String>,
    pub chapter_b_id: Option<Uuid>,
    pub chapter_b_index: Option<i32>,
    pub chapter_b_title: Option<String>,
}

pub struct DuplicateChapterMatchPage {
    pub total: i64,
    pub matched_indices_a: Vec<i32>,
    pub matched_indices_b: Vec<i32>,
    pub items: Vec<DuplicateChapterMatchRecord>,
}

#[derive(Debug, FromRow)]
pub struct BookVersionRecord {
    pub id: Uuid,
    pub title: String,
    pub author: Option<String>,
    pub format: String,
    pub status: String,
    pub chapter_count: i32,
    pub word_count: i64,
    pub file_size_bytes: i64,
    pub cover_path: Option<String>,
    pub created_at: DateTime<Utc>,
    pub is_primary: Option<bool>,
}

pub struct BookWorkRecord {
    pub id: Uuid,
    pub canonical_title: String,
    pub canonical_author: Option<String>,
    pub primary_book_id: Option<Uuid>,
}

pub struct BookVersionsRecord {
    pub work_id: Option<Uuid>,
    pub work: Option<BookWorkRecord>,
    pub versions: Vec<BookVersionRecord>,
}

pub struct DuplicateChapterDiffSource {
    pub book_a_id: Uuid,
    pub book_b_id: Uuid,
    pub chapter_a_id: Uuid,
    pub chapter_b_id: Uuid,
    pub chapter_a_title: Option<String>,
    pub chapter_b_title: Option<String>,
    pub content_a: String,
    pub content_b: String,
    pub chapter_a_start: Option<i32>,
    pub chapter_a_end: Option<i32>,
    pub chapter_b_start: Option<i32>,
    pub chapter_b_end: Option<i32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(
        id: Uuid,
        title: &str,
        author: Option<&str>,
        is_primary: Option<bool>,
    ) -> BookVersionRecord {
        BookVersionRecord {
            id,
            title: title.to_string(),
            author: author.map(str::to_string),
            format: "txt".to_string(),
            status: "ready".to_string(),
            chapter_count: 10,
            word_count: 1_000,
            file_size_bytes: 2_000,
            cover_path: None,
            created_at: Utc::now(),
            is_primary,
        }
    }

    #[test]
    fn restricted_single_visible_version_hides_work_membership() {
        let visible = version(Uuid::now_v7(), "Visible", None, Some(false));

        let work = restricted_work_from_versions(Uuid::now_v7(), &[visible]);

        assert!(work.is_none());
    }

    #[test]
    fn restricted_work_uses_only_visible_primary_metadata() {
        let visible_secondary = version(
            Uuid::now_v7(),
            "Visible secondary",
            Some("Secondary author"),
            Some(false),
        );
        let visible_primary_id = Uuid::now_v7();
        let visible_primary = version(
            visible_primary_id,
            "Visible primary",
            Some("Primary author"),
            Some(true),
        );

        let work =
            restricted_work_from_versions(Uuid::now_v7(), &[visible_secondary, visible_primary])
                .expect("two visible versions should expose their work");

        assert_eq!(work.canonical_title, "Visible primary");
        assert_eq!(work.canonical_author.as_deref(), Some("Primary author"));
        assert_eq!(work.primary_book_id, Some(visible_primary_id));
    }

    #[test]
    fn restricted_work_does_not_promote_a_visible_version_when_primary_is_hidden() {
        let first_id = Uuid::now_v7();
        let visible = vec![
            version(
                first_id,
                "Longest visible",
                Some("Visible author"),
                Some(false),
            ),
            version(Uuid::now_v7(), "Other visible", None, Some(false)),
        ];

        let work = restricted_work_from_versions(Uuid::now_v7(), &visible)
            .expect("two visible versions should expose their work");

        assert_eq!(work.canonical_title, "Longest visible");
        assert_eq!(work.canonical_author.as_deref(), Some("Visible author"));
        assert_eq!(work.primary_book_id, None);
        assert!(visible
            .iter()
            .all(|version| version.is_primary == Some(false)));
    }
}
