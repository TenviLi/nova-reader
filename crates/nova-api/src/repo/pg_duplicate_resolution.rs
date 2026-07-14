use std::str::FromStr;

use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use nova_core::{
    domain::dedup::{DedupIndexCleanupTask, DuplicatePairEvidence, DuplicateReviewStatus},
    Error, Result,
};

use super::pg_duplicate::{decode_duplicate_pair_evidence, PgDuplicateRepository};

/// The pair and concrete versions locked for one resolution decision.
#[derive(Debug)]
pub(crate) struct DuplicateResolutionPair {
    pub review_status: DuplicateReviewStatus,
    pub stale: bool,
    pub recommended_primary_id: Option<Uuid>,
    pub book_a_id: Uuid,
    pub book_b_id: Uuid,
    pub book_a_library_id: Option<Uuid>,
    pub book_b_library_id: Option<Uuid>,
    pub book_a_title: String,
    pub book_b_title: String,
    pub book_a_author: Option<String>,
    pub book_b_author: Option<String>,
    pub book_a_work_id: Option<Uuid>,
    pub book_b_work_id: Option<Uuid>,
    pub algorithm_version: i32,
    pub evidence: DuplicatePairEvidence,
}

#[derive(Debug)]
struct RawDuplicateResolutionPair {
    review_status: String,
    stale: bool,
    recommended_primary_id: Option<Uuid>,
    book_a_id: Uuid,
    book_b_id: Uuid,
    book_a_library_id: Option<Uuid>,
    book_b_library_id: Option<Uuid>,
    book_a_title: String,
    book_b_title: String,
    book_a_author: Option<String>,
    book_b_author: Option<String>,
    book_a_work_id: Option<Uuid>,
    book_b_work_id: Option<Uuid>,
    algorithm_version: i32,
    evidence: serde_json::Value,
}

impl TryFrom<RawDuplicateResolutionPair> for DuplicateResolutionPair {
    type Error = Error;

    fn try_from(row: RawDuplicateResolutionPair) -> Result<Self> {
        let review_status =
            DuplicateReviewStatus::from_str(&row.review_status).map_err(|error| {
                Error::Internal(format!(
                    "invalid duplicate review status persisted in database: {error}"
                ))
            })?;
        let evidence = decode_duplicate_pair_evidence(row.evidence)?;
        Ok(Self {
            review_status,
            stale: row.stale,
            recommended_primary_id: row.recommended_primary_id,
            book_a_id: row.book_a_id,
            book_b_id: row.book_b_id,
            book_a_library_id: row.book_a_library_id,
            book_b_library_id: row.book_b_library_id,
            book_a_title: row.book_a_title,
            book_b_title: row.book_b_title,
            book_a_author: row.book_a_author,
            book_b_author: row.book_b_author,
            book_a_work_id: row.book_a_work_id,
            book_b_work_id: row.book_b_work_id,
            algorithm_version: row.algorithm_version,
            evidence,
        })
    }
}

#[derive(Debug)]
struct ResolutionBookFingerprint {
    book_id: Uuid,
    normalization_version: i32,
    algorithm_version: i32,
    source_content_hash: String,
    layout_hash: String,
    chapter_count: i32,
}

#[derive(Debug)]
struct ResolutionChapterContent {
    id: Uuid,
    book_id: Uuid,
    chapter_index: i32,
    content: String,
}

#[derive(Debug)]
struct ResolutionChapterFingerprint {
    chapter_id: Uuid,
    book_id: Uuid,
    chapter_index: i32,
    normalization_version: i32,
    source_content_hash: String,
}

#[derive(Debug)]
struct RedundantChapterIndexCandidate {
    chapter_index: i32,
    secondary_content: String,
    secondary_source_content_hash: String,
    primary_content: String,
    primary_source_content_hash: String,
}

#[derive(Debug)]
struct ResolutionChapterIdentity {
    id: Uuid,
    book_id: Uuid,
    chapter_index: i32,
}

#[derive(Debug)]
struct ResolutionChapterMapping {
    chapter_a_id: Option<Uuid>,
    chapter_b_id: Option<Uuid>,
    chapter_a_index: Option<i32>,
    chapter_b_index: Option<i32>,
}

#[derive(Debug)]
struct DirectedChapterMapping {
    source_chapter_id: Uuid,
    source_chapter_index: i32,
    target_chapter_id: Uuid,
    target_chapter_index: i32,
}

#[derive(Debug)]
struct ResolutionProgress {
    id: Uuid,
    user_id: Option<Uuid>,
    book_id: Uuid,
    chapter_id: Option<Uuid>,
    chapter_index: Option<i32>,
    current_chapter: i32,
    progress: f64,
    scroll_position: Option<f64>,
    reading_time_secs: i64,
    last_read_at: chrono::DateTime<chrono::Utc>,
}

/// Direction of a verified chapter mapping relative to the selected primary.
#[derive(Debug, Clone, Copy)]
pub(crate) enum ChapterMappingDirection {
    BookAToBookB,
    BookBToBookA,
}

/// Books whose write access must be re-checked inside the locked resolution
/// transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResolutionAuthorizationScope {
    /// Review-only actions affect this candidate pair, not either book's work.
    PairBooks,
    /// Grouping actions can mutate every member of either existing work.
    WorkMembers,
}

impl ChapterMappingDirection {
    const fn primary_is_book_a(self) -> bool {
        matches!(self, Self::BookBToBookA)
    }
}

/// A single atomic duplicate-resolution transaction.
///
/// The raw SQLx transaction never crosses the repository boundary. Dropping
/// this value rolls the decision back; only `commit` publishes it.
pub(crate) struct DuplicateResolutionTransaction<'a> {
    transaction: Transaction<'a, Postgres>,
    locked_book_ids: Vec<Uuid>,
}

impl PgDuplicateRepository {
    pub(crate) async fn begin_resolution(&self) -> Result<DuplicateResolutionTransaction<'_>> {
        Ok(DuplicateResolutionTransaction {
            transaction: self.pool.begin().await?,
            locked_book_ids: Vec::new(),
        })
    }

    /// Returns only exact, pair-scoped chapter mappings whose raw source text
    /// still matches the SHA-256 fingerprints validated during the scan.
    /// Approximate matches and unique chapters are deliberately retained in
    /// both search indexes even after the containing book becomes a secondary
    /// version.
    pub(crate) async fn verified_redundant_chapter_indexes(
        &self,
        pair_id: Uuid,
        secondary_id: Uuid,
        primary_id: Uuid,
    ) -> Result<Vec<i32>> {
        let candidates = sqlx::query_as!(
            RedundantChapterIndexCandidate,
            r#"SELECT secondary_chapter.chapter_index AS "chapter_index!",
                      secondary_chapter.content AS "secondary_content!",
                      secondary_fp.source_content_hash
                        AS "secondary_source_content_hash!",
                      primary_chapter.content AS "primary_content!",
                      primary_fp.source_content_hash
                        AS "primary_source_content_hash!"
               FROM duplicate_pairs pair
               JOIN duplicate_chapter_matches mapping ON mapping.pair_id = pair.id
               JOIN chapters secondary_chapter
                 ON secondary_chapter.id = CASE
                      WHEN pair.book_a_id = $2 THEN mapping.chapter_a_id
                      ELSE mapping.chapter_b_id
                    END
               JOIN chapters primary_chapter
                 ON primary_chapter.id = CASE
                      WHEN pair.book_a_id = $3 THEN mapping.chapter_a_id
                      ELSE mapping.chapter_b_id
                    END
               JOIN chapter_fingerprints secondary_fp
                 ON secondary_fp.chapter_id = secondary_chapter.id
               JOIN chapter_fingerprints primary_fp
                 ON primary_fp.chapter_id = primary_chapter.id
               JOIN books secondary_book ON secondary_book.id = secondary_chapter.book_id
               WHERE pair.id = $1
                 AND pair.resolved = TRUE
                 AND pair.review_status = 'confirmed'
                 AND pair.recommended_primary_id = $3
                 AND (
                   (pair.book_a_id = $2 AND pair.book_b_id = $3)
                   OR (pair.book_a_id = $3 AND pair.book_b_id = $2)
                 )
                 AND secondary_chapter.book_id = $2
                 AND primary_chapter.book_id = $3
                 AND secondary_book.status = 'duplicate'::book_status
                 AND secondary_fp.source_content_hash = primary_fp.source_content_hash
                 AND (
                   (mapping.match_type = 'conservative'
                    AND secondary_fp.conservative_hash = primary_fp.conservative_hash)
                   OR (mapping.match_type = 'layout'
                    AND secondary_fp.layout_hash = primary_fp.layout_hash)
                 )
               ORDER BY secondary_chapter.chapter_index"#,
            pair_id,
            secondary_id,
            primary_id
        )
        .fetch_all(&self.pool)
        .await?;

        let mut indexes = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            let secondary_hash =
                nova_ingest::dedup::sha256(candidate.secondary_content.as_bytes()).to_hex();
            let primary_hash =
                nova_ingest::dedup::sha256(candidate.primary_content.as_bytes()).to_hex();
            if secondary_hash == primary_hash
                && secondary_hash == candidate.secondary_source_content_hash
                && primary_hash == candidate.primary_source_content_hash
            {
                indexes.push(candidate.chapter_index);
            }
        }
        indexes.sort_unstable();
        indexes.dedup();
        Ok(indexes)
    }
}

impl DuplicateResolutionTransaction<'_> {
    pub(crate) async fn lock_pair(
        &mut self,
        pair_id: Uuid,
        scope: ResolutionAuthorizationScope,
    ) -> Result<Option<DuplicateResolutionPair>> {
        let global_acquired: bool =
            sqlx::query_scalar("SELECT try_lock_novel_dedup_global_barrier()")
                .fetch_one(&mut *self.transaction)
                .await?;
        if !global_acquired {
            return Err(Error::RetryableConflict(
                "duplicate resolution global write barrier is busy; retry resolution".into(),
            ));
        }

        // Discover every book this action can lock before taking any row lock,
        // then enter the same fail-fast advisory protocol used by content
        // mutation/publication. Grouping can merge two existing works, so only
        // locking the concrete pair books would still allow two resolutions to
        // deadlock while each waits for members held by the other.
        let initial_book_ids: Vec<Uuid> = sqlx::query_scalar(
            r#"SELECT member.id
               FROM duplicate_pairs pair
               JOIN books book_a ON book_a.id = pair.book_a_id
               JOIN books book_b ON book_b.id = pair.book_b_id
               JOIN books member
                 ON member.id IN (pair.book_a_id, pair.book_b_id)
                 OR (
                   $2
                   AND member.work_id IS NOT NULL
                   AND member.work_id IN (book_a.work_id, book_b.work_id)
                 )
               WHERE pair.id = $1
               ORDER BY member.id"#,
        )
        .bind(pair_id)
        .bind(scope == ResolutionAuthorizationScope::WorkMembers)
        .fetch_all(&mut *self.transaction)
        .await?;
        if initial_book_ids.is_empty() {
            return Ok(None);
        }
        let acquired: bool = sqlx::query_scalar("SELECT try_lock_novel_dedup_books($1)")
            .bind(initial_book_ids.as_slice())
            .fetch_one(&mut *self.transaction)
            .await?;
        if !acquired {
            return Err(Error::RetryableConflict(
                "duplicate resolution book lock is busy; retry resolution".into(),
            ));
        }
        self.locked_book_ids = initial_book_ids;

        let pair = sqlx::query_as!(
            RawDuplicateResolutionPair,
            r#"SELECT p.review_status, p.stale, p.recommended_primary_id,
                      p.book_a_id, p.book_b_id,
                      a.library_id AS book_a_library_id,
                      b.library_id AS book_b_library_id,
                      a.title AS book_a_title, b.title AS book_b_title,
                      a.author AS book_a_author, b.author AS book_b_author,
                      a.work_id AS book_a_work_id, b.work_id AS book_b_work_id,
                      p.algorithm_version, p.evidence
               FROM duplicate_pairs p
               JOIN books a ON a.id = p.book_a_id
               JOIN books b ON b.id = p.book_b_id
               WHERE p.id = $1
               FOR UPDATE OF p, a, b"#,
            pair_id
        )
        .fetch_optional(&mut *self.transaction)
        .await?;

        let pair = pair.map(DuplicateResolutionPair::try_from).transpose()?;
        if pair.as_ref().is_some_and(|pair| {
            !self.locked_book_ids.contains(&pair.book_a_id)
                || !self.locked_book_ids.contains(&pair.book_b_id)
        }) {
            return Err(Error::RetryableConflict(
                "duplicate pair books changed while resolution locks were acquired; retry resolution"
                    .into(),
            ));
        }
        Ok(pair)
    }

    /// Verifies that the persisted pair evidence was computed from the exact
    /// chapter content currently stored for both concrete versions.
    ///
    /// `chapter_fingerprints` are only a cache, so comparing pair evidence to
    /// those rows alone is insufficient: chapter content can change before the
    /// next dedup scan. The current chapter rows are locked and hashed again in
    /// this resolution transaction before any work or reader asset is mutated.
    pub(crate) async fn pair_evidence_matches_current_content(
        &mut self,
        pair: &DuplicateResolutionPair,
    ) -> Result<bool> {
        let book_ids = [pair.book_a_id, pair.book_b_id];
        let book_fingerprints = sqlx::query_as!(
            ResolutionBookFingerprint,
            r#"SELECT bf.book_id, bf.normalization_version, bf.algorithm_version,
                      bf.source_content_hash, bf.layout_hash, bf.chapter_count
               FROM book_fingerprints bf
               WHERE bf.book_id = ANY($1)
               ORDER BY bf.book_id
               FOR UPDATE OF bf"#,
            book_ids.as_slice()
        )
        .fetch_all(&mut *self.transaction)
        .await?;
        let chapters = sqlx::query_as!(
            ResolutionChapterContent,
            r#"SELECT id, book_id, chapter_index AS "chapter_index!", content
               FROM chapters
               WHERE book_id = ANY($1)
               ORDER BY book_id, chapter_index
               FOR UPDATE"#,
            book_ids.as_slice()
        )
        .fetch_all(&mut *self.transaction)
        .await?;
        let chapter_fingerprints = sqlx::query_as!(
            ResolutionChapterFingerprint,
            r#"SELECT chapter_id, book_id, chapter_index, normalization_version,
                      source_content_hash
               FROM chapter_fingerprints
               WHERE book_id = ANY($1)
               ORDER BY book_id, chapter_index
               FOR UPDATE"#,
            book_ids.as_slice()
        )
        .fetch_all(&mut *self.transaction)
        .await?;

        let expected_algorithm = i32::from(nova_ingest::dedup::DEDUP_ALGORITHM_VERSION);
        let expected_normalization =
            i32::from(nova_ingest::dedup::CONSERVATIVE_NORMALIZATION_VERSION);
        if pair.algorithm_version != expected_algorithm
            || pair.evidence.algorithm_version != pair.algorithm_version
            || book_fingerprints.len() != book_ids.len()
            || chapters.len() != chapter_fingerprints.len()
        {
            return Ok(false);
        }

        for (chapter, fingerprint) in chapters.iter().zip(&chapter_fingerprints) {
            if chapter.id != fingerprint.chapter_id
                || chapter.book_id != fingerprint.book_id
                || chapter.chapter_index != fingerprint.chapter_index
                || fingerprint.normalization_version != expected_normalization
                || nova_ingest::dedup::sha256(chapter.content.as_bytes()).to_hex()
                    != fingerprint.source_content_hash
            {
                return Ok(false);
            }
        }

        for fingerprint in &book_fingerprints {
            let evidence_layout_hash = if fingerprint.book_id == pair.book_a_id {
                &pair.evidence.book_a_layout_hash
            } else if fingerprint.book_id == pair.book_b_id {
                &pair.evidence.book_b_layout_hash
            } else {
                return Ok(false);
            };
            let book_chapters: Vec<_> = chapters
                .iter()
                .filter(|chapter| chapter.book_id == fingerprint.book_id)
                .collect();
            let Ok(chapter_count) = i32::try_from(book_chapters.len()) else {
                return Ok(false);
            };
            if fingerprint.algorithm_version != expected_algorithm
                || fingerprint.normalization_version != expected_normalization
                || fingerprint.chapter_count != chapter_count
                || evidence_layout_hash != &fingerprint.layout_hash
                || source_content_hash(&book_chapters) != fingerprint.source_content_hash
            {
                return Ok(false);
            }
        }

        Ok(true)
    }

    pub(crate) async fn mark_pair_stale(&mut self, pair_id: Uuid) -> Result<()> {
        sqlx::query!(
            r#"UPDATE duplicate_pairs
               SET stale = TRUE, updated_at = NOW()
               WHERE id = $1"#,
            pair_id
        )
        .execute(&mut *self.transaction)
        .await?;
        Ok(())
    }

    pub(crate) async fn mark_dismissed(&mut self, pair_id: Uuid, user_id: Uuid) -> Result<()> {
        sqlx::query!(
            r#"UPDATE duplicate_pairs
               SET review_status = 'dismissed', resolved = TRUE,
                   resolution = 'dismiss', resolved_by = $2,
                   resolved_at = NOW(), updated_at = NOW()
               WHERE id = $1"#,
            pair_id,
            user_id
        )
        .execute(&mut *self.transaction)
        .await?;
        Ok(())
    }

    pub(crate) async fn mark_deferred(&mut self, pair_id: Uuid, user_id: Uuid) -> Result<()> {
        sqlx::query!(
            r#"UPDATE duplicate_pairs
               SET review_status = 'deferred', resolved = FALSE,
                   resolution = NULL, resolved_by = $2,
                   resolved_at = NOW(), updated_at = NOW()
               WHERE id = $1"#,
            pair_id,
            user_id
        )
        .execute(&mut *self.transaction)
        .await?;
        Ok(())
    }

    /// Re-checks write access after `lock_pair` has advisory-locked the complete
    /// action scope and row-locked the candidate pair. Grouping actions also
    /// row-lock and authorize every member of either existing work.
    pub(crate) async fn authorize_locked_books(
        &mut self,
        pair: &DuplicateResolutionPair,
        user_id: Uuid,
        scope: ResolutionAuthorizationScope,
    ) -> Result<()> {
        if scope == ResolutionAuthorizationScope::PairBooks {
            return self
                .authorize_locked_book_libraries(
                    &[pair.book_a_library_id, pair.book_b_library_id],
                    user_id,
                )
                .await;
        }

        let mut work_ids: Vec<Uuid> = [pair.book_a_work_id, pair.book_b_work_id]
            .into_iter()
            .flatten()
            .collect();
        work_ids.sort_unstable();
        work_ids.dedup();

        if !work_ids.is_empty() {
            sqlx::query_scalar!(
                r#"SELECT id AS "id!"
                   FROM book_works
                   WHERE id = ANY($1)
                   ORDER BY id
                   FOR UPDATE"#,
                work_ids.as_slice()
            )
            .fetch_all(&mut *self.transaction)
            .await?;
        }

        let pair_book_ids = [pair.book_a_id, pair.book_b_id];
        let members = sqlx::query!(
            r#"SELECT id, library_id
               FROM books
               WHERE id = ANY($1) OR work_id = ANY($2)
               ORDER BY id
               FOR UPDATE"#,
            pair_book_ids.as_slice(),
            work_ids.as_slice()
        )
        .fetch_all(&mut *self.transaction)
        .await?;

        if members
            .iter()
            .any(|member| !self.locked_book_ids.contains(&member.id))
        {
            return Err(Error::RetryableConflict(
                "duplicate work membership changed while resolution locks were acquired; retry resolution"
                    .into(),
            ));
        }

        let library_ids: Vec<Option<Uuid>> = members
            .into_iter()
            .map(|member| member.library_id)
            .collect();
        self.authorize_locked_book_libraries(&library_ids, user_id)
            .await
    }

    async fn authorize_locked_book_libraries(
        &mut self,
        book_library_ids: &[Option<Uuid>],
        user_id: Uuid,
    ) -> Result<()> {
        let role = sqlx::query_scalar!(
            r#"SELECT role::text AS "role!" FROM users WHERE id = $1"#,
            user_id
        )
        .fetch_optional(&mut *self.transaction)
        .await?;
        if role.as_deref() == Some("admin") {
            return Ok(());
        }

        let library_ids = required_library_ids(book_library_ids)?;

        let writable_library_count = sqlx::query_scalar!(
            r#"SELECT COUNT(*) AS "count!"
               FROM libraries library
               WHERE library.id = ANY($1)
                 AND (
                   EXISTS (
                     SELECT 1 FROM library_permissions direct
                     WHERE direct.library_id = library.id
                       AND direct.user_id = $2
                       AND (direct.can_write OR direct.can_manage)
                   )
                   OR EXISTS (
                     SELECT 1
                     FROM library_group_permissions grouped
                     JOIN group_members membership
                       ON membership.group_id = grouped.group_id
                     WHERE grouped.library_id = library.id
                       AND membership.user_id = $2
                       AND (grouped.can_write OR grouped.can_manage)
                   )
                 )"#,
            library_ids.as_slice(),
            user_id
        )
        .fetch_one(&mut *self.transaction)
        .await?;

        if usize::try_from(writable_library_count).ok() != Some(library_ids.len()) {
            return Err(Error::Forbidden);
        }
        Ok(())
    }

    pub(crate) async fn ensure_shared_work(
        &mut self,
        pair_id: Uuid,
        pair: &DuplicateResolutionPair,
        primary_id: Uuid,
        user_id: Uuid,
    ) -> Result<Uuid> {
        let mut existing_work_ids: Vec<Uuid> = [pair.book_a_work_id, pair.book_b_work_id]
            .into_iter()
            .flatten()
            .collect();
        existing_work_ids.sort_unstable();
        existing_work_ids.dedup();
        if !existing_work_ids.is_empty() {
            self.ensure_direct_primary_evidence(pair_id, &existing_work_ids, primary_id)
                .await?;
        }

        let work_id = match (pair.book_a_work_id, pair.book_b_work_id) {
            (Some(a), Some(b)) if a == b => a,
            (Some(a), Some(b)) => {
                sqlx::query!("UPDATE books SET work_id = $1 WHERE work_id = $2", a, b)
                    .execute(&mut *self.transaction)
                    .await?;
                sqlx::query!("DELETE FROM book_works WHERE id = $1", b)
                    .execute(&mut *self.transaction)
                    .await?;
                a
            }
            (Some(id), None) | (None, Some(id)) => id,
            (None, None) => {
                let (title, author) = if primary_id == pair.book_a_id {
                    (&pair.book_a_title, &pair.book_a_author)
                } else {
                    (&pair.book_b_title, &pair.book_b_author)
                };
                sqlx::query_scalar!(
                    r#"INSERT INTO book_works
                       (canonical_title, canonical_author, primary_book_id, created_by)
                       VALUES ($1, $2, $3, $4)
                       RETURNING id AS "id!""#,
                    title,
                    author.as_deref(),
                    primary_id,
                    user_id
                )
                .fetch_one(&mut *self.transaction)
                .await?
            }
        };

        let member_ids = [pair.book_a_id, pair.book_b_id];
        sqlx::query!(
            r#"UPDATE books
               SET work_id = $1, updated_at = NOW()
               WHERE id = ANY($2)"#,
            work_id,
            member_ids.as_slice()
        )
        .execute(&mut *self.transaction)
        .await?;
        sqlx::query!(
            r#"UPDATE book_works
               SET primary_book_id = $2, updated_at = NOW()
               WHERE id = $1"#,
            work_id,
            primary_id
        )
        .execute(&mut *self.transaction)
        .await?;
        Ok(work_id)
    }

    async fn ensure_direct_primary_evidence(
        &mut self,
        current_pair_id: Uuid,
        work_ids: &[Uuid],
        primary_id: Uuid,
    ) -> Result<()> {
        let unverified_member = sqlx::query_scalar!(
            r#"SELECT member.id AS "id!"
               FROM books member
               WHERE member.work_id = ANY($1)
                 AND member.id <> $2
                 AND NOT EXISTS (
                   SELECT 1 FROM duplicate_pairs verified
                   WHERE verified.book_a_id = LEAST(member.id, $2)
                     AND verified.book_b_id = GREATEST(member.id, $2)
                     AND verified.stale = FALSE
                     AND (verified.id = $3 OR verified.review_status = 'confirmed')
                 )
               LIMIT 1"#,
            work_ids,
            primary_id,
            current_pair_id
        )
        .fetch_optional(&mut *self.transaction)
        .await?;

        if unverified_member.is_some() {
            return Err(Error::Validation(
                "every version must be verified against the selected primary before works can merge"
                    .to_string(),
            ));
        }
        Ok(())
    }

    pub(crate) async fn mark_same_work_confirmed(
        &mut self,
        pair_id: Uuid,
        primary_id: Uuid,
        user_id: Uuid,
    ) -> Result<()> {
        sqlx::query!(
            r#"UPDATE duplicate_pairs
               SET review_status = 'confirmed', resolved = TRUE,
                   resolution = 'merge', recommended_primary_id = $2,
                   resolved_by = $3, resolved_at = NOW(), updated_at = NOW()
               WHERE id = $1"#,
            pair_id,
            primary_id,
            user_id
        )
        .execute(&mut *self.transaction)
        .await?;
        Ok(())
    }

    pub(crate) async fn migrate_reader_artifacts(
        &mut self,
        pair_id: Uuid,
        primary_id: Uuid,
        secondary_id: Uuid,
        direction: ChapterMappingDirection,
    ) -> Result<u64> {
        let primary_is_book_a = direction.primary_is_book_a();
        let annotation_count = sqlx::query!(
            r#"UPDATE annotations artifact
               SET book_id = $2,
                   chapter_id = CASE WHEN $4
                                     THEN mapping.chapter_a_id ELSE mapping.chapter_b_id END,
                   chapter_index = CASE WHEN $4
                                        THEN mapping.chapter_a_index ELSE mapping.chapter_b_index END,
                   updated_at = NOW()
               FROM duplicate_chapter_matches mapping
               WHERE mapping.pair_id = $1
                 AND artifact.book_id = $3
                 AND artifact.chapter_id = CASE WHEN $4
                                                THEN mapping.chapter_b_id ELSE mapping.chapter_a_id END
                 AND CASE WHEN $4
                          THEN mapping.chapter_a_id ELSE mapping.chapter_b_id END IS NOT NULL
                 AND mapping.match_type IN ('conservative', 'layout')
                 AND mapping.similarity >= 0.999999
                 AND EXISTS (
                   SELECT 1
                   FROM chapter_fingerprints source_fp
                   JOIN chapter_fingerprints target_fp
                     ON target_fp.source_content_hash = source_fp.source_content_hash
                   WHERE source_fp.chapter_id = CASE WHEN $4
                                                     THEN mapping.chapter_b_id ELSE mapping.chapter_a_id END
                     AND target_fp.chapter_id = CASE WHEN $4
                                                    THEN mapping.chapter_a_id ELSE mapping.chapter_b_id END
                 )"#,
            pair_id,
            primary_id,
            secondary_id,
            primary_is_book_a
        )
        .execute(&mut *self.transaction)
        .await?
        .rows_affected();

        let bookmark_count = sqlx::query!(
            r#"UPDATE bookmarks artifact
               SET book_id = $2,
                   chapter_id = CASE WHEN $4
                                     THEN mapping.chapter_a_id ELSE mapping.chapter_b_id END,
                   chapter_index = CASE WHEN $4
                                        THEN mapping.chapter_a_index ELSE mapping.chapter_b_index END
               FROM duplicate_chapter_matches mapping
               WHERE mapping.pair_id = $1
                 AND artifact.book_id = $3
                 AND artifact.chapter_id = CASE WHEN $4
                                                THEN mapping.chapter_b_id ELSE mapping.chapter_a_id END
                 AND CASE WHEN $4
                          THEN mapping.chapter_a_id ELSE mapping.chapter_b_id END IS NOT NULL
                 AND mapping.match_type IN ('conservative', 'layout')
                 AND mapping.similarity >= 0.999999
                 AND EXISTS (
                   SELECT 1
                   FROM chapter_fingerprints source_fp
                   JOIN chapter_fingerprints target_fp
                     ON target_fp.source_content_hash = source_fp.source_content_hash
                   WHERE source_fp.chapter_id = CASE WHEN $4
                                                     THEN mapping.chapter_b_id ELSE mapping.chapter_a_id END
                     AND target_fp.chapter_id = CASE WHEN $4
                                                    THEN mapping.chapter_a_id ELSE mapping.chapter_b_id END
                 )"#,
            pair_id,
            primary_id,
            secondary_id,
            primary_is_book_a
        )
        .execute(&mut *self.transaction)
        .await?
        .rows_affected();

        let migrated_progress = self
            .migrate_reading_progress(pair_id, primary_id, secondary_id, direction)
            .await?;

        Ok(annotation_count
            .saturating_add(bookmark_count)
            .saturating_add(migrated_progress))
    }

    async fn migrate_reading_progress(
        &mut self,
        pair_id: Uuid,
        primary_id: Uuid,
        secondary_id: Uuid,
        direction: ChapterMappingDirection,
    ) -> Result<u64> {
        let book_ids = [primary_id, secondary_id];
        let chapters = sqlx::query_as!(
            ResolutionChapterIdentity,
            r#"SELECT id, book_id, chapter_index AS "chapter_index!"
               FROM chapters
               WHERE book_id = ANY($1)
               ORDER BY book_id, chapter_index"#,
            book_ids.as_slice()
        )
        .fetch_all(&mut *self.transaction)
        .await?;
        let raw_mappings = sqlx::query_as!(
            ResolutionChapterMapping,
            r#"SELECT mapping.chapter_a_id, mapping.chapter_b_id,
                      mapping.chapter_a_index, mapping.chapter_b_index
               FROM duplicate_chapter_matches mapping
               WHERE mapping.pair_id = $1
                 AND mapping.match_type IN ('conservative', 'layout')
                 AND mapping.similarity >= 0.999999
                 AND EXISTS (
                   SELECT 1
                   FROM chapter_fingerprints chapter_a
                   JOIN chapter_fingerprints chapter_b
                     ON chapter_b.source_content_hash = chapter_a.source_content_hash
                   WHERE chapter_a.chapter_id = mapping.chapter_a_id
                     AND chapter_b.chapter_id = mapping.chapter_b_id
                 )
               ORDER BY mapping.chapter_a_index, mapping.chapter_b_index"#,
            pair_id
        )
        .fetch_all(&mut *self.transaction)
        .await?;
        let mappings: Vec<_> = raw_mappings
            .iter()
            .filter_map(|mapping| directed_mapping(mapping, direction))
            .filter(|mapping| {
                chapter_identity_matches(
                    &chapters,
                    secondary_id,
                    mapping.source_chapter_id,
                    mapping.source_chapter_index,
                ) && chapter_identity_matches(
                    &chapters,
                    primary_id,
                    mapping.target_chapter_id,
                    mapping.target_chapter_index,
                )
            })
            .collect();
        let progress_rows = sqlx::query_as!(
            ResolutionProgress,
            r#"SELECT id, user_id, book_id, chapter_id, chapter_index,
                      current_chapter, progress, scroll_position,
                      reading_time_secs, last_read_at
               FROM reading_progress
               WHERE book_id = ANY($1)
               ORDER BY id
               FOR UPDATE"#,
            book_ids.as_slice()
        )
        .fetch_all(&mut *self.transaction)
        .await?;

        let source_chapter_count = chapters
            .iter()
            .filter(|chapter| chapter.book_id == secondary_id)
            .count();
        let target_chapter_count = chapters
            .iter()
            .filter(|chapter| chapter.book_id == primary_id)
            .count();
        if source_chapter_count == 0 || target_chapter_count == 0 {
            return Ok(0);
        }

        let mut migrated = 0_u64;
        for old in progress_rows
            .iter()
            .filter(|progress| progress.book_id == secondary_id)
        {
            if progress_rows
                .iter()
                .filter(|candidate| {
                    candidate.book_id == secondary_id && candidate.user_id == old.user_id
                })
                .count()
                != 1
            {
                // Nullable legacy ownership can admit more than one row. Do
                // not guess which ambiguous source position should win.
                continue;
            }
            let Some(mapping) = unique_progress_mapping(old, &mappings) else {
                continue;
            };
            let Some(old_position) = comparable_chapter_position(
                old,
                mapping.source_chapter_index,
                source_chapter_count,
            ) else {
                continue;
            };
            let target_progress = full_book_progress(
                mapping.target_chapter_index,
                old_position,
                target_chapter_count,
            );
            let kept_rows: Vec<_> = progress_rows
                .iter()
                .filter(|kept| {
                    kept.book_id == primary_id && kept.user_id == old.user_id && kept.id != old.id
                })
                .collect();

            match kept_rows.as_slice() {
                [] => {
                    sqlx::query!(
                        r#"UPDATE reading_progress
                           SET book_id = $2, chapter_id = $3,
                               current_chapter = $4, chapter_index = $4,
                               cfi = NULL, progress = $5, scroll_position = $6,
                               updated_at = NOW()
                           WHERE id = $1"#,
                        old.id,
                        primary_id,
                        mapping.target_chapter_id,
                        mapping.target_chapter_index,
                        target_progress,
                        old_position
                    )
                    .execute(&mut *self.transaction)
                    .await?;
                    migrated = migrated.saturating_add(1);
                }
                [kept] => {
                    let Some(kept_chapter_index) =
                        progress_chapter_index(kept, primary_id, &chapters)
                    else {
                        continue;
                    };
                    let old_is_later = if mapping.target_chapter_index > kept_chapter_index {
                        true
                    } else if mapping.target_chapter_index < kept_chapter_index {
                        false
                    } else {
                        let Some(kept_position) = comparable_chapter_position(
                            kept,
                            kept_chapter_index,
                            target_chapter_count,
                        ) else {
                            continue;
                        };
                        old_position > kept_position
                    };
                    let reading_time_secs =
                        kept.reading_time_secs.saturating_add(old.reading_time_secs);
                    let last_read_at = kept.last_read_at.max(old.last_read_at);
                    if old_is_later {
                        sqlx::query!(
                            r#"UPDATE reading_progress
                               SET chapter_id = $2, current_chapter = $3,
                                   chapter_index = $3, cfi = NULL, progress = $4,
                                   scroll_position = $5, reading_time_secs = $6,
                                   last_read_at = $7, updated_at = NOW()
                               WHERE id = $1"#,
                            kept.id,
                            mapping.target_chapter_id,
                            mapping.target_chapter_index,
                            target_progress,
                            old_position,
                            reading_time_secs,
                            last_read_at
                        )
                        .execute(&mut *self.transaction)
                        .await?;
                    } else {
                        sqlx::query!(
                            r#"UPDATE reading_progress
                               SET reading_time_secs = $2, last_read_at = $3,
                                   updated_at = NOW()
                               WHERE id = $1"#,
                            kept.id,
                            reading_time_secs,
                            last_read_at
                        )
                        .execute(&mut *self.transaction)
                        .await?;
                    }
                    sqlx::query!("DELETE FROM reading_progress WHERE id = $1", old.id)
                        .execute(&mut *self.transaction)
                        .await?;
                    migrated = migrated.saturating_add(1);
                }
                _ => {}
            }
        }

        Ok(migrated)
    }

    /// Copies work-level organization links to the selected primary version.
    ///
    /// The secondary version remains a concrete, addressable edition, so its
    /// original links are never deleted. Edition-specific provenance
    /// (`series_books`, `book_persons`) and version-specific ratings are not
    /// copied at all.
    pub(crate) async fn copy_shared_book_links(
        &mut self,
        primary_id: Uuid,
        secondary_id: Uuid,
    ) -> Result<u64> {
        let mut copied = 0_u64;
        for relation in BookLinkRelation::ALL {
            copied = copied.saturating_add(
                self.copy_shared_link(relation, primary_id, secondary_id)
                    .await?,
            );
        }
        Ok(copied)
    }

    async fn copy_shared_link(
        &mut self,
        relation: BookLinkRelation,
        primary_id: Uuid,
        secondary_id: Uuid,
    ) -> Result<u64> {
        // SQL is selected from a closed enum; no table or column identifier is
        // supplied by a caller.
        let copied = sqlx::query(relation.insert_sql())
            .bind(primary_id)
            .bind(secondary_id)
            .execute(&mut *self.transaction)
            .await?
            .rows_affected();
        Ok(copied)
    }

    pub(crate) async fn mark_secondary_duplicate(
        &mut self,
        secondary_id: Uuid,
        primary_id: Uuid,
        pair_id: Uuid,
    ) -> Result<()> {
        sqlx::query!(
            r#"UPDATE books
               SET status = 'duplicate'::book_status,
                   metadata = metadata || jsonb_build_object(
                       'duplicate_of', $2::uuid::text,
                       'dedup_pair_id', $3::uuid::text,
                       'dedup_resolved_at', NOW()::text
                   ),
                   updated_at = NOW()
               WHERE id = $1"#,
            secondary_id,
            primary_id,
            pair_id
        )
        .execute(&mut *self.transaction)
        .await?;
        Ok(())
    }

    pub(crate) async fn mark_kept_version_confirmed(
        &mut self,
        pair_id: Uuid,
        resolution: &str,
        primary_id: Uuid,
        user_id: Uuid,
    ) -> Result<()> {
        sqlx::query!(
            r#"UPDATE duplicate_pairs
               SET review_status = 'confirmed', resolved = TRUE,
                   resolution = $2, recommended_primary_id = $3,
                   resolved_by = $4, resolved_at = NOW(), updated_at = NOW()
               WHERE id = $1"#,
            pair_id,
            resolution,
            primary_id,
            user_id
        )
        .execute(&mut *self.transaction)
        .await?;
        Ok(())
    }

    pub(crate) async fn enqueue_index_cleanup(
        &mut self,
        pair_id: Uuid,
        secondary_id: Uuid,
        primary_id: Uuid,
    ) -> Result<Uuid> {
        let task_id = Uuid::new_v4();
        let payload = serde_json::to_value(DedupIndexCleanupTask::new(
            pair_id,
            secondary_id,
            primary_id,
        ))
        .map_err(|error| Error::Internal(format!("serialize dedup index cleanup task: {error}")))?;
        sqlx::query!(
            r#"INSERT INTO tasks
               (id, kind, status, priority, payload, book_id, category,
                max_retries, scheduled_at)
               VALUES ($1, 'deduplicate'::task_kind, 'queued'::task_status,
                       '1'::task_priority, $2, $3, 'maintenance', 3, NOW())"#,
            task_id,
            payload,
            secondary_id
        )
        .execute(&mut *self.transaction)
        .await?;
        Ok(task_id)
    }

    pub(crate) async fn commit(self) -> Result<()> {
        self.transaction.commit().await?;
        Ok(())
    }
}

fn source_content_hash(chapters: &[&ResolutionChapterContent]) -> String {
    nova_ingest::dedup::source_content_hash(
        chapters
            .iter()
            .map(|chapter| (chapter.chapter_index, chapter.content.as_str())),
    )
    .to_hex()
}

fn directed_mapping(
    mapping: &ResolutionChapterMapping,
    direction: ChapterMappingDirection,
) -> Option<DirectedChapterMapping> {
    let (source_chapter_id, source_chapter_index, target_chapter_id, target_chapter_index) =
        match direction {
            ChapterMappingDirection::BookAToBookB => (
                mapping.chapter_a_id?,
                mapping.chapter_a_index?,
                mapping.chapter_b_id?,
                mapping.chapter_b_index?,
            ),
            ChapterMappingDirection::BookBToBookA => (
                mapping.chapter_b_id?,
                mapping.chapter_b_index?,
                mapping.chapter_a_id?,
                mapping.chapter_a_index?,
            ),
        };
    Some(DirectedChapterMapping {
        source_chapter_id,
        source_chapter_index,
        target_chapter_id,
        target_chapter_index,
    })
}

fn chapter_identity_matches(
    chapters: &[ResolutionChapterIdentity],
    book_id: Uuid,
    chapter_id: Uuid,
    chapter_index: i32,
) -> bool {
    chapters.iter().any(|chapter| {
        chapter.id == chapter_id
            && chapter.book_id == book_id
            && chapter.chapter_index == chapter_index
    })
}

fn unique_progress_mapping<'a>(
    progress: &ResolutionProgress,
    mappings: &'a [DirectedChapterMapping],
) -> Option<&'a DirectedChapterMapping> {
    let recorded_index = progress.chapter_index.unwrap_or(progress.current_chapter);
    let mut matching = mappings.iter().filter(|mapping| {
        mapping.source_chapter_index == recorded_index
            && progress
                .chapter_id
                .is_none_or(|chapter_id| chapter_id == mapping.source_chapter_id)
    });
    let mapping = matching.next()?;
    if matching.next().is_some() {
        return None;
    }
    Some(mapping)
}

fn progress_chapter_index(
    progress: &ResolutionProgress,
    book_id: Uuid,
    chapters: &[ResolutionChapterIdentity],
) -> Option<i32> {
    let chapter_index = progress.chapter_index.unwrap_or(progress.current_chapter);
    let matches = match progress.chapter_id {
        Some(chapter_id) => chapter_identity_matches(chapters, book_id, chapter_id, chapter_index),
        None => chapters
            .iter()
            .any(|chapter| chapter.book_id == book_id && chapter.chapter_index == chapter_index),
    };
    matches.then_some(chapter_index)
}

fn comparable_chapter_position(
    progress: &ResolutionProgress,
    chapter_index: i32,
    chapter_count: usize,
) -> Option<f64> {
    if let Some(position) = progress
        .scroll_position
        .filter(|position| position.is_finite() && (0.0..=1.0).contains(position))
    {
        return Some(position);
    }
    if chapter_count == 0 || chapter_index < 0 || !progress.progress.is_finite() {
        return None;
    }
    let position = progress.progress * chapter_count as f64 - f64::from(chapter_index);
    const ROUNDING_TOLERANCE: f64 = 0.000_001;
    if !(-ROUNDING_TOLERANCE..=1.0 + ROUNDING_TOLERANCE).contains(&position) {
        return None;
    }
    Some(position.clamp(0.0, 1.0))
}

fn full_book_progress(chapter_index: i32, chapter_position: f64, chapter_count: usize) -> f64 {
    (f64::from(chapter_index) + chapter_position) / chapter_count as f64
}

fn required_library_ids(book_library_ids: &[Option<Uuid>]) -> Result<Vec<Uuid>> {
    if book_library_ids.iter().any(Option::is_none) {
        return Err(Error::Forbidden);
    }

    let mut library_ids: Vec<Uuid> = book_library_ids.iter().flatten().copied().collect();
    library_ids.sort_unstable();
    library_ids.dedup();
    Ok(library_ids)
}

/// Link tables that may be consolidated onto the selected primary version.
/// Keeping the SQL behind a closed enum prevents dynamic identifier input.
#[derive(Debug, Clone, Copy)]
enum BookLinkRelation {
    Collection,
    Shelf,
    Tag,
}

impl BookLinkRelation {
    const ALL: [Self; 3] = [Self::Collection, Self::Shelf, Self::Tag];

    const fn insert_sql(self) -> &'static str {
        match self {
            Self::Collection => {
                r#"INSERT INTO collection_books (collection_id, book_id, sort_order, added_at)
                   SELECT collection_id, $1, sort_order, added_at
                   FROM collection_books WHERE book_id = $2
                   ON CONFLICT DO NOTHING"#
            }
            Self::Shelf => {
                r#"INSERT INTO shelf_books (shelf_id, book_id, sort_order, added_at)
                   SELECT shelf_id, $1, sort_order, added_at
                   FROM shelf_books WHERE book_id = $2
                   ON CONFLICT DO NOTHING"#
            }
            Self::Tag => {
                r#"INSERT INTO book_tags (book_id, tag_id)
                   SELECT $1, tag_id FROM book_tags WHERE book_id = $2
                   ON CONFLICT DO NOTHING"#
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn strict_redundancy_requires_byte_exact_source_content_on_both_sides() {
        let _write_guard = crate::dedup::DEDUP_DATABASE_TEST_LOCK.lock().await;
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let pool = sqlx::PgPool::connect(&database_url)
            .await
            .expect("connect to PostgreSQL for strict redundancy test");
        crate::migrations::run_database_migrations(&pool)
            .await
            .expect("apply migrations for strict redundancy test");

        let library_id = Uuid::now_v7();
        let mut book_ids = [Uuid::now_v7(), Uuid::now_v7()];
        book_ids.sort_unstable();
        let secondary_id = book_ids[0];
        let primary_id = book_ids[1];
        let secondary_chapter_id = Uuid::now_v7();
        let primary_chapter_id = Uuid::now_v7();
        let pair_id = Uuid::now_v7();
        let secondary_content = "Same   authored text\n";
        let primary_content = "Same authored text\n";

        assert_eq!(
            nova_ingest::dedup::normalize_conservative(secondary_content),
            nova_ingest::dedup::normalize_conservative(primary_content),
            "the fixture must differ only at the raw source layer"
        );
        assert_ne!(
            nova_ingest::dedup::sha256(secondary_content.as_bytes()),
            nova_ingest::dedup::sha256(primary_content.as_bytes()),
            "the fixture must retain distinct source bytes"
        );

        sqlx::query("INSERT INTO libraries (id, name, root_path) VALUES ($1, $2, $3)")
            .bind(library_id)
            .bind("Strict chapter redundancy test")
            .bind(format!("/tmp/nova-strict-redundancy-{library_id}"))
            .execute(&pool)
            .await
            .expect("insert strict redundancy library");
        for (book_id, status, label) in [
            (secondary_id, "duplicate", "secondary"),
            (primary_id, "ready", "primary"),
        ] {
            sqlx::query(
                r#"INSERT INTO books
                   (id, library_id, title, format, status, file_path, file_hash)
                   VALUES ($1, $2, $3, 'txt', $4::book_status, $5, $6)"#,
            )
            .bind(book_id)
            .bind(library_id)
            .bind(format!("Strict redundancy {label}"))
            .bind(status)
            .bind(format!("/tmp/{book_id}.txt"))
            .bind(format!("strict-redundancy-{book_id}"))
            .execute(&pool)
            .await
            .expect("insert strict redundancy book");
        }
        for (chapter_id, book_id, content) in [
            (secondary_chapter_id, secondary_id, secondary_content),
            (primary_chapter_id, primary_id, primary_content),
        ] {
            sqlx::query(
                r#"INSERT INTO chapters
                   (id, book_id, index, chapter_index, title, content)
                   VALUES ($1, $2, 0, 0, 'Chapter', $3)"#,
            )
            .bind(chapter_id)
            .bind(book_id)
            .bind(content)
            .execute(&pool)
            .await
            .expect("insert strict redundancy chapter");

            let conservative_hash = nova_ingest::dedup::sha256(
                nova_ingest::dedup::normalize_conservative(content).as_bytes(),
            )
            .to_hex();
            let layout_hash = nova_ingest::dedup::sha256(
                nova_ingest::dedup::normalize_layout(content).as_bytes(),
            )
            .to_hex();
            let source_hash = nova_ingest::dedup::sha256(content.as_bytes()).to_hex();
            sqlx::query(
                r#"INSERT INTO chapter_fingerprints
                   (chapter_id, book_id, chapter_index, normalization_version,
                    source_content_hash, conservative_hash, layout_hash,
                    char_count, informative, winnowing_count)
                   VALUES ($1, $2, 0, 1, $3, $4, $5, $6, TRUE, 1)"#,
            )
            .bind(chapter_id)
            .bind(book_id)
            .bind(source_hash)
            .bind(conservative_hash)
            .bind(layout_hash)
            .bind(i64::try_from(content.chars().count()).expect("chapter length fits in i64"))
            .execute(&pool)
            .await
            .expect("insert strict redundancy chapter fingerprint");
        }
        sqlx::query(
            r#"INSERT INTO duplicate_pairs
               (id, book_a_id, book_b_id, similarity, method, resolved, resolution,
                relation, review_status, confidence, recommended_primary_id,
                algorithm_version, stale)
               VALUES ($1, $2, $3, 1, 'conservative', TRUE, 'keep_b',
                       'exact_content', 'confirmed', 1, $4, $5, FALSE)"#,
        )
        .bind(pair_id)
        .bind(book_ids[0])
        .bind(book_ids[1])
        .bind(primary_id)
        .bind(i32::from(nova_ingest::dedup::DEDUP_ALGORITHM_VERSION))
        .execute(&pool)
        .await
        .expect("insert confirmed strict redundancy pair");
        sqlx::query(
            r#"INSERT INTO duplicate_chapter_matches
               (pair_id, chapter_a_id, chapter_b_id, chapter_a_index,
                chapter_b_index, match_type, similarity)
               VALUES ($1, $2, $3, 0, 0, 'conservative', 1)"#,
        )
        .bind(pair_id)
        .bind(secondary_chapter_id)
        .bind(primary_chapter_id)
        .execute(&pool)
        .await
        .expect("insert strict redundancy chapter mapping");

        let repository = PgDuplicateRepository::new(pool.clone());
        let mismatched_indexes = repository
            .verified_redundant_chapter_indexes(pair_id, secondary_id, primary_id)
            .await
            .expect("verify normalized-only chapter mapping");
        assert!(
            mismatched_indexes.is_empty(),
            "normalized equality must not authorize index cleanup"
        );
        let mismatched_policy: bool = sqlx::query_scalar("SELECT dedup_chapter_is_redundant($1)")
            .bind(secondary_chapter_id)
            .fetch_one(&pool)
            .await
            .expect("evaluate normalized-only chapter policy");
        assert!(
            !mismatched_policy,
            "normalized equality must not hide a chapter from search or RAG"
        );

        sqlx::query("UPDATE chapters SET content = $2 WHERE id = $1")
            .bind(primary_chapter_id)
            .bind(secondary_content)
            .execute(&pool)
            .await
            .expect("make primary chapter source byte-exact");
        let shared_source_hash = nova_ingest::dedup::sha256(secondary_content.as_bytes()).to_hex();
        let shared_conservative_hash = nova_ingest::dedup::sha256(
            nova_ingest::dedup::normalize_conservative(secondary_content).as_bytes(),
        )
        .to_hex();
        let shared_layout_hash = nova_ingest::dedup::sha256(
            nova_ingest::dedup::normalize_layout(secondary_content).as_bytes(),
        )
        .to_hex();
        sqlx::query(
            r#"INSERT INTO chapter_fingerprints
               (chapter_id, book_id, chapter_index, normalization_version,
                source_content_hash, conservative_hash, layout_hash,
                char_count, informative, winnowing_count)
               VALUES ($1, $2, 0, 1, $3, $4, $5, $6, TRUE, 1)"#,
        )
        .bind(primary_chapter_id)
        .bind(primary_id)
        .bind(&shared_source_hash)
        .bind(&shared_conservative_hash)
        .bind(&shared_layout_hash)
        .bind(i64::try_from(secondary_content.chars().count()).expect("chapter length fits in i64"))
        .execute(&pool)
        .await
        .expect("restore current primary fingerprint");
        sqlx::query(
            r#"UPDATE duplicate_pairs
               SET stale = FALSE, resolved = TRUE, review_status = 'confirmed',
                   recommended_primary_id = $2, updated_at = NOW()
               WHERE id = $1"#,
        )
        .bind(pair_id)
        .bind(primary_id)
        .execute(&pool)
        .await
        .expect("restore pair after byte-exact rescan");

        let exact_indexes = repository
            .verified_redundant_chapter_indexes(pair_id, secondary_id, primary_id)
            .await
            .expect("verify byte-exact chapter mapping");
        assert_eq!(exact_indexes, vec![0]);
        let exact_policy: bool = sqlx::query_scalar("SELECT dedup_chapter_is_redundant($1)")
            .bind(secondary_chapter_id)
            .fetch_one(&pool)
            .await
            .expect("evaluate byte-exact chapter policy");
        assert!(
            exact_policy,
            "current byte-exact source equality may hide the redundant chapter"
        );

        sqlx::query("DELETE FROM books WHERE id = ANY($1)")
            .bind(&book_ids)
            .execute(&pool)
            .await
            .expect("remove strict redundancy books");
        sqlx::query("DELETE FROM libraries WHERE id = $1")
            .bind(library_id)
            .execute(&pool)
            .await
            .expect("remove strict redundancy library");
    }

    #[test]
    fn mapping_direction_identifies_the_selected_primary_side() {
        assert!(!ChapterMappingDirection::BookAToBookB.primary_is_book_a());
        assert!(ChapterMappingDirection::BookBToBookA.primary_is_book_a());
    }

    #[test]
    fn shared_book_link_copy_is_closed_over_organization_relations() {
        assert_eq!(BookLinkRelation::ALL.len(), 3);
        for relation in BookLinkRelation::ALL {
            assert!(relation.insert_sql().contains("ON CONFLICT DO NOTHING"));
            assert!(!relation.insert_sql().contains("series_books"));
            assert!(!relation.insert_sql().contains("book_persons"));
            assert!(!relation.insert_sql().contains("book_ratings"));
            assert!(!relation.insert_sql().contains("DELETE FROM"));
        }
    }

    #[test]
    fn required_library_ids_rejects_unscoped_books_and_deduplicates_libraries() {
        let library_a = Uuid::new_v4();
        let library_b = Uuid::new_v4();
        let ids = required_library_ids(&[Some(library_b), Some(library_a), Some(library_b)])
            .expect("scoped books should produce library ids");
        assert_eq!(
            ids,
            vec![library_a.min(library_b), library_a.max(library_b)]
        );

        assert!(matches!(
            required_library_ids(&[Some(library_a), None]),
            Err(Error::Forbidden)
        ));
    }
}
