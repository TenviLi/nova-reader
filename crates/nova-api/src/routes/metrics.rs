//! Prometheus metrics endpoint for monitoring.
//! Exposes request latency, queue depth, AI cost, active connections, etc.
//! Compatible with Prometheus text exposition format (OpenMetrics).

use axum::{
    extract::State,
    routing::get,
    Router,
};
use std::sync::Arc;

use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/", get(metrics_handler))
}

async fn metrics_handler(State(state): State<Arc<AppState>>) -> String {
    let mut output = String::with_capacity(4096);

    // ─── Database Pool ────────────────────────────────────────────────────────
    let pool = &state.db;
    let pool_size = pool.size();
    let idle_connections = pool.num_idle();

    output.push_str("# HELP nova_db_pool_size Total connections in pool\n");
    output.push_str("# TYPE nova_db_pool_size gauge\n");
    output.push_str(&format!("nova_db_pool_size {}\n\n", pool_size));

    output.push_str("# HELP nova_db_pool_idle Idle connections available\n");
    output.push_str("# TYPE nova_db_pool_idle gauge\n");
    output.push_str(&format!("nova_db_pool_idle {}\n\n", idle_connections));

    output.push_str("# HELP nova_db_pool_active Active connections in use\n");
    output.push_str("# TYPE nova_db_pool_active gauge\n");
    output.push_str(&format!("nova_db_pool_active {}\n\n", pool_size - idle_connections as u32));

    // ─── WebSocket ────────────────────────────────────────────────────────────
    let ws_subscribers = state.progress_broadcast.receiver_count();
    output.push_str("# HELP nova_ws_connections Active WebSocket connections\n");
    output.push_str("# TYPE nova_ws_connections gauge\n");
    output.push_str(&format!("nova_ws_connections {}\n\n", ws_subscribers));

    // ─── Task Queue ───────────────────────────────────────────────────────────
    if let Ok(rows) = sqlx::query!(
        r#"SELECT status, COUNT(*) as count FROM tasks GROUP BY status"#
    )
    .fetch_all(&state.db)
    .await
    {
        output.push_str("# HELP nova_tasks_total Tasks by status\n");
        output.push_str("# TYPE nova_tasks_total gauge\n");
        for row in &rows {
            let status = row.status.as_deref().unwrap_or("unknown");
            let count = row.count.unwrap_or(0);
            output.push_str(&format!("nova_tasks_total{{status=\"{}\"}} {}\n", status, count));
        }
        output.push('\n');
    }

    // ─── Content Metrics ──────────────────────────────────────────────────────
    if let Ok(row) = sqlx::query!(
        r#"SELECT 
            COUNT(*) as book_count,
            COALESCE(SUM(word_count), 0) as total_words,
            COALESCE(SUM(file_size_bytes), 0) as total_storage
           FROM books WHERE status != 'archived'"#
    )
    .fetch_one(&state.db)
    .await
    {
        output.push_str("# HELP nova_books_total Total books in library\n");
        output.push_str("# TYPE nova_books_total gauge\n");
        output.push_str(&format!("nova_books_total {}\n", row.book_count.unwrap_or(0)));

        output.push_str("# HELP nova_words_total Total words across all books\n");
        output.push_str("# TYPE nova_words_total gauge\n");
        output.push_str(&format!("nova_words_total {}\n", row.total_words.unwrap_or(0)));

        output.push_str("# HELP nova_storage_bytes Total book file storage in bytes\n");
        output.push_str("# TYPE nova_storage_bytes gauge\n");
        output.push_str(&format!("nova_storage_bytes {}\n\n", row.total_storage.unwrap_or(0)));
    }

    if let Ok(row) = sqlx::query!("SELECT COUNT(*) as count FROM chapters")
        .fetch_one(&state.db)
        .await
    {
        output.push_str("# HELP nova_chapters_total Total chapters\n");
        output.push_str("# TYPE nova_chapters_total gauge\n");
        output.push_str(&format!("nova_chapters_total {}\n\n", row.count.unwrap_or(0)));
    }

    // ─── User Metrics ─────────────────────────────────────────────────────────
    if let Ok(row) = sqlx::query!("SELECT COUNT(*) as count FROM users")
        .fetch_one(&state.db)
        .await
    {
        output.push_str("# HELP nova_users_total Registered users\n");
        output.push_str("# TYPE nova_users_total gauge\n");
        output.push_str(&format!("nova_users_total {}\n\n", row.count.unwrap_or(0)));
    }

    // ─── AI Usage Metrics ─────────────────────────────────────────────────────
    if let Ok(rows) = sqlx::query!(
        r#"SELECT 
            endpoint,
            COUNT(*) as request_count,
            COALESCE(SUM(tokens_used), 0) as total_tokens,
            COALESCE(AVG(duration_ms), 0) as avg_latency_ms
           FROM ai_usage_logs
           WHERE created_at > now() - interval '1 hour'
           GROUP BY endpoint"#
    )
    .fetch_all(&state.db)
    .await
    {
        output.push_str("# HELP nova_ai_requests_1h AI requests in the last hour\n");
        output.push_str("# TYPE nova_ai_requests_1h gauge\n");
        for row in &rows {
            let endpoint = row.endpoint.as_deref().unwrap_or("unknown");
            output.push_str(&format!(
                "nova_ai_requests_1h{{endpoint=\"{}\"}} {}\n",
                endpoint, row.request_count.unwrap_or(0)
            ));
        }
        output.push('\n');

        output.push_str("# HELP nova_ai_tokens_1h AI tokens consumed in the last hour\n");
        output.push_str("# TYPE nova_ai_tokens_1h gauge\n");
        for row in &rows {
            let endpoint = row.endpoint.as_deref().unwrap_or("unknown");
            output.push_str(&format!(
                "nova_ai_tokens_1h{{endpoint=\"{}\"}} {}\n",
                endpoint, row.total_tokens.unwrap_or(0)
            ));
        }
        output.push('\n');

        output.push_str("# HELP nova_ai_latency_ms_avg Average AI request latency (ms) in the last hour\n");
        output.push_str("# TYPE nova_ai_latency_ms_avg gauge\n");
        for row in &rows {
            let endpoint = row.endpoint.as_deref().unwrap_or("unknown");
            output.push_str(&format!(
                "nova_ai_latency_ms_avg{{endpoint=\"{}\"}} {:.1}\n",
                endpoint, row.avg_latency_ms.unwrap_or(0.0)
            ));
        }
        output.push('\n');
    }

    // ─── Search Metrics ───────────────────────────────────────────────────────
    if let Ok(row) = sqlx::query!(
        r#"SELECT COUNT(*) as count FROM search_history WHERE searched_at > now() - interval '1 hour'"#
    )
    .fetch_all(&state.db)
    .await
    {
        let count: i64 = row.first().and_then(|r| r.count).unwrap_or(0);
        output.push_str("# HELP nova_search_requests_1h Search queries in the last hour\n");
        output.push_str("# TYPE nova_search_requests_1h gauge\n");
        output.push_str(&format!("nova_search_requests_1h {}\n\n", count));
    }

    // ─── Import Sources ───────────────────────────────────────────────────────
    if let Ok(rows) = sqlx::query!(
        r#"SELECT status, COUNT(*) as count FROM import_sources GROUP BY status"#
    )
    .fetch_all(&state.db)
    .await
    {
        output.push_str("# HELP nova_import_sources Import sources by status\n");
        output.push_str("# TYPE nova_import_sources gauge\n");
        for row in &rows {
            let status = row.status.as_deref().unwrap_or("unknown");
            output.push_str(&format!("nova_import_sources{{status=\"{}\"}} {}\n", status, row.count.unwrap_or(0)));
        }
        output.push('\n');
    }

    // ─── Plugin Metrics ───────────────────────────────────────────────────────
    if let Ok(row) = sqlx::query!(
        "SELECT COUNT(*) FILTER (WHERE enabled) as active, COUNT(*) as total FROM plugins"
    )
    .fetch_one(&state.db)
    .await
    {
        output.push_str("# HELP nova_plugins_active Enabled plugins\n");
        output.push_str("# TYPE nova_plugins_active gauge\n");
        output.push_str(&format!("nova_plugins_active {}\n", row.active.unwrap_or(0)));
        output.push_str("# HELP nova_plugins_total Installed plugins\n");
        output.push_str("# TYPE nova_plugins_total gauge\n");
        output.push_str(&format!("nova_plugins_total {}\n\n", row.total.unwrap_or(0)));
    }

    // ─── System Info ──────────────────────────────────────────────────────────
    output.push_str("# HELP nova_info Build information\n");
    output.push_str("# TYPE nova_info gauge\n");
    output.push_str("nova_info{version=\"0.1.0\",rust_version=\"1.82\"} 1\n");

    output
}
