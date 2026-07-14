use chrono::Utc;
use nova_core::domain::task::{TaskKind, TaskPriority};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Message format stored in Redis queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMessage {
    pub id: Uuid,
    pub kind: TaskKind,
    pub priority: TaskPriority,
    pub payload: serde_json::Value,
    pub retry_count: i32,
    pub max_retries: i32,
    pub created_at: chrono::DateTime<Utc>,
    pub scheduled_at: chrono::DateTime<Utc>,
}

/// Interface for enqueueing new tasks.
#[derive(Clone)]
pub struct TaskEnqueuer {
    redis: redis::Client,
    queue_prefix: String,
}

impl TaskEnqueuer {
    pub fn new(redis: redis::Client) -> Self {
        Self {
            redis,
            queue_prefix: "nova:queue".to_string(),
        }
    }

    /// Enqueue a task for immediate processing.
    pub async fn enqueue(
        &self,
        kind: TaskKind,
        payload: serde_json::Value,
        priority: TaskPriority,
    ) -> nova_core::Result<Uuid> {
        let id = Uuid::now_v7();
        let msg = TaskMessage {
            id,
            kind,
            priority,
            payload,
            retry_count: 0,
            max_retries: 3,
            created_at: Utc::now(),
            scheduled_at: Utc::now(),
        };

        let serialized = serde_json::to_string(&msg)
            .map_err(|e| nova_core::Error::Internal(e.to_string()))?;

        let queue_name = format!("{}:{:?}", self.queue_prefix, priority);

        let mut conn = self
            .redis
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| nova_core::Error::Redis(e.to_string()))?;

        conn.lpush::<_, _, ()>(&queue_name, &serialized)
            .await
            .map_err(|e| nova_core::Error::Redis(e.to_string()))?;

        tracing::info!(task_id = %id, task_kind = ?kind, "Task enqueued");

        Ok(id)
    }

    /// Enqueue a task to be processed after a delay.
    pub async fn enqueue_delayed(
        &self,
        kind: TaskKind,
        payload: serde_json::Value,
        delay: std::time::Duration,
    ) -> nova_core::Result<Uuid> {
        let id = Uuid::now_v7();
        let scheduled_at = Utc::now() + chrono::Duration::from_std(delay)
            .map_err(|e| nova_core::Error::Internal(e.to_string()))?;

        let msg = TaskMessage {
            id,
            kind,
            priority: TaskPriority::Normal,
            payload,
            retry_count: 0,
            max_retries: 3,
            created_at: Utc::now(),
            scheduled_at,
        };

        let serialized = serde_json::to_string(&msg)
            .map_err(|e| nova_core::Error::Internal(e.to_string()))?;

        let mut conn = self
            .redis
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| nova_core::Error::Redis(e.to_string()))?;

        // Use sorted set with score = scheduled timestamp
        let score = scheduled_at.timestamp_millis() as f64;
        conn.zadd::<_, _, _, ()>("nova:scheduled", &serialized, score)
            .await
            .map_err(|e| nova_core::Error::Redis(e.to_string()))?;

        Ok(id)
    }
}

/// Interface for consuming tasks from the queue.
pub struct TaskQueue {
    redis: redis::Client,
    queue_prefix: String,
}

impl TaskQueue {
    pub fn new(redis: redis::Client) -> Self {
        Self {
            redis,
            queue_prefix: "nova:queue".to_string(),
        }
    }

    /// Blocking dequeue — waits for the next task (priority-ordered).
    pub async fn dequeue(&self, timeout_secs: u64) -> nova_core::Result<Option<TaskMessage>> {
        let mut conn = self
            .redis
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| nova_core::Error::Redis(e.to_string()))?;

        // Try queues in priority order: Critical > High > Normal > Low
        let queues = [
            format!("{}:Critical", self.queue_prefix),
            format!("{}:High", self.queue_prefix),
            format!("{}:Normal", self.queue_prefix),
            format!("{}:Low", self.queue_prefix),
        ];

        for queue in &queues {
            let result: Option<String> = conn
                .rpop(queue, None)
                .await
                .map_err(|e| nova_core::Error::Redis(e.to_string()))?;

            if let Some(data) = result {
                let msg: TaskMessage = serde_json::from_str(&data)
                    .map_err(|e| nova_core::Error::Parse(e.to_string()))?;
                return Ok(Some(msg));
            }
        }

        let blocking_queues = queues.iter().map(String::as_str).collect::<Vec<_>>();
        let result: Option<[String; 2]> = conn
            .brpop(&blocking_queues, timeout_secs as f64)
            .await
            .map_err(|e| nova_core::Error::Redis(e.to_string()))?;

        let Some([_, data]) = result else {
            return Ok(None);
        };

        let msg: TaskMessage =
            serde_json::from_str(&data).map_err(|e| nova_core::Error::Parse(e.to_string()))?;
        Ok(Some(msg))
    }

    /// Move a failed task to the dead letter queue.
    pub async fn dead_letter(&self, msg: &TaskMessage) -> nova_core::Result<()> {
        let serialized = serde_json::to_string(msg)
            .map_err(|e| nova_core::Error::Internal(e.to_string()))?;

        let mut conn = self
            .redis
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| nova_core::Error::Redis(e.to_string()))?;

        conn.lpush::<_, _, ()>("nova:dead_letter", &serialized)
            .await
            .map_err(|e| nova_core::Error::Redis(e.to_string()))?;

        tracing::warn!(task_id = %msg.id, "Task moved to dead letter queue");
        Ok(())
    }
}
