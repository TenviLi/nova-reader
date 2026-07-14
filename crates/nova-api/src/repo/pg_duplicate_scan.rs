use std::collections::HashSet;

use nova_core::{
    domain::{
        dedup::{DedupScanPhase, DedupScanTask, DuplicatePairEvidence, DuplicateRelation},
        task::TaskExecutionLockMode,
    },
    Error, Result,
};
use serde_json::Value;
use sqlx::{pool::PoolConnection, postgres::PgRow, Postgres, Row, Transaction};
use uuid::Uuid;

use super::pg_duplicate::PgDuplicateRepository;
use crate::task_queue::INTERRUPTED_TASK_RECOVERY_MESSAGE;

const DEDUP_SCAN_BARRIER_LOCK_NAMESPACE: i32 = 0x4e4f_5641;
const DEDUP_SCAN_LIBRARY_LOCK_NAMESPACE: i32 = 0x4445_4455;
const DEDUP_SCAN_BARRIER_RESOURCE: &str = "dedup:scan:barrier";
const DEDUP_SCAN_LIBRARY_RESOURCE_PREFIX: &str = "dedup:scan:library";

/// Holds PostgreSQL session-level advisory locks for one scan attempt.
///
/// The connection is deliberately closed on drop: returning a connection with
/// a session lock to the pool would leak the lock, while closing also releases
/// it when the worker future is cancelled.
#[derive(Debug)]
pub(crate) struct DuplicateScanExecutionLock {
    _connection: PoolConnection<Postgres>,
}

#[derive(Debug, Clone)]
pub(crate) struct EnqueueDuplicateScan {
    pub(crate) library_id: Option<Uuid>,
    pub(crate) requested_by: Uuid,
    pub(crate) include_semantic: bool,
    pub(crate) algorithm_version: i32,
    pub(crate) target_book_ids: Option<Vec<Uuid>>,
}

#[derive(Debug, Clone)]
pub(crate) struct ScanBookRecord {
    pub(crate) id: Uuid,
    pub(crate) file_hash: String,
    pub(crate) format: String,
    pub(crate) file_size_bytes: i64,
    pub(crate) chapter_count: i32,
    pub(crate) word_count: i64,
    pub(crate) metadata_quality: i32,
}

#[derive(Debug, Clone)]
pub(crate) struct ScanChapterRecord {
    pub(crate) id: Uuid,
    pub(crate) chapter_index: i32,
    pub(crate) content: String,
}

#[derive(Debug)]
pub(crate) struct BoundedScanChapterRecords {
    pub(crate) source_bytes: u64,
    pub(crate) chapter_count: u64,
    pub(crate) chapters: Vec<ScanChapterRecord>,
}

#[derive(Debug)]
pub(crate) struct StoredBookFingerprintRecord {
    pub(crate) book_id: Uuid,
    pub(crate) source_content_hash: String,
    pub(crate) conservative_hash: String,
    pub(crate) layout_hash: String,
    pub(crate) char_count: i64,
    pub(crate) text_integrity_bps: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SemanticFreshnessSnapshotRecord {
    pub(crate) book_id: Uuid,
    pub(crate) source_content_hash: String,
}

#[derive(Debug)]
pub(crate) struct StoredChapterFingerprintRecord {
    pub(crate) book_id: Uuid,
    pub(crate) id: Uuid,
    pub(crate) chapter_index: i32,
    pub(crate) conservative_hash: String,
    pub(crate) layout_hash: String,
    pub(crate) char_count: i64,
}

#[derive(Debug)]
pub(crate) struct StoredPassageFingerprintRecord {
    pub(crate) book_id: Uuid,
    pub(crate) chapter_id: Uuid,
    pub(crate) chapter_index: i32,
    pub(crate) fingerprint_hash: i64,
    pub(crate) position: i32,
}

#[derive(Debug, Default)]
pub(crate) struct CachedFingerprintRecords {
    pub(crate) books: Vec<StoredBookFingerprintRecord>,
    pub(crate) chapters: Vec<StoredChapterFingerprintRecord>,
    pub(crate) passages: Vec<StoredPassageFingerprintRecord>,
}

#[derive(Debug, Clone)]
pub(crate) struct ChapterFingerprintWrite {
    pub(crate) chapter_id: Uuid,
    pub(crate) chapter_index: i32,
    pub(crate) normalization_version: i32,
    pub(crate) source_content_hash: String,
    pub(crate) conservative_hash: String,
    pub(crate) layout_hash: String,
    pub(crate) char_count: i64,
    pub(crate) informative: bool,
    pub(crate) winnowing_count: i32,
}

#[derive(Debug, Clone)]
pub(crate) struct PassageFingerprintWrite {
    pub(crate) chapter_id: Uuid,
    pub(crate) normalization_version: i32,
    pub(crate) fingerprint_hash: i64,
    pub(crate) position: i32,
    pub(crate) span_length: i32,
}

#[derive(Debug, Clone)]
pub(crate) struct BookFingerprintWrite {
    pub(crate) book_id: Uuid,
    pub(crate) expected_source_bytes: u64,
    pub(crate) expected_chapter_count: u64,
    pub(crate) normalization_version: i32,
    pub(crate) layout_normalization_version: i32,
    pub(crate) algorithm_version: i32,
    pub(crate) source_content_hash: String,
    pub(crate) conservative_hash: String,
    pub(crate) layout_hash: String,
    pub(crate) chapter_count: i32,
    pub(crate) informative_chapter_count: i32,
    pub(crate) char_count: i64,
    pub(crate) text_integrity_bps: i32,
    pub(crate) chapters: Vec<ChapterFingerprintWrite>,
    pub(crate) passages: Vec<PassageFingerprintWrite>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BookPairRecord {
    pub(crate) a: Uuid,
    pub(crate) b: Uuid,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CountedBookPairRecord {
    pub(crate) a: Uuid,
    pub(crate) b: Uuid,
    pub(crate) shared: i64,
}

#[derive(Debug, Default)]
pub(crate) struct DeterministicCandidateRecords {
    pub(crate) exact_file: Vec<BookPairRecord>,
    pub(crate) exact_content: Vec<BookPairRecord>,
    pub(crate) shared_chapters: Vec<CountedBookPairRecord>,
    pub(crate) shared_passages: Vec<CountedBookPairRecord>,
}

fn decode_bounded_candidate_rows(
    rows: Vec<PgRow>,
    kind: &str,
    max_pair_contributions: i64,
) -> Result<Vec<CountedBookPairRecord>> {
    let pair_contributions = rows
        .first()
        .ok_or_else(|| Error::Internal(format!("bounded {kind} query returned no stats row")))?
        .try_get::<i64, _>("pair_contributions")?;
    if pair_contributions < 0 {
        return Err(Error::Internal(format!(
            "invalid {kind} pair contribution count: {pair_contributions}"
        )));
    }
    if pair_contributions > max_pair_contributions {
        return Err(Error::Validation(format!(
            "duplicate {kind} pair contribution budget exceeded: actual={pair_contributions}, limit={max_pair_contributions}"
        )));
    }

    let mut candidates = Vec::with_capacity(rows.len());
    for row in rows {
        let Some(a) = row.try_get::<Option<Uuid>, _>("a")? else {
            continue;
        };
        let b = row
            .try_get::<Option<Uuid>, _>("b")?
            .ok_or_else(|| Error::Internal(format!("bounded {kind} row has no book b")))?;
        let shared = row
            .try_get::<Option<i64>, _>("shared")?
            .ok_or_else(|| Error::Internal(format!("bounded {kind} row has no shared count")))?;
        candidates.push(CountedBookPairRecord { a, b, shared });
    }
    Ok(candidates)
}

#[derive(Debug)]
pub(crate) struct ChapterContentRecord {
    pub(crate) id: Uuid,
    pub(crate) content: String,
}

#[derive(Debug)]
pub(crate) struct BoundedChapterContentRecords {
    pub(crate) source_bytes: u64,
    pub(crate) chapters: Vec<ChapterContentRecord>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ScanPairCounts {
    pub(crate) found: i32,
    pub(crate) exact: i32,
    pub(crate) contained: i32,
    pub(crate) semantic: i32,
}

#[derive(Debug)]
pub(crate) struct DuplicateChapterMatchWrite {
    pub(crate) chapter_a_id: Option<Uuid>,
    pub(crate) chapter_b_id: Option<Uuid>,
    pub(crate) chapter_a_index: i32,
    pub(crate) chapter_b_index: i32,
    pub(crate) match_type: String,
    pub(crate) similarity: f64,
    pub(crate) shared_fingerprints: i32,
    pub(crate) alignment_group: Option<i32>,
    pub(crate) segment_ordinal: Option<i32>,
    pub(crate) chapter_a_start: Option<i32>,
    pub(crate) chapter_a_end: Option<i32>,
    pub(crate) chapter_b_start: Option<i32>,
    pub(crate) chapter_b_end: Option<i32>,
    pub(crate) matched_chars: i32,
}

#[derive(Debug)]
pub(crate) struct DuplicatePairWrite {
    pub(crate) book_a_id: Uuid,
    pub(crate) book_b_id: Uuid,
    pub(crate) method: String,
    pub(crate) relation: DuplicateRelation,
    pub(crate) confidence: f64,
    pub(crate) shared_chapters: i32,
    pub(crate) coverage_a: f64,
    pub(crate) coverage_b: f64,
    pub(crate) character_coverage_a: f64,
    pub(crate) character_coverage_b: f64,
    pub(crate) longest_contiguous_run: i32,
    pub(crate) order_score: f64,
    pub(crate) contained_book_id: Option<Uuid>,
    pub(crate) recommended_primary_id: Option<Uuid>,
    pub(crate) semantic_score: Option<f64>,
    pub(crate) algorithm_version: i32,
    pub(crate) evidence: DuplicatePairEvidence,
    pub(crate) chapter_matches: Vec<DuplicateChapterMatchWrite>,
}

#[derive(Debug, Clone)]
pub(crate) struct DuplicateBookPublicationSnapshot {
    pub(crate) book_id: Uuid,
    pub(crate) file_hash: String,
    pub(crate) source_content_hash: String,
    pub(crate) conservative_hash: String,
    pub(crate) layout_hash: String,
    pub(crate) algorithm_version: i32,
}

#[derive(Debug)]
pub(crate) struct PublishDuplicateScan {
    pub(crate) scan_id: Uuid,
    pub(crate) scope_book_ids: Vec<Uuid>,
    pub(crate) affected_scope_book_ids: Vec<Uuid>,
    pub(crate) book_snapshots: Vec<DuplicateBookPublicationSnapshot>,
    pub(crate) pairs: Vec<DuplicatePairWrite>,
    pub(crate) counts: ScanPairCounts,
}

#[derive(Debug, Clone)]
pub(crate) struct FinishDuplicateScan {
    pub(crate) scan_id: Uuid,
    pub(crate) scope_book_ids: Vec<Uuid>,
    pub(crate) affected_scope_book_ids: Vec<Uuid>,
    pub(crate) counts: ScanPairCounts,
}

impl PgDuplicateRepository {
    /// Reconcile the duplicate-scan projection after generic queue recovery.
    ///
    /// The recovery marker makes this operation durable and idempotent: if a
    /// transient database failure occurs after tasks were requeued, startup
    /// can retry this query until it succeeds without losing the task IDs.
    pub(crate) async fn synchronize_recovered_scan_tasks(&self) -> Result<u64> {
        let result = sqlx::query(
            r#"UPDATE dedup_scan_runs AS scan
               SET status = 'queued',
                   progress_message = $1,
                   error_message = NULL,
                   completed_at = NULL,
                   updated_at = NOW()
               FROM tasks AS task
               WHERE scan.task_id = task.id
                 AND task.kind = 'deduplicate'::task_kind
                 AND task.status = 'queued'::task_status
                 AND task.error_message = $2
                 AND scan.status IN ('queued', 'running', 'failed')
                 AND (
                     scan.status <> 'queued'
                     OR scan.progress_message IS DISTINCT FROM $1
                     OR scan.error_message IS NOT NULL
                     OR scan.completed_at IS NOT NULL
                 )"#,
        )
        .bind(DedupScanPhase::Recovering.as_str())
        .bind(INTERRUPTED_TASK_RECOVERY_MESSAGE)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// Atomically coalesces an enqueue request with a queued scan or creates a
    /// persistent task and scan run. An active full scan can satisfy another
    /// full request; incremental events are retained in one queued follow-up.
    pub(crate) async fn enqueue_scan(&self, mut input: EnqueueDuplicateScan) -> Result<Uuid> {
        if let Some(targets) = &mut input.target_book_ids {
            targets.sort_unstable();
            targets.dedup();
        }

        let mut tx = self.pool.begin().await?;
        sqlx::query_scalar!(
            r#"SELECT 1 AS "locked!"
               FROM pg_advisory_xact_lock(
                 hashtextextended(COALESCE($1::uuid::text, '__all_libraries__'), 0)
               )"#,
            input.library_id
        )
        .fetch_one(&mut *tx)
        .await?;

        let queued = sqlx::query!(
            r#"SELECT scan.id, task.id AS "task_id!", task.payload,
                      scan.include_semantic
               FROM dedup_scan_runs scan
               JOIN tasks task ON task.id = scan.task_id
               WHERE scan.library_id IS NOT DISTINCT FROM $1
                 AND scan.status = 'queued'
                 AND task.status = 'queued'::task_status
               ORDER BY scan.created_at DESC
               LIMIT 1
               FOR UPDATE OF scan, task"#,
            input.library_id
        )
        .fetch_optional(&mut *tx)
        .await?;

        if let Some(queued) = queued {
            let existing_task = decode_duplicate_scan_task(queued.payload)?;
            let existing_targets = existing_task.target_book_ids;
            let merged_targets =
                merge_scan_target_book_ids(existing_targets, input.target_book_ids.take());
            let merged_semantic = queued.include_semantic || input.include_semantic;
            let books_total =
                count_scan_books(&mut tx, input.library_id, merged_targets.as_deref()).await?;
            let payload = encode_duplicate_scan_task(&duplicate_scan_payload(
                queued.id,
                input.library_id,
                merged_semantic,
                merged_targets.as_deref(),
            ))?;
            sqlx::query!(
                "UPDATE tasks SET payload = $2 WHERE id = $1",
                queued.task_id,
                payload
            )
            .execute(&mut *tx)
            .await?;
            sqlx::query!(
                r#"UPDATE dedup_scan_runs
                   SET include_semantic = $2, books_total = $3, updated_at = NOW()
                   WHERE id = $1"#,
                queued.id,
                merged_semantic,
                i32::try_from(books_total).unwrap_or(i32::MAX)
            )
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            return Ok(queued.id);
        }

        let running = sqlx::query!(
            r#"SELECT scan.id, task.payload, scan.include_semantic
               FROM dedup_scan_runs scan
               JOIN tasks task ON task.id = scan.task_id
               WHERE scan.library_id IS NOT DISTINCT FROM $1
                 AND scan.status = 'running'
                 AND task.status = 'running'::task_status
               ORDER BY scan.created_at DESC LIMIT 1"#,
            input.library_id
        )
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(running) = running {
            let running_is_full = decode_duplicate_scan_task(running.payload)?
                .target_book_ids
                .is_none();
            if input.target_book_ids.is_none()
                && running_is_full
                && (!input.include_semantic || running.include_semantic)
            {
                tx.commit().await?;
                return Ok(running.id);
            }
        }

        let books_total =
            count_scan_books(&mut tx, input.library_id, input.target_book_ids.as_deref()).await?;
        let scan_id = Uuid::now_v7();
        let task_id = Uuid::new_v4();
        let payload = encode_duplicate_scan_task(&duplicate_scan_payload(
            scan_id,
            input.library_id,
            input.include_semantic,
            input.target_book_ids.as_deref(),
        ))?;
        sqlx::query!(
            r#"INSERT INTO tasks
               (id, kind, status, priority, payload, category, max_retries, scheduled_at)
               VALUES ($1, 'deduplicate'::task_kind, 'queued'::task_status,
                       '1'::task_priority, $2, 'preprocess', 3, NOW())"#,
            task_id,
            payload
        )
        .execute(&mut *tx)
        .await?;
        for (resource_key, mode) in duplicate_scan_task_resources(input.library_id) {
            sqlx::query!(
                r#"INSERT INTO task_execution_locks (task_id, resource_key, mode)
                   VALUES ($1, $2, $3)"#,
                task_id,
                resource_key,
                mode.as_str()
            )
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query!(
            r#"INSERT INTO dedup_scan_runs
               (id, library_id, requested_by, task_id, include_semantic,
                algorithm_version, books_total)
               VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
            scan_id,
            input.library_id,
            input.requested_by,
            task_id,
            input.include_semantic,
            input.algorithm_version,
            i32::try_from(books_total).unwrap_or(i32::MAX)
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(scan_id)
    }

    /// Serializes scans whose publication scopes overlap.
    ///
    /// A global scan takes the barrier exclusively. A library scan takes the
    /// barrier in shared mode plus its library lock exclusively. This lets
    /// different libraries run in parallel while global/same-library scans
    /// cannot interleave their atomic publication steps.
    pub(crate) async fn acquire_scan_execution_lock(
        &self,
        scan_id: Uuid,
        task_id: Uuid,
        payload_library_id: Option<Uuid>,
    ) -> Result<DuplicateScanExecutionLock> {
        let mut connection = self.pool.acquire().await?;
        connection.close_on_drop();

        let scan = sqlx::query!(
            r#"SELECT library_id
               FROM dedup_scan_runs
               WHERE id = $1 AND task_id = $2"#,
            scan_id,
            task_id
        )
        .fetch_optional(&mut *connection)
        .await?
        .ok_or_else(|| Error::NotFound {
            entity: "dedup scan task",
            id: format!("{scan_id}/{task_id}"),
        })?;
        if scan.library_id != payload_library_id {
            return Err(Error::Validation(format!(
                "dedup scan {scan_id} library scope does not match its task payload"
            )));
        }

        match scan.library_id {
            None => {
                sqlx::query!(
                    "SELECT pg_advisory_lock($1::integer, 0)",
                    DEDUP_SCAN_BARRIER_LOCK_NAMESPACE
                )
                .execute(&mut *connection)
                .await?;
            }
            Some(library_id) => {
                sqlx::query!(
                    "SELECT pg_advisory_lock_shared($1::integer, 0)",
                    DEDUP_SCAN_BARRIER_LOCK_NAMESPACE
                )
                .execute(&mut *connection)
                .await?;
                sqlx::query!(
                    "SELECT pg_advisory_lock($1::integer, hashtext($2::uuid::text))",
                    DEDUP_SCAN_LIBRARY_LOCK_NAMESPACE,
                    library_id
                )
                .execute(&mut *connection)
                .await?;
            }
        }

        Ok(DuplicateScanExecutionLock {
            _connection: connection,
        })
    }

    pub(crate) async fn mark_scan_running(
        &self,
        scan_id: Uuid,
        algorithm_version: i32,
    ) -> Result<()> {
        sqlx::query!(
            r#"UPDATE dedup_scan_runs
               SET status = 'running', started_at = COALESCE(started_at, NOW()),
                   algorithm_version = $2,
                   error_message = NULL, completed_at = NULL, updated_at = NOW()
               WHERE id = $1"#,
            scan_id,
            algorithm_version
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn scan_books(&self, library_id: Option<Uuid>) -> Result<Vec<ScanBookRecord>> {
        let rows = sqlx::query_as!(
            ScanBookRecord,
            r#"SELECT b.id, b.file_hash, b.format::text AS "format!", b.file_size_bytes,
                      b.chapter_count, b.word_count,
                      ((CASE WHEN NULLIF(BTRIM(b.author), '') IS NULL THEN 0 ELSE 1 END)
                       + (CASE WHEN NULLIF(BTRIM(b.description), '') IS NULL THEN 0 ELSE 1 END)
                       + (CASE WHEN NULLIF(BTRIM(b.cover_path), '') IS NULL THEN 0 ELSE 1 END)
                       + (CASE WHEN b.language::text = 'unknown' THEN 0 ELSE 1 END)
                       + (CASE WHEN NULLIF(BTRIM(b.series_name), '') IS NULL THEN 0 ELSE 1 END))::integer
                        AS "metadata_quality!"
               FROM books b
               WHERE b.status NOT IN ('archived', 'duplicate')
                 AND ($1::uuid IS NULL OR b.library_id = $1)
               ORDER BY b.id"#,
            library_id
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// All books in the requested publication scope, including versions that
    /// are no longer scannable. Inactive IDs are required to stale pairs that
    /// were discovered before a book became archived or duplicate.
    pub(crate) async fn scan_scope_book_ids(&self, library_id: Option<Uuid>) -> Result<Vec<Uuid>> {
        let ids = sqlx::query_scalar!(
            r#"SELECT id AS "id!"
               FROM books
               WHERE $1::uuid IS NULL OR library_id = $1
               ORDER BY id"#,
            library_id
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(ids)
    }

    pub(crate) async fn valid_cached_book_ids(
        &self,
        book_ids: &[Uuid],
        algorithm_version: i32,
        conservative_normalization_version: i32,
        layout_normalization_version: i32,
    ) -> Result<HashSet<Uuid>> {
        if book_ids.is_empty() {
            return Ok(HashSet::new());
        }
        let ids = sqlx::query_scalar!(
            r#"SELECT bf.book_id AS "book_id!"
               FROM book_fingerprints bf
               WHERE bf.book_id = ANY($1)
                 AND bf.algorithm_version = $2
                 AND bf.normalization_version = $3
                 AND bf.layout_normalization_version = $4
                 AND bf.text_integrity_bps IS NOT NULL
                 AND bf.chapter_count = (
                     SELECT COUNT(*)::integer FROM chapters c WHERE c.book_id = bf.book_id
                 )
                 AND bf.chapter_count = (
                     SELECT COUNT(*)::integer
                     FROM chapter_fingerprints cf
                     WHERE cf.book_id = bf.book_id AND cf.normalization_version = $3
                 )
                 AND (
                     SELECT COALESCE(SUM(cf.winnowing_count), 0)::bigint
                     FROM chapter_fingerprints cf WHERE cf.book_id = bf.book_id
                 ) = (
                     SELECT COUNT(*)::bigint
                     FROM passage_fingerprints pf
                     WHERE pf.book_id = bf.book_id AND pf.normalization_version = $4
                 )"#,
            book_ids,
            algorithm_version,
            conservative_normalization_version,
            layout_normalization_version
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(ids.into_iter().collect())
    }

    /// Loads the persisted source snapshot that current semantic vectors must
    /// match. Version filtering lives at the repository seam so callers cannot
    /// accidentally compare Qdrant payloads with a stale fingerprint contract.
    pub(crate) async fn current_semantic_freshness_snapshots(
        &self,
        book_ids: &[Uuid],
    ) -> Result<Vec<SemanticFreshnessSnapshotRecord>> {
        let rows: Vec<(Uuid, String)> = sqlx::query_as(
            r#"SELECT book_id, source_content_hash
               FROM book_fingerprints
               WHERE book_id = ANY($1)
                 AND algorithm_version = $2
                 AND normalization_version = $3
                 AND layout_normalization_version = $4"#,
        )
        .bind(book_ids)
        .bind(i32::from(nova_ingest::dedup::DEDUP_ALGORITHM_VERSION))
        .bind(i32::from(
            nova_ingest::dedup::CONSERVATIVE_NORMALIZATION_VERSION,
        ))
        .bind(i32::from(nova_ingest::dedup::LAYOUT_NORMALIZATION_VERSION))
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(book_id, source_content_hash)| SemanticFreshnessSnapshotRecord {
                    book_id,
                    source_content_hash,
                },
            )
            .collect())
    }

    pub(crate) async fn cached_fingerprints(
        &self,
        cached_ids: &[Uuid],
        passage_cached_ids: &[Uuid],
        conservative_normalization_version: i32,
        layout_normalization_version: i32,
    ) -> Result<CachedFingerprintRecords> {
        if cached_ids.is_empty() {
            return Ok(CachedFingerprintRecords::default());
        }
        let books = sqlx::query_as!(
            StoredBookFingerprintRecord,
            r#"SELECT book_id, source_content_hash, conservative_hash, layout_hash, char_count,
                      text_integrity_bps AS "text_integrity_bps!"
               FROM book_fingerprints WHERE book_id = ANY($1)"#,
            cached_ids
        )
        .fetch_all(&self.pool)
        .await?;
        let chapters = sqlx::query_as!(
            StoredChapterFingerprintRecord,
            r#"SELECT book_id, chapter_id AS id, chapter_index AS "chapter_index!", conservative_hash,
                      layout_hash, char_count
               FROM chapter_fingerprints
               WHERE book_id = ANY($1) AND normalization_version = $2
               ORDER BY book_id, chapter_index"#,
            cached_ids,
            conservative_normalization_version
        )
        .fetch_all(&self.pool)
        .await?;
        let passages = sqlx::query_as!(
            StoredPassageFingerprintRecord,
            r#"SELECT p.book_id, p.chapter_id, c.chapter_index AS "chapter_index!",
                      p.fingerprint_hash, p.position
               FROM passage_fingerprints p
               JOIN chapters c ON c.id = p.chapter_id
               WHERE p.book_id = ANY($1) AND p.normalization_version = $2
               ORDER BY p.book_id, c.chapter_index, p.position"#,
            passage_cached_ids,
            layout_normalization_version
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(CachedFingerprintRecords {
            books,
            chapters,
            passages,
        })
    }

    pub(crate) async fn passage_fingerprint_count(&self, book_ids: &[Uuid]) -> Result<u64> {
        if book_ids.is_empty() {
            return Ok(0);
        }
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)::bigint FROM passage_fingerprints WHERE book_id = ANY($1)",
        )
        .bind(book_ids)
        .fetch_one(&self.pool)
        .await?;
        u64::try_from(count)
            .map_err(|_| Error::Internal(format!("invalid passage fingerprint count: {count}")))
    }

    pub(crate) async fn chapter_fingerprint_count(&self, book_ids: &[Uuid]) -> Result<u64> {
        if book_ids.is_empty() {
            return Ok(0);
        }
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)::bigint FROM chapter_fingerprints WHERE book_id = ANY($1)",
        )
        .bind(book_ids)
        .fetch_one(&self.pool)
        .await?;
        u64::try_from(count)
            .map_err(|_| Error::Internal(format!("invalid chapter fingerprint count: {count}")))
    }

    pub(crate) async fn scan_chapters_bounded(
        &self,
        book_id: Uuid,
        max_source_bytes: u64,
        max_chapters: u64,
    ) -> Result<BoundedScanChapterRecords> {
        let max_source_bytes_i64 = i64::try_from(max_source_bytes).unwrap_or(i64::MAX);
        let max_chapters_i64 = i64::try_from(max_chapters).unwrap_or(i64::MAX);
        let rows = sqlx::query(
            r#"WITH stats AS MATERIALIZED (
                   SELECT COALESCE(SUM(octet_length(content)), 0)::bigint AS source_bytes,
                          COUNT(*)::bigint AS chapter_count
                   FROM chapters
                   WHERE book_id = $1
               )
               SELECT chapter.id, chapter.chapter_index, chapter.content,
                      stats.source_bytes, stats.chapter_count
               FROM stats
               LEFT JOIN chapters AS chapter
                 ON chapter.book_id = $1
                AND stats.source_bytes <= $2
                AND stats.chapter_count <= $3
               ORDER BY chapter.chapter_index NULLS LAST, chapter.id NULLS LAST"#,
        )
        .bind(book_id)
        .bind(max_source_bytes_i64)
        .bind(max_chapters_i64)
        .fetch_all(&self.pool)
        .await?;
        let source_bytes_i64 = rows
            .first()
            .ok_or_else(|| Error::Internal("bounded chapter query returned no stats row".into()))?
            .try_get::<i64, _>("source_bytes")?;
        let source_bytes = u64::try_from(source_bytes_i64).map_err(|_| {
            Error::Internal(format!("invalid source text size: {source_bytes_i64}"))
        })?;
        if source_bytes > max_source_bytes {
            return Err(Error::Validation(format!(
                "duplicate source text per-book budget exceeded for {book_id}: actual_bytes={source_bytes}, limit_bytes={max_source_bytes}"
            )));
        }
        let chapter_count_i64 = rows
            .first()
            .ok_or_else(|| Error::Internal("bounded chapter query returned no stats row".into()))?
            .try_get::<i64, _>("chapter_count")?;
        let chapter_count = u64::try_from(chapter_count_i64)
            .map_err(|_| Error::Internal(format!("invalid chapter count: {chapter_count_i64}")))?;
        if chapter_count > max_chapters {
            return Err(Error::Validation(format!(
                "duplicate chapter row per-book budget exceeded for {book_id}: actual={chapter_count}, limit={max_chapters}"
            )));
        }
        let mut chapters = Vec::with_capacity(rows.len());
        for row in rows {
            let Some(id) = row.try_get::<Option<Uuid>, _>("id")? else {
                continue;
            };
            let chapter_index =
                row.try_get::<Option<i32>, _>("chapter_index")?
                    .ok_or_else(|| {
                        Error::Internal(format!("bounded chapter row {id} has no chapter_index"))
                    })?;
            let content = row
                .try_get::<Option<String>, _>("content")?
                .ok_or_else(|| {
                    Error::Internal(format!("bounded chapter row {id} has no content"))
                })?;
            chapters.push(ScanChapterRecord {
                id,
                chapter_index,
                content,
            });
        }
        Ok(BoundedScanChapterRecords {
            source_bytes,
            chapter_count,
            chapters,
        })
    }

    /// Replaces the book, chapter, and passage fingerprint cache as one unit.
    pub(crate) async fn replace_fingerprints(&self, input: BookFingerprintWrite) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        lock_dedup_publication_books(&mut tx, &[input.book_id]).await?;

        let current_stats = sqlx::query(
            r#"SELECT COALESCE(SUM(octet_length(content)), 0)::bigint AS source_bytes,
                      COUNT(*)::bigint AS chapter_count
               FROM chapters
               WHERE book_id = $1"#,
        )
        .bind(input.book_id)
        .fetch_one(&mut *tx)
        .await?;
        let current_source_bytes_i64 = current_stats.try_get::<i64, _>("source_bytes")?;
        let current_source_bytes = u64::try_from(current_source_bytes_i64).map_err(|_| {
            Error::Internal(format!(
                "invalid source text size for {}: {current_source_bytes_i64}",
                input.book_id
            ))
        })?;
        if current_source_bytes != input.expected_source_bytes {
            return Err(Error::Internal(format!(
                "book {} source size changed while duplicate fingerprints were computed: expected_bytes={}, actual_bytes={current_source_bytes}; retry scan",
                input.book_id, input.expected_source_bytes
            )));
        }
        let current_chapter_count_i64 = current_stats.try_get::<i64, _>("chapter_count")?;
        let current_chapter_count = u64::try_from(current_chapter_count_i64).map_err(|_| {
            Error::Internal(format!(
                "invalid chapter count for {}: {current_chapter_count_i64}",
                input.book_id
            ))
        })?;
        if current_chapter_count != input.expected_chapter_count {
            return Err(Error::Internal(format!(
                "book {} chapter count changed while duplicate fingerprints were computed: expected={}, actual={current_chapter_count}; retry scan",
                input.book_id, input.expected_chapter_count
            )));
        }

        let current_rows = sqlx::query(
            r#"SELECT id, chapter_index,
                      encode(sha256(convert_to(content, 'UTF8')), 'hex') AS source_content_hash
               FROM chapters
               WHERE book_id = $1
               ORDER BY chapter_index, id"#,
        )
        .bind(input.book_id)
        .fetch_all(&mut *tx)
        .await?;
        let mut expected_chapters: Vec<_> = input.chapters.iter().collect();
        expected_chapters
            .sort_unstable_by_key(|chapter| (chapter.chapter_index, chapter.chapter_id));
        if current_rows.len() != expected_chapters.len() {
            return Err(Error::Internal(format!(
                "book {} chapter count changed while duplicate fingerprints were computed: expected={}, actual={}; retry scan",
                input.book_id,
                expected_chapters.len(),
                current_rows.len()
            )));
        }
        for (row, expected) in current_rows.iter().zip(expected_chapters) {
            let current_id = row.try_get::<Uuid, _>("id")?;
            let current_index = row.try_get::<i32, _>("chapter_index")?;
            let current_hash = row.try_get::<String, _>("source_content_hash")?;
            if current_id != expected.chapter_id
                || current_index != expected.chapter_index
                || current_hash != expected.source_content_hash
            {
                return Err(Error::Internal(format!(
                    "book {} chapter snapshot changed while duplicate fingerprints were computed; retry scan",
                    input.book_id
                )));
            }
        }
        sqlx::query!(
            r#"INSERT INTO book_fingerprints
               (book_id, normalization_version, layout_normalization_version,
                algorithm_version, source_content_hash,
                conservative_hash, layout_hash, chapter_count, informative_chapter_count,
                char_count, text_integrity_bps, computed_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, NOW())
               ON CONFLICT (book_id) DO UPDATE SET
                 normalization_version = EXCLUDED.normalization_version,
                 layout_normalization_version = EXCLUDED.layout_normalization_version,
                 algorithm_version = EXCLUDED.algorithm_version,
                 source_content_hash = EXCLUDED.source_content_hash,
                 conservative_hash = EXCLUDED.conservative_hash,
                 layout_hash = EXCLUDED.layout_hash,
                 chapter_count = EXCLUDED.chapter_count,
                 informative_chapter_count = EXCLUDED.informative_chapter_count,
                 char_count = EXCLUDED.char_count,
                 text_integrity_bps = EXCLUDED.text_integrity_bps,
                 computed_at = NOW()"#,
            input.book_id,
            input.normalization_version,
            input.layout_normalization_version,
            input.algorithm_version,
            input.source_content_hash,
            input.conservative_hash,
            input.layout_hash,
            input.chapter_count,
            input.informative_chapter_count,
            input.char_count,
            input.text_integrity_bps
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query!(
            "DELETE FROM passage_fingerprints WHERE book_id = $1",
            input.book_id
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query!(
            "DELETE FROM chapter_fingerprints WHERE book_id = $1",
            input.book_id
        )
        .execute(&mut *tx)
        .await?;

        for chapter in input.chapters {
            sqlx::query!(
                r#"INSERT INTO chapter_fingerprints
                   (chapter_id, book_id, chapter_index, normalization_version,
                    source_content_hash, conservative_hash, layout_hash, char_count,
                    informative, winnowing_count, computed_at)
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, NOW())"#,
                chapter.chapter_id,
                input.book_id,
                chapter.chapter_index,
                chapter.normalization_version,
                chapter.source_content_hash,
                chapter.conservative_hash,
                chapter.layout_hash,
                chapter.char_count,
                chapter.informative,
                chapter.winnowing_count
            )
            .execute(&mut *tx)
            .await?;
        }

        for passage in input.passages {
            sqlx::query!(
                r#"INSERT INTO passage_fingerprints
                   (chapter_id, book_id, normalization_version, fingerprint_hash,
                    position, span_length)
                   VALUES ($1, $2, $3, $4, $5, $6)
                   ON CONFLICT DO NOTHING"#,
                passage.chapter_id,
                input.book_id,
                passage.normalization_version,
                passage.fingerprint_hash,
                passage.position,
                passage.span_length
            )
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn deterministic_candidates(
        &self,
        book_ids: &[Uuid],
        target_book_ids: Option<&[Uuid]>,
        max_hash_document_frequency: i64,
        min_chapter_candidates: i64,
        min_passage_candidates: i64,
        max_pair_contributions: i64,
        max_candidates: i64,
    ) -> Result<DeterministicCandidateRecords> {
        let exact_file = sqlx::query!(
            r#"SELECT a.id AS a, b.id AS b
               FROM books a JOIN books b ON a.id < b.id AND a.file_hash = b.file_hash
               WHERE a.id = ANY($1) AND b.id = ANY($1) AND a.file_hash != ''
                 AND ($2::uuid[] IS NULL OR a.id = ANY($2) OR b.id = ANY($2))
               LIMIT $3"#,
            book_ids,
            target_book_ids,
            max_candidates
        )
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|row| BookPairRecord { a: row.a, b: row.b })
        .collect();

        let exact_content = sqlx::query!(
            r#"SELECT a.book_id AS a, b.book_id AS b
               FROM book_fingerprints a
               JOIN book_fingerprints b
                 ON a.book_id < b.book_id
                AND a.normalization_version = b.normalization_version
                AND a.conservative_hash = b.conservative_hash
               WHERE a.book_id = ANY($1) AND b.book_id = ANY($1)
                 AND a.char_count > 0 AND b.char_count > 0
                 AND ($2::uuid[] IS NULL OR a.book_id = ANY($2) OR b.book_id = ANY($2))
               LIMIT $3"#,
            book_ids,
            target_book_ids,
            max_candidates
        )
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|row| BookPairRecord { a: row.a, b: row.b })
        .collect();

        let shared_chapter_rows = sqlx::query(
            r#"WITH per_book_hash AS MATERIALIZED (
                   SELECT DISTINCT book_id, layout_hash
                   FROM chapter_fingerprints
                   WHERE informative = TRUE AND book_id = ANY($1)
               ), frequencies AS MATERIALIZED (
                   SELECT layout_hash,
                          COUNT(*)::bigint AS frequency,
                          COUNT(*) FILTER (
                              WHERE $3::uuid[] IS NULL OR book_id = ANY($3)
                          )::bigint AS target_frequency
                   FROM per_book_hash
                   GROUP BY layout_hash
                   HAVING COUNT(*) <= $2
               ), stats AS MATERIALIZED (
                   SELECT COALESCE(SUM(
                       target_frequency * (frequency - target_frequency)
                       + (target_frequency * (target_frequency - 1)) / 2
                   ), 0)::bigint AS pair_contributions
                   FROM frequencies
               ), bounded_hashes AS MATERIALIZED (
                   SELECT hash.book_id, hash.layout_hash
                   FROM stats
                   JOIN frequencies AS frequency
                     ON stats.pair_contributions <= $5
                   JOIN per_book_hash AS hash
                     ON hash.layout_hash = frequency.layout_hash
               ), pairs AS MATERIALIZED (
                   SELECT a.book_id AS a, b.book_id AS b, COUNT(*)::bigint AS shared
                   FROM bounded_hashes a
                   JOIN bounded_hashes b
                     ON a.layout_hash = b.layout_hash AND a.book_id < b.book_id
                   WHERE ($3::uuid[] IS NULL OR a.book_id = ANY($3) OR b.book_id = ANY($3))
                   GROUP BY a.book_id, b.book_id
                   HAVING COUNT(*) >= $4
                   LIMIT $6
               )
               SELECT pairs.a, pairs.b, pairs.shared, stats.pair_contributions
               FROM stats
               LEFT JOIN pairs ON TRUE"#,
        )
        .bind(book_ids)
        .bind(max_hash_document_frequency)
        .bind(target_book_ids)
        .bind(min_chapter_candidates)
        .bind(max_pair_contributions)
        .bind(max_candidates)
        .fetch_all(&self.pool)
        .await?;
        let shared_chapters = decode_bounded_candidate_rows(
            shared_chapter_rows,
            "shared chapter",
            max_pair_contributions,
        )?;

        let shared_passage_rows = sqlx::query(
            r#"WITH per_book_hash AS MATERIALIZED (
                   SELECT DISTINCT book_id, fingerprint_hash
                   FROM passage_fingerprints WHERE book_id = ANY($1)
               ), frequencies AS MATERIALIZED (
                   SELECT fingerprint_hash,
                          COUNT(*)::bigint AS frequency,
                          COUNT(*) FILTER (
                              WHERE $3::uuid[] IS NULL OR book_id = ANY($3)
                          )::bigint AS target_frequency
                   FROM per_book_hash
                   GROUP BY fingerprint_hash
                   HAVING COUNT(*) <= $2
               ), stats AS MATERIALIZED (
                   SELECT COALESCE(SUM(
                       target_frequency * (frequency - target_frequency)
                       + (target_frequency * (target_frequency - 1)) / 2
                   ), 0)::bigint AS pair_contributions
                   FROM frequencies
               ), bounded_hashes AS MATERIALIZED (
                   SELECT hash.book_id, hash.fingerprint_hash
                   FROM stats
                   JOIN frequencies AS frequency
                     ON stats.pair_contributions <= $5
                   JOIN per_book_hash AS hash
                     ON hash.fingerprint_hash = frequency.fingerprint_hash
               ), pairs AS MATERIALIZED (
                   SELECT a.book_id AS a, b.book_id AS b, COUNT(*)::bigint AS shared
                   FROM bounded_hashes a
                   JOIN bounded_hashes b
                     ON a.fingerprint_hash = b.fingerprint_hash AND a.book_id < b.book_id
                   WHERE ($3::uuid[] IS NULL OR a.book_id = ANY($3) OR b.book_id = ANY($3))
                   GROUP BY a.book_id, b.book_id
                   HAVING COUNT(*) >= $4
                   LIMIT $6
               )
               SELECT pairs.a, pairs.b, pairs.shared, stats.pair_contributions
               FROM stats
               LEFT JOIN pairs ON TRUE"#,
        )
        .bind(book_ids)
        .bind(max_hash_document_frequency)
        .bind(target_book_ids)
        .bind(min_passage_candidates)
        .bind(max_pair_contributions)
        .bind(max_candidates)
        .fetch_all(&self.pool)
        .await?;
        let shared_passages = decode_bounded_candidate_rows(
            shared_passage_rows,
            "shared passage",
            max_pair_contributions,
        )?;

        Ok(DeterministicCandidateRecords {
            exact_file,
            exact_content,
            shared_chapters,
            shared_passages,
        })
    }

    pub(crate) async fn chapter_contents_bounded(
        &self,
        chapter_ids: &[Uuid],
        max_source_bytes: u64,
    ) -> Result<BoundedChapterContentRecords> {
        if chapter_ids.is_empty() {
            return Ok(BoundedChapterContentRecords {
                source_bytes: 0,
                chapters: Vec::new(),
            });
        }
        let max_source_bytes_i64 = i64::try_from(max_source_bytes).unwrap_or(i64::MAX);
        let rows = sqlx::query(
            r#"WITH stats AS MATERIALIZED (
                   SELECT COALESCE(SUM(octet_length(content)), 0)::bigint AS source_bytes
                   FROM chapters
                   WHERE id = ANY($1)
               )
               SELECT chapter.id, chapter.content, stats.source_bytes
               FROM stats
               LEFT JOIN chapters AS chapter
                 ON chapter.id = ANY($1) AND stats.source_bytes <= $2
               ORDER BY chapter.id NULLS LAST"#,
        )
        .bind(chapter_ids)
        .bind(max_source_bytes_i64)
        .fetch_all(&self.pool)
        .await?;
        let source_bytes_i64 = rows
            .first()
            .ok_or_else(|| {
                Error::Internal("bounded candidate chapter query returned no stats row".into())
            })?
            .try_get::<i64, _>("source_bytes")?;
        let source_bytes = u64::try_from(source_bytes_i64).map_err(|_| {
            Error::Internal(format!(
                "invalid candidate source text size: {source_bytes_i64}"
            ))
        })?;
        if source_bytes > max_source_bytes {
            return Err(Error::Validation(format!(
                "duplicate candidate source text batch budget exceeded: actual_bytes={source_bytes}, limit_bytes={max_source_bytes}"
            )));
        }
        let mut chapters = Vec::with_capacity(rows.len());
        for row in rows {
            let Some(id) = row.try_get::<Option<Uuid>, _>("id")? else {
                continue;
            };
            let content = row
                .try_get::<Option<String>, _>("content")?
                .ok_or_else(|| {
                    Error::Internal(format!("bounded candidate chapter row {id} has no content"))
                })?;
            chapters.push(ChapterContentRecord { id, content });
        }
        Ok(BoundedChapterContentRecords {
            source_bytes,
            chapters,
        })
    }

    pub(crate) async fn chapter_content_sizes(
        &self,
        book_ids: &[Uuid],
    ) -> Result<Vec<(Uuid, i64)>> {
        if book_ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query(
            r#"SELECT book_id,
                      COALESCE(SUM(octet_length(content)), 0)::bigint AS source_bytes
               FROM chapters
               WHERE book_id = ANY($1)
               GROUP BY book_id
               ORDER BY book_id"#,
        )
        .bind(book_ids)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| Ok((row.try_get("book_id")?, row.try_get("source_bytes")?)))
            .collect()
    }

    pub(crate) async fn record_scan_candidates(
        &self,
        scan_id: Uuid,
        candidates_found: i32,
    ) -> Result<()> {
        sqlx::query!(
            "UPDATE dedup_scan_runs SET candidates_found = $2, updated_at = NOW() WHERE id = $1",
            scan_id,
            candidates_found
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn update_scan_progress(
        &self,
        scan_id: Uuid,
        progress: i16,
        phase: DedupScanPhase,
        books_processed: i32,
        chapters_processed: i32,
    ) -> Result<()> {
        sqlx::query!(
            r#"UPDATE dedup_scan_runs
               SET progress = $2, progress_message = $3, books_processed = $4,
                   chapters_processed = $5, updated_at = NOW()
               WHERE id = $1"#,
            scan_id,
            progress,
            phase.as_str(),
            books_processed,
            chapters_processed
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn update_scan_pair_progress(
        &self,
        scan_id: Uuid,
        progress: i16,
        counts: ScanPairCounts,
    ) -> Result<()> {
        sqlx::query!(
            r#"UPDATE dedup_scan_runs
               SET progress = $2, progress_message = $7,
                   pairs_found = $3, exact_pairs = $4, contained_pairs = $5,
                   semantic_pairs = $6, updated_at = NOW()
               WHERE id = $1"#,
            scan_id,
            progress,
            counts.found,
            counts.exact,
            counts.contained,
            counts.semantic,
            DedupScanPhase::Verifying.as_str()
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Publishes a fully verified scan as one transaction. Readers either see
    /// the previous complete result set or this complete result set.
    pub(crate) async fn publish_scan_results(&self, input: PublishDuplicateScan) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        validate_pair_publication_snapshots(&input)?;
        let book_ids: Vec<_> = input
            .book_snapshots
            .iter()
            .map(|snapshot| snapshot.book_id)
            .collect();
        lock_dedup_publication_books(&mut tx, &book_ids).await?;
        revalidate_dedup_publication_books(&mut tx, &input.book_snapshots).await?;
        for pair in input.pairs {
            persist_pair(&mut tx, input.scan_id, pair).await?;
        }
        finalize_scan(
            &mut tx,
            input.scan_id,
            &input.scope_book_ids,
            &input.affected_scope_book_ids,
            input.counts,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn finish_scan(&self, input: FinishDuplicateScan) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        finalize_scan(
            &mut tx,
            input.scan_id,
            &input.scope_book_ids,
            &input.affected_scope_book_ids,
            input.counts,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}

fn validate_pair_publication_snapshots(input: &PublishDuplicateScan) -> Result<()> {
    let snapshots: std::collections::HashMap<_, _> = input
        .book_snapshots
        .iter()
        .map(|snapshot| (snapshot.book_id, snapshot))
        .collect();
    for pair in &input.pairs {
        let Some(book_a) = snapshots.get(&pair.book_a_id) else {
            return Err(Error::Internal(format!(
                "missing duplicate publication snapshot for {}",
                pair.book_a_id
            )));
        };
        let Some(book_b) = snapshots.get(&pair.book_b_id) else {
            return Err(Error::Internal(format!(
                "missing duplicate publication snapshot for {}",
                pair.book_b_id
            )));
        };
        if pair.evidence.algorithm_version != book_a.algorithm_version
            || pair.evidence.algorithm_version != book_b.algorithm_version
            || pair.evidence.book_a_layout_hash != book_a.layout_hash
            || pair.evidence.book_b_layout_hash != book_b.layout_hash
        {
            return Err(Error::Internal(format!(
                "duplicate pair publication evidence does not match book snapshots for {}:{}",
                pair.book_a_id, pair.book_b_id
            )));
        }
    }
    Ok(())
}

async fn lock_dedup_publication_books(
    tx: &mut Transaction<'_, Postgres>,
    book_ids: &[Uuid],
) -> Result<()> {
    if book_ids.is_empty() {
        return Ok(());
    }
    let global_acquired: bool = sqlx::query_scalar("SELECT try_lock_novel_dedup_global_barrier()")
        .fetch_one(&mut **tx)
        .await?;
    if !global_acquired {
        return Err(Error::RetryableConflict(
            "duplicate publication global write barrier is busy; retry scan".into(),
        ));
    }
    let acquired: bool = sqlx::query_scalar("SELECT try_lock_novel_dedup_books($1)")
        .bind(book_ids)
        .fetch_one(&mut **tx)
        .await?;
    if !acquired {
        return Err(Error::RetryableConflict(
            "duplicate publication book lock is busy; retry scan".into(),
        ));
    }
    Ok(())
}

async fn revalidate_dedup_publication_books(
    tx: &mut Transaction<'_, Postgres>,
    snapshots: &[DuplicateBookPublicationSnapshot],
) -> Result<()> {
    if snapshots.is_empty() {
        return Ok(());
    }
    let book_ids: Vec<_> = snapshots.iter().map(|snapshot| snapshot.book_id).collect();
    let rows = sqlx::query(
        r#"SELECT b.id, b.file_hash,
                  bf.algorithm_version, bf.source_content_hash,
                  bf.conservative_hash, bf.layout_hash
             FROM books b
             LEFT JOIN book_fingerprints bf ON bf.book_id = b.id
             WHERE b.id = ANY($1)"#,
    )
    .bind(&book_ids)
    .fetch_all(&mut **tx)
    .await?;
    let mut current = std::collections::HashMap::with_capacity(rows.len());
    for row in rows {
        let book_id: Uuid = row.try_get("id")?;
        current.insert(
            book_id,
            (
                row.try_get::<String, _>("file_hash")?,
                row.try_get::<Option<i32>, _>("algorithm_version")?,
                row.try_get::<Option<String>, _>("source_content_hash")?,
                row.try_get::<Option<String>, _>("conservative_hash")?,
                row.try_get::<Option<String>, _>("layout_hash")?,
            ),
        );
    }
    for snapshot in snapshots {
        let Some((file_hash, algorithm_version, source_hash, conservative_hash, layout_hash)) =
            current.get(&snapshot.book_id)
        else {
            return Err(Error::Internal(format!(
                "duplicate publication source book disappeared: {}",
                snapshot.book_id
            )));
        };
        if file_hash != &snapshot.file_hash
            || *algorithm_version != Some(snapshot.algorithm_version)
            || source_hash.as_deref() != Some(snapshot.source_content_hash.as_str())
            || conservative_hash.as_deref() != Some(snapshot.conservative_hash.as_str())
            || layout_hash.as_deref() != Some(snapshot.layout_hash.as_str())
        {
            return Err(Error::Internal(format!(
                "duplicate publication source changed after verification for {}; retry scan",
                snapshot.book_id
            )));
        }
    }
    Ok(())
}

fn duplicate_scan_task_resources(library_id: Option<Uuid>) -> Vec<(String, TaskExecutionLockMode)> {
    match library_id {
        Some(library_id) => vec![
            (
                DEDUP_SCAN_BARRIER_RESOURCE.to_string(),
                TaskExecutionLockMode::Shared,
            ),
            (
                format!("{DEDUP_SCAN_LIBRARY_RESOURCE_PREFIX}:{library_id}"),
                TaskExecutionLockMode::Exclusive,
            ),
        ],
        None => vec![(
            DEDUP_SCAN_BARRIER_RESOURCE.to_string(),
            TaskExecutionLockMode::Exclusive,
        )],
    }
}

pub(crate) fn duplicate_scan_payload(
    scan_id: Uuid,
    library_id: Option<Uuid>,
    include_semantic: bool,
    target_book_ids: Option<&[Uuid]>,
) -> DedupScanTask {
    DedupScanTask::new(
        scan_id,
        library_id,
        include_semantic,
        target_book_ids.map(<[Uuid]>::to_vec),
    )
}

fn encode_duplicate_scan_task(payload: &DedupScanTask) -> Result<Value> {
    serde_json::to_value(payload)
        .map_err(|error| Error::Internal(format!("serialize duplicate scan task: {error}")))
}

fn decode_duplicate_scan_task(payload: Value) -> Result<DedupScanTask> {
    serde_json::from_value(payload)
        .map_err(|error| Error::Internal(format!("invalid persisted duplicate scan task: {error}")))
}

pub(crate) fn merge_scan_target_book_ids(
    existing: Option<Vec<Uuid>>,
    requested: Option<Vec<Uuid>>,
) -> Option<Vec<Uuid>> {
    match (existing, requested) {
        (None, _) | (_, None) => None,
        (Some(mut existing), Some(requested)) => {
            existing.extend(requested);
            existing.sort_unstable();
            existing.dedup();
            Some(existing)
        }
    }
}

async fn count_scan_books(
    tx: &mut Transaction<'_, Postgres>,
    library_id: Option<Uuid>,
    target_book_ids: Option<&[Uuid]>,
) -> Result<i64> {
    let count = sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "count!"
           FROM books
           WHERE status NOT IN ('archived', 'duplicate')
             AND ($1::uuid IS NULL OR library_id = $1)
             AND ($2::uuid[] IS NULL OR id = ANY($2))"#,
        library_id,
        target_book_ids
    )
    .fetch_one(&mut **tx)
    .await?;
    Ok(count)
}

async fn persist_pair(
    tx: &mut Transaction<'_, Postgres>,
    scan_id: Uuid,
    pair: DuplicatePairWrite,
) -> Result<()> {
    let evidence = serde_json::to_value(&pair.evidence)
        .map_err(|error| Error::Internal(format!("serialize duplicate evidence: {error}")))?;
    let pair_id = sqlx::query_scalar!(
        r#"INSERT INTO duplicate_pairs
           (id, book_a_id, book_b_id, similarity, method, scan_run_id, relation,
            review_status, confidence, shared_chapters, coverage_a, coverage_b,
            character_coverage_a, character_coverage_b, longest_contiguous_run,
            order_score, contained_book_id, recommended_primary_id, semantic_score,
            algorithm_version, evidence, stale, updated_at)
           VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, 'pending', $3, $7,
                   $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18,
                   FALSE, NOW())
           ON CONFLICT (book_a_id, book_b_id) DO UPDATE SET
             similarity = EXCLUDED.similarity,
             method = EXCLUDED.method,
             scan_run_id = EXCLUDED.scan_run_id,
             relation = EXCLUDED.relation,
             (review_status, resolved, resolution, resolved_by, resolved_at) = (
               SELECT
                 CASE WHEN preservation.keep_resolution
                   THEN duplicate_pairs.review_status ELSE 'pending' END,
                 CASE WHEN preservation.keep_resolution
                   THEN duplicate_pairs.resolved ELSE FALSE END,
                 CASE WHEN preservation.keep_resolution
                   THEN duplicate_pairs.resolution ELSE NULL END,
                 CASE WHEN preservation.keep_resolution
                   THEN duplicate_pairs.resolved_by ELSE NULL END,
                 CASE WHEN preservation.keep_resolution
                   THEN duplicate_pairs.resolved_at ELSE NULL END
               FROM LATERAL (
                 SELECT
                   duplicate_pairs.review_status IN ('dismissed', 'confirmed', 'deferred')
                   AND duplicate_pairs.algorithm_version = EXCLUDED.algorithm_version
                   AND duplicate_pairs.evidence->>'book_a_layout_hash'
                       IS NOT DISTINCT FROM EXCLUDED.evidence->>'book_a_layout_hash'
                   AND duplicate_pairs.evidence->>'book_b_layout_hash'
                       IS NOT DISTINCT FROM EXCLUDED.evidence->>'book_b_layout_hash'
                     AS keep_resolution
               ) preservation
             ),
             confidence = EXCLUDED.confidence,
             shared_chapters = EXCLUDED.shared_chapters,
             coverage_a = EXCLUDED.coverage_a,
             coverage_b = EXCLUDED.coverage_b,
             character_coverage_a = EXCLUDED.character_coverage_a,
             character_coverage_b = EXCLUDED.character_coverage_b,
             longest_contiguous_run = EXCLUDED.longest_contiguous_run,
             order_score = EXCLUDED.order_score,
             contained_book_id = EXCLUDED.contained_book_id,
             recommended_primary_id = EXCLUDED.recommended_primary_id,
             semantic_score = EXCLUDED.semantic_score,
             algorithm_version = EXCLUDED.algorithm_version,
             evidence = EXCLUDED.evidence,
             stale = FALSE,
             updated_at = NOW()
           RETURNING id AS "id!""#,
        pair.book_a_id,
        pair.book_b_id,
        pair.confidence,
        pair.method,
        scan_id,
        pair.relation.as_str(),
        pair.shared_chapters,
        pair.coverage_a,
        pair.coverage_b,
        pair.character_coverage_a,
        pair.character_coverage_b,
        pair.longest_contiguous_run,
        pair.order_score,
        pair.contained_book_id,
        pair.recommended_primary_id,
        pair.semantic_score,
        pair.algorithm_version,
        evidence
    )
    .fetch_one(&mut **tx)
    .await?;

    sqlx::query!(
        "DELETE FROM duplicate_chapter_matches WHERE pair_id = $1",
        pair_id
    )
    .execute(&mut **tx)
    .await?;
    for item in pair.chapter_matches {
        sqlx::query!(
            r#"INSERT INTO duplicate_chapter_matches
               (pair_id, chapter_a_id, chapter_b_id, chapter_a_index,
                chapter_b_index, match_type, similarity, shared_fingerprints,
                alignment_group, segment_ordinal,
                chapter_a_start, chapter_a_end, chapter_b_start, chapter_b_end,
                matched_chars)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8,
                       $9, $10, $11, $12, $13, $14, $15)
               ON CONFLICT DO NOTHING"#,
            pair_id,
            item.chapter_a_id,
            item.chapter_b_id,
            item.chapter_a_index,
            item.chapter_b_index,
            item.match_type,
            item.similarity,
            item.shared_fingerprints,
            item.alignment_group,
            item.segment_ordinal,
            item.chapter_a_start,
            item.chapter_a_end,
            item.chapter_b_start,
            item.chapter_b_end,
            item.matched_chars
        )
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn finalize_scan(
    tx: &mut Transaction<'_, Postgres>,
    scan_id: Uuid,
    scope_book_ids: &[Uuid],
    affected_scope_book_ids: &[Uuid],
    counts: ScanPairCounts,
) -> Result<()> {
    sqlx::query!(
        r#"UPDATE duplicate_pairs AS pair
           SET stale = TRUE, updated_at = NOW()
           FROM dedup_scan_runs AS current_scan
           WHERE (
             (pair.book_a_id = ANY($2) AND pair.book_b_id = ANY($1))
             OR (pair.book_b_id = ANY($2) AND pair.book_a_id = ANY($1))
           ) AND current_scan.id = $3
             AND pair.scan_run_id IS DISTINCT FROM $3
             -- A confirmed secondary version is intentionally excluded from
             -- candidate generation after resolution. Its human decision is
             -- therefore preserved only while the same algorithm and the same
             -- source-backed fingerprints still support it. Algorithm upgrades
             -- or invalidated/missing caches must supersede the old evidence.
             AND NOT (
               pair.stale = FALSE
               AND pair.resolved = TRUE
               AND pair.review_status = 'confirmed'
               AND pair.algorithm_version = current_scan.algorithm_version
               AND pair.evidence->>'algorithm_version'
                   = current_scan.algorithm_version::text
               AND EXISTS (
                 SELECT 1
                 FROM book_fingerprints AS fingerprint_a
                 JOIN book_fingerprints AS fingerprint_b ON TRUE
                 WHERE fingerprint_a.book_id = pair.book_a_id
                   AND fingerprint_b.book_id = pair.book_b_id
                   AND fingerprint_a.algorithm_version = current_scan.algorithm_version
                   AND fingerprint_b.algorithm_version = current_scan.algorithm_version
                   AND fingerprint_a.layout_hash = pair.evidence->>'book_a_layout_hash'
                   AND fingerprint_b.layout_hash = pair.evidence->>'book_b_layout_hash'
               )
             )"#,
        scope_book_ids,
        affected_scope_book_ids,
        scan_id
    )
    .execute(&mut **tx)
    .await?;
    sqlx::query!(
        r#"UPDATE dedup_scan_runs
           SET status = 'completed', progress = 100, progress_message = $6,
               pairs_found = $2, exact_pairs = $3, contained_pairs = $4,
               semantic_pairs = $5, completed_at = NOW(), updated_at = NOW()
           WHERE id = $1"#,
        scan_id,
        counts.found,
        counts.exact,
        counts.contained,
        counts.semantic,
        DedupScanPhase::Completed.as_str()
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn insert_dedup_test_book(pool: &sqlx::PgPool, label: &str) -> (Uuid, Uuid) {
        let library_id = Uuid::now_v7();
        let book_id = Uuid::now_v7();
        sqlx::query("INSERT INTO libraries (id, name, root_path) VALUES ($1, $2, $3)")
            .bind(library_id)
            .bind(format!("{label} library"))
            .bind(format!("/tmp/nova-{label}-{library_id}"))
            .execute(pool)
            .await
            .expect("insert dedup repository test library");
        sqlx::query(
            r#"INSERT INTO books
               (id, library_id, title, format, status, file_path, file_hash)
               VALUES ($1, $2, $3, 'txt', 'ready', $4, $5)"#,
        )
        .bind(book_id)
        .bind(library_id)
        .bind(format!("{label} book"))
        .bind(format!("/tmp/{book_id}.txt"))
        .bind(format!("{label}-{book_id}"))
        .execute(pool)
        .await
        .expect("insert dedup repository test book");
        (library_id, book_id)
    }

    async fn remove_dedup_test_book(pool: &sqlx::PgPool, library_id: Uuid, book_id: Uuid) {
        sqlx::query("DELETE FROM books WHERE id = $1")
            .bind(book_id)
            .execute(pool)
            .await
            .expect("remove dedup repository test book");
        sqlx::query(
            r#"DELETE FROM tasks
               WHERE id IN (
                 SELECT task_id FROM dedup_scan_runs
                 WHERE library_id = $1 AND task_id IS NOT NULL
               )"#,
        )
        .bind(library_id)
        .execute(pool)
        .await
        .expect("remove dedup repository scan tasks");
        sqlx::query("DELETE FROM libraries WHERE id = $1")
            .bind(library_id)
            .execute(pool)
            .await
            .expect("remove dedup repository test library");
    }

    #[test]
    fn global_and_library_scans_declare_composable_task_resources() {
        let library_id = Uuid::from_u128(42);
        assert_eq!(
            duplicate_scan_task_resources(None),
            vec![(
                DEDUP_SCAN_BARRIER_RESOURCE.to_string(),
                TaskExecutionLockMode::Exclusive,
            )]
        );
        assert_eq!(
            duplicate_scan_task_resources(Some(library_id)),
            vec![
                (
                    DEDUP_SCAN_BARRIER_RESOURCE.to_string(),
                    TaskExecutionLockMode::Shared,
                ),
                (
                    format!("{DEDUP_SCAN_LIBRARY_RESOURCE_PREFIX}:{library_id}"),
                    TaskExecutionLockMode::Exclusive,
                ),
            ]
        );
    }

    #[tokio::test]
    async fn bounded_chapter_scan_rejects_source_over_budget() {
        let _write_guard = crate::dedup::DEDUP_DATABASE_TEST_LOCK.lock().await;
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let pool = sqlx::PgPool::connect(&database_url)
            .await
            .expect("connect to PostgreSQL for bounded chapter scan test");
        crate::migrations::run_database_migrations(&pool)
            .await
            .expect("apply migrations for bounded chapter scan test");
        let (library_id, book_id) = insert_dedup_test_book(&pool, "bounded-chapter-scan").await;
        let chapter_id = Uuid::now_v7();
        let content = "source text larger than the configured test budget";
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
        .expect("insert oversized bounded chapter fixture");

        let repository = PgDuplicateRepository::new(pool.clone());
        let error = repository
            .scan_chapters_bounded(book_id, 8, 10)
            .await
            .expect_err("oversized source must fail before returning chapter content")
            .to_string();
        assert!(error.contains(&format!("actual_bytes={}", content.len())));
        assert!(error.contains("limit_bytes=8"));

        remove_dedup_test_book(&pool, library_id, book_id).await;
    }

    #[tokio::test]
    async fn bounded_chapter_scan_rejects_excess_empty_chapter_rows() {
        let _write_guard = crate::dedup::DEDUP_DATABASE_TEST_LOCK.lock().await;
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let pool = sqlx::PgPool::connect(&database_url)
            .await
            .expect("connect to PostgreSQL for chapter row cap test");
        crate::migrations::run_database_migrations(&pool)
            .await
            .expect("apply migrations for chapter row cap test");
        let (library_id, book_id) = insert_dedup_test_book(&pool, "chapter-row-cap").await;
        for chapter_index in 0..3_i32 {
            sqlx::query(
                r#"INSERT INTO chapters
                   (id, book_id, index, chapter_index, title, content)
                   VALUES ($1, $2, $3, $3, 'Empty Chapter', '')"#,
            )
            .bind(Uuid::now_v7())
            .bind(book_id)
            .bind(chapter_index)
            .execute(&pool)
            .await
            .expect("insert empty chapter row fixture");
        }

        let repository = PgDuplicateRepository::new(pool.clone());
        let error = repository
            .scan_chapters_bounded(book_id, 64, 2)
            .await
            .expect_err("three empty chapters must fail a two-row budget before row fetch")
            .to_string();
        assert!(error.contains("chapter row per-book budget exceeded"));
        assert!(error.contains("actual=3"));
        assert!(error.contains("limit=2"));

        remove_dedup_test_book(&pool, library_id, book_id).await;
    }

    #[tokio::test]
    async fn zero_anchor_cache_still_requires_the_current_layout_normalization_version() {
        let _write_guard = crate::dedup::DEDUP_DATABASE_TEST_LOCK.lock().await;
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let pool = sqlx::PgPool::connect(&database_url)
            .await
            .expect("connect to PostgreSQL for zero-anchor cache version test");
        crate::migrations::run_database_migrations(&pool)
            .await
            .expect("apply migrations for zero-anchor cache version test");
        let (library_id, book_id) = insert_dedup_test_book(&pool, "zero-anchor-version").await;
        sqlx::query(
            r#"INSERT INTO book_fingerprints
               (book_id, normalization_version, layout_normalization_version,
                algorithm_version, source_content_hash, conservative_hash,
                layout_hash, chapter_count, informative_chapter_count,
                char_count, text_integrity_bps)
               VALUES ($1, 1, 1, 4, $2, $3, $4, 0, 0, 0, 10000)"#,
        )
        .bind(book_id)
        .bind("zero-anchor-source")
        .bind("zero-anchor-conservative")
        .bind("zero-anchor-layout")
        .execute(&pool)
        .await
        .expect("insert zero-anchor fingerprint cache");

        let repository = PgDuplicateRepository::new(pool.clone());
        let current = repository
            .valid_cached_book_ids(&[book_id], 4, 1, 1)
            .await
            .expect("validate matching zero-anchor cache version");
        assert!(current.contains(&book_id));
        let upgraded = repository
            .valid_cached_book_ids(&[book_id], 4, 1, 2)
            .await
            .expect("validate upgraded zero-anchor cache version");
        assert!(
            !upgraded.contains(&book_id),
            "an empty passage set cannot hide a stale layout version"
        );

        remove_dedup_test_book(&pool, library_id, book_id).await;
    }

    #[tokio::test]
    async fn semantic_freshness_snapshots_require_the_complete_current_contract() {
        let _write_guard = crate::dedup::DEDUP_DATABASE_TEST_LOCK.lock().await;
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let pool = sqlx::PgPool::connect(&database_url)
            .await
            .expect("connect to PostgreSQL for semantic freshness snapshot test");
        crate::migrations::run_database_migrations(&pool)
            .await
            .expect("apply migrations for semantic freshness snapshot test");
        let (library_id, current_book_id) =
            insert_dedup_test_book(&pool, "semantic-freshness-snapshot").await;
        let stale_algorithm_book_id = Uuid::now_v7();
        let stale_conservative_book_id = Uuid::now_v7();
        let stale_layout_book_id = Uuid::now_v7();
        let unrequested_current_book_id = Uuid::now_v7();
        let extra_book_ids = [
            stale_algorithm_book_id,
            stale_conservative_book_id,
            stale_layout_book_id,
            unrequested_current_book_id,
        ];
        for book_id in extra_book_ids {
            sqlx::query(
                r#"INSERT INTO books
                   (id, library_id, title, format, status, file_path, file_hash)
                   VALUES ($1, $2, 'Semantic freshness fixture', 'txt', 'ready', $3, $4)"#,
            )
            .bind(book_id)
            .bind(library_id)
            .bind(format!("/tmp/{book_id}.txt"))
            .bind(format!("semantic-freshness-{book_id}"))
            .execute(&pool)
            .await
            .expect("insert semantic freshness fixture book");
        }

        let current_algorithm = i32::from(nova_ingest::dedup::DEDUP_ALGORITHM_VERSION);
        let current_conservative =
            i32::from(nova_ingest::dedup::CONSERVATIVE_NORMALIZATION_VERSION);
        let current_layout = i32::from(nova_ingest::dedup::LAYOUT_NORMALIZATION_VERSION);
        for (book_id, algorithm, conservative, layout, source_hash) in [
            (
                current_book_id,
                current_algorithm,
                current_conservative,
                current_layout,
                "current-source-hash",
            ),
            (
                stale_algorithm_book_id,
                current_algorithm.saturating_add(1),
                current_conservative,
                current_layout,
                "stale-algorithm-source-hash",
            ),
            (
                stale_conservative_book_id,
                current_algorithm,
                current_conservative.saturating_add(1),
                current_layout,
                "stale-conservative-source-hash",
            ),
            (
                stale_layout_book_id,
                current_algorithm,
                current_conservative,
                current_layout.saturating_add(1),
                "stale-layout-source-hash",
            ),
            (
                unrequested_current_book_id,
                current_algorithm,
                current_conservative,
                current_layout,
                "unrequested-current-source-hash",
            ),
        ] {
            sqlx::query(
                r#"INSERT INTO book_fingerprints
                   (book_id, normalization_version, layout_normalization_version,
                    algorithm_version, source_content_hash, conservative_hash,
                    layout_hash, chapter_count, informative_chapter_count,
                    char_count, text_integrity_bps)
                   VALUES ($1, $2, $3, $4, $5, $6, $7, 0, 0, 0, 10000)"#,
            )
            .bind(book_id)
            .bind(conservative)
            .bind(layout)
            .bind(algorithm)
            .bind(source_hash)
            .bind(format!("conservative-{book_id}"))
            .bind(format!("layout-{book_id}"))
            .execute(&pool)
            .await
            .expect("insert semantic freshness fingerprint fixture");
        }

        let requested_book_ids = [
            current_book_id,
            stale_algorithm_book_id,
            stale_conservative_book_id,
            stale_layout_book_id,
        ];
        let snapshots = PgDuplicateRepository::new(pool.clone())
            .current_semantic_freshness_snapshots(&requested_book_ids)
            .await
            .expect("load current semantic freshness snapshots");

        assert_eq!(
            snapshots,
            vec![SemanticFreshnessSnapshotRecord {
                book_id: current_book_id,
                source_content_hash: "current-source-hash".to_string(),
            }],
            "only requested fingerprints matching all current versions are semantically fresh"
        );

        sqlx::query("DELETE FROM books WHERE library_id = $1")
            .bind(library_id)
            .execute(&pool)
            .await
            .expect("remove semantic freshness fixture books");
        sqlx::query("DELETE FROM libraries WHERE id = $1")
            .bind(library_id)
            .execute(&pool)
            .await
            .expect("remove semantic freshness fixture library");
    }

    #[tokio::test]
    async fn bounded_candidate_load_rejects_content_growth_after_size_preflight() {
        let _write_guard = crate::dedup::DEDUP_DATABASE_TEST_LOCK.lock().await;
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let pool = sqlx::PgPool::connect(&database_url)
            .await
            .expect("connect to PostgreSQL for bounded candidate load test");
        crate::migrations::run_database_migrations(&pool)
            .await
            .expect("apply migrations for bounded candidate load test");
        let (library_id, book_id) = insert_dedup_test_book(&pool, "bounded-candidate-load").await;
        let chapter_id = Uuid::now_v7();
        let initial_content = "small";
        sqlx::query(
            r#"INSERT INTO chapters
               (id, book_id, index, chapter_index, title, content)
               VALUES ($1, $2, 0, 0, 'Chapter', $3)"#,
        )
        .bind(chapter_id)
        .bind(book_id)
        .bind(initial_content)
        .execute(&pool)
        .await
        .expect("insert bounded candidate chapter fixture");

        let repository = PgDuplicateRepository::new(pool.clone());
        let preflight_sizes = repository
            .chapter_content_sizes(&[book_id])
            .await
            .expect("preflight candidate source size");
        assert_eq!(
            preflight_sizes,
            vec![(
                book_id,
                i64::try_from(initial_content.len()).expect("fixture size fits i64")
            )]
        );

        let expanded_content = "source text expanded after the size preflight";
        sqlx::query("UPDATE chapters SET content = $2 WHERE id = $1")
            .bind(chapter_id)
            .bind(expanded_content)
            .execute(&pool)
            .await
            .expect("expand candidate content after preflight");
        let remaining_batch_bytes =
            u64::try_from(initial_content.len()).expect("fixture size fits u64");
        let error = repository
            .chapter_contents_bounded(&[chapter_id], remaining_batch_bytes)
            .await
            .expect_err("post-preflight content growth must fail before returning content")
            .to_string();
        assert!(error.contains(&format!("actual_bytes={}", expanded_content.len())));
        assert!(error.contains(&format!("limit_bytes={remaining_batch_bytes}")));

        remove_dedup_test_book(&pool, library_id, book_id).await;
    }

    #[tokio::test]
    async fn deterministic_candidate_query_honors_budget_plus_one_sentinel_cap() {
        let _write_guard = crate::dedup::DEDUP_DATABASE_TEST_LOCK.lock().await;
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let pool = sqlx::PgPool::connect(&database_url)
            .await
            .expect("connect to PostgreSQL for candidate cap test");
        crate::migrations::run_database_migrations(&pool)
            .await
            .expect("apply migrations for candidate cap test");
        let (library_id, first_book_id) = insert_dedup_test_book(&pool, "candidate-cap").await;
        let shared_file_hash = format!("candidate-cap-shared-{library_id}");
        sqlx::query("UPDATE books SET file_hash = $2 WHERE id = $1")
            .bind(first_book_id)
            .bind(&shared_file_hash)
            .execute(&pool)
            .await
            .expect("set first candidate cap file hash");
        let mut book_ids = vec![first_book_id];
        for offset in 1..4 {
            let book_id = Uuid::now_v7();
            sqlx::query(
                r#"INSERT INTO books
                   (id, library_id, title, format, status, file_path, file_hash)
                   VALUES ($1, $2, $3, 'txt', 'ready', $4, $5)"#,
            )
            .bind(book_id)
            .bind(library_id)
            .bind(format!("Candidate cap book {offset}"))
            .bind(format!("/tmp/{book_id}.txt"))
            .bind(&shared_file_hash)
            .execute(&pool)
            .await
            .expect("insert candidate cap book");
            book_ids.push(book_id);
        }

        let repository = PgDuplicateRepository::new(pool.clone());
        let logical_budget = 2_i64;
        let records = repository
            .deterministic_candidates(
                &book_ids,
                None,
                50,
                2,
                4,
                1_000,
                logical_budget.saturating_add(1),
            )
            .await
            .expect("load bounded deterministic candidates");
        assert_eq!(
            records.exact_file.len(),
            usize::try_from(logical_budget.saturating_add(1))
                .expect("candidate fixture limit fits usize"),
            "the repository must return at most the budget+1 sentinel instead of the full six-pair fanout"
        );

        sqlx::query("DELETE FROM books WHERE library_id = $1")
            .bind(library_id)
            .execute(&pool)
            .await
            .expect("remove candidate cap books");
        sqlx::query("DELETE FROM libraries WHERE id = $1")
            .bind(library_id)
            .execute(&pool)
            .await
            .expect("remove candidate cap library");
    }

    #[tokio::test]
    async fn exact_content_candidates_require_matching_conservative_hash() {
        let _write_guard = crate::dedup::DEDUP_DATABASE_TEST_LOCK.lock().await;
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let pool = sqlx::PgPool::connect(&database_url)
            .await
            .expect("connect to PostgreSQL for exact content candidate test");
        crate::migrations::run_database_migrations(&pool)
            .await
            .expect("apply migrations for exact content candidate test");
        let (library_id, first_book_id) =
            insert_dedup_test_book(&pool, "exact-content-candidate").await;
        let second_book_id = Uuid::now_v7();
        sqlx::query(
            r#"INSERT INTO books
               (id, library_id, title, format, status, file_path, file_hash)
               VALUES ($1, $2, 'Exact content candidate book 2', 'txt', 'ready', $3, $4)"#,
        )
        .bind(second_book_id)
        .bind(library_id)
        .bind(format!("/tmp/{second_book_id}.txt"))
        .bind(format!("exact-content-candidate-{second_book_id}"))
        .execute(&pool)
        .await
        .expect("insert second exact content candidate book");

        let shared_layout_hash = format!("shared-layout-{library_id}");
        let shared_conservative_hash = format!("shared-conservative-{library_id}");
        for (book_id, conservative_hash) in [
            (first_book_id, shared_conservative_hash.clone()),
            (
                second_book_id,
                format!("different-conservative-{library_id}"),
            ),
        ] {
            sqlx::query(
                r#"INSERT INTO book_fingerprints
                   (book_id, normalization_version, layout_normalization_version,
                    algorithm_version, source_content_hash, conservative_hash,
                    layout_hash, chapter_count, informative_chapter_count,
                    char_count, text_integrity_bps)
                   VALUES ($1, 1, 1, $2, $3, $4, $5, 1, 1, 128, 10000)"#,
            )
            .bind(book_id)
            .bind(i32::from(nova_ingest::dedup::DEDUP_ALGORITHM_VERSION))
            .bind(format!("source-{book_id}"))
            .bind(conservative_hash)
            .bind(&shared_layout_hash)
            .execute(&pool)
            .await
            .expect("insert exact content candidate fingerprint");
        }

        let repository = PgDuplicateRepository::new(pool.clone());
        let book_ids = [first_book_id, second_book_id];
        let layout_only = repository
            .deterministic_candidates(&book_ids, None, 50, 1, 1, 100, 100)
            .await
            .expect("load layout-only deterministic candidates");
        assert!(
            layout_only.exact_content.is_empty(),
            "layout normalization equality alone must not create an exact-content candidate"
        );

        sqlx::query("UPDATE book_fingerprints SET conservative_hash = $2 WHERE book_id = $1")
            .bind(second_book_id)
            .bind(&shared_conservative_hash)
            .execute(&pool)
            .await
            .expect("make conservative hashes equal");

        let conservative_match = repository
            .deterministic_candidates(&book_ids, None, 50, 1, 1, 100, 100)
            .await
            .expect("load conservative exact-content candidate");
        assert_eq!(conservative_match.exact_content.len(), 1);
        let mut expected_pair = book_ids;
        expected_pair.sort_unstable();
        assert_eq!(
            (
                conservative_match.exact_content[0].a,
                conservative_match.exact_content[0].b,
            ),
            (expected_pair[0], expected_pair[1]),
            "matching conservative hashes at the same normalization version must create the exact-content candidate"
        );

        sqlx::query("DELETE FROM books WHERE library_id = $1")
            .bind(library_id)
            .execute(&pool)
            .await
            .expect("remove exact content candidate books");
        sqlx::query("DELETE FROM libraries WHERE id = $1")
            .bind(library_id)
            .execute(&pool)
            .await
            .expect("remove exact content candidate library");
    }

    #[tokio::test]
    async fn shared_hash_candidate_query_rejects_pair_contributions_before_join() {
        let _write_guard = crate::dedup::DEDUP_DATABASE_TEST_LOCK.lock().await;
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let pool = sqlx::PgPool::connect(&database_url)
            .await
            .expect("connect to PostgreSQL for contribution guard test");
        crate::migrations::run_database_migrations(&pool)
            .await
            .expect("apply migrations for contribution guard test");
        let (library_id, first_book_id) =
            insert_dedup_test_book(&pool, "candidate-contribution").await;
        let mut book_ids = vec![first_book_id];
        for offset in 1..4 {
            let book_id = Uuid::now_v7();
            sqlx::query(
                r#"INSERT INTO books
                   (id, library_id, title, format, status, file_path, file_hash)
                   VALUES ($1, $2, $3, 'txt', 'ready', $4, $5)"#,
            )
            .bind(book_id)
            .bind(library_id)
            .bind(format!("Contribution guard book {offset}"))
            .bind(format!("/tmp/{book_id}.txt"))
            .bind(format!("candidate-contribution-{book_id}"))
            .execute(&pool)
            .await
            .expect("insert contribution guard book");
            book_ids.push(book_id);
        }

        let shared_hash = "a".repeat(64);
        for book_id in &book_ids {
            let chapter_id = Uuid::now_v7();
            sqlx::query(
                r#"INSERT INTO chapters
                   (id, book_id, index, chapter_index, title, content)
                   VALUES ($1, $2, 0, 0, 'Chapter', 'shared candidate text')"#,
            )
            .bind(chapter_id)
            .bind(book_id)
            .execute(&pool)
            .await
            .expect("insert contribution guard chapter");
            sqlx::query(
                r#"INSERT INTO chapter_fingerprints
                   (chapter_id, book_id, chapter_index, normalization_version,
                    source_content_hash, conservative_hash, layout_hash,
                    char_count, informative, winnowing_count)
                   VALUES ($1, $2, 0, 1, $3, $3, $3, 21, TRUE, 0)"#,
            )
            .bind(chapter_id)
            .bind(book_id)
            .bind(&shared_hash)
            .execute(&pool)
            .await
            .expect("insert contribution guard chapter fingerprint");
        }

        let repository = PgDuplicateRepository::new(pool.clone());
        let error = repository
            .deterministic_candidates(&book_ids, None, 50, 1, 4, 5, 100)
            .await
            .expect_err("six pair contributions must fail a five-contribution budget")
            .to_string();
        assert!(error.contains("shared chapter pair contribution budget exceeded"));
        assert!(error.contains("actual=6"));
        assert!(error.contains("limit=5"));

        sqlx::query("DELETE FROM books WHERE library_id = $1")
            .bind(library_id)
            .execute(&pool)
            .await
            .expect("remove contribution guard books");
        sqlx::query("DELETE FROM libraries WHERE id = $1")
            .bind(library_id)
            .execute(&pool)
            .await
            .expect("remove contribution guard library");
    }

    #[tokio::test]
    async fn fingerprint_replacement_rejects_stale_chapter_identity_snapshot() {
        let _write_guard = crate::dedup::DEDUP_DATABASE_TEST_LOCK.lock().await;
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let pool = sqlx::PgPool::connect(&database_url)
            .await
            .expect("connect to PostgreSQL for fingerprint snapshot test");
        crate::migrations::run_database_migrations(&pool)
            .await
            .expect("apply migrations for fingerprint snapshot test");
        let (library_id, book_id) = insert_dedup_test_book(&pool, "fingerprint-snapshot").await;
        let chapter_ids = [Uuid::now_v7(), Uuid::now_v7()];
        let content = "identical chapter content";
        for (chapter_index, chapter_id) in chapter_ids.iter().copied().enumerate() {
            let chapter_index = i32::try_from(chapter_index).expect("fixture index fits i32");
            sqlx::query(
                r#"INSERT INTO chapters
                   (id, book_id, index, chapter_index, title, content)
                   VALUES ($1, $2, $3, $3, 'Chapter', $4)"#,
            )
            .bind(chapter_id)
            .bind(book_id)
            .bind(chapter_index)
            .bind(content)
            .execute(&pool)
            .await
            .expect("insert fingerprint snapshot chapter");
        }

        let chapter_hash = nova_ingest::dedup::sha256(content.as_bytes()).to_hex();
        let source_content_hash =
            nova_ingest::dedup::source_content_hash([(0, content), (1, content)]).to_hex();
        let source_bytes = u64::try_from(content.len())
            .expect("fixture content length fits u64")
            .saturating_mul(2);
        let chapters: Vec<_> = chapter_ids
            .iter()
            .copied()
            .enumerate()
            .map(|(chapter_index, chapter_id)| ChapterFingerprintWrite {
                chapter_id,
                chapter_index: i32::try_from(chapter_index).expect("fixture index fits i32"),
                normalization_version: 1,
                source_content_hash: chapter_hash.clone(),
                conservative_hash: chapter_hash.clone(),
                layout_hash: chapter_hash.clone(),
                char_count: i64::try_from(content.chars().count())
                    .expect("fixture char count fits i64"),
                informative: true,
                winnowing_count: 0,
            })
            .collect();

        // Keep byte content and whole-book source hash unchanged while moving
        // the chapter identities to different indices. A whole-book hash-only
        // recheck cannot detect this stale cache write.
        sqlx::query("UPDATE chapters SET chapter_index = 2 WHERE id = $1")
            .bind(chapter_ids[0])
            .execute(&pool)
            .await
            .expect("move first chapter to temporary index");
        sqlx::query("UPDATE chapters SET chapter_index = 0 WHERE id = $1")
            .bind(chapter_ids[1])
            .execute(&pool)
            .await
            .expect("move second chapter to first index");
        sqlx::query("UPDATE chapters SET chapter_index = 1 WHERE id = $1")
            .bind(chapter_ids[0])
            .execute(&pool)
            .await
            .expect("move first chapter to second index");

        let repository = PgDuplicateRepository::new(pool.clone());
        let error = repository
            .replace_fingerprints(BookFingerprintWrite {
                book_id,
                expected_source_bytes: source_bytes,
                expected_chapter_count: 2,
                normalization_version: 1,
                layout_normalization_version: 1,
                algorithm_version: i32::from(nova_ingest::dedup::DEDUP_ALGORITHM_VERSION),
                source_content_hash,
                conservative_hash: chapter_hash.clone(),
                layout_hash: chapter_hash,
                chapter_count: 2,
                informative_chapter_count: 2,
                char_count: i64::try_from(content.chars().count().saturating_mul(2))
                    .expect("fixture char count fits i64"),
                text_integrity_bps: 10_000,
                chapters,
                passages: Vec::new(),
            })
            .await
            .expect_err("stale chapter identity/index tuple must reject fingerprint publication")
            .to_string();
        assert!(error.contains("chapter snapshot changed"));
        let fingerprint_exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM book_fingerprints WHERE book_id = $1)")
                .bind(book_id)
                .fetch_one(&pool)
                .await
                .expect("check rejected fingerprint publication");
        assert!(!fingerprint_exists);

        remove_dedup_test_book(&pool, library_id, book_id).await;
    }

    #[tokio::test]
    async fn algorithm_upgrade_supersedes_confirmed_pair_but_current_evidence_survives() {
        let _write_guard = crate::dedup::DEDUP_DATABASE_TEST_LOCK.lock().await;
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let pool = sqlx::PgPool::connect(&database_url)
            .await
            .expect("connect to PostgreSQL for duplicate publication test");
        crate::migrations::run_database_migrations(&pool)
            .await
            .expect("apply migrations for duplicate publication test");
        let mut tx = pool
            .begin()
            .await
            .expect("begin duplicate publication test transaction");

        let library_id = Uuid::now_v7();
        sqlx::query("INSERT INTO libraries (id, name, root_path) VALUES ($1, $2, $3)")
            .bind(library_id)
            .bind("Dedup algorithm upgrade test")
            .bind(format!("/tmp/nova-dedup-upgrade-{library_id}"))
            .execute(&mut *tx)
            .await
            .expect("insert duplicate publication test library");

        let mut book_ids: Vec<_> = (0..6).map(|_| Uuid::now_v7()).collect();
        book_ids.sort_unstable();
        for (index, book_id) in book_ids.iter().copied().enumerate() {
            sqlx::query(
                r#"INSERT INTO books
                   (id, library_id, title, format, status, file_path, file_hash)
                   VALUES ($1, $2, $3, 'txt', 'ready', $4, $5)"#,
            )
            .bind(book_id)
            .bind(library_id)
            .bind(format!("Upgrade fixture {index}"))
            .bind(format!("/tmp/{book_id}.txt"))
            .bind(format!("file-{book_id}"))
            .execute(&mut *tx)
            .await
            .expect("insert duplicate publication test book");
        }

        let current_algorithm = i32::from(nova_ingest::dedup::DEDUP_ALGORITHM_VERSION);
        let layout_hashes: Vec<_> = (0..book_ids.len())
            .map(|index| format!("{index:064x}"))
            .collect();
        for (book_id, layout_hash) in book_ids.iter().zip(&layout_hashes) {
            sqlx::query(
                r#"INSERT INTO book_fingerprints
                   (book_id, normalization_version, algorithm_version,
                    source_content_hash, conservative_hash, layout_hash,
                    chapter_count, informative_chapter_count, char_count,
                    text_integrity_bps)
                   VALUES ($1, 1, $2, $3, $3, $3, 0, 0, 0, 10000)"#,
            )
            .bind(book_id)
            .bind(current_algorithm)
            .bind(layout_hash)
            .execute(&mut *tx)
            .await
            .expect("insert current source-backed fingerprint");
        }

        let scan_id = Uuid::now_v7();
        sqlx::query(
            r#"INSERT INTO dedup_scan_runs
               (id, library_id, algorithm_version, status)
               VALUES ($1, $2, $3, 'running')"#,
        )
        .bind(scan_id)
        .bind(library_id)
        .bind(current_algorithm)
        .execute(&mut *tx)
        .await
        .expect("insert current duplicate scan");

        let preserved_pair = Uuid::now_v7();
        let upgraded_pair = Uuid::now_v7();
        let changed_pair = Uuid::now_v7();
        for (pair_id, pair_offset, algorithm_version, evidence_hash_a) in [
            (
                preserved_pair,
                0_usize,
                current_algorithm,
                layout_hashes[0].clone(),
            ),
            (
                upgraded_pair,
                2_usize,
                current_algorithm.saturating_sub(1),
                layout_hashes[2].clone(),
            ),
            (changed_pair, 4_usize, current_algorithm, "f".repeat(64)),
        ] {
            let evidence = serde_json::json!({
                "book_a_layout_hash": evidence_hash_a,
                "book_b_layout_hash": layout_hashes[pair_offset + 1],
                "algorithm_version": algorithm_version,
            });
            sqlx::query(
                r#"INSERT INTO duplicate_pairs
                   (id, book_a_id, book_b_id, similarity, method, relation,
                    review_status, resolved, resolution, algorithm_version,
                    evidence, stale)
                   VALUES ($1, $2, $3, 1, 'chapter_hash', 'exact_content',
                           'confirmed', TRUE, 'keep_b', $4, $5, FALSE)"#,
            )
            .bind(pair_id)
            .bind(book_ids[pair_offset])
            .bind(book_ids[pair_offset + 1])
            .bind(algorithm_version)
            .bind(evidence)
            .execute(&mut *tx)
            .await
            .expect("insert confirmed duplicate pair");
        }

        finalize_scan(
            &mut tx,
            scan_id,
            &book_ids,
            &book_ids,
            ScanPairCounts {
                found: 0,
                exact: 0,
                contained: 0,
                semantic: 0,
            },
        )
        .await
        .expect("finalize current duplicate scan");

        let pair_states: Vec<(Uuid, bool)> =
            sqlx::query_as("SELECT id, stale FROM duplicate_pairs WHERE id = ANY($1) ORDER BY id")
                .bind(&[preserved_pair, upgraded_pair, changed_pair])
                .fetch_all(&mut *tx)
                .await
                .expect("load duplicate pair publication states");
        let states: std::collections::HashMap<_, _> = pair_states.into_iter().collect();
        assert_eq!(states.get(&preserved_pair), Some(&false));
        assert_eq!(states.get(&upgraded_pair), Some(&true));
        assert_eq!(states.get(&changed_pair), Some(&true));

        tx.rollback()
            .await
            .expect("rollback duplicate publication test transaction");
    }
}
