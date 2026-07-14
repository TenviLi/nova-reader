use std::collections::HashMap;
use std::sync::Arc;

use nova_core::domain::task::TaskKind;
use tokio::task::JoinSet;

use crate::handler::TaskHandler;
use crate::queue::{TaskQueue, TaskMessage};

/// Worker pool that spawns concurrent workers to process tasks.
pub struct WorkerPool {
    queue: Arc<TaskQueue>,
    handlers: HashMap<TaskKind, Arc<dyn TaskHandler>>,
    concurrency: usize,
}

impl WorkerPool {
    pub fn new(queue: TaskQueue, concurrency: usize) -> Self {
        Self {
            queue: Arc::new(queue),
            handlers: HashMap::new(),
            concurrency,
        }
    }

    /// Register a handler for a specific task kind.
    pub fn register<H: TaskHandler>(&mut self, handler: H) {
        let kind = handler.task_kind();
        self.handlers.insert(kind, Arc::new(handler));
    }

    /// Start the worker pool. Runs until the cancellation token is triggered.
    pub async fn run(&self, shutdown: tokio::sync::watch::Receiver<bool>) {
        tracing::info!(
            concurrency = self.concurrency,
            handlers = self.handlers.len(),
            "Worker pool starting"
        );

        let mut join_set = JoinSet::new();

        for worker_id in 0..self.concurrency {
            let queue = Arc::clone(&self.queue);
            let handlers = self.handlers.clone();
            let mut shutdown = shutdown.clone();

            join_set.spawn(async move {
                tracing::debug!(worker_id, "Worker started");

                loop {
                    tokio::select! {
                        _ = shutdown.changed() => {
                            if *shutdown.borrow() {
                                tracing::info!(worker_id, "Worker shutting down");
                                break;
                            }
                        }
                        result = queue.dequeue(5) => {
                            match result {
                                Ok(Some(msg)) => {
                                    process_task(worker_id, &handlers, &queue, msg).await;
                                }
                                Ok(None) => {
                                    // No tasks available, loop will retry
                                }
                                Err(e) => {
                                    tracing::error!(worker_id, error = %e, "Queue dequeue error");
                                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                                }
                            }
                        }
                    }
                }
            });
        }

        // Wait for all workers to finish
        while join_set.join_next().await.is_some() {}
    }
}

async fn process_task(
    worker_id: usize,
    handlers: &HashMap<TaskKind, Arc<dyn TaskHandler>>,
    queue: &TaskQueue,
    mut msg: TaskMessage,
) {
    let Some(handler) = handlers.get(&msg.kind) else {
        tracing::error!(
            worker_id,
            task_kind = ?msg.kind,
            "No handler registered for task kind"
        );
        return;
    };

    tracing::info!(
        worker_id,
        task_id = %msg.id,
        task_kind = ?msg.kind,
        retry = msg.retry_count,
        "Processing task"
    );

    match handler.handle(msg.payload.clone()).await {
        Ok(result) => {
            tracing::info!(
                worker_id,
                task_id = %msg.id,
                result = %result,
                "Task completed successfully"
            );
        }
        Err(e) => {
            msg.retry_count += 1;
            if msg.retry_count >= msg.max_retries {
                tracing::error!(
                    worker_id,
                    task_id = %msg.id,
                    error = %e,
                    "Task exhausted retries, moving to dead letter"
                );
                if let Err(dl_err) = queue.dead_letter(&msg).await {
                    tracing::error!(error = %dl_err, "Failed to dead-letter task");
                }
            } else {
                tracing::warn!(
                    worker_id,
                    task_id = %msg.id,
                    error = %e,
                    retry = msg.retry_count,
                    "Task failed, will retry"
                );
                // Re-enqueue with delay (exponential backoff)
                let delay = std::time::Duration::from_millis(
                    handler.retry_delay_ms() * 2u64.pow(msg.retry_count as u32 - 1),
                );
                tokio::time::sleep(delay).await;
                // In production, re-enqueue to Redis here
            }
        }
    }
}
