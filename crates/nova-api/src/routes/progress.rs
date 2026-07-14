use std::sync::Arc;

use axum::{
    extract::{Path, State},
    routing::{get, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    access::{auth_user_id, ensure_book_access, LibraryAccess},
    error::ApiError,
    extractors::AuthUser,
    state::AppState,
};

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/books/{book_id}/progress",
            get(get_progress).put(update_progress),
        )
        .route(
            "/books/{book_id}/sessions",
            get(list_sessions).post(create_session),
        )
        .route(
            "/books/{book_id}/sessions/current",
            put(update_current_session),
        )
}

fn completed_reading_status_value() -> &'static str {
    "completed"
}

fn reader_artifact_access() -> LibraryAccess {
    LibraryAccess::Read
}

fn progress_select_sql() -> &'static str {
    r#"
        SELECT id, book_id, progress, current_chapter, chapter_index,
               reading_time_secs, last_read_at
        FROM reading_progress
        WHERE book_id = $1 AND user_id = $2
        "#
}

fn progress_timestamp_sql() -> &'static str {
    "SELECT last_read_at FROM reading_progress WHERE book_id = $1 AND user_id = $2"
}

fn progress_upsert_sql() -> &'static str {
    r#"
        INSERT INTO reading_progress (id, user_id, book_id, progress, current_chapter, chapter_index, reading_time_secs, last_read_at)
        VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, NOW())
        ON CONFLICT (user_id, book_id) DO UPDATE SET
            progress = $3,
            current_chapter = $4,
            chapter_index = $5,
            reading_time_secs = reading_progress.reading_time_secs + $6,
            last_read_at = NOW(),
            updated_at = NOW()
        "#
}

fn progress_conflict_select_sql() -> &'static str {
    "SELECT id, book_id, progress, current_chapter, chapter_index, reading_time_secs, last_read_at FROM reading_progress WHERE book_id = $1 AND user_id = $2"
}

fn sessions_select_sql() -> &'static str {
    r#"
        SELECT id, book_id, started_at, ended_at, duration_secs, pages_read, words_read
        FROM reading_sessions
        WHERE book_id = $1 AND user_id = $2
        ORDER BY started_at DESC
        LIMIT 50
        "#
}

fn session_insert_sql() -> &'static str {
    r#"
        INSERT INTO reading_sessions (id, user_id, book_id, started_at, duration_secs, pages_read, words_read, start_chapter)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#
}

fn current_session_update_sql() -> &'static str {
    r#"
        UPDATE reading_sessions
        SET duration_secs = COALESCE($3, duration_secs),
            pages_read = COALESCE($4, pages_read),
            words_read = COALESCE($5, words_read),
            end_chapter = COALESCE($6, end_chapter),
            ended_at = NOW(),
            end_time = NOW(),
            updated_at = NOW()
        WHERE id = (
            SELECT id FROM reading_sessions
            WHERE book_id = $1 AND user_id = $2 AND ended_at IS NULL
            ORDER BY started_at DESC LIMIT 1
        )
        "#
}

#[derive(sqlx::FromRow, Serialize)]
struct ProgressRow {
    id: Uuid,
    book_id: Uuid,
    progress: f64,
    current_chapter: i32,
    chapter_index: Option<i32>,
    reading_time_secs: i64,
    last_read_at: chrono::DateTime<chrono::Utc>,
}

async fn get_progress(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(book_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    ensure_book_access(&state, &auth, book_id, reader_artifact_access()).await?;
    let user_id = auth_user_id(&auth)?;
    let row = sqlx::query_as::<_, ProgressRow>(progress_select_sql())
        .bind(book_id)
        .bind(user_id)
        .fetch_optional(&state.db)
        .await
        .map_err(ApiError::from)?;

    match row {
        Some(p) => Ok(Json(serde_json::json!({
            "book_id": p.book_id,
            "progress": p.progress,
            "current_chapter": p.current_chapter,
            "chapter_index": p.chapter_index,
            "reading_time_secs": p.reading_time_secs,
            "last_read_at": p.last_read_at.to_rfc3339(),
        }))),
        None => Ok(Json(serde_json::json!({
            "book_id": book_id,
            "progress": 0.0,
            "current_chapter": 0,
            "chapter_index": null,
            "reading_time_secs": 0,
            "last_read_at": null,
        }))),
    }
}

#[derive(Deserialize)]
struct UpdateProgressRequest {
    progress: Option<f64>,
    current_chapter: Option<i32>,
    chapter_index: Option<i32>,
    reading_time_secs: Option<i64>,
    /// Client timestamp (ISO 8601) for conflict detection in multi-device scenarios.
    /// If the server's last_read_at is newer than this, a conflict is signaled.
    client_last_read_at: Option<String>,
    /// If true, force overwrite even if a conflict is detected.
    force: Option<bool>,
}

async fn update_progress(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(book_id): Path<Uuid>,
    Json(body): Json<UpdateProgressRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    ensure_book_access(&state, &auth, book_id, reader_artifact_access()).await?;
    let user_id = auth_user_id(&auth)?;
    let progress = body.progress.unwrap_or(0.0);
    let current_chapter = body.current_chapter.unwrap_or(0);
    let reading_time = body.reading_time_secs.unwrap_or(0);

    // Conflict detection: if client provides its last known timestamp,
    // check if another device has updated since
    if let Some(ref client_ts_str) = body.client_last_read_at {
        if body.force != Some(true) {
            if let Ok(client_ts) = chrono::DateTime::parse_from_rfc3339(client_ts_str) {
                let server_ts: Option<chrono::DateTime<chrono::Utc>> =
                    sqlx::query_scalar(progress_timestamp_sql())
                        .bind(book_id)
                        .bind(user_id)
                        .fetch_optional(&state.db)
                        .await
                        .map_err(ApiError::from)?;

                if let Some(server_last) = server_ts {
                    if server_last > client_ts {
                        // Conflict: server has newer data
                        let server_row =
                            sqlx::query_as::<_, ProgressRow>(progress_conflict_select_sql())
                                .bind(book_id)
                                .bind(user_id)
                                .fetch_one(&state.db)
                                .await
                                .map_err(ApiError::from)?;

                        return Ok(Json(serde_json::json!({
                            "status": "conflict",
                            "message": "Server has newer progress from another device",
                            "server_progress": {
                                "progress": server_row.progress,
                                "current_chapter": server_row.current_chapter,
                                "chapter_index": server_row.chapter_index,
                                "reading_time_secs": server_row.reading_time_secs,
                                "last_read_at": server_row.last_read_at.to_rfc3339(),
                            },
                            "client_progress": {
                                "progress": progress,
                                "current_chapter": current_chapter,
                                "chapter_index": body.chapter_index,
                                "reading_time_secs": reading_time,
                                "client_last_read_at": client_ts_str,
                            },
                        })));
                    }
                }
            }
        }
    }

    sqlx::query(progress_upsert_sql())
        .bind(user_id)
        .bind(book_id)
        .bind(progress)
        .bind(current_chapter)
        .bind(body.chapter_index)
        .bind(reading_time)
        .execute(&state.db)
        .await
        .map_err(ApiError::from)?;

    if ensure_book_access(&state, &auth, book_id, LibraryAccess::Write)
        .await
        .is_ok()
    {
        if progress > 0.0 && progress < 1.0 {
            let _ = sqlx::query("UPDATE books SET reading_status = 'reading' WHERE id = $1 AND reading_status = 'unread'")
                .bind(book_id)
                .execute(&state.db)
                .await;
        } else if progress >= 1.0 {
            let _ =
                sqlx::query("UPDATE books SET reading_status = $2::reading_status WHERE id = $1")
                    .bind(book_id)
                    .bind(completed_reading_status_value())
                    .execute(&state.db)
                    .await;
        }
    }

    Ok(Json(serde_json::json!({ "status": "ok" })))
}

#[derive(sqlx::FromRow, Serialize)]
struct SessionRow {
    id: Uuid,
    book_id: Uuid,
    started_at: chrono::DateTime<chrono::Utc>,
    ended_at: Option<chrono::DateTime<chrono::Utc>>,
    duration_secs: i32,
    pages_read: i32,
    words_read: i64,
}

async fn list_sessions(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(book_id): Path<Uuid>,
) -> Result<Json<Vec<SessionRow>>, ApiError> {
    ensure_book_access(&state, &auth, book_id, reader_artifact_access()).await?;
    let user_id = auth_user_id(&auth)?;
    let rows = sqlx::query_as::<_, SessionRow>(sessions_select_sql())
        .bind(book_id)
        .bind(user_id)
        .fetch_all(&state.db)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(rows))
}

#[derive(Deserialize)]
struct CreateSessionRequest {
    started_at: Option<chrono::DateTime<chrono::Utc>>,
    duration_secs: Option<i32>,
    pages_read: Option<i32>,
    words_read: Option<i64>,
    start_chapter: Option<i32>,
}

async fn create_session(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(book_id): Path<Uuid>,
    Json(body): Json<CreateSessionRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    ensure_book_access(&state, &auth, book_id, reader_artifact_access()).await?;
    let user_id = auth_user_id(&auth)?;
    let id = Uuid::now_v7();
    let started = body.started_at.unwrap_or_else(chrono::Utc::now);

    sqlx::query(session_insert_sql())
        .bind(id)
        .bind(user_id)
        .bind(book_id)
        .bind(started)
        .bind(body.duration_secs.unwrap_or(0))
        .bind(body.pages_read.unwrap_or(0))
        .bind(body.words_read.unwrap_or(0))
        .bind(body.start_chapter)
        .execute(&state.db)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::json!({ "id": id, "book_id": book_id })))
}

#[derive(Deserialize)]
struct UpdateSessionRequest {
    duration_secs: Option<i32>,
    pages_read: Option<i32>,
    words_read: Option<i64>,
    end_chapter: Option<i32>,
}

async fn update_current_session(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(book_id): Path<Uuid>,
    Json(body): Json<UpdateSessionRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    ensure_book_access(&state, &auth, book_id, reader_artifact_access()).await?;
    let user_id = auth_user_id(&auth)?;
    sqlx::query(current_session_update_sql())
        .bind(book_id)
        .bind(user_id)
        .bind(body.duration_secs)
        .bind(body.pages_read)
        .bind(body.words_read)
        .bind(body.end_chapter)
        .execute(&state.db)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::json!({ "status": "ok" })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completed_progress_uses_book_reading_status_enum_value() {
        assert_eq!(completed_reading_status_value(), "completed");
    }

    #[test]
    fn reader_artifacts_only_require_read_access() {
        assert_eq!(reader_artifact_access(), LibraryAccess::Read);
    }

    #[test]
    fn progress_queries_are_scoped_to_current_user() {
        assert!(progress_select_sql().contains("user_id = $2"));
        assert!(!progress_select_sql().contains("user_id IS NULL"));
        assert!(progress_upsert_sql().contains("user_id, book_id"));
        assert!(progress_upsert_sql().contains("ON CONFLICT (user_id, book_id)"));
    }

    #[test]
    fn session_queries_are_scoped_to_current_user() {
        assert!(sessions_select_sql().contains("user_id = $2"));
        assert!(session_insert_sql().contains("user_id"));
        assert!(current_session_update_sql().contains("user_id = $2"));
    }
}
