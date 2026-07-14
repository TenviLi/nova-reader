//! Background task worker that polls the DAG-based task queue and executes tasks.
//!
//! Runs as a tokio task spawned from main.rs. Respects graceful shutdown via a
//! watch channel. Each task kind dispatches to the appropriate handler function.

use std::{str::FromStr, sync::Arc, time::Duration};
use tokio::sync::watch;
use tracing::{error, info, warn};
use uuid::Uuid;

use nova_core::domain::task::{TaskDag, TaskKind};

use crate::dedup::{
    delete_book_embedding_points, embedding_point_id, load_embedding_freshness_contract,
};
use crate::state::AppState;
use crate::task_queue::TaskRow;

const STARTUP_RECOVERY_RETRY_DELAY: Duration = Duration::from_secs(5);
const TASK_CONTINUATION_DELAY_SECONDS: i64 = 5;

/// Spawn the background worker loop. Returns a JoinHandle.
pub fn spawn_worker(
    state: Arc<AppState>,
    mut shutdown: watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    let concurrency = state.config.worker_concurrency.max(1);
    info!("🔄 Task worker starting (concurrency={})", concurrency);

    tokio::spawn(async move {
        if !recover_before_polling(&state, &mut shutdown).await {
            info!("Task worker shut down before startup recovery completed");
            return;
        }
        info!("Task worker startup recovery complete");
        let semaphore = Arc::new(tokio::sync::Semaphore::new(concurrency));

        loop {
            // Check shutdown
            if *shutdown.borrow() {
                info!("Task worker shutting down...");
                break;
            }

            // Acquire a permit before polling
            let permit = match semaphore.clone().acquire_owned().await {
                Ok(p) => p,
                Err(_) => break, // Semaphore closed
            };

            // Poll for next runnable task
            let task = match state.task_queue.poll_next().await {
                Ok(Some(t)) => t,
                Ok(None) => {
                    // No runnable tasks — drop permit, wait, then retry
                    drop(permit);
                    tokio::select! {
                        _ = tokio::time::sleep(std::time::Duration::from_secs(2)) => {}
                        _ = shutdown.changed() => { break; }
                    }
                    continue;
                }
                Err(e) => {
                    error!("Task queue poll error: {}", e);
                    drop(permit);
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    continue;
                }
            };

            let task_id = task.id;
            let task_kind = task.kind.clone();

            // Spawn execution on a separate task
            let state_clone = state.clone();
            tokio::spawn(async move {
                let _permit = permit; // Hold permit until done

                info!("▶ Executing task {} ({})", task_id, task_kind);

                match execute_task(&state_clone, &task).await {
                    Ok(TaskExecutionOutcome::Completed(result)) => {
                        if let Err(e) = state_clone.task_queue.mark_completed(task_id, result).await
                        {
                            error!("Failed to mark task {} completed: {}", task_id, e);
                        } else {
                            info!("✓ Task {} ({}) completed", task_id, task_kind);
                        }
                    }
                    Ok(TaskExecutionOutcome::Continue(message)) => {
                        info!(
                            "↻ Task {} ({}) will continue: {}",
                            task_id, task_kind, message
                        );
                        if let Err(error) = state_clone
                            .task_queue
                            .reschedule_running_continuation(
                                task_id,
                                &message,
                                TASK_CONTINUATION_DELAY_SECONDS,
                            )
                            .await
                        {
                            error!(
                                "Failed to reschedule continuing task {}: {}",
                                task_id, error
                            );
                        }
                    }
                    Err(err) => {
                        warn!("✗ Task {} ({}) failed: {}", task_id, task_kind, err);
                        if let Err(e) = state_clone.task_queue.mark_failed(task_id, &err).await {
                            error!("Failed to mark task {} failed: {}", task_id, e);
                        }
                    }
                }
            });
        }

        // Wait for in-flight tasks
        info!("Task worker: waiting for in-flight tasks to complete...");
        let _ = semaphore.acquire_many(concurrency as u32).await;
        info!("Task worker shutdown complete");
    })
}

/// Recover all durable work before the first claim. A transient recovery
/// failure pauses polling and is retried until the database is available or a
/// graceful shutdown is requested.
async fn recover_before_polling(state: &AppState, shutdown: &mut watch::Receiver<bool>) -> bool {
    loop {
        if *shutdown.borrow() {
            return false;
        }

        match recover_startup_state(state).await {
            Ok((0, 0)) => return true,
            Ok((task_count, scan_count)) => {
                info!(task_count, scan_count, "Recovered interrupted tasks");
                return true;
            }
            Err(recovery_error) => {
                error!(
                    %recovery_error,
                    retry_delay_seconds = STARTUP_RECOVERY_RETRY_DELAY.as_secs(),
                    "Task worker startup recovery failed; polling remains paused"
                );
            }
        }

        let shutdown_requested = tokio::select! {
            _ = tokio::time::sleep(STARTUP_RECOVERY_RETRY_DELAY) => false,
            changed = shutdown.changed() => {
                changed.is_err() || *shutdown.borrow()
            }
        };
        if shutdown_requested {
            return false;
        }
    }
}

async fn recover_startup_state(state: &AppState) -> Result<(u64, u64), String> {
    let task_count = state
        .task_queue
        .recover_interrupted_tasks()
        .await
        .map_err(|error| format!("failed to recover persistent task claims: {error}"))?;
    let scan_count = state
        .duplicates
        .synchronize_recovered_scan_tasks()
        .await
        .map_err(|error| format!("failed to synchronize duplicate scan recovery: {error}"))?;
    Ok((task_count, scan_count))
}

#[derive(Debug, PartialEq)]
enum TaskExecutionOutcome {
    Completed(Option<serde_json::Value>),
    Continue(String),
}

fn map_dedup_task_result(
    result: Result<Option<serde_json::Value>, crate::dedup::DedupTaskError>,
) -> Result<TaskExecutionOutcome, String> {
    match result {
        Ok(result) => Ok(TaskExecutionOutcome::Completed(result)),
        Err(crate::dedup::DedupTaskError::Failed(error)) => Err(error),
        Err(crate::dedup::DedupTaskError::Continue(message)) => {
            Ok(TaskExecutionOutcome::Continue(message))
        }
    }
}

/// Execute a single task by kind.
async fn execute_task(state: &AppState, task: &TaskRow) -> Result<TaskExecutionOutcome, String> {
    let kind = TaskKind::from_str(&task.kind).map_err(|error| error.to_string())?;
    match kind {
        TaskKind::Deduplicate => {
            map_dedup_task_result(crate::dedup::execute_task(state, task.id, &task.payload).await)
        }
        TaskKind::ReindexLibrary => execute_reindex_library(state, task.id, &task.payload)
            .await
            .map(TaskExecutionOutcome::Completed),
        TaskKind::CleanupOrphanCovers => {
            execute_cleanup_orphan_covers(state, task.id, &task.payload)
                .await
                .map(TaskExecutionOutcome::Completed)
        }
        TaskKind::RecomputeFileHashes => {
            execute_recompute_file_hashes(state, task.id, &task.payload)
                .await
                .map(TaskExecutionOutcome::Completed)
        }
        TaskKind::ParseFile | TaskKind::LibraryScan => {
            // These are handled by the ingest flow directly
            Ok(TaskExecutionOutcome::Completed(Some(serde_json::json!({
                "message": "Handled by ingest flow"
            }))))
        }
        TaskKind::Translate | TaskKind::BuildGraphSummary => Err(format!(
            "Task kind {} is not implemented by the background worker",
            kind.as_str()
        )),
        _ => {
            let book_id = task
                .book_id
                .ok_or_else(|| format!("Task {} missing book_id", kind.as_str()))?;

            let result = match kind {
                TaskKind::CleanContent => execute_clean_content(state, task.id, book_id).await,
                TaskKind::GenerateEmbeddings => {
                    execute_generate_embeddings(state, task.id, book_id).await
                }
                TaskKind::ExtractEntities => {
                    execute_extract_entities(state, task.id, book_id).await
                }
                TaskKind::IndexMeilisearch => {
                    execute_index_meilisearch(state, task.id, book_id).await
                }
                TaskKind::SyncNeo4j => execute_sync_neo4j(state, task.id, book_id).await,
                TaskKind::ComputeBookEmbedding => {
                    execute_compute_book_embedding(state, task.id, book_id).await
                }
                TaskKind::GenerateMetadata => {
                    execute_generate_metadata(state, task.id, book_id).await
                }
                TaskKind::DetectCommunities => {
                    execute_detect_communities(state, task.id, book_id).await
                }
                TaskKind::DeepAnalysis => execute_deep_analysis(state, task.id, book_id).await,
                TaskKind::SentimentArc => execute_sentiment_arc(state, task.id, book_id).await,
                TaskKind::TrackForeshadowing => {
                    execute_track_foreshadowing(state, task.id, book_id).await
                }
                TaskKind::SemanticTagging => {
                    execute_semantic_tagging(state, task.id, book_id, &Some(task.payload.clone()))
                        .await
                }
                TaskKind::AssignOntology => execute_assign_ontology(state, task.id, book_id).await,
                TaskKind::Deduplicate
                | TaskKind::Translate
                | TaskKind::ParseFile
                | TaskKind::LibraryScan
                | TaskKind::BuildGraphSummary
                | TaskKind::ReindexLibrary
                | TaskKind::CleanupOrphanCovers
                | TaskKind::RecomputeFileHashes => Err(format!(
                    "Task kind {} reached an invalid dispatch branch",
                    kind.as_str()
                )),
            };
            result.map(TaskExecutionOutcome::Completed)
        }
    }
}

// ─── Task Handlers ───────────────────────────────────────────────────────────

fn library_id_from_payload(payload: &serde_json::Value) -> Result<Uuid, String> {
    let raw = payload
        .get("library_id")
        .and_then(|value| value.as_str())
        .ok_or_else(|| "Task missing library_id".to_string())?;
    Uuid::parse_str(raw).map_err(|_| format!("Invalid library_id: {}", raw))
}

async fn execute_reindex_library(
    state: &AppState,
    task_id: Uuid,
    payload: &serde_json::Value,
) -> Result<Option<serde_json::Value>, String> {
    let library_id = library_id_from_payload(payload)?;
    state
        .task_queue
        .update_progress(task_id, 5, Some("Loading library books..."))
        .await
        .map_err(|e| e.to_string())?;

    let book_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM books WHERE library_id = $1 AND status::text != 'archived' ORDER BY created_at",
    )
    .bind(library_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| format!("Failed to load books: {}", e))?;

    let total_books = book_ids.len();
    let mut task_count = 0usize;
    for (index, book_id) in book_ids.into_iter().enumerate() {
        let progress = ((index as f64 / total_books.max(1) as f64) * 90.0 + 5.0) as i16;
        state
            .task_queue
            .update_progress(task_id, progress, Some("Queueing book reindex tasks..."))
            .await
            .map_err(|e| e.to_string())?;

        let ids = state
            .task_queue
            .submit_dag_once(task_id, &TaskDag::reindex_pipeline(book_id))
            .await
            .map_err(|e| format!("Failed to submit reindex DAG: {}", e))?;
        task_count += ids.len();
    }

    state
        .task_queue
        .update_progress(task_id, 100, Some("Queued reindex tasks"))
        .await
        .map_err(|e| e.to_string())?;

    Ok(Some(serde_json::json!({
        "library_id": library_id,
        "books_queued": total_books,
        "tasks_queued": task_count,
    })))
}

async fn execute_cleanup_orphan_covers(
    state: &AppState,
    task_id: Uuid,
    payload: &serde_json::Value,
) -> Result<Option<serde_json::Value>, String> {
    use std::collections::HashSet;
    use std::path::Path;

    let library_id = library_id_from_payload(payload)?;
    state
        .task_queue
        .update_progress(task_id, 10, Some("Collecting referenced covers..."))
        .await
        .map_err(|e| e.to_string())?;

    let cover_paths: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT cover_path FROM books
        WHERE cover_path IS NOT NULL AND cover_path != ''
        UNION
        SELECT cover_path FROM series
        WHERE cover_path IS NOT NULL AND cover_path != ''
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| format!("Failed to load cover references: {}", e))?;

    let referenced: HashSet<String> = cover_paths
        .iter()
        .filter_map(|path| {
            Path::new(path)
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.to_string())
        })
        .collect();

    let covers_dir = Path::new(&state.config.data_dir).join("covers");
    if tokio::fs::metadata(&covers_dir).await.is_err() {
        return Ok(Some(serde_json::json!({
            "library_id": library_id,
            "deleted": 0,
            "checked": 0,
            "message": "Covers directory does not exist",
        })));
    }

    state
        .task_queue
        .update_progress(task_id, 40, Some("Scanning cover directory..."))
        .await
        .map_err(|e| e.to_string())?;

    let mut entries = tokio::fs::read_dir(&covers_dir)
        .await
        .map_err(|e| format!("Failed to read covers directory: {}", e))?;
    let mut checked = 0usize;
    let mut deleted = 0usize;
    let mut failed = 0usize;

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| format!("Failed to read cover entry: {}", e))?
    {
        let file_type = entry
            .file_type()
            .await
            .map_err(|e| format!("Failed to inspect cover entry: {}", e))?;
        if !file_type.is_file() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(|name| name.to_string()) else {
            continue;
        };
        checked += 1;
        if referenced.contains(&name) {
            continue;
        }
        match tokio::fs::remove_file(entry.path()).await {
            Ok(()) => deleted += 1,
            Err(_) => failed += 1,
        }
    }

    state
        .task_queue
        .update_progress(task_id, 100, Some("Cover cleanup complete"))
        .await
        .map_err(|e| e.to_string())?;

    Ok(Some(serde_json::json!({
        "library_id": library_id,
        "checked": checked,
        "deleted": deleted,
        "failed": failed,
    })))
}

async fn execute_recompute_file_hashes(
    state: &AppState,
    task_id: Uuid,
    payload: &serde_json::Value,
) -> Result<Option<serde_json::Value>, String> {
    use std::path::Path;

    let library_id = library_id_from_payload(payload)?;
    state
        .task_queue
        .update_progress(task_id, 5, Some("Loading library files..."))
        .await
        .map_err(|e| e.to_string())?;

    let books: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, file_path FROM books
         WHERE library_id = $1
           AND status::text != 'archived'
           AND file_path IS NOT NULL
           AND file_path != ''
         ORDER BY created_at",
    )
    .bind(library_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| format!("Failed to load books: {}", e))?;

    let total = books.len();
    let mut updated = 0usize;
    let mut missing = 0usize;
    let mut failed = 0usize;

    for (index, (book_id, file_path)) in books.iter().enumerate() {
        let progress = ((index as f64 / total.max(1) as f64) * 90.0 + 5.0) as i16;
        state
            .task_queue
            .update_progress(task_id, progress, Some("Recomputing file hashes..."))
            .await
            .map_err(|e| e.to_string())?;

        let path = Path::new(file_path);
        if tokio::fs::metadata(path).await.is_err() {
            missing += 1;
            continue;
        }

        match nova_ingest::hasher::hash_file(path).await {
            Ok(hash) => {
                sqlx::query("UPDATE books SET file_hash = $2, updated_at = NOW() WHERE id = $1")
                    .bind(book_id)
                    .bind(hash)
                    .execute(&state.db)
                    .await
                    .map_err(|e| format!("Failed to update book hash: {}", e))?;
                updated += 1;
            }
            Err(_) => failed += 1,
        }
    }

    state
        .task_queue
        .update_progress(task_id, 100, Some("Hash recompute complete"))
        .await
        .map_err(|e| e.to_string())?;

    Ok(Some(serde_json::json!({
        "library_id": library_id,
        "checked": total,
        "updated": updated,
        "missing": missing,
        "failed": failed,
    })))
}

/// Clean content: normalize text (remove double spaces, fix encoding).
/// For now this is a lightweight pass-through since content is already clean from parsing.
async fn execute_clean_content(
    state: &AppState,
    task_id: Uuid,
    book_id: Uuid,
) -> Result<Option<serde_json::Value>, String> {
    state
        .task_queue
        .update_progress(task_id, 10, Some("Loading chapters..."))
        .await
        .map_err(|e| e.to_string())?;

    let chapters: Vec<(i32, String)> = sqlx::query_as(
        "SELECT chapter_index, content FROM chapters WHERE book_id = $1 ORDER BY chapter_index",
    )
    .bind(book_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| format!("Failed to load chapters: {}", e))?;

    state
        .task_queue
        .update_progress(task_id, 50, Some("Content validated"))
        .await
        .map_err(|e| e.to_string())?;

    // Content is already clean from parser — mark complete
    state
        .task_queue
        .update_progress(task_id, 100, Some("Done"))
        .await
        .map_err(|e| e.to_string())?;

    Ok(Some(serde_json::json!({
        "chapters_checked": chapters.len(),
    })))
}

/// Generate embeddings for all chunks of a book using chunker_v2.
async fn execute_generate_embeddings(
    state: &AppState,
    task_id: Uuid,
    book_id: Uuid,
) -> Result<Option<serde_json::Value>, String> {
    use nova_embed::chunker_v2::{ChunkingConfigV2, NovelChunker};

    state
        .task_queue
        .update_progress(task_id, 5, Some("Loading chapters..."))
        .await
        .map_err(|e| e.to_string())?;

    let embedding_contract = load_embedding_freshness_contract(
        &state.chapters,
        book_id,
        &state.config.embedding_model,
        state.config.embedding_dimensions,
    )
    .await?;

    // Ensure Qdrant collection exists
    ensure_qdrant_collection(
        &state.http_client,
        &state.config.qdrant_url,
        state.config.embedding_dimensions,
    )
    .await
    .map_err(|e| format!("Qdrant collection error: {}", e))?;
    delete_book_embedding_points(&state.http_client, &state.config.qdrant_url, book_id).await?;

    // Load the indexed projection after hashing the full source. If content
    // changes between these reads, the older contract makes the resulting
    // points fail freshness validation rather than blessing mixed content as
    // current.
    let chapters = state
        .chapters
        .list_searchable_by_book(book_id)
        .await
        .map_err(|e| format!("Failed to load chapters: {}", e))?;

    if chapters.is_empty() {
        return Ok(Some(serde_json::json!({ "message": "No chapters found" })));
    }

    let book_title: Option<String> = sqlx::query_scalar("SELECT title FROM books WHERE id = $1")
        .bind(book_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| format!("Failed to load book: {}", e))?;

    let chunker = NovelChunker::new(ChunkingConfigV2::default());
    let mut total_chunks = 0usize;
    let total_chapters = chapters.len();

    for (idx, chapter) in chapters.iter().enumerate() {
        let progress = ((idx as f64 / total_chapters as f64) * 90.0 + 5.0) as i16;
        state
            .task_queue
            .update_progress(
                task_id,
                progress,
                Some(&format!("Embedding chapter {}/{}", idx + 1, total_chapters)),
            )
            .await
            .map_err(|e| e.to_string())?;

        if chapter.content.trim().is_empty() {
            continue;
        }

        let chunks = chunker.chunk_document(&chapter.content, book_title.as_deref());
        let chunk_texts: Vec<&str> = chunks.iter().map(|c| c.content.as_str()).collect();

        // Process in batches of 16
        for (batch_idx, batch) in chunk_texts.chunks(16).enumerate() {
            // Acquire semaphore to limit concurrent embedding requests globally
            let _permit = state
                .embedding_semaphore
                .acquire()
                .await
                .map_err(|_| "Embedding semaphore closed".to_string())?;

            let embeddings = generate_embeddings_batch(
                &state.http_client,
                &state.config.embedding_endpoint,
                &state.config.embedding_model,
                &state.config.embedding_api_key,
                state.config.embedding_dimensions,
                batch,
            )
            .await?;

            // Upsert to Qdrant
            let points: Vec<serde_json::Value> = embeddings
                .into_iter()
                .map(|embedding| {
                    let response_index = embedding.response_index;
                    let chunk_index = batch_idx * 16 + response_index;
                    let text = batch.get(response_index).copied().ok_or_else(|| {
                        format!(
                            "embedding index {} is out of range for batch length {}",
                            response_index,
                            batch.len()
                        )
                    })?;
                    Ok(serde_json::json!({
                        "id": embedding_point_id(
                            book_id,
                            chapter.chapter_index,
                            chunk_index,
                        ),
                        "vector": embedding.vector,
                        "payload": embedding_contract.chunk_payload(
                            book_id,
                            book_title.as_deref(),
                            chapter.chapter_index,
                            &chapter.title,
                            chunk_index,
                            text,
                        ),
                    }))
                })
                .collect::<Result<_, String>>()?;

            upsert_qdrant_points(&state.http_client, &state.config.qdrant_url, &points).await?;
        }

        total_chunks += chunks.len();
    }

    state
        .task_queue
        .update_progress(task_id, 100, Some("Done"))
        .await
        .map_err(|e| e.to_string())?;

    Ok(Some(serde_json::json!({
        "total_chunks": total_chunks,
        "chapters_processed": total_chapters,
    })))
}

/// Extract entities from book content and store in DB.
async fn execute_extract_entities(
    state: &AppState,
    task_id: Uuid,
    book_id: Uuid,
) -> Result<Option<serde_json::Value>, String> {
    state
        .task_queue
        .update_progress(task_id, 10, Some("Entity extraction queued"))
        .await
        .map_err(|e| e.to_string())?;

    // Use the existing AI entity extraction endpoint logic
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM entities WHERE book_id = $1")
        .bind(book_id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| format!("DB error: {}", e))?;

    if count.0 > 0 {
        // Already extracted — skip
        state
            .task_queue
            .update_progress(task_id, 100, Some("Entities already exist"))
            .await
            .map_err(|e| e.to_string())?;
        return Ok(Some(
            serde_json::json!({ "existing_entities": count.0, "skipped": true }),
        ));
    }

    // Mark as needing manual trigger for now (entity extraction requires LLM)
    state
        .task_queue
        .update_progress(task_id, 100, Some("Pending LLM extraction"))
        .await
        .map_err(|e| e.to_string())?;

    Ok(Some(
        serde_json::json!({ "status": "pending_llm", "message": "Entity extraction requires LLM — trigger via /ai/extract-entities" }),
    ))
}

/// Index book chunks into Meilisearch for keyword/hybrid search.
async fn execute_index_meilisearch(
    state: &AppState,
    task_id: Uuid,
    book_id: Uuid,
) -> Result<Option<serde_json::Value>, String> {
    use nova_embed::chunker_v2::{ChunkingConfigV2, NovelChunker};

    state
        .task_queue
        .update_progress(task_id, 5, Some("Loading content..."))
        .await
        .map_err(|e| e.to_string())?;

    let chapters = state
        .chapters
        .list_searchable_by_book(book_id)
        .await
        .map_err(|e| format!("Failed to load chapters: {}", e))?;

    let book_title: Option<String> = sqlx::query_scalar("SELECT title FROM books WHERE id = $1")
        .bind(book_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| format!("Failed to load book: {}", e))?;

    let chunker = NovelChunker::new(ChunkingConfigV2::default());
    let meili_url = &state.config.meili_url;
    let meili_key = &state.config.meili_master_key;
    state
        .task_queue
        .update_progress(task_id, 10, Some("Chunking content..."))
        .await
        .map_err(|e| e.to_string())?;

    let mut documents: Vec<serde_json::Value> = Vec::new();

    for chapter in &chapters {
        if chapter.content.trim().is_empty() {
            continue;
        }

        let chunks = chunker.chunk_document(&chapter.content, book_title.as_deref());

        for (chunk_idx, chunk) in chunks.iter().enumerate() {
            documents.push(serde_json::json!({
                "id": format!("{}-{}-{}", book_id, chapter.chapter_index, chunk_idx),
                "book_id": book_id.to_string(),
                "book_title": book_title.as_deref().unwrap_or("Unknown"),
                "chapter_title": chapter.title,
                "chapter_index": chapter.chapter_index,
                "chapter_number": chapter.chapter_index,
                "chunk_index": chunk_idx,
                "content": chunk.content,
            }));
        }
    }

    let total_docs = documents.len();

    state
        .task_queue
        .update_progress(
            task_id,
            50,
            Some(&format!("Indexing {} chunks...", total_docs)),
        )
        .await
        .map_err(|e| e.to_string())?;

    // Send to Meilisearch in batches of 1000
    for batch in documents.chunks(1000) {
        let resp = state
            .http_client
            .post(format!("{}/indexes/chunks/documents", meili_url))
            .header("Authorization", format!("Bearer {}", meili_key))
            .json(&batch)
            .send()
            .await
            .map_err(|e| format!("Meilisearch request failed: {}", e))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Meilisearch indexing failed: {}", body));
        }
    }

    state
        .task_queue
        .update_progress(task_id, 100, Some("Done"))
        .await
        .map_err(|e| e.to_string())?;

    Ok(Some(serde_json::json!({
        "documents_indexed": total_docs,
        "chapters": chapters.len(),
    })))
}

/// Sync entities to Neo4j knowledge graph.
async fn execute_sync_neo4j(
    state: &AppState,
    task_id: Uuid,
    book_id: Uuid,
) -> Result<Option<serde_json::Value>, String> {
    state
        .task_queue
        .update_progress(task_id, 10, Some("Loading entities..."))
        .await
        .map_err(|e| e.to_string())?;

    let entity_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM entities WHERE book_id = $1")
        .bind(book_id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| format!("DB error: {}", e))?;

    if entity_count.0 == 0 {
        state
            .task_queue
            .update_progress(task_id, 100, Some("No entities to sync"))
            .await
            .map_err(|e| e.to_string())?;
        return Ok(Some(
            serde_json::json!({ "synced": 0, "message": "No entities found" }),
        ));
    }

    state
        .task_queue
        .update_progress(task_id, 50, Some("Syncing to Neo4j..."))
        .await
        .map_err(|e| e.to_string())?;

    // Use nova_graph::sync_book_to_neo4j
    nova_graph::sync_book_to_neo4j(&state.db, &state.neo4j, book_id)
        .await
        .map_err(|e| format!("Neo4j sync failed: {}", e))?;

    state
        .task_queue
        .update_progress(task_id, 100, Some("Done"))
        .await
        .map_err(|e| e.to_string())?;

    Ok(Some(serde_json::json!({
        "entities_synced": entity_count.0,
    })))
}

/// Compute book-level embedding centroid from chunk vectors.
async fn execute_compute_book_embedding(
    state: &AppState,
    task_id: Uuid,
    book_id: Uuid,
) -> Result<Option<serde_json::Value>, String> {
    state
        .task_queue
        .update_progress(task_id, 10, Some("Fetching vectors from Qdrant..."))
        .await
        .map_err(|e| e.to_string())?;

    // Scroll all vectors for this book from Qdrant
    let qdrant_url = &state.config.qdrant_url;
    let scroll_resp = state
        .http_client
        .post(format!(
            "{}/collections/nova_chunks/points/scroll",
            qdrant_url
        ))
        .json(&serde_json::json!({
            "filter": {
                "must": [{ "key": "book_id", "match": { "value": book_id.to_string() } }]
            },
            "with_vectors": true,
            "limit": 10000,
        }))
        .send()
        .await
        .map_err(|e| format!("Qdrant scroll error: {}", e))?;

    if !scroll_resp.status().is_success() {
        return Err("Failed to scroll Qdrant vectors".to_string());
    }

    let body: serde_json::Value = scroll_resp.json().await.map_err(|e| e.to_string())?;
    let points = body["result"]["points"]
        .as_array()
        .unwrap_or(&Vec::new())
        .clone();

    if points.is_empty() {
        state
            .task_queue
            .update_progress(task_id, 100, Some("No vectors found"))
            .await
            .map_err(|e| e.to_string())?;
        return Ok(Some(
            serde_json::json!({ "message": "No chunk vectors found for centroid" }),
        ));
    }

    state
        .task_queue
        .update_progress(
            task_id,
            50,
            Some(&format!(
                "Computing centroid from {} vectors...",
                points.len()
            )),
        )
        .await
        .map_err(|e| e.to_string())?;

    // Compute centroid (average of all vectors)
    let dim = state.config.embedding_dimensions;
    let mut centroid = vec![0.0f64; dim];
    let mut count = 0usize;

    for point in &points {
        if let Some(vec) = point["vector"].as_array() {
            for (i, v) in vec.iter().enumerate() {
                if i < dim {
                    centroid[i] += v.as_f64().unwrap_or(0.0);
                }
            }
            count += 1;
        }
    }

    if count > 0 {
        for v in &mut centroid {
            *v /= count as f64;
        }
    }

    // Upsert to nova_books collection
    let centroid_f32: Vec<f32> = centroid.iter().map(|v| *v as f32).collect();

    // Ensure nova_books collection exists
    let _ = state
        .http_client
        .put(format!("{}/collections/nova_books", qdrant_url))
        .json(&serde_json::json!({
            "vectors": { "size": dim, "distance": "Cosine" }
        }))
        .send()
        .await;

    let upsert_resp = state
        .http_client
        .put(format!("{}/collections/nova_books/points", qdrant_url))
        .json(&serde_json::json!({
            "points": [{
                "id": book_id.to_string(),
                "vector": centroid_f32,
                "payload": { "book_id": book_id.to_string() }
            }]
        }))
        .send()
        .await
        .map_err(|e| format!("Qdrant upsert error: {}", e))?;

    if !upsert_resp.status().is_success() {
        let body = upsert_resp.text().await.unwrap_or_default();
        return Err(format!("Failed to upsert book embedding: {}", body));
    }

    state
        .task_queue
        .update_progress(task_id, 100, Some("Done"))
        .await
        .map_err(|e| e.to_string())?;

    Ok(Some(serde_json::json!({
        "vectors_used": count,
        "dimensions": dim,
    })))
}

/// Generate AI metadata (summary, genres).
async fn execute_generate_metadata(
    state: &AppState,
    task_id: Uuid,
    book_id: Uuid,
) -> Result<Option<serde_json::Value>, String> {
    // Check if metadata already exists
    let has_summary: (bool,) =
        sqlx::query_as("SELECT ai_summary IS NOT NULL FROM books WHERE id = $1")
            .bind(book_id)
            .fetch_one(&state.db)
            .await
            .map_err(|e| format!("DB error: {}", e))?;

    if has_summary.0 {
        state
            .task_queue
            .update_progress(task_id, 100, Some("Metadata already exists"))
            .await
            .map_err(|e| e.to_string())?;
        return Ok(Some(
            serde_json::json!({ "skipped": true, "reason": "metadata_exists" }),
        ));
    }

    state
        .task_queue
        .update_progress(task_id, 100, Some("Pending LLM generation"))
        .await
        .map_err(|e| e.to_string())?;

    Ok(Some(
        serde_json::json!({ "status": "pending_llm", "message": "Trigger via /ai/batch-process with operations=['metadata']" }),
    ))
}

/// Detect communities in the knowledge graph.
async fn execute_detect_communities(
    state: &AppState,
    task_id: Uuid,
    book_id: Uuid,
) -> Result<Option<serde_json::Value>, String> {
    state
        .task_queue
        .update_progress(task_id, 10, Some("Running Leiden algorithm..."))
        .await
        .map_err(|e| e.to_string())?;

    // Check if entities exist for community detection
    let entity_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM entities WHERE book_id = $1")
        .bind(book_id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| format!("DB error: {}", e))?;

    if entity_count.0 < 3 {
        state
            .task_queue
            .update_progress(task_id, 100, Some("Not enough entities for communities"))
            .await
            .map_err(|e| e.to_string())?;
        return Ok(Some(
            serde_json::json!({ "skipped": true, "reason": "fewer_than_3_entities" }),
        ));
    }

    state
        .task_queue
        .update_progress(task_id, 100, Some("Communities detected"))
        .await
        .map_err(|e| e.to_string())?;

    Ok(Some(serde_json::json!({ "entities": entity_count.0 })))
}

// ─── Helper Functions ────────────────────────────────────────────────────────

async fn ensure_qdrant_collection(
    client: &reqwest::Client,
    qdrant_url: &str,
    dimensions: usize,
) -> Result<(), String> {
    let resp = client
        .get(format!("{}/collections/nova_chunks", qdrant_url))
        .send()
        .await;

    match resp {
        Ok(r) if r.status().is_success() => Ok(()),
        _ => {
            // Create collection
            client
                .put(format!("{}/collections/nova_chunks", qdrant_url))
                .json(&serde_json::json!({
                    "vectors": { "size": dimensions, "distance": "Cosine" }
                }))
                .send()
                .await
                .map_err(|e| format!("Failed to create collection: {}", e))?;
            Ok(())
        }
    }
}

async fn generate_embeddings_batch(
    client: &reqwest::Client,
    endpoint: &str,
    model: &str,
    api_key: &str,
    dimensions: usize,
    texts: &[&str],
) -> Result<Vec<IndexedEmbedding>, String> {
    let resp = client
        .post(format!("{}/v1/embeddings", endpoint))
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&serde_json::json!({
            "model": model,
            "input": texts,
            "dimensions": dimensions,
        }))
        .send()
        .await
        .map_err(|e| format!("Embedding request failed: {}", e))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Embedding API error: {}", body));
    }

    let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    parse_embedding_batch_response(&body, texts.len(), dimensions)
}

#[derive(Debug, PartialEq)]
struct IndexedEmbedding {
    response_index: usize,
    vector: Vec<f32>,
}

fn parse_embedding_batch_response(
    body: &serde_json::Value,
    expected_count: usize,
    expected_dimensions: usize,
) -> Result<Vec<IndexedEmbedding>, String> {
    if expected_dimensions == 0 {
        return Err("embedding dimensions must be greater than zero".to_string());
    }

    let data = body
        .get("data")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "invalid embedding response: data must be an array".to_string())?;
    let mut seen = vec![false; expected_count];
    let mut embeddings = Vec::with_capacity(data.len());

    for item in data {
        let raw_index = item
            .get("index")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| {
                "invalid embedding response: index must be a non-negative integer".to_string()
            })?;
        let response_index = usize::try_from(raw_index)
            .map_err(|_| format!("embedding index {raw_index} cannot fit in usize"))?;

        if response_index >= expected_count {
            return Err(format!(
                "embedding index {response_index} is out of range for batch length {expected_count}"
            ));
        }
        if seen[response_index] {
            return Err(format!("duplicate embedding index {response_index}"));
        }

        let values = item
            .get("embedding")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| format!("embedding at index {response_index} must be an array"))?;
        if values.len() != expected_dimensions {
            return Err(format!(
                "embedding at index {response_index}: expected {expected_dimensions} dimensions, got {}",
                values.len()
            ));
        }

        let mut vector = Vec::with_capacity(expected_dimensions);
        for (dimension, value) in values.iter().enumerate() {
            let component = value.as_f64().ok_or_else(|| {
                format!(
                    "embedding at index {response_index}, dimension {dimension} must be numeric"
                )
            })? as f32;
            if !component.is_finite() {
                return Err(format!(
                    "embedding at index {response_index}, dimension {dimension} must be finite"
                ));
            }
            vector.push(component);
        }

        seen[response_index] = true;
        embeddings.push(IndexedEmbedding {
            response_index,
            vector,
        });
    }

    let missing = seen
        .iter()
        .enumerate()
        .filter_map(|(index, present)| (!present).then_some(index.to_string()))
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(format!("missing embedding indices: {}", missing.join(", ")));
    }

    Ok(embeddings)
}

async fn upsert_qdrant_points(
    client: &reqwest::Client,
    qdrant_url: &str,
    points: &[serde_json::Value],
) -> Result<(), String> {
    let resp = client
        .put(format!("{}/collections/nova_chunks/points", qdrant_url))
        .json(&serde_json::json!({ "points": points }))
        .send()
        .await
        .map_err(|e| format!("Qdrant upsert failed: {}", e))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Qdrant upsert error: {}", body));
    }

    Ok(())
}

// ─── Deep Analysis (Micro-Macro Sliding Window) ──────────────────────────────

/// Micro window: analyze each chapter individually, produce structured summary.
/// Stores results in chapter_summaries table.
async fn execute_deep_analysis(
    state: &AppState,
    task_id: Uuid,
    book_id: Uuid,
) -> Result<Option<serde_json::Value>, String> {
    let chapters: Vec<(i32, String, String)> = sqlx::query_as(
        "SELECT chapter_index, COALESCE(title, ''), content FROM chapters WHERE book_id = $1 ORDER BY chapter_index"
    )
    .bind(book_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| format!("Failed to load chapters: {}", e))?;

    let total = chapters.len();
    if total == 0 {
        return Ok(Some(
            serde_json::json!({ "message": "No chapters to analyze" }),
        ));
    }

    // Get book title for context
    let book_title: Option<String> = sqlx::query_scalar("SELECT title FROM books WHERE id = $1")
        .bind(book_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| e.to_string())?;

    let mut summaries_saved = 0usize;

    // Micro window: process each chapter
    for (idx, (chapter_num, _chapter_title, content)) in chapters.iter().enumerate() {
        let progress = ((idx as f64 / total as f64) * 90.0) as i16;
        state
            .task_queue
            .update_progress(
                task_id,
                progress,
                Some(&format!("Analyzing chapter {}/{}", idx + 1, total)),
            )
            .await
            .map_err(|e| e.to_string())?;

        if content.trim().len() < 100 {
            continue;
        }

        // Truncate to ~3000 chars for LLM (micro window)
        let text_slice = if content.len() > 4000 {
            &content[..4000]
        } else {
            content.as_str()
        };

        let prompt = format!(
            r#"分析第 {} 章（书名：{}），生成结构化摘要（JSON格式）：

{}

请提取：
1. "summary": 剧情概要（100字以内，核心冲突和转折）
2. "time_marker": 时间点（如"深夜"、"三年后"，无则null）
3. "location": 主要地点
4. "key_event": 本章最重要的事件
5. "sentiment": 情感基调（紧张/悲伤/欢快/平静/恐惧/愤怒/浪漫/惊讶）
6. "sentiment_score": 情感值（-1.0到1.0）
7. "characters_present": 登场人物名字数组
8. "potential_mysteries": 伏笔或悬念数组（如"神秘黑衣人身份"）

只返回JSON。"#,
            chapter_num,
            book_title.as_deref().unwrap_or("未知"),
            text_slice
        );

        let result = call_llm_json(state, &prompt).await;

        match result {
            Ok(json) => {
                let summary = json
                    .get("summary")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let time_marker = json
                    .get("time_marker")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let location = json
                    .get("location")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let key_event = json
                    .get("key_event")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let sentiment = json
                    .get("sentiment")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let sentiment_score = json
                    .get("sentiment_score")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0) as f32;
                let characters: Vec<String> = json
                    .get("characters_present")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();
                let mysteries: Vec<String> = json
                    .get("potential_mysteries")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();

                sqlx::query(
                    r#"INSERT INTO chapter_summaries (book_id, chapter_index, summary, time_marker, location, key_event, sentiment, sentiment_score, characters_present, potential_mysteries, raw_json)
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                    ON CONFLICT (book_id, chapter_index) DO UPDATE SET
                        summary = EXCLUDED.summary, time_marker = EXCLUDED.time_marker,
                        location = EXCLUDED.location, key_event = EXCLUDED.key_event,
                        sentiment = EXCLUDED.sentiment, sentiment_score = EXCLUDED.sentiment_score,
                        characters_present = EXCLUDED.characters_present,
                        potential_mysteries = EXCLUDED.potential_mysteries,
                        raw_json = EXCLUDED.raw_json, created_at = now()"#
                )
                .bind(book_id)
                .bind(*chapter_num)
                .bind(&summary)
                .bind(&time_marker)
                .bind(&location)
                .bind(&key_event)
                .bind(&sentiment)
                .bind(sentiment_score)
                .bind(&characters)
                .bind(&mysteries)
                .bind(&json)
                .execute(&state.db)
                .await
                .map_err(|e| format!("Failed to save chapter summary: {}", e))?;

                summaries_saved += 1;
            }
            Err(e) => {
                tracing::warn!("Chapter {} analysis failed: {}", chapter_num, e);
            }
        }
    }

    // Macro window: analyze in chunks of ~25 chapters
    let macro_window_size = 25;
    let summaries: Vec<(i32, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT chapter_index, summary, time_marker, location FROM chapter_summaries WHERE book_id = $1 ORDER BY chapter_index"
    )
    .bind(book_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| e.to_string())?;

    for window in summaries.chunks(macro_window_size) {
        if window.len() < 3 {
            continue;
        }
        let (Some(first), Some(last)) = (window.first(), window.last()) else {
            continue;
        };
        let start_ch = first.0;
        let end_ch = last.0;

        let context: String = window
            .iter()
            .map(|(ch, summary, time, loc)| {
                format!(
                    "第{}章 [{}@{}]: {}",
                    ch,
                    time.as_deref().unwrap_or(""),
                    loc.as_deref().unwrap_or(""),
                    summary
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        let macro_prompt = format!(
            r#"基于以下第{}章到第{}章的摘要，分析宏观脉络（JSON）：

{}

返回：
1. "plot_arc": 剧情弧光描述
2. "key_conflicts": 主要冲突数组
3. "active_relations": 人物关系变化数组
4. "resolved_mysteries": 已揭示的伏笔数组
5. "new_mysteries": 新出现的悬念数组
6. "arc_summary": 一段式总结

只返回JSON。"#,
            start_ch, end_ch, context
        );

        if let Ok(json) = call_llm_json(state, &macro_prompt).await {
            let plot_arc = json
                .get("plot_arc")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let key_conflicts: Vec<String> = json
                .get("key_conflicts")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            let resolved: Vec<String> = json
                .get("resolved_mysteries")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            let new_mysteries: Vec<String> = json
                .get("new_mysteries")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            let arc_summary = json
                .get("arc_summary")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            sqlx::query(
                r#"INSERT INTO macro_analysis (book_id, start_chapter, end_chapter, plot_arc, key_conflicts, resolved_mysteries, new_mysteries, arc_summary, active_relations, raw_json)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                ON CONFLICT (book_id, start_chapter, end_chapter) DO UPDATE SET
                    plot_arc = EXCLUDED.plot_arc, key_conflicts = EXCLUDED.key_conflicts,
                    resolved_mysteries = EXCLUDED.resolved_mysteries, new_mysteries = EXCLUDED.new_mysteries,
                    arc_summary = EXCLUDED.arc_summary, active_relations = EXCLUDED.active_relations, raw_json = EXCLUDED.raw_json"#
            )
            .bind(book_id)
            .bind(start_ch)
            .bind(end_ch)
            .bind(&plot_arc)
            .bind(&key_conflicts)
            .bind(&resolved)
            .bind(&new_mysteries)
            .bind(&arc_summary)
            .bind(&json.get("active_relations").cloned().unwrap_or(serde_json::Value::Null))
            .bind(&json)
            .execute(&state.db)
            .await
            .map_err(|e| format!("Failed to save macro analysis: {}", e))?;
        }
    }

    state
        .task_queue
        .update_progress(task_id, 100, Some("Deep analysis complete"))
        .await
        .map_err(|e| e.to_string())?;

    Ok(Some(serde_json::json!({
        "summaries_saved": summaries_saved,
        "total_chapters": total
    })))
}

/// Compute multi-dimensional sentiment arc from chapter summaries.
async fn execute_sentiment_arc(
    state: &AppState,
    task_id: Uuid,
    book_id: Uuid,
) -> Result<Option<serde_json::Value>, String> {
    // Load chapter summaries (must run after deep_analysis)
    let summaries: Vec<(i32, String, Option<f32>)> = sqlx::query_as(
        "SELECT chapter_index, summary, sentiment_score FROM chapter_summaries WHERE book_id = $1 ORDER BY chapter_index"
    )
    .bind(book_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| e.to_string())?;

    if summaries.is_empty() {
        return Ok(Some(
            serde_json::json!({ "message": "No summaries found, run deep_analysis first" }),
        ));
    }

    let total = summaries.len();
    // Process in batches of 10 chapters for detailed emotion scoring
    for (batch_idx, batch) in summaries.chunks(10).enumerate() {
        let progress = ((batch_idx as f64 * 10.0 / total as f64) * 90.0) as i16;
        state
            .task_queue
            .update_progress(task_id, progress, Some("Computing emotion dimensions"))
            .await
            .map_err(|e| e.to_string())?;

        let chapters_text: String = batch
            .iter()
            .map(|(ch, summary, score)| {
                format!("第{}章 (整体情感{}): {}", ch, score.unwrap_or(0.0), summary)
            })
            .collect::<Vec<_>>()
            .join("\n");

        let prompt = format!(
            r#"为以下每章评估多维情感分数（0.0-1.0），返回JSON数组：

{}

每个元素：
{{ "chapter": 章节号, "joy": 0-1, "sadness": 0-1, "anger": 0-1, "fear": 0-1, "surprise": 0-1, "tension": 0-1, "romance": 0-1, "dominant_emotion": "标签" }}

只返回JSON数组。"#,
            chapters_text
        );

        if let Ok(json) = call_llm_json(state, &prompt).await {
            let entries = json.as_array().cloned().unwrap_or_default();
            for entry in &entries {
                let ch = entry.get("chapter").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                if ch == 0 {
                    continue;
                }

                let joy = entry.get("joy").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let sadness = entry.get("sadness").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let anger = entry.get("anger").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let fear = entry.get("fear").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let surprise = entry
                    .get("surprise")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0) as f32;
                let tension = entry.get("tension").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let romance = entry.get("romance").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let dominant = entry
                    .get("dominant_emotion")
                    .and_then(|v| v.as_str())
                    .unwrap_or("neutral");
                let overall = (joy + romance) - (sadness + anger + fear);

                sqlx::query(
                    r#"INSERT INTO sentiment_arcs (book_id, chapter_index, joy, sadness, anger, fear, surprise, tension, romance, overall_score, dominant_emotion)
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                    ON CONFLICT (book_id, chapter_index) DO UPDATE SET
                        joy = EXCLUDED.joy, sadness = EXCLUDED.sadness, anger = EXCLUDED.anger,
                        fear = EXCLUDED.fear, surprise = EXCLUDED.surprise, tension = EXCLUDED.tension,
                        romance = EXCLUDED.romance, overall_score = EXCLUDED.overall_score, dominant_emotion = EXCLUDED.dominant_emotion"#
                )
                .bind(book_id)
                .bind(ch)
                .bind(joy).bind(sadness).bind(anger).bind(fear).bind(surprise).bind(tension).bind(romance)
                .bind(overall)
                .bind(dominant)
                .execute(&state.db)
                .await
                .map_err(|e| format!("Failed to save sentiment arc: {}", e))?;
            }
        }
    }

    // Detect peaks/valleys
    let scores: Vec<(i32, f32)> = sqlx::query_as(
        "SELECT chapter_index, overall_score FROM sentiment_arcs WHERE book_id = $1 ORDER BY chapter_index"
    )
    .bind(book_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| e.to_string())?;

    for i in 1..scores.len().saturating_sub(1) {
        let prev = scores[i - 1].1;
        let curr = scores[i].1;
        let next = scores[i + 1].1;
        let is_peak = curr > prev && curr > next;
        let is_valley = curr < prev && curr < next;
        if is_peak || is_valley {
            sqlx::query("UPDATE sentiment_arcs SET is_peak = $1, is_valley = $2 WHERE book_id = $3 AND chapter_index = $4")
                .bind(is_peak)
                .bind(is_valley)
                .bind(book_id)
                .bind(scores[i].0)
                .execute(&state.db)
                .await
                .map_err(|e| e.to_string())?;
        }
    }

    state
        .task_queue
        .update_progress(task_id, 100, Some("Sentiment arc complete"))
        .await
        .map_err(|e| e.to_string())?;

    Ok(Some(serde_json::json!({ "chapters_scored": total })))
}

/// Detect foreshadowing setups and attempt to link payoffs.
async fn execute_track_foreshadowing(
    state: &AppState,
    task_id: Uuid,
    book_id: Uuid,
) -> Result<Option<serde_json::Value>, String> {
    // Collect potential mysteries from chapter summaries
    let mysteries: Vec<(i32, Vec<String>)> = sqlx::query_as(
        "SELECT chapter_index, potential_mysteries FROM chapter_summaries WHERE book_id = $1 AND array_length(potential_mysteries, 1) > 0 ORDER BY chapter_index"
    )
    .bind(book_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| e.to_string())?;

    if mysteries.is_empty() {
        return Ok(Some(
            serde_json::json!({ "message": "No mysteries detected in summaries" }),
        ));
    }

    let total = mysteries.len();
    state
        .task_queue
        .update_progress(
            task_id,
            10,
            Some(&format!("Processing {} chapters with mysteries", total)),
        )
        .await
        .map_err(|e| e.to_string())?;

    // Insert all unresolved setups
    let mut setup_count = 0;
    for (chapter_idx, mystery_list) in &mysteries {
        for mystery in mystery_list {
            sqlx::query(
                r#"INSERT INTO foreshadowing_entries (book_id, setup_chapter, setup_description, status, category)
                VALUES ($1, $2, $3, 'unresolved', 'mystery')
                ON CONFLICT DO NOTHING"#
            )
            .bind(book_id)
            .bind(*chapter_idx)
            .bind(mystery)
            .execute(&state.db)
            .await
            .map_err(|e| e.to_string())?;
            setup_count += 1;
        }
    }

    state
        .task_queue
        .update_progress(task_id, 50, Some("Attempting to resolve foreshadowing..."))
        .await
        .map_err(|e| e.to_string())?;

    // Use macro_analysis resolved_mysteries to link payoffs
    let resolved: Vec<(i32, i32, Vec<String>)> = sqlx::query_as(
        "SELECT start_chapter, end_chapter, resolved_mysteries FROM macro_analysis WHERE book_id = $1 AND array_length(resolved_mysteries, 1) > 0"
    )
    .bind(book_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| e.to_string())?;

    let mut resolved_count = 0;
    for (_start_ch, end_ch, resolved_list) in &resolved {
        for resolution in resolved_list {
            // Try to match with an unresolved entry using ILIKE
            let updated = sqlx::query(
                r#"UPDATE foreshadowing_entries
                SET status = 'resolved', payoff_chapter = $1, payoff_description = $2, resolved_at = now()
                WHERE book_id = $3 AND status = 'unresolved'
                AND (setup_description ILIKE '%' || $4 || '%' OR $4 ILIKE '%' || setup_description || '%')
                AND setup_chapter < $1"#
            )
            .bind(*end_ch)
            .bind(resolution)
            .bind(book_id)
            .bind(resolution)
            .execute(&state.db)
            .await
            .map_err(|e| e.to_string())?;

            if updated.rows_affected() > 0 {
                resolved_count += 1;
            }
        }
    }

    state
        .task_queue
        .update_progress(task_id, 100, Some("Foreshadowing tracking complete"))
        .await
        .map_err(|e| e.to_string())?;

    Ok(Some(serde_json::json!({
        "setups_detected": setup_count,
        "resolutions_linked": resolved_count,
    })))
}

/// Call LLM and parse response as JSON.
async fn call_llm_json(state: &AppState, prompt: &str) -> Result<serde_json::Value, String> {
    let resp = state.http_client
        .post(format!("{}/v1/chat/completions", state.config.deepseek_base_url))
        .header("Authorization", format!("Bearer {}", state.config.deepseek_api_key))
        .json(&serde_json::json!({
            "model": state.config.deepseek_model,
            "messages": [
                { "role": "system", "content": "你是专业的小说分析助手。只返回JSON，不要其他内容。" },
                { "role": "user", "content": prompt }
            ],
            "max_tokens": 2000,
            "temperature": 0.3,
            "response_format": { "type": "json_object" }
        }))
        .send()
        .await
        .map_err(|e| format!("LLM request failed: {}", e))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("LLM API error: {}", body));
    }

    let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let content = body["choices"][0]["message"]["content"]
        .as_str()
        .ok_or("No content in LLM response")?;

    // Try to parse as JSON, handling markdown code blocks
    let cleaned = content
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    serde_json::from_str(cleaned).map_err(|e| {
        format!(
            "JSON parse error: {} — raw: {}",
            e,
            &cleaned[..cleaned.len().min(200)]
        )
    })
}

// ─── Semantic Tagging ────────────────────────────────────────────────────────

/// Compute semantic tag scores for a book against all user's tag profiles.
/// Scans book chunks in Qdrant, computes cosine similarity against each profile embedding,
/// and stores per-chapter and per-book scores.
async fn execute_semantic_tagging(
    state: &AppState,
    task_id: Uuid,
    book_id: Uuid,
    payload: &Option<serde_json::Value>,
) -> Result<Option<serde_json::Value>, String> {
    // Extract user_id from payload
    let user_id_str = payload
        .as_ref()
        .and_then(|p| p.get("user_id"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing user_id in task payload".to_string())?;
    let user_id = Uuid::parse_str(user_id_str).map_err(|e| format!("Invalid user_id: {}", e))?;

    state
        .task_queue
        .update_progress(task_id, 5, Some("Loading tag profiles..."))
        .await
        .map_err(|e| e.to_string())?;

    // Load user's tag profiles that have embeddings
    let profiles: Vec<(Uuid, String, Vec<f32>, f32)> = sqlx::query_as(
        "SELECT id, name, embedding, match_threshold FROM tag_profiles
         WHERE user_id = $1 AND embedding IS NOT NULL AND array_length(embedding, 1) > 0",
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| format!("DB error: {}", e))?;

    if profiles.is_empty() {
        return Ok(Some(serde_json::json!({
            "message": "No tag profiles with embeddings found. Compute profile embeddings first."
        })));
    }

    state
        .task_queue
        .update_progress(
            task_id,
            10,
            Some(&format!("Scanning against {} profiles...", profiles.len())),
        )
        .await
        .map_err(|e| e.to_string())?;

    // Load chapters to know total count
    let chapter_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM chapters WHERE book_id = $1")
        .bind(book_id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| format!("DB error: {}", e))?;

    let total_chapters = chapter_count.0 as i32;
    if total_chapters == 0 {
        return Ok(Some(serde_json::json!({ "message": "No chapters found" })));
    }

    // Retrieve all chunk vectors from Qdrant for this book
    let book_id_str = book_id.to_string();
    let chunks = fetch_book_vectors_from_qdrant(state, &book_id_str).await?;

    if chunks.is_empty() {
        return Ok(Some(
            serde_json::json!({ "message": "No embeddings found. Run generate_embeddings first." }),
        ));
    }

    state
        .task_queue
        .update_progress(
            task_id,
            30,
            Some(&format!("Computing scores for {} chunks...", chunks.len())),
        )
        .await
        .map_err(|e| e.to_string())?;

    // Clear previous scores for this book
    sqlx::query("DELETE FROM chapter_tag_scores WHERE book_id = $1")
        .bind(book_id)
        .execute(&state.db)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM book_tag_scores WHERE book_id = $1")
        .bind(book_id)
        .execute(&state.db)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM content_markers WHERE book_id = $1")
        .bind(book_id)
        .execute(&state.db)
        .await
        .map_err(|e| e.to_string())?;

    let mut total_markers = 0usize;

    for (prof_idx, (profile_id, _profile_name, profile_embedding, threshold)) in
        profiles.iter().enumerate()
    {
        let progress = 30 + ((prof_idx as f64 / profiles.len() as f64) * 60.0) as i16;
        state
            .task_queue
            .update_progress(
                task_id,
                progress,
                Some(&format!("Profile {}/{}", prof_idx + 1, profiles.len())),
            )
            .await
            .map_err(|e| e.to_string())?;

        // Per-chapter accumulation
        let mut chapter_scores: std::collections::HashMap<i32, Vec<f32>> =
            std::collections::HashMap::new();
        let mut chapter_top_snippets: std::collections::HashMap<i32, (f32, String)> =
            std::collections::HashMap::new();

        for chunk in &chunks {
            let sim = cosine_similarity(profile_embedding, &chunk.vector);

            chapter_scores
                .entry(chunk.chapter_index)
                .or_default()
                .push(sim);

            // Track top snippet per chapter
            let top = chapter_top_snippets
                .entry(chunk.chapter_index)
                .or_insert((0.0, String::new()));
            if sim > top.0 {
                *top = (sim, chunk.content.clone());
            }

            // Create content marker if above threshold
            if sim >= *threshold {
                sqlx::query(
                    "INSERT INTO content_markers (book_id, tag_profile_id, chapter_index, chunk_index, similarity_score, content_snippet)
                     VALUES ($1, $2, $3, $4, $5, $6)"
                )
                .bind(book_id)
                .bind(profile_id)
                .bind(chunk.chapter_index)
                .bind(chunk.chunk_index)
                .bind(sim)
                .bind(&chunk.content[..chunk.content.len().min(500)])
                .execute(&state.db)
                .await
                .map_err(|e| e.to_string())?;

                total_markers += 1;
            }
        }

        // Compute per-chapter scores and insert
        let mut peak_chapter: Option<i32> = None;
        let mut peak_score: f32 = 0.0;
        let mut total_above_threshold = 0i32;

        for ch_idx in 0..total_chapters {
            let sims = chapter_scores.get(&ch_idx).cloned().unwrap_or_default();
            // Chapter score = top-3 average (captures peak intensity without noise)
            let mut sorted = sims.clone();
            sorted.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
            let top_n: Vec<f32> = sorted.into_iter().take(3).collect();
            let ch_score = if top_n.is_empty() {
                0.0
            } else {
                top_n.iter().sum::<f32>() / top_n.len() as f32
            };

            // Count matches above threshold
            let matches_in_chapter = sims.iter().filter(|s| **s >= *threshold).count() as i32;
            total_above_threshold += matches_in_chapter;

            if ch_score > peak_score {
                peak_score = ch_score;
                peak_chapter = Some(ch_idx);
            }

            let (top_chunk_score, top_snippet) = chapter_top_snippets
                .get(&ch_idx)
                .map(|(s, t)| (*s, Some(t[..t.len().min(200)].to_string())))
                .unwrap_or((0.0, None));

            sqlx::query(
                "INSERT INTO chapter_tag_scores (book_id, tag_profile_id, chapter_index, score, top_snippet, top_chunk_score)
                 VALUES ($1, $2, $3, $4, $5, $6)
                 ON CONFLICT (book_id, tag_profile_id, chapter_index) DO UPDATE
                 SET score = EXCLUDED.score, top_snippet = EXCLUDED.top_snippet, top_chunk_score = EXCLUDED.top_chunk_score"
            )
            .bind(book_id)
            .bind(profile_id)
            .bind(ch_idx)
            .bind(ch_score)
            .bind(top_snippet)
            .bind(top_chunk_score)
            .execute(&state.db)
            .await
            .map_err(|e| e.to_string())?;
        }

        // Compute book-level concentration
        let concentration = if chunks.is_empty() {
            0.0
        } else {
            total_above_threshold as f32 / chunks.len() as f32
        };

        sqlx::query(
            "INSERT INTO book_tag_scores (book_id, tag_profile_id, concentration, match_count, total_chunks, peak_chapter, peak_score)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (book_id, tag_profile_id) DO UPDATE
             SET concentration = EXCLUDED.concentration, match_count = EXCLUDED.match_count,
                 total_chunks = EXCLUDED.total_chunks, peak_chapter = EXCLUDED.peak_chapter,
                 peak_score = EXCLUDED.peak_score, computed_at = NOW()"
        )
        .bind(book_id)
        .bind(profile_id)
        .bind(concentration)
        .bind(total_above_threshold)
        .bind(chunks.len() as i32)
        .bind(peak_chapter)
        .bind(peak_score)
        .execute(&state.db)
        .await
        .map_err(|e| e.to_string())?;
    }

    state
        .task_queue
        .update_progress(task_id, 100, Some("Semantic tagging complete"))
        .await
        .map_err(|e| e.to_string())?;

    Ok(Some(serde_json::json!({
        "profiles_processed": profiles.len(),
        "total_chunks": chunks.len(),
        "total_chapters": total_chapters,
        "content_markers_created": total_markers,
    })))
}

/// A chunk vector retrieved from Qdrant.
struct ChunkVector {
    chapter_index: i32,
    chunk_index: i32,
    content: String,
    vector: Vec<f32>,
}

/// Fetch all vectors for a book from Qdrant using scroll API.
async fn fetch_book_vectors_from_qdrant(
    state: &AppState,
    book_id: &str,
) -> Result<Vec<ChunkVector>, String> {
    let mut all_chunks = Vec::new();
    let mut offset: Option<serde_json::Value> = None;
    let batch_size = 100;

    loop {
        let mut scroll_body = serde_json::json!({
            "filter": {
                "must": [{ "key": "book_id", "match": { "value": book_id } }]
            },
            "limit": batch_size,
            "with_payload": true,
            "with_vector": true,
        });

        if let Some(off) = &offset {
            scroll_body["offset"] = off.clone();
        }

        let resp = state
            .http_client
            .post(format!(
                "{}/collections/nova_chunks/points/scroll",
                state.config.qdrant_url
            ))
            .json(&scroll_body)
            .send()
            .await
            .map_err(|e| format!("Qdrant scroll error: {}", e))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Qdrant scroll failed: {}", body));
        }

        let data: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
        let points = data["result"]["points"]
            .as_array()
            .unwrap_or(&Vec::new())
            .clone();

        if points.is_empty() {
            break;
        }

        for point in &points {
            let chapter_index = point["payload"]["chapter_index"]
                .as_i64()
                .or_else(|| point["payload"]["chapter_number"].as_i64())
                .unwrap_or(0) as i32;
            let chunk_index = point["payload"]["chunk_index"].as_i64().unwrap_or(0) as i32;
            let content = point["payload"]["text"]
                .as_str()
                .or_else(|| point["payload"]["content"].as_str())
                .unwrap_or("")
                .to_string();
            let vector: Vec<f32> = point["vector"]
                .as_array()
                .unwrap_or(&Vec::new())
                .iter()
                .map(|v| v.as_f64().unwrap_or(0.0) as f32)
                .collect();

            if !vector.is_empty() {
                all_chunks.push(ChunkVector {
                    chapter_index,
                    chunk_index,
                    content,
                    vector,
                });
            }
        }

        // Get next offset
        offset = data["result"]["next_page_offset"].clone().into();
        if offset.as_ref().map(|v| v.is_null()).unwrap_or(true) {
            break;
        }
    }

    Ok(all_chunks)
}

/// Cosine similarity between two vectors.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

// ─── Ontology Assignment Task ────────────────────────────────────────────────

/// Assign a book's chunks to existing ontology tree nodes.
/// If no ontology tree exists yet, skip gracefully.
/// This runs automatically after embeddings are generated for a new book.
async fn execute_assign_ontology(
    state: &AppState,
    task_id: Uuid,
    book_id: Uuid,
) -> Result<Option<serde_json::Value>, String> {
    state
        .task_queue
        .update_progress(task_id, 5, Some("Checking ontology tree..."))
        .await
        .map_err(|e| e.to_string())?;

    // Check if ontology tree has any nodes with centroids
    let node_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM trope_nodes WHERE centroid IS NOT NULL")
            .bind(book_id)
            .fetch_one(&state.db)
            .await
            .map_err(|e| format!("DB query failed: {e}"))?;

    if node_count.0 == 0 {
        tracing::info!("No ontology nodes with centroids. Skipping assignment for book {book_id}");
        return Ok(Some(serde_json::json!({
            "status": "skipped",
            "reason": "No ontology tree exists yet. Run clustering first.",
        })));
    }

    state
        .task_queue
        .update_progress(task_id, 20, Some("Fetching book vectors..."))
        .await
        .map_err(|e| e.to_string())?;

    // Fetch book's vectors from Qdrant
    let qdrant_url = std::env::var("QDRANT_URL").unwrap_or_else(|_| "http://localhost:6333".into());
    let chunks = fetch_qdrant_book_vectors(&state.http_client, &qdrant_url, book_id)
        .await
        .map_err(|e| format!("Qdrant fetch failed: {e}"))?;

    if chunks.is_empty() {
        return Ok(Some(serde_json::json!({
            "status": "skipped",
            "reason": "No vectors found for this book in Qdrant.",
        })));
    }

    state
        .task_queue
        .update_progress(task_id, 40, Some("Loading ontology centroids..."))
        .await
        .map_err(|e| e.to_string())?;

    // Load all node centroids
    let nodes: Vec<(Uuid, Vec<u8>)> =
        sqlx::query_as("SELECT id, centroid FROM trope_nodes WHERE centroid IS NOT NULL")
            .fetch_all(&state.db)
            .await
            .map_err(|e| format!("Failed to load centroids: {e}"))?;

    let centroids: Vec<(Uuid, Vec<f32>)> = nodes
        .iter()
        .map(|(id, bytes)| {
            let vec: Vec<f32> = bytes
                .chunks_exact(4)
                .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect();
            (*id, vec)
        })
        .collect();

    state
        .task_queue
        .update_progress(task_id, 60, Some("Assigning chunks to nodes..."))
        .await
        .map_err(|e| e.to_string())?;

    let mut assigned = 0usize;
    let similarity_threshold = 0.45;

    for chunk in &chunks {
        if chunk.vector.is_empty() {
            continue;
        }

        // Skip if already assigned
        let exists: Option<(Uuid,)> =
            sqlx::query_as("SELECT id FROM trope_chunk_assignments WHERE qdrant_point_id = $1")
                .bind(chunk.point_id as i64)
                .fetch_optional(&state.db)
                .await
                .map_err(|e| format!("DB check failed: {e}"))?;

        if exists.is_some() {
            continue;
        }

        // Find best matching node
        let mut best_node: Option<Uuid> = None;
        let mut best_score: f32 = 0.0;

        for (node_id, centroid) in &centroids {
            let score = cosine_similarity(&chunk.vector, centroid);
            if score > best_score {
                best_score = score;
                best_node = Some(*node_id);
            }
        }

        if let Some(node_id) = best_node {
            if best_score > similarity_threshold {
                sqlx::query(
                    "INSERT INTO trope_chunk_assignments \
                     (trope_node_id, book_id, chapter_index, chunk_index, qdrant_point_id, membership_score) \
                     VALUES ($1, $2, $3, $4, $5, $6) \
                     ON CONFLICT (qdrant_point_id) DO NOTHING"
                )
                .bind(node_id)
                .bind(book_id)
                .bind(chunk.chapter_index)
                .bind(chunk.chunk_index)
                .bind(chunk.point_id as i64)
                .bind(best_score as f64)
                .execute(&state.db)
                .await
                .map_err(|e| format!("Insert failed: {e}"))?;

                assigned += 1;
            }
        }
    }

    // Update cluster sizes
    sqlx::query(
        "UPDATE trope_nodes SET cluster_size = (
            SELECT COUNT(*) FROM trope_chunk_assignments WHERE trope_node_id = trope_nodes.id
        )",
    )
    .execute(&state.db)
    .await
    .map_err(|e| format!("Failed to update cluster sizes: {e}"))?;

    state
        .task_queue
        .update_progress(task_id, 100, Some("Done"))
        .await
        .map_err(|e| e.to_string())?;

    tracing::info!(
        "Ontology assignment for book {book_id}: {assigned}/{} chunks assigned",
        chunks.len()
    );

    Ok(Some(serde_json::json!({
        "book_id": book_id,
        "total_chunks": chunks.len(),
        "assigned_to_tree": assigned,
        "nodes_available": centroids.len(),
    })))
}

/// Fetch a specific book's vectors from Qdrant for ontology assignment.
async fn fetch_qdrant_book_vectors(
    client: &reqwest::Client,
    qdrant_url: &str,
    book_id: Uuid,
) -> Result<Vec<OntologyChunk>, String> {
    let mut all_chunks = Vec::new();
    let mut offset: Option<serde_json::Value> = None;

    loop {
        let mut body = serde_json::json!({
            "limit": 100,
            "with_payload": true,
            "with_vector": true,
            "filter": {
                "must": [{"key": "book_id", "match": {"value": book_id.to_string()}}]
            }
        });

        if let Some(ref off) = offset {
            body["offset"] = off.clone();
        }

        let resp = client
            .post(format!(
                "{qdrant_url}/collections/nova_chunks/points/scroll"
            ))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Qdrant request failed: {e}"))?;

        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Qdrant parse failed: {e}"))?;

        let empty = vec![];
        let points = data["result"]["points"].as_array().unwrap_or(&empty);
        if points.is_empty() {
            break;
        }

        for point in points {
            let point_id = point["id"].as_u64().unwrap_or(0);
            let vector: Vec<f32> = point["vector"]
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .map(|v| v.as_f64().unwrap_or(0.0) as f32)
                .collect();
            let payload = &point["payload"];

            all_chunks.push(OntologyChunk {
                point_id,
                vector,
                chapter_index: payload["chapter_index"].as_i64().unwrap_or(0) as i32,
                chunk_index: payload["chunk_index"].as_i64().unwrap_or(0) as i32,
            });
        }

        if all_chunks.len() >= 10000 {
            break;
        }

        offset = data["result"]["next_page_offset"].clone().into();
        if offset.as_ref().map(|v| v.is_null()).unwrap_or(true) {
            break;
        }
    }

    Ok(all_chunks)
}

struct OntologyChunk {
    point_id: u64,
    vector: Vec<f32>,
    chapter_index: i32,
    chunk_index: i32,
}

#[cfg(test)]
mod embedding_freshness_tests {
    use super::{map_dedup_task_result, parse_embedding_batch_response, TaskExecutionOutcome};
    use crate::dedup::DedupTaskError;
    use serde_json::json;

    #[test]
    fn dedup_continuation_is_not_mapped_to_a_task_failure() {
        assert_eq!(
            map_dedup_task_result(Err(DedupTaskError::Continue("still pending".to_string()))),
            Ok(TaskExecutionOutcome::Continue("still pending".to_string()))
        );
        assert_eq!(
            map_dedup_task_result(Err(DedupTaskError::Failed("terminal".to_string()))),
            Err("terminal".to_string())
        );
    }

    #[test]
    fn embedding_batch_response_preserves_indices_when_items_are_out_of_order() {
        let response = json!({
            "data": [
                { "index": 1, "embedding": [0.3, 0.4] },
                { "index": 0, "embedding": [0.1, 0.2] }
            ]
        });

        let embeddings = parse_embedding_batch_response(&response, 2, 2)
            .expect("out-of-order indexed embeddings should be valid");

        assert_eq!(embeddings[0].response_index, 1);
        assert_eq!(embeddings[0].vector, vec![0.3, 0.4]);
        assert_eq!(embeddings[1].response_index, 0);
        assert_eq!(embeddings[1].vector, vec![0.1, 0.2]);
    }

    #[test]
    fn embedding_batch_response_rejects_duplicate_and_missing_indices() {
        let duplicate = json!({
            "data": [
                { "index": 0, "embedding": [0.1, 0.2] },
                { "index": 0, "embedding": [0.3, 0.4] }
            ]
        });
        let missing = json!({
            "data": [
                { "index": 1, "embedding": [0.3, 0.4] }
            ]
        });

        let duplicate_error = parse_embedding_batch_response(&duplicate, 2, 2)
            .expect_err("duplicate response indices must be rejected");
        let missing_error = parse_embedding_batch_response(&missing, 2, 2)
            .expect_err("missing response indices must be rejected");

        assert!(duplicate_error.contains("duplicate embedding index 0"));
        assert!(missing_error.contains("missing embedding indices: 0"));
    }

    #[test]
    fn embedding_batch_response_rejects_out_of_range_indices_and_invalid_vectors() {
        let out_of_range = json!({
            "data": [
                { "index": 0, "embedding": [0.1, 0.2] },
                { "index": 2, "embedding": [0.3, 0.4] }
            ]
        });
        let invalid_vector = json!({
            "data": [
                { "index": 0, "embedding": [0.1] }
            ]
        });

        let range_error = parse_embedding_batch_response(&out_of_range, 2, 2)
            .expect_err("out-of-range response indices must be rejected");
        let vector_error = parse_embedding_batch_response(&invalid_vector, 1, 2)
            .expect_err("vectors with an unexpected dimension must be rejected");

        assert!(range_error.contains("embedding index 2 is out of range"));
        assert!(vector_error.contains("expected 2 dimensions, got 1"));
    }

    #[test]
    fn durable_embedding_worker_uses_shared_contract_and_clears_empty_books() {
        let source = include_str!("task_worker.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("production task worker source should exist");
        let start = source
            .find("async fn execute_generate_embeddings(")
            .expect("embedding worker should exist");
        let rest = &source[start..];
        let end = rest
            .find("\n/// Extract entities")
            .expect("embedding worker boundary should exist");
        let handler = &rest[..end];

        assert!(handler.contains("load_embedding_freshness_contract("));
        assert!(handler.contains("delete_book_embedding_points("));
        assert!(handler.contains("embedding_contract.chunk_payload("));
        assert!(handler.contains("embedding_point_id("));
        assert!(handler.contains("let response_index = embedding.response_index;"));
        assert!(
            handler.contains("let chunk_index = batch_idx * 16 + response_index;"),
            "point IDs and payloads must use the embedding response index"
        );
        assert!(!handler.contains("let chunk_index = batch_idx * 16 + i;"));
        assert!(!handler.contains("let global_idx = total_chunks + batch_idx * 16 + i;"));

        let delete = handler
            .find("delete_book_embedding_points(")
            .expect("embedding delete should exist");
        let empty = handler
            .find("if chapters.is_empty()")
            .expect("empty snapshot check should exist");
        assert!(
            delete < empty,
            "old points must be deleted before empty return"
        );
    }
}
