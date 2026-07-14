use std::sync::Arc;

use axum::{
    extract::{Multipart, Path, Query, State},
    routing::{get, post, put},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::access::{
    auth_user_id, default_library_id, ensure_book_access, ensure_library_access,
    visible_library_ids, LibraryAccess,
};
use crate::error::{ApiError, ApiResult};
use crate::extractors::AuthUser;
use crate::repo::pg_exact_file_discovery::RecordExactFileDiscovery;
use crate::state::AppState;
use nova_core::domain::book::*;
use nova_core::domain::dedup_discovery::ExactFileDiscoverySource;
use nova_core::repo::book_repo::*;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/books", get(list_books).post(upload_book))
        .route("/books/upload", post(upload_book))
        .route(
            "/books/{id}",
            get(get_book).put(update_book).delete(delete_book),
        )
        .route("/books/{id}/metadata", put(update_metadata))
        .route("/books/{id}/metadata/scrape", post(scrape_metadata))
        .route(
            "/books/{id}/custom-fields",
            get(get_custom_fields).put(update_custom_fields),
        )
        .route("/books/{id}/reprocess", post(reprocess_book))
        .route("/books/{id}/download", get(download_book))
        .route("/books/{id}/convert", post(convert_format))
        .route("/books/{id}/send", post(send_to_device))
        .route("/books/recent", get(recent_books))
        .route("/books/stats", get(book_stats))
        .route("/books/tags", get(get_book_tags))
        .route("/covers/{filename}", get(serve_cover))
}

#[derive(Debug, Deserialize)]
struct ListBooksQuery {
    #[serde(default = "default_page")]
    page: i64,
    #[serde(default = "default_per_page")]
    per_page: i64,
    #[serde(default)]
    status: Option<BookStatus>,
    #[serde(default)]
    reading_status: Option<ReadingStatus>,
    #[serde(default)]
    language: Option<Language>,
    #[serde(default)]
    format: Option<BookFormat>,
    #[serde(default)]
    library_id: Option<Uuid>,
    #[serde(default)]
    series_id: Option<Uuid>,
    #[serde(default)]
    sort_by: Option<String>,
    #[serde(default)]
    search: Option<String>,
}

fn default_page() -> i64 {
    1
}
fn default_per_page() -> i64 {
    24
}

fn resolve_cover_file(data_dir: &str, cover_path: Option<&str>) -> Option<std::path::PathBuf> {
    let cover_path = cover_path?.trim();
    if cover_path.is_empty() {
        return None;
    }

    if let Some(filename) = cover_path.strip_prefix("/api/covers/") {
        return Some(std::path::Path::new(data_dir).join("covers").join(filename));
    }

    let path = std::path::Path::new(cover_path);
    if path.is_absolute() {
        Some(path.to_path_buf())
    } else if cover_path.contains('/') {
        Some(std::path::Path::new(data_dir).join(cover_path))
    } else {
        Some(
            std::path::Path::new(data_dir)
                .join("covers")
                .join(cover_path),
        )
    }
}

fn validate_book_rating(rating: Option<i16>) -> ApiResult<()> {
    if let Some(value) = rating {
        if !(1..=5).contains(&value) {
            return Err(ApiError::bad_request("rating must be between 1 and 5"));
        }
    }
    Ok(())
}

fn book_progress_map_sql() -> &'static str {
    "SELECT book_id, progress FROM reading_progress WHERE book_id = ANY($1) AND user_id = $2"
}

fn book_progress_sql() -> &'static str {
    "SELECT COALESCE(progress, 0.0) FROM reading_progress WHERE book_id = $1 AND user_id = $2"
}

fn cover_library_candidates_sql() -> &'static str {
    r#"
    SELECT DISTINCT library_id
    FROM (
        SELECT library_id
        FROM books
        WHERE cover_path = $1
           OR cover_path = CONCAT('/api/covers/', $1)
           OR metadata->>'cover_path' = $1
           OR metadata->>'cover_path' = CONCAT('/api/covers/', $1)

        UNION ALL

        SELECT library_id
        FROM series
        WHERE cover_path = $1 OR cover_path = CONCAT('/api/covers/', $1)

        UNION ALL

        SELECT id AS library_id
        FROM libraries
        WHERE cover_path = $1 OR cover_path = CONCAT('/api/covers/', $1)
    ) cover_libraries
    WHERE library_id IS NOT NULL
    "#
}

fn cover_content_type(filename: &str) -> &'static str {
    if filename.ends_with(".png") {
        "image/png"
    } else if filename.ends_with(".webp") {
        "image/webp"
    } else if filename.ends_with(".svg") {
        "image/svg+xml; charset=utf-8"
    } else {
        "image/jpeg"
    }
}

async fn book_list_scope(
    state: &AppState,
    auth: &AuthUser,
    requested_library_id: Option<Uuid>,
    access: LibraryAccess,
) -> ApiResult<Option<Vec<Uuid>>> {
    if let Some(library_id) = requested_library_id {
        ensure_library_access(state, auth, library_id, access).await?;
        Ok(None)
    } else {
        visible_library_ids(state, auth, access).await
    }
}

async fn resolve_upload_library_id(
    state: &AppState,
    auth: &AuthUser,
    requested_library_id: Option<Uuid>,
) -> ApiResult<Option<Uuid>> {
    let library_id = match requested_library_id {
        Some(id) => Some(id),
        None => default_library_id(state).await?,
    };

    if let Some(id) = library_id {
        ensure_library_access(state, auth, id, LibraryAccess::Write).await?;
    } else if visible_library_ids(state, auth, LibraryAccess::Write)
        .await?
        .is_some_and(|ids| ids.is_empty())
    {
        return Err(ApiError::forbidden());
    }

    Ok(library_id)
}

async fn duplicate_in_upload_scope(
    state: &AppState,
    auth: &AuthUser,
    file_hash: &str,
    library_id: Option<Uuid>,
) -> ApiResult<Option<Uuid>> {
    let library_ids = if let Some(id) = library_id {
        Some(vec![id])
    } else {
        visible_library_ids(state, auth, LibraryAccess::Write).await?
    };

    let matched_book_id = state
        .books
        .find_non_archived_by_hash_in_libraries(file_hash, library_ids.as_deref())
        .await
        .map_err(|e| ApiError::internal(format!("DB error: {}", e)))?;

    Ok(matched_book_id)
}

/// List all books with pagination, filtering, and sorting.
async fn list_books(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(params): Query<ListBooksQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let sort_by = match params.sort_by.as_deref() {
        Some("title_asc") => BookSort::TitleAsc,
        Some("title_desc") => BookSort::TitleDesc,
        Some("created_at_asc") => BookSort::CreatedAtAsc,
        Some("created_at_desc") => BookSort::CreatedAtDesc,
        Some("word_count_desc") => BookSort::WordCountDesc,
        Some("word_count_asc") => BookSort::WordCountAsc,
        Some("updated_at") | Some("last_read_at") => BookSort::UpdatedAtDesc,
        _ => BookSort::UpdatedAtDesc,
    };

    let library_ids =
        book_list_scope(&state, &auth, params.library_id, LibraryAccess::Read).await?;

    let filter = BookFilter {
        search: params.search,
        status: params.status,
        reading_status: params.reading_status,
        language: params.language,
        format: params.format,
        library_id: params.library_id,
        library_ids,
        series_id: params.series_id,
        sort_by,
        page: params.page.max(1),
        per_page: params.per_page.clamp(1, 100),
    };

    let result = state.books.list(&filter).await?;
    let total_pages = result.total_pages();

    // Enrich books with reading progress
    let user_id = auth_user_id(&auth)?;
    let book_ids: Vec<Uuid> = result.data.iter().map(|b| b.id.into_uuid()).collect();
    let progress_map = if !book_ids.is_empty() {
        #[derive(sqlx::FromRow)]
        struct ProgressRow {
            book_id: Uuid,
            progress: f64,
        }
        let rows = sqlx::query_as::<_, ProgressRow>(book_progress_map_sql())
            .bind(&book_ids)
            .bind(user_id)
            .fetch_all(&state.db)
            .await
            .unwrap_or_default();
        rows.into_iter()
            .map(|r| (r.book_id, r.progress))
            .collect::<std::collections::HashMap<_, _>>()
    } else {
        std::collections::HashMap::new()
    };

    let rating_map = if !book_ids.is_empty() {
        #[derive(sqlx::FromRow)]
        struct RatingRow {
            id: Uuid,
            user_rating: Option<i16>,
        }
        let rows =
            sqlx::query_as::<_, RatingRow>("SELECT id, user_rating FROM books WHERE id = ANY($1)")
                .bind(&book_ids)
                .fetch_all(&state.db)
                .await
                .unwrap_or_default();
        rows.into_iter()
            .map(|r| (r.id, r.user_rating))
            .collect::<std::collections::HashMap<_, _>>()
    } else {
        std::collections::HashMap::new()
    };

    // Fetch library_id and library_name for all books
    let book_library_map: std::collections::HashMap<Uuid, (Uuid, String)> = if !book_ids.is_empty()
    {
        #[derive(sqlx::FromRow)]
        struct BookLibRow {
            book_id: Uuid,
            library_id: Uuid,
            library_name: String,
        }
        let rows = sqlx::query_as::<_, BookLibRow>(
            "SELECT b.id as book_id, b.library_id, l.name as library_name FROM books b JOIN libraries l ON l.id = b.library_id WHERE b.id = ANY($1)"
        )
        .bind(&book_ids)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();
        rows.into_iter()
            .map(|r| (r.book_id, (r.library_id, r.library_name)))
            .collect()
    } else {
        std::collections::HashMap::new()
    };

    // Serialize books with progress field and flattened metadata
    let enriched: Vec<serde_json::Value> = result
        .data
        .iter()
        .map(|book| {
            let mut val = serde_json::to_value(book).unwrap_or_default();
            if let Some(obj) = val.as_object_mut() {
                let bid = book.id.into_uuid();
                obj.insert(
                    "progress".into(),
                    serde_json::json!(progress_map.get(&bid).copied().unwrap_or(0.0)),
                );
                // Flatten commonly-accessed metadata fields to top level
                obj.insert(
                    "cover_path".into(),
                    serde_json::json!(book.metadata.cover_path),
                );
                obj.insert("series".into(), serde_json::json!(book.metadata.series));
                obj.insert(
                    "series_index".into(),
                    serde_json::json!(book.metadata.volume),
                );
                obj.insert("tags".into(), serde_json::json!(book.metadata.tags));
                obj.insert(
                    "rating".into(),
                    serde_json::json!(rating_map.get(&bid).copied().flatten()),
                );
                // Add library info
                if let Some((lib_id, lib_name)) = book_library_map.get(&bid) {
                    obj.insert("library_id".into(), serde_json::json!(lib_id));
                    obj.insert("library_name".into(), serde_json::json!(lib_name));
                }
            }
            val
        })
        .collect();

    Ok(Json(serde_json::json!({
        "data": enriched,
        "total": result.total,
        "page": result.page,
        "per_page": result.per_page,
        "total_pages": total_pages,
    })))
}

/// Get a single book by ID with full metadata.
async fn get_book(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_book_access(&state, &auth, id, LibraryAccess::Read).await?;
    let user_id = auth_user_id(&auth)?;
    let book = state.books.get(id).await?;
    let mut val = serde_json::to_value(&book).unwrap_or_default();

    // Enrich with reading progress and flatten metadata
    if let Some(obj) = val.as_object_mut() {
        let progress: f64 = sqlx::query_scalar(book_progress_sql())
            .bind(id)
            .bind(user_id)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
            .unwrap_or(0.0);
        obj.insert("progress".into(), serde_json::json!(progress));
        obj.insert(
            "cover_path".into(),
            serde_json::json!(book.metadata.cover_path),
        );
        obj.insert("series".into(), serde_json::json!(book.metadata.series));
        obj.insert(
            "series_index".into(),
            serde_json::json!(book.metadata.volume),
        );
        obj.insert("tags".into(), serde_json::json!(book.metadata.tags));
        let rating: Option<i16> = sqlx::query_scalar("SELECT user_rating FROM books WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten();
        obj.insert("rating".into(), serde_json::json!(rating));
    }

    Ok(Json(val))
}

/// Upload a new book file (streaming to disk to avoid OOM on large files).
async fn upload_book(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    mut multipart: Multipart,
) -> ApiResult<Json<serde_json::Value>> {
    let user_id = auth_user_id(&auth)?;
    let mut file_name: Option<String> = None;
    let mut library_id: Option<Uuid> = None;

    // Generate book ID early so we can stream directly to final path
    let book_id = Uuid::now_v7();
    let temp_dir = format!("data/books/{}", book_id);
    tokio::fs::create_dir_all(&temp_dir)
        .await
        .map_err(|e| nova_core::Error::Internal(format!("failed to create directory: {e}")))?;

    let mut storage_path: Option<String> = None;
    let mut file_size: u64 = 0;
    let mut hasher = {
        use sha2::{Digest, Sha256};
        Sha256::new()
    };

    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| nova_core::Error::Validation(format!("multipart error: {e}")))?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "file" => {
                file_name = field.file_name().map(|s| s.to_string());
                let fname = file_name
                    .clone()
                    .ok_or_else(|| nova_core::Error::Validation("no filename".into()))?;
                // Sanitize filename: strip directory components to prevent path traversal
                let safe_fname = std::path::Path::new(&fname)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .ok_or_else(|| nova_core::Error::Validation("invalid filename".into()))?;
                let path = format!("{}/{}", temp_dir, safe_fname);

                // Stream chunks to disk instead of buffering entire file
                use tokio::io::AsyncWriteExt;
                let mut out = tokio::fs::File::create(&path).await.map_err(|e| {
                    nova_core::Error::Internal(format!("failed to create file: {e}"))
                })?;

                while let Some(chunk) = field.chunk().await.map_err(|e| {
                    nova_core::Error::Validation(format!("failed to read chunk: {e}"))
                })? {
                    file_size += chunk.len() as u64;

                    // Enforce 200MB limit during streaming
                    if file_size > 200 * 1024 * 1024 {
                        // Clean up partial file
                        drop(out);
                        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
                        return Err(nova_core::Error::Validation(
                            "File exceeds 200MB limit".into(),
                        )
                        .into());
                    }

                    use sha2::Digest;
                    hasher.update(&chunk);
                    out.write_all(&chunk).await.map_err(|e| {
                        nova_core::Error::Internal(format!("failed to write chunk: {e}"))
                    })?;
                }
                out.flush().await.map_err(|e| {
                    nova_core::Error::Internal(format!("failed to flush file: {e}"))
                })?;

                storage_path = Some(path);
            }
            "library_id" => {
                let text = field.text().await.map_err(|e| {
                    nova_core::Error::Validation(format!("failed to read library_id: {e}"))
                })?;
                library_id = Uuid::parse_str(&text).ok();
            }
            _ => {}
        }
    }

    let storage_path =
        storage_path.ok_or_else(|| nova_core::Error::Validation("no file provided".into()))?;
    let file_name = file_name.ok_or_else(|| nova_core::Error::Validation("no filename".into()))?;
    library_id = match resolve_upload_library_id(&state, &auth, library_id).await {
        Ok(id) => id,
        Err(err) => {
            let _ = tokio::fs::remove_dir_all(&temp_dir).await;
            return Err(err);
        }
    };

    // Finalize hash
    let file_hash = {
        use sha2::Digest;
        hex::encode(hasher.finalize())
    };

    // Determine format
    let extension = file_name.rsplit('.').next().unwrap_or("").to_lowercase();
    let format = BookFormat::from_extension(&extension).ok_or_else(|| {
        // Clean up uploaded file on format error
        let path = storage_path.clone();
        tokio::spawn(async move {
            let _ = tokio::fs::remove_dir_all(path).await;
        });
        nova_core::Error::Validation(format!("unsupported format: {extension}"))
    })?;

    // Check for duplicates. Persist the skipped source so the exact match is
    // reviewable instead of disappearing into a generic validation error.
    let matched_book_id =
        match duplicate_in_upload_scope(&state, &auth, &file_hash, library_id).await {
            Ok(matched_book_id) => matched_book_id,
            Err(error) => {
                let _ = tokio::fs::remove_dir_all(&temp_dir).await;
                return Err(error);
            }
        };
    if let Some(matched_book_id) = matched_book_id {
        let discovery = match state
            .duplicates
            .record_exact_file_discovery(RecordExactFileDiscovery {
                matched_book_id,
                source: ExactFileDiscoverySource::Upload,
                source_path: &file_name,
                file_hash: &file_hash,
                file_size_bytes: i64::try_from(file_size).unwrap_or(i64::MAX),
                discovered_by: user_id,
            })
            .await
        {
            Ok(discovery) => discovery,
            Err(error) => {
                let _ = tokio::fs::remove_dir_all(&temp_dir).await;
                return Err(error.into());
            }
        };
        // Clean up the uploaded file since it's a duplicate
        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
        return Err(nova_core::Error::Duplicate {
            entity: "book",
            detail: format!(
                "file matches existing book {}; discovery {}",
                discovery.matched_book_id, discovery.id
            ),
        }
        .into());
    }

    // Create book record in DB
    let title = std::path::Path::new(&file_name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled")
        .to_string();

    let book = state
        .books
        .create(&CreateBook {
            title,
            author: None,
            language: Language::Unknown,
            format,
            file_path: storage_path,
            file_hash,
            file_size_bytes: file_size as i64,
            library_id,
        })
        .await?;

    // TODO: Re-enable webhooks
    // crate::routes::webhooks::dispatch_event(&state, "book.created", serde_json::json!({
    //     "book_id": book.id,
    //     "title": book.title,
    //     "format": book.format,
    // })).await;

    Ok(Json(serde_json::json!({
        "id": book.id,
        "title": book.title,
        "format": book.format,
        "size_bytes": book.file_size_bytes,
        "status": book.status,
    })))
}

#[derive(Debug, Deserialize)]
struct UpdateBookRequest {
    title: Option<String>,
    author: Option<String>,
    description: Option<String>,
    reading_status: Option<ReadingStatus>,
    language: Option<Language>,
    genres: Option<Vec<String>>,
    tags: Option<Vec<String>>,
    rating: Option<i16>,
}

/// Update a book's basic fields.
async fn update_book(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateBookRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_book_access(&state, &auth, id, LibraryAccess::Write).await?;
    validate_book_rating(body.rating)?;
    let existing = state.books.get(id).await?;
    let mut metadata = existing.metadata.clone();
    let metadata_update = if body.genres.is_some() || body.tags.is_some() {
        if let Some(genres) = body.genres {
            metadata.genres = genres;
        }
        if let Some(tags) = body.tags {
            metadata.tags = tags;
        }
        Some(
            serde_json::to_value(&metadata)
                .map_err(|e| ApiError::Internal(format!("serialize metadata: {e}")))?,
        )
    } else {
        None
    };

    sqlx::query(
        r#"
        UPDATE books SET
            title = COALESCE($2, title),
            author = COALESCE($3, author),
            description = COALESCE($4, description),
            reading_status = COALESCE($5, reading_status),
            user_rating = COALESCE($6, user_rating),
            language = COALESCE($7, language),
            metadata = COALESCE($8, metadata),
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(&body.title)
    .bind(&body.author)
    .bind(&body.description)
    .bind(&body.reading_status)
    .bind(body.rating)
    .bind(&body.language)
    .bind(&metadata_update)
    .execute(&state.db)
    .await?;

    let book = state.books.get(id).await?;
    let mut val = serde_json::to_value(&book).unwrap_or_default();
    if let Some(obj) = val.as_object_mut() {
        let rating: Option<i16> = sqlx::query_scalar("SELECT user_rating FROM books WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten();
        obj.insert("rating".into(), serde_json::json!(rating));
    }
    Ok(Json(val))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_book_request_accepts_rating_payload() {
        let body: UpdateBookRequest = serde_json::from_value(serde_json::json!({
            "title": "New title",
            "rating": 4,
            "language": "zh",
            "genres": ["玄幻"],
            "tags": ["成长"]
        }))
        .expect("rating payload should deserialize");

        assert_eq!(body.rating, Some(4));
        assert_eq!(body.language, Some(Language::Chinese));
        assert_eq!(body.genres, Some(vec!["玄幻".to_string()]));
        assert_eq!(body.tags, Some(vec!["成长".to_string()]));
    }

    #[test]
    fn book_progress_list_query_is_scoped_to_current_user() {
        assert!(book_progress_map_sql().contains("user_id = $2"));
        assert!(!book_progress_map_sql().contains("user_id IS NULL"));
    }

    #[test]
    fn book_progress_detail_query_is_scoped_to_current_user() {
        assert!(book_progress_sql().contains("user_id = $2"));
        assert!(!book_progress_sql().contains("user_id IS NULL"));
    }

    #[test]
    fn cover_library_candidates_query_matches_filename_and_api_path() {
        let sql = cover_library_candidates_sql();

        assert!(sql.contains("FROM books"));
        assert!(sql.contains("FROM series"));
        assert!(sql.contains("FROM libraries"));
        assert!(sql.contains("cover_path = $1"));
        assert!(sql.contains("metadata->>'cover_path' = $1"));
        assert!(sql.contains("cover_path = CONCAT('/api/covers/', $1)"));
    }

    #[test]
    fn cover_content_type_supports_generated_svg_covers() {
        assert_eq!(cover_content_type("book.png"), "image/png");
        assert_eq!(cover_content_type("book.webp"), "image/webp");
        assert_eq!(
            cover_content_type("book.svg"),
            "image/svg+xml; charset=utf-8"
        );
        assert_eq!(cover_content_type("book.jpg"), "image/jpeg");
    }
}

/// Delete a book (soft delete → archived).
async fn delete_book(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_book_access(&state, &auth, id, LibraryAccess::Write).await?;
    state.books.delete(id).await?;
    Ok(Json(
        serde_json::json!({ "deleted": true, "id": id.to_string() }),
    ))
}

/// Update book metadata (genres, tags, series, etc.).
async fn update_metadata(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(metadata): Json<BookMetadata>,
) -> ApiResult<Json<Book>> {
    ensure_book_access(&state, &auth, id, LibraryAccess::Write).await?;
    let book = state.books.update_metadata(id, &metadata).await?;
    Ok(Json(book))
}

#[derive(Debug, Deserialize)]
struct ScrapeMetadataRequest {
    /// Source to scrape from: "google_books", "bangumi", "douban"
    source: String,
    /// Optional query override (defaults to book title)
    query: Option<String>,
}

/// Scrape metadata from external sources (Google Books, Bangumi, 豆瓣).
async fn scrape_metadata(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ScrapeMetadataRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_book_access(&state, &auth, id, LibraryAccess::Read).await?;
    let book = state.books.get(id).await?;
    let search_query = body.query.unwrap_or_else(|| book.title.clone());

    let result = match body.source.as_str() {
        "google_books" => {
            // Google Books API (free, no key required for basic search)
            let url = format!(
                "https://www.googleapis.com/books/v1/volumes?q={}&maxResults=5",
                urlencoding::encode(&search_query)
            );
            let resp = state
                .http_client
                .get(&url)
                .timeout(std::time::Duration::from_secs(10))
                .send()
                .await;

            match resp {
                Ok(r) if r.status().is_success() => {
                    let data: serde_json::Value = r.json().await.unwrap_or_default();
                    let items = data
                        .get("items")
                        .and_then(|i| i.as_array())
                        .cloned()
                        .unwrap_or_default();
                    let results: Vec<serde_json::Value> = items
                        .iter()
                        .take(5)
                        .map(|item| {
                            let info = &item["volumeInfo"];
                            serde_json::json!({
                                "title": info["title"],
                                "authors": info["authors"],
                                "publisher": info["publisher"],
                                "published_date": info["publishedDate"],
                                "description": info["description"],
                                "isbn": info["industryIdentifiers"],
                                "page_count": info["pageCount"],
                                "categories": info["categories"],
                                "cover_url": info["imageLinks"]["thumbnail"],
                                "language": info["language"],
                            })
                        })
                        .collect();
                    serde_json::json!({ "source": "google_books", "results": results })
                }
                _ => {
                    serde_json::json!({ "source": "google_books", "results": [], "error": "API request failed" })
                }
            }
        }
        "bangumi" => {
            // Bangumi API (anime/manga/novel database)
            let url = format!(
                "https://api.bgm.tv/search/subject/{}?type=1&responseGroup=small&max_results=5",
                urlencoding::encode(&search_query)
            );
            let resp = state
                .http_client
                .get(&url)
                .header("User-Agent", "NovaReader/1.0")
                .timeout(std::time::Duration::from_secs(10))
                .send()
                .await;

            match resp {
                Ok(r) if r.status().is_success() => {
                    let data: serde_json::Value = r.json().await.unwrap_or_default();
                    let items = data
                        .get("list")
                        .and_then(|l| l.as_array())
                        .cloned()
                        .unwrap_or_default();
                    let results: Vec<serde_json::Value> = items
                        .iter()
                        .take(5)
                        .map(|item| {
                            serde_json::json!({
                                "title": item["name"],
                                "title_cn": item["name_cn"],
                                "summary": item["summary"],
                                "bangumi_id": item["id"],
                                "cover_url": item["images"]["large"],
                                "air_date": item["air_date"],
                                "score": item["rating"]["score"],
                            })
                        })
                        .collect();
                    serde_json::json!({ "source": "bangumi", "results": results })
                }
                _ => {
                    serde_json::json!({ "source": "bangumi", "results": [], "error": "API request failed" })
                }
            }
        }
        _ => {
            return Err(nova_core::Error::Validation(format!(
                "Unsupported source: {}. Use 'google_books' or 'bangumi'",
                body.source
            ))
            .into());
        }
    };

    Ok(Json(result))
}

// ─── Format Conversion ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ConvertFormatRequest {
    /// Target format: "epub", "pdf", "txt", "mobi", "azw3"
    target_format: String,
    /// Optional conversion options
    options: Option<ConvertOptions>,
}

#[derive(Debug, Deserialize)]
struct ConvertOptions {
    /// Include cover in output
    include_cover: Option<bool>,
    /// Custom CSS for EPUB output
    custom_css: Option<String>,
    /// Page size for PDF: "a4", "a5", "letter"
    page_size: Option<String>,
    /// Font size for PDF/EPUB (in pt)
    font_size: Option<u32>,
}

/// Convert a book to a different format using Pandoc/Calibre backend.
async fn convert_format(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ConvertFormatRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_book_access(&state, &auth, id, LibraryAccess::Read).await?;
    let book = state.books.get(id).await?;

    let supported_targets = ["epub", "pdf", "txt", "mobi", "azw3", "docx"];
    if !supported_targets.contains(&body.target_format.as_str()) {
        return Err(nova_core::Error::Validation(format!(
            "Unsupported target format: {}. Supported: {:?}",
            body.target_format, supported_targets
        ))
        .into());
    }

    let source_path = &book.file_path;
    let output_dir = format!("{}/converted", state.config.data_dir);
    let output_filename = format!(
        "{}_{}.{}",
        id,
        book.title.replace(' ', "_"),
        body.target_format
    );
    let output_path = format!("{}/{}", output_dir, output_filename);
    let cover_file = body
        .options
        .as_ref()
        .and_then(|opts| opts.include_cover)
        .filter(|include| *include)
        .and_then(|_| {
            resolve_cover_file(&state.config.data_dir, book.metadata.cover_path.as_deref())
        })
        .filter(|path| path.exists());
    let cover_included = cover_file.is_some();

    // Build conversion command (Pandoc for most formats, ebook-convert for mobi/azw3)
    let mut cmd_args: Vec<String> = Vec::new();

    let converter = if ["mobi", "azw3"].contains(&body.target_format.as_str()) {
        // Use Calibre's ebook-convert
        cmd_args.push(source_path.clone());
        cmd_args.push(output_path.clone());
        if let Some(ref opts) = body.options {
            if let Some(size) = opts.font_size {
                cmd_args.push(format!("--base-font-size={}", size));
            }
            if let Some(ref cover) = cover_file {
                cmd_args.push(format!("--cover={}", cover.display()));
            }
        }
        "ebook-convert"
    } else {
        // Use Pandoc
        cmd_args.push("-f".into());
        cmd_args.push(detect_input_format(source_path));
        cmd_args.push("-t".into());
        cmd_args.push(body.target_format.clone());
        cmd_args.push("-o".into());
        cmd_args.push(output_path.clone());
        cmd_args.push(source_path.clone());

        if let Some(ref opts) = body.options {
            if let Some(ref page_size) = opts.page_size {
                cmd_args.push(format!("--variable=papersize:{}", page_size));
            }
            if let Some(ref css) = opts.custom_css {
                cmd_args.push(format!("--css={}", css));
            }
            if body.target_format == "epub" {
                if let Some(ref cover) = cover_file {
                    cmd_args.push(format!("--epub-cover-image={}", cover.display()));
                }
            }
        }
        "pandoc"
    };

    // Ensure output directory exists
    tokio::fs::create_dir_all(&output_dir).await.ok();

    // Spawn conversion process
    let result = tokio::process::Command::new(converter)
        .args(&cmd_args)
        .output()
        .await;

    match result {
        Ok(output) if output.status.success() => Ok(Json(serde_json::json!({
            "book_id": id.to_string(),
            "source_format": detect_input_format(source_path),
            "target_format": body.target_format,
            "output_path": output_path,
            "status": "completed",
            "cover_included": cover_included,
            "download_url": format!("/api/converted/{}", output_filename),
        }))),
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Ok(Json(serde_json::json!({
                "book_id": id.to_string(),
                "status": "failed",
                "error": stderr.to_string(),
            })))
        }
        Err(e) => Ok(Json(serde_json::json!({
            "book_id": id.to_string(),
            "status": "failed",
            "error": format!("Converter not found: {}. Install pandoc or calibre.", e),
        }))),
    }
}

fn detect_input_format(path: &str) -> String {
    let ext = path.rsplit('.').next().unwrap_or("txt").to_lowercase();
    match ext.as_str() {
        "epub" => "epub".into(),
        "txt" | "text" => "plain".into(),
        "html" | "htm" => "html".into(),
        "md" | "markdown" => "markdown".into(),
        "docx" => "docx".into(),
        "pdf" => "pdf".into(),
        _ => "plain".into(),
    }
}

// ─── Send to Device ───────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct SendToDeviceRequest {
    /// Delivery method: "kindle_email", "koreader_sync", "webdav"
    method: String,
    /// Target email (for Kindle) or URL (for WebDAV/KOReader)
    target: String,
    /// Convert to format before sending (optional)
    convert_to: Option<String>,
}

/// Send a book to a device (Kindle via email, KOReader sync, or WebDAV).
async fn send_to_device(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<SendToDeviceRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_book_access(&state, &auth, id, LibraryAccess::Read).await?;
    let book = state.books.get(id).await?;

    match body.method.as_str() {
        "kindle_email" => {
            // Validate Kindle email format
            if !body.target.ends_with("@kindle.com") && !body.target.ends_with("@free.kindle.com") {
                return Err(nova_core::Error::Validation(
                    "Target must be a valid @kindle.com or @free.kindle.com address".into(),
                )
                .into());
            }

            // Queue email send with attachment
            // In production this would use lettre/SMTP
            Ok(Json(serde_json::json!({
                "book_id": id.to_string(),
                "method": "kindle_email",
                "target": body.target,
                "status": "queued",
                "message": format!("'{}' will be sent to {}", book.title, body.target),
                "format": body.convert_to.unwrap_or_else(|| "epub".into()),
            })))
        }
        "webdav" => {
            // Upload to WebDAV server
            if !body.target.starts_with("http://") && !body.target.starts_with("https://") {
                return Err(nova_core::Error::Validation(
                    "WebDAV target must be a valid HTTP(S) URL".into(),
                )
                .into());
            }

            Ok(Json(serde_json::json!({
                "book_id": id.to_string(),
                "method": "webdav",
                "target": body.target,
                "status": "queued",
                "message": format!("'{}' will be uploaded to WebDAV", book.title),
            })))
        }
        "koreader_sync" => Ok(Json(serde_json::json!({
            "book_id": id.to_string(),
            "method": "koreader_sync",
            "target": body.target,
            "status": "queued",
            "message": format!("'{}' will be synced via KOReader progress sync", book.title),
        }))),
        _ => Err(nova_core::Error::Validation(format!(
            "Unsupported method: {}. Use 'kindle_email', 'webdav', or 'koreader_sync'",
            body.method
        ))
        .into()),
    }
}

/// Re-trigger processing pipeline for a book.
async fn reprocess_book(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_book_access(&state, &auth, id, LibraryAccess::Write).await?;
    let book = state.books.get(id).await?;
    state.books.update_status(id, BookStatus::Pending).await?;
    if let Ok(user_id) = auth_user_id(&auth) {
        crate::routes::notifications::emit(
            &state.db,
            user_id,
            crate::routes::notifications::NewNotification::new(
                "info",
                "book",
                format!("重新处理已排队 · {}", book.title),
            )
            .body("系统会重新解析章节、封面和搜索索引")
            .link(format!("/library/{}", id))
            .book(id)
            .metadata(serde_json::json!({ "book_id": id.to_string() })),
        )
        .await;
    }
    Ok(Json(serde_json::json!({
        "id": id.to_string(),
        "status": "pending",
        "message": "Reprocessing queued"
    })))
}

/// Get recently added books.
async fn recent_books(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(params): Query<LimitQuery>,
) -> ApiResult<Json<Vec<serde_json::Value>>> {
    let limit = params.limit.unwrap_or(10).min(50);
    let library_ids = visible_library_ids(&state, &auth, LibraryAccess::Read).await?;

    #[derive(sqlx::FromRow)]
    struct RecentRow {
        id: uuid::Uuid,
        title: String,
        author: Option<String>,
        format: String,
        reading_status: String,
        created_at: chrono::DateTime<chrono::Utc>,
    }

    let rows = if let Some(ids) = library_ids {
        sqlx::query_as::<_, RecentRow>(
            r#"
            SELECT id, title, author, format::text as format,
                   reading_status::text as reading_status, created_at
            FROM books
            WHERE status != 'archived' AND library_id = ANY($2)
            ORDER BY created_at DESC
            LIMIT $1
            "#,
        )
        .bind(limit)
        .bind(&ids)
        .fetch_all(&state.db)
        .await?
    } else {
        sqlx::query_as::<_, RecentRow>(
            r#"
            SELECT id, title, author, format::text as format,
                   reading_status::text as reading_status, created_at
            FROM books
            WHERE status != 'archived'
            ORDER BY created_at DESC
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(&state.db)
        .await?
    };

    let result: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "title": r.title,
                "author": r.author,
                "format": r.format,
                "reading_status": r.reading_status,
                "created_at": r.created_at,
            })
        })
        .collect();

    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
struct LimitQuery {
    limit: Option<i64>,
}

async fn scoped_book_count(
    state: &AppState,
    status: Option<BookStatus>,
    library_ids: Option<&[Uuid]>,
) -> ApiResult<i64> {
    match (status, library_ids) {
        (Some(status), Some(ids)) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM books WHERE status = $1::book_status AND library_id = ANY($2)",
        )
        .bind(status)
        .bind(ids)
        .fetch_one(&state.db)
        .await
        .map_err(ApiError::from),
        (Some(status), None) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM books WHERE status = $1::book_status")
                .bind(status)
                .fetch_one(&state.db)
                .await
                .map_err(ApiError::from)
        }
        (None, Some(ids)) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM books WHERE status != 'archived' AND library_id = ANY($1)",
        )
        .bind(ids)
        .fetch_one(&state.db)
        .await
        .map_err(ApiError::from),
        (None, None) => sqlx::query_scalar("SELECT COUNT(*) FROM books WHERE status != 'archived'")
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::from),
    }
}

/// Book statistics (counts by status).
async fn book_stats(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    let library_ids = visible_library_ids(&state, &auth, LibraryAccess::Read).await?;
    let (total, ready, processing, pending, failed) = if let Some(ids) = library_ids {
        let total = scoped_book_count(&state, None, Some(&ids)).await?;
        let ready = scoped_book_count(&state, Some(BookStatus::Ready), Some(&ids)).await?;
        let processing =
            scoped_book_count(&state, Some(BookStatus::Processing), Some(&ids)).await?;
        let pending = scoped_book_count(&state, Some(BookStatus::Pending), Some(&ids)).await?;
        let failed = scoped_book_count(&state, Some(BookStatus::Failed), Some(&ids)).await?;
        (total, ready, processing, pending, failed)
    } else {
        let total = scoped_book_count(&state, None, None).await?;
        let ready = state.books.count_by_status(BookStatus::Ready).await?;
        let processing = state.books.count_by_status(BookStatus::Processing).await?;
        let pending = state.books.count_by_status(BookStatus::Pending).await?;
        let failed = state.books.count_by_status(BookStatus::Failed).await?;
        (total, ready, processing, pending, failed)
    };

    Ok(Json(serde_json::json!({
        "total": total,
        "ready": ready,
        "processing": processing,
        "pending": pending,
        "failed": failed,
    })))
}

/// Get all tags with counts across the library.
async fn get_book_tags(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<Json<Vec<serde_json::Value>>> {
    let library_ids = visible_library_ids(&state, &auth, LibraryAccess::Read).await?;
    let rows: Vec<(String, i64)> = if let Some(ids) = library_ids {
        sqlx::query_as(
            r#"
            SELECT tag::text, COUNT(*)::bigint as count
            FROM books,
                 jsonb_array_elements_text(COALESCE(metadata->'tags', '[]'::jsonb)) AS tag
            WHERE status != 'archived' AND library_id = ANY($1)
            GROUP BY tag
            ORDER BY count DESC
            LIMIT 100
            "#,
        )
        .bind(&ids)
        .fetch_all(&state.db)
        .await?
    } else {
        sqlx::query_as(
            r#"
            SELECT tag::text, COUNT(*)::bigint as count
            FROM books,
                 jsonb_array_elements_text(COALESCE(metadata->'tags', '[]'::jsonb)) AS tag
            WHERE status != 'archived'
            GROUP BY tag
            ORDER BY count DESC
            LIMIT 100
            "#,
        )
        .fetch_all(&state.db)
        .await?
    };

    let result: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|(tag, count)| serde_json::json!({ "tag": tag, "count": count }))
        .collect();

    Ok(Json(result))
}

/// Download a book file.
async fn download_book(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<impl axum::response::IntoResponse, crate::error::ApiError> {
    ensure_book_access(&state, &auth, id, LibraryAccess::Read).await?;
    let book = state.books.get(id).await?;

    let file_path = std::path::Path::new(&book.file_path);
    if !file_path.exists() {
        return Err(crate::error::ApiError::NotFound(
            "File not found on disk".to_string(),
        ));
    }

    let body = tokio::fs::read(file_path)
        .await
        .map_err(|e| crate::error::ApiError::Internal(format!("Failed to read file: {}", e)))?;

    let content_type = match book.format {
        BookFormat::Epub => "application/epub+zip",
        BookFormat::Pdf => "application/pdf",
        BookFormat::Txt => "text/plain; charset=utf-8",
        BookFormat::Markdown => "text/markdown; charset=utf-8",
        BookFormat::Doc => "application/msword",
        BookFormat::Docx => {
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        }
        _ => "application/octet-stream",
    };

    let filename = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("book");

    let disposition = format!("attachment; filename=\"{}\"", filename);

    Ok((
        [
            (axum::http::header::CONTENT_TYPE, content_type.to_string()),
            (axum::http::header::CONTENT_DISPOSITION, disposition),
        ],
        body,
    ))
}

/// Serve a cover image.
async fn serve_cover(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(filename): Path<String>,
) -> Result<impl axum::response::IntoResponse, crate::error::ApiError> {
    // Sanitize: strip any directory components from filename
    let safe_filename = std::path::Path::new(&filename)
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| crate::error::ApiError::NotFound("Invalid filename".to_string()))?;

    let candidate_library_ids = sqlx::query_scalar::<_, Uuid>(cover_library_candidates_sql())
        .bind(safe_filename)
        .fetch_all(&state.db)
        .await
        .map_err(crate::error::ApiError::from)?;

    if candidate_library_ids.is_empty() {
        return Err(crate::error::ApiError::NotFound(
            "Cover not found".to_string(),
        ));
    }

    if let Some(visible_library_ids) =
        visible_library_ids(&state, &auth, LibraryAccess::Read).await?
    {
        let allowed = candidate_library_ids
            .iter()
            .any(|library_id| visible_library_ids.contains(library_id));
        if !allowed {
            return Err(crate::error::ApiError::forbidden());
        }
    }

    let covers_dir = std::path::Path::new(&state.config.data_dir).join("covers");
    let cover_path = covers_dir.join(safe_filename);

    // Security: ensure the resolved path stays within covers_dir
    let canonical = cover_path
        .canonicalize()
        .map_err(|_| crate::error::ApiError::NotFound("Cover not found".to_string()))?;
    if !canonical.starts_with(
        covers_dir
            .canonicalize()
            .unwrap_or(covers_dir.to_path_buf()),
    ) {
        return Err(crate::error::ApiError::NotFound("Invalid path".to_string()));
    }

    let body = tokio::fs::read(&canonical)
        .await
        .map_err(|_| crate::error::ApiError::NotFound("Cover not found".to_string()))?;

    let content_type = cover_content_type(safe_filename);

    Ok((
        [(axum::http::header::CONTENT_TYPE, content_type.to_string())],
        body,
    ))
}

// ─── Custom Metadata Fields ──────────────────────────────────────────────────

/// Get user-defined custom fields for a book (stored as JSONB).
async fn get_custom_fields(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_book_access(&state, &auth, id, LibraryAccess::Read).await?;
    let row: Option<(Option<serde_json::Value>,)> =
        sqlx::query_as("SELECT custom_fields FROM books WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?;

    let fields = row.and_then(|(f,)| f).unwrap_or(serde_json::json!({}));

    Ok(Json(fields))
}

/// Update custom metadata fields (user-defined key-value pairs).
/// Merges with existing fields (partial update).
async fn update_custom_fields(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(fields): Json<serde_json::Value>,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_book_access(&state, &auth, id, LibraryAccess::Write).await?;
    let row: (Option<serde_json::Value>,) = sqlx::query_as(
        r#"UPDATE books
           SET custom_fields = COALESCE(custom_fields, '{}'::jsonb) || $2::jsonb,
               updated_at = NOW()
           WHERE id = $1
           RETURNING custom_fields"#,
    )
    .bind(id)
    .bind(&fields)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(row.0.unwrap_or(serde_json::json!({}))))
}
