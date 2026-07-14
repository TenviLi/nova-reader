use std::sync::Arc;

use axum::{
    extract::{Path, State},
    routing::get,
    Json, Router,
};
use uuid::Uuid;

use crate::access::{ensure_book_access, LibraryAccess};
use crate::error::ApiResult;
use crate::extractors::AuthUser;
use crate::state::AppState;
use nova_core::repo::chapter_repo::ChapterRepository;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/books/{book_id}/chapters", get(list_chapters))
        .route(
            "/books/{book_id}/chapters/{chapter_index}",
            get(get_chapter),
        )
        .route(
            "/books/{book_id}/chapters/{chapter_index}/content",
            get(get_chapter_content),
        )
        .route(
            "/books/{book_id}/chapters/{chapter_index}/entities",
            get(get_chapter_entities),
        )
}

/// List all chapters of a book (metadata only, no content).
async fn list_chapters(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(book_id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_book_access(&state, &auth, book_id, LibraryAccess::Read).await?;
    let chapters = state.chapters.list_by_book(book_id).await?;

    let data: Vec<serde_json::Value> = chapters
        .iter()
        .map(|ch| {
            serde_json::json!({
                "id": ch.id,
                "book_id": ch.book_id,
                "title": ch.title,
                "index": ch.index,
                "word_count": ch.word_count,
                "created_at": ch.created_at,
            })
        })
        .collect();

    let total = data.len();

    Ok(Json(serde_json::json!({
        "data": data,
        "book_id": book_id.to_string(),
        "total": total,
    })))
}

/// Get chapter metadata by index.
async fn get_chapter(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path((book_id, chapter_index)): Path<(Uuid, i32)>,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_book_access(&state, &auth, book_id, LibraryAccess::Read).await?;
    let chapter = state.chapters.get_by_index(book_id, chapter_index).await?;

    Ok(Json(serde_json::json!({
        "id": chapter.id,
        "book_id": chapter.book_id,
        "title": chapter.title,
        "index": chapter.index,
        "word_count": chapter.word_count,
        "created_at": chapter.created_at,
    })))
}

/// Get chapter full content for the reader.
async fn get_chapter_content(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path((book_id, chapter_index)): Path<(Uuid, i32)>,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_book_access(&state, &auth, book_id, LibraryAccess::Read).await?;
    let chapter = state.chapters.get_by_index(book_id, chapter_index).await?;

    Ok(Json(serde_json::json!({
        "content": chapter.content,
        "title": chapter.title,
        "index": chapter.index,
        "word_count": chapter.word_count,
    })))
}

/// Get entities extracted from a specific chapter.
async fn get_chapter_entities(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path((book_id, chapter_index)): Path<(Uuid, i32)>,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_book_access(&state, &auth, book_id, LibraryAccess::Read).await?;
    // Find the chapter_id first
    let chapter = state.chapters.get_by_index(book_id, chapter_index).await?;

    let entities = sqlx::query_as::<_, ChapterEntityRow>(
        r#"
        SELECT e.id, e.name, e.entity_type::text as entity_type,
               em.start_offset, em.end_offset, em.context_text
        FROM entity_mentions em
        JOIN entities e ON e.id = em.entity_id
        WHERE em.chapter_id = $1
          AND e.book_id = $2
        ORDER BY em.start_offset
        "#,
    )
    .bind(chapter.id)
    .bind(book_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let data: Vec<serde_json::Value> = entities
        .into_iter()
        .map(|e| {
            serde_json::json!({
                "id": e.id,
                "name": e.name,
                "type": e.entity_type,
                "start_offset": e.start_offset,
                "end_offset": e.end_offset,
                "context": e.context_text,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "data": data })))
}

#[derive(sqlx::FromRow)]
struct ChapterEntityRow {
    id: Uuid,
    name: String,
    entity_type: Option<String>,
    start_offset: i64,
    end_offset: i64,
    context_text: Option<String>,
}
