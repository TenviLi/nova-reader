//! Semantic Intelligence routes: Tag Profiles, Auto-Tagging, Trope Heatmap, Vibe Search.
//!
//! Provides endpoints for managing semantic tag profiles and computing similarity-based
//! tag scores across the novel library.

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use nova_core::domain::task::TaskKind;

use crate::access::{ensure_book_access, visible_library_ids, LibraryAccess};
use crate::error::{ApiError, ApiResult};
use crate::extractors::AuthUser;
use crate::state::AppState;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        // Tag Profile CRUD
        .route("/semantic-tags/profiles", get(list_profiles))
        .route("/semantic-tags/profiles", post(create_profile))
        .route("/semantic-tags/profiles/{id}", get(get_profile))
        .route("/semantic-tags/profiles/{id}", put(update_profile))
        .route("/semantic-tags/profiles/{id}", delete(delete_profile))
        .route(
            "/semantic-tags/profiles/{id}/compute-embedding",
            post(compute_profile_embedding),
        )
        // Book Tag Scores
        .route(
            "/semantic-tags/books/{book_id}/scores",
            get(get_book_scores),
        )
        .route("/semantic-tags/books/{book_id}/heatmap", get(get_heatmap))
        .route(
            "/semantic-tags/books/{book_id}/markers",
            get(get_content_markers),
        )
        .route(
            "/semantic-tags/books/{book_id}/compute",
            post(trigger_book_tagging),
        )
        // Library-wide overview
        .route("/semantic-tags/overview", get(library_tag_overview))
        .route("/semantic-tags/radar/{book_id}", get(get_book_radar))
        // Vibe Search
        .route("/search/vibe", post(vibe_search))
        .route("/search/vibe/bookmark", post(save_vibe_bookmark))
        .route("/search/vibe/bookmarks", get(list_vibe_bookmarks))
}

// ─── Data Types ──────────────────────────────────────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
struct TagProfile {
    id: Uuid,
    user_id: Uuid,
    name: String,
    description: Option<String>,
    category: String,
    color: String,
    icon: Option<String>,
    reference_texts: Vec<String>,
    match_threshold: f32,
    is_warning: bool,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Deserialize)]
struct CreateProfileRequest {
    name: String,
    description: Option<String>,
    category: Option<String>,
    color: Option<String>,
    icon: Option<String>,
    reference_texts: Vec<String>,
    match_threshold: Option<f32>,
    is_warning: Option<bool>,
}

#[derive(Deserialize)]
struct UpdateProfileRequest {
    name: Option<String>,
    description: Option<String>,
    category: Option<String>,
    color: Option<String>,
    icon: Option<String>,
    reference_texts: Option<Vec<String>>,
    match_threshold: Option<f32>,
    is_warning: Option<bool>,
}

#[derive(Serialize, sqlx::FromRow)]
struct BookTagScore {
    id: Uuid,
    book_id: Uuid,
    tag_profile_id: Uuid,
    concentration: f32,
    match_count: i32,
    total_chunks: i32,
    peak_chapter: Option<i32>,
    peak_score: Option<f32>,
    computed_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize, sqlx::FromRow)]
struct ChapterTagScore {
    tag_profile_id: Uuid,
    chapter_index: i32,
    score: f32,
    top_snippet: Option<String>,
    top_chunk_score: Option<f32>,
}

#[derive(Serialize, sqlx::FromRow)]
struct ContentMarker {
    id: Uuid,
    tag_profile_id: Uuid,
    chapter_index: i32,
    chunk_index: i32,
    similarity_score: f32,
    content_snippet: String,
    char_offset: Option<i32>,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Deserialize)]
struct VibeSearchRequest {
    /// The text to find similar passages for
    text: String,
    /// Optional: limit to specific books
    #[serde(default)]
    book_ids: Vec<Uuid>,
    /// Max results
    #[serde(default = "default_vibe_limit")]
    limit: usize,
    /// Min similarity threshold
    #[serde(default = "default_vibe_threshold")]
    threshold: f32,
}

fn default_vibe_limit() -> usize {
    20
}
fn default_vibe_threshold() -> f32 {
    0.4
}

#[derive(Deserialize)]
struct SaveVibeBookmarkRequest {
    name: Option<String>,
    source_text: String,
    source_book_id: Option<Uuid>,
    source_chapter_index: Option<i32>,
}

#[derive(Deserialize)]
struct HeatmapQuery {
    /// Which tag profiles to include (comma-separated IDs). If empty, use all.
    tag_ids: Option<String>,
}

#[derive(Deserialize)]
struct OverviewQuery {
    /// Minimum concentration to include
    min_concentration: Option<f32>,
    /// Sort by: 'concentration', 'match_count', 'book_title'
    sort: Option<String>,
    limit: Option<i64>,
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn parse_user_id(auth: &AuthUser) -> Result<Uuid, ApiError> {
    Uuid::parse_str(&auth.id).map_err(|_| ApiError::unauthorized())
}

async fn ensure_semantic_book_read_access(
    state: &AppState,
    auth: &AuthUser,
    book_id: Uuid,
) -> ApiResult<()> {
    ensure_book_access(state, auth, book_id, LibraryAccess::Read).await
}

async fn ensure_semantic_book_write_access(
    state: &AppState,
    auth: &AuthUser,
    book_id: Uuid,
) -> ApiResult<()> {
    ensure_book_access(state, auth, book_id, LibraryAccess::Write).await
}

async fn ensure_optional_source_book_access(
    state: &AppState,
    auth: &AuthUser,
    source_book_id: Option<Uuid>,
) -> ApiResult<()> {
    if let Some(book_id) = source_book_id {
        ensure_semantic_book_read_access(state, auth, book_id).await?;
    }
    Ok(())
}

async fn visible_vibe_book_ids(
    state: &AppState,
    auth: &AuthUser,
    requested_book_ids: &[Uuid],
) -> ApiResult<Option<Vec<Uuid>>> {
    if !requested_book_ids.is_empty() {
        for &book_id in requested_book_ids {
            ensure_semantic_book_read_access(state, auth, book_id).await?;
        }
        return Ok(Some(requested_book_ids.to_vec()));
    }

    match visible_library_ids(state, auth, LibraryAccess::Read).await? {
        None => Ok(None),
        Some(library_ids) if library_ids.is_empty() => Ok(Some(Vec::new())),
        Some(library_ids) => {
            let book_ids =
                sqlx::query_scalar("SELECT id FROM books WHERE library_id = ANY($1::uuid[])")
                    .bind(&library_ids)
                    .fetch_all(&state.db)
                    .await
                    .map_err(ApiError::from)?;
            Ok(Some(book_ids))
        }
    }
}

fn overview_top_books_sql(scoped: bool, sort: &str) -> String {
    let order_by = match sort {
        "match_count" => "bts.match_count DESC, bts.concentration DESC",
        "book_title" => "b.title ASC NULLS LAST, bts.concentration DESC",
        _ => "bts.concentration DESC",
    };

    format!(
        r#"
        SELECT bts.book_id, bts.concentration, bts.match_count, b.title
        FROM book_tag_scores bts
        JOIN books b ON b.id = bts.book_id
        WHERE bts.tag_profile_id = $1 AND bts.concentration >= $2
          {}
        ORDER BY {order_by}
        LIMIT $3
        "#,
        if scoped {
            "AND b.library_id = ANY($4::uuid[])"
        } else {
            ""
        }
    )
}

// ─── Tag Profile CRUD ────────────────────────────────────────────────────────

async fn list_profiles(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<Json<Vec<TagProfile>>> {
    let user_id = parse_user_id(&auth)?;
    let profiles = sqlx::query_as::<_, TagProfile>(
        "SELECT id, user_id, name, description, category, color, icon, reference_texts, match_threshold, is_warning, created_at, updated_at
         FROM tag_profiles WHERE user_id = $1 ORDER BY category, name"
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiError::internal(format!("DB error: {}", e)))?;

    Ok(Json(profiles))
}

async fn create_profile(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<CreateProfileRequest>,
) -> ApiResult<Json<TagProfile>> {
    let user_id = parse_user_id(&auth)?;
    if req.name.trim().is_empty() {
        return Err(ApiError::bad_request("Tag name cannot be empty"));
    }
    if req.reference_texts.is_empty() {
        return Err(ApiError::bad_request(
            "At least one reference text is required",
        ));
    }

    let profile = sqlx::query_as::<_, TagProfile>(
        "INSERT INTO tag_profiles (user_id, name, description, category, color, icon, reference_texts, match_threshold, is_warning)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         RETURNING id, user_id, name, description, category, color, icon, reference_texts, match_threshold, is_warning, created_at, updated_at"
    )
    .bind(user_id)
    .bind(req.name.trim())
    .bind(req.description)
    .bind(req.category.unwrap_or_else(|| "custom".to_string()))
    .bind(req.color.unwrap_or_else(|| "#6366f1".to_string()))
    .bind(req.icon)
    .bind(&req.reference_texts)
    .bind(req.match_threshold.unwrap_or(0.45))
    .bind(req.is_warning.unwrap_or(false))
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to create profile: {}", e)))?;

    Ok(Json(profile))
}

async fn get_profile(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    auth: AuthUser,
) -> ApiResult<Json<TagProfile>> {
    let user_id = parse_user_id(&auth)?;
    let profile = sqlx::query_as::<_, TagProfile>(
        "SELECT id, user_id, name, description, category, color, icon, reference_texts, match_threshold, is_warning, created_at, updated_at
         FROM tag_profiles WHERE id = $1 AND user_id = $2"
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::internal(format!("DB error: {}", e)))?
    .ok_or_else(|| ApiError::not_found("Tag profile not found"))?;

    Ok(Json(profile))
}

async fn update_profile(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    auth: AuthUser,
    Json(req): Json<UpdateProfileRequest>,
) -> ApiResult<Json<TagProfile>> {
    let user_id = parse_user_id(&auth)?;
    // Verify ownership
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM tag_profiles WHERE id = $1 AND user_id = $2)",
    )
    .bind(id)
    .bind(user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiError::internal(format!("DB error: {}", e)))?;

    if !exists {
        return Err(ApiError::not_found("Tag profile not found"));
    }

    let profile = sqlx::query_as::<_, TagProfile>(
        "UPDATE tag_profiles SET
            name = COALESCE($3, name),
            description = COALESCE($4, description),
            category = COALESCE($5, category),
            color = COALESCE($6, color),
            icon = COALESCE($7, icon),
            reference_texts = COALESCE($8, reference_texts),
            match_threshold = COALESCE($9, match_threshold),
            is_warning = COALESCE($10, is_warning),
            updated_at = NOW()
         WHERE id = $1 AND user_id = $2
         RETURNING id, user_id, name, description, category, color, icon, reference_texts, match_threshold, is_warning, created_at, updated_at"
    )
    .bind(id)
    .bind(user_id)
    .bind(req.name)
    .bind(req.description)
    .bind(req.category)
    .bind(req.color)
    .bind(req.icon)
    .bind(req.reference_texts.as_deref())
    .bind(req.match_threshold)
    .bind(req.is_warning)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to update profile: {}", e)))?;

    Ok(Json(profile))
}

async fn delete_profile(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    auth: AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    let user_id = parse_user_id(&auth)?;
    let result = sqlx::query("DELETE FROM tag_profiles WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user_id)
        .execute(&state.db)
        .await
        .map_err(|e| ApiError::internal(format!("DB error: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::not_found("Tag profile not found"));
    }

    Ok(Json(serde_json::json!({ "deleted": true })))
}

/// Compute the average embedding for a tag profile from its reference texts.
async fn compute_profile_embedding(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    auth: AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    let user_id = parse_user_id(&auth)?;
    let profile = sqlx::query_as::<_, TagProfile>(
        "SELECT id, user_id, name, description, category, color, icon, reference_texts, match_threshold, is_warning, created_at, updated_at
         FROM tag_profiles WHERE id = $1 AND user_id = $2"
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::internal(format!("DB error: {}", e)))?
    .ok_or_else(|| ApiError::not_found("Tag profile not found"))?;

    if profile.reference_texts.is_empty() {
        return Err(ApiError::bad_request("No reference texts to embed"));
    }

    // Embed all reference texts
    let texts: Vec<&str> = profile.reference_texts.iter().map(|s| s.as_str()).collect();
    let embeddings = embed_texts_batch(&state, &texts)
        .await
        .map_err(|e| ApiError::internal(format!("Embedding failed: {}", e)))?;

    if embeddings.is_empty() {
        return Err(ApiError::internal("No embeddings returned"));
    }

    // Compute centroid (average vector)
    let dim = embeddings[0].len();
    let mut centroid = vec![0.0f32; dim];
    for emb in &embeddings {
        for (i, v) in emb.iter().enumerate() {
            centroid[i] += v;
        }
    }
    let n = embeddings.len() as f32;
    for v in centroid.iter_mut() {
        *v /= n;
    }

    // L2 normalize the centroid
    let norm: f32 = centroid.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm > 0.0 {
        for v in centroid.iter_mut() {
            *v /= norm;
        }
    }

    // Store embedding
    sqlx::query("UPDATE tag_profiles SET embedding = $1, updated_at = NOW() WHERE id = $2")
        .bind(&centroid)
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to store embedding: {}", e)))?;

    Ok(Json(serde_json::json!({
        "profile_id": id,
        "dimensions": dim,
        "reference_count": embeddings.len(),
        "status": "computed"
    })))
}

// ─── Book Tag Scores & Heatmap ───────────────────────────────────────────────

async fn get_book_scores(
    State(state): State<Arc<AppState>>,
    Path(book_id): Path<Uuid>,
    auth: AuthUser,
) -> ApiResult<Json<Vec<serde_json::Value>>> {
    ensure_semantic_book_read_access(&state, &auth, book_id).await?;
    let user_id = parse_user_id(&auth)?;
    let scores = sqlx::query_as::<_, BookTagScore>(
        "SELECT bts.id, bts.book_id, bts.tag_profile_id, bts.concentration, bts.match_count,
                bts.total_chunks, bts.peak_chapter, bts.peak_score, bts.computed_at
         FROM book_tag_scores bts
         WHERE bts.book_id = $1
         ORDER BY bts.concentration DESC",
    )
    .bind(book_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiError::internal(format!("DB error: {}", e)))?;

    // Join with profile info
    let mut results = Vec::new();
    for score in scores {
        let profile_info: Option<(String, String, String, bool)> = sqlx::query_as(
            "SELECT name, color, category, is_warning FROM tag_profiles WHERE id = $1 AND user_id = $2"
        )
        .bind(score.tag_profile_id)
        .bind(user_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| ApiError::internal(format!("DB error: {}", e)))?;

        if let Some((name, color, category, is_warning)) = profile_info {
            results.push(serde_json::json!({
                "tag_profile_id": score.tag_profile_id,
                "name": name,
                "color": color,
                "category": category,
                "is_warning": is_warning,
                "concentration": score.concentration,
                "match_count": score.match_count,
                "total_chunks": score.total_chunks,
                "peak_chapter": score.peak_chapter,
                "peak_score": score.peak_score,
                "computed_at": score.computed_at,
            }));
        }
    }

    Ok(Json(results))
}

/// Get chapter-level tag scores for heatmap visualization.
async fn get_heatmap(
    State(state): State<Arc<AppState>>,
    Path(book_id): Path<Uuid>,
    auth: AuthUser,
    Query(q): Query<HeatmapQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_semantic_book_read_access(&state, &auth, book_id).await?;
    let user_id = parse_user_id(&auth)?;
    // Parse tag_ids filter
    let tag_ids: Vec<Uuid> = q
        .tag_ids
        .as_deref()
        .unwrap_or("")
        .split(',')
        .filter_map(|s| Uuid::parse_str(s.trim()).ok())
        .collect();

    let scores = if tag_ids.is_empty() {
        sqlx::query_as::<_, ChapterTagScore>(
            "SELECT tag_profile_id, chapter_index, score, top_snippet, top_chunk_score
             FROM chapter_tag_scores cts
             JOIN tag_profiles tp ON tp.id = cts.tag_profile_id
             WHERE cts.book_id = $1 AND tp.user_id = $2
             ORDER BY cts.chapter_index, cts.tag_profile_id",
        )
        .bind(book_id)
        .bind(user_id)
        .fetch_all(&state.db)
        .await
        .map_err(|e| ApiError::internal(format!("DB error: {}", e)))?
    } else {
        sqlx::query_as::<_, ChapterTagScore>(
            "SELECT tag_profile_id, chapter_index, score, top_snippet, top_chunk_score
             FROM chapter_tag_scores cts
             JOIN tag_profiles tp ON tp.id = cts.tag_profile_id
             WHERE cts.book_id = $1 AND cts.tag_profile_id = ANY($2) AND tp.user_id = $3
             ORDER BY cts.chapter_index, cts.tag_profile_id",
        )
        .bind(book_id)
        .bind(&tag_ids)
        .bind(user_id)
        .fetch_all(&state.db)
        .await
        .map_err(|e| ApiError::internal(format!("DB error: {}", e)))?
    };

    // Get tag profile info for legend
    let profiles: Vec<(Uuid, String, String)> = if tag_ids.is_empty() {
        sqlx::query_as(
            "SELECT DISTINCT tp.id, tp.name, tp.color FROM tag_profiles tp
             JOIN chapter_tag_scores cts ON cts.tag_profile_id = tp.id
             WHERE cts.book_id = $1 AND tp.user_id = $2",
        )
        .bind(book_id)
        .bind(user_id)
        .fetch_all(&state.db)
        .await
        .map_err(|e| ApiError::internal(format!("DB error: {}", e)))?
    } else {
        sqlx::query_as(
            "SELECT id, name, color FROM tag_profiles WHERE id = ANY($1) AND user_id = $2",
        )
        .bind(&tag_ids)
        .bind(user_id)
        .fetch_all(&state.db)
        .await
        .map_err(|e| ApiError::internal(format!("DB error: {}", e)))?
    };

    // Get total chapter count
    let total_chapters: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM chapters WHERE book_id = $1")
        .bind(book_id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| ApiError::internal(format!("DB error: {}", e)))?;

    Ok(Json(serde_json::json!({
        "book_id": book_id,
        "total_chapters": total_chapters.0,
        "scores": scores,
        "profiles": profiles.iter().map(|(id, name, color)| serde_json::json!({
            "id": id, "name": name, "color": color
        })).collect::<Vec<_>>(),
    })))
}

/// Get content markers (specific high-similarity chunks) for a book.
async fn get_content_markers(
    State(state): State<Arc<AppState>>,
    Path(book_id): Path<Uuid>,
    auth: AuthUser,
) -> ApiResult<Json<Vec<ContentMarker>>> {
    ensure_semantic_book_read_access(&state, &auth, book_id).await?;
    let user_id = parse_user_id(&auth)?;
    let markers = sqlx::query_as::<_, ContentMarker>(
        "SELECT cm.id, cm.tag_profile_id, cm.chapter_index, cm.chunk_index, cm.similarity_score,
                cm.content_snippet, cm.char_offset, cm.created_at
         FROM content_markers cm
         JOIN tag_profiles tp ON tp.id = cm.tag_profile_id
         WHERE cm.book_id = $1 AND tp.user_id = $2
         ORDER BY cm.similarity_score DESC LIMIT 100",
    )
    .bind(book_id)
    .bind(user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiError::internal(format!("DB error: {}", e)))?;

    Ok(Json(markers))
}

/// Trigger semantic tagging computation for a specific book.
async fn trigger_book_tagging(
    State(state): State<Arc<AppState>>,
    Path(book_id): Path<Uuid>,
    auth: AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_semantic_book_write_access(&state, &auth, book_id).await?;
    let user_id = parse_user_id(&auth)?;
    // Submit a semantic_tagging task
    let task_id = state
        .task_queue
        .submit(
            TaskKind::SemanticTagging,
            Some(book_id),
            serde_json::json!({ "user_id": user_id.to_string() }),
        )
        .await
        .map_err(|e| ApiError::internal(format!("Failed to submit task: {}", e)))?;

    Ok(Json(serde_json::json!({
        "task_id": task_id,
        "message": "Semantic tagging queued",
    })))
}

// ─── Library-wide Overview ───────────────────────────────────────────────────

/// Get an overview of tag scores across the entire library.
async fn library_tag_overview(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(q): Query<OverviewQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let user_id = parse_user_id(&auth)?;
    let min_conc = q.min_concentration.unwrap_or(0.0);
    let limit = q.limit.unwrap_or(50).min(200);
    let sort = q.sort.as_deref().unwrap_or("concentration");

    // Get all user's tag profiles
    let profiles: Vec<(Uuid, String, String, String, bool)> = sqlx::query_as(
        "SELECT id, name, color, category, is_warning FROM tag_profiles WHERE user_id = $1 ORDER BY category, name"
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiError::internal(format!("DB error: {}", e)))?;

    let visible_libraries = visible_library_ids(&state, &auth, LibraryAccess::Read).await?;
    if matches!(visible_libraries, Some(ref ids) if ids.is_empty()) {
        return Ok(Json(serde_json::json!({
            "profiles": [],
            "total_profiles": profiles.len(),
        })));
    }
    let visible_ids = visible_libraries.as_deref();

    // For each profile, get top books
    let mut overview = Vec::new();
    for (profile_id, name, color, category, is_warning) in &profiles {
        let sql = overview_top_books_sql(visible_ids.is_some(), sort);
        let mut query = sqlx::query_as(&sql)
            .bind(profile_id)
            .bind(min_conc)
            .bind(limit);
        if let Some(ids) = visible_ids {
            query = query.bind(ids);
        }
        let top_books: Vec<(Uuid, f32, i32, Option<String>)> = query
            .fetch_all(&state.db)
            .await
            .map_err(|e| ApiError::internal(format!("DB error: {}", e)))?;

        overview.push(serde_json::json!({
            "profile_id": profile_id,
            "name": name,
            "color": color,
            "category": category,
            "is_warning": is_warning,
            "total_matches": top_books.len(),
            "books": top_books.iter().map(|(bid, conc, count, title)| serde_json::json!({
                "book_id": bid,
                "title": title,
                "concentration": conc,
                "match_count": count,
            })).collect::<Vec<_>>(),
        }));
    }

    Ok(Json(serde_json::json!({
        "profiles": overview,
        "total_profiles": profiles.len(),
    })))
}

/// Get radar chart data for a single book (all tag scores).
async fn get_book_radar(
    State(state): State<Arc<AppState>>,
    Path(book_id): Path<Uuid>,
    auth: AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_semantic_book_read_access(&state, &auth, book_id).await?;
    let user_id = parse_user_id(&auth)?;
    let axes: Vec<(String, String, f32, bool)> = sqlx::query_as(
        "SELECT tp.name, tp.color, COALESCE(bts.concentration, 0.0), tp.is_warning
         FROM tag_profiles tp
         LEFT JOIN book_tag_scores bts ON bts.tag_profile_id = tp.id AND bts.book_id = $1
         WHERE tp.user_id = $2
         ORDER BY tp.category, tp.name",
    )
    .bind(book_id)
    .bind(user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiError::internal(format!("DB error: {}", e)))?;

    Ok(Json(serde_json::json!({
        "book_id": book_id,
        "axes": axes.iter().map(|(name, color, score, is_warning)| serde_json::json!({
            "name": name,
            "color": color,
            "score": score,
            "is_warning": is_warning,
        })).collect::<Vec<_>>(),
    })))
}

// ─── Vibe Search ─────────────────────────────────────────────────────────────

/// Semantic passage search: find chunks with similar "vibe" to input text.
async fn vibe_search(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<VibeSearchRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    if req.text.trim().is_empty() {
        return Err(ApiError::bad_request("Search text cannot be empty"));
    }
    if req.text.len() > 5000 {
        return Err(ApiError::bad_request("Text exceeds 5000 character limit"));
    }

    let start = std::time::Instant::now();
    let visible_book_ids = visible_vibe_book_ids(&state, &auth, &req.book_ids).await?;
    if matches!(visible_book_ids, Some(ref ids) if ids.is_empty()) {
        return Ok(Json(serde_json::json!({
            "results": [],
            "total": 0,
            "elapsed_ms": start.elapsed().as_millis() as u64,
            "threshold": req.threshold,
        })));
    }

    // Embed the input text
    let embedding = embed_text_single(&state, &req.text)
        .await
        .map_err(|e| ApiError::internal(format!("Embedding failed: {}", e)))?;

    // Search Qdrant with optional book filter
    let limit = req.limit.min(50);
    let mut search_body = serde_json::json!({
        "vector": embedding,
        "limit": limit,
        "with_payload": true,
        "score_threshold": req.threshold,
    });

    if let Some(ref book_ids) = visible_book_ids {
        search_body["filter"] = serde_json::json!({
            "should": book_ids.iter().map(|id| {
                serde_json::json!({ "key": "book_id", "match": { "value": id.to_string() } })
            }).collect::<Vec<_>>()
        });
    }

    let resp = state
        .http_client
        .post(format!(
            "{}/collections/nova_chunks/points/search",
            state.config.qdrant_url
        ))
        .json(&search_body)
        .send()
        .await
        .map_err(|e| ApiError::internal(format!("Qdrant error: {}", e)))?;

    let results = if resp.status().is_success() {
        let data: serde_json::Value = resp.json().await.unwrap_or_default();
        let allowed_book_ids: Option<std::collections::HashSet<String>> = visible_book_ids
            .as_ref()
            .map(|ids| ids.iter().map(Uuid::to_string).collect());
        data["result"]
            .as_array()
            .unwrap_or(&Vec::new())
            .iter()
            .filter(|hit| {
                allowed_book_ids
                    .as_ref()
                    .is_none_or(|ids| hit["payload"]["book_id"].as_str().is_some_and(|id| ids.contains(id)))
            })
            .map(|hit| {
                serde_json::json!({
                    "book_id": hit["payload"]["book_id"].as_str().unwrap_or(""),
                    "book_title": hit["payload"]["book_title"].as_str().unwrap_or(""),
                    "chapter_title": hit["payload"]["chapter_title"],
                    "chapter_index": hit["payload"]["chapter_index"].as_i64().unwrap_or(0),
                    "chunk_index": hit["payload"]["chunk_index"].as_i64().unwrap_or(0),
                    "content": hit["payload"]["text"].as_str().or_else(|| hit["payload"]["content"].as_str()).unwrap_or(""),
                    "similarity": hit["score"].as_f64().unwrap_or(0.0),
                })
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    let elapsed_ms = start.elapsed().as_millis() as u64;

    Ok(Json(serde_json::json!({
        "results": results,
        "total": results.len(),
        "elapsed_ms": elapsed_ms,
        "threshold": req.threshold,
    })))
}

/// Save a vibe bookmark for quick future searches.
async fn save_vibe_bookmark(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<SaveVibeBookmarkRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let user_id = parse_user_id(&auth)?;
    ensure_optional_source_book_access(&state, &auth, req.source_book_id).await?;
    if req.source_text.trim().is_empty() {
        return Err(ApiError::bad_request("Source text cannot be empty"));
    }

    // Embed the text
    let embedding = embed_text_single(&state, &req.source_text).await.ok();

    let saved: (
        Uuid,
        Option<String>,
        String,
        Option<Uuid>,
        Option<i32>,
        chrono::DateTime<chrono::Utc>,
    ) = sqlx::query_as(
        "INSERT INTO vibe_bookmarks (user_id, name, source_text, source_book_id, source_chapter_index, embedding)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING id, name, source_text, source_book_id, source_chapter_index, created_at"
    )
    .bind(user_id)
    .bind(req.name)
    .bind(&req.source_text)
    .bind(req.source_book_id)
    .bind(req.source_chapter_index)
    .bind(embedding.as_deref())
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to save bookmark: {}", e)))?;

    let (id, name, source_text, source_book_id, source_chapter_index, created_at) = saved;

    Ok(Json(serde_json::json!({
        "id": id,
        "name": name,
        "source_text": source_text,
        "source_book_id": source_book_id,
        "source_chapter_index": source_chapter_index,
        "created_at": created_at,
    })))
}

/// List user's vibe bookmarks.
async fn list_vibe_bookmarks(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<Json<Vec<serde_json::Value>>> {
    let user_id = parse_user_id(&auth)?;
    let visible_library_ids = visible_library_ids(&state, &auth, LibraryAccess::Read).await?;
    let bookmarks: Vec<(
        Uuid,
        Option<String>,
        String,
        Option<Uuid>,
        Option<i32>,
        chrono::DateTime<chrono::Utc>,
    )> = if let Some(library_ids) = visible_library_ids {
        sqlx::query_as(
            r#"
            SELECT vb.id, vb.name, vb.source_text, vb.source_book_id,
                   vb.source_chapter_index, vb.created_at
            FROM vibe_bookmarks vb
            LEFT JOIN books b ON b.id = vb.source_book_id
            WHERE vb.user_id = $1
              AND (
                vb.source_book_id IS NULL
                OR b.library_id = ANY($2::uuid[])
              )
            ORDER BY vb.created_at DESC
            LIMIT 100
            "#,
        )
        .bind(user_id)
        .bind(&library_ids)
        .fetch_all(&state.db)
        .await
        .map_err(|e| ApiError::internal(format!("DB error: {}", e)))?
    } else {
        sqlx::query_as(
            r#"
            SELECT id, name, source_text, source_book_id, source_chapter_index, created_at
            FROM vibe_bookmarks
            WHERE user_id = $1
            ORDER BY created_at DESC
            LIMIT 100
            "#,
        )
        .bind(user_id)
        .fetch_all(&state.db)
        .await
        .map_err(|e| ApiError::internal(format!("DB error: {}", e)))?
    };

    let results: Vec<serde_json::Value> = bookmarks
        .into_iter()
        .map(|(id, name, text, book_id, ch_idx, created)| {
            serde_json::json!({
                "id": id,
                "name": name,
                "source_text": text,
                "source_book_id": book_id,
                "source_chapter_index": ch_idx,
                "created_at": created,
            })
        })
        .collect();

    Ok(Json(results))
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Embed a single text and return the vector as Vec<f32>.
async fn embed_text_single(state: &AppState, text: &str) -> Result<Vec<f32>, String> {
    let texts = vec![text];
    let mut results = embed_texts_batch(state, &texts).await?;
    results
        .pop()
        .ok_or_else(|| "Empty embedding result".to_string())
}

/// Embed multiple texts and return vectors.
async fn embed_texts_batch(state: &AppState, texts: &[&str]) -> Result<Vec<Vec<f32>>, String> {
    let mut request = state
        .http_client
        .post(format!("{}/v1/embeddings", state.config.embedding_endpoint))
        .header("X-Failover-Enabled", "true")
        .json(&serde_json::json!({
            "input": texts,
            "model": &state.config.embedding_model,
        }));

    if !state.config.embedding_api_key.is_empty() {
        request = request.header(
            "Authorization",
            format!("Bearer {}", state.config.embedding_api_key),
        );
    }

    let resp = request
        .send()
        .await
        .map_err(|e| format!("Embedding request failed: {}", e))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Embedding API error: {}", body));
    }

    let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let data = body["data"]
        .as_array()
        .ok_or("Invalid embedding response")?;

    let vectors: Vec<Vec<f32>> = data
        .iter()
        .map(|item| {
            item["embedding"]
                .as_array()
                .unwrap_or(&Vec::new())
                .iter()
                .map(|v| v.as_f64().unwrap_or(0.0) as f32)
                .collect()
        })
        .collect();

    Ok(vectors)
}

#[cfg(test)]
mod tests {
    fn production_source() -> &'static str {
        include_str!("semantic_tags.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("production source should exist")
    }

    #[test]
    fn semantic_tag_book_routes_require_book_acl() {
        let source = production_source();

        assert!(source.contains("ensure_semantic_book_read_access"));
        assert!(source.contains("ensure_semantic_book_write_access"));
        assert!(source.contains("ensure_book_access(state, auth, book_id, LibraryAccess::Read)"));
        assert!(source.contains("ensure_book_access(state, auth, book_id, LibraryAccess::Write)"));
    }

    #[test]
    fn vibe_search_acl_scopes_to_visible_books() {
        let source = production_source();

        assert!(source.contains("visible_vibe_book_ids"));
        assert!(source.contains("visible_library_ids(state, auth, LibraryAccess::Read)"));
        assert!(source.contains("id = ANY($1::uuid[])"));
    }

    #[test]
    fn vibe_bookmark_acl_source_book_requires_read_access() {
        let source = production_source();

        assert!(source.contains("ensure_optional_source_book_access"));
        assert!(source.contains("source_book_id"));
    }
}
