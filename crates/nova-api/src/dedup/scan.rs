use std::collections::{BTreeSet, HashMap, HashSet};
use std::future::Future;

use nova_core::domain::dedup::{
    BookFingerprint, BookSide, ChapterFingerprint, ChapterInput, ChapterMatchKind,
    ClassificationThresholds, DedupScanPhase, DedupScanTask, DuplicateAlignmentGroupEvidence,
    DuplicateAlignmentMappingShape, DuplicateClassification, DuplicateEvidenceSchemaVersion,
    DuplicateMetrics, DuplicatePairEvidence, DuplicatePrimaryRecommendationEvidence,
    DuplicatePrimaryVersionEvidence, DuplicateRelation, DuplicateSemanticChapterMatch,
    DuplicateSemanticEvidence, NormalizationLevel, Sha256Hash, WinnowingConfig,
    WinnowingFingerprint,
};
use nova_ingest::dedup::{
    align_chapters, classify_metrics, classify_pair, fingerprint_book, sha256,
    source_content_hash as hash_source_chapters, winnow,
};
use serde_json::Value;
use uuid::Uuid;

use crate::repo::pg_duplicate::PgDuplicateRepository;
use crate::repo::pg_duplicate_scan::{
    BookFingerprintWrite, BoundedChapterContentRecords, BoundedScanChapterRecords,
    CachedFingerprintRecords, ChapterFingerprintWrite, DeterministicCandidateRecords,
    DuplicateBookPublicationSnapshot, DuplicateChapterMatchWrite, DuplicatePairWrite,
    EnqueueDuplicateScan, FinishDuplicateScan, PassageFingerprintWrite, PublishDuplicateScan,
    ScanBookRecord, ScanChapterRecord, ScanPairCounts, StoredBookFingerprintRecord,
    StoredChapterFingerprintRecord, StoredPassageFingerprintRecord,
};
use crate::state::AppState;

#[cfg(test)]
use crate::repo::pg_duplicate_scan::{
    duplicate_scan_payload as scan_payload, merge_scan_target_book_ids as merge_target_book_ids,
    ChapterContentRecord,
};

use super::{EmbeddingFreshnessContract, ALGORITHM_VERSION};

const MIN_INFORMATIVE_CHARS: u64 = 120;
const MIN_CHAPTER_CANDIDATES: i64 = 2;
const MIN_PASSAGE_CANDIDATES: i64 = 4;
const MAX_HASH_DOCUMENT_FREQUENCY: i64 = 50;
const MAX_PASSAGE_ANCHORS_PER_BOOK: usize = 8192;
const MAX_PASSAGE_ANCHORS_PER_CHAPTER: usize = 1024;
const MAX_CANDIDATE_PAIRS_PER_SCAN: usize = 100_000;
const MAX_CACHED_NORMALIZED_CHARS_PER_SCAN: u64 = 128_000_000;
const MAX_CACHED_NORMALIZED_CHARS_PER_BOOK: u64 = 8_000_000;
const MAX_CACHED_PASSAGE_ANCHORS_PER_SCAN: u64 = 2_000_000;
const MAX_CACHED_CHAPTER_FINGERPRINTS_PER_SCAN: u64 = 1_000_000;
const MAX_SEMANTIC_HITS_PER_SCAN: usize = 1_000_000;
const MAX_PAIR_CONTRIBUTIONS_PER_SCAN: i64 = 10_000_000;
const MAX_PAIR_CONTRIBUTIONS_PER_HASH_SOURCE: i64 = MAX_PAIR_CONTRIBUTIONS_PER_SCAN / 2;
const MAX_PAIR_VERIFICATION_CHARS_PER_SCAN: u128 = 2_000_000_000;
const MAX_TEXT_LOAD_BOOKS_PER_BATCH: usize = 64;
const MAX_TEXT_LOAD_CHAPTERS_PER_BATCH: usize = 4096;
const MAX_TEXT_LOAD_NORMALIZED_CHARS_PER_BATCH: u64 = 8_000_000;
const MAX_SOURCE_BYTES_PER_BOOK: u64 = 64_000_000;
const MAX_CHAPTERS_PER_BOOK: u64 = 100_000;
const MAX_TEXT_LOAD_SOURCE_BYTES_PER_BATCH: u64 = 64_000_000;
const PASSAGE_GRAM_SIZE: usize = 13;
const MAX_ANCHOR_OCCURRENCES_PER_HASH: usize = 8;
const MAX_VERIFIED_SEEDS_PER_PAIR: usize = 65_536;
const MAX_VERIFIED_SEGMENTS_PER_PAIR: usize = 8192;
const MIN_VERIFIED_SEGMENT_CHARS: usize = 48;
const MIN_EQUIVALENT_CHAPTER_SIMILARITY: f64 = 0.75;
#[cfg(test)]
const MAX_EXACT_DIFF_CHARS: usize = 20_000;
#[cfg(test)]
const DIFF_SAMPLE_WINDOWS: usize = 5;
#[cfg(test)]
const DIFF_SAMPLE_WINDOW_CHARS: usize = 4_096;
const MIN_SEMANTIC_SCORE: f64 = 0.90;
const MIN_SEMANTIC_CHUNK_MATCHES: usize = 2;
const MIN_SEMANTIC_CHAPTER_MATCHES: usize = 2;
const MIN_SEMANTIC_ORDER_SCORE: f64 = 0.75;
const MIN_SEMANTIC_SAMPLE_COVERAGE: f64 = 0.25;
const MAX_SEMANTIC_SOURCE_CHAPTERS: usize = 8;
const SEMANTIC_SOURCE_SCROLL_LIMIT: usize = 64;
const MAX_SEMANTIC_SOURCE_SCROLLS: usize = 2;
const SEMANTIC_SEARCH_LIMIT: usize = 64;

pub(crate) async fn enqueue_scan(
    state: &AppState,
    library_id: Option<Uuid>,
    requested_by: Uuid,
    include_semantic: bool,
) -> nova_core::Result<Uuid> {
    state
        .duplicates
        .enqueue_scan(EnqueueDuplicateScan {
            library_id,
            requested_by,
            include_semantic,
            algorithm_version: ALGORITHM_VERSION,
            target_book_ids: None,
        })
        .await
}

pub(crate) async fn enqueue_incremental_scan(
    state: &AppState,
    library_id: Uuid,
    requested_by: Uuid,
    book_ids: Vec<Uuid>,
) -> nova_core::Result<Uuid> {
    state
        .duplicates
        .enqueue_scan(EnqueueDuplicateScan {
            library_id: Some(library_id),
            requested_by,
            include_semantic: false,
            algorithm_version: ALGORITHM_VERSION,
            target_book_ids: Some(book_ids),
        })
        .await
}

type ScanBookRow = ScanBookRecord;
type ScanChapterRow = ScanChapterRecord;
type StoredBookFingerprintRow = StoredBookFingerprintRecord;
type StoredChapterFingerprintRow = StoredChapterFingerprintRecord;
type StoredPassageFingerprintRow = StoredPassageFingerprintRecord;

#[derive(Debug, Clone)]
struct StoredChapter {
    id: Uuid,
    chapter_index: i32,
}

#[derive(Debug, Clone)]
struct PassageAnchor {
    chapter_id: Uuid,
    chapter_index: i32,
    hash: u64,
    position: usize,
}

#[derive(Debug, Clone)]
struct StoredBook {
    book: ScanBookRow,
    chapters: Vec<StoredChapter>,
    fingerprint: BookFingerprint,
    source_content_hash: String,
    anchors: Vec<PassageAnchor>,
    text_integrity_bps: u32,
}

impl StoredBook {
    fn chapter_by_index(&self, index: u32) -> Option<&StoredChapter> {
        self.chapters
            .iter()
            .find(|chapter| u32::try_from(chapter.chapter_index).ok() == Some(index))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct CandidateKey {
    a: Uuid,
    b: Uuid,
}

impl CandidateKey {
    fn new(left: Uuid, right: Uuid) -> Option<Self> {
        if left == right {
            return None;
        }
        let (a, b) = if left < right {
            (left, right)
        } else {
            (right, left)
        };
        Some(Self { a, b })
    }
}

#[derive(Debug, Default, Clone)]
struct CandidateEvidence {
    exact_file: bool,
    exact_content: bool,
    shared_chapter_hashes: i32,
    shared_passage_hashes: i32,
    semantic: Option<SemanticCandidateEvidence>,
}

#[derive(Debug, Clone)]
struct SemanticChunkHit {
    source_book_id: Uuid,
    source_chapter_index: u32,
    source_chunk_id: String,
    target_book_id: Uuid,
    target_chapter_index: u32,
    target_chunk_id: String,
    score: f64,
}

#[derive(Debug, Clone)]
struct SemanticSourcePoint {
    chapter_index: u32,
    chunk_id: String,
    vector: Value,
}

#[derive(Debug, Clone)]
struct SemanticCandidateEvidence {
    score: f64,
    independent_chunk_matches: i32,
    independent_chapter_matches: i32,
    ordered_chapter_matches: Vec<DuplicateSemanticChapterMatch>,
    matched_chapters_a: i32,
    matched_chapters_b: i32,
    order_score: f64,
    sampled_chapters_a: i32,
    sampled_chapters_b: i32,
    sample_coverage_a: Option<f64>,
    sample_coverage_b: Option<f64>,
}

#[derive(Debug, Clone)]
struct PersistedMatch {
    a_index: u32,
    b_index: u32,
    match_type: &'static str,
    similarity: f64,
    shared_fingerprints: i32,
    alignment_group: Option<i32>,
    segment_ordinal: Option<i32>,
    range_a: Option<TextRange>,
    range_b: Option<TextRange>,
    matched_chars: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct TextRange {
    start: usize,
    end: usize,
}

impl TextRange {
    fn new(start: usize, end: usize) -> Option<Self> {
        (start < end).then_some(Self { start, end })
    }

    const fn len(self) -> usize {
        self.end.saturating_sub(self.start)
    }
}

#[derive(Debug, Clone)]
struct NormalizedChapterSpan {
    chapter_index: u32,
    global: TextRange,
}

#[derive(Debug, Clone)]
struct NormalizedBookText {
    content: Vec<char>,
    chapters: Vec<NormalizedChapterSpan>,
    anchors: Vec<(u64, usize)>,
}

#[derive(Debug)]
struct MatchableBookText<'a> {
    base: &'a NormalizedBookText,
    blocked: Vec<TextRange>,
}

#[derive(Debug, Default)]
struct NormalizedTextCache {
    books: HashMap<Uuid, NormalizedBookText>,
}

#[derive(Debug, PartialEq, Eq)]
struct VerificationPlan {
    text_book_ids: Vec<Uuid>,
    normalized_chars: u64,
    pair_verification_chars: u128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct VerifiedSegment {
    a: TextRange,
    b: TextRange,
}

#[derive(Debug, Default)]
struct GroupedPassageAlignment {
    matches: Vec<PersistedMatch>,
    order_score: f64,
    groups: Vec<DuplicateAlignmentGroupEvidence>,
}

#[derive(Debug)]
struct VerifiedPair {
    relation: DuplicateRelation,
    method: &'static str,
    confidence: f64,
    shared_chapters: i32,
    coverage_a: f64,
    coverage_b: f64,
    character_coverage_a: f64,
    character_coverage_b: f64,
    longest_run: i32,
    order_score: f64,
    contained_book_id: Option<Uuid>,
    recommended_primary_id: Option<Uuid>,
    semantic_score: Option<f64>,
    evidence: DuplicatePairEvidence,
    matches: Vec<PersistedMatch>,
}

#[derive(Debug, PartialEq, Eq)]
struct ScanScope {
    scope_book_ids: Vec<Uuid>,
    scannable_book_ids: Vec<Uuid>,
    affected_scope_book_ids: Vec<Uuid>,
    affected_scannable_book_ids: Vec<Uuid>,
    incremental: bool,
}

fn plan_scan_scope(
    mut scope_book_ids: Vec<Uuid>,
    mut scannable_book_ids: Vec<Uuid>,
    target_book_ids: Option<&[Uuid]>,
) -> ScanScope {
    scope_book_ids.sort_unstable();
    scope_book_ids.dedup();
    let scope_ids: HashSet<_> = scope_book_ids.iter().copied().collect();

    scannable_book_ids.retain(|id| scope_ids.contains(id));
    scannable_book_ids.sort_unstable();
    scannable_book_ids.dedup();

    let (affected_scope_book_ids, affected_scannable_book_ids) =
        if let Some(targets) = target_book_ids {
            let target_ids: HashSet<_> = targets.iter().copied().collect();
            (
                scope_book_ids
                    .iter()
                    .copied()
                    .filter(|id| target_ids.contains(id))
                    .collect(),
                scannable_book_ids
                    .iter()
                    .copied()
                    .filter(|id| target_ids.contains(id))
                    .collect(),
            )
        } else {
            (scope_book_ids.clone(), scannable_book_ids.clone())
        };

    ScanScope {
        scope_book_ids,
        scannable_book_ids,
        affected_scope_book_ids,
        affected_scannable_book_ids,
        incremental: target_book_ids.is_some(),
    }
}

pub(crate) async fn execute_scan(
    state: &AppState,
    task_id: Uuid,
    payload: &DedupScanTask,
) -> Result<Option<Value>, String> {
    execute_scan_inner(state, task_id, payload).await
}

async fn execute_scan_inner(
    state: &AppState,
    task_id: Uuid,
    payload: &DedupScanTask,
) -> Result<Option<Value>, String> {
    let scan_id = payload.scan_run_id;
    let library_id = payload.library_id;
    let include_semantic = payload.include_semantic;
    let target_book_ids = payload.target_book_ids.clone();

    let _execution_lock = state
        .duplicates
        .acquire_scan_execution_lock(scan_id, task_id, library_id)
        .await
        .map_err(|error| format!("failed to lock duplicate scan scope: {error}"))?;

    state
        .duplicates
        .mark_scan_running(scan_id, ALGORITHM_VERSION)
        .await
        .map_err(|error| format!("failed to start duplicate scan: {error}"))?;
    update_progress(
        state,
        task_id,
        scan_id,
        2,
        DedupScanPhase::Fingerprinting,
        0,
        0,
    )
    .await?;

    let scope_book_ids = state
        .duplicates
        .scan_scope_book_ids(library_id)
        .await
        .map_err(|error| format!("failed to load duplicate scan scope: {error}"))?;
    let books = state
        .duplicates
        .scan_books(library_id)
        .await
        .map_err(|error| format!("failed to load books for deduplication: {error}"))?;
    let scan_scope = plan_scan_scope(
        scope_book_ids,
        books.iter().map(|book| book.id).collect(),
        target_book_ids.as_deref(),
    );
    let ScanScope {
        scope_book_ids,
        scannable_book_ids,
        affected_scope_book_ids,
        affected_scannable_book_ids,
        incremental,
    } = scan_scope;
    if books.is_empty() {
        state
            .duplicates
            .finish_scan(FinishDuplicateScan {
                scan_id,
                scope_book_ids,
                affected_scope_book_ids,
                counts: ScanPairCounts {
                    found: 0,
                    exact: 0,
                    contained: 0,
                    semantic: 0,
                },
            })
            .await
            .map_err(|error| format!("failed to finalize duplicate scan: {error}"))?;
        return Ok(Some(serde_json::json!({
            "scan_run_id": scan_id,
            "books_processed": 0,
            "pairs_found": 0,
        })));
    }

    let valid_cached_ids =
        load_valid_cached_book_ids(&state.duplicates, &scannable_book_ids).await?;
    let stale_books: Vec<_> = books
        .iter()
        .filter(|book| !valid_cached_ids.contains(&book.id))
        .cloned()
        .collect();
    let affected_ids: HashSet<Uuid> = affected_scannable_book_ids.iter().copied().collect();
    let chapters_processed = books
        .iter()
        .filter(|book| affected_ids.contains(&book.id))
        .fold(0_i32, |total, book| {
            total.saturating_add(book.chapter_count.max(0))
        });
    for (offset, book) in stale_books.iter().cloned().enumerate() {
        // Persist immediately, then drop the in-memory chapter vectors. Only
        // books that actually form candidate pairs are loaded below, keeping
        // a 10k-book scan proportional to candidate density rather than corpus
        // chapter count.
        fingerprint_and_store_book(&state.duplicates, book).await?;
        let progress = 5_i16
            .saturating_add(i16::try_from(((offset + 1) * 40) / stale_books.len()).unwrap_or(40));
        update_progress(
            state,
            task_id,
            scan_id,
            progress,
            DedupScanPhase::Fingerprinting,
            i32::try_from((offset + 1).min(affected_scannable_book_ids.len())).unwrap_or(i32::MAX),
            chapters_processed,
        )
        .await?;
    }

    update_progress(
        state,
        task_id,
        scan_id,
        48,
        DedupScanPhase::CandidateGeneration,
        i32::try_from(affected_scannable_book_ids.len()).unwrap_or(i32::MAX),
        chapters_processed,
    )
    .await?;

    let incremental_targets = incremental.then_some(affected_scannable_book_ids.as_slice());
    let mut candidates =
        load_deterministic_candidates(&state.duplicates, &scannable_book_ids, incremental_targets)
            .await?;
    if include_semantic {
        let semantic_sources = incremental_targets.unwrap_or(&scannable_book_ids);
        let semantic =
            load_semantic_candidates(state, semantic_sources, &scannable_book_ids).await?;
        for (key, semantic_evidence) in semantic {
            let evidence = candidates.entry(key).or_default();
            evidence.semantic = Some(semantic_evidence);
            enforce_candidate_pair_budget(candidates.len())?;
        }
    }

    enforce_candidate_pair_budget(candidates.len())?;

    let passage_candidate_book_ids: HashSet<_> = candidates
        .iter()
        .filter(|(_, evidence)| requires_passage_verification(evidence))
        .flat_map(|(key, _)| [key.a, key.b])
        .collect();
    let mut passage_candidate_ids: Vec<_> = passage_candidate_book_ids.iter().copied().collect();
    passage_candidate_ids.sort_unstable();
    let passage_anchor_count = state
        .duplicates
        .passage_fingerprint_count(&passage_candidate_ids)
        .await
        .map_err(|error| format!("failed to count candidate passage anchors: {error}"))?;
    enforce_passage_anchor_budget(passage_anchor_count)?;

    let candidate_book_ids: HashSet<Uuid> =
        candidates.keys().flat_map(|key| [key.a, key.b]).collect();
    let mut candidate_ids: Vec<_> = candidate_book_ids.iter().copied().collect();
    candidate_ids.sort_unstable();
    let chapter_fingerprint_count = state
        .duplicates
        .chapter_fingerprint_count(&candidate_ids)
        .await
        .map_err(|error| format!("failed to count candidate chapter fingerprints: {error}"))?;
    enforce_chapter_fingerprint_budget(chapter_fingerprint_count)?;
    let candidate_books: Vec<_> = books
        .iter()
        .filter(|book| candidate_book_ids.contains(&book.id))
        .cloned()
        .collect();
    let mut stored_books = load_cached_books(
        &state.duplicates,
        &candidate_books,
        &passage_candidate_book_ids,
    )
    .await?;
    for book in candidate_books {
        if !stored_books.contains_key(&book.id) {
            let mut stored = fingerprint_and_store_book(&state.duplicates, book).await?;
            if !passage_candidate_book_ids.contains(&stored.book.id) {
                stored.anchors.clear();
            }
            stored_books.insert(stored.book.id, stored);
        }
    }

    let candidates_found = i32::try_from(candidates.len()).unwrap_or(i32::MAX);
    state
        .duplicates
        .record_scan_candidates(scan_id, candidates_found)
        .await
        .map_err(|error| format!("failed to record candidates: {error}"))?;

    let mut candidates: Vec<_> = candidates.into_iter().collect();
    candidates.sort_by_key(|(key, _)| (key.a, key.b));
    let verification_plan = plan_verification_resources(&candidates, &stored_books)?;
    let text_books: Vec<_> = verification_plan
        .text_book_ids
        .iter()
        .map(|book_id| {
            stored_books
                .get(book_id)
                .ok_or_else(|| format!("missing stored fingerprint for {book_id}"))
        })
        .collect::<Result<_, _>>()?;
    let normalized_texts = load_normalized_text_cache(&state.duplicates, &text_books).await?;

    let mut pair_count = 0_i32;
    let mut exact_count = 0_i32;
    let mut contained_count = 0_i32;
    let mut semantic_count = 0_i32;
    let mut verified_pairs = Vec::new();
    let total_candidates = candidates.len().max(1);

    for (offset, (key, evidence)) in candidates.into_iter().enumerate() {
        let Some(book_a) = stored_books.get(&key.a) else {
            continue;
        };
        let Some(book_b) = stored_books.get(&key.b) else {
            continue;
        };

        if let Some(verified) = verify_pair(book_a, book_b, &evidence, &normalized_texts)? {
            pair_count = pair_count.saturating_add(1);
            match verified.relation {
                DuplicateRelation::ExactFile | DuplicateRelation::ExactContent => {
                    exact_count = exact_count.saturating_add(1);
                }
                DuplicateRelation::ContainedVersion => {
                    contained_count = contained_count.saturating_add(1);
                }
                DuplicateRelation::SemanticRelation => {
                    semantic_count = semantic_count.saturating_add(1);
                }
                _ => {}
            }
            verified_pairs.push((key, verified));
        }

        if offset % 10 == 0 || offset + 1 == total_candidates {
            let progress = 50_i16.saturating_add(
                i16::try_from(((offset + 1) * 45) / total_candidates).unwrap_or(45),
            );
            update_pair_progress(
                state,
                task_id,
                scan_id,
                progress,
                ScanPairCounts {
                    found: pair_count,
                    exact: exact_count,
                    contained: contained_count,
                    semantic: semantic_count,
                },
            )
            .await?;
        }
    }

    publish_scan_results(
        &state.duplicates,
        scan_id,
        &scope_book_ids,
        &affected_scope_book_ids,
        &stored_books,
        verified_pairs,
        pair_count,
        exact_count,
        contained_count,
        semantic_count,
    )
    .await?;

    Ok(Some(serde_json::json!({
        "scan_run_id": scan_id,
        "books_processed": affected_scannable_book_ids.len(),
        "chapters_processed": chapters_processed,
        "candidates_found": candidates_found,
        "pairs_found": pair_count,
        "exact_pairs": exact_count,
        "contained_pairs": contained_count,
        "semantic_pairs": semantic_count,
    })))
}

async fn load_valid_cached_book_ids(
    repository: &PgDuplicateRepository,
    book_ids: &[Uuid],
) -> Result<HashSet<Uuid>, String> {
    repository
        .valid_cached_book_ids(
            book_ids,
            ALGORITHM_VERSION,
            i32::from(nova_ingest::dedup::CONSERVATIVE_NORMALIZATION_VERSION),
            i32::from(nova_ingest::dedup::LAYOUT_NORMALIZATION_VERSION),
        )
        .await
        .map_err(|error| format!("failed to inspect cached book fingerprints: {error}"))
}

async fn load_cached_books(
    repository: &PgDuplicateRepository,
    books: &[ScanBookRow],
    passage_book_ids: &HashSet<Uuid>,
) -> Result<HashMap<Uuid, StoredBook>, String> {
    let book_ids: Vec<Uuid> = books.iter().map(|book| book.id).collect();
    if book_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let cached_ids: Vec<_> = load_valid_cached_book_ids(repository, &book_ids)
        .await?
        .into_iter()
        .collect();
    if cached_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let passage_cached_ids: Vec<_> = cached_ids
        .iter()
        .copied()
        .filter(|book_id| passage_book_ids.contains(book_id))
        .collect();
    let CachedFingerprintRecords {
        books: book_rows,
        chapters: chapter_rows,
        passages: passage_rows,
    } = repository
        .cached_fingerprints(
            &cached_ids,
            &passage_cached_ids,
            i32::from(nova_ingest::dedup::CONSERVATIVE_NORMALIZATION_VERSION),
            i32::from(nova_ingest::dedup::LAYOUT_NORMALIZATION_VERSION),
        )
        .await
        .map_err(|error| format!("failed to load cached fingerprints: {error}"))?;

    let books_by_id: HashMap<Uuid, ScanBookRow> =
        books.iter().cloned().map(|book| (book.id, book)).collect();
    let mut chapters_by_book: HashMap<Uuid, Vec<StoredChapterFingerprintRow>> = HashMap::new();
    for row in chapter_rows {
        chapters_by_book.entry(row.book_id).or_default().push(row);
    }
    let mut passages_by_book: HashMap<Uuid, Vec<StoredPassageFingerprintRow>> = HashMap::new();
    for row in passage_rows {
        passages_by_book.entry(row.book_id).or_default().push(row);
    }
    let mut cached = HashMap::with_capacity(book_rows.len());
    for book_row in book_rows {
        let Some(book) = books_by_id.get(&book_row.book_id).cloned() else {
            continue;
        };
        let chapter_rows = chapters_by_book
            .remove(&book_row.book_id)
            .unwrap_or_default();
        let passage_rows = passages_by_book
            .remove(&book_row.book_id)
            .unwrap_or_default();
        if let Some(stored) = assemble_cached_book(book, book_row, chapter_rows, passage_rows) {
            cached.insert(stored.book.id, stored);
        }
    }
    Ok(cached)
}

fn assemble_cached_book(
    book: ScanBookRow,
    book_row: StoredBookFingerprintRow,
    chapter_rows: Vec<StoredChapterFingerprintRow>,
    passage_rows: Vec<StoredPassageFingerprintRow>,
) -> Option<StoredBook> {
    let (Ok(conservative_hash), Ok(layout_hash), Ok(char_count), Ok(text_integrity_bps)) = (
        Sha256Hash::from_hex(&book_row.conservative_hash),
        Sha256Hash::from_hex(&book_row.layout_hash),
        u64::try_from(book_row.char_count),
        u32::try_from(book_row.text_integrity_bps),
    ) else {
        return None;
    };

    let mut chapters = Vec::with_capacity(chapter_rows.len());
    let mut chapter_fingerprints = Vec::with_capacity(chapter_rows.len());
    for row in chapter_rows {
        let (Ok(chapter_index), Ok(char_count), Ok(conservative_hash), Ok(layout_hash)) = (
            u32::try_from(row.chapter_index),
            u64::try_from(row.char_count),
            Sha256Hash::from_hex(&row.conservative_hash),
            Sha256Hash::from_hex(&row.layout_hash),
        ) else {
            return None;
        };
        chapters.push(StoredChapter {
            id: row.id,
            chapter_index: row.chapter_index,
        });
        chapter_fingerprints.push(ChapterFingerprint {
            chapter_index,
            conservative_hash,
            layout_hash,
            char_count,
            conservative_normalization_version:
                nova_ingest::dedup::CONSERVATIVE_NORMALIZATION_VERSION,
            layout_normalization_version: nova_ingest::dedup::LAYOUT_NORMALIZATION_VERSION,
        });
    }

    let mut anchors = Vec::with_capacity(passage_rows.len());
    for row in passage_rows {
        let Ok(position) = usize::try_from(row.position) else {
            return None;
        };
        anchors.push(PassageAnchor {
            chapter_id: row.chapter_id,
            chapter_index: row.chapter_index,
            hash: u64::from_ne_bytes(row.fingerprint_hash.to_ne_bytes()),
            position,
        });
    }

    Some(StoredBook {
        book,
        chapters,
        fingerprint: BookFingerprint {
            conservative_hash,
            layout_hash,
            char_count,
            conservative_normalization_version:
                nova_ingest::dedup::CONSERVATIVE_NORMALIZATION_VERSION,
            layout_normalization_version: nova_ingest::dedup::LAYOUT_NORMALIZATION_VERSION,
            chapters: chapter_fingerprints,
        },
        source_content_hash: book_row.source_content_hash,
        anchors,
        text_integrity_bps,
    })
}

async fn fingerprint_and_store_book(
    repository: &PgDuplicateRepository,
    book: ScanBookRow,
) -> Result<StoredBook, String> {
    let BoundedScanChapterRecords {
        source_bytes,
        chapter_count,
        chapters,
    } = repository
        .scan_chapters_bounded(book.id, MAX_SOURCE_BYTES_PER_BOOK, MAX_CHAPTERS_PER_BOOK)
        .await
        .map_err(|error| {
            format!(
                "failed to load bounded source text for {}: {error}",
                book.id
            )
        })?;
    let loaded_source_bytes = chapters.iter().try_fold(0_u64, |total, chapter| {
        u64::try_from(chapter.content.len())
            .map(|length| total.saturating_add(length))
            .map_err(|_| format!("source chapter size overflow for {}", book.id))
    })?;
    if loaded_source_bytes != source_bytes {
        return Err(format!(
            "bounded source text snapshot mismatch for {}: query_bytes={source_bytes}, loaded_bytes={loaded_source_bytes}",
            book.id,
        ));
    }
    let loaded_chapter_count = u64::try_from(chapters.len()).unwrap_or(u64::MAX);
    if loaded_chapter_count != chapter_count {
        return Err(format!(
            "bounded chapter row snapshot mismatch for {}: query_count={chapter_count}, loaded_count={loaded_chapter_count}",
            book.id,
        ));
    }

    let inputs: Vec<ChapterInput<'_>> = chapters
        .iter()
        .map(|chapter| {
            let chapter_index = u32::try_from(chapter.chapter_index).map_err(|_| {
                format!(
                    "invalid negative chapter index for book {}, chapter {}: {}",
                    book.id, chapter.id, chapter.chapter_index
                )
            })?;
            Ok(ChapterInput {
                chapter_index,
                content: &chapter.content,
            })
        })
        .collect::<Result<_, String>>()?;
    let fingerprint = fingerprint_book(&inputs);
    if fingerprint.char_count > MAX_CACHED_NORMALIZED_CHARS_PER_BOOK {
        return Err(format!(
            "duplicate normalized text per-book budget exceeded for {}: actual_chars={}, limit_chars={MAX_CACHED_NORMALIZED_CHARS_PER_BOOK}",
            book.id, fingerprint.char_count
        ));
    }
    let source_content_hash = source_content_hash(&chapters);
    let text_integrity_bps = text_integrity_basis_points(&chapters);
    let anchors = select_passage_anchors(&chapters);
    let anchor_counts = anchors.iter().fold(HashMap::new(), |mut counts, anchor| {
        let count = counts.entry(anchor.chapter_id).or_insert(0_usize);
        *count = count.saturating_add(1);
        counts
    });
    let chapter_writes = fingerprint
        .chapters
        .iter()
        .zip(&chapters)
        .map(|(chapter_fingerprint, chapter)| {
            let chapter_index = u32::try_from(chapter.chapter_index).map_err(|_| {
                format!(
                    "invalid negative chapter index for book {}, chapter {}: {}",
                    book.id, chapter.id, chapter.chapter_index
                )
            })?;
            if chapter_fingerprint.chapter_index != chapter_index {
                return Err(format!(
                    "fingerprint chapter order mismatch for book {}, chapter {}",
                    book.id, chapter.id
                ));
            }
            Ok(ChapterFingerprintWrite {
                chapter_id: chapter.id,
                chapter_index: chapter.chapter_index,
                normalization_version: i32::from(
                    nova_ingest::dedup::CONSERVATIVE_NORMALIZATION_VERSION,
                ),
                source_content_hash: sha256(chapter.content.as_bytes()).to_hex(),
                conservative_hash: chapter_fingerprint.conservative_hash.to_hex(),
                layout_hash: chapter_fingerprint.layout_hash.to_hex(),
                char_count: i64::try_from(chapter_fingerprint.char_count).unwrap_or(i64::MAX),
                informative: chapter_fingerprint.char_count >= MIN_INFORMATIVE_CHARS,
                winnowing_count: i32::try_from(
                    anchor_counts.get(&chapter.id).copied().unwrap_or_default(),
                )
                .unwrap_or(i32::MAX),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let passage_writes = anchors
        .iter()
        .map(|anchor| PassageFingerprintWrite {
            chapter_id: anchor.chapter_id,
            normalization_version: i32::from(nova_ingest::dedup::LAYOUT_NORMALIZATION_VERSION),
            fingerprint_hash: i64::from_ne_bytes(anchor.hash.to_ne_bytes()),
            position: i32::try_from(anchor.position).unwrap_or(i32::MAX),
            span_length: 13,
        })
        .collect();
    repository
        .replace_fingerprints(BookFingerprintWrite {
            book_id: book.id,
            expected_source_bytes: source_bytes,
            expected_chapter_count: chapter_count,
            normalization_version: i32::from(
                nova_ingest::dedup::CONSERVATIVE_NORMALIZATION_VERSION,
            ),
            layout_normalization_version: i32::from(
                nova_ingest::dedup::LAYOUT_NORMALIZATION_VERSION,
            ),
            algorithm_version: ALGORITHM_VERSION,
            source_content_hash: source_content_hash.clone(),
            conservative_hash: fingerprint.conservative_hash.to_hex(),
            layout_hash: fingerprint.layout_hash.to_hex(),
            chapter_count: i32::try_from(fingerprint.chapters.len()).unwrap_or(i32::MAX),
            informative_chapter_count: i32::try_from(
                fingerprint
                    .chapters
                    .iter()
                    .filter(|chapter| chapter.char_count >= MIN_INFORMATIVE_CHARS)
                    .count(),
            )
            .unwrap_or(i32::MAX),
            char_count: i64::try_from(fingerprint.char_count).unwrap_or(i64::MAX),
            text_integrity_bps: i32::try_from(text_integrity_bps).unwrap_or(10_000),
            chapters: chapter_writes,
            passages: passage_writes,
        })
        .await
        .map_err(|error| format!("failed to replace fingerprints for {}: {error}", book.id))?;

    let stored_chapters = chapters
        .into_iter()
        .map(|chapter| StoredChapter {
            id: chapter.id,
            chapter_index: chapter.chapter_index,
        })
        .collect();

    Ok(StoredBook {
        book,
        chapters: stored_chapters,
        fingerprint,
        source_content_hash,
        anchors,
        text_integrity_bps,
    })
}

fn text_integrity_basis_points(chapters: &[ScanChapterRow]) -> u32 {
    const MOJIBAKE_MARKERS: [&str; 6] = ["ï¿½", "锟斤拷", "â€™", "â€œ", "â€", "ÃÂ"];

    let mut visible_chars = 0_usize;
    let mut suspicious_chars = 0_usize;
    for chapter in chapters {
        for character in chapter.content.chars() {
            if !character.is_whitespace() {
                visible_chars = visible_chars.saturating_add(1);
            }
            if character == '\u{fffd}'
                || (character.is_control() && !character.is_whitespace())
                || matches!(character as u32, 0xE000..=0xF8FF | 0xF0000..=0x10FFFF)
            {
                suspicious_chars = suspicious_chars.saturating_add(1);
            }
        }
        for marker in MOJIBAKE_MARKERS {
            let marker_chars = marker.chars().count();
            suspicious_chars = suspicious_chars.saturating_add(
                chapter
                    .content
                    .matches(marker)
                    .count()
                    .saturating_mul(marker_chars),
            );
        }
    }
    if visible_chars == 0 {
        return 0;
    }
    let suspicious_bps = suspicious_chars
        .saturating_mul(10_000)
        .checked_div(visible_chars)
        .unwrap_or(10_000)
        .min(10_000);
    u32::try_from(10_000_usize.saturating_sub(suspicious_bps)).unwrap_or_default()
}

fn source_content_hash(chapters: &[ScanChapterRow]) -> String {
    hash_source_chapters(
        chapters
            .iter()
            .map(|chapter| (chapter.chapter_index, chapter.content.as_str())),
    )
    .to_hex()
}

fn select_passage_anchors(chapters: &[ScanChapterRow]) -> Vec<PassageAnchor> {
    let config = WinnowingConfig {
        gram_size: 13,
        window_size: 32,
    };
    let informative_chapters: Vec<_> = chapters
        .iter()
        .filter(|chapter| {
            chapter.content.chars().count() >= usize::try_from(MIN_INFORMATIVE_CHARS).unwrap_or(120)
        })
        .collect();
    if informative_chapters.is_empty() {
        return Vec::new();
    }

    // For pathological books with more chapters than the book-level budget,
    // sample chapters uniformly instead of silently keeping only the prefix.
    let selected_chapters: Vec<_> = if informative_chapters.len() > MAX_PASSAGE_ANCHORS_PER_BOOK {
        (0..MAX_PASSAGE_ANCHORS_PER_BOOK)
            .filter_map(|slot| {
                let index =
                    slot.saturating_mul(informative_chapters.len()) / MAX_PASSAGE_ANCHORS_PER_BOOK;
                informative_chapters.get(index).copied()
            })
            .collect()
    } else {
        informative_chapters
    };
    // Spend more of the fixed book budget on books parsed as one or a few huge
    // chapters. This keeps small shared regions observable inside merged text
    // without letting any book exceed the persisted 8k-anchor ceiling.
    let per_chapter_limit = (MAX_PASSAGE_ANCHORS_PER_BOOK / selected_chapters.len())
        .clamp(1, MAX_PASSAGE_ANCHORS_PER_CHAPTER);
    let mut anchors = Vec::new();
    for chapter in selected_chapters {
        let Ok(fingerprints) = winnow(&chapter.content, NormalizationLevel::Layout, config) else {
            continue;
        };
        for index in coverage_sample_fingerprint_indices(&fingerprints, per_chapter_limit) {
            let Some(fingerprint) = fingerprints.get(index) else {
                continue;
            };
            anchors.push(PassageAnchor {
                chapter_id: chapter.id,
                chapter_index: chapter.chapter_index,
                hash: fingerprint.hash,
                position: fingerprint.position,
            });
        }
    }
    anchors.sort_by_key(|anchor| (anchor.chapter_index, anchor.position));
    anchors
}

/// Sample Winnowing output by normalized character position, not by fingerprint
/// ordinal: low-entropy text can emit far denser minima than ordinary prose.
/// A quarter of the budget protects each edge while the remainder covers the
/// whole chapter uniformly.
fn coverage_sample_fingerprint_indices(
    fingerprints: &[WinnowingFingerprint],
    limit: usize,
) -> Vec<usize> {
    if fingerprints.len() <= limit {
        return (0..fingerprints.len()).collect();
    }
    if limit == 0 {
        return Vec::new();
    }

    let edge_budget = (limit / 4).max(1).min(limit / 2);
    let uniform_budget = limit.saturating_sub(edge_budget.saturating_mul(2));
    let mut selected = BTreeSet::new();
    selected.extend(0..edge_budget.min(fingerprints.len()));
    selected.extend(fingerprints.len().saturating_sub(edge_budget)..fingerprints.len());
    if uniform_budget > 0 {
        let span = fingerprints
            .last()
            .map_or(1_u128, |fingerprint| fingerprint.position as u128 + 1);
        let denominator = (uniform_budget as u128).saturating_mul(2);
        for slot in 0..uniform_budget {
            let target = (slot as u128)
                .saturating_mul(2)
                .saturating_add(1)
                .saturating_mul(span)
                / denominator;
            let insertion =
                fingerprints.partition_point(|fingerprint| (fingerprint.position as u128) < target);
            let index = match (insertion.checked_sub(1), fingerprints.get(insertion)) {
                (Some(previous), Some(next)) => {
                    let previous_distance =
                        target.abs_diff(fingerprints[previous].position as u128);
                    let next_distance = (next.position as u128).abs_diff(target);
                    if previous_distance <= next_distance {
                        previous
                    } else {
                        insertion
                    }
                }
                (Some(previous), None) => previous,
                (None, Some(_)) => insertion,
                (None, None) => continue,
            };
            selected.insert(index);
        }
    }
    for index in coverage_sample_indices(fingerprints.len(), limit) {
        selected.insert(index);
        if selected.len() >= limit {
            break;
        }
    }
    selected.into_iter().take(limit).collect()
}

/// Bounded deterministic sampling for an already position-ordered list.
fn coverage_sample_indices(length: usize, limit: usize) -> Vec<usize> {
    if length <= limit {
        return (0..length).collect();
    }
    if limit == 0 {
        return Vec::new();
    }

    let edge_budget = (limit / 4).max(1).min(limit / 2);
    let uniform_budget = limit.saturating_sub(edge_budget.saturating_mul(2));
    let mut selected = BTreeSet::new();
    selected.extend(0..edge_budget.min(length));
    selected.extend(length.saturating_sub(edge_budget)..length);
    if uniform_budget > 0 {
        let denominator = (uniform_budget as u128).saturating_mul(2);
        for slot in 0..uniform_budget {
            let numerator = (slot as u128)
                .saturating_mul(2)
                .saturating_add(1)
                .saturating_mul(length as u128);
            let index = usize::try_from(numerator / denominator)
                .unwrap_or(length.saturating_sub(1))
                .min(length.saturating_sub(1));
            selected.insert(index);
        }
    }
    if selected.len() < limit {
        for index in 0..length {
            selected.insert(index);
            if selected.len() >= limit {
                break;
            }
        }
    }
    selected.into_iter().take(limit).collect()
}

async fn load_deterministic_candidates(
    repository: &PgDuplicateRepository,
    book_ids: &[Uuid],
    target_book_ids: Option<&[Uuid]>,
) -> Result<HashMap<CandidateKey, CandidateEvidence>, String> {
    let mut candidates: HashMap<CandidateKey, CandidateEvidence> = HashMap::new();
    let DeterministicCandidateRecords {
        exact_file,
        exact_content,
        shared_chapters,
        shared_passages,
    } = repository
        .deterministic_candidates(
            book_ids,
            target_book_ids,
            MAX_HASH_DOCUMENT_FREQUENCY,
            MIN_CHAPTER_CANDIDATES,
            MIN_PASSAGE_CANDIDATES,
            MAX_PAIR_CONTRIBUTIONS_PER_HASH_SOURCE,
            i64::try_from(MAX_CANDIDATE_PAIRS_PER_SCAN.saturating_add(1)).unwrap_or(i64::MAX),
        )
        .await
        .map_err(|error| format!("failed to find deterministic candidates: {error}"))?;

    for (kind, count) in [
        ("exact_file", exact_file.len()),
        ("exact_content", exact_content.len()),
        ("shared_chapters", shared_chapters.len()),
        ("shared_passages", shared_passages.len()),
    ] {
        if count > MAX_CANDIDATE_PAIRS_PER_SCAN {
            return Err(format!(
                "duplicate {kind} candidate budget exceeded: actual_at_least={count}, limit={MAX_CANDIDATE_PAIRS_PER_SCAN}"
            ));
        }
    }

    for pair in exact_file {
        if let Some(key) = CandidateKey::new(pair.a, pair.b) {
            candidates.entry(key).or_default().exact_file = true;
            enforce_candidate_pair_budget(candidates.len())?;
        }
    }
    for pair in exact_content {
        if let Some(key) = CandidateKey::new(pair.a, pair.b) {
            candidates.entry(key).or_default().exact_content = true;
            enforce_candidate_pair_budget(candidates.len())?;
        }
    }
    for pair in shared_chapters {
        if let Some(key) = CandidateKey::new(pair.a, pair.b) {
            candidates.entry(key).or_default().shared_chapter_hashes =
                i32::try_from(pair.shared).unwrap_or(i32::MAX);
            enforce_candidate_pair_budget(candidates.len())?;
        }
    }
    for pair in shared_passages {
        if let Some(key) = CandidateKey::new(pair.a, pair.b) {
            candidates.entry(key).or_default().shared_passage_hashes =
                i32::try_from(pair.shared).unwrap_or(i32::MAX);
            enforce_candidate_pair_budget(candidates.len())?;
        }
    }

    Ok(candidates)
}

fn enforce_candidate_pair_budget(candidate_pairs: usize) -> Result<(), String> {
    if candidate_pairs > MAX_CANDIDATE_PAIRS_PER_SCAN {
        Err(format!(
            "duplicate candidate pair budget exceeded: actual={candidate_pairs}, limit={MAX_CANDIDATE_PAIRS_PER_SCAN}"
        ))
    } else {
        Ok(())
    }
}

fn enforce_passage_anchor_budget(passage_anchors: u64) -> Result<(), String> {
    if passage_anchors > MAX_CACHED_PASSAGE_ANCHORS_PER_SCAN {
        Err(format!(
            "duplicate passage anchor cache budget exceeded: actual={passage_anchors}, limit={MAX_CACHED_PASSAGE_ANCHORS_PER_SCAN}"
        ))
    } else {
        Ok(())
    }
}

fn enforce_chapter_fingerprint_budget(chapter_fingerprints: u64) -> Result<(), String> {
    if chapter_fingerprints > MAX_CACHED_CHAPTER_FINGERPRINTS_PER_SCAN {
        Err(format!(
            "duplicate chapter fingerprint cache budget exceeded: actual={chapter_fingerprints}, limit={MAX_CACHED_CHAPTER_FINGERPRINTS_PER_SCAN}"
        ))
    } else {
        Ok(())
    }
}

fn plan_verification_resources(
    candidates: &[(CandidateKey, CandidateEvidence)],
    stored_books: &HashMap<Uuid, StoredBook>,
) -> Result<VerificationPlan, String> {
    enforce_candidate_pair_budget(candidates.len())?;
    let mut text_book_ids = HashSet::new();
    let mut pair_verification_chars = 0_u128;
    for (key, evidence) in candidates {
        let book_a = stored_books
            .get(&key.a)
            .ok_or_else(|| format!("missing stored fingerprint for {}", key.a))?;
        let book_b = stored_books
            .get(&key.b)
            .ok_or_else(|| format!("missing stored fingerprint for {}", key.b))?;
        if !requires_passage_verification(evidence)
            || book_a.anchors.is_empty()
            || book_b.anchors.is_empty()
        {
            continue;
        }
        text_book_ids.insert(key.a);
        text_book_ids.insert(key.b);
        pair_verification_chars = pair_verification_chars
            .saturating_add(u128::from(book_a.fingerprint.char_count))
            .saturating_add(u128::from(book_b.fingerprint.char_count));
    }
    if pair_verification_chars > MAX_PAIR_VERIFICATION_CHARS_PER_SCAN {
        return Err(format!(
            "duplicate pair verification character budget exceeded: actual={pair_verification_chars}, limit={MAX_PAIR_VERIFICATION_CHARS_PER_SCAN}"
        ));
    }

    let mut text_book_ids: Vec<_> = text_book_ids.into_iter().collect();
    text_book_ids.sort_unstable();
    let normalized_chars = text_book_ids.iter().try_fold(0_u64, |total, book_id| {
        let book = stored_books
            .get(book_id)
            .ok_or_else(|| format!("missing stored fingerprint for {book_id}"))?;
        if book.fingerprint.char_count > MAX_CACHED_NORMALIZED_CHARS_PER_BOOK {
            return Err(format!(
                "duplicate normalized text per-book budget exceeded for {book_id}: actual_chars={}, limit_chars={MAX_CACHED_NORMALIZED_CHARS_PER_BOOK}",
                book.fingerprint.char_count
            ));
        }
        Ok::<_, String>(total.saturating_add(book.fingerprint.char_count))
    })?;
    if normalized_chars > MAX_CACHED_NORMALIZED_CHARS_PER_SCAN {
        return Err(format!(
            "duplicate normalized text cache budget exceeded: actual_chars={normalized_chars}, limit_chars={MAX_CACHED_NORMALIZED_CHARS_PER_SCAN}"
        ));
    }

    Ok(VerificationPlan {
        text_book_ids,
        normalized_chars,
        pair_verification_chars,
    })
}

fn requires_passage_verification(evidence: &CandidateEvidence) -> bool {
    i64::from(evidence.shared_passage_hashes) >= MIN_PASSAGE_CANDIDATES
}

async fn load_normalized_text_cache(
    repository: &PgDuplicateRepository,
    books: &[&StoredBook],
) -> Result<NormalizedTextCache, String> {
    let book_ids: Vec<_> = books.iter().map(|book| book.book.id).collect();
    let source_sizes: HashMap<_, _> = repository
        .chapter_content_sizes(&book_ids)
        .await
        .map_err(|error| format!("failed to inspect candidate source text sizes: {error}"))?
        .into_iter()
        .map(|(book_id, source_bytes)| {
            u64::try_from(source_bytes)
                .map(|source_bytes| (book_id, source_bytes))
                .map_err(|_| format!("invalid source text size for {book_id}: {source_bytes}"))
        })
        .collect::<Result<_, _>>()?;
    for book in books {
        let source_bytes = source_sizes.get(&book.book.id).copied().unwrap_or_default();
        if source_bytes > MAX_SOURCE_BYTES_PER_BOOK {
            return Err(format!(
                "duplicate source text per-book budget exceeded for {}: actual_bytes={source_bytes}, limit_bytes={MAX_SOURCE_BYTES_PER_BOOK}",
                book.book.id
            ));
        }
    }
    load_normalized_text_cache_with(
        books,
        &source_sizes,
        |chapter_ids, remaining_source_bytes| async move {
            repository
                .chapter_contents_bounded(&chapter_ids, remaining_source_bytes)
                .await
                .map_err(|error| format!("failed to load bounded candidate source text: {error}"))
        },
    )
    .await
}

async fn load_normalized_text_cache_with<F, Fut>(
    books: &[&StoredBook],
    source_sizes: &HashMap<Uuid, u64>,
    mut load_chapters: F,
) -> Result<NormalizedTextCache, String>
where
    F: FnMut(Vec<Uuid>, u64) -> Fut,
    Fut: Future<Output = Result<BoundedChapterContentRecords, String>>,
{
    let mut cache = NormalizedTextCache {
        books: HashMap::with_capacity(books.len()),
    };
    for batch in normalized_text_load_batches(books, source_sizes) {
        let chapter_ids: Vec<_> = batch
            .iter()
            .flat_map(|book| book.chapters.iter().map(|chapter| chapter.id))
            .collect();
        let mut contents = HashMap::new();
        let mut remaining_source_bytes = MAX_TEXT_LOAD_SOURCE_BYTES_PER_BATCH;
        for chapter_chunk in chapter_ids.chunks(MAX_TEXT_LOAD_CHAPTERS_PER_BATCH) {
            let BoundedChapterContentRecords {
                source_bytes,
                chapters,
            } = load_chapters(chapter_chunk.to_vec(), remaining_source_bytes).await?;
            if source_bytes > remaining_source_bytes {
                return Err(format!(
                    "bounded candidate source loader exceeded its contract: actual_bytes={source_bytes}, limit_bytes={remaining_source_bytes}"
                ));
            }
            let loaded_source_bytes = chapters.iter().try_fold(0_u64, |total, chapter| {
                u64::try_from(chapter.content.len())
                    .map(|length| total.saturating_add(length))
                    .map_err(|_| format!("candidate chapter {} size overflow", chapter.id))
            })?;
            if loaded_source_bytes != source_bytes {
                return Err(format!(
                    "bounded candidate source snapshot mismatch: query_bytes={source_bytes}, loaded_bytes={loaded_source_bytes}"
                ));
            }
            remaining_source_bytes = remaining_source_bytes.saturating_sub(source_bytes);
            contents.extend(chapters.into_iter().map(|row| (row.id, row.content)));
        }
        for book in batch {
            let mut source_chapters = book.chapters.clone();
            source_chapters.sort_by_key(|chapter| chapter.chapter_index);
            let loaded_source_hash =
                hash_source_chapters(source_chapters.iter().filter_map(|chapter| {
                    contents
                        .get(&chapter.id)
                        .map(|content| (chapter.chapter_index, content.as_str()))
                }))
                .to_hex();
            if loaded_source_hash != book.source_content_hash {
                return Err(format!(
                    "candidate source text changed before verification for {}; retry scan",
                    book.book.id
                ));
            }
            let normalized = normalized_book_text(book, &contents)?;
            let normalized_chars = u64::try_from(normalized.content.len()).unwrap_or(u64::MAX);
            if normalized_chars != book.fingerprint.char_count {
                return Err(format!(
                    "candidate source text changed during duplicate scan for {}: expected_chars={}, actual_chars={normalized_chars}",
                    book.book.id, book.fingerprint.char_count
                ));
            }
            cache.books.insert(book.book.id, normalized);
        }
    }
    Ok(cache)
}

fn normalized_text_load_batches<'a>(
    books: &[&'a StoredBook],
    source_sizes: &HashMap<Uuid, u64>,
) -> Vec<Vec<&'a StoredBook>> {
    let mut batches = Vec::new();
    let mut current = Vec::new();
    let mut current_chars = 0_u64;
    let mut current_source_bytes = 0_u64;
    for &book in books {
        let next_chars = current_chars.saturating_add(book.fingerprint.char_count);
        let book_source_bytes = source_sizes.get(&book.book.id).copied().unwrap_or_default();
        let next_source_bytes = current_source_bytes.saturating_add(book_source_bytes);
        if !current.is_empty()
            && (current.len() >= MAX_TEXT_LOAD_BOOKS_PER_BATCH
                || next_chars > MAX_TEXT_LOAD_NORMALIZED_CHARS_PER_BATCH
                || next_source_bytes > MAX_TEXT_LOAD_SOURCE_BYTES_PER_BATCH)
        {
            batches.push(std::mem::take(&mut current));
            current_chars = 0;
            current_source_bytes = 0;
        }
        current.push(book);
        current_chars = current_chars.saturating_add(book.fingerprint.char_count);
        current_source_bytes = current_source_bytes.saturating_add(book_source_bytes);
    }
    if !current.is_empty() {
        batches.push(current);
    }
    batches
}

fn verify_pair(
    book_a: &StoredBook,
    book_b: &StoredBook,
    evidence: &CandidateEvidence,
    normalized_texts: &NormalizedTextCache,
) -> Result<Option<VerifiedPair>, String> {
    // Candidate generation and cache repair are separate database phases. A
    // content/file update can invalidate an exact candidate and cause the book
    // to be re-fingerprinted before verification, so exact flags must be
    // revalidated against the StoredBook snapshots that publication will use.
    let mut verified_evidence = evidence.clone();
    verified_evidence.exact_file = evidence.exact_file
        && !book_a.book.file_hash.is_empty()
        && book_a.book.file_hash == book_b.book.file_hash;
    verified_evidence.exact_content = evidence.exact_content
        && book_a.fingerprint.char_count > 0
        && book_b.fingerprint.char_count > 0
        && book_a.fingerprint.conservative_normalization_version
            == book_b.fingerprint.conservative_normalization_version
        && book_a.fingerprint.conservative_hash == book_b.fingerprint.conservative_hash;
    let evidence = &verified_evidence;

    let alignment = align_chapters(&book_a.fingerprint, &book_b.fingerprint);
    let classification = classify_pair(
        &book_a.fingerprint,
        &book_b.fingerprint,
        &alignment,
        &ClassificationThresholds::default(),
    );
    let mut matches: Vec<PersistedMatch> = alignment
        .matches
        .iter()
        .filter_map(|chapter_match| {
            let range_a = full_chapter_range(book_a, chapter_match.a_index)?;
            let range_b = full_chapter_range(book_b, chapter_match.b_index)?;
            Some(PersistedMatch {
                a_index: chapter_match.a_index,
                b_index: chapter_match.b_index,
                match_type: match chapter_match.kind {
                    ChapterMatchKind::Conservative => "conservative",
                    ChapterMatchKind::Layout => "layout",
                },
                similarity: 1.0,
                shared_fingerprints: 0,
                alignment_group: None,
                segment_ordinal: None,
                range_a: Some(range_a),
                range_b: Some(range_b),
                matched_chars: i32::try_from(range_a.len().min(range_b.len())).unwrap_or(i32::MAX),
            })
        })
        .collect();

    let grouped = if requires_passage_verification(evidence) {
        verified_passage_chapter_matches(normalized_texts, book_a, book_b, &matches)?
    } else {
        GroupedPassageAlignment::default()
    };
    let GroupedPassageAlignment {
        matches: near_matches,
        order_score: grouped_order_score,
        groups: boundary_groups,
    } = grouped;
    matches.extend(near_matches);
    matches.sort_by_key(|item| (item.a_index, item.b_index));

    let combined_order_score = match (
        alignment.matches.is_empty(),
        matches.iter().all(|item| item.match_type != "winnowing"),
    ) {
        (false, false) => alignment.order_score.min(grouped_order_score),
        (false, true) => alignment.order_score,
        (true, false) => grouped_order_score,
        (true, true) => 0.0,
    };
    let combined = combined_metrics(book_a, book_b, &matches, combined_order_score);
    let mut final_classification = final_deterministic_classification(
        classification,
        classify_combined(&combined, book_a, book_b),
        evidence,
    );

    if final_classification.relation == DuplicateRelation::NotDuplicate
        && evidence.semantic.is_some()
    {
        final_classification = DuplicateClassification {
            relation: DuplicateRelation::SemanticRelation,
            contained: None,
        };
    }
    if final_classification.relation == DuplicateRelation::NotDuplicate {
        return Ok(None);
    }

    let contained_book_id = if final_classification.relation == DuplicateRelation::ContainedVersion
    {
        match final_classification.contained {
            Some(BookSide::A) => Some(book_a.book.id),
            Some(BookSide::B) => Some(book_b.book.id),
            None if combined.coverage_a > combined.coverage_b => Some(book_a.book.id),
            None if combined.coverage_b > combined.coverage_a => Some(book_b.book.id),
            None => None,
        }
    } else {
        None
    };
    let recommended_primary_id = matches!(
        final_classification.relation,
        DuplicateRelation::ExactFile
            | DuplicateRelation::ExactContent
            | DuplicateRelation::ContainedVersion
            | DuplicateRelation::HighOverlap
    )
    .then(|| preferred_primary_version(book_a, book_b));
    let primary_evidence_a = public_primary_version_evidence(book_a);
    let primary_evidence_b = public_primary_version_evidence(book_b);

    let confidence = confidence_for(final_classification.relation, &combined, evidence);
    let method = if evidence.exact_file {
        "file_hash"
    } else if evidence.exact_content {
        "content_hash"
    } else if combined.near_matches > 0 {
        "winnowing"
    } else if combined.shared_chapters > 0 {
        "chapter_hash"
    } else {
        "semantic"
    };

    let semantic_score = evidence.semantic.as_ref().map(|semantic| semantic.score);
    let semantic_hits = evidence
        .semantic
        .as_ref()
        .map_or(0, |semantic| semantic.independent_chunk_matches);
    let semantic_evidence = evidence.semantic.as_ref().map(|semantic| {
        let chapter_count_a = book_a.fingerprint.chapters.len();
        let chapter_count_b = book_b.fingerprint.chapters.len();
        DuplicateSemanticEvidence {
            score: semantic.score,
            independent_chunk_matches: semantic.independent_chunk_matches,
            independent_chapter_matches: semantic.independent_chapter_matches,
            ordered_chapter_matches: semantic.ordered_chapter_matches.clone(),
            matched_chapters_a: semantic.matched_chapters_a,
            matched_chapters_b: semantic.matched_chapters_b,
            order_score: semantic.order_score,
            sampled_chapters_a: semantic.sampled_chapters_a,
            sampled_chapters_b: semantic.sampled_chapters_b,
            sample_coverage_a: semantic.sample_coverage_a,
            sample_coverage_b: semantic.sample_coverage_b,
            book_chapters_a: chapter_count_a,
            book_chapters_b: chapter_count_b,
            observed_book_coverage_a: ratio(
                usize::try_from(semantic.matched_chapters_a).unwrap_or_default(),
                chapter_count_a,
            ),
            observed_book_coverage_b: ratio(
                usize::try_from(semantic.matched_chapters_b).unwrap_or_default(),
                chapter_count_b,
            ),
        }
    });

    Ok(Some(VerifiedPair {
        relation: final_classification.relation,
        method,
        confidence,
        shared_chapters: combined.shared_chapters,
        coverage_a: combined.coverage_a,
        coverage_b: combined.coverage_b,
        character_coverage_a: combined.character_coverage_a,
        character_coverage_b: combined.character_coverage_b,
        longest_run: combined.longest_run,
        order_score: combined.order_score,
        contained_book_id,
        recommended_primary_id,
        semantic_score,
        evidence: DuplicatePairEvidence {
            schema_version: DuplicateEvidenceSchemaVersion::V2,
            exact_file: evidence.exact_file,
            exact_content: evidence.exact_content,
            shared_chapter_hashes: evidence.shared_chapter_hashes,
            shared_passage_hashes: evidence.shared_passage_hashes,
            semantic_hits,
            semantic: semantic_evidence,
            primary_recommendation: Some(DuplicatePrimaryRecommendationEvidence {
                recommended_book_id: recommended_primary_id,
                unique_informative_content_dominates: true,
                reader_assets_considered: false,
                book_a: primary_evidence_a,
                book_b: primary_evidence_b,
            }),
            equivalent_chapters: combined.equivalent_chapters,
            matched_chapters_a: combined.matched_chapters_a,
            matched_chapters_b: combined.matched_chapters_b,
            shared_characters: combined.shared_chars,
            unique_characters_a: combined.unique_chars_a,
            unique_characters_b: combined.unique_chars_b,
            alignment_schema_version: 2,
            chapter_boundary_groups: boundary_groups,
            unique_chapters_a: combined.unique_a,
            unique_chapters_b: combined.unique_b,
            book_a_layout_hash: book_a.fingerprint.layout_hash.to_hex(),
            book_b_layout_hash: book_b.fingerprint.layout_hash.to_hex(),
            algorithm_version: ALGORITHM_VERSION,
        },
        matches,
    }))
}

/// Exact whole-file/content evidence is authoritative. Every other relation
/// must be classified from the complete verified evidence set, including
/// source-checked Winnowing matches. Keeping an earlier partial classification
/// would under-classify mixed exact/near containment (for example 8 exact
/// chapters plus 2 chapters with removable watermarks).
fn final_deterministic_classification(
    chapter_hash_classification: DuplicateClassification,
    combined_classification: DuplicateClassification,
    evidence: &CandidateEvidence,
) -> DuplicateClassification {
    if evidence.exact_file {
        DuplicateClassification {
            relation: DuplicateRelation::ExactFile,
            contained: None,
        }
    } else if evidence.exact_content
        || chapter_hash_classification.relation == DuplicateRelation::ExactContent
    {
        DuplicateClassification {
            relation: DuplicateRelation::ExactContent,
            contained: None,
        }
    } else {
        combined_classification
    }
}

#[derive(Debug)]
struct CombinedMetrics {
    shared_chapters: i32,
    equivalent_chapters: i32,
    matched_chapters_a: i32,
    matched_chapters_b: i32,
    near_matches: i32,
    verified_passage_fingerprints: usize,
    equivalent_longest_run: i32,
    coverage_a: f64,
    coverage_b: f64,
    character_coverage_a: f64,
    character_coverage_b: f64,
    longest_run: i32,
    order_score: f64,
    shared_chars: u64,
    total_chars_a: u64,
    total_chars_b: u64,
    unique_chars_a: u64,
    unique_chars_b: u64,
    unique_a: Vec<u32>,
    unique_b: Vec<u32>,
}

fn combined_metrics(
    book_a: &StoredBook,
    book_b: &StoredBook,
    matches: &[PersistedMatch],
    order_score: f64,
) -> CombinedMetrics {
    let informative_matches: Vec<&PersistedMatch> = matches
        .iter()
        .filter(|item| {
            book_a
                .fingerprint
                .chapters
                .iter()
                .find(|chapter| chapter.chapter_index == item.a_index)
                .is_some_and(|chapter| chapter.char_count >= MIN_INFORMATIVE_CHARS)
                && book_b
                    .fingerprint
                    .chapters
                    .iter()
                    .find(|chapter| chapter.chapter_index == item.b_index)
                    .is_some_and(|chapter| chapter.char_count >= MIN_INFORMATIVE_CHARS)
        })
        .collect();
    let informative_a: Vec<_> = book_a
        .fingerprint
        .chapters
        .iter()
        .filter(|chapter| chapter.char_count >= MIN_INFORMATIVE_CHARS)
        .collect();
    let informative_b: Vec<_> = book_b
        .fingerprint
        .chapters
        .iter()
        .filter(|chapter| chapter.char_count >= MIN_INFORMATIVE_CHARS)
        .collect();
    let a_coverage = chapter_coverage(book_a, &informative_matches, true);
    let b_coverage = chapter_coverage(book_b, &informative_matches, false);
    let matched_chars_a: f64 = a_coverage
        .values()
        .map(|coverage| coverage.matched_chars)
        .sum();
    let matched_chars_b: f64 = b_coverage
        .values()
        .map(|coverage| coverage.matched_chars)
        .sum();
    let total_chars_a: u64 = informative_a.iter().map(|chapter| chapter.char_count).sum();
    let total_chars_b: u64 = informative_b.iter().map(|chapter| chapter.char_count).sum();
    let metric_matches: Vec<PersistedMatch> = informative_matches.into_iter().cloned().collect();
    let matched_indices_a = covered_chapter_indices(&a_coverage, 0.35);
    let matched_indices_b = covered_chapter_indices(&b_coverage, 0.35);
    let equivalent_indices_a =
        covered_chapter_indices(&a_coverage, MIN_EQUIVALENT_CHAPTER_SIMILARITY);
    let equivalent_indices_b =
        covered_chapter_indices(&b_coverage, MIN_EQUIVALENT_CHAPTER_SIMILARITY);
    let matched_chapters_a = equivalent_indices_a.len();
    let matched_chapters_b = equivalent_indices_b.len();
    let equivalent = matched_chapters_a.max(matched_chapters_b);
    let shared = matched_indices_a.len().max(matched_indices_b.len());
    let shared_chars = matched_chars_a.min(matched_chars_b).max(0.0) as u64;

    CombinedMetrics {
        shared_chapters: i32::try_from(shared).unwrap_or(i32::MAX),
        equivalent_chapters: i32::try_from(equivalent).unwrap_or(i32::MAX),
        matched_chapters_a: i32::try_from(matched_chapters_a).unwrap_or(i32::MAX),
        matched_chapters_b: i32::try_from(matched_chapters_b).unwrap_or(i32::MAX),
        near_matches: i32::try_from(
            metric_matches
                .iter()
                .filter(|item| item.match_type == "winnowing")
                .count(),
        )
        .unwrap_or(i32::MAX),
        verified_passage_fingerprints: metric_matches
            .iter()
            .filter(|item| item.match_type == "winnowing")
            .filter_map(|item| usize::try_from(item.shared_fingerprints).ok())
            .sum(),
        equivalent_longest_run: i32::try_from(
            longest_consecutive_chapter_run(&equivalent_indices_a)
                .max(longest_consecutive_chapter_run(&equivalent_indices_b)),
        )
        .unwrap_or(i32::MAX),
        coverage_a: average_chapter_coverage(&informative_a, &a_coverage),
        coverage_b: average_chapter_coverage(&informative_b, &b_coverage),
        character_coverage_a: ratio_f64(matched_chars_a, total_chars_a),
        character_coverage_b: ratio_f64(matched_chars_b, total_chars_b),
        longest_run: i32::try_from(
            longest_consecutive_chapter_run(&matched_indices_a)
                .max(longest_consecutive_chapter_run(&matched_indices_b)),
        )
        .unwrap_or(i32::MAX),
        order_score: order_score.clamp(0.0, 1.0),
        shared_chars,
        total_chars_a,
        total_chars_b,
        unique_chars_a: total_chars_a.saturating_sub(matched_chars_a.max(0.0) as u64),
        unique_chars_b: total_chars_b.saturating_sub(matched_chars_b.max(0.0) as u64),
        unique_a: informative_a
            .iter()
            .filter(|chapter| {
                a_coverage
                    .get(&chapter.chapter_index)
                    .is_none_or(|coverage| coverage.fraction < 0.35)
            })
            .map(|chapter| chapter.chapter_index)
            .collect(),
        unique_b: informative_b
            .iter()
            .filter(|chapter| {
                b_coverage
                    .get(&chapter.chapter_index)
                    .is_none_or(|coverage| coverage.fraction < 0.35)
            })
            .map(|chapter| chapter.chapter_index)
            .collect(),
    }
}

#[derive(Debug, Clone, Copy)]
struct ChapterCoverageValue {
    matched_chars: f64,
    fraction: f64,
}

fn full_chapter_range(book: &StoredBook, chapter_index: u32) -> Option<TextRange> {
    let length = book
        .fingerprint
        .chapters
        .iter()
        .find(|chapter| chapter.chapter_index == chapter_index)?
        .char_count;
    TextRange::new(0, usize::try_from(length).ok()?)
}

fn chapter_coverage(
    book: &StoredBook,
    matches: &[&PersistedMatch],
    side_a: bool,
) -> HashMap<u32, ChapterCoverageValue> {
    let mut weighted_ranges: HashMap<u32, Vec<(TextRange, f64)>> = HashMap::new();
    for item in matches {
        let (index, range) = if side_a {
            (item.a_index, item.range_a)
        } else {
            (item.b_index, item.range_b)
        };
        let range = range.or_else(|| full_chapter_range(book, index));
        if let Some(range) = range {
            weighted_ranges
                .entry(index)
                .or_default()
                .push((range, item.similarity.clamp(0.0, 1.0)));
        }
    }

    let mut coverage = HashMap::new();
    for chapter in &book.fingerprint.chapters {
        let Some(ranges) = weighted_ranges.get(&chapter.chapter_index) else {
            continue;
        };
        let chapter_chars = chapter.char_count as f64;
        if chapter_chars <= 0.0 {
            continue;
        }
        // Selected global segments never reuse text on either side. The cap is
        // a defensive guard for legacy/test rows and floating-point rounding.
        let matched_chars = ranges
            .iter()
            .map(|(range, weight)| range.len() as f64 * weight)
            .sum::<f64>()
            .min(chapter_chars);
        coverage.insert(
            chapter.chapter_index,
            ChapterCoverageValue {
                matched_chars,
                fraction: (matched_chars / chapter_chars).clamp(0.0, 1.0),
            },
        );
    }
    coverage
}

fn covered_chapter_indices(
    coverage: &HashMap<u32, ChapterCoverageValue>,
    threshold: f64,
) -> Vec<u32> {
    let mut indices: Vec<_> = coverage
        .iter()
        .filter_map(|(&index, value)| (value.fraction >= threshold).then_some(index))
        .collect();
    indices.sort_unstable();
    indices
}

fn average_chapter_coverage(
    chapters: &[&ChapterFingerprint],
    coverage: &HashMap<u32, ChapterCoverageValue>,
) -> f64 {
    ratio_f64(
        chapters
            .iter()
            .map(|chapter| {
                coverage
                    .get(&chapter.chapter_index)
                    .map_or(0.0, |value| value.fraction)
            })
            .sum(),
        chapters.len(),
    )
}

fn longest_consecutive_chapter_run(indices: &[u32]) -> usize {
    let mut best = 0_usize;
    let mut current = 0_usize;
    let mut previous = None;
    for &index in indices {
        current = match previous {
            Some(value) if index == value + 1 => current + 1,
            _ => 1,
        };
        best = best.max(current);
        previous = Some(index);
    }
    best
}

fn verified_passage_chapter_matches(
    normalized_texts: &NormalizedTextCache,
    book_a: &StoredBook,
    book_b: &StoredBook,
    existing: &[PersistedMatch],
) -> Result<GroupedPassageAlignment, String> {
    if book_a.anchors.is_empty() || book_b.anchors.is_empty() {
        return Ok(GroupedPassageAlignment::default());
    }

    let normalized_a = normalized_texts
        .books
        .get(&book_a.book.id)
        .ok_or_else(|| format!("missing normalized candidate text for {}", book_a.book.id))?;
    let normalized_b = normalized_texts
        .books
        .get(&book_b.book.id)
        .ok_or_else(|| format!("missing normalized candidate text for {}", book_b.book.id))?;
    let used_a: HashSet<u32> = existing.iter().map(|item| item.a_index).collect();
    let used_b: HashSet<u32> = existing.iter().map(|item| item.b_index).collect();
    let matchable_a = MatchableBookText::new(normalized_a, &used_a);
    let matchable_b = MatchableBookText::new(normalized_b, &used_b);
    let segments = verified_global_segments(&matchable_a, &matchable_b);
    if segments.is_empty() {
        return Ok(GroupedPassageAlignment::default());
    }
    let (selected, order_score) =
        select_grouped_passage_segments(normalized_a, normalized_b, existing, &segments)?;
    let (matches, groups) = segment_chapter_mappings(normalized_a, normalized_b, &selected);
    Ok(GroupedPassageAlignment {
        matches,
        order_score,
        groups,
    })
}

/// Exact chapter matches are authoritative fixed anchors. Passage matches may
/// fill gaps between them, but a passage that crosses an exact anchor would
/// make two independently ordered sub-alignments look globally ordered. Keep
/// only passage segments that are compatible with every exact anchor, then
/// compute the weighted non-crossing chain over the combined evidence.
fn select_grouped_passage_segments(
    book_a: &NormalizedBookText,
    book_b: &NormalizedBookText,
    exact_matches: &[PersistedMatch],
    passage_segments: &[VerifiedSegment],
) -> Result<(Vec<VerifiedSegment>, f64), String> {
    let mut exact_segments: Vec<_> = exact_matches
        .iter()
        .map(|item| persisted_match_global_segment(book_a, book_b, item))
        .collect::<Result<_, _>>()?;
    exact_segments.sort_by_key(|segment| (segment.a.start, segment.b.start));
    if exact_segments
        .windows(2)
        .any(|pair| !segments_are_non_crossing(&pair[0], &pair[1]))
    {
        return Err("exact chapter alignment is not globally non-crossing".to_string());
    }
    let compatible_passages: Vec<_> = passage_segments
        .iter()
        .copied()
        .filter(|passage| passage_is_compatible_with_exact(&exact_segments, passage))
        .collect();

    let exact_set: HashSet<_> = exact_segments.iter().copied().collect();
    let mut combined = exact_segments;
    combined.extend(compatible_passages);
    let (selected, order_score) = maximum_non_crossing_segments(&combined);
    let passages = selected
        .into_iter()
        .filter(|segment| !exact_set.contains(segment))
        .collect();
    Ok((passages, order_score))
}

fn persisted_match_global_segment(
    book_a: &NormalizedBookText,
    book_b: &NormalizedBookText,
    item: &PersistedMatch,
) -> Result<VerifiedSegment, String> {
    let chapter_a = book_a
        .chapters
        .iter()
        .find(|chapter| chapter.chapter_index == item.a_index)
        .ok_or_else(|| format!("missing normalized chapter A {}", item.a_index))?;
    let chapter_b = book_b
        .chapters
        .iter()
        .find(|chapter| chapter.chapter_index == item.b_index)
        .ok_or_else(|| format!("missing normalized chapter B {}", item.b_index))?;
    let local_a = item
        .range_a
        .ok_or_else(|| format!("exact chapter A {} has no text range", item.a_index))?;
    let local_b = item
        .range_b
        .ok_or_else(|| format!("exact chapter B {} has no text range", item.b_index))?;
    let global_a = local_to_global_range(chapter_a, local_a)
        .ok_or_else(|| format!("invalid exact chapter A {} text range", item.a_index))?;
    let global_b = local_to_global_range(chapter_b, local_b)
        .ok_or_else(|| format!("invalid exact chapter B {} text range", item.b_index))?;
    if global_a.len() != global_b.len() {
        return Err(format!(
            "exact chapter mapping {}:{} has unequal normalized ranges",
            item.a_index, item.b_index
        ));
    }
    Ok(VerifiedSegment {
        a: global_a,
        b: global_b,
    })
}

fn local_to_global_range(chapter: &NormalizedChapterSpan, local: TextRange) -> Option<TextRange> {
    let start = chapter.global.start.checked_add(local.start)?;
    let end = chapter.global.start.checked_add(local.end)?;
    (end <= chapter.global.end)
        .then(|| TextRange::new(start, end))
        .flatten()
}

fn segments_are_non_crossing(left: &VerifiedSegment, right: &VerifiedSegment) -> bool {
    (left.a.end <= right.a.start && left.b.end <= right.b.start)
        || (right.a.end <= left.a.start && right.b.end <= left.b.start)
}

fn passage_is_compatible_with_exact(
    exact_segments: &[VerifiedSegment],
    passage: &VerifiedSegment,
) -> bool {
    let next = exact_segments.partition_point(|exact| exact.a.end <= passage.a.start);
    if let Some(previous) = next
        .checked_sub(1)
        .and_then(|index| exact_segments.get(index))
    {
        if previous.b.end > passage.b.start {
            return false;
        }
    }
    if let Some(following) = exact_segments.get(next) {
        if following.a.start < passage.a.end || passage.b.end > following.b.start {
            return false;
        }
    }
    true
}

fn normalized_book_text(
    book: &StoredBook,
    contents: &HashMap<Uuid, String>,
) -> Result<NormalizedBookText, String> {
    let mut stored_chapters = book.chapters.clone();
    stored_chapters.sort_by_key(|chapter| chapter.chapter_index);
    let mut content = Vec::new();
    let mut chapters = Vec::with_capacity(stored_chapters.len());
    for chapter in stored_chapters {
        let raw = contents
            .get(&chapter.id)
            .ok_or_else(|| format!("missing source content for chapter {}", chapter.id))?;
        let Ok(chapter_index) = u32::try_from(chapter.chapter_index) else {
            continue;
        };
        let normalized: Vec<char> = nova_ingest::dedup::normalize_layout(raw).chars().collect();
        let start = content.len();
        content.extend(normalized);
        let end = content.len();
        if let Some(global) = TextRange::new(start, end) {
            chapters.push(NormalizedChapterSpan {
                chapter_index,
                global,
            });
        }
    }

    let spans_by_index: HashMap<i32, TextRange> = chapters
        .iter()
        .filter_map(|chapter| {
            i32::try_from(chapter.chapter_index)
                .ok()
                .map(|index| (index, chapter.global))
        })
        .collect();
    let anchors = book
        .anchors
        .iter()
        .filter_map(|anchor| {
            let span = spans_by_index.get(&anchor.chapter_index)?;
            let position = span.start.checked_add(anchor.position)?;
            (position.saturating_add(PASSAGE_GRAM_SIZE) <= span.end)
                .then_some((anchor.hash, position))
        })
        .collect();

    Ok(NormalizedBookText {
        content,
        chapters,
        anchors,
    })
}

impl<'a> MatchableBookText<'a> {
    fn new(base: &'a NormalizedBookText, excluded_chapters: &HashSet<u32>) -> Self {
        let blocked = base
            .chapters
            .iter()
            .filter(|chapter| excluded_chapters.contains(&chapter.chapter_index))
            .map(|chapter| chapter.global)
            .collect();
        Self { base, blocked }
    }

    fn allowed_range(&self, position: usize) -> Option<TextRange> {
        if position >= self.base.content.len() {
            return None;
        }
        let next = self
            .blocked
            .partition_point(|blocked| blocked.end <= position);
        if self
            .blocked
            .get(next)
            .is_some_and(|blocked| blocked.start <= position)
        {
            return None;
        }
        let start = next
            .checked_sub(1)
            .and_then(|index| self.blocked.get(index))
            .map_or(0, |blocked| blocked.end);
        let end = self
            .blocked
            .get(next)
            .map_or(self.base.content.len(), |blocked| blocked.start);
        TextRange::new(start, end)
    }

    fn gram_is_matchable(&self, position: usize) -> bool {
        let end = position.saturating_add(PASSAGE_GRAM_SIZE);
        self.allowed_range(position)
            .is_some_and(|allowed| end <= allowed.end)
    }
}

#[cfg(test)]
fn verified_text_similarity(
    content_a: &str,
    content_b: &str,
    chars_a: &[char],
    chars_b: &[char],
) -> f64 {
    if chars_a.is_empty() || chars_b.is_empty() {
        return 0.0;
    }
    if chars_a.len().max(chars_b.len()) <= MAX_EXACT_DIFF_CHARS {
        return f64::from(similar::TextDiff::from_chars(content_a, content_b).ratio());
    }

    let length_ratio =
        (2.0 * chars_a.len().min(chars_b.len()) as f64) / (chars_a.len() + chars_b.len()) as f64;
    let mut sampled_ratio = 0.0_f64;
    for slot in 0..DIFF_SAMPLE_WINDOWS {
        let sample_a = proportional_window(chars_a, slot, DIFF_SAMPLE_WINDOWS);
        let sample_b = proportional_window(chars_b, slot, DIFF_SAMPLE_WINDOWS);
        let sample_a: String = sample_a.iter().collect();
        let sample_b: String = sample_b.iter().collect();
        sampled_ratio += f64::from(similar::TextDiff::from_chars(&sample_a, &sample_b).ratio());
    }
    (sampled_ratio / DIFF_SAMPLE_WINDOWS as f64 * length_ratio).clamp(0.0, 1.0)
}

#[cfg(test)]
fn proportional_window(characters: &[char], slot: usize, slots: usize) -> &[char] {
    let window_len = characters.len().min(DIFF_SAMPLE_WINDOW_CHARS);
    let available = characters.len().saturating_sub(window_len);
    let denominator = slots.saturating_sub(1).max(1);
    let start = available.saturating_mul(slot) / denominator;
    &characters[start..start + window_len]
}

fn verified_global_segments(
    book_a: &MatchableBookText<'_>,
    book_b: &MatchableBookText<'_>,
) -> Vec<VerifiedSegment> {
    let mut anchors_a: HashMap<u64, Vec<usize>> = HashMap::new();
    let mut anchors_b: HashMap<u64, Vec<usize>> = HashMap::new();
    for &(hash, position) in &book_a.base.anchors {
        if book_a.gram_is_matchable(position) {
            anchors_a.entry(hash).or_default().push(position);
        }
    }
    for &(hash, position) in &book_b.base.anchors {
        if book_b.gram_is_matchable(position) {
            anchors_b.entry(hash).or_default().push(position);
        }
    }

    let mut seeds = Vec::new();
    'hashes: for (hash, positions_a) in anchors_a {
        let Some(positions_b) = anchors_b.get(&hash) else {
            continue;
        };
        let sampled_a: Vec<_> =
            coverage_sample_indices(positions_a.len(), MAX_ANCHOR_OCCURRENCES_PER_HASH)
                .into_iter()
                .filter_map(|index| positions_a.get(index).copied())
                .collect();
        let sampled_b: Vec<_> =
            coverage_sample_indices(positions_b.len(), MAX_ANCHOR_OCCURRENCES_PER_HASH)
                .into_iter()
                .filter_map(|index| positions_b.get(index).copied())
                .collect();
        for &a_position in &sampled_a {
            for &b_position in &sampled_b {
                if source_gram_matches(book_a, a_position, book_b, b_position) {
                    seeds.push((a_position, b_position));
                    if seeds.len() >= MAX_VERIFIED_SEEDS_PER_PAIR {
                        break 'hashes;
                    }
                }
            }
        }
    }
    seeds.sort_unstable();

    let mut by_diagonal: HashMap<i128, Vec<VerifiedSegment>> = HashMap::new();
    let mut unique = HashSet::new();
    let mut segments = Vec::new();
    for (a_position, b_position) in seeds {
        let diagonal = a_position as i128 - b_position as i128;
        if by_diagonal.get(&diagonal).is_some_and(|known| {
            known.iter().any(|segment| {
                segment.a.start <= a_position
                    && a_position < segment.a.end
                    && segment.b.start <= b_position
                    && b_position < segment.b.end
            })
        }) {
            continue;
        }
        let Some(segment) = extend_verified_seed(book_a, a_position, book_b, b_position) else {
            continue;
        };
        if segment.a.len() < MIN_VERIFIED_SEGMENT_CHARS || !unique.insert(segment) {
            continue;
        }
        by_diagonal.entry(diagonal).or_default().push(segment);
        segments.push(segment);
    }

    if segments.len() > MAX_VERIFIED_SEGMENTS_PER_PAIR {
        segments.sort_by_key(|segment| std::cmp::Reverse(segment.a.len()));
        segments.truncate(MAX_VERIFIED_SEGMENTS_PER_PAIR);
    }
    segments.sort_by_key(|segment| (segment.a.start, segment.b.start));
    segments
}

fn source_gram_matches(
    book_a: &MatchableBookText<'_>,
    a_position: usize,
    book_b: &MatchableBookText<'_>,
    b_position: usize,
) -> bool {
    let a_end = a_position.saturating_add(PASSAGE_GRAM_SIZE);
    let b_end = b_position.saturating_add(PASSAGE_GRAM_SIZE);
    book_a
        .base
        .content
        .get(a_position..a_end)
        .zip(book_b.base.content.get(b_position..b_end))
        .is_some_and(|(a, b)| {
            a == b && book_a.gram_is_matchable(a_position) && book_b.gram_is_matchable(b_position)
        })
}

fn extend_verified_seed(
    book_a: &MatchableBookText<'_>,
    a_position: usize,
    book_b: &MatchableBookText<'_>,
    b_position: usize,
) -> Option<VerifiedSegment> {
    if !source_gram_matches(book_a, a_position, book_b, b_position) {
        return None;
    }
    let allowed_a = book_a.allowed_range(a_position)?;
    let allowed_b = book_b.allowed_range(b_position)?;
    let mut a_start = a_position;
    let mut b_start = b_position;
    while a_start > allowed_a.start
        && b_start > allowed_b.start
        && book_a.base.content.get(a_start - 1) == book_b.base.content.get(b_start - 1)
    {
        a_start -= 1;
        b_start -= 1;
    }

    let mut a_end = a_position.saturating_add(PASSAGE_GRAM_SIZE);
    let mut b_end = b_position.saturating_add(PASSAGE_GRAM_SIZE);
    while a_end < allowed_a.end
        && b_end < allowed_b.end
        && book_a.base.content.get(a_end) == book_b.base.content.get(b_end)
    {
        a_end += 1;
        b_end += 1;
    }
    Some(VerifiedSegment {
        a: TextRange::new(a_start, a_end)?,
        b: TextRange::new(b_start, b_end)?,
    })
}

#[derive(Debug, Clone, Copy, Default)]
struct ChainState {
    weight: usize,
    segment: Option<usize>,
}

fn maximum_non_crossing_segments(segments: &[VerifiedSegment]) -> (Vec<VerifiedSegment>, f64) {
    if segments.is_empty() {
        return (Vec::new(), 0.0);
    }
    let mut by_start: Vec<usize> = (0..segments.len()).collect();
    by_start.sort_by_key(|&index| (segments[index].a.start, segments[index].b.start));
    let mut by_end = by_start.clone();
    by_end.sort_by_key(|&index| segments[index].a.end);
    let mut b_ends: Vec<_> = segments.iter().map(|segment| segment.b.end).collect();
    b_ends.sort_unstable();
    b_ends.dedup();

    let mut fenwick = vec![ChainState::default(); b_ends.len() + 1];
    let mut weights = vec![0_usize; segments.len()];
    let mut previous = vec![None; segments.len()];
    let mut end_cursor = 0_usize;
    for current in by_start {
        while end_cursor < by_end.len()
            && segments[by_end[end_cursor]].a.end <= segments[current].a.start
        {
            let ended = by_end[end_cursor];
            let coordinate = b_ends.partition_point(|value| *value < segments[ended].b.end) + 1;
            fenwick_update(
                &mut fenwick,
                coordinate,
                ChainState {
                    weight: weights[ended],
                    segment: Some(ended),
                },
            );
            end_cursor += 1;
        }
        let query_coordinate = b_ends.partition_point(|value| *value <= segments[current].b.start);
        let best = fenwick_query(&fenwick, query_coordinate);
        weights[current] = best.weight.saturating_add(segments[current].a.len());
        previous[current] = best.segment;
    }

    let Some((mut cursor, _)) = weights.iter().enumerate().max_by_key(|(_, weight)| *weight) else {
        return (Vec::new(), 0.0);
    };
    let mut selected = Vec::new();
    loop {
        selected.push(segments[cursor]);
        let Some(prior) = previous[cursor] else {
            break;
        };
        cursor = prior;
    }
    selected.reverse();
    let selected_chars: usize = selected.iter().map(|segment| segment.a.len()).sum();
    let possible_chars = interval_union_length(segments.iter().map(|segment| segment.a)).min(
        interval_union_length(segments.iter().map(|segment| segment.b)),
    );
    (selected, ratio_f64(selected_chars as f64, possible_chars))
}

fn fenwick_update(tree: &mut [ChainState], mut index: usize, value: ChainState) {
    while index < tree.len() {
        if value.weight > tree[index].weight {
            tree[index] = value;
        }
        index += index & index.wrapping_neg();
    }
}

fn fenwick_query(tree: &[ChainState], mut index: usize) -> ChainState {
    let mut best = ChainState::default();
    while index > 0 {
        if tree[index].weight > best.weight {
            best = tree[index];
        }
        index &= index - 1;
    }
    best
}

fn interval_union_length(ranges: impl IntoIterator<Item = TextRange>) -> usize {
    let mut ranges: Vec<_> = ranges.into_iter().collect();
    ranges.sort_by_key(|range| (range.start, range.end));
    let mut total = 0_usize;
    let mut current: Option<TextRange> = None;
    for range in ranges {
        current = match current {
            Some(active) if range.start <= active.end => Some(TextRange {
                start: active.start,
                end: active.end.max(range.end),
            }),
            Some(active) => {
                total = total.saturating_add(active.len());
                Some(range)
            }
            None => Some(range),
        };
    }
    total.saturating_add(current.map_or(0, TextRange::len))
}

fn chapter_at(text: &NormalizedBookText, position: usize) -> Option<&NormalizedChapterSpan> {
    let index = text
        .chapters
        .partition_point(|chapter| chapter.global.end <= position);
    text.chapters
        .get(index)
        .filter(|chapter| chapter.global.start <= position && position < chapter.global.end)
}

fn segment_chapter_mappings(
    book_a: &NormalizedBookText,
    book_b: &NormalizedBookText,
    segments: &[VerifiedSegment],
) -> (Vec<PersistedMatch>, Vec<DuplicateAlignmentGroupEvidence>) {
    let mut matches = Vec::new();
    for segment in segments {
        let mut a_position = segment.a.start;
        let mut b_position = segment.b.start;
        while a_position < segment.a.end && b_position < segment.b.end {
            let (Some(chapter_a), Some(chapter_b)) = (
                chapter_at(book_a, a_position),
                chapter_at(book_b, b_position),
            ) else {
                break;
            };
            let remaining = segment.a.end.saturating_sub(a_position);
            let length = remaining
                .min(chapter_a.global.end.saturating_sub(a_position))
                .min(chapter_b.global.end.saturating_sub(b_position));
            if length == 0 {
                break;
            }
            let Some(range_a) = TextRange::new(
                a_position.saturating_sub(chapter_a.global.start),
                a_position
                    .saturating_sub(chapter_a.global.start)
                    .saturating_add(length),
            ) else {
                break;
            };
            let Some(range_b) = TextRange::new(
                b_position.saturating_sub(chapter_b.global.start),
                b_position
                    .saturating_sub(chapter_b.global.start)
                    .saturating_add(length),
            ) else {
                break;
            };
            matches.push(PersistedMatch {
                a_index: chapter_a.chapter_index,
                b_index: chapter_b.chapter_index,
                match_type: "winnowing",
                similarity: 1.0,
                shared_fingerprints: i32::try_from((length / PASSAGE_GRAM_SIZE).max(1))
                    .unwrap_or(i32::MAX),
                alignment_group: None,
                segment_ordinal: None,
                range_a: Some(range_a),
                range_b: Some(range_b),
                matched_chars: i32::try_from(length).unwrap_or(i32::MAX),
            });
            a_position = a_position.saturating_add(length);
            b_position = b_position.saturating_add(length);
        }
    }
    assign_alignment_groups(matches)
}

fn assign_alignment_groups(
    mut matches: Vec<PersistedMatch>,
) -> (Vec<PersistedMatch>, Vec<DuplicateAlignmentGroupEvidence>) {
    if matches.is_empty() {
        return (matches, Vec::new());
    }
    let mut parent: Vec<usize> = (0..matches.len()).collect();
    let mut set_sizes = vec![1_usize; matches.len()];
    let mut last_a = HashMap::new();
    let mut last_b = HashMap::new();
    for (index, item) in matches.iter().enumerate() {
        if let Some(previous) = last_a.insert(item.a_index, index) {
            union_sets(&mut parent, &mut set_sizes, index, previous);
        }
        if let Some(previous) = last_b.insert(item.b_index, index) {
            union_sets(&mut parent, &mut set_sizes, index, previous);
        }
    }
    let mut members: HashMap<usize, Vec<usize>> = HashMap::new();
    for index in 0..matches.len() {
        let root = find_set(&mut parent, index);
        members.entry(root).or_default().push(index);
    }
    let mut groups: Vec<_> = members.into_values().collect();
    groups.sort_by_key(|indices| {
        indices
            .iter()
            .map(|&index| (matches[index].a_index, matches[index].b_index))
            .min()
            .unwrap_or_default()
    });

    let mut evidence = Vec::with_capacity(groups.len());
    for (group_index, mut indices) in groups.into_iter().enumerate() {
        indices.sort_by_key(|&index| {
            let item = &matches[index];
            (
                item.a_index,
                item.range_a.map_or(0, |range| range.start),
                item.b_index,
                item.range_b.map_or(0, |range| range.start),
            )
        });
        let group_id = i32::try_from(group_index).unwrap_or(i32::MAX);
        let mut chapters_a = HashSet::new();
        let mut chapters_b = HashSet::new();
        let mut matched_chars = 0_i64;
        for (ordinal, &index) in indices.iter().enumerate() {
            matches[index].alignment_group = Some(group_id);
            matches[index].segment_ordinal = Some(i32::try_from(ordinal).unwrap_or(i32::MAX));
            chapters_a.insert(matches[index].a_index);
            chapters_b.insert(matches[index].b_index);
            matched_chars = matched_chars.saturating_add(i64::from(matches[index].matched_chars));
        }
        let mut chapters_a: Vec<_> = chapters_a.into_iter().collect();
        let mut chapters_b: Vec<_> = chapters_b.into_iter().collect();
        chapters_a.sort_unstable();
        chapters_b.sort_unstable();
        let mapping_shape = match (chapters_a.len(), chapters_b.len()) {
            (1, 1) => DuplicateAlignmentMappingShape::OneToOne,
            (1, _) => DuplicateAlignmentMappingShape::OneToMany,
            (_, 1) => DuplicateAlignmentMappingShape::ManyToOne,
            _ => DuplicateAlignmentMappingShape::ManyToMany,
        };
        evidence.push(DuplicateAlignmentGroupEvidence {
            id: group_id,
            mapping_shape,
            chapters_a,
            chapters_b,
            matched_characters: matched_chars,
            segment_count: indices.len(),
            source_verified: true,
        });
    }
    matches.sort_by_key(|item| {
        (
            item.alignment_group,
            item.segment_ordinal,
            item.a_index,
            item.b_index,
        )
    });
    (matches, evidence)
}

fn find_set(parent: &mut [usize], index: usize) -> usize {
    let mut root = index;
    while parent[root] != root {
        root = parent[root];
    }
    let mut cursor = index;
    while parent[cursor] != cursor {
        let next = parent[cursor];
        parent[cursor] = root;
        cursor = next;
    }
    root
}

fn union_sets(parent: &mut [usize], set_sizes: &mut [usize], left: usize, right: usize) {
    let mut left_root = find_set(parent, left);
    let mut right_root = find_set(parent, right);
    if left_root != right_root {
        if set_sizes[left_root] < set_sizes[right_root] {
            std::mem::swap(&mut left_root, &mut right_root);
        }
        parent[right_root] = left_root;
        set_sizes[left_root] = set_sizes[left_root].saturating_add(set_sizes[right_root]);
    }
}

#[cfg(test)]
fn verified_shared_anchors(
    anchors_a: &[PassageAnchor],
    a_index: i32,
    content_a: &[char],
    anchors_b: &[PassageAnchor],
    b_index: i32,
    content_b: &[char],
) -> usize {
    let positions_b: HashMap<u64, Vec<usize>> = anchors_b
        .iter()
        .filter(|anchor| anchor.chapter_index == b_index)
        .fold(HashMap::new(), |mut positions, anchor| {
            positions
                .entry(anchor.hash)
                .or_default()
                .push(anchor.position);
            positions
        });
    anchors_a
        .iter()
        .filter(|anchor| anchor.chapter_index == a_index)
        .filter(|anchor_a| {
            positions_b.get(&anchor_a.hash).is_some_and(|positions| {
                positions.iter().any(|&position_b| {
                    content_a
                        .get(anchor_a.position..anchor_a.position.saturating_add(PASSAGE_GRAM_SIZE))
                        .zip(
                            content_b.get(position_b..position_b.saturating_add(PASSAGE_GRAM_SIZE)),
                        )
                        .is_some_and(|(a, b)| a == b)
                })
            })
        })
        .count()
}

fn classify_combined(
    metrics: &CombinedMetrics,
    book_a: &StoredBook,
    book_b: &StoredBook,
) -> DuplicateClassification {
    let count_a = book_a
        .fingerprint
        .chapters
        .iter()
        .filter(|chapter| chapter.char_count >= MIN_INFORMATIVE_CHARS)
        .count();
    let count_b = book_b
        .fingerprint
        .chapters
        .iter()
        .filter(|chapter| chapter.char_count >= MIN_INFORMATIVE_CHARS)
        .count();
    classify_metrics(
        &DuplicateMetrics {
            chapter_count_a: count_a,
            chapter_count_b: count_b,
            equivalent_chapters: usize::try_from(metrics.equivalent_chapters).unwrap_or(usize::MAX),
            coverage_a: metrics.coverage_a,
            coverage_b: metrics.coverage_b,
            character_coverage_a: metrics.character_coverage_a,
            character_coverage_b: metrics.character_coverage_b,
            longest_run: usize::try_from(metrics.equivalent_longest_run).unwrap_or(usize::MAX),
            order_score: metrics.order_score,
            added_in_a: metrics.unique_a.len(),
            added_in_b: metrics.unique_b.len(),
            total_chars_a: metrics.total_chars_a,
            total_chars_b: metrics.total_chars_b,
            unique_chars_a: metrics.unique_chars_a,
            unique_chars_b: metrics.unique_chars_b,
            verified_passage_fingerprints: metrics.verified_passage_fingerprints,
        },
        &ClassificationThresholds::default(),
    )
}

fn confidence_for(
    relation: DuplicateRelation,
    metrics: &CombinedMetrics,
    evidence: &CandidateEvidence,
) -> f64 {
    match relation {
        DuplicateRelation::ExactFile => 1.0,
        DuplicateRelation::ExactContent => 0.995,
        DuplicateRelation::ContainedVersion => (metrics.coverage_a.max(metrics.coverage_b) * 0.7
            + metrics.order_score * 0.3)
            .clamp(0.0, 0.98),
        DuplicateRelation::HighOverlap => {
            ((metrics.coverage_a + metrics.coverage_b + metrics.order_score) / 3.0).clamp(0.0, 0.94)
        }
        DuplicateRelation::PartialOverlap => {
            let chapter_signal = f64::from(metrics.shared_chapters.min(10)) / 10.0;
            (chapter_signal * 0.5
                + metrics.coverage_a.max(metrics.coverage_b) * 0.3
                + metrics.order_score * 0.2)
                .clamp(0.0, 0.88)
        }
        DuplicateRelation::SemanticRelation => evidence
            .semantic
            .as_ref()
            .map(|semantic| semantic.score)
            .unwrap_or_default()
            .clamp(0.0, 0.75),
        _ => 0.0,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct PrimaryVersionRank {
    /// Lexicographic order is intentional: genuinely new informative content
    /// dominates metadata, while repeated or tiny padding does not count as
    /// additional completeness.
    text_integrity_tier: u8,
    unique_informative_chars: u64,
    unique_informative_chapters: usize,
    informative_chapter_ratio_bps: u32,
    unique_informative_ratio_bps: u32,
    content_chars: u64,
    informative_chapters: usize,
    chapter_count: i32,
    word_count: i64,
    metadata_quality: i32,
    format_quality: i32,
    file_size_bytes: i64,
    text_integrity_bps: u32,
}

#[derive(Debug, Clone, Copy)]
struct PrimaryContentQuality {
    unique_informative_chars: u64,
    unique_informative_chapters: usize,
    informative_chapters: usize,
    repeated_informative_chapters: usize,
    total_chapters: usize,
    informative_chapter_ratio_bps: u32,
    unique_informative_ratio_bps: u32,
}

fn primary_version_rank(book: &StoredBook) -> PrimaryVersionRank {
    let quality = primary_content_quality(book);
    PrimaryVersionRank {
        text_integrity_tier: text_integrity_tier(book.text_integrity_bps),
        unique_informative_chars: quality.unique_informative_chars,
        unique_informative_chapters: quality.unique_informative_chapters,
        informative_chapter_ratio_bps: quality.informative_chapter_ratio_bps,
        unique_informative_ratio_bps: quality.unique_informative_ratio_bps,
        content_chars: book.fingerprint.char_count,
        informative_chapters: quality.informative_chapters,
        chapter_count: book.book.chapter_count.max(0),
        word_count: book.book.word_count.max(0),
        metadata_quality: book.book.metadata_quality.max(0),
        format_quality: format_quality(&book.book.format),
        file_size_bytes: book.book.file_size_bytes.max(0),
        text_integrity_bps: book.text_integrity_bps,
    }
}

fn primary_content_quality(book: &StoredBook) -> PrimaryContentQuality {
    let total_chapters = book.fingerprint.chapters.len();
    let informative: Vec<_> = book
        .fingerprint
        .chapters
        .iter()
        .filter(|chapter| chapter.char_count >= MIN_INFORMATIVE_CHARS)
        .collect();
    let informative_chapters = informative.len();
    let mut unique_layout_hashes = HashSet::with_capacity(informative_chapters);
    let mut unique_informative_chars = 0_u64;
    for chapter in informative {
        if unique_layout_hashes.insert(chapter.layout_hash) {
            unique_informative_chars = unique_informative_chars.saturating_add(chapter.char_count);
        }
    }
    let unique_informative_chapters = unique_layout_hashes.len();
    PrimaryContentQuality {
        unique_informative_chars,
        unique_informative_chapters,
        informative_chapters,
        repeated_informative_chapters: informative_chapters
            .saturating_sub(unique_informative_chapters),
        total_chapters,
        informative_chapter_ratio_bps: ratio_basis_points(informative_chapters, total_chapters),
        unique_informative_ratio_bps: ratio_basis_points(
            unique_informative_chapters,
            informative_chapters,
        ),
    }
}

fn public_primary_version_evidence(book: &StoredBook) -> DuplicatePrimaryVersionEvidence {
    let quality = primary_content_quality(book);
    DuplicatePrimaryVersionEvidence {
        content_chars: book.fingerprint.char_count,
        unique_informative_chars: quality.unique_informative_chars,
        total_chapters: quality.total_chapters,
        informative_chapters: quality.informative_chapters,
        unique_informative_chapters: quality.unique_informative_chapters,
        repeated_informative_chapters: quality.repeated_informative_chapters,
        informative_chapter_ratio: ratio(quality.informative_chapters, quality.total_chapters),
        unique_informative_ratio: ratio(
            quality.unique_informative_chapters,
            quality.informative_chapters,
        ),
        word_count: book.book.word_count.max(0),
        metadata_quality: book.book.metadata_quality.max(0),
        format_quality: format_quality(&book.book.format),
        file_size_bytes: book.book.file_size_bytes.max(0),
        text_integrity_score: f64::from(book.text_integrity_bps) / 10_000.0,
    }
}

const fn text_integrity_tier(score_bps: u32) -> u8 {
    if score_bps >= 9_950 {
        2
    } else if score_bps >= 9_800 {
        1
    } else {
        0
    }
}

fn format_quality(format: &str) -> i32 {
    match format {
        "epub" => 4,
        "mobi" | "azw3" => 3,
        "pdf" | "docx" | "doc" => 2,
        _ => 1,
    }
}

fn ratio_basis_points(numerator: usize, denominator: usize) -> u32 {
    if denominator == 0 {
        return 0;
    }
    u32::try_from(numerator.saturating_mul(10_000) / denominator).unwrap_or(10_000)
}

fn preferred_primary_version(a: &StoredBook, b: &StoredBook) -> Uuid {
    if primary_version_rank(a) >= primary_version_rank(b) {
        a.book.id
    } else {
        b.book.id
    }
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

#[allow(clippy::cast_precision_loss)]
fn ratio_f64<T>(numerator: f64, denominator: T) -> f64
where
    T: TryInto<u64>,
{
    let Ok(denominator) = denominator.try_into() else {
        return 0.0;
    };
    if denominator == 0 {
        return 0.0;
    }
    (numerator / denominator as f64).clamp(0.0, 1.0)
}

#[cfg(test)]
fn longest_contiguous_run(matches: &[PersistedMatch]) -> usize {
    let mut ordered = matches.to_vec();
    ordered.sort_by_key(|item| (item.a_index, item.b_index));
    let mut best = 0;
    let mut current = 0;
    let mut previous: Option<(u32, u32)> = None;
    for item in ordered {
        current = match previous {
            Some((a, b)) if item.a_index == a + 1 && item.b_index == b + 1 => current + 1,
            _ => 1,
        };
        best = best.max(current);
        previous = Some((item.a_index, item.b_index));
    }
    best
}

#[cfg(test)]
fn longest_ordered_subsequence(matches: &[PersistedMatch]) -> usize {
    let mut ordered = matches.to_vec();
    ordered.sort_by_key(|item| (item.a_index, item.b_index));
    let mut tails: Vec<u32> = Vec::new();
    for item in ordered {
        match tails.binary_search(&item.b_index) {
            Ok(position) => tails[position] = item.b_index,
            Err(position) if position == tails.len() => tails.push(item.b_index),
            Err(position) => tails[position] = item.b_index,
        }
    }
    tails.len()
}

fn duplicate_pair_write(
    key: CandidateKey,
    book_a: &StoredBook,
    book_b: &StoredBook,
    verified: VerifiedPair,
) -> DuplicatePairWrite {
    let VerifiedPair {
        relation,
        method,
        confidence,
        shared_chapters,
        coverage_a,
        coverage_b,
        character_coverage_a,
        character_coverage_b,
        longest_run,
        order_score,
        contained_book_id,
        recommended_primary_id,
        semantic_score,
        evidence,
        matches,
    } = verified;
    let chapter_matches = matches
        .into_iter()
        .map(|item| DuplicateChapterMatchWrite {
            chapter_a_id: book_a
                .chapter_by_index(item.a_index)
                .map(|chapter| chapter.id),
            chapter_b_id: book_b
                .chapter_by_index(item.b_index)
                .map(|chapter| chapter.id),
            chapter_a_index: i32::try_from(item.a_index).unwrap_or(i32::MAX),
            chapter_b_index: i32::try_from(item.b_index).unwrap_or(i32::MAX),
            match_type: item.match_type.to_string(),
            similarity: item.similarity,
            shared_fingerprints: item.shared_fingerprints,
            alignment_group: item.alignment_group,
            segment_ordinal: item.segment_ordinal,
            chapter_a_start: item
                .range_a
                .and_then(|range| i32::try_from(range.start).ok()),
            chapter_a_end: item.range_a.and_then(|range| i32::try_from(range.end).ok()),
            chapter_b_start: item
                .range_b
                .and_then(|range| i32::try_from(range.start).ok()),
            chapter_b_end: item.range_b.and_then(|range| i32::try_from(range.end).ok()),
            matched_chars: item.matched_chars,
        })
        .collect();
    DuplicatePairWrite {
        book_a_id: key.a,
        book_b_id: key.b,
        method: method.to_string(),
        relation,
        confidence,
        shared_chapters,
        coverage_a,
        coverage_b,
        character_coverage_a,
        character_coverage_b,
        longest_contiguous_run: longest_run,
        order_score,
        contained_book_id,
        recommended_primary_id,
        semantic_score,
        algorithm_version: ALGORITHM_VERSION,
        evidence,
        chapter_matches,
    }
}

fn semantic_chapter_index(point: &Value) -> Option<u32> {
    point
        .get("payload")
        .and_then(|payload| payload.get("chapter_index"))
        .and_then(Value::as_u64)
        .and_then(|index| u32::try_from(index).ok())
}

fn semantic_chunk_id(point: &Value) -> Option<String> {
    let payload = point.get("payload");
    if let Some(chunk_id) = payload
        .and_then(|value| value.get("chunk_id"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        return Some(format!("chunk:{chunk_id}"));
    }

    if let (Some(chapter_index), Some(chunk_index)) = (
        payload
            .and_then(|value| value.get("chapter_index"))
            .and_then(Value::as_i64),
        payload
            .and_then(|value| value.get("chunk_index"))
            .and_then(Value::as_i64),
    ) {
        return Some(format!("chapter:{chapter_index}:chunk:{chunk_index}"));
    }

    match point.get("id") {
        Some(Value::String(id)) if !id.is_empty() => Some(format!("point:{id}")),
        Some(Value::Number(id)) => Some(format!("point:{id}")),
        _ => None,
    }
}

fn select_semantic_source_points(points: &[Value]) -> Vec<SemanticSourcePoint> {
    let mut by_chapter: HashMap<u32, SemanticSourcePoint> = HashMap::new();
    for point in points {
        let (Some(chapter_index), Some(chunk_id), Some(vector)) = (
            semantic_chapter_index(point),
            semantic_chunk_id(point),
            point.get("vector").cloned(),
        ) else {
            continue;
        };
        let candidate = SemanticSourcePoint {
            chapter_index,
            chunk_id,
            vector,
        };
        by_chapter
            .entry(chapter_index)
            .and_modify(|selected| {
                if candidate.chunk_id < selected.chunk_id {
                    *selected = candidate.clone();
                }
            })
            .or_insert(candidate);
    }

    let mut distinct: Vec<_> = by_chapter.into_values().collect();
    distinct.sort_by_key(|point| point.chapter_index);
    if distinct.len() <= MAX_SEMANTIC_SOURCE_CHAPTERS {
        return distinct;
    }

    (0..MAX_SEMANTIC_SOURCE_CHAPTERS)
        .filter_map(|slot| {
            let index = slot.saturating_mul(distinct.len().saturating_sub(1))
                / MAX_SEMANTIC_SOURCE_CHAPTERS.saturating_sub(1).max(1);
            distinct.get(index).cloned()
        })
        .collect()
}

fn select_fresh_semantic_source_points(
    points: &[Value],
    book_id: Uuid,
    contract: &EmbeddingFreshnessContract,
) -> Vec<SemanticSourcePoint> {
    let fresh_points: Vec<_> = points
        .iter()
        .filter(|point| contract.matches_qdrant_point(point, book_id))
        .cloned()
        .collect();
    select_semantic_source_points(&fresh_points)
}

fn aggregate_semantic_hits(
    hits: impl IntoIterator<Item = SemanticChunkHit>,
    sampled_chapters: &HashMap<Uuid, HashSet<u32>>,
) -> HashMap<CandidateKey, SemanticCandidateEvidence> {
    #[derive(Default)]
    struct Accumulator {
        chunk_pairs: HashMap<(String, String), f64>,
        chapter_pairs: HashMap<(u32, u32), f64>,
    }

    let mut evidence_by_candidate: HashMap<CandidateKey, Accumulator> = HashMap::new();
    for hit in hits {
        if !hit.score.is_finite() || hit.score < MIN_SEMANTIC_SCORE {
            continue;
        }
        if !sampled_chapters
            .get(&hit.source_book_id)
            .is_some_and(|chapters| chapters.contains(&hit.source_chapter_index))
        {
            continue;
        }
        let Some(key) = CandidateKey::new(hit.source_book_id, hit.target_book_id) else {
            continue;
        };
        let (chunk_pair, chapter_pair) = if hit.source_book_id == key.a {
            (
                (hit.source_chunk_id, hit.target_chunk_id),
                (hit.source_chapter_index, hit.target_chapter_index),
            )
        } else {
            (
                (hit.target_chunk_id, hit.source_chunk_id),
                (hit.target_chapter_index, hit.source_chapter_index),
            )
        };
        let accumulator = evidence_by_candidate.entry(key).or_default();
        accumulator
            .chunk_pairs
            .entry(chunk_pair)
            .and_modify(|score| *score = score.max(hit.score))
            .or_insert(hit.score);
        accumulator
            .chapter_pairs
            .entry(chapter_pair)
            .and_modify(|score| *score = score.max(hit.score))
            .or_insert(hit.score);
    }

    evidence_by_candidate
        .into_iter()
        .filter_map(|(key, accumulator)| {
            let independent_chunk_matches =
                independent_semantic_match_count(&accumulator.chunk_pairs);
            if independent_chunk_matches < MIN_SEMANTIC_CHUNK_MATCHES {
                return None;
            }

            let independent_chapter_matches =
                independent_semantic_chapter_match_count(&accumulator.chapter_pairs);
            if independent_chapter_matches < MIN_SEMANTIC_CHAPTER_MATCHES {
                return None;
            }

            let ordered_chapter_matches =
                longest_ordered_semantic_matches(&accumulator.chapter_pairs);
            if ordered_chapter_matches.len() < MIN_SEMANTIC_CHAPTER_MATCHES {
                return None;
            }
            let order_score = ratio(ordered_chapter_matches.len(), independent_chapter_matches);
            if order_score < MIN_SEMANTIC_ORDER_SCORE {
                return None;
            }

            let matched_a: HashSet<_> = ordered_chapter_matches
                .iter()
                .map(|item| item.chapter_a_index)
                .collect();
            let matched_b: HashSet<_> = ordered_chapter_matches
                .iter()
                .map(|item| item.chapter_b_index)
                .collect();
            let sampled_a = sampled_chapters.get(&key.a);
            let sampled_b = sampled_chapters.get(&key.b);
            let sample_coverage_a = sampled_a.map(|chapters| {
                ratio(
                    chapters
                        .iter()
                        .filter(|chapter| matched_a.contains(chapter))
                        .count(),
                    chapters.len(),
                )
            });
            let sample_coverage_b = sampled_b.map(|chapters| {
                ratio(
                    chapters
                        .iter()
                        .filter(|chapter| matched_b.contains(chapter))
                        .count(),
                    chapters.len(),
                )
            });
            let strongest_sample_coverage = sample_coverage_a
                .unwrap_or_default()
                .max(sample_coverage_b.unwrap_or_default());
            if strongest_sample_coverage < MIN_SEMANTIC_SAMPLE_COVERAGE {
                return None;
            }

            let score = ordered_chapter_matches
                .iter()
                .map(|item| item.score)
                .sum::<f64>()
                / ordered_chapter_matches.len() as f64;
            Some((
                key,
                SemanticCandidateEvidence {
                    score,
                    independent_chunk_matches: i32::try_from(independent_chunk_matches)
                        .unwrap_or(i32::MAX),
                    independent_chapter_matches: i32::try_from(independent_chapter_matches)
                        .unwrap_or(i32::MAX),
                    matched_chapters_a: i32::try_from(matched_a.len()).unwrap_or(i32::MAX),
                    matched_chapters_b: i32::try_from(matched_b.len()).unwrap_or(i32::MAX),
                    order_score,
                    sampled_chapters_a: i32::try_from(sampled_a.map_or(0, HashSet::len))
                        .unwrap_or(i32::MAX),
                    sampled_chapters_b: i32::try_from(sampled_b.map_or(0, HashSet::len))
                        .unwrap_or(i32::MAX),
                    sample_coverage_a,
                    sample_coverage_b,
                    ordered_chapter_matches,
                },
            ))
        })
        .collect()
}

fn independent_semantic_chapter_match_count(chapter_pairs: &HashMap<(u32, u32), f64>) -> usize {
    let mut left_chapters: Vec<u32> = chapter_pairs.keys().map(|(left, _)| *left).collect();
    left_chapters.sort_unstable();
    left_chapters.dedup();
    let mut right_chapters: Vec<u32> = chapter_pairs.keys().map(|(_, right)| *right).collect();
    right_chapters.sort_unstable();
    right_chapters.dedup();

    let left_indices: HashMap<u32, usize> = left_chapters
        .iter()
        .enumerate()
        .map(|(index, chapter)| (*chapter, index))
        .collect();
    let right_indices: HashMap<u32, usize> = right_chapters
        .iter()
        .enumerate()
        .map(|(index, chapter)| (*chapter, index))
        .collect();
    let mut adjacency = vec![Vec::new(); left_chapters.len()];
    for (left, right) in chapter_pairs.keys() {
        let (Some(&left_index), Some(&right_index)) =
            (left_indices.get(left), right_indices.get(right))
        else {
            continue;
        };
        adjacency[left_index].push(right_index);
    }
    for targets in &mut adjacency {
        targets.sort_unstable();
        targets.dedup();
    }

    let mut matched_right = vec![None; right_chapters.len()];
    let mut count = 0;
    for left_index in 0..left_chapters.len() {
        let mut visited_right = vec![false; right_chapters.len()];
        if try_augment_semantic_match(
            left_index,
            &adjacency,
            &mut visited_right,
            &mut matched_right,
        ) {
            count += 1;
        }
    }
    count
}

fn longest_ordered_semantic_matches(
    chapter_pairs: &HashMap<(u32, u32), f64>,
) -> Vec<DuplicateSemanticChapterMatch> {
    let mut matches: Vec<_> = chapter_pairs
        .iter()
        .map(
            |(&(chapter_a_index, chapter_b_index), &score)| DuplicateSemanticChapterMatch {
                chapter_a_index,
                chapter_b_index,
                score,
            },
        )
        .collect();
    matches.sort_by_key(|item| (item.chapter_a_index, item.chapter_b_index));
    if matches.is_empty() {
        return Vec::new();
    }

    let mut lengths = vec![1_usize; matches.len()];
    let mut score_sums: Vec<f64> = matches.iter().map(|item| item.score).collect();
    let mut previous = vec![None; matches.len()];
    for current in 0..matches.len() {
        for candidate in 0..current {
            if matches[candidate].chapter_a_index >= matches[current].chapter_a_index
                || matches[candidate].chapter_b_index >= matches[current].chapter_b_index
            {
                continue;
            }
            let candidate_length = lengths[candidate] + 1;
            let candidate_score = score_sums[candidate] + matches[current].score;
            if candidate_length > lengths[current]
                || (candidate_length == lengths[current] && candidate_score > score_sums[current])
            {
                lengths[current] = candidate_length;
                score_sums[current] = candidate_score;
                previous[current] = Some(candidate);
            }
        }
    }

    let Some(mut cursor) = (0..matches.len()).max_by(|&left, &right| {
        lengths[left]
            .cmp(&lengths[right])
            .then_with(|| score_sums[left].total_cmp(&score_sums[right]))
    }) else {
        return Vec::new();
    };
    let mut ordered = Vec::with_capacity(lengths[cursor]);
    loop {
        ordered.push(matches[cursor]);
        let Some(next) = previous[cursor] else {
            break;
        };
        cursor = next;
    }
    ordered.reverse();
    ordered
}

fn independent_semantic_match_count(chunk_pairs: &HashMap<(String, String), f64>) -> usize {
    let mut left_chunks: Vec<&str> = chunk_pairs.keys().map(|(left, _)| left.as_str()).collect();
    left_chunks.sort_unstable();
    left_chunks.dedup();
    let mut right_chunks: Vec<&str> = chunk_pairs
        .keys()
        .map(|(_, right)| right.as_str())
        .collect();
    right_chunks.sort_unstable();
    right_chunks.dedup();

    let left_indices: HashMap<&str, usize> = left_chunks
        .iter()
        .enumerate()
        .map(|(index, chunk)| (*chunk, index))
        .collect();
    let right_indices: HashMap<&str, usize> = right_chunks
        .iter()
        .enumerate()
        .map(|(index, chunk)| (*chunk, index))
        .collect();
    let mut adjacency = vec![Vec::new(); left_chunks.len()];
    for (left, right) in chunk_pairs.keys() {
        let (Some(&left_index), Some(&right_index)) = (
            left_indices.get(left.as_str()),
            right_indices.get(right.as_str()),
        ) else {
            continue;
        };
        adjacency[left_index].push(right_index);
    }
    for targets in &mut adjacency {
        targets.sort_unstable();
        targets.dedup();
    }

    let mut matched_right = vec![None; right_chunks.len()];
    let mut count = 0;
    for left_index in 0..left_chunks.len() {
        let mut visited_right = vec![false; right_chunks.len()];
        if try_augment_semantic_match(
            left_index,
            &adjacency,
            &mut visited_right,
            &mut matched_right,
        ) {
            count += 1;
        }
    }
    count
}

fn try_augment_semantic_match(
    left_index: usize,
    adjacency: &[Vec<usize>],
    visited_right: &mut [bool],
    matched_right: &mut [Option<usize>],
) -> bool {
    for &right_index in &adjacency[left_index] {
        if visited_right[right_index] {
            continue;
        }
        visited_right[right_index] = true;
        let can_reassign = match matched_right[right_index] {
            Some(previous_left) => {
                try_augment_semantic_match(previous_left, adjacency, visited_right, matched_right)
            }
            None => true,
        };
        if can_reassign {
            matched_right[right_index] = Some(left_index);
            return true;
        }
    }
    false
}

async fn load_semantic_candidates(
    state: &AppState,
    source_book_ids: &[Uuid],
    allowed_book_ids: &[Uuid],
) -> Result<HashMap<CandidateKey, SemanticCandidateEvidence>, String> {
    let allowed: HashSet<Uuid> = allowed_book_ids.iter().copied().collect();
    let freshness_snapshots = state
        .duplicates
        .current_semantic_freshness_snapshots(allowed_book_ids)
        .await
        .map_err(|error| format!("failed to load semantic freshness snapshots: {error}"))?;
    let freshness_contracts: HashMap<_, _> = freshness_snapshots
        .into_iter()
        .map(|snapshot| {
            (
                snapshot.book_id,
                EmbeddingFreshnessContract::from_source_content_hash(
                    snapshot.source_content_hash,
                    &state.config.embedding_model,
                    state.config.embedding_dimensions,
                ),
            )
        })
        .collect();
    let mut semantic_hits = Vec::new();
    let mut semantic_pairs = HashSet::new();
    let mut sampled_chapters: HashMap<Uuid, HashSet<u32>> = HashMap::new();
    for source_book in source_book_ids {
        if allowed.is_empty() || (allowed.len() == 1 && allowed.contains(source_book)) {
            continue;
        }
        let Some(source_contract) = freshness_contracts.get(source_book) else {
            continue;
        };

        let mut source_point_pool = Vec::new();
        let mut scroll_offset: Option<Value> = None;
        for _ in 0..MAX_SEMANTIC_SOURCE_SCROLLS {
            let mut scroll_request = serde_json::json!({
                "filter": { "must": source_contract.qdrant_book_filter_conditions(*source_book) },
                "limit": SEMANTIC_SOURCE_SCROLL_LIMIT,
                "with_vector": true,
                "with_payload": true,
            });
            if let Some(offset) = scroll_offset.as_ref() {
                scroll_request["offset"] = offset.clone();
            }
            let response = state
                .http_client
                .post(format!(
                    "{}/collections/nova_chunks/points/scroll",
                    state.config.qdrant_url
                ))
                .json(&scroll_request)
                .send()
                .await
                .map_err(|error| format!("Qdrant semantic source scroll failed: {error}"))?;
            if !response.status().is_success() {
                return Err(format!(
                    "Qdrant semantic source scroll returned {}",
                    response.status()
                ));
            }
            let body = response
                .json::<Value>()
                .await
                .map_err(|error| format!("invalid Qdrant semantic scroll response: {error}"))?;
            let Some(points) = body
                .get("result")
                .and_then(|result| result.get("points"))
                .and_then(Value::as_array)
            else {
                return Err("Qdrant semantic scroll response missing result.points".to_string());
            };
            source_point_pool.extend(points.iter().cloned());
            if select_fresh_semantic_source_points(
                &source_point_pool,
                *source_book,
                source_contract,
            )
            .len()
                >= MAX_SEMANTIC_SOURCE_CHAPTERS
            {
                break;
            }
            scroll_offset = body
                .get("result")
                .and_then(|result| result.get("next_page_offset"))
                .filter(|offset| !offset.is_null())
                .cloned();
            if scroll_offset.is_none() {
                break;
            }
        }

        let source_points =
            select_fresh_semantic_source_points(&source_point_pool, *source_book, source_contract);
        sampled_chapters.insert(
            *source_book,
            source_points
                .iter()
                .map(|point| point.chapter_index)
                .collect(),
        );
        if source_points.is_empty() {
            continue;
        }
        let searches: Vec<Value> = source_points
            .iter()
            .map(|point| {
                serde_json::json!({
                    "vector": &point.vector,
                    "limit": SEMANTIC_SEARCH_LIMIT,
                    "with_payload": true,
                    "score_threshold": MIN_SEMANTIC_SCORE,
                    "filter": {
                        "must": source_contract.qdrant_common_filter_conditions(),
                        "must_not": [{
                            "key": "book_id",
                            "match": { "value": source_book.to_string() }
                        }]
                    }
                })
            })
            .collect();
        let search = state
            .http_client
            .post(format!(
                "{}/collections/nova_chunks/points/search/batch",
                state.config.qdrant_url
            ))
            .json(&serde_json::json!({ "searches": searches }))
            .send()
            .await
            .map_err(|error| format!("Qdrant semantic batch search failed: {error}"))?;
        if !search.status().is_success() {
            return Err(format!(
                "Qdrant semantic batch search returned {}",
                search.status()
            ));
        }
        let results = search
            .json::<Value>()
            .await
            .map_err(|error| format!("invalid Qdrant semantic batch response: {error}"))?;
        let Some(result_sets) = results.get("result").and_then(Value::as_array) else {
            return Err("Qdrant semantic batch response missing result".to_string());
        };
        if result_sets.len() != source_points.len() {
            return Err(format!(
                "Qdrant semantic batch returned {} result sets for {} searches",
                result_sets.len(),
                source_points.len()
            ));
        }
        for (point, hits) in source_points.iter().zip(result_sets) {
            let Some(hits) = hits.as_array() else {
                continue;
            };
            for hit in hits {
                let Some(target_id) = hit
                    .get("payload")
                    .and_then(|payload| payload.get("book_id"))
                    .and_then(Value::as_str)
                    .and_then(|value| Uuid::parse_str(value).ok())
                else {
                    continue;
                };
                if !allowed.contains(&target_id) {
                    continue;
                }
                let Some(target_contract) = freshness_contracts.get(&target_id) else {
                    continue;
                };
                if !target_contract.matches_qdrant_point(hit, target_id) {
                    continue;
                }
                let (Some(target_chapter_index), Some(target_chunk_id)) =
                    (semantic_chapter_index(hit), semantic_chunk_id(hit))
                else {
                    continue;
                };
                let score = hit.get("score").and_then(Value::as_f64).unwrap_or_default();
                if semantic_hits.len() >= MAX_SEMANTIC_HITS_PER_SCAN {
                    return Err(format!(
                        "semantic hit budget exceeded: actual_at_least={}, limit={MAX_SEMANTIC_HITS_PER_SCAN}",
                        semantic_hits.len().saturating_add(1)
                    ));
                }
                if let Some(key) = CandidateKey::new(*source_book, target_id) {
                    semantic_pairs.insert(key);
                    if semantic_pairs.len() > MAX_CANDIDATE_PAIRS_PER_SCAN {
                        return Err(format!(
                            "semantic candidate pair budget exceeded: actual_at_least={}, limit={MAX_CANDIDATE_PAIRS_PER_SCAN}",
                            semantic_pairs.len()
                        ));
                    }
                }
                semantic_hits.push(SemanticChunkHit {
                    source_book_id: *source_book,
                    source_chapter_index: point.chapter_index,
                    source_chunk_id: point.chunk_id.clone(),
                    target_book_id: target_id,
                    target_chapter_index,
                    target_chunk_id,
                    score,
                });
            }
        }
    }
    Ok(aggregate_semantic_hits(semantic_hits, &sampled_chapters))
}

async fn update_progress(
    state: &AppState,
    task_id: Uuid,
    scan_id: Uuid,
    progress: i16,
    phase: DedupScanPhase,
    books_processed: i32,
    chapters_processed: i32,
) -> Result<(), String> {
    state
        .task_queue
        .update_progress(task_id, progress, Some(phase.as_str()))
        .await
        .map_err(|error| format!("failed to update task progress: {error}"))?;
    state
        .duplicates
        .update_scan_progress(
            scan_id,
            progress,
            phase,
            books_processed,
            chapters_processed,
        )
        .await
        .map_err(|error| format!("failed to update scan progress: {error}"))
}

async fn update_pair_progress(
    state: &AppState,
    task_id: Uuid,
    scan_id: Uuid,
    progress: i16,
    counts: ScanPairCounts,
) -> Result<(), String> {
    state
        .task_queue
        .update_progress(task_id, progress, Some(DedupScanPhase::Verifying.as_str()))
        .await
        .map_err(|error| format!("failed to update task progress: {error}"))?;
    state
        .duplicates
        .update_scan_pair_progress(scan_id, progress, counts)
        .await
        .map_err(|error| format!("failed to update scan pair progress: {error}"))
}

#[allow(clippy::too_many_arguments)]
async fn publish_scan_results(
    repository: &PgDuplicateRepository,
    scan_id: Uuid,
    scope_book_ids: &[Uuid],
    affected_scope_book_ids: &[Uuid],
    stored_books: &HashMap<Uuid, StoredBook>,
    verified_pairs: Vec<(CandidateKey, VerifiedPair)>,
    pairs_found: i32,
    exact_pairs: i32,
    contained_pairs: i32,
    semantic_pairs: i32,
) -> Result<(), String> {
    let mut pairs = Vec::with_capacity(verified_pairs.len());
    let mut publication_book_ids = HashSet::new();
    for (key, verified) in verified_pairs {
        let book_a = stored_books
            .get(&key.a)
            .ok_or_else(|| format!("missing stored fingerprint for {}", key.a))?;
        let book_b = stored_books
            .get(&key.b)
            .ok_or_else(|| format!("missing stored fingerprint for {}", key.b))?;
        publication_book_ids.insert(key.a);
        publication_book_ids.insert(key.b);
        pairs.push(duplicate_pair_write(key, book_a, book_b, verified));
    }
    let mut publication_book_ids: Vec<_> = publication_book_ids.into_iter().collect();
    publication_book_ids.sort_unstable();
    let book_snapshots: Vec<_> = publication_book_ids
        .into_iter()
        .map(|book_id| {
            let book = stored_books
                .get(&book_id)
                .ok_or_else(|| format!("missing stored fingerprint for {book_id}"))?;
            Ok::<_, String>(DuplicateBookPublicationSnapshot {
                book_id,
                file_hash: book.book.file_hash.clone(),
                source_content_hash: book.source_content_hash.clone(),
                conservative_hash: book.fingerprint.conservative_hash.to_hex(),
                layout_hash: book.fingerprint.layout_hash.to_hex(),
                algorithm_version: ALGORITHM_VERSION,
            })
        })
        .collect::<Result<_, _>>()?;

    repository
        .publish_scan_results(PublishDuplicateScan {
            scan_id,
            scope_book_ids: scope_book_ids.to_vec(),
            affected_scope_book_ids: affected_scope_book_ids.to_vec(),
            book_snapshots,
            pairs,
            counts: ScanPairCounts {
                found: pairs_found,
                exact: exact_pairs,
                contained: contained_pairs,
                semantic: semantic_pairs,
            },
        })
        .await
        .map_err(|error| format!("failed to publish duplicate scan: {error}"))
}
#[cfg(test)]
mod tests {
    use super::*;

    fn match_at(a: u32, b: u32) -> PersistedMatch {
        PersistedMatch {
            a_index: a,
            b_index: b,
            match_type: "layout",
            similarity: 1.0,
            shared_fingerprints: 0,
            alignment_group: None,
            segment_ordinal: None,
            range_a: None,
            range_b: None,
            matched_chars: 0,
        }
    }

    fn stored_book(id: Uuid, chapter_count: u32) -> StoredBook {
        let contents: Vec<String> = (0..chapter_count)
            .map(|index| format!("章节 {index} {}", "足够长的正文内容".repeat(30)))
            .collect();
        stored_book_with_contents(id, &contents)
    }

    fn stored_book_with_contents(id: Uuid, contents: &[String]) -> StoredBook {
        let inputs: Vec<ChapterInput<'_>> = contents
            .iter()
            .enumerate()
            .filter_map(|(index, content)| {
                u32::try_from(index).ok().map(|chapter_index| ChapterInput {
                    chapter_index,
                    content,
                })
            })
            .collect();
        let fingerprint = fingerprint_book(&inputs);
        let chapters: Vec<_> = contents
            .iter()
            .enumerate()
            .filter_map(|(chapter_index, _)| {
                i32::try_from(chapter_index)
                    .ok()
                    .map(|chapter_index| StoredChapter {
                        id: Uuid::now_v7(),
                        chapter_index,
                    })
            })
            .collect();
        let scan_chapters: Vec<_> = chapters
            .iter()
            .zip(contents)
            .map(|(chapter, content)| ScanChapterRow {
                id: chapter.id,
                chapter_index: chapter.chapter_index,
                content: content.clone(),
            })
            .collect();
        let source_content_hash = source_content_hash(&scan_chapters);
        let chapter_count = i32::try_from(contents.len()).unwrap_or(i32::MAX);
        StoredBook {
            book: ScanBookRow {
                id,
                file_hash: format!("test-file-hash-{id}"),
                format: "txt".to_string(),
                file_size_bytes: 1_000,
                chapter_count,
                word_count: i64::from(chapter_count) * 1_000,
                metadata_quality: 0,
            },
            chapters,
            fingerprint,
            source_content_hash,
            anchors: select_passage_anchors(&scan_chapters),
            text_integrity_bps: 10_000,
        }
    }

    fn source_contents(book: &StoredBook, contents: &[String]) -> HashMap<Uuid, String> {
        book.chapters
            .iter()
            .zip(contents)
            .map(|(chapter, content)| (chapter.id, content.clone()))
            .collect()
    }

    fn boundary_test_content(seed: u32) -> String {
        (0..80)
            .map(|part| format!("片段-{seed:04}-{part:04}-星河远航与旧城灯火。"))
            .collect()
    }

    fn semantic_hit(
        source_book_id: Uuid,
        source_chapter_index: u32,
        source_chunk_id: &str,
        target_book_id: Uuid,
        target_chapter_index: u32,
        target_chunk_id: &str,
        score: f64,
    ) -> SemanticChunkHit {
        SemanticChunkHit {
            source_book_id,
            source_chapter_index,
            source_chunk_id: source_chunk_id.to_string(),
            target_book_id,
            target_chapter_index,
            target_chunk_id: target_chunk_id.to_string(),
            score,
        }
    }

    fn sampled_chapters(samples: &[(Uuid, &[u32])]) -> HashMap<Uuid, HashSet<u32>> {
        samples
            .iter()
            .map(|(book_id, chapters)| (*book_id, chapters.iter().copied().collect()))
            .collect()
    }

    fn semantic_source_point(chapter_index: u32, chunk_index: u32) -> Value {
        serde_json::json!({
            "id": format!("{chapter_index}-{chunk_index}"),
            "vector": [0.1, 0.2],
            "payload": {
                "chapter_index": chapter_index,
                "chunk_index": chunk_index,
            },
        })
    }

    fn semantic_freshness_contract(source_hash: &str) -> EmbeddingFreshnessContract {
        EmbeddingFreshnessContract::from_source_content_hash(
            source_hash.to_string(),
            "test-embedding-model",
            2,
        )
    }

    fn fresh_semantic_point(
        book_id: Uuid,
        source_hash: &str,
        chapter_index: u32,
        chunk_index: u32,
    ) -> Value {
        let contract = semantic_freshness_contract(source_hash);
        serde_json::json!({
            "id": format!("{chapter_index}-{chunk_index}"),
            "vector": [0.1, 0.2],
            "payload": contract.chunk_payload(
                book_id,
                None,
                i32::try_from(chapter_index).expect("test chapter index fits i32"),
                "Chapter",
                usize::try_from(chunk_index).expect("test chunk index fits usize"),
                "Text",
            ),
        })
    }

    #[tokio::test]
    async fn normalized_text_cache_batches_queries_and_reuses_each_book_across_fanout() {
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        };

        let books: Vec<_> = (1..=130)
            .map(|id| stored_book(Uuid::from_u128(id), 1))
            .collect();
        let mut source = HashMap::new();
        let mut source_sizes = HashMap::new();
        for book in &books {
            let content = "章节 0 ".to_string() + &"足够长的正文内容".repeat(30);
            source.insert(book.chapters[0].id, content.clone());
            source_sizes.insert(
                book.book.id,
                u64::try_from(content.len()).expect("test content length fits u64"),
            );
        }
        let source = Arc::new(source);
        let query_count = Arc::new(AtomicUsize::new(0));
        let query_counter = Arc::clone(&query_count);
        let source_for_loader = Arc::clone(&source);
        let book_refs: Vec<_> = books.iter().collect();
        let cache = load_normalized_text_cache_with(
            &book_refs,
            &source_sizes,
            move |chapter_ids, remaining_source_bytes| {
                query_counter.fetch_add(1, Ordering::SeqCst);
                let chapters: Vec<_> = chapter_ids
                    .into_iter()
                    .filter_map(|id| {
                        source_for_loader
                            .get(&id)
                            .cloned()
                            .map(|content| ChapterContentRecord { id, content })
                    })
                    .collect();
                let source_bytes = chapters.iter().fold(0_u64, |total, chapter| {
                    total.saturating_add(
                        u64::try_from(chapter.content.len()).expect("test content length fits u64"),
                    )
                });
                let result = if source_bytes > remaining_source_bytes {
                    Err("test candidate source budget exceeded".to_string())
                } else {
                    Ok(BoundedChapterContentRecords {
                        source_bytes,
                        chapters,
                    })
                };
                std::future::ready(result)
            },
        )
        .await
        .expect("candidate text cache should load");

        assert_eq!(cache.books.len(), books.len());
        assert_eq!(
            query_count.load(Ordering::SeqCst),
            3,
            "130 one-chapter books should load in three 64-book batches"
        );
        let evidence = CandidateEvidence {
            shared_passage_hashes: i32::try_from(MIN_PASSAGE_CANDIDATES).unwrap_or(4),
            ..CandidateEvidence::default()
        };
        for (left, right) in [(0, 1), (0, 2), (1, 2)] {
            assert!(verify_pair(&books[left], &books[right], &evidence, &cache)
                .expect("cached pair verification should succeed")
                .is_some());
        }
        assert_eq!(
            query_count.load(Ordering::SeqCst),
            3,
            "pair fanout must not trigger more repository reads"
        );
    }

    #[test]
    fn semantic_only_candidates_do_not_allocate_normalized_text_cache() {
        let book_a = stored_book(Uuid::from_u128(201), 3);
        let book_b = stored_book(Uuid::from_u128(202), 3);
        let stored_books = HashMap::from([(book_a.book.id, book_a), (book_b.book.id, book_b)]);
        let evidence = CandidateEvidence {
            semantic: Some(SemanticCandidateEvidence {
                score: 0.95,
                independent_chunk_matches: 2,
                independent_chapter_matches: 2,
                ordered_chapter_matches: Vec::new(),
                matched_chapters_a: 2,
                matched_chapters_b: 2,
                order_score: 1.0,
                sampled_chapters_a: 2,
                sampled_chapters_b: 2,
                sample_coverage_a: Some(1.0),
                sample_coverage_b: Some(1.0),
            }),
            ..CandidateEvidence::default()
        };
        let candidates = vec![(
            CandidateKey::new(Uuid::from_u128(201), Uuid::from_u128(202)).expect("distinct books"),
            evidence,
        )];

        let plan = plan_verification_resources(&candidates, &stored_books)
            .expect("semantic-only plan should fit");
        assert!(plan.text_book_ids.is_empty());
        assert_eq!(plan.normalized_chars, 0);
        assert_eq!(plan.pair_verification_chars, 0);
    }

    #[test]
    fn candidate_and_verification_budgets_fail_with_actual_and_limit() {
        let pair_error = enforce_candidate_pair_budget(MAX_CANDIDATE_PAIRS_PER_SCAN + 1)
            .expect_err("oversized candidate fanout must fail");
        assert!(pair_error.contains(&format!("actual={}", MAX_CANDIDATE_PAIRS_PER_SCAN + 1)));
        assert!(pair_error.contains(&format!("limit={MAX_CANDIDATE_PAIRS_PER_SCAN}")));

        let anchor_error = enforce_passage_anchor_budget(MAX_CACHED_PASSAGE_ANCHORS_PER_SCAN + 1)
            .expect_err("oversized passage cache must fail");
        assert!(anchor_error.contains(&format!(
            "actual={}",
            MAX_CACHED_PASSAGE_ANCHORS_PER_SCAN + 1
        )));
        assert!(anchor_error.contains(&format!("limit={MAX_CACHED_PASSAGE_ANCHORS_PER_SCAN}")));

        let chapter_error =
            enforce_chapter_fingerprint_budget(MAX_CACHED_CHAPTER_FINGERPRINTS_PER_SCAN + 1)
                .expect_err("oversized chapter fingerprint cache must fail");
        assert!(chapter_error.contains(&format!(
            "actual={}",
            MAX_CACHED_CHAPTER_FINGERPRINTS_PER_SCAN + 1
        )));
        assert!(
            chapter_error.contains(&format!("limit={MAX_CACHED_CHAPTER_FINGERPRINTS_PER_SCAN}"))
        );

        let mut book_a = stored_book(Uuid::from_u128(301), 1);
        let mut book_b = stored_book(Uuid::from_u128(302), 1);
        book_a.fingerprint.char_count = 1_100_000_000;
        book_b.fingerprint.char_count = 1_100_000_000;
        let stored_books = HashMap::from([(book_a.book.id, book_a), (book_b.book.id, book_b)]);
        let candidates = vec![(
            CandidateKey::new(Uuid::from_u128(301), Uuid::from_u128(302)).expect("distinct books"),
            CandidateEvidence {
                shared_passage_hashes: i32::try_from(MIN_PASSAGE_CANDIDATES).unwrap_or(4),
                ..CandidateEvidence::default()
            },
        )];
        let verification_error = plan_verification_resources(&candidates, &stored_books)
            .expect_err("oversized verification workload must fail");
        assert!(verification_error.contains("actual=2200000000"));
        assert!(
            verification_error.contains(&format!("limit={MAX_PAIR_VERIFICATION_CHARS_PER_SCAN}"))
        );
    }

    #[test]
    fn canonical_candidate_key_is_direction_independent() {
        let a = Uuid::from_u128(10);
        let b = Uuid::from_u128(20);
        assert_eq!(CandidateKey::new(a, b), CandidateKey::new(b, a));
    }

    #[test]
    fn exact_candidate_flags_are_revalidated_after_cache_repair() {
        let contents_a = vec!["甲辰星海远征".repeat(80)];
        let contents_b = vec!["乙巳山河旧梦".repeat(80)];
        let book_a = stored_book_with_contents(Uuid::from_u128(401), &contents_a);
        let book_b = stored_book_with_contents(Uuid::from_u128(402), &contents_b);
        let stale_candidate = CandidateEvidence {
            exact_file: true,
            exact_content: true,
            ..CandidateEvidence::default()
        };

        let result = verify_pair(
            &book_a,
            &book_b,
            &stale_candidate,
            &NormalizedTextCache::default(),
        )
        .expect("stale exact evidence should be safely reclassified");

        assert!(
            result.is_none(),
            "an exact candidate generated before cache invalidation must not override the repaired current fingerprints"
        );
    }

    #[test]
    fn primary_recommendation_never_trades_content_for_metadata_or_assets() {
        let complete_id = Uuid::from_u128(10);
        let incomplete_id = Uuid::from_u128(20);
        let complete = stored_book(complete_id, 12);
        let mut incomplete = stored_book(incomplete_id, 10);
        incomplete.book.format = "epub".to_string();
        incomplete.book.metadata_quality = 5;
        incomplete.book.file_size_bytes = i64::MAX;

        assert_eq!(
            preferred_primary_version(&complete, &incomplete),
            complete_id
        );
    }

    #[test]
    fn primary_recommendation_is_stable_when_content_and_quality_are_equal() {
        let first_id = Uuid::from_u128(10);
        let second_id = Uuid::from_u128(20);
        let first = stored_book(first_id, 10);
        let mut second = first.clone();
        second.book.id = second_id;
        assert_eq!(preferred_primary_version(&first, &second), first_id);
    }

    #[test]
    fn primary_recommendation_rejects_a_longer_version_with_corrupt_text() {
        let clean_id = Uuid::from_u128(10);
        let corrupt_id = Uuid::from_u128(20);
        let clean = stored_book(clean_id, 10);
        let mut corrupt = stored_book(corrupt_id, 12);
        corrupt.text_integrity_bps = 9_700;

        assert_eq!(preferred_primary_version(&clean, &corrupt), clean_id);
    }

    #[test]
    fn text_integrity_penalizes_replacement_and_control_characters() {
        let clean = [ScanChapterRow {
            id: Uuid::from_u128(1),
            chapter_index: 0,
            content: "风雪夜归人，灯火仍明。".repeat(20),
        }];
        let corrupt = [ScanChapterRow {
            id: Uuid::from_u128(2),
            chapter_index: 0,
            content: "\u{fffd}\u{fffd}\u{0000}锟斤拷".repeat(20),
        }];

        assert_eq!(text_integrity_basis_points(&clean), 10_000);
        assert!(text_integrity_basis_points(&corrupt) < 9_800);
    }

    #[test]
    fn primary_recommendation_rejects_repeated_chapter_padding() {
        let clean_id = Uuid::from_u128(10);
        let padded_id = Uuid::from_u128(20);
        let clean = stored_book(clean_id, 10);
        let mut padded_contents: Vec<String> = (0..10)
            .map(|index| format!("章节 {index} {}", "足够长的正文内容".repeat(30)))
            .collect();
        let repeated = padded_contents[9].clone();
        padded_contents.extend(std::iter::repeat_n(repeated, 20));
        let padded = stored_book_with_contents(padded_id, &padded_contents);

        assert_eq!(preferred_primary_version(&clean, &padded), clean_id);
        let quality = primary_content_quality(&padded);
        assert_eq!(quality.unique_informative_chapters, 10);
        assert_eq!(quality.repeated_informative_chapters, 20);
    }

    #[test]
    fn primary_recommendation_rejects_tiny_noise_chapter_padding() {
        let clean_id = Uuid::from_u128(10);
        let padded_id = Uuid::from_u128(20);
        let clean = stored_book(clean_id, 10);
        let mut padded_contents: Vec<String> = (0..10)
            .map(|index| format!("章节 {index} {}", "足够长的正文内容".repeat(30)))
            .collect();
        padded_contents.extend((0..20).map(|index| format!("广告 {index}")));
        let padded = stored_book_with_contents(padded_id, &padded_contents);

        assert_eq!(preferred_primary_version(&clean, &padded), clean_id);
        let quality = primary_content_quality(&padded);
        assert_eq!(quality.informative_chapters, 10);
        assert_eq!(quality.total_chapters, 30);
    }

    #[test]
    fn public_primary_evidence_contains_no_reader_asset_signal() {
        let book = stored_book(Uuid::from_u128(10), 10);
        let evidence = serde_json::to_value(public_primary_version_evidence(&book))
            .expect("public primary evidence serializes");
        let encoded = evidence.to_string();
        assert!(!encoded.contains("reader"));
    }

    #[test]
    fn semantic_candidate_rejects_independent_chunks_from_the_same_chapter() {
        let a = Uuid::from_u128(10);
        let b = Uuid::from_u128(20);
        let samples = sampled_chapters(&[(a, &[3, 7])]);

        let candidates = aggregate_semantic_hits(
            [
                semantic_hit(a, 3, "a-3-1", b, 9, "b-9-1", 0.96),
                semantic_hit(a, 3, "a-3-2", b, 9, "b-9-2", 0.95),
            ],
            &samples,
        );

        assert!(candidates.is_empty());
    }

    #[test]
    fn semantic_source_sampling_spans_distinct_chapters() {
        let mut points = Vec::new();
        for chapter_index in (0..12).rev() {
            points.push(semantic_source_point(chapter_index, 1));
            points.push(semantic_source_point(chapter_index, 0));
        }

        let selected = select_semantic_source_points(&points);
        let selected_chapters: Vec<_> = selected.iter().map(|point| point.chapter_index).collect();
        let distinct: HashSet<_> = selected_chapters.iter().copied().collect();

        assert_eq!(selected.len(), MAX_SEMANTIC_SOURCE_CHAPTERS);
        assert_eq!(distinct.len(), MAX_SEMANTIC_SOURCE_CHAPTERS);
        assert_eq!(selected_chapters.first(), Some(&0));
        assert_eq!(selected_chapters.last(), Some(&11));
        assert!(selected
            .iter()
            .all(|point| point.chunk_id.ends_with(":chunk:0")));
    }

    #[test]
    fn semantic_source_sampling_ignores_stale_and_legacy_points() {
        let book_id = Uuid::from_u128(10);
        let contract = semantic_freshness_contract("current-source-hash");
        let fresh = fresh_semantic_point(book_id, "current-source-hash", 2, 0);
        let stale = fresh_semantic_point(book_id, "previous-source-hash", 1, 0);
        let legacy = semantic_source_point(0, 0);

        let selected =
            select_fresh_semantic_source_points(&[legacy, stale, fresh], book_id, &contract);

        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].chapter_index, 2);
    }

    #[test]
    fn semantic_target_freshness_rejects_stale_or_different_embedding_contracts() {
        let book_id = Uuid::from_u128(20);
        let contract = semantic_freshness_contract("current-source-hash");
        let fresh = fresh_semantic_point(book_id, "current-source-hash", 3, 0);
        assert!(contract.matches_qdrant_point(&fresh, book_id));

        for (field, stale_value) in [
            (
                "book_source_content_hash",
                serde_json::json!("previous-source-hash"),
            ),
            ("embedding_model", serde_json::json!("different-model")),
            ("embedding_dimensions", serde_json::json!(1)),
            ("embedding_payload_version", serde_json::json!(0)),
        ] {
            let mut stale = fresh.clone();
            stale["payload"][field] = stale_value;
            assert!(
                !contract.matches_qdrant_point(&stale, book_id),
                "stale target field {field} must be rejected"
            );
        }
    }

    #[test]
    fn semantic_candidate_accepts_ordered_matches_from_distinct_chapters() {
        let a = Uuid::from_u128(10);
        let b = Uuid::from_u128(20);
        let key = CandidateKey::new(a, b).expect("different books form a candidate key");
        let samples = sampled_chapters(&[(a, &[1, 4, 8, 12])]);

        let candidates = aggregate_semantic_hits(
            [
                semantic_hit(a, 1, "a-1-0", b, 10, "b-10-0", 0.97),
                semantic_hit(a, 8, "a-8-0", b, 30, "b-30-0", 0.93),
            ],
            &samples,
        );
        let evidence = candidates
            .get(&key)
            .expect("two ordered chapter matches form a semantic candidate");

        assert_eq!(evidence.independent_chapter_matches, 2);
        assert_eq!(evidence.matched_chapters_a, 2);
        assert_eq!(evidence.matched_chapters_b, 2);
        assert_eq!(evidence.ordered_chapter_matches.len(), 2);
        assert!((evidence.order_score - 1.0).abs() < f64::EPSILON);
        assert_eq!(evidence.sampled_chapters_a, 4);
        assert_eq!(evidence.sampled_chapters_b, 0);
        assert_eq!(evidence.sample_coverage_a, Some(0.5));
        assert_eq!(evidence.sample_coverage_b, None);
        assert!((evidence.score - 0.95).abs() < 1e-12);
    }

    #[test]
    fn semantic_candidate_deduplicates_chapter_pairs_in_both_directions() {
        let a = Uuid::from_u128(10);
        let b = Uuid::from_u128(20);
        let key = CandidateKey::new(a, b).expect("different books form a candidate key");
        let samples = sampled_chapters(&[(a, &[1, 2]), (b, &[10, 20])]);

        let candidates = aggregate_semantic_hits(
            [
                semantic_hit(a, 1, "a-1", b, 10, "b-10", 0.91),
                semantic_hit(b, 10, "b-10", a, 1, "a-1", 0.97),
                semantic_hit(a, 2, "a-2", b, 20, "b-20", 0.93),
                semantic_hit(b, 20, "b-20", a, 2, "a-2", 0.92),
            ],
            &samples,
        );
        let evidence = candidates
            .get(&key)
            .expect("two chapter pairs form one canonical semantic candidate");

        assert_eq!(evidence.independent_chunk_matches, 2);
        assert_eq!(evidence.independent_chapter_matches, 2);
        assert_eq!(evidence.ordered_chapter_matches.len(), 2);
        assert!((evidence.score - 0.95).abs() < 1e-12);
    }

    #[test]
    fn semantic_candidate_rejects_reordered_chapter_evidence() {
        let a = Uuid::from_u128(10);
        let b = Uuid::from_u128(20);
        let samples = sampled_chapters(&[(a, &[1, 2, 3])]);

        let candidates = aggregate_semantic_hits(
            [
                semantic_hit(a, 1, "a-1", b, 30, "b-30", 0.96),
                semantic_hit(a, 2, "a-2", b, 20, "b-20", 0.95),
                semantic_hit(a, 3, "a-3", b, 10, "b-10", 0.94),
            ],
            &samples,
        );

        assert!(candidates.is_empty());
    }

    #[test]
    fn semantic_candidate_accepts_order_score_boundary() {
        let a = Uuid::from_u128(10);
        let b = Uuid::from_u128(20);
        let key = CandidateKey::new(a, b).expect("different books form a candidate key");
        let samples = sampled_chapters(&[(a, &[1, 2, 3, 4])]);

        let candidates = aggregate_semantic_hits(
            [
                semantic_hit(a, 1, "a-1", b, 10, "b-10", 0.96),
                semantic_hit(a, 2, "a-2", b, 20, "b-20", 0.95),
                semantic_hit(a, 3, "a-3", b, 30, "b-30", 0.94),
                semantic_hit(a, 4, "a-4", b, 5, "b-5", 0.93),
            ],
            &samples,
        );
        let evidence = candidates
            .get(&key)
            .expect("three of four ordered matches meet the order boundary");

        assert!((evidence.order_score - MIN_SEMANTIC_ORDER_SCORE).abs() < f64::EPSILON);
        assert_eq!(evidence.ordered_chapter_matches.len(), 3);
    }

    #[test]
    fn semantic_candidate_enforces_sample_coverage_boundary() {
        let a = Uuid::from_u128(10);
        let b = Uuid::from_u128(20);
        let hits = [
            semantic_hit(a, 1, "a-1", b, 11, "b-11", 0.96),
            semantic_hit(a, 2, "a-2", b, 12, "b-12", 0.95),
        ];
        let at_boundary = sampled_chapters(&[(a, &[1, 2, 3, 4, 5, 6, 7, 8])]);
        let below_boundary = sampled_chapters(&[(a, &[1, 2, 3, 4, 5, 6, 7, 8, 9])]);

        assert!(!aggregate_semantic_hits(hits.clone(), &at_boundary).is_empty());
        assert!(aggregate_semantic_hits(hits, &below_boundary).is_empty());
    }

    #[test]
    fn semantic_chunk_identity_prefers_stable_payload_coordinates() {
        let first = serde_json::json!({
            "id": 101,
            "payload": { "chapter_index": 7, "chunk_index": 3 }
        });
        let reindexed = serde_json::json!({
            "id": 202,
            "payload": { "chapter_index": 7, "chunk_index": 3 }
        });

        assert_eq!(semantic_chunk_id(&first), semantic_chunk_id(&reindexed));
        assert_eq!(
            semantic_chunk_id(&first).as_deref(),
            Some("chapter:7:chunk:3")
        );
    }

    #[test]
    fn contiguous_run_requires_both_books_to_advance() {
        let matches = vec![match_at(1, 4), match_at(2, 5), match_at(3, 9)];
        assert_eq!(longest_contiguous_run(&matches), 2);
    }

    #[test]
    fn order_score_detects_reordered_matches() {
        let matches = vec![match_at(1, 3), match_at(2, 2), match_at(3, 1)];
        assert_eq!(longest_ordered_subsequence(&matches), 1);
    }

    #[test]
    fn mixed_exact_and_near_evidence_can_upgrade_partial_to_containment() {
        let exact_only = DuplicateClassification {
            relation: DuplicateRelation::PartialOverlap,
            contained: None,
        };
        let combined = DuplicateClassification {
            relation: DuplicateRelation::ContainedVersion,
            contained: Some(BookSide::A),
        };

        assert_eq!(
            final_deterministic_classification(exact_only, combined, &CandidateEvidence::default()),
            combined
        );
    }

    #[test]
    fn grouped_segments_detect_ten_chapters_merged_into_one_longer_version() {
        let short_contents: Vec<_> = (0..10).map(boundary_test_content).collect();
        let mut long_contents = vec![short_contents.concat()];
        long_contents.extend((100..105).map(boundary_test_content));
        let short = stored_book_with_contents(Uuid::from_u128(10), &short_contents);
        let long = stored_book_with_contents(Uuid::from_u128(20), &long_contents);
        let mut contents = source_contents(&short, &short_contents);
        contents.extend(source_contents(&long, &long_contents));
        let normalized_short =
            normalized_book_text(&short, &contents).expect("short source should normalize");
        let normalized_long =
            normalized_book_text(&long, &contents).expect("long source should normalize");

        let matchable_short = MatchableBookText::new(&normalized_short, &HashSet::new());
        let matchable_long = MatchableBookText::new(&normalized_long, &HashSet::new());
        let segments = verified_global_segments(&matchable_short, &matchable_long);
        let (selected, order_score) = maximum_non_crossing_segments(&segments);
        let (matches, groups) =
            segment_chapter_mappings(&normalized_short, &normalized_long, &selected);
        let metrics = combined_metrics(&short, &long, &matches, order_score);
        let classification = classify_combined(&metrics, &short, &long);

        assert_eq!(classification.relation, DuplicateRelation::ContainedVersion);
        assert_eq!(classification.contained, Some(BookSide::A));
        assert_eq!(metrics.matched_chapters_a, 10);
        assert_eq!(metrics.matched_chapters_b, 1);
        assert!((metrics.coverage_a - 1.0).abs() < 1e-12);
        assert!((metrics.character_coverage_a - 1.0).abs() < 1e-12);
        assert!((metrics.character_coverage_b - (2.0 / 3.0)).abs() < 0.01);
        assert!((0.0..=1.0).contains(&metrics.coverage_b));
        assert!((0.0..=1.0).contains(&metrics.character_coverage_b));
        assert_eq!(groups.len(), 1);
        assert_eq!(
            groups[0].mapping_shape,
            DuplicateAlignmentMappingShape::ManyToOne
        );
        assert_eq!(groups[0].chapters_a.len(), 10);
        assert_eq!(groups[0].chapters_b.len(), 1);
    }

    #[test]
    fn grouped_containment_uses_unique_characters_when_new_text_stays_in_merged_chapter() {
        let short_contents: Vec<_> = (0..10).map(boundary_test_content).collect();
        let mut merged = short_contents.concat();
        merged.push_str(&boundary_test_content(500));
        let long_contents = vec![merged];
        let short = stored_book_with_contents(Uuid::from_u128(30), &short_contents);
        let long = stored_book_with_contents(Uuid::from_u128(40), &long_contents);
        let mut contents = source_contents(&short, &short_contents);
        contents.extend(source_contents(&long, &long_contents));
        let normalized_short =
            normalized_book_text(&short, &contents).expect("short source should normalize");
        let normalized_long =
            normalized_book_text(&long, &contents).expect("long source should normalize");
        let matchable_short = MatchableBookText::new(&normalized_short, &HashSet::new());
        let matchable_long = MatchableBookText::new(&normalized_long, &HashSet::new());
        let segments = verified_global_segments(&matchable_short, &matchable_long);
        let (selected, order_score) = maximum_non_crossing_segments(&segments);
        let (matches, _) = segment_chapter_mappings(&normalized_short, &normalized_long, &selected);
        let metrics = combined_metrics(&short, &long, &matches, order_score);

        assert!(
            metrics.unique_b.is_empty(),
            "the merged chapter is partly covered"
        );
        assert!(metrics.unique_chars_b >= 120);
        assert_eq!(
            classify_combined(&metrics, &short, &long),
            DuplicateClassification {
                relation: DuplicateRelation::ContainedVersion,
                contained: Some(BookSide::A),
            }
        );
    }

    #[test]
    fn grouped_segments_are_direction_independent_for_one_to_many_boundaries() {
        let short_contents: Vec<_> = (0..10).map(boundary_test_content).collect();
        let mut long_contents = vec![short_contents.concat()];
        long_contents.extend((100..105).map(boundary_test_content));
        let long = stored_book_with_contents(Uuid::from_u128(50), &long_contents);
        let short = stored_book_with_contents(Uuid::from_u128(60), &short_contents);
        let mut contents = source_contents(&long, &long_contents);
        contents.extend(source_contents(&short, &short_contents));
        let normalized_long =
            normalized_book_text(&long, &contents).expect("long source should normalize");
        let normalized_short =
            normalized_book_text(&short, &contents).expect("short source should normalize");
        let matchable_long = MatchableBookText::new(&normalized_long, &HashSet::new());
        let matchable_short = MatchableBookText::new(&normalized_short, &HashSet::new());
        let segments = verified_global_segments(&matchable_long, &matchable_short);
        let (selected, order_score) = maximum_non_crossing_segments(&segments);
        let (matches, groups) =
            segment_chapter_mappings(&normalized_long, &normalized_short, &selected);
        let metrics = combined_metrics(&long, &short, &matches, order_score);

        assert_eq!(
            classify_combined(&metrics, &long, &short).contained,
            Some(BookSide::B)
        );
        assert_eq!(
            groups[0].mapping_shape,
            DuplicateAlignmentMappingShape::OneToMany
        );
        assert_eq!(metrics.matched_chapters_a, 1);
        assert_eq!(metrics.matched_chapters_b, 10);
    }

    #[test]
    fn positional_anchor_sampling_preserves_a_small_prefix_in_a_huge_merged_chapter() {
        let short_contents: Vec<_> = (0..10).map(boundary_test_content).collect();
        let mut merged = short_contents.concat();
        let shared_prefix_chars = nova_ingest::dedup::normalize_layout(&merged)
            .chars()
            .count();
        merged.push_str(&"百万字新增正文".repeat(125_000));
        let long_contents = vec![merged];
        let short = stored_book_with_contents(Uuid::from_u128(61), &short_contents);
        let long = stored_book_with_contents(Uuid::from_u128(62), &long_contents);
        let short_hashes: HashSet<_> = short.anchors.iter().map(|anchor| anchor.hash).collect();
        let shared_hashes: HashSet<_> = long
            .anchors
            .iter()
            .filter(|anchor| short_hashes.contains(&anchor.hash))
            .map(|anchor| anchor.hash)
            .collect();

        assert!(
            long.anchors
                .iter()
                .filter(|anchor| anchor.position < shared_prefix_chars)
                .count()
                >= usize::try_from(MIN_PASSAGE_CANDIDATES).unwrap_or(4),
            "edge coverage must retain several anchors from the small common prefix"
        );
        assert!(
            shared_hashes.len() >= usize::try_from(MIN_PASSAGE_CANDIDATES).unwrap_or(4),
            "the pair must still pass deterministic passage candidate generation"
        );
    }

    #[test]
    fn larger_single_chapter_budget_preserves_a_small_shared_middle_region() {
        let short_contents: Vec<_> = (10..20).map(boundary_test_content).collect();
        let shared = short_contents.concat();
        let added_prefix = "前置新增".repeat(125_000);
        let added_suffix = "后置新增".repeat(125_000);
        let long_contents = vec![format!("{added_prefix}{shared}{added_suffix}")];
        let short = stored_book_with_contents(Uuid::from_u128(65), &short_contents);
        let long = stored_book_with_contents(Uuid::from_u128(66), &long_contents);
        let short_hashes: HashSet<_> = short.anchors.iter().map(|anchor| anchor.hash).collect();
        let shared_hashes: HashSet<_> = long
            .anchors
            .iter()
            .filter(|anchor| short_hashes.contains(&anchor.hash))
            .map(|anchor| anchor.hash)
            .collect();

        assert!(long.anchors.len() <= MAX_PASSAGE_ANCHORS_PER_BOOK);
        assert!(long.anchors.len() <= MAX_PASSAGE_ANCHORS_PER_CHAPTER);
        assert!(
            shared_hashes.len() >= usize::try_from(MIN_PASSAGE_CANDIDATES).unwrap_or(4),
            "a roughly 10k-character middle region in a million-character chapter must remain a candidate"
        );
    }

    #[test]
    fn repeated_hash_occurrences_are_sampled_instead_of_discarded() {
        let repeated = boundary_test_content(700);
        let contents_a = vec![repeated.clone(); 9];
        let mut contents_b = vec![format!("{repeated}{}", "异文".repeat(60)); 9];
        contents_b.push(boundary_test_content(701));
        let book_a = stored_book_with_contents(Uuid::from_u128(63), &contents_a);
        let book_b = stored_book_with_contents(Uuid::from_u128(64), &contents_b);
        let hashes_a: HashSet<_> = book_a.anchors.iter().map(|anchor| anchor.hash).collect();
        let hashes_b: HashSet<_> = book_b.anchors.iter().map(|anchor| anchor.hash).collect();
        assert!(
            hashes_a.intersection(&hashes_b).count()
                >= usize::try_from(MIN_PASSAGE_CANDIDATES).unwrap_or(4)
        );

        let mut source = source_contents(&book_a, &contents_a);
        source.extend(source_contents(&book_b, &contents_b));
        let normalized_a =
            normalized_book_text(&book_a, &source).expect("repeated source A should normalize");
        let normalized_b =
            normalized_book_text(&book_b, &source).expect("repeated source B should normalize");
        let matchable_a = MatchableBookText::new(&normalized_a, &HashSet::new());
        let matchable_b = MatchableBookText::new(&normalized_b, &HashSet::new());
        let segments = verified_global_segments(&matchable_a, &matchable_b);
        assert!(
            !segments.is_empty(),
            "more than eight occurrences of one hash must not erase all evidence"
        );
        let (selected, order_score) = maximum_non_crossing_segments(&segments);
        let (matches, _) = segment_chapter_mappings(&normalized_a, &normalized_b, &selected);
        let metrics = combined_metrics(&book_a, &book_b, &matches, order_score);
        assert!(metrics.matched_chapters_a >= 8);
        assert_eq!(
            classify_combined(&metrics, &book_a, &book_b),
            DuplicateClassification {
                relation: DuplicateRelation::ContainedVersion,
                contained: Some(BookSide::A),
            }
        );
    }

    #[test]
    fn exact_and_grouped_segments_must_share_one_global_order() {
        let contents_a: Vec<_> = (0..10).map(boundary_test_content).collect();
        let mut contents_b: Vec<_> = contents_a[2..].to_vec();
        for content in &contents_a[..2] {
            let mut moved = content.clone();
            moved.push_str(&"增补".repeat(30));
            contents_b.push(moved);
        }
        let book_a = stored_book_with_contents(Uuid::from_u128(70), &contents_a);
        let book_b = stored_book_with_contents(Uuid::from_u128(80), &contents_b);
        let alignment = align_chapters(&book_a.fingerprint, &book_b.fingerprint);
        assert_eq!(alignment.matches.len(), 8);

        let exact_matches: Vec<_> = alignment
            .matches
            .iter()
            .filter_map(|chapter_match| {
                let range_a = full_chapter_range(&book_a, chapter_match.a_index)?;
                let range_b = full_chapter_range(&book_b, chapter_match.b_index)?;
                Some(PersistedMatch {
                    a_index: chapter_match.a_index,
                    b_index: chapter_match.b_index,
                    match_type: "layout",
                    similarity: 1.0,
                    shared_fingerprints: 0,
                    alignment_group: None,
                    segment_ordinal: None,
                    range_a: Some(range_a),
                    range_b: Some(range_b),
                    matched_chars: i32::try_from(range_a.len()).unwrap_or(i32::MAX),
                })
            })
            .collect();
        let mut source = source_contents(&book_a, &contents_a);
        source.extend(source_contents(&book_b, &contents_b));
        let used_a: HashSet<_> = exact_matches.iter().map(|item| item.a_index).collect();
        let used_b: HashSet<_> = exact_matches.iter().map(|item| item.b_index).collect();
        let normalized_a =
            normalized_book_text(&book_a, &source).expect("book A source should normalize");
        let normalized_b =
            normalized_book_text(&book_b, &source).expect("book B source should normalize");
        let matchable_a = MatchableBookText::new(&normalized_a, &used_a);
        let matchable_b = MatchableBookText::new(&normalized_b, &used_b);
        let passage_segments = verified_global_segments(&matchable_a, &matchable_b);
        assert!(
            !passage_segments.is_empty(),
            "the moved chapters must have verified passage evidence"
        );

        let (unconstrained, passage_order_score) = maximum_non_crossing_segments(&passage_segments);
        let (unconstrained_matches, _) =
            segment_chapter_mappings(&normalized_a, &normalized_b, &unconstrained);
        let mut false_global_alignment = exact_matches.clone();
        false_global_alignment.extend(unconstrained_matches);
        let false_metrics = combined_metrics(
            &book_a,
            &book_b,
            &false_global_alignment,
            alignment.order_score.min(passage_order_score),
        );
        assert_eq!(
            classify_combined(&false_metrics, &book_a, &book_b).relation,
            DuplicateRelation::ContainedVersion,
            "independently ordered subsets reproduce the former false containment"
        );

        let (selected_passages, _) = select_grouped_passage_segments(
            &normalized_a,
            &normalized_b,
            &exact_matches,
            &passage_segments,
        )
        .expect("global grouped alignment should succeed");
        assert!(
            selected_passages.is_empty(),
            "moved near matches cross the exact chapter anchors"
        );
        let fixed_metrics =
            combined_metrics(&book_a, &book_b, &exact_matches, alignment.order_score);
        assert_ne!(
            classify_combined(&fixed_metrics, &book_a, &book_b).relation,
            DuplicateRelation::ContainedVersion
        );
    }

    #[test]
    fn alignment_group_union_find_handles_large_boundary_fanout_iteratively() {
        const FRAGMENT_COUNT: u32 = 25_000;
        for one_to_many in [true, false] {
            let matches: Vec<_> = (0..FRAGMENT_COUNT)
                .map(|index| PersistedMatch {
                    a_index: if one_to_many { 0 } else { index },
                    b_index: if one_to_many { index } else { 0 },
                    match_type: "winnowing",
                    similarity: 1.0,
                    shared_fingerprints: 1,
                    alignment_group: None,
                    segment_ordinal: None,
                    range_a: TextRange::new(0, 1),
                    range_b: TextRange::new(0, 1),
                    matched_chars: 1,
                })
                .collect();

            let (grouped, evidence) = assign_alignment_groups(matches);
            assert_eq!(grouped.len(), FRAGMENT_COUNT as usize);
            assert!(grouped.iter().all(|item| item.alignment_group == Some(0)));
            assert_eq!(evidence.len(), 1);
            assert_eq!(
                evidence[0].mapping_shape,
                if one_to_many {
                    DuplicateAlignmentMappingShape::OneToMany
                } else {
                    DuplicateAlignmentMappingShape::ManyToOne
                }
            );
            assert_eq!(evidence[0].segment_count, FRAGMENT_COUNT as usize);
        }
    }

    #[test]
    fn crossing_segments_reduce_order_evidence_without_reusing_text() {
        let segments = [
            VerifiedSegment {
                a: TextRange::new(0, 100).expect("valid range"),
                b: TextRange::new(100, 200).expect("valid range"),
            },
            VerifiedSegment {
                a: TextRange::new(100, 200).expect("valid range"),
                b: TextRange::new(0, 100).expect("valid range"),
            },
        ];

        let (selected, order_score) = maximum_non_crossing_segments(&segments);

        assert_eq!(selected.len(), 1);
        assert!((order_score - 0.5).abs() < f64::EPSILON);
        assert_eq!(
            interval_union_length([
                TextRange::new(0, 100).expect("valid range"),
                TextRange::new(0, 100).expect("valid range"),
                TextRange::new(50, 150).expect("valid range"),
            ]),
            150
        );
    }

    #[test]
    fn source_verified_passage_matches_are_partial_not_containment() {
        let short = stored_book(Uuid::from_u128(10), 10);
        let long = stored_book(Uuid::from_u128(20), 15);
        let matches: Vec<_> = (0..10)
            .map(|index| PersistedMatch {
                a_index: index,
                b_index: index,
                match_type: "winnowing",
                similarity: 0.35,
                shared_fingerprints: 4,
                alignment_group: None,
                segment_ordinal: None,
                range_a: None,
                range_b: None,
                matched_chars: 0,
            })
            .collect();
        let metrics = combined_metrics(&short, &long, &matches, 1.0);
        assert!((metrics.coverage_a - 0.35).abs() < f64::EPSILON);
        assert_eq!(metrics.equivalent_chapters, 0);
        assert_eq!(
            classify_combined(&metrics, &short, &long).relation,
            DuplicateRelation::PartialOverlap
        );
    }

    #[test]
    fn strong_passage_matches_can_establish_version_containment() {
        let short = stored_book(Uuid::from_u128(10), 10);
        let long = stored_book(Uuid::from_u128(20), 15);
        let matches: Vec<_> = (0..10)
            .map(|index| PersistedMatch {
                a_index: index,
                b_index: index,
                match_type: "winnowing",
                similarity: 0.9,
                shared_fingerprints: 12,
                alignment_group: None,
                segment_ordinal: None,
                range_a: None,
                range_b: None,
                matched_chars: 0,
            })
            .collect();
        let metrics = combined_metrics(&short, &long, &matches, 1.0);

        assert_eq!(metrics.equivalent_chapters, 10);
        assert_eq!(
            classify_combined(&metrics, &short, &long).relation,
            DuplicateRelation::ContainedVersion
        );
    }

    #[test]
    fn source_text_verification_rejects_a_rolling_hash_collision() {
        let anchor_a = PassageAnchor {
            chapter_id: Uuid::from_u128(1),
            chapter_index: 0,
            hash: 42,
            position: 0,
        };
        let anchor_b = PassageAnchor {
            chapter_id: Uuid::from_u128(2),
            chapter_index: 0,
            hash: 42,
            position: 0,
        };
        let content_a: Vec<char> = "abcdefghijklm".chars().collect();
        let different_content: Vec<char> = "ABCDEFGHIJKLM".chars().collect();

        assert_eq!(
            verified_shared_anchors(
                std::slice::from_ref(&anchor_a),
                0,
                &content_a,
                std::slice::from_ref(&anchor_b),
                0,
                &different_content,
            ),
            0
        );
        assert_eq!(
            verified_shared_anchors(&[anchor_a], 0, &content_a, &[anchor_b], 0, &content_a),
            1
        );
    }

    #[test]
    fn equal_length_near_matches_are_overlap_not_containment() {
        let book_a = stored_book(Uuid::from_u128(10), 10);
        let book_b = stored_book(Uuid::from_u128(20), 10);
        let matches: Vec<_> = (0..10)
            .map(|index| PersistedMatch {
                a_index: index,
                b_index: index,
                match_type: "winnowing",
                similarity: 0.9,
                shared_fingerprints: 12,
                alignment_group: None,
                segment_ordinal: None,
                range_a: None,
                range_b: None,
                matched_chars: 0,
            })
            .collect();
        let metrics = combined_metrics(&book_a, &book_b, &matches, 1.0);

        assert_eq!(
            classify_combined(&metrics, &book_a, &book_b).relation,
            DuplicateRelation::HighOverlap
        );
    }

    #[test]
    fn queued_incremental_scans_union_targets_while_a_full_scan_dominates() {
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        assert_eq!(
            merge_target_book_ids(Some(vec![a]), Some(vec![b])),
            Some(vec![a, b])
        );
        assert_eq!(merge_target_book_ids(None, Some(vec![a])), None);
        assert_eq!(merge_target_book_ids(Some(vec![a]), None), None);
    }

    #[test]
    fn target_ids_round_trip_through_the_persistent_task_payload() {
        let scan_id = Uuid::from_u128(10);
        let library_id = Uuid::from_u128(20);
        let targets = vec![Uuid::from_u128(30), Uuid::from_u128(40)];
        let payload = scan_payload(scan_id, Some(library_id), false, Some(&targets));

        let encoded = serde_json::to_value(&payload).expect("scan task serializes");
        let decoded: DedupScanTask =
            serde_json::from_value(encoded).expect("scan task deserializes");
        assert_eq!(decoded.target_book_ids, Some(targets));
    }

    #[test]
    fn full_scan_keeps_inactive_books_in_the_stale_publication_scope() {
        let active = Uuid::from_u128(10);
        let archived = Uuid::from_u128(20);

        let scope = plan_scan_scope(vec![active, archived], vec![active], None);

        assert_eq!(scope.scope_book_ids, vec![active, archived]);
        assert_eq!(scope.scannable_book_ids, vec![active]);
        assert_eq!(scope.affected_scope_book_ids, vec![active, archived]);
        assert_eq!(scope.affected_scannable_book_ids, vec![active]);
        assert!(!scope.incremental);
    }

    #[test]
    fn inactive_incremental_target_is_staled_without_becoming_a_candidate_source() {
        let active = Uuid::from_u128(10);
        let archived = Uuid::from_u128(20);

        let scope = plan_scan_scope(vec![active, archived], vec![active], Some(&[archived]));

        assert_eq!(scope.affected_scope_book_ids, vec![archived]);
        assert!(scope.affected_scannable_book_ids.is_empty());
        assert!(scope.incremental);
    }

    #[test]
    fn target_payload_remains_incremental_even_when_it_covers_every_active_book() {
        let active = Uuid::from_u128(10);
        let archived = Uuid::from_u128(20);

        let scope = plan_scan_scope(vec![active, archived], vec![active], Some(&[active]));

        assert_eq!(scope.affected_scannable_book_ids, scope.scannable_book_ids);
        assert!(scope.incremental);
    }

    #[test]
    fn source_diff_does_not_promote_a_shared_prefix_to_an_equivalent_chapter() {
        let shared = "共同开头内容".repeat(20);
        let content_a = format!("{shared}{}", "天地玄黄".repeat(300));
        let content_b = format!("{shared}{}", "宇宙洪荒".repeat(300));
        let chars_a: Vec<char> = content_a.chars().collect();
        let chars_b: Vec<char> = content_b.chars().collect();

        assert!(verified_text_similarity(&content_a, &content_b, &chars_a, &chars_b) < 0.75);
    }

    #[test]
    fn passage_anchor_budget_samples_the_end_of_pathological_books() {
        let chapters: Vec<_> = (0..(MAX_PASSAGE_ANCHORS_PER_BOOK + 100))
            .map(|index| ScanChapterRow {
                id: Uuid::from_u128(index as u128 + 1),
                chapter_index: i32::try_from(index).unwrap_or(i32::MAX),
                content: format!("章节{index} {}", "独特且足够长的正文内容".repeat(20)),
            })
            .collect();

        let anchors = select_passage_anchors(&chapters);
        let last_sampled = anchors
            .iter()
            .map(|anchor| anchor.chapter_index)
            .max()
            .unwrap_or_default();
        assert!(last_sampled > i32::try_from(chapters.len() * 99 / 100).unwrap_or(i32::MAX));
        assert!(anchors.len() <= MAX_PASSAGE_ANCHORS_PER_BOOK);
    }
}
