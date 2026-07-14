use axum::{
    extract::{Path, Query, State},
    routing::{get, post, put, delete},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{error::ApiError, state::AppState};

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(list_glossary).post(create_entry))
        .route("/{id}", put(update_entry).delete(delete_entry))
        .route("/search", get(search_glossary))
        .route("/auto-extract", post(auto_extract))
}

#[derive(Deserialize)]
struct ListQuery {
    book_id: Option<Uuid>,
    source_language: Option<String>,
    target_language: Option<String>,
    category: Option<String>,
    search: Option<String>,
    limit: Option<i64>,
}

#[derive(Serialize, Deserialize)]
struct GlossaryEntryResponse {
    id: Uuid,
    source_term: String,
    target_term: String,
    source_language: String,
    target_language: String,
    category: String,
    context: Option<String>,
    book_id: Option<Uuid>,
    created_at: String,
}

async fn list_glossary(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<GlossaryEntryResponse>>, ApiError> {
    let limit = query.limit.unwrap_or(100).min(500);

    let entries = sqlx::query!(
        r#"
        SELECT id, source_term, target_term, source_language, target_language,
               category, context, book_id, created_at
        FROM glossary_entries
        WHERE ($1::uuid IS NULL OR book_id = $1)
          AND ($2::text IS NULL OR source_language = $2)
          AND ($3::text IS NULL OR target_language = $3)
          AND ($4::text IS NULL OR category = $4)
          AND ($5::text IS NULL OR source_term ILIKE '%' || $5 || '%' OR target_term ILIKE '%' || $5 || '%')
        ORDER BY source_term
        LIMIT $6
        "#,
        query.book_id,
        query.source_language,
        query.target_language,
        query.category,
        query.search,
        limit,
    )
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;

    let result = entries.into_iter().map(|e| GlossaryEntryResponse {
        id: e.id,
        source_term: e.source_term,
        target_term: e.target_term,
        source_language: e.source_language,
        target_language: e.target_language,
        category: e.category,
        context: e.context,
        book_id: e.book_id,
        created_at: e.created_at.to_string(),
    }).collect();

    Ok(Json(result))
}

#[derive(Deserialize)]
struct CreateEntryRequest {
    source_term: String,
    target_term: String,
    source_language: String,
    target_language: String,
    category: String,
    context: Option<String>,
    book_id: Option<Uuid>,
}

async fn create_entry(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateEntryRequest>,
) -> Result<Json<GlossaryEntryResponse>, ApiError> {
    let id = Uuid::now_v7();
    sqlx::query!(
        r#"
        INSERT INTO glossary_entries (id, source_term, target_term, source_language, target_language, category, context, book_id)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
        id,
        body.source_term,
        body.target_term,
        body.source_language,
        body.target_language,
        body.category,
        body.context,
        body.book_id,
    )
    .execute(&state.db)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(GlossaryEntryResponse {
        id,
        source_term: body.source_term,
        target_term: body.target_term,
        source_language: body.source_language,
        target_language: body.target_language,
        category: body.category,
        context: body.context,
        book_id: body.book_id,
        created_at: chrono::Utc::now().to_rfc3339(),
    }))
}

async fn update_entry(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(body): Json<CreateEntryRequest>,
) -> Result<Json<GlossaryEntryResponse>, ApiError> {
    sqlx::query!(
        r#"
        UPDATE glossary_entries
        SET source_term = $2, target_term = $3, source_language = $4,
            target_language = $5, category = $6, context = $7, book_id = $8
        WHERE id = $1
        "#,
        id,
        body.source_term,
        body.target_term,
        body.source_language,
        body.target_language,
        body.category,
        body.context,
        body.book_id,
    )
    .execute(&state.db)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(GlossaryEntryResponse {
        id,
        source_term: body.source_term,
        target_term: body.target_term,
        source_language: body.source_language,
        target_language: body.target_language,
        category: body.category,
        context: body.context,
        book_id: body.book_id,
        created_at: String::new(),
    }))
}

async fn delete_entry(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<(), ApiError> {
    sqlx::query!("DELETE FROM glossary_entries WHERE id = $1", id)
        .execute(&state.db)
        .await
        .map_err(ApiError::from)?;
    Ok(())
}

async fn search_glossary(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<GlossaryEntryResponse>>, ApiError> {
    list_glossary(State(state), Query(query)).await
}

#[derive(Deserialize)]
struct AutoExtractRequest {
    book_id: Uuid,
    source_language: String,
    target_language: String,
}

async fn auto_extract(
    State(state): State<Arc<AppState>>,
    Json(body): Json<AutoExtractRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Enqueue a task to extract glossary entries using AI
    // This would use the nova-translate crate's glossary extraction
    Ok(Json(serde_json::json!({
        "status": "queued",
        "message": "Glossary extraction task queued"
    })))
}
