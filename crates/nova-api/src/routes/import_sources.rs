use std::sync::Arc;

use axum::{
    extract::{Path, State},
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::error::ApiResult;
use crate::state::AppState;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/import-sources", get(list_sources).post(create_source))
        .route("/import-sources/{id}", get(get_source).put(update_source).delete(delete_source))
        .route("/import-sources/{id}/check", post(trigger_check))
        .route("/import-sources/{id}/logs", get(get_logs))
        .route("/import-sources/{id}/pause", post(pause_source))
        .route("/import-sources/{id}/resume", post(resume_source))
}

#[derive(Debug, Deserialize)]
struct CreateSourceRequest {
    name: String,
    source_type: String, // "web_scraper", "rss", "atom", "api"
    url: String,
    book_id: Option<Uuid>,
    config: Option<serde_json::Value>,
    check_interval_minutes: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct UpdateSourceRequest {
    name: Option<String>,
    url: Option<String>,
    config: Option<serde_json::Value>,
    check_interval_minutes: Option<i32>,
}

/// List all import sources for the current user.
async fn list_sources(
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<serde_json::Value>> {
    let sources = sqlx::query_as!(
        ImportSourceRow,
        r#"SELECT id, name, source_type, url, status, book_id,
           last_check_at, total_chapters_imported, error_message, created_at
           FROM import_sources ORDER BY created_at DESC"#
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(serde_json::json!({ "sources": sources, "total": sources.len() })))
}

/// Create a new import source.
async fn create_source(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateSourceRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    // Validate source type
    let valid_types = ["web_scraper", "rss", "atom", "api"];
    if !valid_types.contains(&body.source_type.as_str()) {
        return Err(crate::error::ApiError::bad_request(
            &format!("Invalid source_type. Must be one of: {:?}", valid_types)
        ));
    }

    // Validate URL
    if !body.url.starts_with("http://") && !body.url.starts_with("https://") {
        return Err(crate::error::ApiError::bad_request("URL must start with http:// or https://"));
    }

    let id = Uuid::new_v4();
    let config = body.config.unwrap_or(serde_json::json!({}));
    let interval = body.check_interval_minutes.unwrap_or(60).max(5); // minimum 5 min

    sqlx::query!(
        r#"INSERT INTO import_sources (id, user_id, name, source_type, url, book_id, config, check_interval_minutes)
           VALUES ($1, $2, $3, $4::import_source_type, $5, $6, $7, $8)"#,
        id,
        Uuid::nil(), // TODO: extract from auth context
        body.name,
        body.source_type,
        body.url,
        body.book_id,
        config,
        interval
    )
    .execute(&state.db)
    .await?;

    Ok(Json(serde_json::json!({
        "id": id,
        "name": body.name,
        "source_type": body.source_type,
        "url": body.url,
        "status": "active",
        "check_interval_minutes": interval,
    })))
}

/// Get a single import source with details.
async fn get_source(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let source = sqlx::query!(
        r#"SELECT id, name, source_type, url, status, config, book_id,
           last_check_at, last_chapter_at, total_chapters_imported, error_message, 
           check_interval_minutes, created_at, updated_at
           FROM import_sources WHERE id = $1"#,
        id
    )
    .fetch_optional(&state.db)
    .await?;

    match source {
        Some(s) => Ok(Json(serde_json::json!(s))),
        None => Err(crate::error::ApiError::not_found("Import source not found")),
    }
}

/// Update an import source.
async fn update_source(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateSourceRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    if let Some(ref url) = body.url {
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(crate::error::ApiError::bad_request("URL must start with http:// or https://"));
        }
    }

    sqlx::query!(
        r#"UPDATE import_sources SET
           name = COALESCE($2, name),
           url = COALESCE($3, url),
           config = COALESCE($4, config),
           check_interval_minutes = COALESCE($5, check_interval_minutes),
           updated_at = now()
           WHERE id = $1"#,
        id,
        body.name,
        body.url,
        body.config,
        body.check_interval_minutes
    )
    .execute(&state.db)
    .await?;

    Ok(Json(serde_json::json!({ "updated": true, "id": id })))
}

/// Delete an import source.
async fn delete_source(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    sqlx::query!("DELETE FROM import_sources WHERE id = $1", id)
        .execute(&state.db)
        .await?;
    Ok(Json(serde_json::json!({ "deleted": true, "id": id })))
}

/// Trigger an immediate check for new chapters.
async fn trigger_check(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    // In production, this would enqueue a background job
    sqlx::query!(
        "UPDATE import_sources SET last_check_at = now() WHERE id = $1",
        id
    )
    .execute(&state.db)
    .await?;

    Ok(Json(serde_json::json!({
        "id": id,
        "status": "check_queued",
        "message": "Manual check triggered"
    })))
}

/// Get import logs for a source.
async fn get_logs(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let logs = sqlx::query!(
        r#"SELECT id, action, chapter_title, chapter_index, details, created_at
           FROM import_logs WHERE source_id = $1
           ORDER BY created_at DESC LIMIT 50"#,
        id
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(serde_json::json!({ "logs": logs, "total": logs.len() })))
}

/// Pause an import source.
async fn pause_source(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    sqlx::query!(
        "UPDATE import_sources SET status = 'paused', updated_at = now() WHERE id = $1",
        id
    )
    .execute(&state.db)
    .await?;
    Ok(Json(serde_json::json!({ "id": id, "status": "paused" })))
}

/// Resume a paused import source.
async fn resume_source(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    sqlx::query!(
        "UPDATE import_sources SET status = 'active', error_message = NULL, updated_at = now() WHERE id = $1",
        id
    )
    .execute(&state.db)
    .await?;
    Ok(Json(serde_json::json!({ "id": id, "status": "active" })))
}

// Row type for query_as
#[derive(Debug, serde::Serialize, sqlx::FromRow)]
struct ImportSourceRow {
    id: Uuid,
    name: String,
    source_type: String,
    url: String,
    status: String,
    book_id: Option<Uuid>,
    last_check_at: Option<chrono::DateTime<chrono::Utc>>,
    total_chapters_imported: i32,
    error_message: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
}
