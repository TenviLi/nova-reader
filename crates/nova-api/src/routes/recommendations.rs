use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    access::{auth_user_id, ensure_book_access, visible_library_ids, LibraryAccess},
    error::ApiError,
    extractors::AuthUser,
    state::AppState,
};

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/recommendations", get(get_recommendations))
        .route("/recommendations/queue", get(get_reading_queue))
        .route(
            "/recommendations/feedback",
            axum::routing::post(submit_feedback),
        )
        .route(
            "/recommendations/feedback/{book_id}",
            axum::routing::delete(clear_feedback),
        )
        .route("/books/{id}/similar", get(find_similar_books))
        .route(
            "/books/{id}/compute-embedding",
            axum::routing::post(compute_book_embedding),
        )
        .route(
            "/books/{id}/similar-semantic",
            get(find_similar_books_semantic),
        )
}

#[derive(Deserialize)]
struct SimilarBooksQuery {
    #[serde(default = "default_limit")]
    limit: i64,
}

fn default_limit() -> i64 {
    10
}

#[derive(Serialize, sqlx::FromRow)]
struct SimpleBook {
    id: Uuid,
    title: String,
    author: Option<String>,
    #[sqlx(default)]
    cover_path: Option<String>,
    #[sqlx(default)]
    format: Option<String>,
    #[sqlx(default)]
    reading_status: Option<String>,
}

#[derive(sqlx::FromRow)]
struct SemanticBookRow {
    id: Uuid,
    title: String,
    author: Option<String>,
    cover_path: Option<String>,
    format: Option<String>,
    reading_status: Option<String>,
    language: Option<String>,
}

struct SemanticHit {
    book_id: Uuid,
    score: f64,
    chunk_count: Option<i64>,
}

fn scoped_library_ids_empty(library_ids: &Option<Vec<Uuid>>) -> bool {
    matches!(library_ids, Some(ids) if ids.is_empty())
}

fn book_library_filter(alias: &str, placeholder: usize, scoped: bool) -> String {
    if scoped {
        format!(" AND {alias}.library_id = ANY(${placeholder}::uuid[])")
    } else {
        String::new()
    }
}

fn similar_books_sql(scoped: bool) -> String {
    format!(
        r#"
        WITH candidates AS (
                 SELECT b.id, b.title, b.author, b.metadata->>'cover_path' as cover_path, b.format::text as format,
                   b.reading_status::text as reading_status,
                   (b.author IS NOT NULL AND b.author = $3) as same_author,
                   (b.language::text = $4) as same_language,
                   ($5::uuid IS NOT NULL AND b.series_id = $5) as same_series,
                   ARRAY(
                       SELECT DISTINCT signal
                       FROM (
                           SELECT value AS signal FROM jsonb_array_elements_text(COALESCE(b.tags, '[]'::jsonb))
                           UNION
                           SELECT value AS signal FROM jsonb_array_elements_text(COALESCE(b.metadata->'tags', '[]'::jsonb))
                           UNION
                           SELECT value AS signal FROM jsonb_array_elements_text(COALESCE(b.metadata->'genres', '[]'::jsonb))
                       ) s
                       WHERE signal = ANY($6::text[])
                   ) as shared_signals
            FROM books b
            WHERE b.id != $1
              AND b.status != 'archived'
              {library_filter}
        )
        SELECT id, title, author, cover_path, format, reading_status,
               same_author, same_language, same_series, shared_signals,
               cardinality(shared_signals)::int as shared_signal_count
        FROM candidates
        WHERE same_author OR same_language OR same_series OR cardinality(shared_signals) > 0
        ORDER BY
            cardinality(shared_signals) DESC,
            same_series DESC,
            same_author DESC,
            same_language DESC,
            id DESC
        LIMIT $2
        "#,
        library_filter = book_library_filter("b", 7, scoped)
    )
}

fn recommendations_in_progress_sql(scoped: bool) -> String {
    format!(
        r#"
        SELECT b.id, b.title, b.author, b.metadata->>'cover_path' as cover_path, b.format::text as format,
               b.reading_status::text as reading_status
        FROM books b
        JOIN reading_progress rp ON rp.book_id = b.id AND rp.user_id = $2
        WHERE b.status != 'archived'
          AND COALESCE(rp.progress, 0) > 0
          AND COALESCE(rp.progress, 0) < 1
          AND NOT (b.id = ANY($1::uuid[]))
          {library_filter}
        ORDER BY rp.last_read_at DESC NULLS LAST, b.updated_at DESC
        LIMIT 8
        "#,
        library_filter = book_library_filter("b", 3, scoped)
    )
}

fn recommendations_signal_match_sql(scoped: bool) -> String {
    format!(
        r#"
        WITH preferred_signals AS (
            SELECT signal, SUM(engagement)::double precision as weight
            FROM (
                SELECT b.id,
                       (1.0
                        + COALESCE(rp.progress, 0) * 3.0
                        + LEAST(COALESCE(rp.reading_time_secs, 0) / 3600.0, 6.0)
                        + CASE WHEN b.reading_status = 'completed' THEN 2.0 ELSE 0 END
                       ) AS engagement
                FROM books b
                LEFT JOIN reading_progress rp ON rp.book_id = b.id AND rp.user_id = $2
                WHERE b.status != 'archived'
                  AND b.reading_status IN ('reading', 'completed')
                  AND NOT (b.id = ANY($1::uuid[]))
                  {profile_library_filter}
            ) eng
            JOIN books b ON b.id = eng.id
              {signal_library_filter}
            CROSS JOIN LATERAL (
                SELECT value AS signal FROM jsonb_array_elements_text(COALESCE(b.tags, '[]'::jsonb))
                UNION
                SELECT value AS signal FROM jsonb_array_elements_text(COALESCE(b.metadata->'tags', '[]'::jsonb))
                UNION
                SELECT value AS signal FROM jsonb_array_elements_text(COALESCE(b.metadata->'genres', '[]'::jsonb))
            ) s
            WHERE signal <> ''
            GROUP BY signal
            ORDER BY weight DESC
            LIMIT 12
        ),
        candidate_signals AS (
            SELECT b.id, b.title, b.author, b.format::text as format,
                   b.reading_status::text as reading_status,
                   b.metadata->>'cover_path' as cover_path,
                   ARRAY_AGG(DISTINCT ps.signal) as shared_signals,
                   SUM(ps.weight)::double precision as signal_score,
                   MAX(b.updated_at) as updated_at
            FROM books b
            CROSS JOIN LATERAL (
                SELECT value AS signal FROM jsonb_array_elements_text(COALESCE(b.tags, '[]'::jsonb))
                UNION
                SELECT value AS signal FROM jsonb_array_elements_text(COALESCE(b.metadata->'tags', '[]'::jsonb))
                UNION
                SELECT value AS signal FROM jsonb_array_elements_text(COALESCE(b.metadata->'genres', '[]'::jsonb))
            ) s
            JOIN preferred_signals ps ON ps.signal = s.signal
            WHERE b.status != 'archived'
              AND b.reading_status IN ('unread', 'reading')
              AND NOT (b.id = ANY($1::uuid[]))
              {candidate_library_filter}
            GROUP BY b.id, b.title, b.author, b.format, b.reading_status, b.metadata
        )
        SELECT id, title, author, cover_path, format, reading_status, shared_signals, signal_score
        FROM candidate_signals
        ORDER BY signal_score DESC, updated_at DESC
        LIMIT 8
        "#,
        profile_library_filter = book_library_filter("b", 3, scoped),
        signal_library_filter = book_library_filter("b", 3, scoped),
        candidate_library_filter = book_library_filter("b", 3, scoped)
    )
}

fn recommendations_unread_sql(scoped: bool) -> String {
    format!(
        r#"
        SELECT b.id, b.title, b.author, b.metadata->>'cover_path' as cover_path, b.format::text as format,
               b.reading_status::text as reading_status
        FROM books b
        WHERE b.reading_status = 'unread' AND b.status != 'archived'
          AND NOT (b.id = ANY($1::uuid[]))
          {library_filter}
        ORDER BY b.created_at DESC
        LIMIT 8
        "#,
        library_filter = book_library_filter("b", 2, scoped)
    )
}

fn recommendations_recent_sql(scoped: bool) -> String {
    format!(
        r#"
        SELECT b.id, b.title, b.author, b.metadata->>'cover_path' as cover_path, b.format::text as format,
               b.reading_status::text as reading_status
        FROM books b
        WHERE b.status != 'archived'
          AND NOT (b.id = ANY($1::uuid[]))
          {library_filter}
        ORDER BY b.created_at DESC
        LIMIT 8
        "#,
        library_filter = book_library_filter("b", 2, scoped)
    )
}

fn semantic_anchors_sql(scoped: bool) -> String {
    format!(
        r#"
        SELECT b.id,
               b.title,
               COALESCE(rp.progress, 0)::double precision as progress,
               COALESCE(rp.reading_time_secs, 0)::bigint as reading_time_secs,
               (b.reading_status = 'completed') as completed,
               COALESCE(rf.feedback = 'like', false) as liked
        FROM books b
        LEFT JOIN reading_progress rp ON rp.book_id = b.id AND rp.user_id = $1
        LEFT JOIN recommendation_feedback rf
          ON rf.user_id = $1 AND rf.book_id = b.id AND rf.feedback = 'like'
        WHERE b.status != 'archived'
          AND b.reading_status IN ('reading', 'completed')
          AND NOT (b.id = ANY($2::uuid[]))
          {library_filter}
        ORDER BY (
            1.0
            + COALESCE(rp.progress, 0) * 3.0
            + LEAST(COALESCE(rp.reading_time_secs, 0) / 3600.0, 6.0)
            + CASE WHEN b.reading_status = 'completed' THEN 2.0 ELSE 0 END
            + CASE WHEN rf.feedback = 'like' THEN 1.5 ELSE 0 END
        ) DESC,
        rp.last_read_at DESC NULLS LAST,
        b.updated_at DESC
        LIMIT $3
        "#,
        library_filter = book_library_filter("b", 4, scoped)
    )
}

fn semantic_books_by_ids_sql(scoped: bool) -> String {
    format!(
        r#"
        SELECT b.id, b.title, b.author, b.metadata->>'cover_path' as cover_path,
               b.format::text as format, b.reading_status::text as reading_status,
               b.language::text as language
        FROM books b
        WHERE b.id = ANY($1::uuid[])
          AND b.status != 'archived'
          {library_filter}
        "#,
        library_filter = book_library_filter("b", 2, scoped)
    )
}

fn reading_queue_sql(scoped: bool) -> String {
    format!(
        r#"
        SELECT b.id, b.title, b.author, b.format::text as format,
               b.reading_status::text as reading_status
        FROM books b
        WHERE b.status != 'archived'
          AND b.reading_status IN ('reading', 'unread')
          {library_filter}
        ORDER BY
          CASE b.reading_status
            WHEN 'reading' THEN 0
            WHEN 'unread' THEN 1
          END,
          b.updated_at DESC
        LIMIT 15
        "#,
        library_filter = book_library_filter("b", 1, scoped)
    )
}

async fn fetch_visible_semantic_books(
    state: &AppState,
    book_ids: &[Uuid],
    visible_library_ids: Option<&[Uuid]>,
) -> Result<Vec<SemanticBookRow>, ApiError> {
    if book_ids.is_empty() {
        return Ok(Vec::new());
    }

    let sql = semantic_books_by_ids_sql(visible_library_ids.is_some());
    let mut query = sqlx::query_as::<_, SemanticBookRow>(&sql).bind(book_ids);
    if let Some(ids) = visible_library_ids {
        query = query.bind(ids);
    }

    query.fetch_all(&state.db).await.map_err(ApiError::from)
}

fn qdrant_semantic_hits(results: &[serde_json::Value]) -> Vec<SemanticHit> {
    results
        .iter()
        .filter_map(|hit| {
            let score = hit["score"].as_f64()?;
            let payload = &hit["payload"];
            let book_id = Uuid::parse_str(payload["book_id"].as_str()?).ok()?;
            Some(SemanticHit {
                book_id,
                score,
                chunk_count: payload["chunk_count"].as_i64(),
            })
        })
        .collect()
}

/// Find books similar to a given book (by author/language/tags).
async fn find_similar_books(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(book_id): Path<Uuid>,
    Query(params): Query<SimilarBooksQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    ensure_book_access(&state, &auth, book_id, LibraryAccess::Read).await?;
    let limit = params.limit.min(50);
    let visible_libraries = visible_library_ids(&state, &auth, LibraryAccess::Read).await?;
    if scoped_library_ids_empty(&visible_libraries) {
        return Ok(Json(serde_json::json!([])));
    }

    // Get source book info for similarity calculation
    #[derive(sqlx::FromRow)]
    struct SourceInfo {
        author: Option<String>,
        language: String,
        series_id: Option<Uuid>,
        signals: Vec<String>,
    }
    let source = sqlx::query_as::<_, SourceInfo>(
        r#"
        SELECT author, language::text as language, series_id,
               ARRAY(
                   SELECT DISTINCT signal
                   FROM (
                       SELECT value AS signal FROM jsonb_array_elements_text(COALESCE(tags, '[]'::jsonb))
                       UNION
                       SELECT value AS signal FROM jsonb_array_elements_text(COALESCE(metadata->'tags', '[]'::jsonb))
                       UNION
                       SELECT value AS signal FROM jsonb_array_elements_text(COALESCE(metadata->'genres', '[]'::jsonb))
                   ) s
                   WHERE signal <> ''
               ) as signals
        FROM books
        WHERE id = $1
        "#,
    )
    .bind(book_id)
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::from)?;

    let source = match source {
        Some(s) => s,
        None => return Ok(Json(serde_json::json!([]))),
    };

    #[derive(sqlx::FromRow)]
    struct SimilarRow {
        id: Uuid,
        title: String,
        author: Option<String>,
        cover_path: Option<String>,
        format: Option<String>,
        reading_status: Option<String>,
        same_author: bool,
        same_language: bool,
        same_series: bool,
        shared_signals: Vec<String>,
        shared_signal_count: i32,
    }

    let sql = similar_books_sql(visible_libraries.is_some());
    let mut query = sqlx::query_as::<_, SimilarRow>(&sql)
        .bind(book_id)
        .bind(limit)
        .bind(&source.author)
        .bind(&source.language)
        .bind(source.series_id)
        .bind(&source.signals);
    if let Some(ids) = visible_libraries.as_deref() {
        query = query.bind(ids);
    }
    let similar = query.fetch_all(&state.db).await.map_err(ApiError::from)?;

    let results: Vec<serde_json::Value> = similar
        .into_iter()
        .map(|b| {
            // Calculate a meaningful similarity score
            let mut score = 0.15;
            if b.same_author {
                score += 0.25;
            }
            if b.same_series {
                score += 0.25;
            }
            if b.same_language {
                score += 0.1;
            }
            score += (b.shared_signal_count as f64 * 0.08).min(0.35);
            let score = score.min(1.0);

            let mut reasons: Vec<String> = Vec::new();
            if b.same_series {
                reasons.push("同系列".into());
            }
            if b.same_author {
                reasons.push("同作者".into());
            }
            if !b.shared_signals.is_empty() {
                reasons.push(format!("共享标签：{}", b.shared_signals.join("、")));
            }
            if b.same_language {
                reasons.push("同语言".into());
            }
            let reason = if reasons.is_empty() {
                "相关书籍".to_string()
            } else {
                reasons.join(" / ")
            };

            serde_json::json!({
                "id": b.id,
                "title": b.title,
                "author": b.author,
                "cover_path": b.cover_path,
                "format": b.format,
                "reading_status": b.reading_status,
                "similarity_score": score,
                "reason": reason,
                "match_reasons": reasons,
                "shared_themes": b.shared_signals,
                "shared_entities": [],
            })
        })
        .collect();

    Ok(Json(serde_json::json!(results)))
}

#[derive(Serialize)]
struct RecommendationGroup {
    id: String,
    category: String,
    reason: String,
    books: Vec<serde_json::Value>,
}

const SEMANTIC_PROFILE_ANCHOR_LIMIT: i64 = 6;
const SEMANTIC_RECOMMENDATION_LIMIT: usize = 8;

#[derive(sqlx::FromRow)]
struct SemanticAnchorRow {
    id: Uuid,
    title: String,
    progress: f64,
    reading_time_secs: i64,
    completed: bool,
    liked: bool,
}

struct SemanticVectorAnchor {
    id: Uuid,
    title: String,
    point_id: u64,
}

fn semantic_anchor_weight(
    progress: f64,
    reading_time_secs: i64,
    completed: bool,
    liked: bool,
) -> f32 {
    let progress = progress.clamp(0.0, 1.0) as f32;
    let reading_hours = ((reading_time_secs.max(0) as f32) / 3600.0).min(6.0);
    1.0 + progress * 3.0
        + reading_hours
        + if completed { 2.0 } else { 0.0 }
        + if liked { 1.5 } else { 0.0 }
}

fn weighted_normalized_centroid(vectors: &[(Vec<f32>, f32)]) -> Option<Vec<f32>> {
    let dim = vectors
        .iter()
        .find(|(vector, weight)| !vector.is_empty() && *weight > 0.0)
        .map(|(vector, _)| vector.len())?;
    let mut centroid = vec![0.0f32; dim];
    let mut total_weight = 0.0f32;

    for (vector, weight) in vectors {
        if *weight <= 0.0 || vector.len() != dim {
            continue;
        }
        for (idx, value) in vector.iter().enumerate() {
            centroid[idx] += value * *weight;
        }
        total_weight += *weight;
    }

    if total_weight <= f32::EPSILON {
        return None;
    }

    for value in &mut centroid {
        *value /= total_weight;
    }

    let norm = centroid
        .iter()
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt();
    if norm <= f32::EPSILON {
        return None;
    }

    for value in &mut centroid {
        *value /= norm;
    }

    Some(centroid)
}

fn semantic_anchor_reason(anchor_titles: &[String]) -> String {
    match anchor_titles {
        [] => "根据你的书籍语义画像找到的相近作品".to_string(),
        [title] => format!("根据《{}》的语义画像找到的相近作品", title),
        titles => {
            let visible_titles = titles
                .iter()
                .take(2)
                .map(|title| format!("《{}》", title))
                .collect::<String>();
            format!(
                "根据{}等 {} 本书的语义画像找到的相近作品",
                visible_titles,
                titles.len()
            )
        }
    }
}

/// Get personalized recommendations based on reading history.
async fn get_recommendations(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<Vec<RecommendationGroup>>, ApiError> {
    let user_id = auth_user_id(&auth)?;
    let visible_libraries = visible_library_ids(&state, &auth, LibraryAccess::Read).await?;
    if scoped_library_ids_empty(&visible_libraries) {
        return Ok(Json(Vec::new()));
    }
    let scoped = visible_libraries.is_some();
    let visible_ids = visible_libraries.as_deref();
    let mut groups: Vec<RecommendationGroup> = Vec::new();

    // Books the user explicitly dismissed / marked not-interested — suppress everywhere.
    let dismissed: Vec<Uuid> = sqlx::query_scalar(
        "SELECT book_id FROM recommendation_feedback WHERE user_id = $1 AND feedback IN ('dismiss', 'not_interested')"
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;

    // Group 1: Continue reading (books with progress > 0, not completed)
    let sql = recommendations_in_progress_sql(scoped);
    let mut query = sqlx::query_as::<_, SimpleBook>(&sql)
        .bind(&dismissed)
        .bind(user_id);
    if let Some(ids) = visible_ids {
        query = query.bind(ids);
    }
    let in_progress = query.fetch_all(&state.db).await.map_err(ApiError::from)?;

    if !in_progress.is_empty() {
        groups.push(RecommendationGroup {
            id: "in-progress".into(),
            category: "继续阅读".into(),
            reason: "你还没读完这些书".into(),
            books: in_progress
                .into_iter()
                .map(|b| {
                    serde_json::json!({
                        "id": b.id, "title": b.title, "author": b.author,
                        "cover_path": b.cover_path,
                        "format": b.format, "reading_status": b.reading_status,
                        "score": 0.95,
                        "match_reason": "正在阅读",
                    })
                })
                .collect(),
        });
    }

    // Semantic group: books closest to the user's multi-book semantic profile.
    if let Some(group) = build_semantic_group(&state, user_id, &dismissed, visible_ids).await {
        groups.push(group);
    }

    #[derive(sqlx::FromRow)]
    struct SignalRecommendationRow {
        id: Uuid,
        title: String,
        author: Option<String>,
        cover_path: Option<String>,
        format: Option<String>,
        reading_status: Option<String>,
        shared_signals: Vec<String>,
        signal_score: f64,
    }

    // Preferred signals are now weighted by *reading behavior* (progress + time spent +
    // completion bonus) rather than a flat per-book count, so heavily-read books dominate
    // the taste profile. Negative-feedback books are excluded from the profile entirely.
    let sql = recommendations_signal_match_sql(scoped);
    let mut query = sqlx::query_as::<_, SignalRecommendationRow>(&sql)
        .bind(&dismissed)
        .bind(user_id);
    if let Some(ids) = visible_ids {
        query = query.bind(ids);
    }
    let signal_matches = query.fetch_all(&state.db).await.map_err(ApiError::from)?;

    if !signal_matches.is_empty() {
        let max_signal = signal_matches
            .iter()
            .map(|b| b.signal_score)
            .fold(0.0_f64, f64::max)
            .max(1.0);
        groups.push(RecommendationGroup {
            id: "ai-signal-match".into(),
            category: "偏好延展".into(),
            reason: "根据你正在读和已读作品的阅读时长、进度加权匹配".into(),
            books: signal_matches
                .into_iter()
                .map(|b| {
                    let score = (0.4 + 0.6 * (b.signal_score / max_signal)).min(1.0);
                    let reason = if b.shared_signals.is_empty() {
                        "命中近期阅读偏好".to_string()
                    } else {
                        format!("共享标签：{}", b.shared_signals.join("、"))
                    };

                    serde_json::json!({
                        "id": b.id,
                        "title": b.title,
                        "author": b.author,
                        "cover_path": b.cover_path,
                        "format": b.format,
                        "reading_status": b.reading_status,
                        "score": score,
                        "match_reason": reason,
                        "shared_themes": b.shared_signals,
                        "recommendation_score": b.signal_score,
                    })
                })
                .collect(),
        });
    }

    // Group 2: Unread books
    let sql = recommendations_unread_sql(scoped);
    let mut query = sqlx::query_as::<_, SimpleBook>(&sql).bind(&dismissed);
    if let Some(ids) = visible_ids {
        query = query.bind(ids);
    }
    let unread = query.fetch_all(&state.db).await.map_err(ApiError::from)?;

    if !unread.is_empty() {
        groups.push(RecommendationGroup {
            id: "unread".into(),
            category: "等待探索".into(),
            reason: "这些书还在等着你".into(),
            books: unread
                .into_iter()
                .map(|b| {
                    serde_json::json!({
                        "id": b.id, "title": b.title, "author": b.author,
                        "cover_path": b.cover_path,
                        "format": b.format, "reading_status": b.reading_status,
                        "score": 0.72,
                        "match_reason": "尚未开始",
                    })
                })
                .collect(),
        });
    }

    // Group 3: Recently added
    let sql = recommendations_recent_sql(scoped);
    let mut query = sqlx::query_as::<_, SimpleBook>(&sql).bind(&dismissed);
    if let Some(ids) = visible_ids {
        query = query.bind(ids);
    }
    let recent = query.fetch_all(&state.db).await.map_err(ApiError::from)?;

    if !recent.is_empty() {
        groups.push(RecommendationGroup {
            id: "recent".into(),
            category: "最新入库".into(),
            reason: "最近添加的书籍".into(),
            books: recent
                .into_iter()
                .map(|b| {
                    serde_json::json!({
                        "id": b.id, "title": b.title, "author": b.author,
                        "cover_path": b.cover_path,
                        "format": b.format, "reading_status": b.reading_status,
                        "score": 0.64,
                        "match_reason": "新入库",
                    })
                })
                .collect(),
        });
    }

    Ok(Json(groups))
}

#[derive(Deserialize)]
struct FeedbackRequest {
    book_id: Uuid,
    /// One of: dismiss | not_interested | like
    #[serde(default = "default_feedback")]
    feedback: String,
}

fn default_feedback() -> String {
    "dismiss".to_string()
}

/// POST /recommendations/feedback — record a like / dismiss / not-interested signal.
async fn submit_feedback(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<FeedbackRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user_id = auth_user_id(&auth)?;
    ensure_book_access(&state, &auth, body.book_id, LibraryAccess::Read).await?;
    let feedback = match body.feedback.as_str() {
        "dismiss" | "not_interested" | "like" => body.feedback.as_str(),
        _ => return Err(ApiError::bad_request("Invalid feedback type")),
    };

    sqlx::query(
        r#"
        INSERT INTO recommendation_feedback (user_id, book_id, feedback)
        VALUES ($1, $2, $3)
        ON CONFLICT (user_id, book_id)
        DO UPDATE SET feedback = EXCLUDED.feedback, created_at = NOW()
        "#,
    )
    .bind(user_id)
    .bind(body.book_id)
    .bind(feedback)
    .execute(&state.db)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(
        serde_json::json!({ "status": "ok", "feedback": feedback }),
    ))
}

/// DELETE /recommendations/feedback/:book_id — undo a previous feedback signal.
async fn clear_feedback(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(book_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user_id = auth_user_id(&auth)?;
    ensure_book_access(&state, &auth, book_id, LibraryAccess::Read).await?;
    sqlx::query("DELETE FROM recommendation_feedback WHERE user_id = $1 AND book_id = $2")
        .bind(user_id)
        .bind(book_id)
        .execute(&state.db)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(serde_json::json!({ "status": "ok" })))
}

/// Build a "猜你喜欢" group from a weighted multi-book semantic profile, then return
/// nearest neighbours from the `nova_books` Qdrant collection.
async fn build_semantic_group(
    state: &AppState,
    user_id: Uuid,
    dismissed: &[Uuid],
    visible_library_ids: Option<&[Uuid]>,
) -> Option<RecommendationGroup> {
    let sql = semantic_anchors_sql(visible_library_ids.is_some());
    let mut query = sqlx::query_as::<_, SemanticAnchorRow>(&sql)
        .bind(user_id)
        .bind(dismissed)
        .bind(SEMANTIC_PROFILE_ANCHOR_LIMIT);
    if let Some(ids) = visible_library_ids {
        query = query.bind(ids);
    }
    let anchors: Vec<SemanticAnchorRow> = query
        .fetch_all(&state.db)
        .await
        .ok()
        .filter(|anchors| !anchors.is_empty())?;

    let qdrant_url = &state.config.qdrant_url;
    let mut weighted_vectors: Vec<(Vec<f32>, f32)> = Vec::new();
    let mut vector_anchors: Vec<SemanticVectorAnchor> = Vec::new();

    for anchor in anchors {
        let point_id = anchor.id.as_u128() as u64 % (u64::MAX - 1);
        let vector_resp = match state
            .http_client
            .get(format!(
                "{}/collections/nova_books/points/{}?with_vector=true&with_payload=false",
                qdrant_url, point_id
            ))
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(_) => continue,
        };
        if !vector_resp.status().is_success() {
            continue;
        }

        let point_data: serde_json::Value = match vector_resp.json().await {
            Ok(data) => data,
            Err(_) => continue,
        };
        let Some(vector_values) = point_data["result"]["vector"].as_array() else {
            continue;
        };
        let vector: Vec<f32> = vector_values
            .iter()
            .filter_map(|value| value.as_f64().map(|float| float as f32))
            .collect();
        if vector.is_empty() {
            continue;
        }

        let weight = semantic_anchor_weight(
            anchor.progress,
            anchor.reading_time_secs,
            anchor.completed,
            anchor.liked,
        );
        weighted_vectors.push((vector, weight));
        vector_anchors.push(SemanticVectorAnchor {
            id: anchor.id,
            title: anchor.title,
            point_id,
        });
    }

    let profile_vector = weighted_normalized_centroid(&weighted_vectors)?;
    let anchor_titles: Vec<String> = vector_anchors
        .iter()
        .map(|anchor| anchor.title.clone())
        .collect();
    let anchor_id_set: HashSet<Uuid> = vector_anchors.iter().map(|anchor| anchor.id).collect();
    let dismissed_set: HashSet<Uuid> = dismissed.iter().copied().collect();
    let mut excluded_point_ids: HashSet<u64> = vector_anchors
        .iter()
        .map(|anchor| anchor.point_id)
        .collect();
    excluded_point_ids.extend(
        dismissed
            .iter()
            .map(|id| id.as_u128() as u64 % (u64::MAX - 1)),
    );
    let excluded_point_ids: Vec<u64> = excluded_point_ids.into_iter().collect();
    let reason = semantic_anchor_reason(&anchor_titles);

    let search_resp = state
        .http_client
        .post(format!(
            "{}/collections/nova_books/points/search",
            qdrant_url
        ))
        .json(&serde_json::json!({
            "vector": profile_vector,
            "limit": SEMANTIC_RECOMMENDATION_LIMIT * 4,
            "with_payload": true,
            "score_threshold": 0.5,
            "filter": { "must_not": [{ "has_id": excluded_point_ids }] },
        }))
        .send()
        .await
        .ok()?;
    if !search_resp.status().is_success() {
        return None;
    }
    let search_data: serde_json::Value = search_resp.json().await.ok()?;

    let hits = qdrant_semantic_hits(search_data["result"].as_array()?)
        .into_iter()
        .filter(|hit| {
            !dismissed_set.contains(&hit.book_id) && !anchor_id_set.contains(&hit.book_id)
        })
        .collect::<Vec<_>>();
    let book_ids = hits.iter().map(|hit| hit.book_id).collect::<Vec<_>>();
    let visible_books = fetch_visible_semantic_books(state, &book_ids, visible_library_ids)
        .await
        .ok()?;
    let mut books_by_id: HashMap<Uuid, SemanticBookRow> = visible_books
        .into_iter()
        .map(|book| (book.id, book))
        .collect();

    let books: Vec<serde_json::Value> = hits
        .into_iter()
        .filter_map(|hit| {
            let book = books_by_id.remove(&hit.book_id)?;
            Some(serde_json::json!({
                "id": book.id,
                "title": book.title,
                "author": book.author,
                "cover_path": book.cover_path,
                "format": book.format,
                "reading_status": book.reading_status,
                "score": hit.score,
                "match_reason": reason,
                "semantic_anchors": anchor_titles,
                "semantic_anchor_count": anchor_titles.len(),
                "similarity_score": hit.score,
                "recommendation_score": hit.score,
            }))
        })
        .take(SEMANTIC_RECOMMENDATION_LIMIT)
        .collect();

    if books.is_empty() {
        return None;
    }

    Some(RecommendationGroup {
        id: "semantic-similar".into(),
        category: "猜你喜欢".into(),
        reason,
        books,
    })
}

/// Reading queue — prioritized list of what to read next.
async fn get_reading_queue(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let visible_libraries = visible_library_ids(&state, &auth, LibraryAccess::Read).await?;
    if scoped_library_ids_empty(&visible_libraries) {
        return Ok(Json(serde_json::json!({
            "queue": [],
            "total": 0,
        })));
    }

    let sql = reading_queue_sql(visible_libraries.is_some());
    let mut query = sqlx::query_as::<_, SimpleBook>(&sql);
    if let Some(ids) = visible_libraries.as_deref() {
        query = query.bind(ids);
    }

    let queue = query.fetch_all(&state.db).await.map_err(ApiError::from)?;

    let items: Vec<serde_json::Value> = queue
        .into_iter()
        .map(|b| {
            let priority = if b.reading_status.as_deref() == Some("reading") {
                "high"
            } else {
                "medium"
            };
            let reason = if b.reading_status.as_deref() == Some("reading") {
                "正在阅读"
            } else {
                "待阅读"
            };
            serde_json::json!({
                "id": b.id,
                "title": b.title,
                "author": b.author,
                "reading_status": b.reading_status,
                "priority": priority,
                "reason": reason,
            })
        })
        .collect();

    let total = items.len();
    Ok(Json(serde_json::json!({
        "queue": items,
        "total": total,
    })))
}

/// POST /books/:id/compute-embedding — Compute book-level embedding as centroid of chunk vectors.
async fn compute_book_embedding(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(book_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    ensure_book_access(&state, &auth, book_id, LibraryAccess::Write).await?;

    // Fetch all chunk embeddings for this book
    let chunks: Vec<(Vec<f32>,)> = sqlx::query_as(
        "SELECT embedding FROM text_chunks WHERE book_id = $1 AND embedding IS NOT NULL",
    )
    .bind(book_id)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;

    if chunks.is_empty() {
        return Ok(Json(serde_json::json!({
            "status": "no_embeddings",
            "book_id": book_id,
            "message": "No chunk embeddings found. Run embedding generation first.",
        })));
    }

    // Compute centroid (mean of all chunk vectors)
    let dim = chunks[0].0.len();
    let count = chunks.len() as f32;
    let mut centroid = vec![0.0f32; dim];

    for (embedding,) in &chunks {
        if embedding.len() == dim {
            for (i, val) in embedding.iter().enumerate() {
                centroid[i] += val;
            }
        }
    }

    // Normalize to unit vector (for cosine similarity)
    for val in centroid.iter_mut() {
        *val /= count;
    }
    let norm: f32 = centroid.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm > 0.0 {
        for val in centroid.iter_mut() {
            *val /= norm;
        }
    }

    // Store in Qdrant collection "nova_books"
    let qdrant_url = &state.config.qdrant_url;
    let point_id = book_id.as_u128() as u64 % (u64::MAX - 1); // Deterministic ID from UUID

    // Ensure collection exists
    let _ = state
        .http_client
        .put(format!("{}/collections/nova_books", qdrant_url))
        .json(&serde_json::json!({
            "vectors": {
                "size": dim,
                "distance": "Cosine",
            }
        }))
        .send()
        .await;

    // Get book metadata for payload
    let book_meta: Option<(String, Option<String>, Option<String>)> =
        sqlx::query_as("SELECT title, author, language::text FROM books WHERE id = $1")
            .bind(book_id)
            .fetch_optional(&state.db)
            .await
            .map_err(ApiError::from)?;

    let (title, author, language) = book_meta.unwrap_or(("Unknown".into(), None, None));

    // Upsert point
    let resp = state
        .http_client
        .put(format!("{}/collections/nova_books/points", qdrant_url))
        .json(&serde_json::json!({
            "points": [{
                "id": point_id,
                "vector": centroid,
                "payload": {
                    "book_id": book_id.to_string(),
                    "title": title,
                    "author": author,
                    "language": language,
                    "chunk_count": chunks.len(),
                }
            }]
        }))
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("Qdrant upsert failed: {}", e)))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(ApiError::Internal(format!("Qdrant error: {}", body)));
    }

    Ok(Json(serde_json::json!({
        "status": "completed",
        "book_id": book_id,
        "dimensions": dim,
        "chunk_count": chunks.len(),
    })))
}

/// GET /books/:id/similar-semantic — Find semantically similar books using book-level embeddings.
async fn find_similar_books_semantic(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(book_id): Path<Uuid>,
    Query(params): Query<SimilarBooksQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    ensure_book_access(&state, &auth, book_id, LibraryAccess::Read).await?;
    let limit = params.limit.min(20) as usize;
    let visible_libraries = visible_library_ids(&state, &auth, LibraryAccess::Read).await?;
    if scoped_library_ids_empty(&visible_libraries) {
        return Ok(Json(serde_json::json!({
            "similar": [],
            "source_book_id": book_id,
        })));
    }
    let qdrant_url = &state.config.qdrant_url;
    let point_id = book_id.as_u128() as u64 % (u64::MAX - 1);

    // Fetch the book's vector first, then search nearest neighbors.
    let vector_resp = state
        .http_client
        .get(format!(
            "{}/collections/nova_books/points/{}",
            qdrant_url, point_id
        ))
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("Qdrant fetch failed: {}", e)))?;

    if !vector_resp.status().is_success() {
        return Ok(Json(serde_json::json!({
            "similar": [],
            "message": "Book embedding not found. Run compute-embedding first.",
        })));
    }

    let point_data: serde_json::Value = vector_resp
        .json()
        .await
        .map_err(|e| ApiError::Internal(format!("Qdrant parse failed: {}", e)))?;

    let vector = point_data["result"]["vector"].as_array();
    let vector = match vector {
        Some(v) => v
            .iter()
            .filter_map(|x| x.as_f64().map(|f| f as f32))
            .collect::<Vec<f32>>(),
        None => {
            return Ok(Json(serde_json::json!({
                "similar": [],
                "message": "Book vector not found in Qdrant.",
            })))
        }
    };

    // Now search with the vector
    let search_resp = state
        .http_client
        .post(format!(
            "{}/collections/nova_books/points/search",
            qdrant_url
        ))
        .json(&serde_json::json!({
            "vector": vector,
            "limit": if visible_libraries.is_some() { limit * 3 } else { limit },
            "with_payload": true,
            "filter": {
                "must_not": [{
                    "has_id": [point_id]
                }]
            },
        }))
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("Qdrant search failed: {}", e)))?;

    let search_data: serde_json::Value = search_resp
        .json()
        .await
        .map_err(|e| ApiError::Internal(format!("Qdrant parse failed: {}", e)))?;

    let hits = search_data["result"]
        .as_array()
        .map(|results| qdrant_semantic_hits(results))
        .unwrap_or_default();
    let book_ids = hits.iter().map(|hit| hit.book_id).collect::<Vec<_>>();
    let visible_books =
        fetch_visible_semantic_books(&state, &book_ids, visible_libraries.as_deref()).await?;
    let mut books_by_id: HashMap<Uuid, SemanticBookRow> = visible_books
        .into_iter()
        .map(|book| (book.id, book))
        .collect();

    let similar: Vec<serde_json::Value> = hits
        .into_iter()
        .filter_map(|hit| {
            let book = books_by_id.remove(&hit.book_id)?;
            Some(serde_json::json!({
                "book_id": book.id,
                "title": book.title,
                "author": book.author,
                "language": book.language,
                "format": book.format,
                "reading_status": book.reading_status,
                "cover_path": book.cover_path,
                "similarity_score": hit.score,
                "chunk_count": hit.chunk_count,
            }))
        })
        .take(limit)
        .collect();

    Ok(Json(serde_json::json!({
        "similar": similar,
        "source_book_id": book_id,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.0001,
            "expected {actual} to be close to {expected}"
        );
    }

    #[test]
    fn semantic_anchor_weight_rewards_engaged_and_liked_books() {
        let weight = semantic_anchor_weight(0.5, 7200, true, true);

        assert_close(weight, 8.0);
    }

    #[test]
    fn weighted_normalized_centroid_builds_stable_profile_vector() {
        let centroid =
            weighted_normalized_centroid(&[(vec![1.0, 0.0], 3.0), (vec![0.0, 1.0], 1.0)])
                .expect("centroid should exist");

        assert_eq!(centroid.len(), 2);
        assert_close(centroid[0], 0.9486833);
        assert_close(centroid[1], 0.31622777);
    }

    #[test]
    fn weighted_normalized_centroid_ignores_bad_vectors() {
        let centroid = weighted_normalized_centroid(&[
            (vec![1.0, 0.0], 1.0),
            (vec![9.0, 9.0, 9.0], 10.0),
            (vec![0.0, 1.0], 0.0),
        ])
        .expect("centroid should use the one valid weighted vector");

        assert_eq!(centroid, vec![1.0, 0.0]);
    }

    #[test]
    fn semantic_anchor_reason_mentions_cross_book_anchors() {
        let reason = semantic_anchor_reason(&[
            "星海边境".to_string(),
            "冬眠者".to_string(),
            "群山回声".to_string(),
        ]);

        assert_eq!(
            reason,
            "根据《星海边境》《冬眠者》等 3 本书的语义画像找到的相近作品"
        );
    }

    #[test]
    fn recommendations_acl_sql_filters_visible_libraries() {
        assert!(similar_books_sql(true).contains("b.library_id = ANY($7::uuid[])"));
        assert!(recommendations_in_progress_sql(true).contains("b.library_id = ANY($3::uuid[])"));
        assert!(recommendations_unread_sql(true).contains("b.library_id = ANY($2::uuid[])"));
        assert!(recommendations_recent_sql(true).contains("b.library_id = ANY($2::uuid[])"));
        assert!(reading_queue_sql(true).contains("b.library_id = ANY($1::uuid[])"));
    }

    #[test]
    fn semantic_recommendation_acl_sql_filters_visible_libraries() {
        let signal_sql = recommendations_signal_match_sql(true);
        assert!(
            signal_sql.matches("b.library_id = ANY($3::uuid[])").count() >= 3,
            "signal recommendation sql should filter profile, signal, and candidate books"
        );

        assert!(semantic_anchors_sql(true).contains("b.library_id = ANY($4::uuid[])"));
        assert!(semantic_books_by_ids_sql(true).contains("b.library_id = ANY($2::uuid[])"));
    }

    #[test]
    fn recommendation_profile_sql_scopes_reader_artifacts_to_current_user() {
        assert!(recommendations_in_progress_sql(false).contains("rp.user_id = $2"));
        assert!(recommendations_signal_match_sql(false).contains("rp.user_id = $2"));
        assert!(semantic_anchors_sql(false).contains("rp.user_id = $1"));
    }
}
