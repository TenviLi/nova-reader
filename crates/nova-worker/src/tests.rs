#[cfg(test)]
mod queue_tests {
    use crate::queue::TaskMessage;
    use chrono::Utc;
    use nova_core::domain::task::{TaskKind, TaskPriority};
    use uuid::Uuid;

    fn make_message(kind: TaskKind, priority: TaskPriority) -> TaskMessage {
        TaskMessage {
            id: Uuid::now_v7(),
            kind,
            priority,
            payload: serde_json::json!({"book_id": "test-123"}),
            retry_count: 0,
            max_retries: 3,
            created_at: Utc::now(),
            scheduled_at: Utc::now(),
        }
    }

    #[test]
    fn task_message_serde_roundtrip() {
        let msg = make_message(TaskKind::ParseFile, TaskPriority::Normal);
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: TaskMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, msg.id);
        assert_eq!(deserialized.kind, TaskKind::ParseFile);
        assert_eq!(deserialized.priority, TaskPriority::Normal);
        assert_eq!(deserialized.retry_count, 0);
        assert_eq!(deserialized.max_retries, 3);
    }

    #[test]
    fn task_message_all_kinds_serialize() {
        let kinds = vec![
            TaskKind::ParseFile,
            TaskKind::GenerateEmbeddings,
            TaskKind::ExtractEntities,
            TaskKind::Deduplicate,
            TaskKind::Translate,
            TaskKind::CleanContent,
            TaskKind::LibraryScan,
            TaskKind::GenerateMetadata,
            TaskKind::BuildGraphSummary,
        ];
        for kind in kinds {
            let msg = make_message(kind, TaskPriority::High);
            let json = serde_json::to_string(&msg).unwrap();
            assert!(!json.is_empty());
            let back: TaskMessage = serde_json::from_str(&json).unwrap();
            assert_eq!(back.kind, kind);
        }
    }

    #[test]
    fn task_message_payload_is_preserved() {
        let payload = serde_json::json!({
            "book_id": "abc-123",
            "chapters": [1, 2, 3],
            "options": {"force": true}
        });
        let msg = TaskMessage {
            id: Uuid::now_v7(),
            kind: TaskKind::GenerateEmbeddings,
            priority: TaskPriority::High,
            payload: payload.clone(),
            retry_count: 2,
            max_retries: 5,
            created_at: Utc::now(),
            scheduled_at: Utc::now(),
        };

        let json = serde_json::to_string(&msg).unwrap();
        let back: TaskMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(back.payload["book_id"], "abc-123");
        assert_eq!(back.payload["chapters"][1], 2);
        assert_eq!(back.payload["options"]["force"], true);
        assert_eq!(back.retry_count, 2);
        assert_eq!(back.max_retries, 5);
    }

    #[test]
    fn task_enqueuer_construction() {
        // Smoke test: constructing with a valid redis URL should not panic
        // (We can't actually connect in unit tests but construction is fine)
        let client = redis::Client::open("redis://localhost:6379").unwrap();
        let enqueuer = crate::queue::TaskEnqueuer::new(client);
        // enqueuer exists and can be cloned
        let _clone = enqueuer.clone();
    }

    #[test]
    fn task_queue_construction() {
        let client = redis::Client::open("redis://localhost:6379").unwrap();
        let _queue = crate::queue::TaskQueue::new(client);
    }
}

#[cfg(test)]
mod handler_tests {
    use async_trait::async_trait;
    use nova_core::domain::task::TaskKind;
    use crate::handler::TaskHandler;

    struct TestHandler;

    #[async_trait]
    impl TaskHandler for TestHandler {
        fn task_kind(&self) -> TaskKind {
            TaskKind::CleanContent
        }

        async fn handle(&self, payload: serde_json::Value) -> nova_core::Result<serde_json::Value> {
            Ok(serde_json::json!({"processed": payload["input"]}))
        }
    }

    #[tokio::test]
    async fn handler_trait_default_max_retries() {
        let handler = TestHandler;
        assert_eq!(handler.max_retries(), 3);
    }

    #[tokio::test]
    async fn handler_trait_default_retry_delay() {
        let handler = TestHandler;
        assert_eq!(handler.retry_delay_ms(), 5000);
    }

    #[tokio::test]
    async fn handler_can_process_payload() {
        let handler = TestHandler;
        let payload = serde_json::json!({"input": "hello"});
        let result = handler.handle(payload).await.unwrap();
        assert_eq!(result["processed"], "hello");
    }

    #[test]
    fn handler_task_kind_matches() {
        let handler = TestHandler;
        assert_eq!(handler.task_kind(), TaskKind::CleanContent);
    }
}

#[cfg(test)]
mod scheduler_tests {
    use crate::scheduler::WorkerPool;
    use crate::queue::TaskQueue;

    #[test]
    fn worker_pool_construction() {
        let client = redis::Client::open("redis://localhost:6379").unwrap();
        let queue = TaskQueue::new(client);
        let pool = WorkerPool::new(queue, 4);
        // Construction should succeed without needing a connection
        let _ = pool;
    }
}
