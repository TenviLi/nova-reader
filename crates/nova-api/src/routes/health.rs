use std::sync::Arc;

use axum::{extract::State, routing::get, Json, Router};
use serde_json::{json, Value};

use crate::state::AppState;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/health", get(health_check))
        .route("/health/ready", get(readiness_check))
        .route("/health/setup-status", get(setup_status))
}

/// Start time for uptime calculation.
static START_TIME: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

pub fn init_start_time() {
    START_TIME.get_or_init(std::time::Instant::now);
}

/// Basic liveness probe — includes service health for admin panel.
async fn health_check(State(state): State<Arc<AppState>>) -> Json<Value> {
    let db_ok = sqlx::query("SELECT 1").fetch_one(&state.db).await.is_ok();

    let redis_ok = state.redis.get_multiplexed_async_connection().await.is_ok();

    // Meilisearch check via HTTP client
    let meilisearch_ok = state
        .http_client
        .get("http://localhost:7700/health")
        .timeout(std::time::Duration::from_secs(2))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false);

    // Qdrant check via HTTP client
    let qdrant_ok = state
        .http_client
        .get(format!("{}/healthz", state.config.qdrant_url))
        .timeout(std::time::Duration::from_secs(2))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false);

    let uptime = START_TIME.get().map(|t| t.elapsed().as_secs()).unwrap_or(0);

    Json(json!({
        "status": if db_ok && redis_ok { "ok" } else { "degraded" },
        "service": "nova-reader",
        "version": env!("CARGO_PKG_VERSION"),
        "database": db_ok,
        "redis": redis_ok,
        "qdrant": qdrant_ok,
        "meilisearch": meilisearch_ok,
        "uptime_seconds": uptime,
    }))
}

/// Readiness probe — checks all dependent services.
async fn readiness_check(State(state): State<Arc<AppState>>) -> Json<Value> {
    let db_ok = sqlx::query("SELECT 1").fetch_one(&state.db).await.is_ok();

    let redis_ok = {
        let result: Result<redis::aio::MultiplexedConnection, _> =
            state.redis.get_multiplexed_async_connection().await;
        result.is_ok()
    };

    let all_ok = db_ok && redis_ok;

    Json(json!({
        "status": if all_ok { "ready" } else { "degraded" },
        "checks": {
            "postgres": if db_ok { "up" } else { "down" },
            "redis": if redis_ok { "up" } else { "down" },
        }
    }))
}

/// Check if initial setup has been completed (any users exist).
async fn setup_status(State(state): State<Arc<AppState>>) -> Json<Value> {
    let has_users = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users")
        .fetch_one(&state.db)
        .await
        .unwrap_or(0)
        > 0;

    Json(json!({
        "needs_setup": !has_users,
        "initialized": has_users,
    }))
}
