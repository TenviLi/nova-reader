use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{de, Deserialize, Deserializer, Serialize};
use uuid::Uuid;

use nova_core::domain::dedup::{
    DedupScanPhase, DuplicateCandidateKind, DuplicatePairEvidence, DuplicateRelation,
    DuplicateReviewStatus,
};

use crate::access::{
    auth_user_id, default_library_id, ensure_book_access, ensure_library_access, is_admin,
    visible_library_ids, LibraryAccess,
};
use crate::dedup::{self, ResolveAction};
use crate::error::{ApiError, ApiResult};
use crate::extractors::AuthUser;
use crate::repo::pg_duplicate::{
    BookVersionsRecord, DuplicateChapterMatchRecord, DuplicatePairFilter, DuplicatePairRecord,
    DuplicateScanRecord,
};
use crate::repo::pg_exact_file_discovery::{ExactFileDiscoveryFilter, ExactFileDiscoveryRecord};
use crate::state::AppState;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/duplicates", get(list_pairs))
        .route(
            "/duplicates/exact-file-discoveries",
            get(list_exact_file_discoveries),
        )
        .route("/duplicates/scans", post(start_scan))
        .route("/duplicates/scans/latest", get(latest_scan))
        .route("/duplicates/scans/{id}", get(get_scan))
        .route("/duplicates/{id}", get(get_pair))
        .route("/duplicates/{id}/resolve", post(resolve_pair))
        .route(
            "/duplicates/{pair_id}/matches/{match_id}/diff",
            get(get_chapter_diff),
        )
        .route("/books/{id}/versions", get(list_book_versions))
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ScanResponse {
    id: Uuid,
    pub library_id: Option<Uuid>,
    task_id: Option<Uuid>,
    include_semantic: bool,
    algorithm_version: i32,
    status: String,
    progress: i16,
    progress_message: Option<DedupScanPhase>,
    books_total: i32,
    books_processed: i32,
    chapters_processed: i32,
    candidates_found: i32,
    pairs_found: i32,
    exact_pairs: i32,
    contained_pairs: i32,
    semantic_pairs: i32,
    error_message: Option<String>,
    started_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

impl From<DuplicateScanRecord> for ScanResponse {
    fn from(record: DuplicateScanRecord) -> Self {
        Self {
            id: record.id,
            library_id: record.library_id,
            task_id: record.task_id,
            include_semantic: record.include_semantic,
            algorithm_version: record.algorithm_version,
            status: record.status,
            progress: record.progress,
            progress_message: record
                .progress_message
                .as_deref()
                .and_then(DedupScanPhase::from_wire),
            books_total: record.books_total,
            books_processed: record.books_processed,
            chapters_processed: record.chapters_processed,
            candidates_found: record.candidates_found,
            pairs_found: record.pairs_found,
            exact_pairs: record.exact_pairs,
            contained_pairs: record.contained_pairs,
            semantic_pairs: record.semantic_pairs,
            error_message: record.error_message,
            started_at: record.started_at,
            completed_at: record.completed_at,
            created_at: record.created_at,
        }
    }
}

#[derive(Debug, Deserialize)]
struct StartScanRequest {
    library_id: Option<Uuid>,
    #[serde(default)]
    include_semantic: bool,
}

#[derive(Debug, Default, Deserialize)]
struct ScanQuery {
    library_id: Option<Uuid>,
}

#[derive(Debug, Default, Deserialize)]
struct PairQuery {
    library_id: Option<Uuid>,
    candidate_kind: Option<DuplicateCandidateKind>,
    #[serde(default, deserialize_with = "deserialize_reviewable_relation")]
    relation: Option<DuplicateRelation>,
    status: Option<DuplicateReviewStatus>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
struct PairDetailQuery {
    match_limit: Option<i64>,
    match_offset: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
struct ExactFileDiscoveryQuery {
    library_id: Option<Uuid>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct ResolveRequest {
    action: ResolveAction,
}

#[derive(Debug, Serialize)]
struct BookSummary {
    id: Uuid,
    title: String,
    author: Option<String>,
    format: String,
    file_size: i64,
    word_count: i64,
    chapter_count: i32,
    cover_path: Option<String>,
}

#[derive(Debug, Serialize)]
struct PairSummary {
    id: Uuid,
    relation: DuplicateRelation,
    status: DuplicateReviewStatus,
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
    evidence: DuplicatePairEvidence,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    book_a: BookSummary,
    book_b: BookSummary,
}

#[derive(Debug, Serialize)]
struct PairListResponse {
    items: Vec<PairSummary>,
    total: i64,
    limit: i64,
    offset: i64,
}

#[derive(Debug, Serialize)]
struct ExactFileDiscoveryListResponse {
    items: Vec<ExactFileDiscoveryRecord>,
    total: i64,
    limit: i64,
    offset: i64,
}

#[derive(Debug, Serialize)]
struct ChapterMatchResponse {
    id: Uuid,
    match_type: String,
    similarity: f64,
    shared_fingerprints: i32,
    alignment_group: Option<i32>,
    segment_ordinal: Option<i32>,
    chapter_a_start: Option<i32>,
    chapter_a_end: Option<i32>,
    chapter_b_start: Option<i32>,
    chapter_b_end: Option<i32>,
    matched_chars: i32,
    chapter_a_id: Option<Uuid>,
    chapter_a_index: Option<i32>,
    chapter_a_title: Option<String>,
    chapter_b_id: Option<Uuid>,
    chapter_b_index: Option<i32>,
    chapter_b_title: Option<String>,
}

impl From<DuplicateChapterMatchRecord> for ChapterMatchResponse {
    fn from(record: DuplicateChapterMatchRecord) -> Self {
        Self {
            id: record.id,
            match_type: record.match_type,
            similarity: record.similarity,
            shared_fingerprints: record.shared_fingerprints,
            alignment_group: record.alignment_group,
            segment_ordinal: record.segment_ordinal,
            chapter_a_start: record.chapter_a_start,
            chapter_a_end: record.chapter_a_end,
            chapter_b_start: record.chapter_b_start,
            chapter_b_end: record.chapter_b_end,
            matched_chars: record.matched_chars,
            chapter_a_id: record.chapter_a_id,
            chapter_a_index: record.chapter_a_index,
            chapter_a_title: record.chapter_a_title,
            chapter_b_id: record.chapter_b_id,
            chapter_b_index: record.chapter_b_index,
            chapter_b_title: record.chapter_b_title,
        }
    }
}

impl From<DuplicatePairRecord> for PairSummary {
    fn from(row: DuplicatePairRecord) -> Self {
        Self {
            id: row.id,
            relation: row.relation,
            status: row.status,
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
            evidence: row.evidence,
            created_at: row.created_at,
            updated_at: row.updated_at,
            book_a: BookSummary {
                id: row.book_a_id,
                title: row.book_a_title,
                author: row.book_a_author,
                format: row.book_a_format,
                file_size: row.book_a_file_size,
                word_count: row.book_a_word_count,
                chapter_count: row.book_a_chapter_count,
                cover_path: row.book_a_cover_path,
            },
            book_b: BookSummary {
                id: row.book_b_id,
                title: row.book_b_title,
                author: row.book_b_author,
                format: row.book_b_format,
                file_size: row.book_b_file_size,
                word_count: row.book_b_word_count,
                chapter_count: row.book_b_chapter_count,
                cover_path: row.book_b_cover_path,
            },
        }
    }
}

#[derive(Debug, Serialize)]
struct PairDetail {
    #[serde(flatten)]
    pair: PairSummary,
    chapter_matches: Vec<ChapterMatchResponse>,
    chapter_matches_total: i64,
    chapter_matches_limit: i64,
    chapter_matches_offset: i64,
    matched_indices_a: Vec<i32>,
    matched_indices_b: Vec<i32>,
}

async fn fetch_scan(state: &AppState, id: Uuid) -> ApiResult<ScanResponse> {
    state
        .duplicates
        .scan(id)
        .await?
        .map(Into::into)
        .ok_or_else(|| ApiError::not_found("duplicate scan"))
}

async fn start_scan(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<StartScanRequest>,
) -> ApiResult<Json<ScanResponse>> {
    let user_id = auth_user_id(&auth)?;
    let library_id = resolve_scan_scope(&state, &auth, body.library_id).await?;

    let scan_id = dedup::enqueue_scan(&state, library_id, user_id, body.include_semantic).await?;

    Ok(Json(fetch_scan(&state, scan_id).await?))
}

async fn resolve_scan_scope(
    state: &AppState,
    auth: &AuthUser,
    requested: Option<Uuid>,
) -> ApiResult<Option<Uuid>> {
    if let Some(id) = requested {
        ensure_library_access(state, auth, id, LibraryAccess::Write).await?;
        return Ok(Some(id));
    }

    if is_admin(state, auth).await? {
        return Ok(None);
    }

    let visible = visible_library_ids(state, auth, LibraryAccess::Write)
        .await?
        .unwrap_or_default();
    if visible.len() == 1 {
        return Ok(visible.first().copied());
    }

    if let Some(default_id) = default_library_id(state).await? {
        if visible.contains(&default_id) {
            return Ok(Some(default_id));
        }
    }

    Err(ApiError::bad_request(
        "library_id is required when multiple writable libraries are visible",
    ))
}

async fn latest_scan(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(query): Query<ScanQuery>,
) -> ApiResult<Json<Option<ScanResponse>>> {
    if let Some(id) = query.library_id {
        ensure_library_access(&state, &auth, id, LibraryAccess::Read).await?;
    } else if !is_admin(&state, &auth).await? {
        let visible = visible_library_ids(&state, &auth, LibraryAccess::Read)
            .await?
            .unwrap_or_default();
        let row = state
            .duplicates
            .latest_scan_in_libraries(&visible)
            .await?
            .map(Into::into);
        return Ok(Json(row));
    }

    let row = state
        .duplicates
        .latest_scan(query.library_id)
        .await?
        .map(Into::into);

    Ok(Json(row))
}

async fn get_scan(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<ScanResponse>> {
    let scan = fetch_scan(&state, id).await?;
    if let Some(library_id) = scan.library_id {
        ensure_library_access(&state, &auth, library_id, LibraryAccess::Read).await?;
    } else if !is_admin(&state, &auth).await? {
        return Err(ApiError::forbidden());
    }
    Ok(Json(scan))
}

async fn list_pairs(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(query): Query<PairQuery>,
) -> ApiResult<Json<PairListResponse>> {
    if let Some(library_id) = query.library_id {
        ensure_library_access(&state, &auth, library_id, LibraryAccess::Read).await?;
    }

    let visible_ids = visible_library_ids(&state, &auth, LibraryAccess::Read).await?;
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let offset = query.offset.unwrap_or(0).max(0);

    if matches!(&visible_ids, Some(ids) if ids.is_empty()) {
        return Ok(Json(PairListResponse {
            items: Vec::new(),
            total: 0,
            limit,
            offset,
        }));
    }

    let rows = state
        .duplicates
        .list_pairs(DuplicatePairFilter {
            visible_library_ids: visible_ids.as_deref(),
            library_id: query.library_id,
            candidate_kind: query.candidate_kind,
            relation: query.relation,
            status: query.status,
            limit,
            offset,
        })
        .await?;
    let total = rows.first().map_or(0, |row| row.total_count);
    Ok(Json(PairListResponse {
        items: rows.into_iter().map(Into::into).collect(),
        total,
        limit,
        offset,
    }))
}

async fn list_exact_file_discoveries(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(query): Query<ExactFileDiscoveryQuery>,
) -> ApiResult<Json<ExactFileDiscoveryListResponse>> {
    if let Some(library_id) = query.library_id {
        ensure_library_access(&state, &auth, library_id, LibraryAccess::Read).await?;
    }

    let visible_ids = visible_library_ids(&state, &auth, LibraryAccess::Read).await?;
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let offset = query.offset.unwrap_or(0).max(0);

    if matches!(&visible_ids, Some(ids) if ids.is_empty()) {
        return Ok(Json(ExactFileDiscoveryListResponse {
            items: Vec::new(),
            total: 0,
            limit,
            offset,
        }));
    }

    let rows = state
        .duplicates
        .list_exact_file_discoveries(ExactFileDiscoveryFilter {
            visible_library_ids: visible_ids.as_deref(),
            library_id: query.library_id,
            limit,
            offset,
        })
        .await?;
    let total = rows.first().map_or(0, |row| row.total_count);

    Ok(Json(ExactFileDiscoveryListResponse {
        items: rows,
        total,
        limit,
        offset,
    }))
}

async fn get_pair(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Query(query): Query<PairDetailQuery>,
) -> ApiResult<Json<PairDetail>> {
    let row = state
        .duplicates
        .pair(id)
        .await?
        .ok_or_else(|| ApiError::not_found("duplicate pair"))?;
    ensure_book_access(&state, &auth, row.book_a_id, LibraryAccess::Read).await?;
    ensure_book_access(&state, &auth, row.book_b_id, LibraryAccess::Read).await?;

    let match_limit = query.match_limit.unwrap_or(100).clamp(1, 500);
    let match_offset = query.match_offset.unwrap_or(0).max(0);
    let matches = state
        .duplicates
        .chapter_matches(id, match_limit, match_offset)
        .await?;

    Ok(Json(PairDetail {
        pair: row.into(),
        chapter_matches: matches.items.into_iter().map(Into::into).collect(),
        chapter_matches_total: matches.total,
        chapter_matches_limit: match_limit,
        chapter_matches_offset: match_offset,
        matched_indices_a: matches.matched_indices_a,
        matched_indices_b: matches.matched_indices_b,
    }))
}

async fn resolve_pair(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ResolveRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let books = state
        .duplicates
        .pair_book_ids(id)
        .await?
        .ok_or_else(|| ApiError::not_found("duplicate pair"))?;
    ensure_book_access(&state, &auth, books.0, LibraryAccess::Write).await?;
    ensure_book_access(&state, &auth, books.1, LibraryAccess::Write).await?;
    if body.action.groups_versions() {
        let pair_books = [books.0, books.1];
        let work_member_ids = state
            .duplicates
            .work_member_ids_for_books(&pair_books)
            .await?;
        for member_id in work_member_ids {
            ensure_book_access(&state, &auth, member_id, LibraryAccess::Write).await?;
        }
    }

    let result = dedup::resolve_pair(&state, id, body.action, auth_user_id(&auth)?).await?;
    Ok(Json(result))
}

async fn list_book_versions(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_book_access(&state, &auth, id, LibraryAccess::Read).await?;
    let visible_ids = visible_library_ids(&state, &auth, LibraryAccess::Read).await?;
    let records = state
        .duplicates
        .book_versions(id, visible_ids.as_deref())
        .await?;
    Ok(Json(book_versions_payload(records)))
}

fn book_versions_payload(records: BookVersionsRecord) -> serde_json::Value {
    let work_id = records.work_id;
    let work = records.work.map(|work| {
        serde_json::json!({
            "id": work.id,
            "canonical_title": work.canonical_title,
            "canonical_author": work.canonical_author,
            "primary_book_id": work.primary_book_id,
        })
    });
    let versions: Vec<_> = records
        .versions
        .into_iter()
        .map(|version| {
            serde_json::json!({
                "id": version.id,
                "title": version.title,
                "author": version.author,
                "format": version.format,
                "status": version.status,
                "chapter_count": version.chapter_count,
                "word_count": version.word_count,
                "file_size": version.file_size_bytes,
                "cover_path": version.cover_path,
                "created_at": version.created_at,
                "is_primary": version.is_primary.unwrap_or(work_id.is_none()),
            })
        })
        .collect();
    serde_json::json!({
        "work": work,
        "versions": versions,
    })
}

async fn get_chapter_diff(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path((pair_id, match_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<serde_json::Value>> {
    let source = state
        .duplicates
        .chapter_diff_source(pair_id, match_id)
        .await?
        .ok_or_else(|| ApiError::not_found("duplicate chapter match"))?;
    ensure_book_access(&state, &auth, source.book_a_id, LibraryAccess::Read).await?;
    ensure_book_access(&state, &auth, source.book_b_id, LibraryAccess::Read).await?;

    let content_a = aligned_segment(
        &source.content_a,
        source.chapter_a_start,
        source.chapter_a_end,
    );
    let content_b = aligned_segment(
        &source.content_b,
        source.chapter_b_start,
        source.chapter_b_end,
    );
    let diff = dedup::compare_chapter_texts(&content_a, &content_b);

    Ok(Json(serde_json::json!({
        "pair_id": pair_id,
        "match_id": match_id,
        "chapter_a": {
            "id": source.chapter_a_id,
            "title": source.chapter_a_title,
            "character_count": diff.character_count_a,
        },
        "chapter_b": {
            "id": source.chapter_b_id,
            "title": source.chapter_b_title,
            "character_count": diff.character_count_b,
        },
        "changes": diff.changes,
        "ratio": diff.ratio,
        "truncated": diff.truncated,
    })))
}

fn aligned_segment(content: &str, start: Option<i32>, end: Option<i32>) -> String {
    let (Some(start), Some(end)) = (start, end) else {
        return content.to_string();
    };
    let (Ok(start), Ok(end)) = (usize::try_from(start), usize::try_from(end)) else {
        return content.to_string();
    };
    let normalized: Vec<char> = nova_ingest::dedup::normalize_layout(content)
        .chars()
        .collect();
    normalized
        .get(start..end)
        .map(|segment| segment.iter().collect())
        .unwrap_or_else(|| content.to_string())
}

fn deserialize_reviewable_relation<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<DuplicateRelation>, D::Error>
where
    D: Deserializer<'de>,
{
    let relation = Option::<DuplicateRelation>::deserialize(deserializer)?;
    if matches!(relation, Some(value) if !value.is_reviewable()) {
        return Err(de::Error::custom(
            "not_duplicate is a classifier result, not a reviewable relation",
        ));
    }
    Ok(relation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::pg_duplicate::BookVersionRecord;

    #[test]
    fn pair_query_uses_closed_relation_and_status_vocabularies() {
        let query: PairQuery = serde_json::from_value(serde_json::json!({
            "candidate_kind": "content",
            "relation": "contained_version",
            "status": "pending"
        }))
        .expect("known filter values should deserialize");

        assert_eq!(query.candidate_kind, Some(DuplicateCandidateKind::Content));
        assert_eq!(query.relation, Some(DuplicateRelation::ContainedVersion));
        assert_eq!(query.status, Some(DuplicateReviewStatus::Pending));
        assert!(serde_json::from_value::<PairQuery>(serde_json::json!({
            "relation": "made_up"
        }))
        .is_err());
        assert!(serde_json::from_value::<PairQuery>(serde_json::json!({
            "relation": "not_duplicate"
        }))
        .is_err());
    }

    #[test]
    fn resolution_action_uses_stable_wire_names() {
        assert_eq!(ResolveAction::KeepA.as_str(), "keep_a");
        assert_eq!(ResolveAction::SameWork.as_str(), "same_work");
    }

    #[test]
    fn grouped_diff_uses_normalized_segment_offsets() {
        assert_eq!(aligned_segment("甲 乙丙丁", Some(1), Some(3)), "乙丙");
        assert_eq!(aligned_segment("甲乙", Some(9), Some(12)), "甲乙");
    }

    #[test]
    fn version_payload_does_not_expose_hidden_work_for_one_visible_member() {
        let visible_id = Uuid::now_v7();
        let records = BookVersionsRecord {
            work_id: Some(Uuid::now_v7()),
            work: None,
            versions: vec![BookVersionRecord {
                id: visible_id,
                title: "Visible version".to_string(),
                author: Some("Visible author".to_string()),
                format: "txt".to_string(),
                status: "ready".to_string(),
                chapter_count: 10,
                word_count: 1_000,
                file_size_bytes: 2_000,
                cover_path: None,
                created_at: Utc::now(),
                is_primary: Some(false),
            }],
        };

        let payload = book_versions_payload(records);

        assert!(payload["work"].is_null());
        assert_eq!(payload["versions"].as_array().map(Vec::len), Some(1));
        assert_eq!(payload["versions"][0]["id"], visible_id.to_string());
        assert_eq!(payload["versions"][0]["is_primary"], false);
    }
}
