use std::sync::Arc;

use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use chrono::Datelike;
use redis::AsyncCommands;
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    access::{auth_user_id, ensure_book_access, visible_library_ids, LibraryAccess},
    error::ApiError,
    extractors::AuthUser,
    state::AppState,
};

const DASHBOARD_CACHE_KEY_PREFIX: &str = "nova:cache:dashboard";
const DASHBOARD_CACHE_TTL: u64 = 60; // 60 seconds

fn stats_library_access() -> LibraryAccess {
    LibraryAccess::Read
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

fn dashboard_cache_key(user_id: Uuid, library_scope: Option<&[Uuid]>) -> String {
    let scope = match library_scope {
        None => "all".to_string(),
        Some(ids) if ids.is_empty() => "none".to_string(),
        Some(ids) => {
            let mut ids = ids.iter().map(Uuid::to_string).collect::<Vec<_>>();
            ids.sort();
            ids.join(",")
        }
    };
    format!("{DASHBOARD_CACHE_KEY_PREFIX}:user:{user_id}:libraries:{scope}")
}

fn empty_system_stats() -> serde_json::Value {
    serde_json::json!({
        "total_books": 0,
        "total_annotations": 0,
        "total_entities": 0,
        "total_chapters": 0,
        "storage_used_bytes": 0,
        "tasks_pending": 0,
        "tasks_completed": 0,
    })
}

fn empty_dashboard_stats() -> serde_json::Value {
    serde_json::json!({
        "total_books": 0,
        "books_in_progress": 0,
        "currently_reading": 0,
        "finished": 0,
        "total_annotations": 0,
        "total_sessions": 0,
        "reading_time_today_mins": 0,
        "tasks_running": 0,
        "storage_used_gb": 0.0,
        "entities_extracted": 0,
    })
}

fn empty_reading_stats(days: i32) -> serde_json::Value {
    serde_json::json!({
        "totalBooksRead": 0,
        "totalReadingTime": 0,
        "totalWordsRead": 0,
        "totalAnnotations": 0,
        "avgDailyMinutes": 0,
        "longestStreak": 0,
        "currentStreak": 0,
        "booksThisMonth": 0,
        "pagesThisWeek": 0,
        "period_days": days,
        "sessions": 0,
        "total_duration_secs": 0,
        "total_pages": 0,
        "total_words": 0,
        "avg_session_mins": 0,
    })
}

fn empty_library_stats() -> serde_json::Value {
    serde_json::json!({
        "total_books": 0,
        "total_words": 0,
        "total_size_bytes": 0,
        "total_libraries": 0,
        "total_persons": 0,
    })
}

fn count_books_sql(scoped: bool) -> String {
    format!(
        "SELECT COUNT(*) FROM books b WHERE TRUE{}",
        book_library_filter("b", 1, scoped)
    )
}

fn count_books_by_reading_status_sql(scoped: bool) -> String {
    format!(
        "SELECT COUNT(*) FROM books b WHERE b.reading_status = $1::reading_status{}",
        book_library_filter("b", 2, scoped)
    )
}

fn storage_used_sql(scoped: bool) -> String {
    format!(
        "SELECT COALESCE(SUM(b.file_size_bytes), 0)::bigint FROM books b WHERE TRUE{}",
        book_library_filter("b", 1, scoped)
    )
}

fn count_annotations_sql(scoped_user: bool, scoped_libraries: bool) -> String {
    let mut sql = String::from(
        "SELECT COUNT(*) FROM annotations a JOIN books b ON b.id = a.book_id WHERE TRUE",
    );
    if scoped_user {
        sql.push_str(" AND a.user_id = $1");
    }
    let library_placeholder = if scoped_user { 2 } else { 1 };
    sql.push_str(&book_library_filter(
        "b",
        library_placeholder,
        scoped_libraries,
    ));
    sql
}

fn count_entities_sql(scoped: bool) -> String {
    format!(
        "SELECT COUNT(*) FROM entities e JOIN books b ON b.id = e.book_id WHERE TRUE{}",
        book_library_filter("b", 1, scoped)
    )
}

fn count_chapters_sql(scoped: bool) -> String {
    format!(
        "SELECT COUNT(*) FROM chapters c JOIN books b ON b.id = c.book_id WHERE TRUE{}",
        book_library_filter("b", 1, scoped)
    )
}

fn count_tasks_by_status_sql(scoped: bool) -> String {
    if scoped {
        String::from(
            "SELECT COUNT(*) FROM tasks t JOIN books b ON b.id = t.book_id WHERE t.status = $1::task_status AND b.library_id = ANY($2::uuid[])",
        )
    } else {
        String::from("SELECT COUNT(*) FROM tasks WHERE status = $1::task_status")
    }
}

fn reading_session_count_since_sql(scoped: bool) -> String {
    format!(
        r#"
        SELECT COUNT(*)
        FROM reading_sessions rs
        JOIN books b ON b.id = rs.book_id
        WHERE rs.user_id = $1
          AND rs.started_at > NOW() - make_interval(days => $2)
          {}
        "#,
        book_library_filter("b", 3, scoped)
    )
}

fn dashboard_session_count_sql(scoped: bool) -> String {
    format!(
        r#"
        SELECT COUNT(*)
        FROM reading_sessions rs
        JOIN books b ON b.id = rs.book_id
        WHERE rs.user_id = $1
          {}
        "#,
        book_library_filter("b", 2, scoped)
    )
}

fn dashboard_reading_time_today_sql(scoped: bool) -> String {
    format!(
        r#"
        SELECT COALESCE(SUM(rs.duration_secs), 0)::bigint
        FROM reading_sessions rs
        JOIN books b ON b.id = rs.book_id
        WHERE rs.user_id = $1
          AND rs.started_at::date = CURRENT_DATE
          {}
        "#,
        book_library_filter("b", 2, scoped)
    )
}

fn reading_session_sum_since_sql(column: &str, scoped: bool) -> String {
    format!(
        r#"
        SELECT COALESCE(SUM(rs.{column}), 0)::bigint
        FROM reading_sessions rs
        JOIN books b ON b.id = rs.book_id
        WHERE rs.user_id = $1
          AND rs.started_at > NOW() - make_interval(days => $2)
          {}
        "#,
        book_library_filter("b", 3, scoped)
    )
}

fn user_completed_books_sql(scoped: bool, this_month: bool) -> String {
    let date_filter = if this_month {
        " AND rp.updated_at > NOW() - INTERVAL '30 days'"
    } else {
        ""
    };
    format!(
        r#"
        SELECT COUNT(DISTINCT rp.book_id)
        FROM reading_progress rp
        JOIN books b ON b.id = rp.book_id
        WHERE rp.user_id = $1
          AND rp.progress >= 1.0
          {date_filter}
          {}
        "#,
        book_library_filter("b", 2, scoped)
    )
}

fn user_annotations_count_sql(scoped: bool) -> String {
    format!(
        r#"
        SELECT COUNT(*)
        FROM annotations a
        JOIN books b ON b.id = a.book_id
        WHERE a.user_id = $1
          {}
        "#,
        book_library_filter("b", 2, scoped)
    )
}

fn pages_this_week_sql(scoped: bool) -> String {
    format!(
        r#"
        SELECT COALESCE(SUM(rs.pages_read), 0)::bigint
        FROM reading_sessions rs
        JOIN books b ON b.id = rs.book_id
        WHERE rs.user_id = $1
          AND rs.started_at > NOW() - INTERVAL '7 days'
          {}
        "#,
        book_library_filter("b", 2, scoped)
    )
}

fn streak_days_sql(scoped: bool) -> String {
    format!(
        r#"
        SELECT DISTINCT rs.started_at::date
        FROM reading_sessions rs
        JOIN books b ON b.id = rs.book_id
        WHERE rs.user_id = $1
          {}
        ORDER BY rs.started_at::date DESC
        LIMIT 365
        "#,
        book_library_filter("b", 2, scoped)
    )
}

fn reading_heatmap_sql(scoped: bool) -> String {
    format!(
        r#"
        SELECT rs.started_at::date as day,
               COALESCE(SUM(rs.duration_secs), 0)::bigint as total_secs,
               COUNT(*)::bigint as session_count
        FROM reading_sessions rs
        JOIN books b ON b.id = rs.book_id
        WHERE rs.user_id = $1
          AND rs.started_at > NOW() - make_interval(days => $2)
          {}
        GROUP BY rs.started_at::date
        ORDER BY day
        "#,
        book_library_filter("b", 3, scoped)
    )
}

fn library_words_sql(scoped: bool) -> String {
    format!(
        "SELECT COALESCE(SUM(b.word_count), 0)::bigint FROM books b WHERE TRUE{}",
        book_library_filter("b", 1, scoped)
    )
}

fn library_count_sql(scoped: bool) -> String {
    if scoped {
        String::from("SELECT COUNT(*) FROM libraries l WHERE l.id = ANY($1::uuid[])")
    } else {
        String::from("SELECT COUNT(*) FROM libraries")
    }
}

fn library_person_count_sql(scoped: bool) -> String {
    format!(
        r#"
        SELECT COUNT(DISTINCT bp.person_id)
        FROM book_persons bp
        JOIN books b ON b.id = bp.book_id
        WHERE TRUE{}
        "#,
        book_library_filter("b", 1, scoped)
    )
}

fn reading_sessions_sql(scoped: bool) -> String {
    let limit_placeholder = if scoped { 3 } else { 2 };
    format!(
        r#"
        SELECT rs.id, rs.book_id, rs.duration_secs::bigint as duration_secs,
               rs.words_read::bigint as words_read,
               rs.started_at, b.title as book_title
        FROM reading_sessions rs
        JOIN books b ON b.id = rs.book_id
        WHERE rs.user_id = $1
          {}
        ORDER BY rs.started_at DESC
        LIMIT ${limit_placeholder}
        "#,
        book_library_filter("b", 2, scoped)
    )
}

fn reading_memories_sql(scoped: bool) -> String {
    let limit_placeholder = if scoped { 5 } else { 4 };
    format!(
        r#"
        SELECT DISTINCT ON (rs.book_id)
               b.id AS book_id,
               b.title,
               b.author,
               b.metadata->>'cover_path' AS cover_path,
               rs.started_at AS read_date
        FROM reading_sessions rs
        JOIN books b ON b.id = rs.book_id
        WHERE rs.user_id = $1
          AND EXTRACT(MONTH FROM rs.started_at)::int = $2
          AND EXTRACT(DAY FROM rs.started_at)::int = $3
          AND EXTRACT(YEAR FROM rs.started_at)::int < EXTRACT(YEAR FROM NOW())::int
          AND b.status != 'archived'
          {}
        ORDER BY rs.book_id, rs.started_at DESC
        LIMIT ${limit_placeholder}
        "#,
        book_library_filter("b", 4, scoped)
    )
}

fn reading_goal_sessions_this_week_sql(scoped: bool) -> String {
    format!(
        r#"
        SELECT COUNT(DISTINCT rs.started_at::date)
        FROM reading_sessions rs
        JOIN books b ON b.id = rs.book_id
        WHERE rs.user_id = $1
          AND rs.started_at > NOW() - INTERVAL '7 days'
          {}
        "#,
        book_library_filter("b", 2, scoped)
    )
}

fn reading_goal_books_this_year_sql(scoped: bool) -> String {
    format!(
        r#"
        SELECT COUNT(DISTINCT rp.book_id)
        FROM reading_progress rp
        JOIN books b ON b.id = rp.book_id
        WHERE rp.user_id = $1
          AND rp.progress >= 1.0
          AND rp.updated_at > date_trunc('year', NOW())
          {}
        "#,
        book_library_filter("b", 2, scoped)
    )
}

fn activities_sql(scoped: bool) -> String {
    let limit_placeholder = if scoped { 3 } else { 2 };
    let offset_placeholder = if scoped { 4 } else { 3 };
    let library_filter = book_library_filter("b", 2, scoped);
    format!(
        r#"
        (
            SELECT rs.id, 'reading'::text as activity_type,
                   b.title as book_title, b.id as book_id,
                   CONCAT('阅读了 ', rs.duration_secs / 60, ' 分钟') as description,
                   rs.started_at as created_at,
                   (rs.duration_secs / 60)::bigint as duration_minutes,
                   rs.pages_read
            FROM reading_sessions rs
            JOIN books b ON b.id = rs.book_id
            WHERE rs.user_id = $1
              {library_filter}
        )
        UNION ALL
        (
            SELECT a.id, 'annotation'::text as activity_type,
                   b.title as book_title, b.id as book_id,
                   CONCAT('添加了批注: ', LEFT(a.selected_text, 50)) as description,
                   a.created_at,
                   NULL::bigint as duration_minutes,
                   NULL::integer as pages_read
            FROM annotations a
            JOIN books b ON b.id = a.book_id
            WHERE a.user_id = $1
              {library_filter}
        )
        UNION ALL
        (
            SELECT bm.id, 'annotation'::text as activity_type,
                   b.title as book_title, b.id as book_id,
                   CONCAT('添加了书签: ', bm.title) as description,
                   bm.created_at,
                   NULL::bigint as duration_minutes,
                   NULL::integer as pages_read
            FROM bookmarks bm
            JOIN books b ON b.id = bm.book_id
            WHERE bm.user_id = $1
              {library_filter}
        )
        ORDER BY created_at DESC
        LIMIT ${limit_placeholder} OFFSET ${offset_placeholder}
        "#
    )
}

fn record_session_insert_sql() -> &'static str {
    r#"
        INSERT INTO reading_sessions (id, user_id, book_id, started_at, duration_secs, pages_read, words_read, start_chapter, end_chapter)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        "#
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/stats", get(system_stats))
        .route("/stats/dashboard", get(dashboard))
        .route("/stats/reading", get(reading_stats))
        .route("/stats/reading/heatmap", get(reading_heatmap))
        .route(
            "/stats/reading/sessions",
            get(reading_sessions).post(record_reading_session),
        )
        .route("/stats/reading/memories", get(reading_memories))
        .route("/stats/reading/goals", get(reading_goals))
        .route("/stats/activities", get(activities))
        .route("/stats/library", get(library_stats))
}

/// System-level stats for the admin panel.
async fn system_stats(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user_id = auth_user_id(&auth)?;
    let visible_libraries = visible_library_ids(&state, &auth, stats_library_access()).await?;
    if scoped_library_ids_empty(&visible_libraries) {
        return Ok(Json(empty_system_stats()));
    }
    let library_scope = visible_libraries.as_deref();
    let scoped = library_scope.is_some();

    let total_books: i64 = if let Some(ids) = library_scope {
        sqlx::query_scalar(&count_books_sql(true))
            .bind(ids)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    } else {
        sqlx::query_scalar(&count_books_sql(false))
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    };
    let total_annotations: i64 = if let Some(ids) = library_scope {
        sqlx::query_scalar(&user_annotations_count_sql(true))
            .bind(user_id)
            .bind(ids)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    } else {
        sqlx::query_scalar(&count_annotations_sql(false, false))
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    };
    let total_entities: i64 = if let Some(ids) = library_scope {
        sqlx::query_scalar(&count_entities_sql(true))
            .bind(ids)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    } else {
        sqlx::query_scalar(&count_entities_sql(false))
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    };
    let total_chapters: i64 = if let Some(ids) = library_scope {
        sqlx::query_scalar(&count_chapters_sql(true))
            .bind(ids)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    } else {
        sqlx::query_scalar(&count_chapters_sql(false))
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    };
    let storage_used_bytes: i64 = if let Some(ids) = library_scope {
        sqlx::query_scalar(&storage_used_sql(true))
            .bind(ids)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    } else {
        sqlx::query_scalar(&storage_used_sql(false))
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    };
    let tasks_pending: i64 = if let Some(ids) = library_scope {
        sqlx::query_scalar(&count_tasks_by_status_sql(scoped))
            .bind("queued")
            .bind(ids)
            .fetch_one(&state.db)
            .await
            .unwrap_or(0)
    } else {
        sqlx::query_scalar(&count_tasks_by_status_sql(scoped))
            .bind("queued")
            .fetch_one(&state.db)
            .await
            .unwrap_or(0)
    };
    let tasks_completed: i64 = if let Some(ids) = library_scope {
        sqlx::query_scalar(&count_tasks_by_status_sql(scoped))
            .bind("completed")
            .bind(ids)
            .fetch_one(&state.db)
            .await
            .unwrap_or(0)
    } else {
        sqlx::query_scalar(&count_tasks_by_status_sql(scoped))
            .bind("completed")
            .fetch_one(&state.db)
            .await
            .unwrap_or(0)
    };

    Ok(Json(serde_json::json!({
        "total_books": total_books,
        "total_annotations": total_annotations,
        "total_entities": total_entities,
        "total_chapters": total_chapters,
        "storage_used_bytes": storage_used_bytes,
        "tasks_pending": tasks_pending,
        "tasks_completed": tasks_completed,
    })))
}

async fn dashboard(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user_id = auth_user_id(&auth)?;
    let visible_libraries = visible_library_ids(&state, &auth, stats_library_access()).await?;
    if scoped_library_ids_empty(&visible_libraries) {
        return Ok(Json(empty_dashboard_stats()));
    }
    let library_scope = visible_libraries.as_deref();
    let scoped = library_scope.is_some();
    let cache_key = dashboard_cache_key(user_id, library_scope);

    // Try Redis cache first
    if let Ok(mut conn) = state.redis.get_multiplexed_async_connection().await {
        if let Ok(cached) = conn.get::<_, String>(&cache_key).await {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&cached) {
                return Ok(Json(val));
            }
        }
    }

    let total_books: i64 = if let Some(ids) = library_scope {
        sqlx::query_scalar(&count_books_sql(true))
            .bind(ids)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    } else {
        sqlx::query_scalar(&count_books_sql(false))
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    };
    let books_in_progress: i64 = if let Some(ids) = library_scope {
        sqlx::query_scalar(&count_books_by_reading_status_sql(true))
            .bind("reading")
            .bind(ids)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    } else {
        sqlx::query_scalar(&count_books_by_reading_status_sql(false))
            .bind("reading")
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    };
    let total_finished: i64 = if let Some(ids) = library_scope {
        sqlx::query_scalar(&count_books_by_reading_status_sql(true))
            .bind("completed")
            .bind(ids)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    } else {
        sqlx::query_scalar(&count_books_by_reading_status_sql(false))
            .bind("completed")
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    };
    let total_annotations: i64 = if let Some(ids) = library_scope {
        sqlx::query_scalar(&user_annotations_count_sql(true))
            .bind(user_id)
            .bind(ids)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    } else {
        sqlx::query_scalar(&user_annotations_count_sql(false))
            .bind(user_id)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    };
    let total_sessions: i64 = if let Some(ids) = library_scope {
        sqlx::query_scalar(&dashboard_session_count_sql(true))
            .bind(user_id)
            .bind(ids)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    } else {
        sqlx::query_scalar(&dashboard_session_count_sql(false))
            .bind(user_id)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    };

    // Reading time today (in minutes)
    let reading_time_today_secs: i64 = if let Some(ids) = library_scope {
        sqlx::query_scalar(&dashboard_reading_time_today_sql(true))
            .bind(user_id)
            .bind(ids)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    } else {
        sqlx::query_scalar(&dashboard_reading_time_today_sql(false))
            .bind(user_id)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    };

    // Tasks currently running
    let tasks_running: i64 = if let Some(ids) = library_scope {
        sqlx::query_scalar(&count_tasks_by_status_sql(scoped))
            .bind("running")
            .bind(ids)
            .fetch_one(&state.db)
            .await
            .unwrap_or(0)
    } else {
        sqlx::query_scalar(&count_tasks_by_status_sql(scoped))
            .bind("running")
            .fetch_one(&state.db)
            .await
            .unwrap_or(0)
    };

    // Storage used (in GB)
    let storage_bytes: i64 = if let Some(ids) = library_scope {
        sqlx::query_scalar(&storage_used_sql(true))
            .bind(ids)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    } else {
        sqlx::query_scalar(&storage_used_sql(false))
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    };
    let storage_used_gb = (storage_bytes as f64) / (1024.0 * 1024.0 * 1024.0);

    // Entities extracted
    let entities_extracted: i64 = if let Some(ids) = library_scope {
        sqlx::query_scalar(&count_entities_sql(true))
            .bind(ids)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    } else {
        sqlx::query_scalar(&count_entities_sql(false))
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    };

    let result = serde_json::json!({
        "total_books": total_books,
        "books_in_progress": books_in_progress,
        "currently_reading": books_in_progress,
        "finished": total_finished,
        "total_annotations": total_annotations,
        "total_sessions": total_sessions,
        "reading_time_today_mins": reading_time_today_secs / 60,
        "tasks_running": tasks_running,
        "storage_used_gb": format!("{:.2}", storage_used_gb).parse::<f64>().unwrap_or(0.0),
        "entities_extracted": entities_extracted,
    });

    // Write to cache (best-effort)
    if let Ok(mut conn) = state.redis.get_multiplexed_async_connection().await {
        let _ = conn
            .set_ex::<_, _, ()>(
                cache_key,
                serde_json::to_string(&result).unwrap_or_default(),
                DASHBOARD_CACHE_TTL,
            )
            .await;
    }

    Ok(Json(result))
}

#[derive(Deserialize)]
struct StatsQuery {
    days: Option<i32>,
    range: Option<String>,
}

async fn reading_stats(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(params): Query<StatsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let days = match params.range.as_deref() {
        Some("year") => 365,
        Some("month") => 30,
        Some("week") => 7,
        _ => params.days.unwrap_or(365),
    };

    let user_id = auth_user_id(&auth)?;
    let visible_libraries = visible_library_ids(&state, &auth, stats_library_access()).await?;
    if scoped_library_ids_empty(&visible_libraries) {
        return Ok(Json(empty_reading_stats(days)));
    }
    let library_scope = visible_libraries.as_deref();

    let sessions_count: i64 = if let Some(ids) = library_scope {
        sqlx::query_scalar(&reading_session_count_since_sql(true))
            .bind(user_id)
            .bind(days)
            .bind(ids)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    } else {
        sqlx::query_scalar(&reading_session_count_since_sql(false))
            .bind(user_id)
            .bind(days)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    };

    let total_duration: i64 = if let Some(ids) = library_scope {
        sqlx::query_scalar(&reading_session_sum_since_sql("duration_secs", true))
            .bind(user_id)
            .bind(days)
            .bind(ids)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    } else {
        sqlx::query_scalar(&reading_session_sum_since_sql("duration_secs", false))
            .bind(user_id)
            .bind(days)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    };

    let total_pages: i64 = if let Some(ids) = library_scope {
        sqlx::query_scalar(&reading_session_sum_since_sql("pages_read", true))
            .bind(user_id)
            .bind(days)
            .bind(ids)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    } else {
        sqlx::query_scalar(&reading_session_sum_since_sql("pages_read", false))
            .bind(user_id)
            .bind(days)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    };

    let total_words: i64 = if let Some(ids) = library_scope {
        sqlx::query_scalar(&reading_session_sum_since_sql("words_read", true))
            .bind(user_id)
            .bind(days)
            .bind(ids)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    } else {
        sqlx::query_scalar(&reading_session_sum_since_sql("words_read", false))
            .bind(user_id)
            .bind(days)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    };

    let total_books_read: i64 = if let Some(ids) = library_scope {
        sqlx::query_scalar(&user_completed_books_sql(true, false))
            .bind(user_id)
            .bind(ids)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    } else {
        sqlx::query_scalar(&user_completed_books_sql(false, false))
            .bind(user_id)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    };

    let total_annotations: i64 = if let Some(ids) = library_scope {
        sqlx::query_scalar(&user_annotations_count_sql(true))
            .bind(user_id)
            .bind(ids)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    } else {
        sqlx::query_scalar(&user_annotations_count_sql(false))
            .bind(user_id)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    };

    let books_this_month: i64 = if let Some(ids) = library_scope {
        sqlx::query_scalar(&user_completed_books_sql(true, true))
            .bind(user_id)
            .bind(ids)
            .fetch_one(&state.db)
            .await
            .unwrap_or(0)
    } else {
        sqlx::query_scalar(&user_completed_books_sql(false, true))
            .bind(user_id)
            .fetch_one(&state.db)
            .await
            .unwrap_or(0)
    };

    let pages_this_week: i64 = if let Some(ids) = library_scope {
        sqlx::query_scalar(&pages_this_week_sql(true))
            .bind(user_id)
            .bind(ids)
            .fetch_one(&state.db)
            .await
            .unwrap_or(0)
    } else {
        sqlx::query_scalar(&pages_this_week_sql(false))
            .bind(user_id)
            .fetch_one(&state.db)
            .await
            .unwrap_or(0)
    };

    // Calculate streak (consecutive days with reading sessions)
    let streak_days: Vec<chrono::NaiveDate> = if let Some(ids) = library_scope {
        sqlx::query_scalar(&streak_days_sql(true))
            .bind(user_id)
            .bind(ids)
            .fetch_all(&state.db)
            .await
            .unwrap_or_default()
    } else {
        sqlx::query_scalar(&streak_days_sql(false))
            .bind(user_id)
            .fetch_all(&state.db)
            .await
            .unwrap_or_default()
    };

    let current_streak = calculate_streak(&streak_days);
    let avg_daily_mins = if days > 0 {
        total_duration / 60 / days as i64
    } else {
        0
    };

    Ok(Json(serde_json::json!({
        // Fields the frontend expects
        "totalBooksRead": total_books_read,
        "totalReadingTime": total_duration,
        "totalWordsRead": total_words,
        "totalAnnotations": total_annotations,
        "avgDailyMinutes": avg_daily_mins,
        "longestStreak": current_streak,
        "currentStreak": current_streak,
        "booksThisMonth": books_this_month,
        "pagesThisWeek": pages_this_week,
        // Legacy fields
        "period_days": days,
        "sessions": sessions_count,
        "total_duration_secs": total_duration,
        "total_pages": total_pages,
        "total_words": total_words,
        "avg_session_mins": if sessions_count > 0 { total_duration / sessions_count / 60 } else { 0 },
    })))
}

/// Calculate current reading streak from sorted dates (most recent first).
fn calculate_streak(dates: &[chrono::NaiveDate]) -> i64 {
    if dates.is_empty() {
        return 0;
    }
    let today = chrono::Utc::now().date_naive();
    let mut streak = 0i64;
    let mut expected = today;

    for &day in dates {
        if day == expected {
            streak += 1;
            expected -= chrono::Duration::days(1);
        } else if day == expected - chrono::Duration::days(1) {
            // Allow one-day gap (started yesterday streak)
            expected = day;
            streak += 1;
            expected -= chrono::Duration::days(1);
        } else {
            break;
        }
    }
    streak
}

#[derive(sqlx::FromRow)]
struct HeatmapRow {
    day: chrono::NaiveDate,
    total_secs: i64,
    session_count: i64,
}

async fn reading_heatmap(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(params): Query<StatsQuery>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    let days = params.days.unwrap_or(365);
    let user_id = auth_user_id(&auth)?;
    let visible_libraries = visible_library_ids(&state, &auth, stats_library_access()).await?;
    if scoped_library_ids_empty(&visible_libraries) {
        return Ok(Json(Vec::new()));
    }
    let library_scope = visible_libraries.as_deref();

    let rows = if let Some(ids) = library_scope {
        sqlx::query_as::<_, HeatmapRow>(&reading_heatmap_sql(true))
            .bind(user_id)
            .bind(days)
            .bind(ids)
            .fetch_all(&state.db)
            .await
            .map_err(ApiError::from)?
    } else {
        sqlx::query_as::<_, HeatmapRow>(&reading_heatmap_sql(false))
            .bind(user_id)
            .bind(days)
            .fetch_all(&state.db)
            .await
            .map_err(ApiError::from)?
    };

    let result: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "date": r.day.to_string(),
                "total_secs": r.total_secs,
                "total_minutes": r.total_secs / 60,
                "total_words": 0,
                "sessions": r.session_count,
                "sessions_count": r.session_count,
            })
        })
        .collect();

    Ok(Json(result))
}

async fn library_stats(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let visible_libraries = visible_library_ids(&state, &auth, stats_library_access()).await?;
    if scoped_library_ids_empty(&visible_libraries) {
        return Ok(Json(empty_library_stats()));
    }
    let library_scope = visible_libraries.as_deref();

    let total_books: i64 = if let Some(ids) = library_scope {
        sqlx::query_scalar(&count_books_sql(true))
            .bind(ids)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    } else {
        sqlx::query_scalar(&count_books_sql(false))
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    };
    let total_words: i64 = if let Some(ids) = library_scope {
        sqlx::query_scalar(&library_words_sql(true))
            .bind(ids)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    } else {
        sqlx::query_scalar(&library_words_sql(false))
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    };
    let total_size: i64 = if let Some(ids) = library_scope {
        sqlx::query_scalar(&storage_used_sql(true))
            .bind(ids)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    } else {
        sqlx::query_scalar(&storage_used_sql(false))
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    };
    let total_libraries: i64 = if let Some(ids) = library_scope {
        sqlx::query_scalar(&library_count_sql(true))
            .bind(ids)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    } else {
        sqlx::query_scalar(&library_count_sql(false))
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    };
    let total_persons: i64 = if let Some(ids) = library_scope {
        sqlx::query_scalar(&library_person_count_sql(true))
            .bind(ids)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    } else {
        sqlx::query_scalar(&library_person_count_sql(false))
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    };

    Ok(Json(serde_json::json!({
        "total_books": total_books,
        "total_words": total_words,
        "total_size_bytes": total_size,
        "total_libraries": total_libraries,
        "total_persons": total_persons,
    })))
}

// ─── Reading Sessions List ─────────────────────────────────────

#[derive(Deserialize)]
struct SessionsQuery {
    #[serde(default = "default_sessions_limit")]
    limit: i64,
}

fn default_sessions_limit() -> i64 {
    20
}

async fn reading_sessions(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(params): Query<SessionsQuery>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    #[derive(sqlx::FromRow)]
    struct SessionRow {
        id: uuid::Uuid,
        book_id: uuid::Uuid,
        duration_secs: i64,
        words_read: i64,
        started_at: chrono::DateTime<chrono::Utc>,
        book_title: Option<String>,
    }

    let user_id = auth_user_id(&auth)?;
    let visible_libraries = visible_library_ids(&state, &auth, stats_library_access()).await?;
    if scoped_library_ids_empty(&visible_libraries) {
        return Ok(Json(Vec::new()));
    }
    let library_scope = visible_libraries.as_deref();
    let limit = params.limit.clamp(1, 100);

    let sessions = if let Some(ids) = library_scope {
        sqlx::query_as::<_, SessionRow>(&reading_sessions_sql(true))
            .bind(user_id)
            .bind(ids)
            .bind(limit)
            .fetch_all(&state.db)
            .await
            .map_err(ApiError::from)?
    } else {
        sqlx::query_as::<_, SessionRow>(&reading_sessions_sql(false))
            .bind(user_id)
            .bind(limit)
            .fetch_all(&state.db)
            .await
            .map_err(ApiError::from)?
    };

    let result: Vec<serde_json::Value> = sessions
        .into_iter()
        .map(|s| {
            serde_json::json!({
                "id": s.id,
                "book_id": s.book_id,
                "book_title": s.book_title.unwrap_or_default(),
                "start_chapter": 0,
                "end_chapter": 0,
                "words_read": s.words_read,
                "duration_secs": s.duration_secs,
                "started_at": s.started_at,
            })
        })
        .collect();

    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
struct MemoriesQuery {
    month: u32,
    day: u32,
    #[serde(default)]
    limit: Option<i64>,
}

/// Books read on the same calendar day in previous years.
async fn reading_memories(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(params): Query<MemoriesQuery>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    if !(1..=12).contains(&params.month) || !(1..=31).contains(&params.day) {
        return Err(ApiError::bad_request("month/day out of range"));
    }

    let user_id = auth_user_id(&auth)?;
    let visible_libraries = visible_library_ids(&state, &auth, stats_library_access()).await?;
    if scoped_library_ids_empty(&visible_libraries) {
        return Ok(Json(Vec::new()));
    }
    let library_scope = visible_libraries.as_deref();

    #[derive(sqlx::FromRow)]
    struct MemoryRow {
        book_id: uuid::Uuid,
        title: String,
        author: Option<String>,
        cover_path: Option<String>,
        read_date: chrono::DateTime<chrono::Utc>,
    }

    let limit = params.limit.unwrap_or(6).clamp(1, 20);
    let rows = if let Some(ids) = library_scope {
        sqlx::query_as::<_, MemoryRow>(&reading_memories_sql(true))
            .bind(user_id)
            .bind(params.month as i32)
            .bind(params.day as i32)
            .bind(ids)
            .bind(limit)
            .fetch_all(&state.db)
            .await
            .map_err(ApiError::from)?
    } else {
        sqlx::query_as::<_, MemoryRow>(&reading_memories_sql(false))
            .bind(user_id)
            .bind(params.month as i32)
            .bind(params.day as i32)
            .bind(limit)
            .fetch_all(&state.db)
            .await
            .map_err(ApiError::from)?
    };

    let current_year = chrono::Utc::now().date_naive().year();
    let result: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|row| {
            let read_year = row.read_date.date_naive().year();
            serde_json::json!({
                "book_id": row.book_id,
                "title": row.title,
                "author": row.author,
                "cover_path": row.cover_path,
                "read_date": row.read_date,
                "years_ago": current_year - read_year,
            })
        })
        .collect();

    Ok(Json(result))
}

// ─── Reading Goals ─────────────────────────────────────────────

async fn reading_goals(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    let user_id = auth_user_id(&auth)?;
    let visible_libraries = visible_library_ids(&state, &auth, stats_library_access()).await?;
    let library_scope = visible_libraries.as_deref();

    if scoped_library_ids_empty(&visible_libraries) {
        return Ok(Json(vec![
            serde_json::json!({
                "id": "annual-books",
                "label": "年度阅读目标",
                "goal_type": "books_read",
                "target": 24,
                "progress": 0,
                "period": "year",
            }),
            serde_json::json!({
                "id": "weekly-sessions",
                "label": "每周阅读天数",
                "goal_type": "sessions",
                "target": 5,
                "progress": 0,
                "period": "week",
            }),
        ]));
    }

    // Default goals based on actual data (reading_goals table may not exist yet)
    let books_this_year = if let Some(ids) = library_scope {
        sqlx::query_scalar(&reading_goal_books_this_year_sql(true))
            .bind(user_id)
            .bind(ids)
            .fetch_one(&state.db)
            .await
            .unwrap_or(0)
    } else {
        sqlx::query_scalar(&reading_goal_books_this_year_sql(false))
            .bind(user_id)
            .fetch_one(&state.db)
            .await
            .unwrap_or(0)
    };

    let sessions_this_week: i64 = if let Some(ids) = library_scope {
        sqlx::query_scalar(&reading_goal_sessions_this_week_sql(true))
            .bind(user_id)
            .bind(ids)
            .fetch_one(&state.db)
            .await
            .unwrap_or(0)
    } else {
        sqlx::query_scalar(&reading_goal_sessions_this_week_sql(false))
            .bind(user_id)
            .fetch_one(&state.db)
            .await
            .unwrap_or(0)
    };

    Ok(Json(vec![
        serde_json::json!({
            "id": "annual-books",
            "label": "年度阅读目标",
            "goal_type": "books_read",
            "target": 24,
            "progress": books_this_year,
            "period": "year",
        }),
        serde_json::json!({
            "id": "weekly-sessions",
            "label": "每周阅读天数",
            "goal_type": "sessions",
            "target": 5,
            "progress": sessions_this_week,
            "period": "week",
        }),
    ]))
}

// ─── Activities Feed ───────────────────────────────────────────

#[derive(Deserialize)]
struct ActivitiesQuery {
    #[serde(default = "default_activities_limit")]
    limit: i64,
    #[serde(default)]
    offset: Option<i64>,
}

fn default_activities_limit() -> i64 {
    50
}

async fn activities(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(params): Query<ActivitiesQuery>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    let limit = params.limit.clamp(1, 100);
    let offset = params.offset.unwrap_or(0).max(0);
    let user_id = auth_user_id(&auth)?;
    let visible_libraries = visible_library_ids(&state, &auth, stats_library_access()).await?;
    if scoped_library_ids_empty(&visible_libraries) {
        return Ok(Json(Vec::new()));
    }
    let library_scope = visible_libraries.as_deref();

    // Combine reading sessions + annotations + bookmarks as activity feed
    #[derive(sqlx::FromRow)]
    struct ActivityRow {
        id: uuid::Uuid,
        activity_type: String,
        book_title: String,
        book_id: uuid::Uuid,
        description: String,
        created_at: chrono::DateTime<chrono::Utc>,
        duration_minutes: Option<i64>,
        pages_read: Option<i32>,
    }

    let activities = if let Some(ids) = library_scope {
        sqlx::query_as::<_, ActivityRow>(&activities_sql(true))
            .bind(user_id)
            .bind(ids)
            .bind(limit)
            .bind(offset)
            .fetch_all(&state.db)
            .await
            .map_err(ApiError::from)?
    } else {
        sqlx::query_as::<_, ActivityRow>(&activities_sql(false))
            .bind(user_id)
            .bind(limit)
            .bind(offset)
            .fetch_all(&state.db)
            .await
            .map_err(ApiError::from)?
    };

    let result: Vec<serde_json::Value> = activities
        .into_iter()
        .map(|a| {
            serde_json::json!({
                "id": a.id,
                "type": a.activity_type,
                "book_title": a.book_title,
                "book_id": a.book_id,
                "description": a.description,
                "created_at": a.created_at,
                "duration_minutes": a.duration_minutes,
                "pages_read": a.pages_read,
            })
        })
        .collect();

    Ok(Json(result))
}

/// Record a new reading session from the reader.
#[derive(serde::Deserialize)]
struct RecordSessionRequest {
    book_id: uuid::Uuid,
    started_at: Option<chrono::DateTime<chrono::Utc>>,
    duration_secs: Option<i32>,
    pages_read: Option<i32>,
    words_read: Option<i64>,
    start_chapter: Option<i32>,
    end_chapter: Option<i32>,
}

async fn record_reading_session(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<RecordSessionRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    ensure_book_access(&state, &auth, body.book_id, stats_library_access()).await?;
    let user_id = auth_user_id(&auth)?;
    let id = uuid::Uuid::now_v7();
    let started = body.started_at.unwrap_or_else(chrono::Utc::now);

    sqlx::query(record_session_insert_sql())
        .bind(id)
        .bind(user_id)
        .bind(body.book_id)
        .bind(started)
        .bind(body.duration_secs.unwrap_or(0))
        .bind(body.pages_read.unwrap_or(0))
        .bind(body.words_read.unwrap_or(0))
        .bind(body.start_chapter)
        .bind(body.end_chapter)
        .execute(&state.db)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(
        serde_json::json!({ "id": id, "book_id": body.book_id }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_reader_artifact_queries_are_scoped_to_user_and_visible_libraries() {
        assert!(record_session_insert_sql().contains("id, user_id, book_id"));
        assert!(reading_sessions_sql(true).contains("rs.user_id = $1"));
        assert!(reading_sessions_sql(true).contains("b.library_id = ANY($2::uuid[])"));
        assert!(reading_memories_sql(true).contains("rs.user_id = $1"));
        assert!(reading_memories_sql(true).contains("b.library_id = ANY($4::uuid[])"));

        let activities = activities_sql(true);
        assert!(activities.contains("rs.user_id = $1"));
        assert!(activities.contains("a.user_id = $1"));
        assert!(activities.contains("bm.user_id = $1"));
        assert_eq!(
            activities.matches("b.library_id = ANY($2::uuid[])").count(),
            3
        );
    }

    #[test]
    fn personal_stats_queries_scope_to_user_and_visible_libraries() {
        assert!(reading_session_count_since_sql(true).contains("rs.user_id = $1"));
        assert!(reading_session_count_since_sql(true).contains("b.library_id = ANY($3::uuid[])"));
        assert!(reading_heatmap_sql(true).contains("rs.user_id = $1"));
        assert!(reading_heatmap_sql(true).contains("b.library_id = ANY($3::uuid[])"));
        assert!(user_completed_books_sql(true, false).contains("rp.user_id = $1"));
        assert!(user_completed_books_sql(true, false).contains("b.library_id = ANY($2::uuid[])"));
        assert!(user_annotations_count_sql(true).contains("a.user_id = $1"));
        assert!(user_annotations_count_sql(true).contains("b.library_id = ANY($2::uuid[])"));
        assert!(reading_goal_books_this_year_sql(true).contains("rp.user_id = $1"));
        assert!(reading_goal_books_this_year_sql(true).contains("b.library_id = ANY($2::uuid[])"));
        assert!(reading_goal_sessions_this_week_sql(true).contains("rs.user_id = $1"));
        assert!(
            reading_goal_sessions_this_week_sql(true).contains("b.library_id = ANY($2::uuid[])")
        );
    }

    #[test]
    fn global_stats_queries_can_be_limited_to_visible_libraries() {
        assert!(count_books_sql(true).contains("b.library_id = ANY($1::uuid[])"));
        assert!(count_annotations_sql(true, true).contains("a.user_id = $1"));
        assert!(count_annotations_sql(true, true).contains("b.library_id = ANY($2::uuid[])"));
        assert!(count_entities_sql(true).contains("JOIN books b ON b.id = e.book_id"));
        assert!(count_entities_sql(true).contains("b.library_id = ANY($1::uuid[])"));
        assert!(library_person_count_sql(true).contains("b.library_id = ANY($1::uuid[])"));
    }

    #[test]
    fn dashboard_cache_key_is_partitioned_by_user_and_library_scope() {
        let user_id = Uuid::parse_str("018f0000-0000-7000-8000-000000000001").unwrap();
        let library_a = Uuid::parse_str("018f0000-0000-7000-8000-00000000000a").unwrap();
        let library_b = Uuid::parse_str("018f0000-0000-7000-8000-00000000000b").unwrap();

        assert_eq!(
            dashboard_cache_key(user_id, None),
            "nova:cache:dashboard:user:018f0000-0000-7000-8000-000000000001:libraries:all"
        );
        assert_eq!(
            dashboard_cache_key(user_id, Some(&[library_b, library_a])),
            "nova:cache:dashboard:user:018f0000-0000-7000-8000-000000000001:libraries:018f0000-0000-7000-8000-00000000000a,018f0000-0000-7000-8000-00000000000b"
        );
        assert_eq!(
            dashboard_cache_key(user_id, Some(&[])),
            "nova:cache:dashboard:user:018f0000-0000-7000-8000-000000000001:libraries:none"
        );
    }
}
