//! Stub AI routes — return "unavailable" until AI API keys are configured.

use std::sync::Arc;

use axum::{
    extract::State,
    response::sse::{Event, Sse},
    routing::{get, post},
    Json, Router,
};
use futures::stream::{self, Stream};
use serde::Deserialize;
use std::convert::Infallible;
use uuid::Uuid;

use crate::{error::ApiError, state::AppState};

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/ai/chat", post(ai_chat))
        .route("/ai/chat/stream", post(ai_chat_stream))
        .route("/ai/summarize", post(ai_summarize))
        .route("/ai/extract-entities", post(ai_extract_entities))
        .route("/ai/analyze-style", post(ai_analyze_style))
        .route("/ai/suggest-tags", post(ai_suggest_tags))
        .route("/ai/generate-outline", post(ai_generate_outline))
        .route("/ai/batch-process", post(ai_batch_process))
        .route("/ai/ingest-embeddings", post(ai_ingest_embeddings))
        .route("/ai/detect-plot-holes", post(ai_detect_plot_holes))
        .route(
            "/ai/generate-chapter-titles",
            post(ai_generate_chapter_titles),
        )
        .route("/ai/cleanup-forum-text", post(ai_cleanup_forum_text))
        .route("/ai/sentiment-arc", post(ai_sentiment_arc))
        .route("/ai/extract-glossary", post(ai_extract_glossary))
        .route("/ai/usage/summary", get(ai_usage_summary))
        .route("/ai/usage/daily", get(ai_usage_daily))
        .route("/ai/usage/operations", get(ai_usage_operations))
}

/// Usage tracking routes only (real AI routes provided by `ai` module).
pub fn usage_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/ai/usage/summary", get(ai_usage_summary))
        .route("/ai/usage/daily", get(ai_usage_daily))
        .route("/ai/usage/operations", get(ai_usage_operations))
}

#[derive(Deserialize)]
struct ChatRequest {
    #[allow(dead_code)]
    message: Option<String>,
    #[allow(dead_code)]
    messages: Option<Vec<serde_json::Value>>,
    #[allow(dead_code)]
    book_id: Option<Uuid>,
    #[allow(dead_code)]
    context: Option<String>,
}

async fn ai_chat(Json(_body): Json<ChatRequest>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(serde_json::json!({
        "response": "AI 功能尚未配置。请在管理面板设置 AI API 密钥后使用。",
        "model": "none",
        "tokens_used": 0,
    })))
}

async fn ai_chat_stream(
    Json(_body): Json<ChatRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let events = vec![
        Ok(Event::default()
            .data(r#"{"content":"AI 功能尚未配置。请设置 API 密钥。","done":false}"#)),
        Ok(Event::default().data(r#"{"content":"","done":true}"#)),
    ];
    Sse::new(stream::iter(events))
}

#[derive(Deserialize)]
struct BookIdRequest {
    #[allow(dead_code)]
    book_id: Option<Uuid>,
    #[allow(dead_code)]
    chapter_index: Option<i32>,
}

async fn ai_summarize(
    Json(_body): Json<BookIdRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        serde_json::json!({ "summary": "AI 摘要功能尚未配置。", "status": "unavailable" }),
    ))
}

async fn ai_extract_entities(
    Json(_body): Json<BookIdRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        serde_json::json!({ "entities": [], "status": "unavailable" }),
    ))
}

async fn ai_analyze_style(
    Json(_body): Json<BookIdRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        serde_json::json!({ "style": null, "status": "unavailable" }),
    ))
}

async fn ai_suggest_tags(
    Json(_body): Json<BookIdRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        serde_json::json!({ "tags": [], "status": "unavailable" }),
    ))
}

async fn ai_generate_outline(
    Json(_body): Json<BookIdRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        serde_json::json!({ "outline": [], "status": "unavailable" }),
    ))
}

async fn ai_batch_process(
    Json(_body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        serde_json::json!({ "task_id": null, "status": "unavailable" }),
    ))
}

async fn ai_ingest_embeddings(
    Json(_body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(serde_json::json!({ "status": "unavailable" })))
}

async fn ai_detect_plot_holes(
    Json(_body): Json<BookIdRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        serde_json::json!({ "issues": [], "status": "unavailable" }),
    ))
}

async fn ai_generate_chapter_titles(
    Json(_body): Json<BookIdRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        serde_json::json!({ "titles": [], "status": "unavailable" }),
    ))
}

async fn ai_cleanup_forum_text(
    Json(_body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        serde_json::json!({ "cleaned_text": "", "status": "unavailable" }),
    ))
}

async fn ai_sentiment_arc(
    Json(_body): Json<BookIdRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        serde_json::json!({ "arc": [], "status": "unavailable" }),
    ))
}

async fn ai_extract_glossary(
    Json(_body): Json<BookIdRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        serde_json::json!({ "entries": [], "status": "unavailable" }),
    ))
}

async fn ai_usage_summary(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(serde_json::json!({
        "total_tokens": 0,
        "total_requests": 0,
        "total_cost_usd": 0.0,
        "models_used": [],
        "period": "all_time",
    })))
}

async fn ai_usage_daily(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(serde_json::json!({ "data": [] })))
}

async fn ai_usage_operations(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(serde_json::json!({ "data": [] })))
}
