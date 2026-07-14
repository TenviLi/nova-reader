//! Task queue with DAG dependency scheduling.
//!
//! Tasks are stored in PostgreSQL. Each task can depend on other tasks.
//! A task becomes eligible to run only when all its dependencies are completed.
//! The queue polls for runnable tasks and dispatches them.

use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use nova_core::domain::task::{QueueStats, TaskDag, TaskKind, TaskPriority};

pub(crate) const INTERRUPTED_TASK_RECOVERY_MESSAGE: &str = "Recovered after worker restart";

/// Manages the task queue and DAG scheduling.
#[derive(Clone)]
pub struct TaskQueue {
    db: PgPool,
}

/// A task row as stored in PostgreSQL.
#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct TaskRow {
    pub id: Uuid,
    pub kind: String,
    pub status: String,
    pub priority: String,
    pub payload: serde_json::Value,
    pub result: Option<serde_json::Value>,
    pub error_message: Option<String>,
    pub retry_count: i32,
    pub max_retries: i32,
    pub book_id: Option<Uuid>,
    pub category: String,
    pub progress: i16,
    pub progress_message: Option<String>,
    pub scheduled_at: chrono::DateTime<Utc>,
    pub started_at: Option<chrono::DateTime<Utc>>,
    pub completed_at: Option<chrono::DateTime<Utc>>,
    pub created_at: chrono::DateTime<Utc>,
}

impl TaskQueue {
    pub fn new(db: PgPool) -> Self {
        Self { db }
    }

    /// Release task claims left behind by an interrupted embedded worker.
    ///
    /// Recovery is deliberately global: every handler is required to be
    /// idempotent, so no task kind may remain permanently stranded in
    /// `running` or legacy `retrying` after a process restart. Domain-specific
    /// projections reconcile from the durable recovery marker before polling
    /// begins.
    pub async fn recover_interrupted_tasks(&self) -> Result<u64, sqlx::Error> {
        let task_ids: Vec<Uuid> = sqlx::query_scalar(
            r#"UPDATE tasks
               SET status = 'queued'::task_status,
                   started_at = NULL,
                   completed_at = NULL,
                   scheduled_at = CASE
                       WHEN status = 'running'::task_status THEN NOW()
                       ELSE scheduled_at
                   END,
                   error_message = $1
               WHERE status IN ('running'::task_status, 'retrying'::task_status)
               RETURNING id"#,
        )
        .bind(INTERRUPTED_TASK_RECOVERY_MESSAGE)
        .fetch_all(&self.db)
        .await?;
        u64::try_from(task_ids.len()).map_err(|_| {
            sqlx::Error::Protocol("recovered task count exceeds the supported range".to_string())
        })
    }

    /// Submit a single task. Returns the task ID.
    pub async fn submit(
        &self,
        kind: TaskKind,
        book_id: Option<Uuid>,
        payload: serde_json::Value,
    ) -> Result<Uuid, sqlx::Error> {
        let id = Uuid::new_v4();
        let category = kind.category();
        sqlx::query(
            "INSERT INTO tasks (id, kind, status, priority, payload, book_id, category, max_retries, scheduled_at)
             VALUES ($1, $2::task_kind, 'queued'::task_status, '1'::task_priority, $3, $4, $5, 3, NOW())"
        )
        .bind(id)
        .bind(kind.as_str())
        .bind(&payload)
        .bind(book_id)
        .bind(category.as_str())
        .execute(&self.db)
        .await?;
        Ok(id)
    }

    /// Submit a full DAG of tasks for a book. Returns the created task IDs.
    pub async fn submit_dag(&self, dag: &TaskDag) -> Result<Vec<Uuid>, sqlx::Error> {
        self.submit_dag_with_scope(None, dag).await
    }

    /// Submit task-derived DAG work exactly once for a durable parent scope.
    /// Replaying the parent returns the original child IDs instead of emitting
    /// duplicate work.
    pub async fn submit_dag_once(
        &self,
        submission_id: Uuid,
        dag: &TaskDag,
    ) -> Result<Vec<Uuid>, sqlx::Error> {
        self.submit_dag_with_scope(Some(submission_id), dag).await
    }

    async fn submit_dag_with_scope(
        &self,
        submission_id: Option<Uuid>,
        dag: &TaskDag,
    ) -> Result<Vec<Uuid>, sqlx::Error> {
        let mut tx = self.db.begin().await?;
        let mut task_ids: Vec<(TaskKind, Uuid)> = Vec::new();

        // First pass: create all task rows
        for node in &dag.tasks {
            let id = Uuid::new_v4();
            let kind_str = node.kind.as_str();
            let category_str = node.kind.category().as_str();
            let priority_str = priority_to_db_str(node.priority);
            let idempotency_key = submission_id
                .map(|submission_id| format!("{submission_id}:{}:{kind_str}", dag.book_id));

            let persisted_id: Uuid = sqlx::query_scalar(
                r#"INSERT INTO tasks
                   (id, kind, status, priority, payload, book_id, category,
                    max_retries, scheduled_at, idempotency_key)
                   VALUES ($1, $2::task_kind, 'queued'::task_status,
                           $3::task_priority, $4, $5, $6, 3, NOW(), $7)
                   ON CONFLICT (idempotency_key)
                       WHERE idempotency_key IS NOT NULL
                   DO UPDATE SET idempotency_key = EXCLUDED.idempotency_key
                   RETURNING id"#,
            )
            .bind(id)
            .bind(kind_str)
            .bind(priority_str)
            .bind(&node.payload)
            .bind(dag.book_id)
            .bind(category_str)
            .bind(idempotency_key)
            .fetch_one(&mut *tx)
            .await?;

            task_ids.push((node.kind, persisted_id));
        }

        // Second pass: create dependency edges
        for node in &dag.tasks {
            let task_id = task_ids
                .iter()
                .find(|(kind, _)| *kind == node.kind)
                .map(|(_, id)| *id)
                .ok_or_else(|| {
                    sqlx::Error::Protocol(format!("submitted task DAG is missing {:?}", node.kind))
                })?;
            for dep_kind in &node.depends_on {
                if let Some((_, dep_id)) = task_ids.iter().find(|(k, _)| k == dep_kind) {
                    sqlx::query(
                        "INSERT INTO task_dependencies (task_id, depends_on) VALUES ($1, $2)
                         ON CONFLICT DO NOTHING",
                    )
                    .bind(task_id)
                    .bind(dep_id)
                    .execute(&mut *tx)
                    .await?;
                }
            }
        }

        tx.commit().await?;
        Ok(task_ids.into_iter().map(|(_, id)| id).collect())
    }

    /// Atomically claim the next runnable task: queued, all dependencies
    /// completed, highest priority first.
    pub async fn poll_next(&self) -> Result<Option<TaskRow>, sqlx::Error> {
        // A task is runnable if:
        // 1. status = 'queued'
        // 2. scheduled_at <= NOW()
        // 3. All tasks it depends on have status = 'completed'
        // 4. No older active task holds a conflicting declared resource
        let row = sqlx::query_as!(
            TaskRow,
            r#"WITH candidate AS (
                 SELECT t.id
                 FROM tasks t
                 WHERE t.status = 'queued'
                   AND t.scheduled_at <= NOW()
                   AND NOT EXISTS (
                       SELECT 1 FROM task_dependencies td
                       JOIN tasks dep ON dep.id = td.depends_on
                       WHERE td.task_id = t.id AND dep.status != 'completed'
                   )
                   AND NOT EXISTS (
                       SELECT 1
                       FROM task_execution_locks requested_lock
                       JOIN task_execution_locks blocking_lock
                         ON blocking_lock.resource_key = requested_lock.resource_key
                        AND blocking_lock.task_id != requested_lock.task_id
                        AND (
                          requested_lock.mode = 'exclusive'
                          OR blocking_lock.mode = 'exclusive'
                        )
                       JOIN tasks blocking_task ON blocking_task.id = blocking_lock.task_id
                       WHERE requested_lock.task_id = t.id
                         AND blocking_task.status IN (
                           'queued'::task_status,
                           'running'::task_status,
                           'retrying'::task_status
                         )
                         AND (
                           blocking_task.created_at < t.created_at
                           OR (
                             blocking_task.created_at = t.created_at
                             AND blocking_task.id < t.id
                           )
                         )
                   )
                 ORDER BY t.priority DESC, t.scheduled_at ASC
                 LIMIT 1
                 FOR UPDATE OF t SKIP LOCKED
             )
             UPDATE tasks claimed
             SET status = 'running'::task_status,
                 started_at = COALESCE(claimed.started_at, NOW()),
                 error_message = NULL
             FROM candidate
             WHERE claimed.id = candidate.id
             RETURNING claimed.id, claimed.kind::text AS "kind!",
                       claimed.status::text AS "status!",
                       claimed.priority::text AS "priority!", claimed.payload,
                       claimed.result, claimed.error_message, claimed.retry_count,
                       claimed.max_retries, claimed.book_id,
                       claimed.category AS "category!",
                       claimed.progress, claimed.progress_message,
                       claimed.scheduled_at, claimed.started_at,
                       claimed.completed_at, claimed.created_at"#,
        )
        .fetch_optional(&self.db)
        .await?;

        Ok(row)
    }

    /// Update task progress (0-100).
    pub async fn update_progress(
        &self,
        task_id: Uuid,
        progress: i16,
        message: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE tasks SET progress = $2, progress_message = $3 WHERE id = $1")
            .bind(task_id)
            .bind(progress)
            .bind(message)
            .execute(&self.db)
            .await?;
        Ok(())
    }

    /// Persist resumable external-work state on the currently claimed task.
    /// The payload comparison is an optimistic claim token: once another
    /// worker advances the payload, a stale writer can no longer overwrite it.
    pub async fn persist_running_payload(
        &self,
        task_id: Uuid,
        expected_kind: TaskKind,
        expected_payload: &impl serde::Serialize,
        updated_payload: &impl serde::Serialize,
    ) -> Result<(), sqlx::Error> {
        let expected_payload = serde_json::to_value(expected_payload)
            .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
        let updated_payload = serde_json::to_value(updated_payload)
            .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
        let result = sqlx::query(
            r#"UPDATE tasks
               SET payload = $2
               WHERE id = $1
                 AND kind = $3::task_kind
                 AND status = 'running'::task_status
                 AND payload = $4"#,
        )
        .bind(task_id)
        .bind(updated_payload)
        .bind(expected_kind.as_str())
        .bind(expected_payload)
        .execute(&self.db)
        .await?;
        if result.rows_affected() == 1 {
            Ok(())
        } else {
            Err(sqlx::Error::RowNotFound)
        }
    }

    /// Yield an in-progress external operation without spending its failure
    /// budget. The same durable task becomes runnable again after a short
    /// delay and retains its resumable payload.
    pub async fn reschedule_running_continuation(
        &self,
        task_id: Uuid,
        message: &str,
        delay_seconds: i64,
    ) -> Result<(), sqlx::Error> {
        if delay_seconds < 0 {
            return Err(sqlx::Error::Protocol(
                "task continuation delay cannot be negative".to_string(),
            ));
        }
        let result = sqlx::query(
            r#"UPDATE tasks
               SET status = 'queued'::task_status,
                   started_at = NULL,
                   scheduled_at = NOW() + ($3::bigint * INTERVAL '1 second'),
                   progress_message = $2,
                   error_message = NULL
               WHERE id = $1
                 AND status = 'running'::task_status"#,
        )
        .bind(task_id)
        .bind(message)
        .bind(delay_seconds)
        .execute(&self.db)
        .await?;
        if result.rows_affected() == 1 {
            Ok(())
        } else {
            Err(sqlx::Error::RowNotFound)
        }
    }

    /// Mark a task as completed with optional result data.
    pub async fn mark_completed(
        &self,
        task_id: Uuid,
        result: Option<serde_json::Value>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE tasks SET status = 'completed'::task_status, completed_at = NOW(), progress = 100,
                              result = $2, error_message = NULL
             WHERE id = $1"
        )
        .bind(task_id)
        .bind(result)
        .execute(&self.db)
        .await?;
        Ok(())
    }

    /// Mark a task as failed. Will retry if under max_retries.
    pub async fn mark_failed(&self, task_id: Uuid, error: &str) -> Result<(), sqlx::Error> {
        // Check if we should retry
        let row = sqlx::query_as::<_, (i32, i32)>(
            "SELECT retry_count, max_retries FROM tasks WHERE id = $1",
        )
        .bind(task_id)
        .fetch_optional(&self.db)
        .await?;

        if let Some((retry_count, max_retries)) = row {
            if retry_count < max_retries {
                // Schedule retry with exponential backoff
                let delay_secs = 30 * (2_i64.pow(retry_count as u32));
                sqlx::query(
                    "UPDATE tasks SET status = 'queued'::task_status, retry_count = retry_count + 1,
                     error_message = $2, started_at = NULL,
                     scheduled_at = NOW() + ($3::bigint * INTERVAL '1 second')
                     WHERE id = $1"
                )
                .bind(task_id)
                .bind(error)
                .bind(delay_secs)
                .execute(&self.db)
                .await?;
            } else {
                // Move to dead letter
                sqlx::query(
                    "UPDATE tasks SET status = 'dead_letter'::task_status, error_message = $2, completed_at = NOW()
                     WHERE id = $1"
                )
                .bind(task_id)
                .bind(error)
                .execute(&self.db)
                .await?;
            }
        }
        Ok(())
    }

    /// Cancel a task (and all dependent tasks that can't proceed).
    pub async fn cancel(&self, task_id: Uuid) -> Result<i64, sqlx::Error> {
        // Cancel the task itself
        sqlx::query(
            "UPDATE tasks SET status = 'cancelled'::task_status, completed_at = NOW()
             WHERE id = $1 AND status IN ('queued', 'retrying')",
        )
        .bind(task_id)
        .execute(&self.db)
        .await?;

        // Cancel all downstream tasks that depend on this one
        let result = sqlx::query(
            "UPDATE tasks SET status = 'cancelled'::task_status, completed_at = NOW(),
                    error_message = 'Dependency cancelled'
             WHERE id IN (
                 SELECT td.task_id FROM task_dependencies td
                 WHERE td.depends_on = $1
             ) AND status IN ('queued', 'retrying')",
        )
        .bind(task_id)
        .execute(&self.db)
        .await?;

        Ok(result.rows_affected() as i64 + 1)
    }

    /// Get queue stats for dashboard.
    pub async fn stats(&self) -> Result<QueueStats, sqlx::Error> {
        let row = sqlx::query_as::<_, (i64, i64, i64, i64, i64, f64)>(
            "SELECT
                COUNT(*) FILTER (WHERE status = 'queued') as queued,
                COUNT(*) FILTER (WHERE status = 'running') as running,
                COUNT(*) FILTER (WHERE status = 'completed' AND completed_at > NOW() - INTERVAL '24 hours') as completed_today,
                COUNT(*) FILTER (WHERE status IN ('failed', 'dead_letter') AND completed_at > NOW() - INTERVAL '24 hours') as failed_today,
                COUNT(*) FILTER (WHERE status = 'dead_letter') as dead_letter_count,
                COALESCE(AVG(EXTRACT(EPOCH FROM (completed_at - started_at)) * 1000)
                    FILTER (WHERE status = 'completed' AND completed_at > NOW() - INTERVAL '24 hours'), 0)::float8 as avg_ms
             FROM tasks"
        )
        .fetch_one(&self.db)
        .await?;

        Ok(QueueStats {
            queued: row.0,
            running: row.1,
            completed_today: row.2,
            failed_today: row.3,
            dead_letter_count: row.4,
            avg_processing_time_ms: row.5,
        })
    }

    /// List tasks with optional filters.
    pub async fn list(
        &self,
        status: Option<&str>,
        book_id: Option<Uuid>,
        category: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<TaskRow>, i64), sqlx::Error> {
        // Whitelist filter values to prevent SQL injection
        let valid_statuses = [
            "queued",
            "running",
            "completed",
            "failed",
            "retrying",
            "cancelled",
            "dead_letter",
        ];
        let valid_categories = ["import", "preprocess", "ai", "index", "maintenance"];

        let status_filter = status.filter(|s| valid_statuses.contains(s));
        let category_filter = category.filter(|c| valid_categories.contains(c));

        // Build WHERE clause with positional params
        let mut conditions = vec!["1=1".to_string()];
        let mut param_idx = 1;

        if status_filter.is_some() {
            conditions.push(format!("t.status = ${}::task_status", param_idx));
            param_idx += 1;
        }
        if book_id.is_some() {
            conditions.push(format!("t.book_id = ${}", param_idx));
            param_idx += 1;
        }
        if category_filter.is_some() {
            conditions.push(format!("t.category = ${}", param_idx));
        }
        let where_clause = conditions.join(" AND ");

        // Count query
        let count_sql = format!("SELECT COUNT(*) FROM tasks t WHERE {}", where_clause);
        let mut count_q = sqlx::query_as::<_, (i64,)>(&count_sql);
        if let Some(s) = status_filter {
            count_q = count_q.bind(s);
        }
        if let Some(bid) = book_id {
            count_q = count_q.bind(bid);
        }
        if let Some(c) = category_filter {
            count_q = count_q.bind(c);
        }
        let total = count_q.fetch_one(&self.db).await?.0;

        // Data query
        let data_sql = format!(
            "SELECT t.id, t.kind::text, t.status::text, t.priority::text,
                    t.payload, t.result, t.error_message, t.retry_count, t.max_retries,
                    t.book_id, t.category, t.progress, t.progress_message,
                    t.scheduled_at, t.started_at, t.completed_at, t.created_at
             FROM tasks t
             WHERE {}
             ORDER BY t.created_at DESC
             LIMIT {} OFFSET {}",
            where_clause, limit, offset
        );
        let mut data_q = sqlx::query_as::<_, TaskRow>(&data_sql);
        if let Some(s) = status_filter {
            data_q = data_q.bind(s);
        }
        if let Some(bid) = book_id {
            data_q = data_q.bind(bid);
        }
        if let Some(c) = category_filter {
            data_q = data_q.bind(c);
        }
        let rows = data_q.fetch_all(&self.db).await?;

        Ok((rows, total))
    }

    /// Get dependencies for a task.
    pub async fn get_dependencies(&self, task_id: Uuid) -> Result<Vec<Uuid>, sqlx::Error> {
        let rows: Vec<(Uuid,)> =
            sqlx::query_as("SELECT depends_on FROM task_dependencies WHERE task_id = $1")
                .bind(task_id)
                .fetch_all(&self.db)
                .await?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }
}

fn priority_to_db_str(priority: TaskPriority) -> &'static str {
    match priority {
        TaskPriority::Low => "0",
        TaskPriority::Normal => "1",
        TaskPriority::High => "2",
        TaskPriority::Critical => "3",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::pg_duplicate::PgDuplicateRepository;
    use nova_core::domain::dedup::DedupIndexCleanupTask;

    #[sqlx::test(migrations = false)]
    #[ignore = "requires a PostgreSQL server with permission to create isolated test databases"]
    async fn persists_resumable_cleanup_payload_only_on_the_claimed_task(
        pool: PgPool,
    ) -> sqlx::Result<()> {
        sqlx::raw_sql(
            r#"CREATE TYPE task_kind AS ENUM ('deduplicate');
               CREATE TYPE task_status AS ENUM ('queued', 'running', 'completed');
               CREATE TABLE tasks (
                   id UUID PRIMARY KEY,
                   kind task_kind NOT NULL,
                   status task_status NOT NULL,
                   payload JSONB NOT NULL,
                   retry_count INTEGER NOT NULL DEFAULT 0,
                   scheduled_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                   started_at TIMESTAMPTZ,
                   progress_message TEXT,
                   error_message TEXT
               );"#,
        )
        .execute(&pool)
        .await?;
        let task_id = Uuid::now_v7();
        let other_task_id = Uuid::now_v7();
        let mut payload =
            DedupIndexCleanupTask::new(Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3));
        let legacy_payload = serde_json::to_value(&payload)
            .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
        sqlx::query(
            r#"INSERT INTO tasks (id, kind, status, payload)
               VALUES ($1, 'deduplicate', 'running', $3),
                      ($2, 'deduplicate', 'running', $3)"#,
        )
        .bind(task_id)
        .bind(other_task_id)
        .bind(&legacy_payload)
        .execute(&pool)
        .await?;

        payload.meilisearch_task_uid = Some(42);
        TaskQueue::new(pool.clone())
            .persist_running_payload(task_id, TaskKind::Deduplicate, &legacy_payload, &payload)
            .await?;
        let mut stale_update = payload.clone();
        stale_update.meilisearch_task_uid = Some(43);
        let stale_error = TaskQueue::new(pool.clone())
            .persist_running_payload(
                task_id,
                TaskKind::Deduplicate,
                &legacy_payload,
                &stale_update,
            )
            .await
            .expect_err("a stale payload must not overwrite the advanced task");
        assert!(matches!(stale_error, sqlx::Error::RowNotFound));

        let persisted: Vec<(Uuid, serde_json::Value)> =
            sqlx::query_as("SELECT id, payload FROM tasks ORDER BY id")
                .fetch_all(&pool)
                .await?;
        let payload_for = |id| {
            persisted
                .iter()
                .find(|row| row.0 == id)
                .map(|row| &row.1)
                .expect("seeded task remains present")
        };
        assert_eq!(payload_for(task_id)["meilisearch_task_uid"], 42);
        assert!(payload_for(other_task_id)
            .get("meilisearch_task_uid")
            .is_none());

        TaskQueue::new(pool.clone())
            .reschedule_running_continuation(task_id, "external task is still pending", 5)
            .await?;
        let continuation_state: (String, i32, Option<String>, bool) = sqlx::query_as(
            r#"SELECT status::text, retry_count, progress_message,
                      scheduled_at > NOW()
               FROM tasks WHERE id = $1"#,
        )
        .bind(task_id)
        .fetch_one(&pool)
        .await?;
        assert_eq!(
            continuation_state,
            (
                "queued".to_string(),
                0,
                Some("external task is still pending".to_string()),
                true,
            ),
            "continuation must preserve the failure budget and schedule the same task"
        );
        Ok(())
    }

    #[sqlx::test(migrations = false)]
    #[ignore = "requires a PostgreSQL server with permission to create isolated test databases"]
    async fn startup_recovery_requeues_every_interrupted_task_and_synchronizes_scan(
        pool: PgPool,
    ) -> sqlx::Result<()> {
        create_recovery_test_schema(&pool).await?;
        let running_task_id = Uuid::now_v7();
        let retrying_task_id = Uuid::now_v7();
        let scan_task_id = Uuid::now_v7();
        let completed_task_id = Uuid::now_v7();
        let scan_id = Uuid::now_v7();

        sqlx::query(
            r#"INSERT INTO tasks
               (id, kind, status, priority, payload, category, scheduled_at,
                started_at, completed_at)
               VALUES
               ($1, 'deep_analysis', 'running', '1', '{}'::jsonb, 'ai',
                NOW() + INTERVAL '1 day', NOW(), NULL),
               ($2, 'semantic_tagging', 'retrying', '1', '{}'::jsonb, 'ai',
                NOW() + INTERVAL '1 day', NULL, NULL),
               ($3, 'deduplicate', 'running', '1',
                jsonb_build_object('operation', 'scan', 'scan_run_id', $5),
                'preprocess', NOW() + INTERVAL '1 day', NOW(), NULL),
               ($4, 'clean_content', 'completed', '1', '{}'::jsonb, 'preprocess',
                NOW(), NOW() - INTERVAL '1 minute', NOW())"#,
        )
        .bind(running_task_id)
        .bind(retrying_task_id)
        .bind(scan_task_id)
        .bind(completed_task_id)
        .bind(scan_id)
        .execute(&pool)
        .await?;

        sqlx::query(
            r#"INSERT INTO dedup_scan_runs
               (id, task_id, include_semantic, algorithm_version, status,
                progress, progress_message, error_message, started_at)
               VALUES ($1, $2, FALSE, 3, 'running', 42, 'verifying',
                       'stale failure', NOW())"#,
        )
        .bind(scan_id)
        .bind(scan_task_id)
        .execute(&pool)
        .await?;

        let retry_scheduled_before: chrono::DateTime<Utc> =
            sqlx::query_scalar("SELECT scheduled_at FROM tasks WHERE id = $1")
                .bind(retrying_task_id)
                .fetch_one(&pool)
                .await?;

        let recovered = TaskQueue::new(pool.clone())
            .recover_interrupted_tasks()
            .await?;
        assert_eq!(recovered, 3);
        let synchronized_scans = PgDuplicateRepository::new(pool.clone())
            .synchronize_recovered_scan_tasks()
            .await
            .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
        assert_eq!(synchronized_scans, 1);

        let task_states: Vec<(
            Uuid,
            String,
            Option<chrono::DateTime<Utc>>,
            chrono::DateTime<Utc>,
            Option<chrono::DateTime<Utc>>,
        )> = sqlx::query_as(
            r#"SELECT id, status::text, started_at, scheduled_at, completed_at
               FROM tasks
               WHERE id = ANY($1)
               ORDER BY id"#,
        )
        .bind(vec![
            running_task_id,
            retrying_task_id,
            scan_task_id,
            completed_task_id,
        ])
        .fetch_all(&pool)
        .await?;

        let state_for = |task_id| {
            task_states
                .iter()
                .find(|state| state.0 == task_id)
                .expect("seeded task state must exist")
        };
        for task_id in [running_task_id, retrying_task_id, scan_task_id] {
            let state = state_for(task_id);
            assert_eq!(state.1, "queued");
            assert!(state.2.is_none(), "recovered task must release its claim");
            assert!(state.4.is_none(), "recovered task must remain incomplete");
        }
        let recovered_running_tasks_are_due: bool = sqlx::query_scalar(
            "SELECT bool_and(scheduled_at <= NOW()) FROM tasks WHERE id = ANY($1)",
        )
        .bind(vec![running_task_id, scan_task_id])
        .fetch_one(&pool)
        .await?;
        assert!(recovered_running_tasks_are_due);
        assert_eq!(state_for(retrying_task_id).3, retry_scheduled_before);
        assert_eq!(state_for(completed_task_id).1, "completed");

        let scan_state: (
            String,
            Option<String>,
            Option<String>,
            Option<chrono::DateTime<Utc>>,
        ) = sqlx::query_as(
            r#"SELECT status, progress_message, error_message, completed_at
               FROM dedup_scan_runs WHERE id = $1"#,
        )
        .bind(scan_id)
        .fetch_one(&pool)
        .await?;
        assert_eq!(scan_state.0, "queued");
        assert_eq!(scan_state.1.as_deref(), Some("recovering"));
        assert!(
            scan_state.2.is_none(),
            "recovery must clear stale scan errors"
        );
        assert!(scan_state.3.is_none());

        let reindex_book_id = Uuid::now_v7();
        let submission_id = Uuid::now_v7();
        let dag = TaskDag::reindex_pipeline(reindex_book_id);
        let first_submission = TaskQueue::new(pool.clone())
            .submit_dag_once(submission_id, &dag)
            .await?;
        let replayed_submission = TaskQueue::new(pool.clone())
            .submit_dag_once(submission_id, &dag)
            .await?;
        assert_eq!(first_submission, replayed_submission);
        let persisted_child_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM tasks WHERE idempotency_key LIKE $1")
                .bind(format!("{submission_id}:%"))
                .fetch_one(&pool)
                .await?;
        assert_eq!(persisted_child_count, 3);

        Ok(())
    }

    async fn create_recovery_test_schema(pool: &PgPool) -> sqlx::Result<()> {
        sqlx::raw_sql(
            r#"CREATE TYPE task_kind AS ENUM (
                   'deep_analysis', 'semantic_tagging', 'deduplicate', 'clean_content',
                   'generate_embeddings', 'index_meilisearch', 'compute_book_embedding'
               );
               CREATE TYPE task_status AS ENUM (
                   'queued', 'running', 'completed', 'failed', 'retrying',
                   'cancelled', 'dead_letter'
               );
               CREATE TYPE task_priority AS ENUM ('0', '1', '2', '3');
               CREATE TABLE tasks (
                   id UUID PRIMARY KEY,
                   kind task_kind NOT NULL,
                   status task_status NOT NULL,
                   priority task_priority NOT NULL,
                   payload JSONB NOT NULL DEFAULT '{}'::jsonb,
                   book_id UUID,
                   category VARCHAR(32) NOT NULL,
                   error_message TEXT,
                   max_retries INTEGER NOT NULL DEFAULT 3,
                   idempotency_key TEXT,
                   scheduled_at TIMESTAMPTZ NOT NULL,
                   started_at TIMESTAMPTZ,
                   completed_at TIMESTAMPTZ,
                   created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
               );
               CREATE UNIQUE INDEX idx_tasks_idempotency_key
                   ON tasks(idempotency_key) WHERE idempotency_key IS NOT NULL;
               CREATE TABLE task_dependencies (
                   task_id UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
                   depends_on UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
                   PRIMARY KEY (task_id, depends_on)
               );
               CREATE TABLE dedup_scan_runs (
                   id UUID PRIMARY KEY,
                   task_id UUID REFERENCES tasks(id),
                   include_semantic BOOLEAN NOT NULL,
                   algorithm_version INTEGER NOT NULL,
                   status VARCHAR(24) NOT NULL,
                   progress SMALLINT NOT NULL DEFAULT 0,
                   progress_message TEXT,
                   error_message TEXT,
                   started_at TIMESTAMPTZ,
                   completed_at TIMESTAMPTZ,
                   updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
               );"#,
        )
        .execute(pool)
        .await?;
        Ok(())
    }
}
