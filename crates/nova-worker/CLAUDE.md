# nova-worker

Redis-backed distributed task queue with at-least-once delivery, priority queues (High/Normal/Low), configurable retries, and dead letter management. Inspired by Go's Asynq pattern.

## Architecture

```
src/
├── lib.rs           — Re-exports
├── queue.rs         — TaskQueue (consumer) + TaskEnqueuer (producer)
├── handler.rs       — TaskHandler trait definition
├── handlers/        — Concrete handler implementations
│   ├── ingest.rs    — IngestFileHandler (parse + split + embed + index)
│   ├── translate.rs — TranslateChapterHandler (glossary-aware)
│   ├── embed.rs     — EmbedChunksHandler (batch embedding)
│   ├── extract.rs   — ExtractEntitiesHandler (NER + graph)
│   └── scan.rs      — ScanLibraryHandler (directory discovery)
└── scheduler.rs     — WorkerPool (concurrent worker management)
```

## Key Types

```rust
pub trait TaskHandler: Send + Sync {
    fn task_kind(&self) -> TaskKind;
    async fn handle(&self, payload: Value) -> Result<Value>;
}

pub struct TaskEnqueuer { /* enqueue(kind, payload, priority) → task_id */ }
pub struct TaskQueue { /* listen(), process_next() → loops forever */ }
pub struct WorkerPool { /* new(concurrency), start(), graceful_shutdown() */ }
```

## Key Patterns

- **Priority queues**: `nova:queue:HIGH`, `:NORMAL`, `:LOW` Redis lists
- **Retry with backoff**: Exponential backoff, max 3 retries (configurable)
- **Dead letter**: Failed tasks after max retries → `nova:queue:dead`
- **At-least-once**: Tasks only removed from queue after handler returns Ok
- **Graceful shutdown**: Waits for in-flight tasks on SIGTERM
- **Concurrency**: `NOVA_WORKER_CONCURRENCY=8` parallel task executors

## Dependencies

- **Internal**: nova-core, nova-ingest, nova-search, nova-embed, nova-translate, nova-graph
- **External**: redis, tokio, serde_json, sqlx

## Build & Test

```bash
cargo build -p nova-worker
cargo test -p nova-worker
cargo run -p nova-worker  # Starts worker pool
```

## Environment

Requires: Redis (task queue), PostgreSQL (task status), all services for handlers
