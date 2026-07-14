use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    access::{ensure_book_access, LibraryAccess},
    error::{ApiError, ApiResult},
    extractors::AuthUser,
    state::AppState,
};

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/analysis/{book_id}/summaries", get(get_chapter_summaries))
        .route("/analysis/{book_id}/sentiment", get(get_sentiment_arc))
        .route("/analysis/{book_id}/foreshadowing", get(get_foreshadowing))
        .route("/analysis/{book_id}/macro", get(get_macro_analysis))
        .route("/analysis/{book_id}/state-changes", get(get_state_changes))
        .route("/analysis/{book_id}/overview", get(get_analysis_overview))
}

#[derive(Deserialize)]
struct PaginationQuery {
    limit: Option<i64>,
    offset: Option<i64>,
    status: Option<String>,
    character: Option<String>,
}

async fn ensure_analysis_book_read_access(
    state: &AppState,
    auth: &AuthUser,
    book_id: Uuid,
) -> ApiResult<()> {
    ensure_book_access(state, auth, book_id, LibraryAccess::Read).await
}

// ─── Chapter Summaries ───────────────────────────────────────────────────────

#[derive(sqlx::FromRow, Serialize)]
struct ChapterSummaryRow {
    id: Uuid,
    chapter_index: i32,
    summary: String,
    time_marker: Option<String>,
    location: Option<String>,
    key_event: Option<String>,
    sentiment: Option<String>,
    sentiment_score: Option<f32>,
    characters_present: Vec<String>,
    potential_mysteries: Vec<String>,
}

async fn get_chapter_summaries(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(book_id): Path<Uuid>,
    Query(q): Query<PaginationQuery>,
) -> Result<Json<Vec<ChapterSummaryRow>>, ApiError> {
    ensure_analysis_book_read_access(&state, &auth, book_id).await?;
    let limit = q.limit.unwrap_or(100).min(500);
    let offset = q.offset.unwrap_or(0);

    let rows = sqlx::query_as::<_, ChapterSummaryRow>(
        "SELECT id, chapter_index, summary, time_marker, location, key_event, sentiment, sentiment_score, characters_present, potential_mysteries
         FROM chapter_summaries WHERE book_id = $1 ORDER BY chapter_index LIMIT $2 OFFSET $3"
    )
    .bind(book_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(rows))
}

// ─── Sentiment Arc ───────────────────────────────────────────────────────────

#[derive(sqlx::FromRow, Serialize)]
struct SentimentArcRow {
    chapter_index: i32,
    joy: f32,
    sadness: f32,
    anger: f32,
    fear: f32,
    surprise: f32,
    tension: f32,
    romance: f32,
    overall_score: f32,
    dominant_emotion: Option<String>,
    is_peak: bool,
    is_valley: bool,
}

async fn get_sentiment_arc(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(book_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    ensure_analysis_book_read_access(&state, &auth, book_id).await?;
    let rows = sqlx::query_as::<_, SentimentArcRow>(
        "SELECT chapter_index, joy, sadness, anger, fear, surprise, tension, romance, overall_score, dominant_emotion, is_peak, is_valley
         FROM sentiment_arcs WHERE book_id = $1 ORDER BY chapter_index"
    )
    .bind(book_id)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;

    // Also compute stats
    let avg_score: f64 =
        rows.iter().map(|r| r.overall_score as f64).sum::<f64>() / rows.len().max(1) as f64;
    let peaks: Vec<i32> = rows
        .iter()
        .filter(|r| r.is_peak)
        .map(|r| r.chapter_index)
        .collect();
    let valleys: Vec<i32> = rows
        .iter()
        .filter(|r| r.is_valley)
        .map(|r| r.chapter_index)
        .collect();

    Ok(Json(serde_json::json!({
        "data": rows.iter().map(|r| serde_json::json!({
            "chapter": r.chapter_index,
            "joy": r.joy, "sadness": r.sadness, "anger": r.anger,
            "fear": r.fear, "surprise": r.surprise, "tension": r.tension, "romance": r.romance,
            "overall": r.overall_score, "dominant": r.dominant_emotion,
            "is_peak": r.is_peak, "is_valley": r.is_valley,
        })).collect::<Vec<_>>(),
        "stats": {
            "average_score": avg_score,
            "peaks": peaks,
            "valleys": valleys,
            "total_chapters": rows.len(),
        }
    })))
}

// ─── Foreshadowing ───────────────────────────────────────────────────────────

#[derive(sqlx::FromRow, Serialize)]
struct ForeshadowingRow {
    id: Uuid,
    setup_chapter: i32,
    setup_description: String,
    payoff_chapter: Option<i32>,
    payoff_description: Option<String>,
    confidence: f32,
    status: String,
    category: String,
    created_at: chrono::DateTime<chrono::Utc>,
    resolved_at: Option<chrono::DateTime<chrono::Utc>>,
}

async fn get_foreshadowing(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(book_id): Path<Uuid>,
    Query(q): Query<PaginationQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    ensure_analysis_book_read_access(&state, &auth, book_id).await?;
    let status_filter = q.status.as_deref().unwrap_or("all");

    let rows = if status_filter == "all" {
        sqlx::query_as::<_, ForeshadowingRow>(
            "SELECT id, setup_chapter, setup_description, payoff_chapter, payoff_description, confidence, status, category, created_at, resolved_at
             FROM foreshadowing_entries WHERE book_id = $1 ORDER BY setup_chapter"
        )
        .bind(book_id)
        .fetch_all(&state.db)
        .await
        .map_err(ApiError::from)?
    } else {
        sqlx::query_as::<_, ForeshadowingRow>(
            "SELECT id, setup_chapter, setup_description, payoff_chapter, payoff_description, confidence, status, category, created_at, resolved_at
             FROM foreshadowing_entries WHERE book_id = $1 AND status = $2 ORDER BY setup_chapter"
        )
        .bind(book_id)
        .bind(status_filter)
        .fetch_all(&state.db)
        .await
        .map_err(ApiError::from)?
    };

    let unresolved = rows.iter().filter(|r| r.status == "unresolved").count();
    let resolved = rows.iter().filter(|r| r.status == "resolved").count();

    Ok(Json(serde_json::json!({
        "entries": rows,
        "stats": {
            "total": rows.len(),
            "unresolved": unresolved,
            "resolved": resolved,
        }
    })))
}

// ─── Macro Analysis ──────────────────────────────────────────────────────────

#[derive(sqlx::FromRow, Serialize)]
struct MacroAnalysisRow {
    id: Uuid,
    start_chapter: i32,
    end_chapter: i32,
    plot_arc: Option<String>,
    key_conflicts: Vec<String>,
    resolved_mysteries: Vec<String>,
    new_mysteries: Vec<String>,
    arc_summary: Option<String>,
}

async fn get_macro_analysis(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(book_id): Path<Uuid>,
) -> Result<Json<Vec<MacroAnalysisRow>>, ApiError> {
    ensure_analysis_book_read_access(&state, &auth, book_id).await?;
    let rows = sqlx::query_as::<_, MacroAnalysisRow>(
        "SELECT id, start_chapter, end_chapter, plot_arc, key_conflicts, resolved_mysteries, new_mysteries, arc_summary
         FROM macro_analysis WHERE book_id = $1 ORDER BY start_chapter"
    )
    .bind(book_id)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(rows))
}

// ─── Character State Changes ─────────────────────────────────────────────────

#[derive(sqlx::FromRow, Serialize)]
struct StateChangeRow {
    id: Uuid,
    character_name: String,
    chapter_index: i32,
    state_type: String,
    from_state: Option<String>,
    to_state: String,
    trigger_event: Option<String>,
    significance: f32,
}

async fn get_state_changes(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(book_id): Path<Uuid>,
    Query(q): Query<PaginationQuery>,
) -> Result<Json<Vec<StateChangeRow>>, ApiError> {
    ensure_analysis_book_read_access(&state, &auth, book_id).await?;
    let rows = if let Some(ref character) = q.character {
        sqlx::query_as::<_, StateChangeRow>(
            "SELECT id, character_name, chapter_index, state_type, from_state, to_state, trigger_event, significance
             FROM character_state_changes WHERE book_id = $1 AND character_name = $2 ORDER BY chapter_index"
        )
        .bind(book_id)
        .bind(character)
        .fetch_all(&state.db)
        .await
        .map_err(ApiError::from)?
    } else {
        sqlx::query_as::<_, StateChangeRow>(
            "SELECT id, character_name, chapter_index, state_type, from_state, to_state, trigger_event, significance
             FROM character_state_changes WHERE book_id = $1 ORDER BY chapter_index"
        )
        .bind(book_id)
        .fetch_all(&state.db)
        .await
        .map_err(ApiError::from)?
    };

    Ok(Json(rows))
}

// ─── Overview (combined stats) ───────────────────────────────────────────────

async fn get_analysis_overview(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(book_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    ensure_analysis_book_read_access(&state, &auth, book_id).await?;
    let summary_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM chapter_summaries WHERE book_id = $1")
            .bind(book_id)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?;
    let sentiment_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM sentiment_arcs WHERE book_id = $1")
            .bind(book_id)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?;
    let foreshadowing_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM foreshadowing_entries WHERE book_id = $1")
            .bind(book_id)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?;
    let unresolved_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM foreshadowing_entries WHERE book_id = $1 AND status = 'unresolved'",
    )
    .bind(book_id)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::from)?;
    let macro_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM macro_analysis WHERE book_id = $1")
            .bind(book_id)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?;

    Ok(Json(serde_json::json!({
        "chapter_summaries": summary_count.0,
        "sentiment_arcs": sentiment_count.0,
        "foreshadowing_total": foreshadowing_count.0,
        "foreshadowing_unresolved": unresolved_count.0,
        "macro_windows": macro_count.0,
        "has_deep_analysis": summary_count.0 > 0,
    })))
}

#[cfg(test)]
mod tests {
    fn production_source() -> &'static str {
        include_str!("analysis.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("production source should exist")
    }

    #[test]
    fn analysis_book_routes_require_read_acl() {
        let source = production_source();

        assert!(source.contains("use crate::{"));
        assert!(source.contains("extractors::AuthUser"));
        assert!(source.contains("ensure_analysis_book_read_access"));
        assert!(source.contains("ensure_book_access(state, auth, book_id, LibraryAccess::Read)"));
        assert_eq!(
            source
                .matches("ensure_analysis_book_read_access(&state, &auth, book_id).await?")
                .count(),
            6
        );
    }
}
