use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    access::{visible_library_ids, LibraryAccess},
    error::ApiError,
    extractors::AuthUser,
    state::AppState,
};

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/persons", get(list_persons))
        .route("/persons/{id}", get(get_person))
        .route("/persons/{id}/books", get(person_books))
}

#[derive(Deserialize)]
struct ListParams {
    search: Option<String>,
    role: Option<String>,
    limit: Option<i64>,
}

#[derive(Serialize)]
struct PersonResponse {
    id: String,
    name: String,
    original_name: Option<String>,
    avatar_path: Option<String>,
    roles: Vec<String>,
    book_count: i32,
    total_word_count: i64,
}

#[derive(sqlx::FromRow)]
struct PersonRow {
    id: Uuid,
    name: String,
    original_name: Option<String>,
    image_path: Option<String>,
    roles: Vec<String>,
    book_count: i32,
    total_word_count: i64,
}

fn person_library_access() -> LibraryAccess {
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

fn list_persons_sql(scoped: bool) -> String {
    let library_filter = book_library_filter("b", 4, scoped);
    let roles_library_filter = book_library_filter("role_books", 4, scoped);
    format!(
        r#"
        SELECT
            p.id,
            p.name,
            p.original_name,
            p.image_path,
            COALESCE(
                (SELECT array_agg(DISTINCT bp.role::text)
                 FROM book_persons bp
                 JOIN books role_books ON role_books.id = bp.book_id
                 WHERE bp.person_id = p.id
                   AND role_books.status != 'archived'
                   {roles_library_filter}),
                ARRAY[]::text[]
            ) as roles,
            COUNT(DISTINCT b.id)::int as book_count,
            COALESCE(SUM(b.word_count), 0)::bigint as total_word_count
        FROM persons p
        JOIN book_persons bp2 ON bp2.person_id = p.id
        JOIN books b ON b.id = bp2.book_id
        WHERE ($1 = '' OR p.name ILIKE '%' || $1 || '%' OR p.original_name ILIKE '%' || $1 || '%')
          AND b.status != 'archived'
          {library_filter}
          AND (
              $2 = ''
              OR EXISTS (
                  SELECT 1
                  FROM book_persons bp3
                  JOIN books role_books ON role_books.id = bp3.book_id
                  WHERE bp3.person_id = p.id
                    AND bp3.role::text = $2
                    AND role_books.status != 'archived'
                    {roles_library_filter}
              )
          )
        GROUP BY p.id
        ORDER BY book_count DESC, p.name ASC
        LIMIT $3
        "#
    )
}

fn get_person_sql(scoped: bool) -> String {
    let library_filter = book_library_filter("b", 2, scoped);
    let roles_library_filter = book_library_filter("role_books", 2, scoped);
    format!(
        r#"
        SELECT
            p.id,
            p.name,
            p.original_name,
            p.biography,
            p.image_path,
            COALESCE(
                (SELECT array_agg(DISTINCT bp.role::text)
                 FROM book_persons bp
                 JOIN books role_books ON role_books.id = bp.book_id
                 WHERE bp.person_id = p.id
                   AND role_books.status != 'archived'
                   {roles_library_filter}),
                ARRAY[]::text[]
            ) as roles,
            COUNT(DISTINCT b.id)::int as book_count,
            COALESCE(SUM(b.word_count), 0)::bigint as total_word_count,
            p.created_at
        FROM persons p
        JOIN book_persons bp2 ON bp2.person_id = p.id
        JOIN books b ON b.id = bp2.book_id
        WHERE p.id = $1
          AND b.status != 'archived'
          {library_filter}
        GROUP BY p.id
        "#
    )
}

fn person_books_sql(scoped: bool) -> String {
    format!(
        r#"
        SELECT
            b.id,
            b.title,
            b.cover_path,
            b.word_count,
            bp.role::text as role
        FROM book_persons bp
        JOIN books b ON b.id = bp.book_id
        WHERE bp.person_id = $1
          AND b.status != 'archived'
          {}
        ORDER BY b.title
        "#,
        book_library_filter("b", 2, scoped)
    )
}

async fn list_persons(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<PersonResponse>>, ApiError> {
    let limit = params.limit.unwrap_or(100).min(500);
    let search_filter = params.search.as_deref().unwrap_or("");
    let role_filter = params.role.as_deref().unwrap_or("");
    let visible_libraries = visible_library_ids(&state, &auth, person_library_access()).await?;
    if scoped_library_ids_empty(&visible_libraries) {
        return Ok(Json(Vec::new()));
    }

    let sql = list_persons_sql(visible_libraries.is_some());
    let mut query = sqlx::query_as::<_, PersonRow>(&sql)
        .bind(search_filter)
        .bind(role_filter)
        .bind(limit);
    if let Some(ids) = visible_libraries.as_deref() {
        query = query.bind(ids);
    }
    let rows = query.fetch_all(&state.db).await.map_err(ApiError::from)?;

    let persons = rows
        .into_iter()
        .map(|r| PersonResponse {
            id: r.id.to_string(),
            name: r.name,
            original_name: r.original_name,
            avatar_path: r.image_path,
            roles: r.roles,
            book_count: r.book_count,
            total_word_count: r.total_word_count,
        })
        .collect();

    Ok(Json(persons))
}

#[derive(sqlx::FromRow)]
struct PersonDetailRow {
    id: Uuid,
    name: String,
    original_name: Option<String>,
    biography: Option<String>,
    image_path: Option<String>,
    roles: Vec<String>,
    book_count: i32,
    total_word_count: i64,
    created_at: chrono::DateTime<chrono::Utc>,
}

async fn get_person(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let visible_libraries = visible_library_ids(&state, &auth, person_library_access()).await?;
    if scoped_library_ids_empty(&visible_libraries) {
        return Err(ApiError::NotFound("Person not found".to_string()));
    }

    let sql = get_person_sql(visible_libraries.is_some());
    let mut query = sqlx::query_as::<_, PersonDetailRow>(&sql).bind(id);
    if let Some(ids) = visible_libraries.as_deref() {
        query = query.bind(ids);
    }
    let row = query
        .fetch_optional(&state.db)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound("Person not found".to_string()))?;

    Ok(Json(serde_json::json!({
        "id": row.id.to_string(),
        "name": row.name,
        "original_name": row.original_name,
        "biography": row.biography,
        "avatar_path": row.image_path,
        "roles": row.roles,
        "book_count": row.book_count,
        "total_word_count": row.total_word_count,
        "created_at": row.created_at.to_rfc3339(),
    })))
}

#[derive(sqlx::FromRow)]
struct PersonBookRow {
    id: Uuid,
    title: String,
    cover_path: Option<String>,
    word_count: i64,
    role: String,
}

async fn person_books(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    let visible_libraries = visible_library_ids(&state, &auth, person_library_access()).await?;
    if scoped_library_ids_empty(&visible_libraries) {
        return Ok(Json(Vec::new()));
    }

    let sql = person_books_sql(visible_libraries.is_some());
    let mut query = sqlx::query_as::<_, PersonBookRow>(&sql).bind(id);
    if let Some(ids) = visible_libraries.as_deref() {
        query = query.bind(ids);
    }
    let rows = query.fetch_all(&state.db).await.map_err(ApiError::from)?;

    let books: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id.to_string(),
                "title": r.title,
                "cover_path": r.cover_path,
                "word_count": r.word_count,
                "role": r.role,
            })
        })
        .collect();

    Ok(Json(books))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn person_routes_are_scoped_by_visible_libraries() {
        let source = include_str!("persons.rs");

        assert!(source.contains("extractors::AuthUser"));
        assert!(source.contains("visible_library_ids(&state, &auth, person_library_access())"));
        assert!(source.contains("b.library_id = ANY($4::uuid[])"));
        assert!(source.contains("b.library_id = ANY($2::uuid[])"));
    }

    #[test]
    fn person_queries_hide_archived_or_invisible_books() {
        assert!(list_persons_sql(true).contains("b.status != 'archived'"));
        assert!(get_person_sql(true).contains("JOIN book_persons bp2"));
        assert!(get_person_sql(true).contains("b.library_id = ANY($2::uuid[])"));
        assert!(person_books_sql(true).contains("b.status != 'archived'"));
        assert!(person_books_sql(true).contains("b.library_id = ANY($2::uuid[])"));
    }
}
