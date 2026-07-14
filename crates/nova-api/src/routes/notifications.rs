use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::extractors::AuthUser;
use crate::state::AppState;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/notifications",
            get(list_notifications).delete(clear_notifications),
        )
        .route("/notifications/unread-count", get(unread_count))
        .route("/notifications/read-all", post(mark_all_read))
        .route("/notifications/{id}/read", post(mark_read))
        .route(
            "/notifications/{id}",
            axum::routing::delete(delete_notification),
        )
}

// ─────────────────────────── Emission helpers ───────────────────────────

/// A notification to be persisted. Construct via the builder helpers below.
#[derive(Debug, Clone)]
pub struct NewNotification {
    pub level: &'static str,
    pub category: &'static str,
    pub title: String,
    pub body: String,
    pub link: Option<String>,
    pub book_id: Option<Uuid>,
    pub metadata: serde_json::Value,
}

impl NewNotification {
    pub fn new(level: &'static str, category: &'static str, title: impl Into<String>) -> Self {
        Self {
            level,
            category,
            title: title.into(),
            body: String::new(),
            link: None,
            book_id: None,
            metadata: serde_json::json!({}),
        }
    }

    pub fn body(mut self, body: impl Into<String>) -> Self {
        self.body = body.into();
        self
    }

    pub fn link(mut self, link: impl Into<String>) -> Self {
        self.link = Some(link.into());
        self
    }

    pub fn book(mut self, book_id: Uuid) -> Self {
        self.book_id = Some(book_id);
        self
    }

    pub fn metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }
}

/// Persist a notification for a single user. Errors are logged, never propagated,
/// so notification failures can never break the originating request.
pub async fn emit(db: &PgPool, user_id: Uuid, n: NewNotification) {
    let res = sqlx::query(
        "INSERT INTO notifications (user_id, level, category, title, body, link, book_id, metadata) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(user_id)
    .bind(n.level)
    .bind(n.category)
    .bind(&n.title)
    .bind(&n.body)
    .bind(&n.link)
    .bind(n.book_id)
    .bind(&n.metadata)
    .execute(db)
    .await;
    if let Err(e) = res {
        tracing::warn!("failed to emit notification: {e}");
    }
}

/// Persist a notification for every user (system-wide broadcast).
#[allow(dead_code)]
pub async fn emit_broadcast(db: &PgPool, n: NewNotification) {
    let res = sqlx::query(
        "INSERT INTO notifications (user_id, level, category, title, body, link, book_id, metadata) \
         SELECT id, $1, $2, $3, $4, $5, $6, $7 FROM users",
    )
    .bind(n.level)
    .bind(n.category)
    .bind(&n.title)
    .bind(&n.body)
    .bind(&n.link)
    .bind(n.book_id)
    .bind(&n.metadata)
    .execute(db)
    .await;
    if let Err(e) = res {
        tracing::warn!("failed to broadcast notification: {e}");
    }
}

// ─────────────────────────── Handlers ───────────────────────────

#[derive(Debug, Deserialize)]
struct ListQuery {
    category: Option<String>,
    #[serde(default)]
    unread_only: bool,
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}

fn default_limit() -> i64 {
    50
}

fn parse_user(auth: &AuthUser) -> ApiResult<Uuid> {
    Uuid::parse_str(&auth.id).map_err(|_| ApiError::unauthorized())
}

#[derive(sqlx::FromRow)]
struct NotificationRow {
    id: Uuid,
    level: String,
    category: String,
    title: String,
    body: String,
    link: Option<String>,
    book_id: Option<Uuid>,
    metadata: serde_json::Value,
    read_at: Option<chrono::DateTime<chrono::Utc>>,
    created_at: chrono::DateTime<chrono::Utc>,
}

fn row_to_json(r: NotificationRow) -> serde_json::Value {
    serde_json::json!({
        "id": r.id.to_string(),
        "level": r.level,
        "category": r.category,
        "title": r.title,
        "body": r.body,
        "link": r.link,
        "book_id": r.book_id.map(|b| b.to_string()),
        "metadata": r.metadata,
        "read": r.read_at.is_some(),
        "created_at": r.created_at.to_rfc3339(),
    })
}

async fn list_notifications(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let user_id = parse_user(&auth)?;
    let limit = q.limit.clamp(1, 200);
    let offset = q.offset.max(0);
    let category = q.category.filter(|c| c != "all");

    let rows: Vec<NotificationRow> = sqlx::query_as(
        "SELECT id, level, category, title, body, link, book_id, metadata, read_at, created_at \
         FROM notifications \
         WHERE user_id = $1 \
           AND ($2::text IS NULL OR category = $2) \
           AND ($3 = false OR read_at IS NULL) \
         ORDER BY created_at DESC \
         LIMIT $4 OFFSET $5",
    )
    .bind(user_id)
    .bind(&category)
    .bind(q.unread_only)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;

    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM notifications WHERE user_id = $1 \
           AND ($2::text IS NULL OR category = $2)",
    )
    .bind(user_id)
    .bind(&category)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    let unread: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM notifications WHERE user_id = $1 AND read_at IS NULL",
    )
    .bind(user_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    Ok(Json(serde_json::json!({
        "items": rows.into_iter().map(row_to_json).collect::<Vec<_>>(),
        "total": total,
        "unread": unread,
    })))
}

async fn unread_count(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    let user_id = parse_user(&auth)?;
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM notifications WHERE user_id = $1 AND read_at IS NULL",
    )
    .bind(user_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);
    Ok(Json(serde_json::json!({ "count": count })))
}

async fn mark_read(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let user_id = parse_user(&auth)?;
    sqlx::query(
        "UPDATE notifications SET read_at = now() WHERE id = $1 AND user_id = $2 AND read_at IS NULL",
    )
    .bind(id)
    .bind(user_id)
    .execute(&state.db)
    .await
    .map_err(ApiError::from)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn mark_all_read(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    let user_id = parse_user(&auth)?;
    let result = sqlx::query(
        "UPDATE notifications SET read_at = now() WHERE user_id = $1 AND read_at IS NULL",
    )
    .bind(user_id)
    .execute(&state.db)
    .await
    .map_err(ApiError::from)?;
    Ok(Json(
        serde_json::json!({ "ok": true, "updated": result.rows_affected() }),
    ))
}

async fn delete_notification(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let user_id = parse_user(&auth)?;
    sqlx::query("DELETE FROM notifications WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user_id)
        .execute(&state.db)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Debug, Deserialize)]
struct ClearQuery {
    #[serde(default)]
    read_only: bool,
}

async fn clear_notifications(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(q): Query<ClearQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let user_id = parse_user(&auth)?;
    let sql = if q.read_only {
        "DELETE FROM notifications WHERE user_id = $1 AND read_at IS NOT NULL"
    } else {
        "DELETE FROM notifications WHERE user_id = $1"
    };
    let result = sqlx::query(sql)
        .bind(user_id)
        .execute(&state.db)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(
        serde_json::json!({ "ok": true, "deleted": result.rows_affected() }),
    ))
}
