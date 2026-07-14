use std::sync::Arc;

use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::{
    ai_usage::AiUsageTracker,
    error::ApiError,
    extractors::{AdminUser, AuthUser},
    state::AppState,
};

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/me", get(my_usage_summary))
        .route("/summary", get(usage_summary))
        .route("/daily", get(daily_breakdown))
        .route("/operations", get(operation_breakdown))
        .route("/recent", get(recent_usage_logs))
}

async fn my_usage_summary(
    auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Query(query): Query<UsageQuery>,
) -> Result<Json<UsageSummaryResponse>, ApiError> {
    let days = query.days.unwrap_or(30);
    let user_id = uuid::Uuid::parse_str(&auth.id).map_err(|_| ApiError::unauthorized())?;
    let tracker = AiUsageTracker::new(state.db.clone());
    let summary = tracker
        .get_user_summary(user_id, days)
        .await
        .map_err(ApiError::from)?;

    let error_rate = if summary.request_count > 0 {
        summary.error_count as f64 / summary.request_count as f64
    } else {
        0.0
    };

    Ok(Json(UsageSummaryResponse {
        request_count: summary.request_count,
        total_prompt_tokens: summary.total_prompt_tokens,
        total_completion_tokens: summary.total_completion_tokens,
        total_tokens: summary.total_tokens,
        total_cost_cents: summary.total_cost_cents,
        avg_latency_ms: summary.avg_latency_ms,
        error_count: summary.error_count,
        error_rate,
    }))
}

#[derive(Deserialize)]
struct UsageQuery {
    days: Option<i32>,
    limit: Option<i64>,
}

#[derive(Serialize)]
struct UsageSummaryResponse {
    request_count: i32,
    total_prompt_tokens: i32,
    total_completion_tokens: i32,
    total_tokens: i32,
    total_cost_cents: f64,
    avg_latency_ms: i32,
    error_count: i32,
    error_rate: f64,
}

#[derive(Serialize, sqlx::FromRow)]
struct RecentUsageLog {
    id: uuid::Uuid,
    operation: String,
    model: String,
    provider: String,
    total_tokens: i32,
    cost_cents: f64,
    latency_ms: i32,
    request_summary: Option<String>,
    success: bool,
    error_message: Option<String>,
    username: Option<String>,
    book_title: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
}

async fn usage_summary(
    _admin: AdminUser,
    State(state): State<Arc<AppState>>,
    Query(query): Query<UsageQuery>,
) -> Result<Json<UsageSummaryResponse>, ApiError> {
    let days = query.days.unwrap_or(30);

    // Get global summary (no user filter for admin view)
    let row = sqlx::query_as::<_, crate::ai_usage::UsageSummaryRow>(
        r#"SELECT
            COUNT(*)::int4 AS request_count,
            COALESCE(SUM(prompt_tokens), 0)::int4 AS total_prompt_tokens,
            COALESCE(SUM(completion_tokens), 0)::int4 AS total_completion_tokens,
            COALESCE(SUM(total_tokens), 0)::int4 AS total_tokens_sum,
            COALESCE(SUM(cost_cents), 0.0) AS total_cost_cents,
            COALESCE(AVG(latency_ms), 0)::int4 AS avg_latency_ms,
            COUNT(*) FILTER (WHERE NOT success)::int4 AS error_count
        FROM ai_usage_logs
        WHERE created_at >= NOW() - ($1 || ' days')::interval"#,
    )
    .bind(days.to_string())
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::from)?;

    let error_rate = if row.request_count > 0 {
        row.error_count as f64 / row.request_count as f64
    } else {
        0.0
    };

    Ok(Json(UsageSummaryResponse {
        request_count: row.request_count,
        total_prompt_tokens: row.total_prompt_tokens,
        total_completion_tokens: row.total_completion_tokens,
        total_tokens: row.total_tokens_sum,
        total_cost_cents: row.total_cost_cents,
        avg_latency_ms: row.avg_latency_ms,
        error_count: row.error_count,
        error_rate,
    }))
}

async fn daily_breakdown(
    _admin: AdminUser,
    State(state): State<Arc<AppState>>,
    Query(query): Query<UsageQuery>,
) -> Result<Json<Vec<crate::ai_usage::DailyUsage>>, ApiError> {
    let days = query.days.unwrap_or(30);
    let tracker = AiUsageTracker::new(state.db.clone());
    let data = tracker
        .get_daily_breakdown(days)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(data))
}

async fn operation_breakdown(
    _admin: AdminUser,
    State(state): State<Arc<AppState>>,
    Query(query): Query<UsageQuery>,
) -> Result<Json<Vec<crate::ai_usage::OperationUsage>>, ApiError> {
    let days = query.days.unwrap_or(30);
    let tracker = AiUsageTracker::new(state.db.clone());
    let data = tracker
        .get_operation_breakdown(days)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(data))
}

async fn recent_usage_logs(
    _admin: AdminUser,
    State(state): State<Arc<AppState>>,
    Query(query): Query<UsageQuery>,
) -> Result<Json<Vec<RecentUsageLog>>, ApiError> {
    let days = query.days.unwrap_or(30);
    let limit = query.limit.unwrap_or(50).clamp(1, 200);

    let logs = sqlx::query_as::<_, RecentUsageLog>(
        r#"
        SELECT l.id, l.operation, l.model, l.provider, l.total_tokens, l.cost_cents,
               l.latency_ms, l.request_summary, l.success, l.error_message,
               u.username, b.title as book_title, l.created_at
        FROM ai_usage_logs l
        LEFT JOIN users u ON u.id = l.user_id
        LEFT JOIN books b ON b.id = l.book_id
        WHERE l.created_at >= NOW() - ($1 || ' days')::interval
        ORDER BY l.created_at DESC
        LIMIT $2
        "#,
    )
    .bind(days.to_string())
    .bind(limit)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(logs))
}

#[cfg(test)]
mod tests {
    fn production_source() -> &'static str {
        include_str!("ai_usage.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("production source should exist")
    }

    #[test]
    fn ai_usage_routes_are_admin_only() {
        let source = production_source();

        assert!(source.contains("AdminUser"));
        for handler in [
            "usage_summary",
            "daily_breakdown",
            "operation_breakdown",
            "recent_usage_logs",
        ] {
            let marker = format!("async fn {handler}(");
            let start = source
                .find(&marker)
                .unwrap_or_else(|| panic!("{handler} should exist"));
            let body = &source[start..source[start..].find(") ->").map(|idx| start + idx).unwrap()];
            assert!(body.contains("_admin: AdminUser"));
        }
    }
}
