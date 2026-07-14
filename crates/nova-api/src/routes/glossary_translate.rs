//! Stub glossary and translation routes.

use std::sync::Arc;

use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    access::{ensure_book_access, is_admin, visible_library_ids, LibraryAccess},
    error::ApiError,
    extractors::AuthUser,
    state::AppState,
};

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/glossary", get(list_glossary).post(create_glossary_entry))
        .route("/glossary/lookup", get(lookup_term))
}

#[derive(Deserialize)]
struct GlossaryQuery {
    book_id: Option<Uuid>,
    search: Option<String>,
}

fn list_glossary_sql(book_filter: bool, library_scoped: bool) -> String {
    let library_join = if library_scoped {
        "LEFT JOIN books b ON b.id = ge.book_id"
    } else {
        ""
    };
    let scope_filter = if book_filter {
        "AND (ge.book_id = $1 OR ge.is_global = true OR ge.book_id IS NULL)"
    } else if library_scoped {
        "AND (ge.is_global = true OR ge.book_id IS NULL OR b.library_id = ANY($2::uuid[]))"
    } else {
        ""
    };
    let search_placeholder = if book_filter { 2 } else { 1 };

    format!(
        r#"
        SELECT ge.id, ge.source_term as term, ge.target_term as definition,
               ge.source_language, ge.target_language, ge.book_id, ge.created_at
        FROM glossary_entries ge
        {library_join}
        WHERE (${search_placeholder}::text IS NULL
               OR ge.source_term ILIKE '%' || ${search_placeholder} || '%'
               OR ge.target_term ILIKE '%' || ${search_placeholder} || '%')
          {scope_filter}
        ORDER BY ge.source_term
        LIMIT 200
        "#
    )
}

fn lookup_glossary_sql(book_filter: bool, library_scoped: bool) -> String {
    let library_join = if library_scoped {
        "LEFT JOIN books b ON b.id = ge.book_id"
    } else {
        ""
    };
    let scope_filter = if book_filter {
        "AND (ge.book_id = $2 OR ge.is_global = true OR ge.book_id IS NULL)"
    } else if library_scoped {
        "AND (ge.is_global = true OR ge.book_id IS NULL OR b.library_id = ANY($2::uuid[]))"
    } else {
        ""
    };

    format!(
        r#"
        SELECT ge.id, ge.source_term as term, ge.target_term as definition,
               ge.source_language, ge.target_language, ge.book_id, ge.created_at
        FROM glossary_entries ge
        {library_join}
        WHERE ge.source_term ILIKE $1
          {scope_filter}
        ORDER BY ge.source_term
        LIMIT 20
        "#
    )
}

async fn list_glossary(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(params): Query<GlossaryQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let entries = if let Some(book_id) = params.book_id {
        ensure_book_access(&state, &auth, book_id, LibraryAccess::Read).await?;
        let sql = list_glossary_sql(true, false);
        sqlx::query_as::<_, GlossaryRow>(&sql)
            .bind(book_id)
            .bind(params.search.as_deref())
            .fetch_all(&state.db)
            .await
            .map_err(ApiError::from)?
    } else {
        let visible_libraries = visible_library_ids(&state, &auth, LibraryAccess::Read).await?;
        let sql = list_glossary_sql(false, visible_libraries.is_some());
        let mut query = sqlx::query_as::<_, GlossaryRow>(&sql).bind(params.search.as_deref());
        if let Some(ids) = visible_libraries.as_deref() {
            query = query.bind(ids);
        }
        query.fetch_all(&state.db).await.map_err(ApiError::from)?
    };

    let data: Vec<serde_json::Value> = entries
        .into_iter()
        .map(|e| {
            serde_json::json!({
                "id": e.id,
                "term": e.term,
                "definition": e.definition,
                "source_language": e.source_language,
                "target_language": e.target_language,
                "book_id": e.book_id,
                "created_at": e.created_at,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "data": data })))
}

#[derive(sqlx::FromRow, Default)]
struct GlossaryRow {
    id: uuid::Uuid,
    term: String,
    definition: Option<String>,
    source_language: Option<String>,
    target_language: Option<String>,
    book_id: Option<uuid::Uuid>,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Deserialize)]
struct CreateGlossaryRequest {
    term: String,
    definition: Option<String>,
    source_language: Option<String>,
    target_language: Option<String>,
    book_id: Option<uuid::Uuid>,
}

async fn create_glossary_entry(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<CreateGlossaryRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if let Some(book_id) = body.book_id {
        ensure_book_access(&state, &auth, book_id, LibraryAccess::Write).await?;
    } else if !is_admin(&state, &auth).await? {
        return Err(ApiError::forbidden());
    }

    let id = uuid::Uuid::now_v7();
    sqlx::query(
        "INSERT INTO glossary_entries (id, source_term, target_term, source_language, target_language, book_id, is_global) VALUES ($1, $2, $3, $4, $5, $6, $7)"
    )
    .bind(id)
    .bind(&body.term)
    .bind(body.definition.as_deref().unwrap_or(""))
    .bind(&body.source_language)
    .bind(&body.target_language)
    .bind(body.book_id)
    .bind(body.book_id.is_none())
    .execute(&state.db)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(serde_json::json!({ "id": id, "term": body.term })))
}

#[derive(Deserialize)]
struct LookupQuery {
    term: Option<String>,
    book_id: Option<Uuid>,
}

async fn lookup_term(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(params): Query<LookupQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let term = params.term.unwrap_or_default();
    if term.trim().is_empty() {
        return Ok(Json(serde_json::json!({ "data": [] })));
    }

    let results = if let Some(book_id) = params.book_id {
        ensure_book_access(&state, &auth, book_id, LibraryAccess::Read).await?;
        let sql = lookup_glossary_sql(true, false);
        sqlx::query_as::<_, GlossaryRow>(&sql)
            .bind(format!("%{}%", term))
            .bind(book_id)
            .fetch_all(&state.db)
            .await
            .map_err(ApiError::from)?
    } else {
        let visible_libraries = visible_library_ids(&state, &auth, LibraryAccess::Read).await?;
        let sql = lookup_glossary_sql(false, visible_libraries.is_some());
        let mut query = sqlx::query_as::<_, GlossaryRow>(&sql).bind(format!("%{}%", term));
        if let Some(ids) = visible_libraries.as_deref() {
            query = query.bind(ids);
        }
        query.fetch_all(&state.db).await.map_err(ApiError::from)?
    };

    let data: Vec<serde_json::Value> = results
        .into_iter()
        .map(|e| {
            serde_json::json!({
                "id": e.id,
                "term": e.term,
                "definition": e.definition,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "data": data })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn production_source() -> &'static str {
        include_str!("glossary_translate.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("production source should exist")
    }

    #[test]
    fn glossary_routes_require_authenticated_acl() {
        let source = production_source();

        assert!(source.contains("extractors::AuthUser"));
        assert!(source.contains("visible_library_ids(&state, &auth, LibraryAccess::Read)"));
        assert!(source
            .contains("ensure_book_access(&state, &auth, book_id, LibraryAccess::Read).await?"));
        assert!(source
            .contains("ensure_book_access(&state, &auth, book_id, LibraryAccess::Write).await?"));
    }

    #[test]
    fn glossary_queries_scope_book_bound_entries() {
        assert!(list_glossary_sql(true, true).contains("ge.book_id = $1"));
        assert!(list_glossary_sql(false, true).contains("b.library_id = ANY($2::uuid[])"));
        assert!(lookup_glossary_sql(false, true).contains("b.library_id = ANY($2::uuid[])"));
    }

    #[test]
    fn glossary_lookup_accepts_book_scope_and_requires_read_access() {
        let source = production_source();

        assert!(source.contains("book_id: Option<Uuid>"));
        assert!(source.contains("if let Some(book_id) = params.book_id"));
        assert!(source
            .contains("ensure_book_access(&state, &auth, book_id, LibraryAccess::Read).await?"));
        assert!(lookup_glossary_sql(true, false).contains("ge.book_id = $2"));
    }

    #[test]
    fn glossary_empty_library_scope_still_allows_global_entries() {
        assert!(list_glossary_sql(false, true).contains("ge.is_global = true"));
        assert!(lookup_glossary_sql(false, true).contains("ge.is_global = true"));
        assert!(!production_source().contains("scoped_library_ids_empty"));
    }

    #[test]
    fn glossary_queries_propagate_database_errors() {
        let source = production_source();

        assert!(!source.contains(".await\n            .unwrap_or_default()"));
        assert!(source.contains(".await\n            .map_err(ApiError::from)?"));
    }
}
