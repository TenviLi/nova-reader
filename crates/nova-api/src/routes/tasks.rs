use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    response::sse::{Event, Sse},
    routing::{get, post},
    Json, Router,
};
use futures::stream::{self, Stream};
use serde::Deserialize;
use sqlx::{Postgres, QueryBuilder};
use std::convert::Infallible;
use std::time::Duration;
use uuid::Uuid;

use crate::access::{ensure_book_access, is_admin, visible_library_ids, LibraryAccess};
use crate::error::{ApiError, ApiResult};
use crate::extractors::AuthUser;
use crate::state::AppState;
use crate::task_queue::TaskRow;
use nova_core::domain::task::{QueueStats, TaskDag};

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/tasks", get(list_tasks))
        .route("/tasks/{id}", get(get_task))
        .route("/tasks/{id}/cancel", post(cancel_task))
        .route("/tasks/{id}/retry", post(retry_task))
        .route("/tasks/stats", get(get_queue_stats))
        .route("/tasks/stream", get(task_stream))
        .route("/tasks/submit-pipeline", post(submit_pipeline))
}

#[derive(Debug, Deserialize)]
struct ListTasksQuery {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    book_id: Option<Uuid>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    page: Option<i64>,
    #[serde(default)]
    per_page: Option<i64>,
}

fn valid_task_status(status: Option<&str>) -> Option<&str> {
    let valid_statuses = [
        "queued",
        "running",
        "completed",
        "failed",
        "retrying",
        "cancelled",
        "dead_letter",
    ];
    status.filter(|value| valid_statuses.contains(value))
}

fn valid_task_category(category: Option<&str>) -> Option<&str> {
    let valid_categories = ["import", "preprocess", "ai", "index", "maintenance"];
    category.filter(|value| valid_categories.contains(value))
}

fn add_visible_task_filters<'a>(
    query: &mut QueryBuilder<'a, Postgres>,
    visible_ids: &'a [Uuid],
    status: Option<&'a str>,
    category: Option<&'a str>,
) {
    query
        .push(" FROM tasks t JOIN books b ON b.id = t.book_id WHERE b.library_id = ANY(")
        .push_bind(visible_ids)
        .push("::uuid[])");

    if let Some(status) = valid_task_status(status) {
        query
            .push(" AND t.status = ")
            .push_bind(status)
            .push("::task_status");
    }
    if let Some(category) = valid_task_category(category) {
        query.push(" AND t.category = ").push_bind(category);
    }
}

async fn list_visible_book_tasks(
    state: &AppState,
    visible_ids: &[Uuid],
    status: Option<&str>,
    category: Option<&str>,
    limit: i64,
    offset: i64,
) -> ApiResult<(Vec<TaskRow>, i64)> {
    if visible_ids.is_empty() {
        return Ok((Vec::new(), 0));
    }

    let mut count_query = QueryBuilder::<Postgres>::new("SELECT COUNT(*)");
    add_visible_task_filters(&mut count_query, visible_ids, status, category);
    let total: (i64,) = count_query
        .build_query_as()
        .fetch_one(&state.db)
        .await
        .map_err(|e| ApiError::Internal(format!("DB error: {}", e)))?;

    let mut data_query = QueryBuilder::<Postgres>::new(
        "SELECT t.id, t.kind::text, t.status::text, t.priority::text,
                t.payload, t.result, t.error_message, t.retry_count, t.max_retries,
                t.book_id, t.category, t.progress, t.progress_message,
                t.scheduled_at, t.started_at, t.completed_at, t.created_at",
    );
    add_visible_task_filters(&mut data_query, visible_ids, status, category);
    data_query
        .push(" ORDER BY t.created_at DESC LIMIT ")
        .push_bind(limit)
        .push(" OFFSET ")
        .push_bind(offset);

    let tasks = data_query
        .build_query_as::<TaskRow>()
        .fetch_all(&state.db)
        .await
        .map_err(|e| ApiError::Internal(format!("DB error: {}", e)))?;

    Ok((tasks, total.0))
}

fn empty_queue_stats() -> QueueStats {
    QueueStats {
        queued: 0,
        running: 0,
        completed_today: 0,
        failed_today: 0,
        dead_letter_count: 0,
        avg_processing_time_ms: 0.0,
    }
}

async fn scoped_queue_stats(state: &AppState, visible_ids: &[Uuid]) -> ApiResult<QueueStats> {
    if visible_ids.is_empty() {
        return Ok(empty_queue_stats());
    }

    let row = sqlx::query_as::<_, (i64, i64, i64, i64, i64, f64)>(
        "SELECT
                COUNT(*) FILTER (WHERE t.status = 'queued') as queued,
                COUNT(*) FILTER (WHERE t.status = 'running') as running,
                COUNT(*) FILTER (WHERE t.status = 'completed' AND t.completed_at > NOW() - INTERVAL '24 hours') as completed_today,
                COUNT(*) FILTER (WHERE t.status IN ('failed', 'dead_letter') AND t.completed_at > NOW() - INTERVAL '24 hours') as failed_today,
                COUNT(*) FILTER (WHERE t.status = 'dead_letter') as dead_letter_count,
                COALESCE(AVG(EXTRACT(EPOCH FROM (t.completed_at - t.started_at)) * 1000)
                    FILTER (WHERE t.status = 'completed' AND t.completed_at > NOW() - INTERVAL '24 hours'), 0)::float8 as avg_ms
         FROM tasks t
         JOIN books b ON b.id = t.book_id
         WHERE b.library_id = ANY($1::uuid[])",
    )
    .bind(visible_ids)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("DB error: {}", e)))?;

    Ok(QueueStats {
        queued: row.0,
        running: row.1,
        completed_today: row.2,
        failed_today: row.3,
        dead_letter_count: row.4,
        avg_processing_time_ms: row.5,
    })
}

async fn global_category_stats(state: &AppState) -> Vec<serde_json::Value> {
    let categories: Vec<(String, i64, i64, i64)> = sqlx::query_as(
        "SELECT category,
                COUNT(*) FILTER (WHERE status = 'queued') as queued,
                COUNT(*) FILTER (WHERE status = 'running') as running,
                COUNT(*) FILTER (WHERE status = 'completed' AND completed_at > NOW() - INTERVAL '24 hours') as completed
         FROM tasks
         GROUP BY category",
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    categories
        .into_iter()
        .map(|(cat, q, r, c)| {
            serde_json::json!({ "category": cat, "queued": q, "running": r, "completed_today": c })
        })
        .collect()
}

async fn scoped_category_stats(
    state: &AppState,
    visible_ids: &[Uuid],
) -> ApiResult<Vec<serde_json::Value>> {
    if visible_ids.is_empty() {
        return Ok(Vec::new());
    }

    let categories: Vec<(String, i64, i64, i64)> = sqlx::query_as(
        "SELECT t.category,
                COUNT(*) FILTER (WHERE t.status = 'queued') as queued,
                COUNT(*) FILTER (WHERE t.status = 'running') as running,
                COUNT(*) FILTER (WHERE t.status = 'completed' AND t.completed_at > NOW() - INTERVAL '24 hours') as completed
         FROM tasks t
         JOIN books b ON b.id = t.book_id
         WHERE b.library_id = ANY($1::uuid[])
         GROUP BY t.category",
    )
    .bind(visible_ids)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("DB error: {}", e)))?;

    Ok(categories
        .into_iter()
        .map(|(cat, q, r, c)| {
            serde_json::json!({ "category": cat, "queued": q, "running": r, "completed_today": c })
        })
        .collect())
}

async fn stream_queue_counts(db: &sqlx::PgPool, visible_ids: Option<&[Uuid]>) -> (i64, i64) {
    match visible_ids {
        Some(ids) if ids.is_empty() => (0, 0),
        Some(ids) => sqlx::query_as::<_, (i64, i64)>(
            "SELECT COUNT(*) FILTER (WHERE t.status = 'running'),
                    COUNT(*) FILTER (WHERE t.status = 'queued')
             FROM tasks t
             JOIN books b ON b.id = t.book_id
             WHERE b.library_id = ANY($1::uuid[])",
        )
        .bind(ids)
        .fetch_one(db)
        .await
        .unwrap_or((0, 0)),
        None => sqlx::query_as::<_, (i64, i64)>(
            "SELECT COUNT(*) FILTER (WHERE status = 'running'),
                    COUNT(*) FILTER (WHERE status = 'queued')
             FROM tasks",
        )
        .fetch_one(db)
        .await
        .unwrap_or((0, 0)),
    }
}

async fn task_book_id(state: &AppState, task_id: Uuid) -> ApiResult<Option<Uuid>> {
    sqlx::query_as::<_, (Option<Uuid>,)>("SELECT book_id FROM tasks WHERE id = $1")
        .bind(task_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| ApiError::Internal(format!("DB error: {}", e)))?
        .map(|row| row.0)
        .ok_or_else(|| ApiError::NotFound(format!("Task {} not found", task_id)))
}

async fn ensure_task_access(
    state: &AppState,
    auth: &AuthUser,
    task_id: Uuid,
    access: LibraryAccess,
) -> ApiResult<()> {
    match task_book_id(state, task_id).await? {
        Some(book_id) => ensure_book_access(state, auth, book_id, access).await,
        None if is_admin(state, auth).await? => Ok(()),
        None => Err(ApiError::forbidden()),
    }
}

/// List tasks with filtering and pagination.
async fn list_tasks(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(params): Query<ListTasksQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let per_page = params.per_page.unwrap_or(20).min(100);
    let page = params.page.unwrap_or(1).max(1);
    let offset = (page - 1) * per_page;

    let (tasks, total) = if let Some(book_id) = params.book_id {
        ensure_book_access(&state, &auth, book_id, LibraryAccess::Read).await?;
        state
            .task_queue
            .list(
                params.status.as_deref(),
                params.book_id,
                params.category.as_deref(),
                per_page,
                offset,
            )
            .await
            .map_err(|e| ApiError::Internal(format!("DB error: {}", e)))?
    } else if is_admin(&state, &auth).await? {
        state
            .task_queue
            .list(
                params.status.as_deref(),
                None,
                params.category.as_deref(),
                per_page,
                offset,
            )
            .await
            .map_err(|e| ApiError::Internal(format!("DB error: {}", e)))?
    } else {
        let visible_ids = visible_library_ids(&state, &auth, LibraryAccess::Read)
            .await?
            .unwrap_or_default();
        list_visible_book_tasks(
            &state,
            &visible_ids,
            params.status.as_deref(),
            params.category.as_deref(),
            per_page,
            offset,
        )
        .await?
    };

    Ok(Json(serde_json::json!({
        "data": tasks,
        "total": total,
        "page": page,
        "per_page": per_page,
    })))
}

/// Get a specific task with its dependencies.
async fn get_task(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_task_access(&state, &auth, id, LibraryAccess::Read).await?;
    let task = sqlx::query_as::<_, TaskRow>(
        "SELECT t.id, t.kind::text, t.status::text, t.priority::text,
                t.payload, t.result, t.error_message, t.retry_count, t.max_retries,
                t.book_id, t.category, t.progress, t.progress_message,
                t.scheduled_at, t.started_at, t.completed_at, t.created_at
         FROM tasks t WHERE t.id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("DB error: {}", e)))?;

    match task {
        Some(t) => {
            let deps = state
                .task_queue
                .get_dependencies(id)
                .await
                .map_err(|e| ApiError::Internal(format!("DB error: {}", e)))?;
            Ok(Json(serde_json::json!({
                "task": t,
                "dependencies": deps,
            })))
        }
        None => Err(ApiError::NotFound(format!("Task {} not found", id))),
    }
}

/// Cancel a running or queued task (and dependents).
async fn cancel_task(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_task_access(&state, &auth, id, LibraryAccess::Write).await?;
    let cancelled = state
        .task_queue
        .cancel(id)
        .await
        .map_err(|e| ApiError::Internal(format!("DB error: {}", e)))?;

    Ok(Json(serde_json::json!({
        "message": format!("Cancelled {} task(s)", cancelled),
        "cancelled_count": cancelled,
    })))
}

/// Retry a failed task — reset to queued.
async fn retry_task(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_task_access(&state, &auth, id, LibraryAccess::Write).await?;
    sqlx::query(
        "UPDATE tasks SET status = 'queued'::task_status, error_message = NULL,
                progress = 0, progress_message = NULL, scheduled_at = NOW()
         WHERE id = $1 AND status IN ('failed', 'dead_letter', 'cancelled')",
    )
    .bind(id)
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("DB error: {}", e)))?;

    Ok(Json(serde_json::json!({
        "message": "Task re-queued",
        "id": id.to_string(),
    })))
}

/// Get queue statistics for the dashboard.
async fn get_queue_stats(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    let (stats, category_stats) = if is_admin(&state, &auth).await? {
        (
            state
                .task_queue
                .stats()
                .await
                .map_err(|e| ApiError::Internal(format!("DB error: {}", e)))?,
            global_category_stats(&state).await,
        )
    } else {
        let visible_ids = visible_library_ids(&state, &auth, LibraryAccess::Read)
            .await?
            .unwrap_or_default();
        (
            scoped_queue_stats(&state, &visible_ids).await?,
            scoped_category_stats(&state, &visible_ids).await?,
        )
    };

    Ok(Json(serde_json::json!({
        "stats": stats,
        "categories": category_stats,
    })))
}

#[derive(Deserialize)]
struct SubmitPipelineBody {
    book_id: Uuid,
    #[serde(default = "default_pipeline")]
    pipeline: String,
}

fn default_pipeline() -> String {
    "full".to_string()
}

/// Submit a full processing pipeline DAG for a book.
async fn submit_pipeline(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<SubmitPipelineBody>,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_book_access(&state, &auth, body.book_id, LibraryAccess::Write).await?;
    let dag = match body.pipeline.as_str() {
        "full" => TaskDag::full_pipeline(body.book_id),
        "reindex" => TaskDag::reindex_pipeline(body.book_id),
        "deep_analysis" => TaskDag::deep_analysis_pipeline(body.book_id),
        _ => {
            return Err(ApiError::bad_request(
                "Invalid pipeline type. Use 'full', 'reindex', or 'deep_analysis'",
            ))
        }
    };

    let task_ids = state
        .task_queue
        .submit_dag(&dag)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to submit DAG: {}", e)))?;

    Ok(Json(serde_json::json!({
        "message": format!("Submitted {} tasks for book {}", task_ids.len(), body.book_id),
        "task_ids": task_ids,
        "pipeline": body.pipeline,
    })))
}

/// SSE stream for real-time task updates.
async fn task_stream(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    let db = state.db.clone();
    let visible_ids = if is_admin(&state, &auth).await? {
        None
    } else {
        Some(
            visible_library_ids(&state, &auth, LibraryAccess::Read)
                .await?
                .unwrap_or_default(),
        )
    };
    let stream = stream::unfold((db, visible_ids), move |(db, visible_ids)| async move {
        tokio::time::sleep(Duration::from_secs(2)).await;

        let stats = stream_queue_counts(&db, visible_ids.as_deref()).await;

        let event_data = serde_json::json!({
            "running": stats.0,
            "queued": stats.1,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });

        let event = Event::default()
            .event("task_update")
            .data(event_data.to_string());

        Some((Ok(event), (db, visible_ids)))
    });

    Ok(Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("heartbeat"),
    ))
}

#[cfg(test)]
mod tests {
    fn production_source() -> &'static str {
        include_str!("tasks.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("production source should exist")
    }

    #[test]
    fn book_bound_task_routes_require_acl() {
        let source = production_source();

        assert!(source.contains("extractors::AuthUser"));
        assert!(source.contains("ensure_task_access"));
        assert!(source
            .contains("ensure_book_access(&state, &auth, book_id, LibraryAccess::Read).await?"));
        assert!(
            source.contains("ensure_task_access(&state, &auth, id, LibraryAccess::Read).await?")
        );
        assert_eq!(
            source
                .matches("ensure_task_access(&state, &auth, id, LibraryAccess::Write).await?")
                .count(),
            2
        );
        assert!(source.contains(
            "ensure_book_access(&state, &auth, body.book_id, LibraryAccess::Write).await?"
        ));
    }

    #[test]
    fn task_stats_and_stream_are_scoped_for_non_admins() {
        let source = production_source();

        assert!(source.contains("async fn scoped_queue_stats("));
        assert!(source.contains("async fn scoped_category_stats("));
        assert!(source.contains("async fn stream_queue_counts("));
        assert!(source.contains("JOIN books b ON b.id = t.book_id"));

        assert!(source.contains("auth: AuthUser"));
        assert!(source.contains("visible_library_ids(&state, &auth, LibraryAccess::Read)"));
        assert!(source.contains("scoped_queue_stats(&state, &visible_ids).await?"));
        assert!(source.contains("scoped_category_stats(&state, &visible_ids).await?"));
        assert!(source.contains("stream_queue_counts(&db, visible_ids.as_deref()).await"));
    }
}
