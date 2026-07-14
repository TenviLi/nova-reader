use std::sync::Arc;

use axum::{
    extract::{Path, State},
    routing::{delete, get, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    access::{auth_user_id, ensure_book_access, is_admin, visible_library_ids, LibraryAccess},
    error::ApiError,
    extractors::AuthUser,
    state::AppState,
};

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/shelves", get(list_shelves).post(create_shelf))
        .route(
            "/shelves/{id}",
            get(get_shelf).put(update_shelf).delete(delete_shelf),
        )
        .route(
            "/shelves/{id}/books",
            get(list_shelf_books).post(add_book_to_shelf),
        )
        .route(
            "/shelves/{id}/books/{book_id}",
            delete(remove_book_from_shelf),
        )
        .route("/shelves/{id}/reorder", put(reorder_shelf))
        .route("/shelves/smart", get(list_smart_shelves).post(create_smart_shelf))
        .route("/shelves/smart/{id}/books", get(get_smart_shelf_books))
}

#[derive(sqlx::FromRow, Serialize)]
struct ShelfRow {
    id: Uuid,
    name: String,
    description: Option<String>,
    is_system: bool,
    sort_order: i32,
    created_at: chrono::DateTime<chrono::Utc>,
}

fn shelf_select_sql(filter_by_library: bool) -> &'static str {
    if filter_by_library {
        r#"SELECT DISTINCT s.id, s.name, s.description, s.is_system, s.sort_order, s.created_at
        FROM shelves s
        LEFT JOIN shelf_books sb ON sb.shelf_id = s.id
        LEFT JOIN books b ON b.id = sb.book_id
        WHERE s.is_system = true OR s.owner_id = $2 OR b.library_id = ANY($1)
        ORDER BY s.sort_order, s.name"#
    } else {
        "SELECT id, name, description, is_system, sort_order, created_at FROM shelves ORDER BY sort_order, name"
    }
}

fn shelf_by_id_sql() -> &'static str {
    "SELECT id, name, description, is_system, sort_order, created_at FROM shelves WHERE id = $1"
}

fn shelf_read_access_sql() -> &'static str {
    r#"SELECT EXISTS(
        SELECT 1
        FROM shelves s
        LEFT JOIN shelf_books sb ON sb.shelf_id = s.id
        LEFT JOIN books b ON b.id = sb.book_id
        WHERE s.id = $1
          AND (s.is_system = true OR s.owner_id = $3 OR b.library_id = ANY($2))
    )"#
}

fn shelf_access_sql() -> &'static str {
    r#"SELECT EXISTS(
        SELECT 1
        FROM shelves s
        LEFT JOIN shelf_books sb ON sb.shelf_id = s.id
        LEFT JOIN books b ON b.id = sb.book_id
        WHERE s.id = $1
          AND (s.owner_id = $3 OR b.library_id = ANY($2))
    )"#
}

fn shelf_owner_access_sql() -> &'static str {
    "SELECT EXISTS(SELECT 1 FROM shelves WHERE id = $1 AND owner_id = $2 AND is_system = false)"
}

fn shelf_books_sql(filter_by_library: bool) -> &'static str {
    if filter_by_library {
        r#"SELECT b.id, b.title, b.author, b.cover_path, sb.sort_order, sb.added_at
        FROM shelf_books sb JOIN books b ON b.id = sb.book_id
        WHERE sb.shelf_id = $1 AND b.library_id = ANY($2)
        ORDER BY sb.sort_order, sb.added_at"#
    } else {
        r#"SELECT b.id, b.title, b.author, b.cover_path, sb.sort_order, sb.added_at
        FROM shelf_books sb JOIN books b ON b.id = sb.book_id
        WHERE sb.shelf_id = $1 ORDER BY sb.sort_order, sb.added_at"#
    }
}

fn smart_shelf_books_sql(filter_by_library: bool) -> &'static str {
    if filter_by_library {
        "SELECT id, title, author, cover_path FROM books b WHERE b.reading_status::text = $1 AND b.library_id = ANY($2) LIMIT 50"
    } else {
        "SELECT id, title, author, cover_path FROM books b WHERE b.reading_status::text = $1 LIMIT 50"
    }
}

async fn ensure_shelf_exists(state: &AppState, shelf_id: Uuid) -> Result<(), ApiError> {
    let exists =
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM shelves WHERE id = $1)")
            .bind(shelf_id)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?;

    if exists {
        Ok(())
    } else {
        Err(ApiError::NotFound("Shelf not found".to_string()))
    }
}

async fn ensure_shelf_access(
    state: &AppState,
    auth: &AuthUser,
    shelf_id: Uuid,
    access: LibraryAccess,
) -> Result<(), ApiError> {
    match visible_library_ids(state, auth, access).await? {
        None => ensure_shelf_exists(state, shelf_id).await,
        Some(library_ids) => {
            let user_id = auth_user_id(auth)?;
            let access_sql = if access == LibraryAccess::Read {
                shelf_read_access_sql()
            } else {
                shelf_access_sql()
            };
            let allowed = sqlx::query_scalar::<_, bool>(access_sql)
                .bind(shelf_id)
                .bind(&library_ids)
                .bind(user_id)
                .fetch_one(&state.db)
                .await
                .map_err(ApiError::from)?;

            if allowed {
                Ok(())
            } else {
                Err(ApiError::forbidden())
            }
        }
    }
}

async fn ensure_shelf_mutation_access(
    state: &AppState,
    auth: &AuthUser,
    shelf_id: Uuid,
) -> Result<(), ApiError> {
    ensure_shelf_exists(state, shelf_id).await?;

    let user_id = auth_user_id(auth)?;
    let allowed = if is_admin(state, auth).await? {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM shelves WHERE id = $1 AND is_system = false)",
        )
        .bind(shelf_id)
        .fetch_one(&state.db)
        .await
        .map_err(ApiError::from)?
    } else {
        sqlx::query_scalar::<_, bool>(shelf_owner_access_sql())
            .bind(shelf_id)
            .bind(user_id)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?
    };

    if allowed {
        Ok(())
    } else {
        Err(ApiError::forbidden())
    }
}

async fn ensure_shelf_create_access(state: &AppState, auth: &AuthUser) -> Result<Uuid, ApiError> {
    let user_id = auth_user_id(auth)?;
    let create_access = LibraryAccess::Write;
    match visible_library_ids(state, auth, create_access).await? {
        None => Ok(user_id),
        Some(library_ids) if library_ids.is_empty() => Err(ApiError::forbidden()),
        Some(_) => Ok(user_id),
    }
}

async fn list_shelves(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<Vec<ShelfRow>>, ApiError> {
    let rows = match visible_library_ids(&state, &auth, LibraryAccess::Read).await? {
        None => sqlx::query_as::<_, ShelfRow>(shelf_select_sql(false))
            .fetch_all(&state.db)
            .await
            .map_err(ApiError::from)?,
        Some(library_ids) => {
            let user_id = auth_user_id(&auth)?;
            sqlx::query_as::<_, ShelfRow>(shelf_select_sql(true))
                .bind(&library_ids)
                .bind(user_id)
                .fetch_all(&state.db)
                .await
                .map_err(ApiError::from)?
        }
    };
    Ok(Json(rows))
}

async fn get_shelf(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ShelfRow>, ApiError> {
    ensure_shelf_access(&state, &auth, id, LibraryAccess::Read).await?;

    let row = sqlx::query_as::<_, ShelfRow>(shelf_by_id_sql())
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound("Shelf not found".to_string()))?;
    Ok(Json(row))
}

#[derive(Deserialize)]
struct CreateShelfRequest {
    name: String,
    description: Option<String>,
}

async fn create_shelf(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<CreateShelfRequest>,
) -> Result<Json<ShelfRow>, ApiError> {
    let owner_id = ensure_shelf_create_access(&state, &auth).await?;

    let row = sqlx::query_as::<_, ShelfRow>(
        r#"INSERT INTO shelves (id, name, description, owner_id) VALUES ($1, $2, $3, $4)
        RETURNING id, name, description, is_system, sort_order, created_at"#,
    )
    .bind(Uuid::now_v7())
    .bind(&body.name)
    .bind(&body.description)
    .bind(owner_id)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::from)?;
    Ok(Json(row))
}

#[derive(Deserialize)]
struct UpdateShelfRequest {
    name: Option<String>,
    description: Option<String>,
    sort_order: Option<i32>,
}

async fn update_shelf(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateShelfRequest>,
) -> Result<Json<ShelfRow>, ApiError> {
    ensure_shelf_mutation_access(&state, &auth, id).await?;

    let row = sqlx::query_as::<_, ShelfRow>(
        r#"UPDATE shelves SET name = COALESCE($2, name), description = COALESCE($3, description), sort_order = COALESCE($4, sort_order)
        WHERE id = $1 RETURNING id, name, description, is_system, sort_order, created_at"#,
    )
    .bind(id).bind(&body.name).bind(&body.description).bind(body.sort_order)
    .fetch_one(&state.db).await.map_err(ApiError::from)?;
    Ok(Json(row))
}

async fn delete_shelf(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<(), ApiError> {
    ensure_shelf_mutation_access(&state, &auth, id).await?;

    let is_system: bool = sqlx::query_scalar("SELECT is_system FROM shelves WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .map_err(ApiError::from)?
        .unwrap_or(false);
    if is_system {
        return Err(ApiError::bad_request("Cannot delete system shelf"));
    }
    sqlx::query("DELETE FROM shelves WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(ApiError::from)?;
    Ok(())
}

#[derive(sqlx::FromRow, Serialize)]
struct ShelfBookRow {
    id: Uuid,
    title: String,
    author: Option<String>,
    cover_path: Option<String>,
    sort_order: i32,
    added_at: chrono::DateTime<chrono::Utc>,
}

async fn list_shelf_books(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<ShelfBookRow>>, ApiError> {
    ensure_shelf_access(&state, &auth, id, LibraryAccess::Read).await?;

    let rows = match visible_library_ids(&state, &auth, LibraryAccess::Read).await? {
        None => sqlx::query_as::<_, ShelfBookRow>(shelf_books_sql(false))
            .bind(id)
            .fetch_all(&state.db)
            .await
            .map_err(ApiError::from)?,
        Some(library_ids) if library_ids.is_empty() => Vec::new(),
        Some(library_ids) => sqlx::query_as::<_, ShelfBookRow>(shelf_books_sql(true))
            .bind(id)
            .bind(&library_ids)
            .fetch_all(&state.db)
            .await
            .map_err(ApiError::from)?,
    };
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct AddBookRequest {
    book_id: Uuid,
}

async fn add_book_to_shelf(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<AddBookRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    ensure_book_access(&state, &auth, body.book_id, LibraryAccess::Write).await?;
    ensure_shelf_mutation_access(&state, &auth, id).await?;

    sqlx::query(
        "INSERT INTO shelf_books (shelf_id, book_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(id)
    .bind(body.book_id)
    .execute(&state.db)
    .await
    .map_err(ApiError::from)?;
    Ok(Json(serde_json::json!({ "status": "ok" })))
}

async fn remove_book_from_shelf(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path((id, book_id)): Path<(Uuid, Uuid)>,
) -> Result<(), ApiError> {
    ensure_book_access(&state, &auth, book_id, LibraryAccess::Write).await?;
    ensure_shelf_mutation_access(&state, &auth, id).await?;

    sqlx::query("DELETE FROM shelf_books WHERE shelf_id = $1 AND book_id = $2")
        .bind(id)
        .bind(book_id)
        .execute(&state.db)
        .await
        .map_err(ApiError::from)?;
    Ok(())
}

/// Reorder books within a shelf.
#[derive(Deserialize)]
struct ReorderRequest {
    book_ids: Vec<Uuid>,
}

async fn reorder_shelf(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ReorderRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    ensure_shelf_mutation_access(&state, &auth, id).await?;
    for book_id in &body.book_ids {
        ensure_book_access(&state, &auth, *book_id, LibraryAccess::Write).await?;
    }

    for (i, book_id) in body.book_ids.iter().enumerate() {
        sqlx::query("UPDATE shelf_books SET sort_order = $3 WHERE shelf_id = $1 AND book_id = $2")
            .bind(id)
            .bind(book_id)
            .bind(i as i32)
            .execute(&state.db)
            .await
            .map_err(ApiError::from)?;
    }
    Ok(Json(serde_json::json!({ "status": "ok" })))
}

/// Create a smart shelf (dynamic filter-based shelf).
#[derive(sqlx::FromRow, Serialize)]
struct SmartShelfRow {
    id: Uuid,
    name: String,
    description: Option<String>,
    filter_criteria: serde_json::Value,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Deserialize)]
struct CreateSmartShelfRequest {
    name: String,
    description: Option<String>,
    #[serde(alias = "filter_criteria")]
    filter: serde_json::Value,
}

async fn list_smart_shelves(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<Vec<SmartShelfRow>>, ApiError> {
    let user_id = auth_user_id(&auth)?;
    let rows = sqlx::query_as::<_, SmartShelfRow>(
        r#"
        SELECT id, name, description, filter_criteria, created_at
        FROM smart_shelves
        WHERE owner_id = $1
        ORDER BY created_at DESC
        "#,
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(rows))
}

async fn create_smart_shelf(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<CreateSmartShelfRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let owner_id = ensure_shelf_create_access(&state, &auth).await?;

    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO smart_shelves (id, name, description, filter_criteria, owner_id) VALUES ($1, $2, $3, $4, $5)"
    )
    .bind(id)
    .bind(&body.name)
    .bind(&body.description)
    .bind(&body.filter)
    .bind(owner_id)
    .execute(&state.db)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(serde_json::json!({
        "id": id,
        "name": body.name,
        "description": body.description,
        "filter_criteria": body.filter,
    })))
}

/// Get books matching a smart shelf's filter.
async fn get_smart_shelf_books(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user_id = auth_user_id(&auth)?;

    let filter: Option<serde_json::Value> = sqlx::query_scalar(
        "SELECT filter_criteria FROM smart_shelves WHERE id = $1 AND owner_id = $2",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::from)?;
    let filter = filter.ok_or_else(|| ApiError::not_found("Smart shelf not found"))?;

    let status = filter
        .get("reading_status")
        .and_then(|s| s.as_str())
        .unwrap_or("reading");
    let books = match visible_library_ids(&state, &auth, LibraryAccess::Read).await? {
        None => sqlx::query_as::<_, BookIdTitle>(smart_shelf_books_sql(false))
            .bind(status)
            .fetch_all(&state.db)
            .await
            .map_err(ApiError::from)?,
        Some(library_ids) if library_ids.is_empty() => Vec::new(),
        Some(library_ids) => sqlx::query_as::<_, BookIdTitle>(smart_shelf_books_sql(true))
            .bind(status)
            .bind(&library_ids)
            .fetch_all(&state.db)
            .await
            .map_err(ApiError::from)?,
    };

    let data: Vec<serde_json::Value> = books
        .into_iter()
        .map(|b| {
            serde_json::json!({
                "id": b.id, "title": b.title, "author": b.author, "cover_path": b.cover_path,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "data": data })))
}

#[derive(sqlx::FromRow)]
struct BookIdTitle {
    id: Uuid,
    title: String,
    author: Option<String>,
    cover_path: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{
        shelf_access_sql, shelf_owner_access_sql, shelf_read_access_sql, shelf_select_sql,
    };

    fn route_source() -> &'static str {
        include_str!("shelves.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("route source")
    }

    #[test]
    fn shelf_handlers_require_authenticated_user() {
        let source = route_source();

        assert!(source.contains("extractors::AuthUser"));
        assert_eq!(source.matches("auth: AuthUser").count(), 12);
        assert!(source.contains("auth_user_id(auth)?") || source.contains("auth_user_id(&auth)?"));
    }

    #[test]
    fn shelf_book_mutations_require_book_write_access() {
        let source = route_source();

        assert!(source.contains(
            "ensure_book_access(&state, &auth, body.book_id, LibraryAccess::Write).await?"
        ));
        assert!(source
            .contains("ensure_book_access(&state, &auth, book_id, LibraryAccess::Write).await?"));
        assert!(source
            .contains("ensure_book_access(&state, &auth, *book_id, LibraryAccess::Write).await?"));
    }

    #[test]
    fn shelf_container_mutations_are_owner_only_and_block_system_shelves() {
        let source = route_source();

        assert!(source.contains("fn shelf_owner_access_sql()"));
        assert!(shelf_owner_access_sql().contains("owner_id = $2"));
        assert!(shelf_owner_access_sql().contains("is_system = false"));
        assert!(!source.contains("fn shelf_all_books_access_sql()"));
        assert!(!source.contains("visible_library_ids(state, auth, LibraryAccess::Write).await?"));
        assert!(source.contains("ensure_shelf_mutation_access(&state, &auth, id).await?"));
    }

    #[test]
    fn removing_shelf_book_requires_container_mutation_access() {
        let source = route_source();
        let handler = source
            .split("async fn remove_book_from_shelf")
            .nth(1)
            .expect("remove handler should exist")
            .split("sqlx::query(\"DELETE FROM shelf_books")
            .next()
            .expect("remove handler should delete shelf_books");

        assert!(handler
            .contains("ensure_book_access(&state, &auth, book_id, LibraryAccess::Write).await?"));
        assert!(handler.contains("ensure_shelf_mutation_access(&state, &auth, id).await?"));
        assert!(!handler
            .contains("ensure_shelf_access(&state, &auth, id, LibraryAccess::Write).await?"));
    }

    #[test]
    fn shelf_empty_containers_are_anchored_by_owner() {
        assert!(shelf_select_sql(true).contains("s.is_system = true"));
        assert!(shelf_select_sql(true).contains("s.owner_id = $2"));
        assert!(shelf_read_access_sql().contains("s.is_system = true"));
        assert!(shelf_access_sql().contains("s.owner_id = $3"));
        assert!(shelf_owner_access_sql().contains("owner_id = $2"));
        assert!(shelf_owner_access_sql().contains("is_system = false"));
    }

    #[test]
    fn shelf_book_listing_filters_to_visible_libraries() {
        let source = route_source();

        assert!(
            source.contains("ensure_shelf_access(&state, &auth, id, LibraryAccess::Read).await?")
        );
        assert!(source.contains("visible_library_ids(&state, &auth, LibraryAccess::Read).await?"));
        assert!(source.contains("b.library_id = ANY($2)"));
        assert!(source.contains("Vec::new()"));
    }

    #[test]
    fn smart_shelf_status_queries_filter_to_owner_and_visible_libraries() {
        let source = route_source();

        assert!(source.contains("#[serde(alias = \"filter_criteria\")]"));
        assert!(source.contains("filter_criteria, owner_id"));
        assert!(source.contains("WHERE id = $1 AND owner_id = $2"));
        assert!(source.contains("smart_shelf_books_sql(true)"));
        assert!(source.contains("b.library_id = ANY($2)"));
    }
}
