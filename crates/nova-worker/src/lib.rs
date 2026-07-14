//! # Nova Worker
//!
//! A Redis-backed distributed task queue inspired by Go's Asynq.
//! Provides at-least-once execution guarantees, configurable retries,
//! priority queues, and dead letter management.

pub mod handler;
pub mod handlers;
pub mod queue;
pub mod scheduler;

pub use handler::TaskHandler;
pub use handlers::{
    EmbedChunksHandler, ExtractEntitiesHandler, IngestFileHandler,
    ScanLibraryHandler, TranslateChapterHandler,
};
pub use queue::{TaskQueue, TaskEnqueuer};
pub use scheduler::WorkerPool;

#[cfg(test)]
mod tests;
