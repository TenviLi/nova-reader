use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    routing::{get, post, put},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use nova_core::domain::dedup_discovery::ExactFileDiscoverySource;
use nova_core::domain::task::TaskKind;
use nova_core::repo::library_repo::{
    CreateLibrary, LibraryFilter, LibraryRepository, UpdateLibrary,
};

use crate::access::{auth_user_id, ensure_library_access, visible_library_ids, LibraryAccess};
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AdminUser, AuthUser};
use crate::repo::pg_book::BookFileRecord;
use crate::repo::pg_chapter::ImportedChapter;
use crate::repo::pg_exact_file_discovery::RecordExactFileDiscovery;
use crate::state::AppState;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/libraries", get(list_libraries).post(create_library))
        .route(
            "/libraries/{id}",
            get(get_library).put(update_library).delete(delete_library),
        )
        .route("/libraries/{id}/scan", post(trigger_scan))
        .route("/libraries/{id}/scan-status", get(get_scan_status))
        .route("/libraries/{id}/analyze", post(trigger_analyze))
        .route(
            "/libraries/{id}/maintenance/{action}",
            post(queue_maintenance_task),
        )
        .route(
            "/libraries/{id}/permissions",
            get(get_library_permissions).put(set_library_permissions),
        )
        .route(
            "/libraries/{id}/features",
            get(get_library_features).put(set_library_features),
        )
        .route("/libraries/{id}/series", get(list_library_series))
        .route("/libraries/series", get(list_all_series))
        .route("/libraries/series/{id}", get(get_series))
        .route("/libraries/series/{id}/books", get(get_series_books))
        .route("/libraries/series/{id}/reorder", put(reorder_series_books))
        .route(
            "/libraries/series/{id}/metadata",
            put(update_series_metadata),
        )
}

#[derive(Debug, Deserialize)]
struct CreateLibraryRequest {
    name: String,
    root_path: String,
    description: Option<String>,
    auto_scan: Option<bool>,
    scan_interval_secs: Option<i64>,
    include_extensions: Option<Vec<String>>,
    exclude_patterns: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct UpdateLibraryRequest {
    name: Option<String>,
    description: Option<String>,
    auto_scan: Option<bool>,
    scan_interval_secs: Option<i64>,
    include_extensions: Option<Vec<String>>,
    exclude_patterns: Option<Vec<String>>,
}

/// List libraries visible to the current user.
/// Admins see all libraries. Non-admins only see libraries they have `can_read` permission for.
async fn list_libraries(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    match visible_library_ids(&state, &auth, LibraryAccess::Read).await? {
        None => {
            let filter = LibraryFilter {
                search: None,
                page: 1,
                per_page: 100,
            };
            let result = state.libraries.list(&filter).await?;
            return Ok(Json(serde_json::json!({
                "data": result.data,
                "total": result.total,
            })));
        }
        Some(ids) if ids.is_empty() => Ok(Json(serde_json::json!({
            "data": [],
            "total": 0,
        }))),
        Some(ids) => {
            let rows = sqlx::query_as::<_, (serde_json::Value,)>(
                "SELECT row_to_json(l.*)
                 FROM libraries l
                 WHERE l.id = ANY($1)
                 ORDER BY l.name",
            )
            .bind(&ids)
            .fetch_all(&state.db)
            .await
            .map_err(ApiError::from)?;

            let data: Vec<serde_json::Value> = rows.into_iter().map(|(r,)| r).collect();
            let total = data.len();

            Ok(Json(serde_json::json!({
                "data": data,
                "total": total,
            })))
        }
    }
}

/// Create a new library (monitored directory).
async fn create_library(
    _admin: AdminUser,
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateLibraryRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    // Auto-create the root directory if it doesn't exist
    if !body.root_path.is_empty() {
        tokio::fs::create_dir_all(&body.root_path)
            .await
            .map_err(|e| {
                ApiError::Internal(format!(
                    "Failed to create library directory '{}': {}",
                    body.root_path, e
                ))
            })?;
    }

    let input = CreateLibrary {
        name: body.name,
        root_path: body.root_path,
        description: body.description,
        auto_scan: body.auto_scan.unwrap_or(true),
        scan_interval_secs: body.scan_interval_secs.unwrap_or(3600),
        include_extensions: body.include_extensions.unwrap_or_else(|| {
            vec![
                "txt".into(),
                "epub".into(),
                "pdf".into(),
                "docx".into(),
                "doc".into(),
                "md".into(),
                "html".into(),
            ]
        }),
        exclude_patterns: body.exclude_patterns.unwrap_or_default(),
    };

    let library = state.libraries.create(&input).await?;
    let payload = serde_json::to_value(&library)
        .map_err(|error| ApiError::internal(format!("serialize created library: {error}")))?;
    Ok(Json(payload))
}

/// Get library details with real-time stats aggregated from DB.
async fn get_library(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_library_access(&state, &auth, id, LibraryAccess::Read).await?;
    let library = state.libraries.get(id).await?;
    let mut json = serde_json::to_value(&library)
        .map_err(|error| ApiError::internal(format!("serialize library details: {error}")))?;

    // Aggregate live stats from books table
    let stats: Option<(i64, i64)> = sqlx::query_as(
        "SELECT COUNT(*)::bigint, COALESCE(SUM(file_size_bytes), 0)::bigint FROM books WHERE library_id = $1"
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();

    if let Some((book_count, total_size)) = stats {
        json["book_count"] = serde_json::json!(book_count);
        json["total_size_bytes"] = serde_json::json!(total_size);
    }

    // Series count
    let series_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*)::bigint FROM series WHERE library_id = $1")
            .bind(id)
            .fetch_one(&state.db)
            .await
            .unwrap_or(0);
    json["series_count"] = serde_json::json!(series_count);

    Ok(Json(json))
}

/// Update library settings.
async fn update_library(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateLibraryRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_library_access(&state, &auth, id, LibraryAccess::Manage).await?;
    let input = UpdateLibrary {
        name: body.name,
        description: body.description,
        auto_scan: body.auto_scan,
        scan_interval_secs: body.scan_interval_secs,
        include_extensions: body.include_extensions,
        exclude_patterns: body.exclude_patterns,
    };

    let library = state.libraries.update(id, &input).await?;
    let payload = serde_json::to_value(&library)
        .map_err(|error| ApiError::internal(format!("serialize updated library: {error}")))?;
    Ok(Json(payload))
}

/// Delete a library (does NOT delete files on disk).
async fn delete_library(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_library_access(&state, &auth, id, LibraryAccess::Manage).await?;
    state.libraries.delete(id).await?;
    Ok(Json(serde_json::json!({
        "message": "Library removed. Files on disk are untouched."
    })))
}

async fn import_paths_identify_same_source(
    scanned_path: &std::path::Path,
    stored_path: &str,
) -> bool {
    let stored_path = std::path::Path::new(stored_path);
    if scanned_path == stored_path {
        return true;
    }

    let (scanned, stored) = tokio::join!(
        tokio::fs::canonicalize(scanned_path),
        tokio::fs::canonicalize(stored_path)
    );
    matches!((scanned, stored), (Ok(scanned), Ok(stored)) if scanned == stored)
}

/// Trigger a manual scan of the library.
/// Walks the root_path, finds book files, and imports any new ones.
async fn trigger_scan(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    use nova_core::domain::book::{BookFormat, Language};
    use nova_core::repo::book_repo::BookRepository;

    ensure_library_access(&state, &auth, id, LibraryAccess::Manage).await?;
    let user_id = auth_user_id(&auth)?;
    let library = state.libraries.get(id).await?;
    let root_path = std::path::Path::new(&library.root_path);

    if !root_path.exists() {
        return Err(ApiError::bad_request(&format!(
            "Library path does not exist: {}",
            library.root_path
        )));
    }

    // Get existing file hashes to skip duplicates
    let existing_files = state.books.list_file_records_by_library(id).await?;

    let mut hash_matches = std::collections::HashMap::<String, Vec<BookFileRecord>>::new();
    for record in existing_files {
        hash_matches
            .entry(record.file_hash.clone())
            .or_default()
            .push(record);
    }

    // Supported extensions from library config
    let extensions: Vec<String> = sqlx::query_scalar(
        "SELECT jsonb_array_elements_text(COALESCE(include_extensions, '[]'::jsonb))
             FROM libraries WHERE id = $1",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_else(|_| vec!["txt".into(), "epub".into(), "pdf".into(), "md".into()]);

    // Walk directory recursively
    let mut new_books = 0u32;
    let mut new_book_ids = Vec::new();
    let mut skipped = 0u32;
    let mut duplicate_discoveries = Vec::new();
    let mut errors = Vec::new();

    let mut stack = vec![root_path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut entries = match tokio::fs::read_dir(&dir).await {
            Ok(e) => e,
            Err(e) => {
                errors.push(format!("{}: {}", dir.display(), e));
                continue;
            }
        };

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }

            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase())
                .unwrap_or_default();

            if !extensions.contains(&ext) {
                continue;
            }

            // Check format is supported
            let format = match BookFormat::from_extension(&ext) {
                Some(f) => f,
                None => continue,
            };

            // Compute file hash
            let file_data = match tokio::fs::read(&path).await {
                Ok(d) => d,
                Err(e) => {
                    errors.push(format!("{}: {}", path.display(), e));
                    continue;
                }
            };

            use sha2::{Digest, Sha256};
            let hash = hex::encode(Sha256::digest(&file_data));

            if let Some(matches) = hash_matches.get(&hash) {
                let mut known_source = false;
                for candidate in matches {
                    if import_paths_identify_same_source(&path, &candidate.file_path).await {
                        known_source = true;
                        break;
                    }
                }
                if known_source {
                    skipped += 1;
                    continue;
                }

                let matched_book_id = matches
                    .first()
                    .map(|candidate| candidate.book_id)
                    .ok_or_else(|| ApiError::internal("empty exact-file match set"))?;
                let source_path = path.to_string_lossy();
                let discovery = state
                    .duplicates
                    .record_exact_file_discovery(RecordExactFileDiscovery {
                        matched_book_id,
                        source: ExactFileDiscoverySource::LibraryScan,
                        source_path: source_path.as_ref(),
                        file_hash: &hash,
                        file_size_bytes: i64::try_from(file_data.len()).unwrap_or(i64::MAX),
                        discovered_by: user_id,
                    })
                    .await?;
                skipped += 1;
                if duplicate_discoveries.len() < 100 {
                    duplicate_discoveries.push(discovery);
                }
                continue;
            }

            // Create book record
            let raw_name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Untitled")
                .to_string();

            // Try to extract author from filename patterns:
            // "title - author", "【author】title", "[author]title"
            let (title, detected_author) = extract_title_author(&raw_name);

            let file_size = file_data.len() as i64;
            let file_path = path.to_string_lossy().to_string();

            let book_result = state
                .books
                .create(&nova_core::domain::book::CreateBook {
                    title: title.clone(),
                    author: detected_author.clone(),
                    language: Language::Unknown,
                    format: format.clone(),
                    file_path: file_path.clone(),
                    file_hash: hash.clone(),
                    file_size_bytes: file_size,
                    library_id: Some(id),
                })
                .await;

            match book_result {
                Ok(book) => {
                    hash_matches
                        .entry(hash.clone())
                        .or_default()
                        .push(BookFileRecord {
                            file_hash: hash,
                            book_id: book.id.into(),
                            file_path,
                        });

                    // ─── Inline Chapter Parsing ──────────────────────
                    // Parse text content and split into chapters immediately
                    let content = String::from_utf8_lossy(&file_data).to_string();
                    if content.is_empty() {
                        state
                            .books
                            .update_status(
                                book.id.into(),
                                nova_core::domain::book::BookStatus::Failed,
                            )
                            .await?;
                        errors.push(format!("{}: file has no text content", path.display()));
                        continue;
                    }
                    let chapters = split_into_chapters(&content);
                    if let Err(error) = state
                        .chapters
                        .insert_imported_and_mark_ready(book.id.into(), &chapters)
                        .await
                    {
                        state
                            .books
                            .update_status(
                                book.id.into(),
                                nova_core::domain::book::BookStatus::Failed,
                            )
                            .await?;
                        errors.push(format!(
                            "{}: failed to persist chapters atomically: {error}",
                            path.display()
                        ));
                        continue;
                    }
                    new_books += 1;
                    new_book_ids.push(book.id.into());

                    // Link author as person if detected
                    if let Some(ref author_name) = detected_author {
                        let _ = link_author_person(&state.db, book.id.into(), author_name).await;
                    }
                }
                Err(e) => errors.push(format!("{}: {}", path.display(), e)),
            }
        }
    }

    // Update library stats
    let _ = sqlx::query(
        "UPDATE libraries SET book_count = (SELECT COUNT(*) FROM books WHERE library_id = $1), last_scan_at = NOW(), scan_status = 'idle' WHERE id = $1"
    )
    .bind(id)
    .execute(&state.db)
    .await;

    // ─── Series Auto-Detection ──────────────────────────────────────
    // Group books by detected series name from filename patterns
    let series_created = auto_detect_series(&state, id).await;

    // Fingerprint newly imported content and refresh materialized duplicate
    // evidence asynchronously. enqueue_scan reuses an active run, so retries
    // and repeated filesystem scans do not create a task storm.
    let dedup_scan_id = if new_books > 0 {
        Some(crate::dedup::enqueue_incremental_scan(&state, id, user_id, new_book_ids).await?)
    } else {
        None
    };

    // ─── Notify the user that the scan finished ─────────────────────
    if let Ok(user_id) = Uuid::parse_str(&auth.id) {
        let level = if errors.is_empty() {
            "success"
        } else {
            "warning"
        };
        let body = format!(
            "新增 {new_books} 本 · 跳过 {skipped} 本重复 · 识别 {series_created} 个系列{}",
            if errors.is_empty() {
                String::new()
            } else {
                format!(" · {} 个错误", errors.len())
            }
        );
        crate::routes::notifications::emit(
            &state.db,
            user_id,
            crate::routes::notifications::NewNotification::new(
                level,
                "library",
                format!("书库扫描完成 · {}", library.name),
            )
            .body(body)
            .link(format!("/library?library={id}"))
            .metadata(serde_json::json!({
                "library_id": id.to_string(),
                "new_books": new_books,
                "skipped": skipped,
            })),
        )
        .await;
    }

    let duplicate_discoveries_truncated =
        usize::try_from(skipped).unwrap_or(usize::MAX) > duplicate_discoveries.len();

    Ok(Json(serde_json::json!({
        "message": "Scan completed",
        "library_id": id.to_string(),
        "new_books": new_books,
        "skipped_duplicates": skipped,
        "duplicate_discoveries": duplicate_discoveries,
        "duplicate_discoveries_truncated": duplicate_discoveries_truncated,
        "series_detected": series_created,
        "dedup_scan_id": dedup_scan_id,
        "errors": errors.len(),
        "error_details": errors.into_iter().take(10).collect::<Vec<_>>(),
    })))
}

/// Return the latest persisted scan status for a library.
async fn get_scan_status(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_library_access(&state, &auth, id, LibraryAccess::Read).await?;

    #[derive(sqlx::FromRow)]
    struct ScanStatusRow {
        scan_status: String,
        book_count: i64,
        last_scan_at: Option<chrono::DateTime<chrono::Utc>>,
        last_scan_duration_ms: Option<i64>,
    }

    let row = sqlx::query_as::<_, ScanStatusRow>(
        "SELECT
            scan_status,
            book_count::bigint AS book_count,
            last_scan_at,
            last_scan_duration_ms
         FROM libraries
         WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| ApiError::NotFound(format!("Library {} not found", id)))?;

    let status = match row.scan_status.as_str() {
        "scanning" | "processing" | "error" => row.scan_status.as_str(),
        _ if row.last_scan_at.is_some() => "complete",
        _ => "idle",
    };

    Ok(Json(serde_json::json!({
        "status": status,
        "total_files": row.book_count,
        "processed_files": row.book_count,
        "new_books": 0,
        "errors": [],
        "started_at": row.last_scan_at,
        "elapsed_seconds": row.last_scan_duration_ms.unwrap_or_default() / 1000,
    })))
}

/// Trigger AI analysis on all books in a library (entities, summaries, tags).
async fn trigger_analyze(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_library_access(&state, &auth, id, LibraryAccess::Manage).await?;
    // Verify library exists
    let library = state.libraries.get(id).await?;

    let stored_features: serde_json::Value =
        sqlx::query_scalar("SELECT COALESCE(features, '{}'::jsonb) FROM libraries WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await
            .map_err(ApiError::from)?
            .unwrap_or_else(|| serde_json::json!({}));
    let features = library_features_with_defaults(&stored_features);
    if !features["enable_ai"].as_bool().unwrap_or(true) {
        return Err(ApiError::bad_request("AI is disabled for this library"));
    }

    // Get books that haven't been analyzed yet (no entities)
    let unanalyzed_books: Vec<(Uuid, String)> = sqlx::query_as(
        r#"SELECT b.id, b.title FROM books b
           WHERE b.library_id = $1
           AND NOT EXISTS (SELECT 1 FROM entities e WHERE e.book_id = b.id)
           ORDER BY b.created_at DESC
           LIMIT 50"#,
    )
    .bind(id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let total = unanalyzed_books.len();

    // Queue analysis tasks through the current task queue contract.
    let mut queued = 0;
    for (book_id, _title) in &unanalyzed_books {
        let payload = serde_json::json!({
            "book_id": book_id.to_string(),
            "library_id": id.to_string(),
        });
        if state
            .task_queue
            .submit(TaskKind::DeepAnalysis, Some(*book_id), payload)
            .await
            .is_ok()
        {
            queued += 1;
        }
    }

    if let Ok(user_id) = Uuid::parse_str(&auth.id) {
        crate::routes::notifications::emit(
            &state.db,
            user_id,
            crate::routes::notifications::NewNotification::new(
                "info",
                "ai",
                format!("AI 分析已排队 · {}", library.name),
            )
            .body(format!("已为 {queued} 本未分析书籍排队实体提取与摘要任务"))
            .link(format!("/library?library={id}"))
            .metadata(serde_json::json!({ "library_id": id.to_string(), "tasks_queued": queued })),
        )
        .await;
    }

    Ok(Json(serde_json::json!({
        "message": "Analysis queued",
        "library_id": id.to_string(),
        "total_unanalyzed": total,
        "tasks_queued": queued,
    })))
}

struct MaintenanceTaskConfig {
    action: &'static str,
    kind: TaskKind,
    title: &'static str,
}

fn maintenance_task_config(action: &str) -> Option<MaintenanceTaskConfig> {
    match action {
        "reindex" | "reindex-library" => Some(MaintenanceTaskConfig {
            action: "reindex",
            kind: TaskKind::ReindexLibrary,
            title: "重建书库索引",
        }),
        "cleanup-orphan-covers" | "cleanup_orphan_covers" => Some(MaintenanceTaskConfig {
            action: "cleanup-orphan-covers",
            kind: TaskKind::CleanupOrphanCovers,
            title: "清理孤儿封面",
        }),
        "recompute-hashes" | "recompute-file-hashes" | "recompute_file_hashes" => {
            Some(MaintenanceTaskConfig {
                action: "recompute-hashes",
                kind: TaskKind::RecomputeFileHashes,
                title: "重新计算文件哈希",
            })
        }
        _ => None,
    }
}

/// Queue a background maintenance task for a library.
async fn queue_maintenance_task(
    _admin: AdminUser,
    State(state): State<Arc<AppState>>,
    Path((id, action)): Path<(Uuid, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    let library = state.libraries.get(id).await?;
    let Some(config) = maintenance_task_config(&action) else {
        return Err(ApiError::bad_request(
            "Invalid maintenance action. Use 'reindex', 'cleanup-orphan-covers', or 'recompute-hashes'",
        ));
    };

    let task_id = state
        .task_queue
        .submit(
            config.kind,
            None,
            serde_json::json!({
                "library_id": id.to_string(),
                "library_name": library.name,
                "action": config.action,
            }),
        )
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to queue maintenance task: {}", e)))?;

    Ok(Json(serde_json::json!({
        "message": format!("{}已加入任务队列", config.title),
        "library_id": id,
        "action": config.action,
        "task_id": task_id,
    })))
}

// ─── Chapter Splitter ───────────────────────────────────────────────

/// Extract title and author from filename using common Chinese ebook patterns.
/// Patterns: "title - author", "【author】title", "[author]title", "title（author）"
fn extract_title_author(filename: &str) -> (String, Option<String>) {
    // Pattern: 【author】title or [author]title
    if let Some(rest) = filename.strip_prefix('【') {
        if let Some(idx) = rest.find('】') {
            let author = rest[..idx].trim().to_string();
            let title = rest[idx + '】'.len_utf8()..].trim().to_string();
            if !author.is_empty() && !title.is_empty() {
                return (title, Some(author));
            }
        }
    }
    if let Some(rest) = filename.strip_prefix('[') {
        if let Some(idx) = rest.find(']') {
            let author = rest[..idx].trim().to_string();
            let title = rest[idx + 1..].trim().to_string();
            if !author.is_empty() && !title.is_empty() {
                return (title, Some(author));
            }
        }
    }
    // Pattern: "title - author" (last " - " delimiter)
    if let Some(idx) = filename.rfind(" - ") {
        let left = filename[..idx].trim().to_string();
        let right = filename[idx + 3..].trim().to_string();
        // Author is typically shorter; title is longer
        if !left.is_empty() && !right.is_empty() && right.chars().count() <= 10 {
            return (left, Some(right));
        }
    }
    // Pattern: "title（author）" or "title(author)"
    for (open, close) in &[('（', '）'), ('(', ')')] {
        if filename.ends_with(*close) {
            if let Some(idx) = filename.rfind(*open) {
                let title = filename[..idx].trim().to_string();
                let author = filename[idx + open.len_utf8()..filename.len() - close.len_utf8()]
                    .trim()
                    .to_string();
                if !title.is_empty() && !author.is_empty() && author.chars().count() <= 10 {
                    return (title, Some(author));
                }
            }
        }
    }
    (filename.to_string(), None)
}

/// Create or find a person record and link it to a book as author.
async fn link_author_person(db: &sqlx::PgPool, book_id: Uuid, author_name: &str) {
    // Find or create person
    let person_id: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM persons WHERE name = $1 LIMIT 1")
            .bind(author_name)
            .fetch_optional(db)
            .await
            .ok()
            .flatten();

    let person_id = match person_id {
        Some(id) => id,
        None => {
            let new_id = Uuid::now_v7();
            let result = sqlx::query(
                "INSERT INTO persons (id, name, sort_name) VALUES ($1, $2, $2) ON CONFLICT (name) DO NOTHING"
            )
            .bind(new_id)
            .bind(author_name)
            .execute(db)
            .await;
            if result.is_err() {
                // If conflict, fetch existing
                sqlx::query_scalar::<_, Uuid>("SELECT id FROM persons WHERE name = $1")
                    .bind(author_name)
                    .fetch_optional(db)
                    .await
                    .ok()
                    .flatten()
                    .unwrap_or(new_id)
            } else {
                new_id
            }
        }
    };

    // Link person to book as author
    let _ = sqlx::query(
        "INSERT INTO book_persons (book_id, person_id, role) VALUES ($1, $2, 'author') ON CONFLICT DO NOTHING"
    )
    .bind(book_id)
    .bind(person_id)
    .execute(db)
    .await;
}

/// Split text content into chapters using common Chinese/English chapter patterns.
fn split_into_chapters(text: &str) -> Vec<ImportedChapter> {
    let Ok(chapter_re) = regex::Regex::new(
        r"(?m)^(第[一二三四五六七八九十百千\d]+[章节回卷集部篇][\s:：]*.{0,50}|Chapter\s+\d+[\s:：]*.{0,80}|CHAPTER\s+\d+[\s:：]*.{0,80})",
    ) else {
        return vec![ImportedChapter {
            title: "全文".to_string(),
            content: text.to_string(),
        }];
    };

    let matches: Vec<_> = chapter_re.find_iter(text).collect();

    if matches.is_empty() {
        // No chapters found — treat entire text as one chapter
        return vec![ImportedChapter {
            title: "全文".to_string(),
            content: text.to_string(),
        }];
    }

    let mut chapters = Vec::new();
    for (i, m) in matches.iter().enumerate() {
        let title = m.as_str().trim().to_string();
        let start = m.start();
        let end = if i + 1 < matches.len() {
            matches[i + 1].start()
        } else {
            text.len()
        };
        let content = text[start..end].to_string();
        chapters.push(ImportedChapter { title, content });
    }

    // If there's content before the first chapter, add it as prologue
    if matches[0].start() > 100 {
        let prologue = text[..matches[0].start()].trim().to_string();
        if !prologue.is_empty() {
            chapters.insert(
                0,
                ImportedChapter {
                    title: "序章".to_string(),
                    content: prologue,
                },
            );
        }
    }

    chapters
}

/// Auto-detect series from book titles using common patterns.
/// Groups books like "斗破苍穹 第1卷", "斗破苍穹 第2卷" into a "斗破苍穹" series.
async fn auto_detect_series(state: &Arc<AppState>, library_id: Uuid) -> usize {
    // Patterns: "Title Vol.X", "Title 第X卷", "Title - X", "Title (X)", etc.
    let books: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id, title FROM books WHERE library_id = $1 AND series_id IS NULL")
            .bind(library_id)
            .fetch_all(&state.db)
            .await
            .unwrap_or_default();

    if books.is_empty() {
        return 0;
    }

    // Extract series name from title by stripping volume/number suffixes
    let volume_patterns = [regex::Regex::new(
        r"(?i)\s*(第\s*\d+\s*[卷册部章集]|vol\.?\s*\d+|\(\d+\)|\s+-\s*\d+|\s+\d+$)",
    )
    .ok()];

    let mut series_map: std::collections::HashMap<String, Vec<(Uuid, u32)>> =
        std::collections::HashMap::new();

    for (book_id, title) in &books {
        let mut series_name = title.clone();
        let mut volume_num = 0u32;

        for pattern in volume_patterns.iter().flatten() {
            if let Some(m) = pattern.find(&series_name) {
                // Extract number from the match
                let matched = m.as_str();
                let num_str: String = matched.chars().filter(|c| c.is_ascii_digit()).collect();
                volume_num = num_str.parse().unwrap_or(0);
                series_name = series_name[..m.start()].trim().to_string();
                break;
            }
        }

        // Only group if we extracted a meaningful series name different from title
        if !series_name.is_empty() && series_name != *title && series_name.chars().count() >= 2 {
            series_map
                .entry(series_name)
                .or_default()
                .push((*book_id, volume_num));
        }
    }

    let mut created = 0;
    for (series_name, books_in_series) in &series_map {
        // Only create series if there are 2+ books
        if books_in_series.len() < 2 {
            continue;
        }

        // Check if series already exists
        let existing: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM series WHERE library_id = $1 AND name = $2")
                .bind(library_id)
                .bind(series_name)
                .fetch_optional(&state.db)
                .await
                .ok()
                .flatten();

        let series_id = match existing {
            Some(id) => id,
            None => {
                let new_id = Uuid::new_v4();
                let folder_path = format!("/virtual/{}", series_name);
                let _ = sqlx::query(
                    "INSERT INTO series (id, library_id, name, sort_name, folder_path) VALUES ($1, $2, $3, $4, $5)"
                )
                .bind(new_id)
                .bind(library_id)
                .bind(series_name)
                .bind(series_name)
                .bind(&folder_path)
                .execute(&state.db)
                .await;
                created += 1;
                new_id
            }
        };

        // Assign books to series with volume numbers
        for (book_id, vol) in books_in_series {
            let _ =
                sqlx::query("UPDATE books SET series_id = $1, series_volume = $2 WHERE id = $3")
                    .bind(series_id)
                    .bind(*vol as i32)
                    .bind(book_id)
                    .execute(&state.db)
                    .await;

            // Also insert into series_books junction table
            let _ = sqlx::query(
                "INSERT INTO series_books (series_id, book_id, sort_order) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING"
            )
            .bind(series_id)
            .bind(book_id)
            .bind(*vol as f64)
            .execute(&state.db)
            .await;
        }
    }

    created
}

// ─── Series Endpoints ──────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct SeriesRow {
    id: Uuid,
    library_id: Uuid,
    name: String,
    sort_name: String,
    original_name: Option<String>,
    summary: Option<String>,
    author: Option<String>,
    language: String,
    status: String,
    genres: Vec<String>,
    tags: Vec<String>,
    book_count: Option<i64>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Deserialize)]
struct SeriesQuery {
    #[serde(default)]
    search: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    library_id: Option<Uuid>,
    #[serde(default)]
    sort_by: Option<String>,
    #[serde(default)]
    sort_dir: Option<String>,
}

fn series_read_access() -> LibraryAccess {
    LibraryAccess::Read
}

fn series_write_access() -> LibraryAccess {
    LibraryAccess::Write
}

fn series_books_progress_sql() -> &'static str {
    "SELECT book_id, progress FROM reading_progress WHERE book_id = ANY($1) AND user_id = $2"
}

async fn series_library_id(state: &AppState, series_id: Uuid) -> ApiResult<Uuid> {
    sqlx::query_scalar("SELECT library_id FROM series WHERE id = $1")
        .bind(series_id)
        .fetch_optional(&state.db)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound(format!("Series {} not found", series_id)))
}

/// List all series across all libraries.
async fn list_all_series(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(params): Query<SeriesQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let visible_libraries = if let Some(library_id) = params.library_id {
        ensure_library_access(&state, &auth, library_id, series_read_access()).await?;
        None
    } else {
        visible_library_ids(&state, &auth, series_read_access()).await?
    };

    if matches!(&visible_libraries, Some(ids) if ids.is_empty()) {
        return Ok(Json(serde_json::json!([])));
    }

    let mut sql = String::from(
        r#"
        SELECT s.id, s.library_id, s.name, s.sort_name, s.original_name,
               s.summary, s.author, s.language::text as language,
               s.status::text as status, s.genres, s.tags,
               s.created_at, s.updated_at,
               (SELECT COUNT(*) FROM books WHERE series_id = s.id)::bigint as book_count
        FROM series s
        WHERE 1=1
        "#,
    );
    let mut binds: Vec<String> = Vec::new();

    if let Some(ref search) = params.search {
        binds.push(format!("%{}%", search));
        sql.push_str(&format!(" AND s.name ILIKE ${}", binds.len()));
    }
    if let Some(ref status) = params.status {
        binds.push(status.clone());
        sql.push_str(&format!(" AND s.status::text = ${}", binds.len()));
    }
    if let Some(library_id) = params.library_id {
        sql.push_str(&format!(" AND s.library_id = ${}::uuid", binds.len() + 1));
        binds.push(library_id.to_string());
    } else if visible_libraries.is_some() {
        sql.push_str(&format!(" AND s.library_id = ANY(${})", binds.len() + 1));
    }

    let order_col = match params.sort_by.as_deref() {
        Some("book_count") => "book_count",
        Some("created_at") => "s.created_at",
        Some("updated_at") => "s.updated_at",
        Some("status") => "s.status",
        Some("name") | Some("sort_name") | None => "s.sort_name",
        _ => "s.sort_name",
    };
    let order_dir = match params.sort_dir.as_deref() {
        Some("desc") => "DESC",
        _ => "ASC",
    };
    sql.push_str(&format!(
        " ORDER BY {order_col} {order_dir}, s.sort_name ASC"
    ));

    // Build the query dynamically
    let mut query = sqlx::query_as::<_, SeriesRow>(&sql);
    for bind in &binds {
        query = query.bind(bind);
    }
    if params.library_id.is_none() {
        if let Some(ids) = &visible_libraries {
            query = query.bind(ids);
        }
    }

    let rows = query.fetch_all(&state.db).await.map_err(ApiError::from)?;

    // Batch fetch word counts and folder paths
    let series_ids: Vec<Uuid> = rows.iter().map(|s| s.id).collect();
    let library_ids: Vec<Uuid> = rows
        .iter()
        .map(|s| s.library_id)
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    // Word counts per series
    let word_counts: std::collections::HashMap<Uuid, i64> = if !series_ids.is_empty() {
        sqlx::query_as::<_, (Uuid, i64)>(
            "SELECT series_id, COALESCE(SUM(word_count), 0)::bigint FROM books WHERE series_id = ANY($1) GROUP BY series_id"
        )
        .bind(&series_ids)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default()
        .into_iter()
        .collect()
    } else {
        Default::default()
    };

    // Library paths
    let lib_paths: std::collections::HashMap<Uuid, String> = if !library_ids.is_empty() {
        sqlx::query_as::<_, (Uuid, String)>(
            "SELECT id, root_path FROM libraries WHERE id = ANY($1)",
        )
        .bind(&library_ids)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default()
        .into_iter()
        .collect()
    } else {
        Default::default()
    };

    // Book covers per series (first 4 for mosaic)
    let series_covers: std::collections::HashMap<Uuid, Vec<String>> = if !series_ids.is_empty() {
        sqlx::query_as::<_, (Uuid, String)>(
            r#"SELECT series_id, cover_path FROM (
                SELECT series_id, cover_path, ROW_NUMBER() OVER (PARTITION BY series_id ORDER BY created_at) as rn
                FROM books WHERE series_id = ANY($1) AND cover_path IS NOT NULL
            ) sub WHERE rn <= 4"#
        )
        .bind(&series_ids)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default()
        .into_iter()
        .fold(std::collections::HashMap::new(), |mut acc, (sid, cover)| {
            acc.entry(sid).or_insert_with(Vec::new).push(cover);
            acc
        })
    } else {
        Default::default()
    };

    let data: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|s| {
            let covers = series_covers.get(&s.id).cloned().unwrap_or_default();
            serde_json::json!({
                "id": s.id,
                "library_id": s.library_id,
                "name": s.name,
                "sort_name": s.sort_name,
                "original_name": s.original_name,
                "summary": s.summary,
                "author": s.author,
                "language": s.language,
                "status": s.status,
                "genres": s.genres,
                "tags": s.tags,
                "book_count": s.book_count.unwrap_or(0),
                "total_word_count": word_counts.get(&s.id).copied().unwrap_or(0),
                "folder_path": lib_paths.get(&s.library_id).cloned().unwrap_or_default(),
                "book_covers": covers,
                "created_at": s.created_at,
                "updated_at": s.updated_at,
            })
        })
        .collect();

    Ok(Json(serde_json::json!(data)))
}

/// List series within a specific library.
async fn list_library_series(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(library_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    ensure_library_access(&state, &auth, library_id, series_read_access()).await?;
    let rows = sqlx::query_as::<_, SeriesRow>(
        r#"
        SELECT s.id, s.library_id, s.name, s.sort_name, s.original_name,
               s.summary, s.author, s.language::text as language,
               s.status::text as status, s.genres, s.tags,
               s.created_at, s.updated_at,
               (SELECT COUNT(*) FROM books WHERE series_id = s.id)::bigint as book_count
        FROM series s
        WHERE s.library_id = $1
        ORDER BY s.sort_name
        "#,
    )
    .bind(library_id)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;

    let data: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|s| {
            serde_json::json!({
                "id": s.id,
                "library_id": s.library_id,
                "name": s.name,
                "sort_name": s.sort_name,
                "original_name": s.original_name,
                "summary": s.summary,
                "author": s.author,
                "language": s.language,
                "status": s.status,
                "genres": s.genres,
                "tags": s.tags,
                "book_count": s.book_count.unwrap_or(0),
                "created_at": s.created_at,
                "updated_at": s.updated_at,
            })
        })
        .collect();

    Ok(Json(serde_json::json!(data)))
}

/// Get a single series by ID.
async fn get_series(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let row = sqlx::query_as::<_, SeriesRow>(
        r#"
        SELECT s.id, s.library_id, s.name, s.sort_name, s.original_name,
               s.summary, s.author, s.language::text as language,
               s.status::text as status, s.genres, s.tags,
               s.created_at, s.updated_at,
               (SELECT COUNT(*) FROM books WHERE series_id = s.id)::bigint as book_count
        FROM series s
        WHERE s.id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::from)?
    .ok_or_else(|| ApiError::NotFound(format!("Series {} not found", id)))?;
    ensure_library_access(&state, &auth, row.library_id, series_read_access()).await?;

    // Compute total word count
    let total_word_count: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(word_count), 0)::bigint FROM books WHERE series_id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    // Get folder path from library
    let folder_path: Option<String> =
        sqlx::query_scalar("SELECT root_path FROM libraries WHERE id = $1")
            .bind(row.library_id)
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None);

    Ok(Json(serde_json::json!({
        "id": row.id,
        "library_id": row.library_id,
        "name": row.name,
        "sort_name": row.sort_name,
        "original_name": row.original_name,
        "summary": row.summary,
        "description": row.summary,
        "author": row.author,
        "language": row.language,
        "status": row.status,
        "genres": row.genres,
        "tags": row.tags,
        "book_count": row.book_count.unwrap_or(0),
        "total_word_count": total_word_count,
        "folder_path": folder_path.unwrap_or_default(),
        "metadata": {
            "genres": row.genres,
            "tags": row.tags,
            "summary": row.summary,
        },
        "created_at": row.created_at,
        "updated_at": row.updated_at,
    })))
}

/// Get all books in a series.
async fn get_series_books(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(series_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let library_id = series_library_id(&state, series_id).await?;
    ensure_library_access(&state, &auth, library_id, series_read_access()).await?;
    let user_id = auth_user_id(&auth)?;

    #[derive(sqlx::FromRow)]
    struct SeriesBookRow {
        id: Uuid,
        title: String,
        author: Option<String>,
        format: String,
        reading_status: String,
        sort_order: Option<f64>,
        word_count: i64,
        cover_path: Option<String>,
        created_at: chrono::DateTime<chrono::Utc>,
    }

    let books = sqlx::query_as::<_, SeriesBookRow>(
        r#"
        SELECT b.id, b.title, b.author, b.format::text as format,
               b.reading_status::text as reading_status,
               sb.sort_order, b.word_count, b.cover_path, b.created_at
        FROM books b
        LEFT JOIN series_books sb ON sb.book_id = b.id AND sb.series_id = $1
        WHERE b.library_id = $2 AND (b.series_id = $1 OR sb.series_id = $1)
        ORDER BY COALESCE(sb.sort_order, 9999), b.title
        "#,
    )
    .bind(series_id)
    .bind(library_id)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;

    // Get chapter counts
    let book_ids: Vec<Uuid> = books.iter().map(|b| b.id).collect();
    let chapter_counts: std::collections::HashMap<Uuid, i64> = if !book_ids.is_empty() {
        sqlx::query_as::<_, (Uuid, i64)>(
            "SELECT book_id, COUNT(*)::bigint FROM chapters WHERE book_id = ANY($1) GROUP BY book_id"
        )
        .bind(&book_ids)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default()
        .into_iter()
        .collect()
    } else {
        Default::default()
    };

    // Get progress
    let progress_map: std::collections::HashMap<Uuid, f64> = if !book_ids.is_empty() {
        sqlx::query_as::<_, (Uuid, f64)>(series_books_progress_sql())
            .bind(&book_ids)
            .bind(user_id)
            .fetch_all(&state.db)
            .await
            .unwrap_or_default()
            .into_iter()
            .collect()
    } else {
        Default::default()
    };

    let data: Vec<serde_json::Value> = books
        .into_iter()
        .map(|b| {
            serde_json::json!({
                "id": b.id,
                "title": b.title,
                "author": b.author,
                "format": b.format,
                "reading_status": b.reading_status,
                "sort_order": b.sort_order,
                "word_count": b.word_count,
                "chapter_count": chapter_counts.get(&b.id).copied().unwrap_or(0),
                "progress": progress_map.get(&b.id).copied().unwrap_or(0.0),
                "cover_path": b.cover_path,
                "created_at": b.created_at,
            })
        })
        .collect();

    Ok(Json(serde_json::json!(data)))
}

#[derive(Debug, Deserialize)]
struct ReorderSeriesBooksRequest {
    book_ids: Vec<Uuid>,
}

/// Persist manual book ordering within a series.
async fn reorder_series_books(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(series_id): Path<Uuid>,
    Json(body): Json<ReorderSeriesBooksRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let library_id = series_library_id(&state, series_id).await?;
    ensure_library_access(&state, &auth, library_id, series_write_access()).await?;

    if body.book_ids.is_empty() {
        return Ok(Json(serde_json::json!({
            "status": "ok",
            "updated": 0,
        })));
    }

    let allowed_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT b.id)::bigint
         FROM books b
         LEFT JOIN series_books sb ON sb.book_id = b.id AND sb.series_id = $1
         WHERE b.id = ANY($2) AND b.library_id = $3 AND (b.series_id = $1 OR sb.series_id = $1)",
    )
    .bind(series_id)
    .bind(&body.book_ids)
    .bind(library_id)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::from)?;

    if allowed_count != body.book_ids.len() as i64 {
        return Err(ApiError::bad_request(
            "book_ids must all belong to the series",
        ));
    }

    let mut tx = state.db.begin().await.map_err(ApiError::from)?;
    for (idx, book_id) in body.book_ids.iter().enumerate() {
        sqlx::query(
            "INSERT INTO series_books (series_id, book_id, sort_order)
             VALUES ($1, $2, $3)
             ON CONFLICT (series_id, book_id) DO UPDATE SET sort_order = EXCLUDED.sort_order",
        )
        .bind(series_id)
        .bind(book_id)
        .bind(idx as f64)
        .execute(&mut *tx)
        .await
        .map_err(ApiError::from)?;
    }
    tx.commit().await.map_err(ApiError::from)?;

    Ok(Json(serde_json::json!({
        "status": "ok",
        "updated": body.book_ids.len(),
    })))
}

/// Update series metadata.
async fn update_series_metadata(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let library_id = series_library_id(&state, id).await?;
    ensure_library_access(&state, &auth, library_id, series_write_access()).await?;

    // Update only provided fields
    if let Some(name) = body.get("name").and_then(|v| v.as_str()) {
        sqlx::query("UPDATE series SET name = $1, updated_at = NOW() WHERE id = $2")
            .bind(name)
            .bind(id)
            .execute(&state.db)
            .await
            .map_err(ApiError::from)?;
    }
    if let Some(summary) = body.get("summary").and_then(|v| v.as_str()) {
        sqlx::query("UPDATE series SET summary = $1, updated_at = NOW() WHERE id = $2")
            .bind(summary)
            .bind(id)
            .execute(&state.db)
            .await
            .map_err(ApiError::from)?;
    }
    if let Some(author) = body.get("author").and_then(|v| v.as_str()) {
        sqlx::query("UPDATE series SET author = $1, updated_at = NOW() WHERE id = $2")
            .bind(author)
            .bind(id)
            .execute(&state.db)
            .await
            .map_err(ApiError::from)?;
    }

    Ok(Json(serde_json::json!({
        "message": "Series metadata updated",
        "id": id,
    })))
}

// ─── Library Permissions ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct PermissionEntry {
    user_id: Uuid,
    can_read: bool,
    can_write: bool,
    can_manage: bool,
}

#[derive(Debug, Deserialize)]
struct GroupPermissionEntry {
    group_id: Uuid,
    can_read: bool,
    can_write: bool,
    can_manage: bool,
}

fn permission_enabled(read: bool, write: bool, manage: bool) -> bool {
    read || write || manage
}

/// GET /libraries/:id/permissions — list all user and group permissions for a library
async fn get_library_permissions(
    _admin: AdminUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let rows = sqlx::query_as::<_, (Uuid, bool, bool, bool)>(
        "SELECT user_id, can_read, can_write, can_manage FROM library_permissions WHERE library_id = $1"
    )
    .bind(id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let permissions: Vec<serde_json::Value> = rows
        .iter()
        .map(|(uid, read, write, manage)| {
            serde_json::json!({
                "user_id": uid,
                "can_read": read,
                "can_write": write,
                "can_manage": manage,
            })
        })
        .collect();

    let group_rows = sqlx::query_as::<_, (Uuid, bool, bool, bool)>(
        "SELECT group_id, can_read, can_write, can_manage FROM library_group_permissions WHERE library_id = $1"
    )
    .bind(id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let group_permissions: Vec<serde_json::Value> = group_rows
        .iter()
        .map(|(gid, read, write, manage)| {
            serde_json::json!({
                "group_id": gid,
                "can_read": read,
                "can_write": write,
                "can_manage": manage,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "permissions": permissions,
        "group_permissions": group_permissions,
    })))
}

#[derive(Debug, Deserialize)]
struct SetPermissionsRequest {
    #[serde(default)]
    permissions: Vec<PermissionEntry>,
    #[serde(default)]
    group_permissions: Vec<GroupPermissionEntry>,
}

/// PUT /libraries/:id/permissions — replace user and group permissions (admin only)
async fn set_library_permissions(
    _admin: AdminUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(body): Json<SetPermissionsRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    // Verify library exists
    let _library = state.libraries.get(id).await?;

    let mut tx = state.db.begin().await.map_err(ApiError::from)?;

    sqlx::query("DELETE FROM library_permissions WHERE library_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(ApiError::from)?;

    for perm in &body.permissions {
        if !permission_enabled(perm.can_read, perm.can_write, perm.can_manage) {
            continue;
        }
        sqlx::query(
            "INSERT INTO library_permissions (library_id, user_id, can_read, can_write, can_manage)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (library_id, user_id) DO UPDATE SET
                can_read = $3, can_write = $4, can_manage = $5",
        )
        .bind(id)
        .bind(perm.user_id)
        .bind(perm.can_read)
        .bind(perm.can_write)
        .bind(perm.can_manage)
        .execute(&mut *tx)
        .await
        .map_err(ApiError::from)?;
    }

    sqlx::query("DELETE FROM library_group_permissions WHERE library_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(ApiError::from)?;

    for perm in &body.group_permissions {
        if !permission_enabled(perm.can_read, perm.can_write, perm.can_manage) {
            continue;
        }
        sqlx::query(
            "INSERT INTO library_group_permissions (library_id, group_id, can_read, can_write, can_manage)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (library_id, group_id) DO UPDATE SET
                can_read = $3, can_write = $4, can_manage = $5"
        )
        .bind(id)
        .bind(perm.group_id)
        .bind(perm.can_read)
        .bind(perm.can_write)
        .bind(perm.can_manage)
        .execute(&mut *tx)
        .await
        .map_err(ApiError::from)?;
    }

    tx.commit().await.map_err(ApiError::from)?;

    Ok(Json(serde_json::json!({
        "status": "ok",
        "message": "Permissions updated",
        "library_id": id,
        "user_entries": body.permissions.len(),
        "group_entries": body.group_permissions.len(),
    })))
}

// ─── Library Feature Toggles ─────────────────────────────────────────────────

/// Defaults applied when a feature key is absent from the stored JSONB blob.
fn library_features_with_defaults(stored: &serde_json::Value) -> serde_json::Value {
    let get = |key: &str, default: bool| -> bool {
        stored.get(key).and_then(|v| v.as_bool()).unwrap_or(default)
    };
    serde_json::json!({
        "enable_ai": get("enable_ai", true),
        "enable_translation": get("enable_translation", true),
        "enable_graph": get("enable_graph", true),
        "allow_guests": get("allow_guests", false),
    })
}

/// GET /libraries/:id/features — read library-level feature toggles
async fn get_library_features(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_library_access(&state, &auth, id, LibraryAccess::Read).await?;
    let stored: serde_json::Value =
        sqlx::query_scalar("SELECT COALESCE(features, '{}'::jsonb) FROM libraries WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await
            .map_err(ApiError::from)?
            .ok_or_else(|| ApiError::not_found("library"))?;

    Ok(Json(library_features_with_defaults(&stored)))
}

#[derive(Debug, Deserialize)]
struct SetFeaturesRequest {
    enable_ai: Option<bool>,
    enable_translation: Option<bool>,
    enable_graph: Option<bool>,
    allow_guests: Option<bool>,
}

/// PUT /libraries/:id/features — persist library-level feature toggles
async fn set_library_features(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<SetFeaturesRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_library_access(&state, &auth, id, LibraryAccess::Manage).await?;
    // Merge with current values so a partial payload preserves untouched toggles.
    let current: serde_json::Value =
        sqlx::query_scalar("SELECT COALESCE(features, '{}'::jsonb) FROM libraries WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await
            .map_err(ApiError::from)?
            .ok_or_else(|| ApiError::not_found("library"))?;

    let merged = library_features_with_defaults(&current);
    let features = serde_json::json!({
        "enable_ai": body.enable_ai.unwrap_or_else(|| merged["enable_ai"].as_bool().unwrap_or(true)),
        "enable_translation": body.enable_translation.unwrap_or_else(|| merged["enable_translation"].as_bool().unwrap_or(true)),
        "enable_graph": body.enable_graph.unwrap_or_else(|| merged["enable_graph"].as_bool().unwrap_or(true)),
        "allow_guests": body.allow_guests.unwrap_or_else(|| merged["allow_guests"].as_bool().unwrap_or(false)),
    });

    sqlx::query("UPDATE libraries SET features = $1 WHERE id = $2")
        .bind(&features)
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(features))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn series_read_endpoints_require_read_access() {
        assert_eq!(series_read_access(), LibraryAccess::Read);
    }

    #[test]
    fn series_mutation_endpoints_require_write_access() {
        assert_eq!(series_write_access(), LibraryAccess::Write);
    }

    #[test]
    fn series_books_progress_is_scoped_to_current_user() {
        assert!(series_books_progress_sql().contains("user_id = $2"));
        assert!(!series_books_progress_sql().contains("user_id IS NULL"));
    }

    #[test]
    fn series_member_queries_scope_books_to_series_library() {
        let source = include_str!("libraries.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("production source should exist");

        assert!(
            source.contains("WHERE b.library_id = $2 AND (b.series_id = $1 OR sb.series_id = $1)")
        );
        assert!(source.contains("WHERE b.id = ANY($2) AND b.library_id = $3 AND (b.series_id = $1 OR sb.series_id = $1)"));
    }

    #[test]
    fn maintenance_actions_resolve_to_typed_task_kinds() {
        let cases = [
            ("reindex", TaskKind::ReindexLibrary),
            ("cleanup-orphan-covers", TaskKind::CleanupOrphanCovers),
            ("recompute-hashes", TaskKind::RecomputeFileHashes),
        ];
        for (action, expected) in cases {
            assert_eq!(
                maintenance_task_config(action).map(|config| config.kind),
                Some(expected)
            );
        }
        assert!(maintenance_task_config("drop-library").is_none());
    }

    #[tokio::test]
    async fn exact_file_discovery_excludes_the_already_imported_source_path() {
        let path = std::path::Path::new("/tmp/nova-reader-known-source.txt");

        assert!(import_paths_identify_same_source(path, "/tmp/nova-reader-known-source.txt").await);
        assert!(
            !import_paths_identify_same_source(path, "/tmp/nova-reader-distinct-source.txt").await
        );
    }

    #[tokio::test]
    async fn inline_chapter_persistence_waits_before_rows_and_commits_the_whole_book() {
        let _write_guard = crate::dedup::DEDUP_DATABASE_TEST_LOCK.lock().await;
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let db = sqlx::PgPool::connect(&database_url)
            .await
            .expect("connect for inline chapter transaction test");
        crate::migrations::run_database_migrations(&db)
            .await
            .expect("apply migrations for inline chapter transaction test");
        let library_id = Uuid::now_v7();
        let book_id = Uuid::now_v7();

        sqlx::query("INSERT INTO libraries (id, name, root_path) VALUES ($1, $2, $3)")
            .bind(library_id)
            .bind("Inline chapter transaction test")
            .bind(format!("/tmp/nova-inline-chapters-{library_id}"))
            .execute(&db)
            .await
            .expect("insert inline chapter test library");
        sqlx::query(
            r#"INSERT INTO books
               (id, library_id, title, format, status, file_path, file_hash)
               VALUES ($1, $2, 'Inline chapter book', 'txt', 'processing', $3, $4)"#,
        )
        .bind(book_id)
        .bind(library_id)
        .bind(format!("/tmp/{book_id}.txt"))
        .bind(format!("inline-chapter-hash-{book_id}"))
        .execute(&db)
        .await
        .expect("insert inline chapter test book");

        let mut publisher = db.begin().await.expect("begin competing publication");
        sqlx::query("SELECT lock_novel_dedup_global_barrier()")
            .execute(&mut *publisher)
            .await
            .expect("publication holds global barrier");
        let chapter_repository = crate::repo::pg_chapter::PgChapterRepository::new(db.clone());
        let chapters = vec![
            ImportedChapter {
                title: "One".into(),
                content: "first chapter".into(),
            },
            ImportedChapter {
                title: "Two".into(),
                content: "second chapter".into(),
            },
        ];
        let mut writer = tokio::spawn(async move {
            chapter_repository
                .insert_imported_and_mark_ready(book_id, &chapters)
                .await
        });
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), &mut writer)
                .await
                .is_err(),
            "import should wait at its first lock instead of partially inserting"
        );
        let visible_chapters: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM chapters WHERE book_id = $1")
                .bind(book_id)
                .fetch_one(&db)
                .await
                .expect("inspect chapters while writer is waiting");
        assert_eq!(visible_chapters, 0);

        publisher
            .commit()
            .await
            .expect("release competing publication");
        writer
            .await
            .expect("join inline chapter writer")
            .expect("inline chapter transaction succeeds after barrier release");
        let persisted: (i64, String, i32, i64) = sqlx::query_as(
            r#"SELECT COUNT(chapter.id), book.status::text,
                      book.chapter_count, book.word_count
               FROM books book
               LEFT JOIN chapters chapter ON chapter.book_id = book.id
               WHERE book.id = $1
               GROUP BY book.id"#,
        )
        .bind(book_id)
        .fetch_one(&db)
        .await
        .expect("load atomically persisted book");
        assert_eq!(persisted.0, 2);
        assert_eq!(persisted.1, "ready");
        assert_eq!(persisted.2, 2);
        assert_eq!(persisted.3, 25);

        sqlx::query("DELETE FROM books WHERE id = $1")
            .bind(book_id)
            .execute(&db)
            .await
            .expect("clean inline chapter test book");
        sqlx::query(
            r#"DELETE FROM tasks
               WHERE id IN (
                 SELECT task_id FROM dedup_scan_runs
                 WHERE library_id = $1 AND task_id IS NOT NULL
               )"#,
        )
        .bind(library_id)
        .execute(&db)
        .await
        .expect("clean inline chapter scan tasks");
        sqlx::query("DELETE FROM libraries WHERE id = $1")
            .bind(library_id)
            .execute(&db)
            .await
            .expect("clean inline chapter test library");
    }
}
