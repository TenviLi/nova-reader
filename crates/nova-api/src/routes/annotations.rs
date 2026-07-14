use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    access::{auth_user_id, ensure_book_access, LibraryAccess},
    error::ApiError,
    extractors::AuthUser,
    state::AppState,
};

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/books/{book_id}/annotations",
            get(list_annotations).post(create_annotation),
        )
        .route(
            "/books/{book_id}/annotations/{id}",
            delete(delete_annotation),
        )
        .route(
            "/books/{book_id}/annotations/export",
            get(export_annotations),
        )
        .route(
            "/books/{book_id}/bookmarks",
            get(list_bookmarks).post(create_bookmark),
        )
        .route("/books/{book_id}/bookmarks/{id}", delete(delete_bookmark))
        .route("/annotations/{id}/share", post(share_annotation))
}

pub fn public_routes() -> Router<Arc<AppState>> {
    Router::new().route("/shared/annotations/{token}", get(get_shared_annotation))
}

fn reader_artifact_access() -> LibraryAccess {
    LibraryAccess::Read
}

fn shared_annotation_access() -> LibraryAccess {
    LibraryAccess::Read
}

fn annotation_select_sql() -> &'static str {
    r#"
        SELECT id, book_id, chapter_index, start_offset, end_offset,
               selected_text, note, color::text as color, created_at
        FROM annotations
        WHERE book_id = $1 AND user_id = $2
        ORDER BY chapter_index, start_offset
        "#
}

fn annotation_insert_sql() -> &'static str {
    r#"
        INSERT INTO annotations (id, user_id, book_id, chapter_index, start_offset, end_offset,
                                selected_text, note, color, chapter_id)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9::highlight_color,
                COALESCE((SELECT id FROM chapters WHERE book_id = $3 AND chapter_index = $4 LIMIT 1),
                         (SELECT id FROM chapters WHERE book_id = $3 LIMIT 1)))
        "#
}

fn annotation_delete_sql() -> &'static str {
    "DELETE FROM annotations WHERE id = $1 AND book_id = $2 AND user_id = $3"
}

fn bookmark_select_sql() -> &'static str {
    r#"
        SELECT id, book_id, chapter_index, position, title, created_at
        FROM bookmarks
        WHERE book_id = $1 AND user_id = $2
        ORDER BY chapter_index, position
        "#
}

fn bookmark_insert_sql() -> &'static str {
    r#"
        INSERT INTO bookmarks (id, user_id, book_id, chapter_index, position, title, chapter_id)
        VALUES ($1, $2, $3, $4, $5, $6,
                COALESCE((SELECT id FROM chapters WHERE book_id = $3 AND chapter_index = $4 LIMIT 1),
                         (SELECT id FROM chapters WHERE book_id = $3 LIMIT 1)))
        "#
}

fn bookmark_delete_sql() -> &'static str {
    "DELETE FROM bookmarks WHERE id = $1 AND book_id = $2 AND user_id = $3"
}

fn annotation_subject_sql() -> &'static str {
    "SELECT book_id, user_id FROM annotations WHERE id = $1"
}

#[derive(sqlx::FromRow)]
struct AnnotationRow {
    id: Uuid,
    book_id: Uuid,
    chapter_index: Option<i32>,
    start_offset: i64,
    end_offset: i64,
    selected_text: String,
    note: Option<String>,
    color: String,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize)]
struct AnnotationResponse {
    id: Uuid,
    book_id: Uuid,
    chapter_index: Option<i32>,
    start_offset: i64,
    end_offset: i64,
    selected_text: String,
    note: Option<String>,
    color: String,
    created_at: String,
}

async fn list_annotations(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(book_id): Path<Uuid>,
) -> Result<Json<Vec<AnnotationResponse>>, ApiError> {
    ensure_book_access(&state, &auth, book_id, reader_artifact_access()).await?;
    let user_id = auth_user_id(&auth)?;
    let rows = sqlx::query_as::<_, AnnotationRow>(annotation_select_sql())
        .bind(book_id)
        .bind(user_id)
        .fetch_all(&state.db)
        .await
        .map_err(ApiError::from)?;

    let result = rows
        .into_iter()
        .map(|a| AnnotationResponse {
            id: a.id,
            book_id: a.book_id,
            chapter_index: a.chapter_index,
            start_offset: a.start_offset,
            end_offset: a.end_offset,
            selected_text: a.selected_text,
            note: a.note,
            color: a.color,
            created_at: a.created_at.to_rfc3339(),
        })
        .collect();

    Ok(Json(result))
}

#[derive(Deserialize)]
struct CreateAnnotationRequest {
    chapter_index: Option<i32>,
    start_offset: i64,
    end_offset: i64,
    selected_text: String,
    note: Option<String>,
    #[serde(default = "default_color")]
    color: String,
}

fn default_color() -> String {
    "yellow".to_string()
}

async fn create_annotation(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(book_id): Path<Uuid>,
    Json(body): Json<CreateAnnotationRequest>,
) -> Result<Json<AnnotationResponse>, ApiError> {
    ensure_book_access(&state, &auth, book_id, reader_artifact_access()).await?;
    let user_id = auth_user_id(&auth)?;
    let id = Uuid::now_v7();
    sqlx::query(annotation_insert_sql())
        .bind(id)
        .bind(user_id)
        .bind(book_id)
        .bind(body.chapter_index)
        .bind(body.start_offset)
        .bind(body.end_offset)
        .bind(&body.selected_text)
        .bind(&body.note)
        .bind(&body.color)
        .execute(&state.db)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(AnnotationResponse {
        id,
        book_id,
        chapter_index: body.chapter_index,
        start_offset: body.start_offset,
        end_offset: body.end_offset,
        selected_text: body.selected_text,
        note: body.note,
        color: body.color,
        created_at: chrono::Utc::now().to_rfc3339(),
    }))
}

async fn delete_annotation(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path((book_id, id)): Path<(Uuid, Uuid)>,
) -> Result<(), ApiError> {
    ensure_book_access(&state, &auth, book_id, reader_artifact_access()).await?;
    let user_id = auth_user_id(&auth)?;
    sqlx::query(annotation_delete_sql())
        .bind(id)
        .bind(book_id)
        .bind(user_id)
        .execute(&state.db)
        .await
        .map_err(ApiError::from)?;
    Ok(())
}

#[derive(sqlx::FromRow)]
struct BookmarkRow {
    id: Uuid,
    book_id: Uuid,
    chapter_index: Option<i32>,
    position: Option<f64>,
    title: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize)]
struct BookmarkResponse {
    id: Uuid,
    book_id: Uuid,
    chapter_index: Option<i32>,
    position: Option<f64>,
    title: Option<String>,
    created_at: String,
}

async fn list_bookmarks(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(book_id): Path<Uuid>,
) -> Result<Json<Vec<BookmarkResponse>>, ApiError> {
    ensure_book_access(&state, &auth, book_id, reader_artifact_access()).await?;
    let user_id = auth_user_id(&auth)?;
    let rows = sqlx::query_as::<_, BookmarkRow>(bookmark_select_sql())
        .bind(book_id)
        .bind(user_id)
        .fetch_all(&state.db)
        .await
        .map_err(ApiError::from)?;

    let result = rows
        .into_iter()
        .map(|b| BookmarkResponse {
            id: b.id,
            book_id: b.book_id,
            chapter_index: b.chapter_index,
            position: b.position,
            title: b.title,
            created_at: b.created_at.to_rfc3339(),
        })
        .collect();

    Ok(Json(result))
}

#[derive(Deserialize)]
struct CreateBookmarkRequest {
    chapter_index: Option<i32>,
    position: Option<f64>,
    title: Option<String>,
}

async fn create_bookmark(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(book_id): Path<Uuid>,
    Json(body): Json<CreateBookmarkRequest>,
) -> Result<Json<BookmarkResponse>, ApiError> {
    ensure_book_access(&state, &auth, book_id, reader_artifact_access()).await?;
    let user_id = auth_user_id(&auth)?;
    let id = Uuid::now_v7();
    sqlx::query(bookmark_insert_sql())
        .bind(id)
        .bind(user_id)
        .bind(book_id)
        .bind(body.chapter_index)
        .bind(body.position)
        .bind(&body.title)
        .execute(&state.db)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(BookmarkResponse {
        id,
        book_id,
        chapter_index: body.chapter_index,
        position: body.position,
        title: body.title,
        created_at: chrono::Utc::now().to_rfc3339(),
    }))
}

async fn delete_bookmark(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path((book_id, id)): Path<(Uuid, Uuid)>,
) -> Result<(), ApiError> {
    ensure_book_access(&state, &auth, book_id, reader_artifact_access()).await?;
    let user_id = auth_user_id(&auth)?;
    sqlx::query(bookmark_delete_sql())
        .bind(id)
        .bind(book_id)
        .bind(user_id)
        .execute(&state.db)
        .await
        .map_err(ApiError::from)?;
    Ok(())
}

async fn share_annotation(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let subject = annotation_subject(&state, id).await?;
    let user_id = auth_user_id(&auth)?;
    if subject.user_id != Some(user_id) {
        return Err(ApiError::forbidden());
    }
    ensure_book_access(&state, &auth, subject.book_id, shared_annotation_access()).await?;

    let token = Uuid::new_v4().simple().to_string()[..32].to_string();

    sqlx::query(
        r#"INSERT INTO annotation_shares (annotation_id, token)
           VALUES ($1, $2)
           ON CONFLICT (annotation_id) DO UPDATE SET token = $2, created_at = NOW()"#,
    )
    .bind(id)
    .bind(&token)
    .execute(&state.db)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(serde_json::json!({ "token": token })))
}

#[derive(sqlx::FromRow)]
struct SharedAnnotationRow {
    selected_text: String,
    note: Option<String>,
    color: String,
    book_title: String,
    book_author: Option<String>,
}

async fn get_shared_annotation(
    State(state): State<Arc<AppState>>,
    Path(token): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let row = sqlx::query_as::<_, SharedAnnotationRow>(
        r#"SELECT a.selected_text, a.note, a.color::text as color,
                  b.title as book_title, b.author as book_author
           FROM annotation_shares s
           JOIN annotations a ON a.id = s.annotation_id
           JOIN books b ON b.id = a.book_id
           WHERE s.token = $1"#,
    )
    .bind(&token)
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::from)?
    .ok_or_else(|| ApiError::NotFound("Shared annotation not found".to_string()))?;

    Ok(Json(serde_json::json!({
        "text": row.selected_text,
        "note": row.note,
        "color": row.color,
        "book_title": row.book_title,
        "book_author": row.book_author,
    })))
}

/// Export annotations for a book in various formats.
#[derive(Deserialize)]
struct ExportQuery {
    format: Option<String>,
}

async fn export_annotations(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(book_id): Path<Uuid>,
    Query(params): Query<ExportQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    ensure_book_access(&state, &auth, book_id, LibraryAccess::Read).await?;
    let user_id = auth_user_id(&auth)?;
    let format = params.format.unwrap_or_else(|| "json".to_string());

    let rows = sqlx::query_as::<_, AnnotationRow>(annotation_select_sql())
        .bind(book_id)
        .bind(user_id)
        .fetch_all(&state.db)
        .await
        .map_err(ApiError::from)?;

    match format.as_str() {
        "markdown" => {
            let mut md = String::from("# Annotations\n\n");
            for a in &rows {
                md.push_str(&format!("## Chapter {}\n\n", a.chapter_index.unwrap_or(0)));
                md.push_str(&format!("> {}\n\n", a.selected_text));
                if let Some(ref note) = a.note {
                    md.push_str(&format!("*{}*\n\n", note));
                }
                md.push_str("---\n\n");
            }
            Ok(Json(
                serde_json::json!({ "content": md, "format": "markdown" }),
            ))
        }
        "notion" => {
            // Notion-compatible blocks
            let blocks: Vec<serde_json::Value> = rows
                .iter()
                .map(|a| {
                    serde_json::json!({
                        "type": "quote",
                        "quote": { "text": a.selected_text },
                        "note": a.note,
                    })
                })
                .collect();
            Ok(Json(
                serde_json::json!({ "blocks": blocks, "format": "notion" }),
            ))
        }
        _ => {
            // JSON format
            let data: Vec<serde_json::Value> = rows
                .iter()
                .map(|a| {
                    serde_json::json!({
                        "id": a.id,
                        "chapter_index": a.chapter_index,
                        "selected_text": a.selected_text,
                        "note": a.note,
                        "color": a.color,
                        "created_at": a.created_at.to_rfc3339(),
                    })
                })
                .collect();
            Ok(Json(
                serde_json::json!({ "annotations": data, "format": "json" }),
            ))
        }
    }
}

#[derive(sqlx::FromRow)]
struct AnnotationSubject {
    book_id: Uuid,
    user_id: Option<Uuid>,
}

async fn annotation_subject(
    state: &AppState,
    annotation_id: Uuid,
) -> Result<AnnotationSubject, ApiError> {
    sqlx::query_as::<_, AnnotationSubject>(annotation_subject_sql())
        .bind(annotation_id)
        .fetch_optional(&state.db)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound("Annotation not found".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reader_artifacts_only_require_read_access() {
        assert_eq!(reader_artifact_access(), LibraryAccess::Read);
    }

    #[test]
    fn shared_annotation_creation_only_requires_read_access_to_own_artifact() {
        assert_eq!(shared_annotation_access(), LibraryAccess::Read);
    }

    #[test]
    fn annotation_queries_are_scoped_to_current_user() {
        assert!(annotation_select_sql().contains("user_id = $2"));
        assert!(annotation_insert_sql().contains("user_id"));
        assert!(annotation_delete_sql().contains("user_id = $3"));
        assert!(annotation_subject_sql().contains("user_id"));
    }

    #[test]
    fn annotation_export_and_share_do_not_expose_other_users_artifacts() {
        let source = include_str!("annotations.rs");
        let manage_access = ["LibraryAccess", "::Manage"].concat();

        assert!(source.contains("query_as::<_, AnnotationRow>(annotation_select_sql())"));
        assert!(source.contains("subject.user_id != Some(user_id)"));
        assert!(source.contains("return Err(ApiError::forbidden())"));
        assert!(!source.contains(&manage_access));
    }

    #[test]
    fn bookmark_queries_are_scoped_to_current_user() {
        assert!(bookmark_select_sql().contains("user_id = $2"));
        assert!(bookmark_insert_sql().contains("user_id"));
        assert!(bookmark_delete_sql().contains("user_id = $3"));
    }
}
