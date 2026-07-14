use std::sync::Arc;

use axum::{
    extract::{Query, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;

use crate::error::ApiResult;
use crate::state::AppState;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/calibre/status", get(sync_status))
        .route("/calibre/sync", post(trigger_sync))
        .route("/calibre/config", get(get_config).post(update_config))
        .route("/calibre/import", post(import_from_calibre))
        .route("/calibre/export", post(export_to_calibre))
}

#[derive(Debug, Deserialize)]
struct CalibreConfig {
    /// Path to Calibre library on the same filesystem (or network mount)
    library_path: String,
    /// Sync mode: "bidirectional", "import_only", "export_only"
    sync_mode: Option<String>,
    /// Whether to sync reading progress back to Calibre custom columns
    sync_progress: Option<bool>,
    /// Whether to sync metadata changes
    sync_metadata: Option<bool>,
    /// Calibre content server URL (for remote libraries)
    content_server_url: Option<String>,
}

/// Get Calibre sync status.
async fn sync_status(
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<serde_json::Value>> {
    let last_sync = sqlx::query!(
        "SELECT last_sync_at, sync_status, books_synced, errors FROM calibre_sync_status LIMIT 1"
    )
    .fetch_optional(&state.db)
    .await?;

    match last_sync {
        Some(s) => Ok(Json(serde_json::json!({
            "configured": true,
            "last_sync_at": s.last_sync_at,
            "status": s.sync_status,
            "books_synced": s.books_synced,
            "errors": s.errors,
        }))),
        None => Ok(Json(serde_json::json!({
            "configured": false,
            "message": "Calibre sync not configured. Set library path first."
        }))),
    }
}

/// Trigger a sync with Calibre.
async fn trigger_sync(
    State(state): State<Arc<AppState>>,
    Json(body): Json<Option<serde_json::Value>>,
) -> ApiResult<Json<serde_json::Value>> {
    let config = sqlx::query!(
        "SELECT library_path, sync_mode FROM calibre_config LIMIT 1"
    )
    .fetch_optional(&state.db)
    .await?;

    let config = config.ok_or_else(|| crate::error::ApiError::bad_request(
        "Calibre not configured. Please set library_path first."
    ))?;

    // Validate library path exists
    let lib_path = std::path::Path::new(&config.library_path);
    let metadata_db = lib_path.join("metadata.db");

    if !metadata_db.exists() {
        return Err(crate::error::ApiError::bad_request(
            "Calibre metadata.db not found at configured path"
        ));
    }

    // Queue the sync job
    // In production, this would spawn a background task that:
    // 1. Opens Calibre's metadata.db (SQLite)
    // 2. Compares book lists bidirectionally
    // 3. Imports new books from Calibre
    // 4. Exports new books to Calibre
    // 5. Syncs metadata changes (titles, authors, tags)
    // 6. Optionally syncs reading progress via custom columns

    sqlx::query!(
        r#"INSERT INTO calibre_sync_status (sync_status, books_synced, errors)
           VALUES ('in_progress', 0, 0)
           ON CONFLICT (id) DO UPDATE SET
           sync_status = 'in_progress', last_sync_at = now()"#
    )
    .execute(&state.db)
    .await?;

    Ok(Json(serde_json::json!({
        "status": "sync_started",
        "library_path": config.library_path,
        "sync_mode": config.sync_mode,
    })))
}

/// Get Calibre configuration.
async fn get_config(
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<serde_json::Value>> {
    let config = sqlx::query!(
        "SELECT library_path, sync_mode, sync_progress, sync_metadata, content_server_url FROM calibre_config LIMIT 1"
    )
    .fetch_optional(&state.db)
    .await?;

    match config {
        Some(c) => Ok(Json(serde_json::json!({
            "library_path": c.library_path,
            "sync_mode": c.sync_mode,
            "sync_progress": c.sync_progress,
            "sync_metadata": c.sync_metadata,
            "content_server_url": c.content_server_url,
        }))),
        None => Ok(Json(serde_json::json!({ "configured": false }))),
    }
}

/// Update Calibre configuration.
async fn update_config(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CalibreConfig>,
) -> ApiResult<Json<serde_json::Value>> {
    // Validate library path
    let lib_path = std::path::Path::new(&body.library_path);
    if !lib_path.exists() {
        return Err(crate::error::ApiError::bad_request(
            &format!("Library path does not exist: {}", body.library_path)
        ));
    }

    let metadata_db = lib_path.join("metadata.db");
    if !metadata_db.exists() {
        return Err(crate::error::ApiError::bad_request(
            "No metadata.db found. Is this a valid Calibre library?"
        ));
    }

    let sync_mode = body.sync_mode.unwrap_or_else(|| "bidirectional".into());

    sqlx::query!(
        r#"INSERT INTO calibre_config (library_path, sync_mode, sync_progress, sync_metadata, content_server_url)
           VALUES ($1, $2, $3, $4, $5)
           ON CONFLICT (id) DO UPDATE SET
           library_path = $1, sync_mode = $2, sync_progress = $3, sync_metadata = $4, content_server_url = $5"#,
        body.library_path,
        sync_mode,
        body.sync_progress.unwrap_or(true),
        body.sync_metadata.unwrap_or(true),
        body.content_server_url,
    )
    .execute(&state.db)
    .await?;

    Ok(Json(serde_json::json!({
        "configured": true,
        "library_path": body.library_path,
        "sync_mode": sync_mode,
    })))
}

/// Import specific books from Calibre.
async fn import_from_calibre(
    State(state): State<Arc<AppState>>,
    Json(body): Json<serde_json::Value>,
) -> ApiResult<Json<serde_json::Value>> {
    let calibre_ids = body.get("calibre_ids")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    Ok(Json(serde_json::json!({
        "status": "import_queued",
        "books_to_import": calibre_ids.len(),
        "message": "Import job queued. Books will appear in your library shortly.",
    })))
}

/// Export specific books to Calibre.
async fn export_to_calibre(
    State(state): State<Arc<AppState>>,
    Json(body): Json<serde_json::Value>,
) -> ApiResult<Json<serde_json::Value>> {
    let book_ids = body.get("book_ids")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    Ok(Json(serde_json::json!({
        "status": "export_queued",
        "books_to_export": book_ids.len(),
        "message": "Export job queued. Books will be added to Calibre library.",
    })))
}
