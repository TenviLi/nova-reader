//! AI usage tracking — logs all LLM API calls with token counts, latency, and cost estimation.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

/// Token pricing table (USD per 1M tokens, as of 2026)
const DEEPSEEK_CHAT_INPUT_PRICE: f64 = 0.14; // $0.14/1M input tokens
const DEEPSEEK_CHAT_OUTPUT_PRICE: f64 = 0.28; // $0.28/1M output tokens
const DEEPSEEK_REASONER_INPUT_PRICE: f64 = 0.55;
const DEEPSEEK_REASONER_OUTPUT_PRICE: f64 = 2.19;

/// Records a single AI API call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiUsageRecord {
    pub user_id: Option<Uuid>,
    pub book_id: Option<Uuid>,
    pub operation: String,
    pub model: String,
    pub provider: String,
    pub prompt_tokens: i32,
    pub completion_tokens: i32,
    pub total_tokens: i32,
    pub latency_ms: i32,
    pub request_summary: Option<String>,
    pub success: bool,
    pub error_message: Option<String>,
    pub metadata: serde_json::Value,
}

/// Persists AI usage records to the database.
pub struct AiUsageTracker {
    db: PgPool,
}

impl AiUsageTracker {
    pub fn new(db: PgPool) -> Self {
        Self { db }
    }

    /// Log a completed AI API call.
    pub async fn log(&self, record: AiUsageRecord) {
        let cost = estimate_cost(
            &record.model,
            record.prompt_tokens,
            record.completion_tokens,
        );

        let result = sqlx::query(
            r#"INSERT INTO ai_usage_logs
               (user_id, book_id, operation, model, provider, prompt_tokens, completion_tokens, total_tokens, cost_cents, latency_ms, request_summary, success, error_message, metadata)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)"#,
        )
        .bind(record.user_id)
        .bind(record.book_id)
        .bind(&record.operation)
        .bind(&record.model)
        .bind(&record.provider)
        .bind(record.prompt_tokens)
        .bind(record.completion_tokens)
        .bind(record.total_tokens)
        .bind(cost)
        .bind(record.latency_ms)
        .bind(&record.request_summary)
        .bind(record.success)
        .bind(&record.error_message)
        .bind(&record.metadata)
        .execute(&self.db)
        .await;

        if let Err(e) = result {
            tracing::warn!(error = %e, "Failed to log AI usage");
        }
    }

    /// Get usage summary for a user within a date range.
    pub async fn get_user_summary(
        &self,
        user_id: Uuid,
        days: i32,
    ) -> Result<UsageSummary, sqlx::Error> {
        let row = sqlx::query_as::<_, UsageSummaryRow>(
            r#"SELECT
                COUNT(*)::int4 AS request_count,
                COALESCE(SUM(prompt_tokens), 0)::int4 AS total_prompt_tokens,
                COALESCE(SUM(completion_tokens), 0)::int4 AS total_completion_tokens,
                COALESCE(SUM(total_tokens), 0)::int4 AS total_tokens_sum,
                COALESCE(SUM(cost_cents), 0.0) AS total_cost_cents,
                COALESCE(AVG(latency_ms), 0)::int4 AS avg_latency_ms,
                COUNT(*) FILTER (WHERE NOT success)::int4 AS error_count
            FROM ai_usage_logs
            WHERE user_id = $1 AND created_at >= NOW() - ($2 || ' days')::interval"#,
        )
        .bind(user_id)
        .bind(days.to_string())
        .fetch_one(&self.db)
        .await?;

        Ok(UsageSummary {
            request_count: row.request_count,
            total_prompt_tokens: row.total_prompt_tokens,
            total_completion_tokens: row.total_completion_tokens,
            total_tokens: row.total_tokens_sum,
            total_cost_cents: row.total_cost_cents,
            avg_latency_ms: row.avg_latency_ms,
            error_count: row.error_count,
        })
    }

    /// Get daily breakdown for charts.
    pub async fn get_daily_breakdown(&self, days: i32) -> Result<Vec<DailyUsage>, sqlx::Error> {
        let rows = sqlx::query_as::<_, DailyUsageRow>(
            r#"SELECT
                DATE(created_at) AS date,
                COUNT(*)::int4 AS requests,
                COALESCE(SUM(total_tokens), 0)::int4 AS tokens,
                COALESCE(SUM(cost_cents), 0.0) AS cost_cents
            FROM ai_usage_logs
            WHERE created_at >= NOW() - ($1 || ' days')::interval
            GROUP BY DATE(created_at)
            ORDER BY date DESC"#,
        )
        .bind(days.to_string())
        .fetch_all(&self.db)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| DailyUsage {
                date: r.date.to_string(),
                requests: r.requests,
                tokens: r.tokens,
                cost_cents: r.cost_cents,
            })
            .collect())
    }

    /// Get per-operation breakdown.
    pub async fn get_operation_breakdown(
        &self,
        days: i32,
    ) -> Result<Vec<OperationUsage>, sqlx::Error> {
        let rows = sqlx::query_as::<_, OperationUsageRow>(
            r#"SELECT
                operation,
                COUNT(*)::int4 AS count,
                COALESCE(SUM(total_tokens), 0)::int4 AS tokens,
                COALESCE(SUM(cost_cents), 0.0) AS cost_cents,
                COALESCE(AVG(latency_ms), 0)::int4 AS avg_latency
            FROM ai_usage_logs
            WHERE created_at >= NOW() - ($1 || ' days')::interval
            GROUP BY operation
            ORDER BY tokens DESC"#,
        )
        .bind(days.to_string())
        .fetch_all(&self.db)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| OperationUsage {
                operation: r.operation,
                count: r.count,
                tokens: r.tokens,
                cost_cents: r.cost_cents,
                avg_latency_ms: r.avg_latency,
            })
            .collect())
    }
}

/// Estimate cost in USD cents based on model and token counts.
pub fn estimate_cost(model: &str, prompt_tokens: i32, completion_tokens: i32) -> f64 {
    let (input_price, output_price) = if model.contains("reasoner") {
        (
            DEEPSEEK_REASONER_INPUT_PRICE,
            DEEPSEEK_REASONER_OUTPUT_PRICE,
        )
    } else {
        (DEEPSEEK_CHAT_INPUT_PRICE, DEEPSEEK_CHAT_OUTPUT_PRICE)
    };

    let input_cost = (prompt_tokens as f64 / 1_000_000.0) * input_price * 100.0;
    let output_cost = (completion_tokens as f64 / 1_000_000.0) * output_price * 100.0;
    input_cost + output_cost
}

// ─── Response Types ──────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct UsageSummary {
    pub request_count: i32,
    pub total_prompt_tokens: i32,
    pub total_completion_tokens: i32,
    pub total_tokens: i32,
    pub total_cost_cents: f64,
    pub avg_latency_ms: i32,
    pub error_count: i32,
}

#[derive(Debug, Serialize)]
pub struct DailyUsage {
    pub date: String,
    pub requests: i32,
    pub tokens: i32,
    pub cost_cents: f64,
}

#[derive(Debug, Serialize)]
pub struct OperationUsage {
    pub operation: String,
    pub count: i32,
    pub tokens: i32,
    pub cost_cents: f64,
    pub avg_latency_ms: i32,
}

// ─── Internal Row Types ──────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
pub struct UsageSummaryRow {
    pub request_count: i32,
    pub total_prompt_tokens: i32,
    pub total_completion_tokens: i32,
    pub total_tokens_sum: i32,
    pub total_cost_cents: f64,
    pub avg_latency_ms: i32,
    pub error_count: i32,
}

#[derive(sqlx::FromRow)]
struct DailyUsageRow {
    date: chrono::NaiveDate,
    requests: i32,
    tokens: i32,
    cost_cents: f64,
}

#[derive(sqlx::FromRow)]
struct OperationUsageRow {
    operation: String,
    count: i32,
    tokens: i32,
    cost_cents: f64,
    avg_latency: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cost_estimation_deepseek_chat() {
        // 1000 input tokens + 500 output tokens with deepseek-chat
        let cost = estimate_cost("deepseek-chat", 1000, 500);
        // Input: 1000/1M * 0.14 * 100 = 0.014 cents
        // Output: 500/1M * 0.28 * 100 = 0.014 cents
        assert!((cost - 0.028).abs() < 0.001);
    }

    #[test]
    fn test_cost_estimation_deepseek_reasoner() {
        let cost = estimate_cost("deepseek-reasoner", 10000, 5000);
        // Input: 10000/1M * 0.55 * 100 = 0.55 cents
        // Output: 5000/1M * 2.19 * 100 = 1.095 cents
        assert!((cost - 1.645).abs() < 0.01);
    }

    #[test]
    fn test_cost_estimation_zero_tokens() {
        let cost = estimate_cost("deepseek-chat", 0, 0);
        assert_eq!(cost, 0.0);
    }
}
