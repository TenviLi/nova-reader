use std::time::Duration;

use serde_json::Value;
use uuid::Uuid;

use nova_core::domain::dedup::{DedupIndexCleanupTask, DuplicateReviewStatus};
use nova_core::domain::task::TaskKind;

use crate::dedup::{DedupTaskError, ResolveAction};
use crate::error::{ApiError, ApiResult};
use crate::repo::pg_duplicate_resolution::{
    ChapterMappingDirection, DuplicateResolutionPair, ResolutionAuthorizationScope,
};
use crate::state::AppState;
use crate::task_queue::TaskQueue;

const MEILI_TASK_POLL_POLICY: MeiliTaskPollPolicy = MeiliTaskPollPolicy {
    attempts: 100,
    interval: Duration::from_millis(100),
};

#[derive(Debug, Clone, Copy)]
struct MeiliTaskPollPolicy {
    attempts: usize,
    interval: Duration,
}

#[derive(Debug, PartialEq, Eq)]
enum MeiliTaskState {
    Pending,
    Succeeded,
    Failed(String),
}

pub(crate) async fn resolve_pair(
    state: &AppState,
    pair_id: Uuid,
    action: ResolveAction,
    user_id: Uuid,
) -> ApiResult<Value> {
    let authorization_scope = authorization_scope(action);
    let mut transaction = state.duplicates.begin_resolution().await?;
    let pair = transaction
        .lock_pair(pair_id, authorization_scope)
        .await?
        .ok_or_else(|| ApiError::not_found("duplicate pair"))?;
    if matches!(
        pair.review_status,
        DuplicateReviewStatus::Confirmed | DuplicateReviewStatus::Dismissed
    ) {
        return Err(ApiError::bad_request("duplicate pair is already resolved"));
    }

    transaction
        .authorize_locked_books(&pair, user_id, authorization_scope)
        .await?;

    if pair.stale {
        return Err(stale_pair_error());
    }
    if action.groups_versions()
        && !transaction
            .pair_evidence_matches_current_content(&pair)
            .await?
    {
        transaction.mark_pair_stale(pair_id).await?;
        transaction.commit().await?;
        return Err(stale_pair_error());
    }

    match action {
        ResolveAction::Dismiss => {
            transaction.mark_dismissed(pair_id, user_id).await?;
            transaction.commit().await?;
            return Ok(serde_json::json!({
                "pair_id": pair_id,
                "action": action.as_str(),
                "status": "dismissed",
            }));
        }
        ResolveAction::Defer => {
            transaction.mark_deferred(pair_id, user_id).await?;
            transaction.commit().await?;
            return Ok(serde_json::json!({
                "pair_id": pair_id,
                "action": action.as_str(),
                "status": "deferred",
            }));
        }
        ResolveAction::KeepA | ResolveAction::KeepB | ResolveAction::SameWork => {}
    }

    let primary_id = match action {
        ResolveAction::KeepA => pair.book_a_id,
        ResolveAction::KeepB => pair.book_b_id,
        ResolveAction::SameWork => preferred_primary(&pair),
        ResolveAction::Dismiss | ResolveAction::Defer => {
            return Err(ApiError::bad_request(
                "resolution action cannot merge versions",
            ));
        }
    };
    let work_id = transaction
        .ensure_shared_work(pair_id, &pair, primary_id, user_id)
        .await?;

    if matches!(action, ResolveAction::SameWork) {
        transaction
            .mark_same_work_confirmed(pair_id, primary_id, user_id)
            .await?;
        transaction.commit().await?;
        return Ok(serde_json::json!({
            "pair_id": pair_id,
            "action": action.as_str(),
            "status": "confirmed",
            "work_id": work_id,
            "primary_book_id": primary_id,
            "archived": false,
        }));
    }

    let secondary_id = if primary_id == pair.book_a_id {
        pair.book_b_id
    } else {
        pair.book_a_id
    };
    let mapping_direction = if primary_id == pair.book_a_id {
        ChapterMappingDirection::BookBToBookA
    } else {
        ChapterMappingDirection::BookAToBookB
    };
    let mapped_annotations = transaction
        .migrate_reader_artifacts(pair_id, primary_id, secondary_id, mapping_direction)
        .await?;
    let copied_links = transaction
        .copy_shared_book_links(primary_id, secondary_id)
        .await?;
    transaction
        .mark_secondary_duplicate(secondary_id, primary_id, pair_id)
        .await?;
    transaction
        .mark_kept_version_confirmed(pair_id, action.as_str(), primary_id, user_id)
        .await?;
    let cleanup_task_id = transaction
        .enqueue_index_cleanup(pair_id, secondary_id, primary_id)
        .await?;
    transaction.commit().await?;
    Ok(serde_json::json!({
        "pair_id": pair_id,
        "action": action.as_str(),
        "status": "confirmed",
        "work_id": work_id,
        "primary_book_id": primary_id,
        "secondary_book_id": secondary_id,
        "secondary_status": "duplicate",
        "source_file_deleted": false,
        "reader_artifacts_mapped": mapped_annotations,
        "library_links_copied": copied_links,
        "index_cleanup_task_id": cleanup_task_id,
    }))
}

fn stale_pair_error() -> ApiError {
    ApiError::bad_request("duplicate pair evidence is stale; rescan before merging versions")
}

const fn authorization_scope(action: ResolveAction) -> ResolutionAuthorizationScope {
    if action.groups_versions() {
        ResolutionAuthorizationScope::WorkMembers
    } else {
        ResolutionAuthorizationScope::PairBooks
    }
}

fn preferred_primary(pair: &DuplicateResolutionPair) -> Uuid {
    if pair.recommended_primary_id == Some(pair.book_a_id) {
        pair.book_a_id
    } else if pair.recommended_primary_id == Some(pair.book_b_id) {
        pair.book_b_id
    } else if pair.book_a_work_id.is_some() && pair.book_b_work_id.is_none() {
        pair.book_a_id
    } else if pair.book_b_work_id.is_some() && pair.book_a_work_id.is_none() {
        pair.book_b_id
    } else {
        pair.book_a_id
    }
}

pub(super) async fn execute_index_cleanup(
    state: &AppState,
    task_id: Uuid,
    payload: &DedupIndexCleanupTask,
) -> Result<Option<Value>, DedupTaskError> {
    let secondary_id = payload.secondary_book_id;
    let primary_id = payload.primary_book_id;
    let pair_id = payload.pair_id.ok_or_else(|| {
        DedupTaskError::Failed(
            "index cleanup task predates pair-scoped safety evidence".to_string(),
        )
    })?;
    let chapter_indexes = state
        .duplicates
        .verified_redundant_chapter_indexes(pair_id, secondary_id, primary_id)
        .await
        .map_err(|error| {
            DedupTaskError::Failed(format!("failed to verify redundant chapters: {error}"))
        })?;
    let issues = cleanup_secondary_indexes(
        state,
        task_id,
        payload,
        secondary_id,
        primary_id,
        &chapter_indexes,
    )
    .await;
    let mut failures = Vec::new();
    let mut continuations = Vec::new();
    for issue in issues {
        match issue {
            DedupTaskError::Failed(error) => failures.push(error),
            DedupTaskError::Continue(message) => continuations.push(message),
        }
    }
    if !failures.is_empty() {
        return Err(DedupTaskError::Failed(failures.join("; ")));
    }
    if !continuations.is_empty() {
        return Err(DedupTaskError::Continue(continuations.join("; ")));
    }
    Ok(Some(serde_json::json!({
        "secondary_book_id": secondary_id,
        "primary_book_id": primary_id,
        "redundant_chapter_indexes": chapter_indexes,
        "indexes_cleaned": ["qdrant", "meilisearch", "neo4j_version_link"],
    })))
}

async fn cleanup_secondary_indexes(
    state: &AppState,
    task_id: Uuid,
    payload: &DedupIndexCleanupTask,
    secondary_id: Uuid,
    primary_id: Uuid,
    chapter_indexes: &[i32],
) -> Vec<DedupTaskError> {
    let mut issues = Vec::new();
    if !chapter_indexes.is_empty() {
        let qdrant = state
            .http_client
            .post(format!(
                "{}/collections/nova_chunks/points/delete?wait=true",
                state.config.qdrant_url
            ))
            .json(&serde_json::json!({
                "filter": {
                    "must": [
                        { "key": "book_id", "match": { "value": secondary_id.to_string() } },
                        { "key": "chapter_index", "match": { "any": chapter_indexes } }
                    ]
                }
            }))
            .send()
            .await;
        match qdrant {
            Ok(response) if response.status().is_success() => {}
            Ok(response) => issues.push(DedupTaskError::Failed(format!(
                "Qdrant cleanup returned {}",
                response.status()
            ))),
            Err(error) => issues.push(DedupTaskError::Failed(format!(
                "Qdrant cleanup failed: {error}"
            ))),
        }

        let chapter_filter = chapter_indexes
            .iter()
            .map(i32::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        if let Err(issue) = delete_meilisearch_documents_and_wait(
            state,
            task_id,
            payload,
            format!(
                "book_id = '{}' AND chapter_index IN [{}]",
                secondary_id, chapter_filter
            ),
        )
        .await
        {
            issues.push(issue);
        }
    }

    if let Err(error) = state
        .neo4j
        .execute(
            "MATCH (n {book_id: $secondary}) SET n.duplicate_of = $primary",
            Some(serde_json::json!({
                "secondary": secondary_id.to_string(),
                "primary": primary_id.to_string(),
            })),
        )
        .await
    {
        issues.push(DedupTaskError::Failed(format!(
            "Neo4j version-link update failed: {error}"
        )));
    }
    issues
}

async fn delete_meilisearch_documents_and_wait(
    state: &AppState,
    task_id: Uuid,
    payload: &DedupIndexCleanupTask,
    filter: String,
) -> Result<(), DedupTaskError> {
    delete_meilisearch_documents_and_wait_with_policy(
        &state.http_client,
        &state.task_queue,
        task_id,
        payload,
        &state.config.meili_url,
        &state.config.meili_master_key,
        filter,
        MEILI_TASK_POLL_POLICY,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn delete_meilisearch_documents_and_wait_with_policy(
    client: &reqwest::Client,
    task_queue: &TaskQueue,
    task_id: Uuid,
    payload: &DedupIndexCleanupTask,
    meili_url: &str,
    meili_master_key: &str,
    filter: String,
    poll_policy: MeiliTaskPollPolicy,
) -> Result<(), DedupTaskError> {
    let base_url = meili_url.trim_end_matches('/');
    let mut persisted_payload = payload.clone();
    let task_uid = if let Some(task_uid) = persisted_payload.meilisearch_task_uid {
        task_uid
    } else {
        let response = client
            .post(format!("{base_url}/indexes/chunks/documents/delete"))
            .bearer_auth(meili_master_key)
            .json(&serde_json::json!({ "filter": filter }))
            .send()
            .await
            .map_err(|error| {
                DedupTaskError::Failed(format!("Meilisearch cleanup failed: {error}"))
            })?;
        if !response.status().is_success() {
            return Err(DedupTaskError::Failed(format!(
                "Meilisearch cleanup returned {}",
                response.status()
            )));
        }
        let enqueue = response.json::<Value>().await.map_err(|error| {
            DedupTaskError::Failed(format!("Meilisearch cleanup response was invalid: {error}"))
        })?;
        let task_uid = enqueue
            .get("taskUid")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                DedupTaskError::Failed("Meilisearch cleanup response omitted taskUid".to_string())
            })?;
        let mut resumable_payload = payload.clone();
        resumable_payload.meilisearch_task_uid = Some(task_uid);
        task_queue
            .persist_running_payload(task_id, TaskKind::Deduplicate, payload, &resumable_payload)
            .await
            .map_err(|error| {
                DedupTaskError::Failed(format!(
                    "failed to persist Meilisearch cleanup task {task_uid} before polling: {error}"
                ))
            })?;
        persisted_payload = resumable_payload;
        task_uid
    };

    for attempt in 0..poll_policy.attempts {
        let response = client
            .get(format!("{base_url}/tasks/{task_uid}"))
            .bearer_auth(meili_master_key)
            .send()
            .await
            .map_err(|error| {
                DedupTaskError::Failed(format!("Meilisearch cleanup task poll failed: {error}"))
            })?;
        if !response.status().is_success() {
            return Err(DedupTaskError::Failed(format!(
                "Meilisearch cleanup task poll returned {}",
                response.status()
            )));
        }
        let task = response.json::<Value>().await.map_err(|error| {
            DedupTaskError::Failed(format!(
                "Meilisearch cleanup task response was invalid: {error}"
            ))
        })?;
        match meili_task_state(&task).map_err(DedupTaskError::Failed)? {
            MeiliTaskState::Succeeded => return Ok(()),
            MeiliTaskState::Failed(error) => {
                let mut retryable_payload = persisted_payload.clone();
                retryable_payload.meilisearch_task_uid = None;
                task_queue
                    .persist_running_payload(
                        task_id,
                        TaskKind::Deduplicate,
                        &persisted_payload,
                        &retryable_payload,
                    )
                    .await
                    .map_err(|persist_error| {
                        DedupTaskError::Failed(format!(
                            "Meilisearch cleanup task failed: {error}; failed to reset terminal task {task_uid} for retry: {persist_error}"
                        ))
                    })?;
                return Err(DedupTaskError::Failed(format!(
                    "Meilisearch cleanup task failed: {error}"
                )));
            }
            MeiliTaskState::Pending if attempt + 1 < poll_policy.attempts => {
                tokio::time::sleep(poll_policy.interval).await;
            }
            MeiliTaskState::Pending => {}
        }
    }

    Err(DedupTaskError::Continue(format!(
        "Meilisearch cleanup task {task_uid} did not finish within the retry window"
    )))
}

fn meili_task_state(task: &Value) -> Result<MeiliTaskState, String> {
    match task.get("status").and_then(Value::as_str) {
        Some("enqueued" | "processing") => Ok(MeiliTaskState::Pending),
        Some("succeeded") => Ok(MeiliTaskState::Succeeded),
        Some("failed" | "canceled") => Ok(MeiliTaskState::Failed(
            task.pointer("/error/message")
                .and_then(Value::as_str)
                .unwrap_or("unknown Meilisearch task failure")
                .to_string(),
        )),
        Some(status) => Err(format!("unknown Meilisearch task status: {status}")),
        None => Err("Meilisearch task response omitted status".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        extract::{Path, State},
        routing::{get, post},
        Json, Router,
    };
    use sqlx::PgPool;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    #[test]
    fn wire_actions_are_stable_for_persisted_resolutions() {
        assert_eq!(ResolveAction::KeepA.as_str(), "keep_a");
        assert_eq!(ResolveAction::KeepB.as_str(), "keep_b");
        assert_eq!(ResolveAction::SameWork.as_str(), "same_work");
    }

    #[test]
    fn review_only_actions_authorize_pair_books_without_hidden_work_members() {
        assert_eq!(
            authorization_scope(ResolveAction::Dismiss),
            ResolutionAuthorizationScope::PairBooks
        );
        assert_eq!(
            authorization_scope(ResolveAction::Defer),
            ResolutionAuthorizationScope::PairBooks
        );
        for action in [
            ResolveAction::KeepA,
            ResolveAction::KeepB,
            ResolveAction::SameWork,
        ] {
            assert_eq!(
                authorization_scope(action),
                ResolutionAuthorizationScope::WorkMembers
            );
        }
    }

    #[test]
    fn meilisearch_cleanup_waits_for_success_and_surfaces_terminal_failure() {
        assert_eq!(
            meili_task_state(&serde_json::json!({ "status": "processing" })),
            Ok(MeiliTaskState::Pending)
        );
        assert_eq!(
            meili_task_state(&serde_json::json!({ "status": "succeeded" })),
            Ok(MeiliTaskState::Succeeded)
        );
        assert_eq!(
            meili_task_state(&serde_json::json!({
                "status": "failed",
                "error": { "message": "index write failed" }
            })),
            Ok(MeiliTaskState::Failed("index write failed".to_string()))
        );
        assert!(meili_task_state(&serde_json::json!({})).is_err());
    }

    #[derive(Clone, Default)]
    struct MockMeilisearch {
        enqueue_count: Arc<AtomicUsize>,
        poll_count: Arc<AtomicUsize>,
    }

    async fn enqueue_delete(State(mock): State<MockMeilisearch>) -> Json<Value> {
        mock.enqueue_count.fetch_add(1, Ordering::SeqCst);
        Json(serde_json::json!({ "taskUid": 42 }))
    }

    async fn poll_task(
        State(mock): State<MockMeilisearch>,
        Path(task_uid): Path<u64>,
    ) -> Json<Value> {
        let poll_index = mock.poll_count.fetch_add(1, Ordering::SeqCst);
        if task_uid == 99 {
            Json(serde_json::json!({
                "status": "failed",
                "error": { "message": "index write failed" }
            }))
        } else if poll_index == 0 {
            Json(serde_json::json!({ "status": "processing" }))
        } else {
            Json(serde_json::json!({ "status": "succeeded" }))
        }
    }

    #[sqlx::test(migrations = false)]
    #[ignore = "requires a PostgreSQL server with permission to create isolated test databases"]
    async fn meilisearch_cleanup_resumes_pending_and_reenqueues_after_terminal_failure(
        pool: PgPool,
    ) -> sqlx::Result<()> {
        sqlx::raw_sql(
            r#"CREATE TYPE task_kind AS ENUM ('deduplicate');
               CREATE TYPE task_status AS ENUM ('running');
               CREATE TABLE tasks (
                   id UUID PRIMARY KEY,
                   kind task_kind NOT NULL,
                   status task_status NOT NULL,
                   payload JSONB NOT NULL
               );"#,
        )
        .execute(&pool)
        .await?;

        let task_id = Uuid::now_v7();
        let cleanup =
            DedupIndexCleanupTask::new(Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3));
        let initial_payload = serde_json::to_value(&cleanup)
            .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
        sqlx::query(
            r#"INSERT INTO tasks (id, kind, status, payload)
               VALUES ($1, 'deduplicate', 'running', $2)"#,
        )
        .bind(task_id)
        .bind(initial_payload)
        .execute(&pool)
        .await?;

        let mock = MockMeilisearch::default();
        let app = Router::new()
            .route("/indexes/chunks/documents/delete", post(enqueue_delete))
            .route("/tasks/{task_uid}", get(poll_task))
            .with_state(mock.clone());
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .map_err(sqlx::Error::Io)?;
        let base_url = format!("http://{}", listener.local_addr().map_err(sqlx::Error::Io)?);
        let server = tokio::spawn(async move { axum::serve(listener, app).await });
        let queue = crate::task_queue::TaskQueue::new(pool.clone());
        let client = reqwest::Client::new();
        let one_poll = MeiliTaskPollPolicy {
            attempts: 1,
            interval: Duration::ZERO,
        };

        let first_attempt = delete_meilisearch_documents_and_wait_with_policy(
            &client,
            &queue,
            task_id,
            &cleanup,
            &base_url,
            "test-key",
            "book_id = 'book'".to_string(),
            one_poll,
        )
        .await;
        let pending = first_attempt.expect_err("one pending poll must yield a continuation");
        assert!(matches!(
            pending,
            DedupTaskError::Continue(message)
                if message.contains("did not finish within the retry window")
        ));

        let persisted: Value = sqlx::query_scalar("SELECT payload FROM tasks WHERE id = $1")
            .bind(task_id)
            .fetch_one(&pool)
            .await?;
        let resumed: DedupIndexCleanupTask = serde_json::from_value(persisted)
            .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
        assert_eq!(resumed.meilisearch_task_uid, Some(42));
        assert_eq!(mock.enqueue_count.load(Ordering::SeqCst), 1);
        assert_eq!(mock.poll_count.load(Ordering::SeqCst), 1);

        delete_meilisearch_documents_and_wait_with_policy(
            &client,
            &queue,
            task_id,
            &resumed,
            &base_url,
            "test-key",
            "book_id = 'book'".to_string(),
            one_poll,
        )
        .await
        .expect("retry resumes and observes success");
        assert_eq!(mock.enqueue_count.load(Ordering::SeqCst), 1);
        assert_eq!(mock.poll_count.load(Ordering::SeqCst), 2);

        let terminal_failure = DedupIndexCleanupTask {
            meilisearch_task_uid: Some(99),
            ..resumed
        };
        let terminal_payload = serde_json::to_value(&terminal_failure)
            .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
        sqlx::query("UPDATE tasks SET payload = $2 WHERE id = $1")
            .bind(task_id)
            .bind(terminal_payload)
            .execute(&pool)
            .await?;
        let error = delete_meilisearch_documents_and_wait_with_policy(
            &client,
            &queue,
            task_id,
            &terminal_failure,
            &base_url,
            "test-key",
            "book_id = 'book'".to_string(),
            one_poll,
        )
        .await
        .expect_err("terminal Meilisearch failure must fail durable cleanup");
        assert!(matches!(
            error,
            DedupTaskError::Failed(message) if message.contains("index write failed")
        ));
        assert_eq!(mock.enqueue_count.load(Ordering::SeqCst), 1);

        let retry_payload: Value = sqlx::query_scalar("SELECT payload FROM tasks WHERE id = $1")
            .bind(task_id)
            .fetch_one(&pool)
            .await?;
        let retry_cleanup: DedupIndexCleanupTask = serde_json::from_value(retry_payload)
            .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
        assert_eq!(retry_cleanup.meilisearch_task_uid, None);
        delete_meilisearch_documents_and_wait_with_policy(
            &client,
            &queue,
            task_id,
            &retry_cleanup,
            &base_url,
            "test-key",
            "book_id = 'book'".to_string(),
            one_poll,
        )
        .await
        .expect("retry after terminal failure must enqueue a fresh delete");
        assert_eq!(mock.enqueue_count.load(Ordering::SeqCst), 2);

        server.abort();
        Ok(())
    }
}
