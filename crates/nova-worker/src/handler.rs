use async_trait::async_trait;
use nova_core::domain::task::TaskKind;

/// Trait that all task handlers must implement.
/// Each handler processes a specific type of background task.
#[async_trait]
pub trait TaskHandler: Send + Sync + 'static {
    /// The kind of task this handler processes.
    fn task_kind(&self) -> TaskKind;

    /// Execute the task with the given JSON payload.
    /// Returns Ok(()) on success, or an error that may trigger retry.
    async fn handle(&self, payload: serde_json::Value) -> nova_core::Result<serde_json::Value>;

    /// Maximum number of retries for this task type.
    fn max_retries(&self) -> i32 {
        3
    }

    /// Delay between retries in milliseconds (exponential backoff base).
    fn retry_delay_ms(&self) -> u64 {
        5000
    }
}
