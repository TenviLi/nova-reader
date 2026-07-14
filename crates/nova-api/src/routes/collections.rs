use std::sync::Arc;

use axum::{
    extract::{Path, State},
    routing::{delete, get},
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
        .route(
            "/collections",
            get(list_collections).post(create_collection),
        )
        .route(
            "/collections/{id}",
            get(get_collection)
                .put(update_collection)
                .delete(delete_collection),
        )
        .route(
            "/collections/{id}/books",
            get(list_collection_books).post(add_book_to_collection),
        )
        .route(
            "/collections/{id}/books/{book_id}",
            delete(remove_book_from_collection),
        )
}

#[derive(sqlx::FromRow, Serialize)]
struct CollectionRow {
    id: Uuid,
    name: String,
    description: Option<String>,
    cover_path: Option<String>,
    sort_order: i32,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

fn collection_select_sql(filter_by_library: bool) -> &'static str {
    if filter_by_library {
        r#"SELECT DISTINCT c.id, c.name, c.description, c.cover_path, c.sort_order, c.created_at, c.updated_at
        FROM collections c
        LEFT JOIN collection_books cb ON cb.collection_id = c.id
        LEFT JOIN books b ON b.id = cb.book_id
        WHERE c.owner_id = $2 OR b.library_id = ANY($1)
        ORDER BY c.sort_order, c.name"#
    } else {
        "SELECT id, name, description, cover_path, sort_order, created_at, updated_at FROM collections ORDER BY sort_order, name"
    }
}

fn collection_by_id_sql() -> &'static str {
    "SELECT id, name, description, cover_path, sort_order, created_at, updated_at FROM collections WHERE id = $1"
}

fn collection_access_sql() -> &'static str {
    r#"SELECT EXISTS(
        SELECT 1
        FROM collections c
        LEFT JOIN collection_books cb ON cb.collection_id = c.id
        LEFT JOIN books b ON b.id = cb.book_id
        WHERE c.id = $1
          AND (c.owner_id = $3 OR b.library_id = ANY($2))
    )"#
}

fn collection_owner_access_sql() -> &'static str {
    "SELECT EXISTS(SELECT 1 FROM collections WHERE id = $1 AND owner_id = $2)"
}

fn collection_books_sql(filter_by_library: bool) -> &'static str {
    if filter_by_library {
        r#"SELECT b.id, b.title, b.author, b.cover_path, cb.sort_order, cb.added_at
        FROM collection_books cb JOIN books b ON b.id = cb.book_id
        WHERE cb.collection_id = $1 AND b.library_id = ANY($2)
        ORDER BY cb.sort_order, cb.added_at"#
    } else {
        r#"SELECT b.id, b.title, b.author, b.cover_path, cb.sort_order, cb.added_at
        FROM collection_books cb JOIN books b ON b.id = cb.book_id
        WHERE cb.collection_id = $1 ORDER BY cb.sort_order, cb.added_at"#
    }
}

async fn ensure_collection_exists(state: &AppState, collection_id: Uuid) -> Result<(), ApiError> {
    let exists =
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM collections WHERE id = $1)")
            .bind(collection_id)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from)?;

    if exists {
        Ok(())
    } else {
        Err(ApiError::NotFound("Collection not found".to_string()))
    }
}

async fn ensure_collection_access(
    state: &AppState,
    auth: &AuthUser,
    collection_id: Uuid,
    access: LibraryAccess,
) -> Result<(), ApiError> {
    match visible_library_ids(state, auth, access).await? {
        None => ensure_collection_exists(state, collection_id).await,
        Some(library_ids) => {
            let user_id = auth_user_id(auth)?;
            let allowed = sqlx::query_scalar::<_, bool>(collection_access_sql())
                .bind(collection_id)
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

async fn ensure_collection_mutation_access(
    state: &AppState,
    auth: &AuthUser,
    collection_id: Uuid,
) -> Result<(), ApiError> {
    ensure_collection_exists(state, collection_id).await?;

    if is_admin(state, auth).await? {
        return Ok(());
    }

    let user_id = auth_user_id(auth)?;
    let allowed = sqlx::query_scalar::<_, bool>(collection_owner_access_sql())
        .bind(collection_id)
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

async fn ensure_collection_create_access(
    state: &AppState,
    auth: &AuthUser,
) -> Result<Uuid, ApiError> {
    let user_id = auth_user_id(auth)?;
    let create_access = LibraryAccess::Write;
    match visible_library_ids(state, auth, create_access).await? {
        None => Ok(user_id),
        Some(library_ids) if library_ids.is_empty() => Err(ApiError::forbidden()),
        Some(_) => Ok(user_id),
    }
}

async fn list_collections(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<Vec<CollectionRow>>, ApiError> {
    let rows = match visible_library_ids(&state, &auth, LibraryAccess::Read).await? {
        None => sqlx::query_as::<_, CollectionRow>(collection_select_sql(false))
            .fetch_all(&state.db)
            .await
            .map_err(ApiError::from)?,
        Some(library_ids) => {
            let user_id = auth_user_id(&auth)?;
            sqlx::query_as::<_, CollectionRow>(collection_select_sql(true))
                .bind(&library_ids)
                .bind(user_id)
                .fetch_all(&state.db)
                .await
                .map_err(ApiError::from)?
        }
    };
    Ok(Json(rows))
}

async fn get_collection(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<CollectionRow>, ApiError> {
    ensure_collection_access(&state, &auth, id, LibraryAccess::Read).await?;

    let row = sqlx::query_as::<_, CollectionRow>(collection_by_id_sql())
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound("Collection not found".to_string()))?;
    Ok(Json(row))
}

#[derive(Deserialize)]
struct CreateCollectionRequest {
    name: String,
    description: Option<String>,
}

async fn create_collection(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<CreateCollectionRequest>,
) -> Result<Json<CollectionRow>, ApiError> {
    let owner_id = ensure_collection_create_access(&state, &auth).await?;

    let row = sqlx::query_as::<_, CollectionRow>(
        r#"INSERT INTO collections (id, name, description, owner_id) VALUES ($1, $2, $3, $4)
        RETURNING id, name, description, cover_path, sort_order, created_at, updated_at"#,
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
struct UpdateCollectionRequest {
    name: Option<String>,
    description: Option<String>,
    sort_order: Option<i32>,
}

async fn update_collection(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateCollectionRequest>,
) -> Result<Json<CollectionRow>, ApiError> {
    ensure_collection_mutation_access(&state, &auth, id).await?;

    let row = sqlx::query_as::<_, CollectionRow>(
        r#"UPDATE collections SET name = COALESCE($2, name), description = COALESCE($3, description), sort_order = COALESCE($4, sort_order)
        WHERE id = $1 RETURNING id, name, description, cover_path, sort_order, created_at, updated_at"#,
    )
    .bind(id)
    .bind(&body.name)
    .bind(&body.description)
    .bind(body.sort_order)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::from)?;
    Ok(Json(row))
}

async fn delete_collection(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<(), ApiError> {
    ensure_collection_mutation_access(&state, &auth, id).await?;

    sqlx::query("DELETE FROM collections WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(ApiError::from)?;
    Ok(())
}

#[derive(sqlx::FromRow, Serialize)]
struct CollectionBookRow {
    id: Uuid,
    title: String,
    author: Option<String>,
    cover_path: Option<String>,
    sort_order: i32,
    added_at: chrono::DateTime<chrono::Utc>,
}

async fn list_collection_books(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<CollectionBookRow>>, ApiError> {
    ensure_collection_access(&state, &auth, id, LibraryAccess::Read).await?;

    let rows = match visible_library_ids(&state, &auth, LibraryAccess::Read).await? {
        None => sqlx::query_as::<_, CollectionBookRow>(collection_books_sql(false))
            .bind(id)
            .fetch_all(&state.db)
            .await
            .map_err(ApiError::from)?,
        Some(library_ids) if library_ids.is_empty() => Vec::new(),
        Some(library_ids) => sqlx::query_as::<_, CollectionBookRow>(collection_books_sql(true))
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

async fn add_book_to_collection(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<AddBookRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    ensure_book_access(&state, &auth, body.book_id, LibraryAccess::Write).await?;
    ensure_collection_mutation_access(&state, &auth, id).await?;

    sqlx::query("INSERT INTO collection_books (collection_id, book_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(id).bind(body.book_id).execute(&state.db).await.map_err(ApiError::from)?;
    Ok(Json(serde_json::json!({ "status": "ok" })))
}

async fn remove_book_from_collection(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path((id, book_id)): Path<(Uuid, Uuid)>,
) -> Result<(), ApiError> {
    ensure_book_access(&state, &auth, book_id, LibraryAccess::Write).await?;
    ensure_collection_mutation_access(&state, &auth, id).await?;

    sqlx::query("DELETE FROM collection_books WHERE collection_id = $1 AND book_id = $2")
        .bind(id)
        .bind(book_id)
        .execute(&state.db)
        .await
        .map_err(ApiError::from)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{collection_access_sql, collection_owner_access_sql, collection_select_sql};

    fn route_source() -> &'static str {
        include_str!("collections.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("route source")
    }

    #[test]
    fn collection_handlers_require_authenticated_user() {
        let source = route_source();

        assert!(source.contains("extractors::AuthUser"));
        assert_eq!(source.matches("auth: AuthUser").count(), 8);
        assert!(source.contains("auth_user_id(auth)?") || source.contains("auth_user_id(&auth)?"));
    }

    #[test]
    fn collection_book_mutations_require_book_write_access() {
        let source = route_source();

        assert!(source.contains(
            "ensure_book_access(&state, &auth, body.book_id, LibraryAccess::Write).await?"
        ));
        assert!(source
            .contains("ensure_book_access(&state, &auth, book_id, LibraryAccess::Write).await?"));
    }

    #[test]
    fn collection_container_mutations_are_owner_only() {
        let source = route_source();

        assert!(source.contains("fn collection_owner_access_sql()"));
        assert!(collection_owner_access_sql().contains("owner_id = $2"));
        assert!(!source.contains("fn collection_all_books_access_sql()"));
        assert!(!source.contains("visible_library_ids(state, auth, LibraryAccess::Write).await?"));
        assert!(source.contains("ensure_collection_mutation_access(&state, &auth, id).await?"));
    }

    #[test]
    fn removing_collection_book_requires_container_mutation_access() {
        let source = route_source();
        let handler = source
            .split("async fn remove_book_from_collection")
            .nth(1)
            .expect("remove handler should exist")
            .split("sqlx::query(\"DELETE FROM collection_books")
            .next()
            .expect("remove handler should delete collection_books");

        assert!(handler
            .contains("ensure_book_access(&state, &auth, book_id, LibraryAccess::Write).await?"));
        assert!(handler.contains("ensure_collection_mutation_access(&state, &auth, id).await?"));
        assert!(!handler
            .contains("ensure_collection_access(&state, &auth, id, LibraryAccess::Write).await?"));
    }

    #[test]
    fn collection_empty_containers_are_anchored_by_owner() {
        assert!(collection_select_sql(true).contains("c.owner_id = $2"));
        assert!(collection_access_sql().contains("c.owner_id = $3"));
        assert!(collection_owner_access_sql().contains("owner_id = $2"));
    }

    #[test]
    fn collection_book_listing_filters_to_visible_libraries() {
        let source = route_source();

        assert!(source
            .contains("ensure_collection_access(&state, &auth, id, LibraryAccess::Read).await?"));
        assert!(source.contains("visible_library_ids(&state, &auth, LibraryAccess::Read).await?"));
        assert!(source.contains("b.library_id = ANY($2)"));
        assert!(source.contains("Vec::new()"));
    }
}
