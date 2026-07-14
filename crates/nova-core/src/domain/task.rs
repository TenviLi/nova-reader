use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use uuid::Uuid;

use crate::Id;

/// Marker type for Task IDs.
#[derive(Debug, Clone, Copy)]
pub struct TaskMarker;
pub type TaskId = Id<TaskMarker>;

/// A background task managed by the worker queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub kind: TaskKind,
    pub status: TaskStatus,
    pub priority: TaskPriority,
    pub payload: serde_json::Value,
    pub result: Option<serde_json::Value>,
    pub error_message: Option<String>,
    pub retry_count: i32,
    pub max_retries: i32,
    pub book_id: Option<Uuid>,
    pub category: TaskCategory,
    pub progress: i16,
    pub progress_message: Option<String>,
    pub scheduled_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// Types of background tasks the system processes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "task_kind", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    /// Parse a new file into chapters
    ParseFile,
    /// Generate embeddings for text chunks
    GenerateEmbeddings,
    /// Extract entities for knowledge graph
    ExtractEntities,
    /// Perform deduplication check
    Deduplicate,
    /// Translate content
    Translate,
    /// Clean/normalize text content
    CleanContent,
    /// Full library scan
    LibraryScan,
    /// Generate book metadata (summary, genres)
    GenerateMetadata,
    /// Build graph community summaries
    BuildGraphSummary,
    /// Index chunks into Meilisearch
    IndexMeilisearch,
    /// Sync entities to Neo4j
    SyncNeo4j,
    /// Compute book-level embedding centroid
    ComputeBookEmbedding,
    /// Detect communities in knowledge graph
    DetectCommunities,
    /// Deep analysis: chapter summaries + sentiment arcs + foreshadowing + state tracking
    DeepAnalysis,
    /// Compute per-chapter sentiment arc scores
    SentimentArc,
    /// Detect and track foreshadowing (setup/payoff)
    TrackForeshadowing,
    /// Compute semantic tag scores against user-defined profiles
    SemanticTagging,
    /// Assign book chunks to ontology tree (trope discovery + persona tracking)
    AssignOntology,
    /// Enqueue per-book reindexing work for a library
    ReindexLibrary,
    /// Remove generated cover files no longer referenced by library books/series
    CleanupOrphanCovers,
    /// Recompute SHA-256 file hashes for books in a library
    RecomputeFileHashes,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown task kind: {0}")]
pub struct UnknownTaskKind(pub String);

impl TaskKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ParseFile => "parse_file",
            Self::GenerateEmbeddings => "generate_embeddings",
            Self::ExtractEntities => "extract_entities",
            Self::Deduplicate => "deduplicate",
            Self::Translate => "translate",
            Self::CleanContent => "clean_content",
            Self::LibraryScan => "library_scan",
            Self::GenerateMetadata => "generate_metadata",
            Self::BuildGraphSummary => "build_graph_summary",
            Self::IndexMeilisearch => "index_meilisearch",
            Self::SyncNeo4j => "sync_neo4j",
            Self::ComputeBookEmbedding => "compute_book_embedding",
            Self::DetectCommunities => "detect_communities",
            Self::DeepAnalysis => "deep_analysis",
            Self::SentimentArc => "sentiment_arc",
            Self::TrackForeshadowing => "track_foreshadowing",
            Self::SemanticTagging => "semantic_tagging",
            Self::AssignOntology => "assign_ontology",
            Self::ReindexLibrary => "reindex_library",
            Self::CleanupOrphanCovers => "cleanup_orphan_covers",
            Self::RecomputeFileHashes => "recompute_file_hashes",
        }
    }
}

impl FromStr for TaskKind {
    type Err = UnknownTaskKind;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "parse_file" => Ok(Self::ParseFile),
            "generate_embeddings" => Ok(Self::GenerateEmbeddings),
            "extract_entities" => Ok(Self::ExtractEntities),
            "deduplicate" => Ok(Self::Deduplicate),
            "translate" => Ok(Self::Translate),
            "clean_content" => Ok(Self::CleanContent),
            "library_scan" => Ok(Self::LibraryScan),
            "generate_metadata" => Ok(Self::GenerateMetadata),
            "build_graph_summary" => Ok(Self::BuildGraphSummary),
            "index_meilisearch" => Ok(Self::IndexMeilisearch),
            "sync_neo4j" => Ok(Self::SyncNeo4j),
            "compute_book_embedding" => Ok(Self::ComputeBookEmbedding),
            "detect_communities" => Ok(Self::DetectCommunities),
            "deep_analysis" => Ok(Self::DeepAnalysis),
            "sentiment_arc" => Ok(Self::SentimentArc),
            "track_foreshadowing" => Ok(Self::TrackForeshadowing),
            "semantic_tagging" => Ok(Self::SemanticTagging),
            "assign_ontology" => Ok(Self::AssignOntology),
            "reindex_library" => Ok(Self::ReindexLibrary),
            "cleanup_orphan_covers" => Ok(Self::CleanupOrphanCovers),
            "recompute_file_hashes" => Ok(Self::RecomputeFileHashes),
            other => Err(UnknownTaskKind(other.to_string())),
        }
    }
}

/// Task category for UI grouping (like immich).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskCategory {
    Import,
    Preprocess,
    Ai,
    Index,
    Maintenance,
}

impl Default for TaskCategory {
    fn default() -> Self {
        Self::Ai
    }
}

impl TaskCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Import => "import",
            Self::Preprocess => "preprocess",
            Self::Ai => "ai",
            Self::Index => "index",
            Self::Maintenance => "maintenance",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "import" => Self::Import,
            "preprocess" => Self::Preprocess,
            "ai" => Self::Ai,
            "index" => Self::Index,
            "maintenance" => Self::Maintenance,
            _ => Self::Ai,
        }
    }
}

impl TaskKind {
    /// Get the default category for this task kind.
    pub fn category(&self) -> TaskCategory {
        match self {
            Self::ParseFile | Self::LibraryScan => TaskCategory::Import,
            Self::CleanContent | Self::Deduplicate | Self::Translate => TaskCategory::Preprocess,
            Self::GenerateEmbeddings
            | Self::ExtractEntities
            | Self::GenerateMetadata
            | Self::BuildGraphSummary
            | Self::DetectCommunities
            | Self::ComputeBookEmbedding
            | Self::DeepAnalysis
            | Self::SentimentArc
            | Self::TrackForeshadowing
            | Self::SemanticTagging
            | Self::AssignOntology => TaskCategory::Ai,
            Self::IndexMeilisearch | Self::SyncNeo4j => TaskCategory::Index,
            Self::ReindexLibrary | Self::CleanupOrphanCovers | Self::RecomputeFileHashes => {
                TaskCategory::Maintenance
            }
        }
    }
}

/// Task execution status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "task_status", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Retrying,
    Cancelled,
    DeadLetter,
}

/// Task priority levels for queue ordering.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, sqlx::Type,
)]
#[sqlx(type_name = "task_priority")]
#[serde(rename_all = "lowercase")]
pub enum TaskPriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

/// Conflict mode for a durable task resource declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskExecutionLockMode {
    Shared,
    Exclusive,
}

impl TaskExecutionLockMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Shared => "shared",
            Self::Exclusive => "exclusive",
        }
    }
}

/// Summary of queue health for the admin dashboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueStats {
    pub queued: i64,
    pub running: i64,
    pub completed_today: i64,
    pub failed_today: i64,
    pub dead_letter_count: i64,
    pub avg_processing_time_ms: f64,
}

/// DAG of tasks for a single book processing pipeline.
/// Defines which tasks depend on which other tasks.
#[derive(Debug, Clone)]
pub struct TaskDag {
    pub book_id: Uuid,
    pub tasks: Vec<TaskNode>,
}

#[derive(Debug, Clone)]
pub struct TaskNode {
    pub kind: TaskKind,
    pub priority: TaskPriority,
    pub depends_on: Vec<TaskKind>,
    pub payload: serde_json::Value,
}

impl TaskDag {
    /// Create a full processing DAG for a newly ingested book.
    pub fn full_pipeline(book_id: Uuid) -> Self {
        let payload = serde_json::json!({ "book_id": book_id.to_string() });
        Self {
            book_id,
            tasks: vec![
                TaskNode {
                    kind: TaskKind::CleanContent,
                    priority: TaskPriority::High,
                    depends_on: vec![],
                    payload: payload.clone(),
                },
                TaskNode {
                    kind: TaskKind::GenerateEmbeddings,
                    priority: TaskPriority::Normal,
                    depends_on: vec![TaskKind::CleanContent],
                    payload: payload.clone(),
                },
                TaskNode {
                    kind: TaskKind::ExtractEntities,
                    priority: TaskPriority::Normal,
                    depends_on: vec![TaskKind::CleanContent],
                    payload: payload.clone(),
                },
                TaskNode {
                    kind: TaskKind::IndexMeilisearch,
                    priority: TaskPriority::High,
                    depends_on: vec![TaskKind::CleanContent],
                    payload: payload.clone(),
                },
                TaskNode {
                    kind: TaskKind::GenerateMetadata,
                    priority: TaskPriority::Low,
                    depends_on: vec![TaskKind::CleanContent],
                    payload: payload.clone(),
                },
                TaskNode {
                    kind: TaskKind::SyncNeo4j,
                    priority: TaskPriority::Normal,
                    depends_on: vec![TaskKind::ExtractEntities],
                    payload: payload.clone(),
                },
                TaskNode {
                    kind: TaskKind::ComputeBookEmbedding,
                    priority: TaskPriority::Low,
                    depends_on: vec![TaskKind::GenerateEmbeddings],
                    payload: payload.clone(),
                },
                TaskNode {
                    kind: TaskKind::DetectCommunities,
                    priority: TaskPriority::Low,
                    depends_on: vec![TaskKind::SyncNeo4j],
                    payload: payload.clone(),
                },
                TaskNode {
                    kind: TaskKind::AssignOntology,
                    priority: TaskPriority::Low,
                    depends_on: vec![TaskKind::GenerateEmbeddings],
                    payload: payload.clone(),
                },
            ],
        }
    }

    /// Create a deep analysis DAG (Micro-Macro sliding window).
    /// Runs after entities are extracted; produces chapter summaries, sentiment arcs,
    /// foreshadowing tracking, and macro-window arc analysis.
    pub fn deep_analysis_pipeline(book_id: Uuid) -> Self {
        let payload = serde_json::json!({ "book_id": book_id.to_string() });
        Self {
            book_id,
            tasks: vec![
                TaskNode {
                    kind: TaskKind::DeepAnalysis,
                    priority: TaskPriority::Normal,
                    depends_on: vec![],
                    payload: payload.clone(),
                },
                TaskNode {
                    kind: TaskKind::SentimentArc,
                    priority: TaskPriority::Low,
                    depends_on: vec![TaskKind::DeepAnalysis],
                    payload: payload.clone(),
                },
                TaskNode {
                    kind: TaskKind::TrackForeshadowing,
                    priority: TaskPriority::Low,
                    depends_on: vec![TaskKind::DeepAnalysis],
                    payload: payload.clone(),
                },
            ],
        }
    }

    /// Create a lightweight re-embedding DAG (when chunking config changes).
    pub fn reindex_pipeline(book_id: Uuid) -> Self {
        let payload = serde_json::json!({ "book_id": book_id.to_string() });
        Self {
            book_id,
            tasks: vec![
                TaskNode {
                    kind: TaskKind::GenerateEmbeddings,
                    priority: TaskPriority::Normal,
                    depends_on: vec![],
                    payload: payload.clone(),
                },
                TaskNode {
                    kind: TaskKind::IndexMeilisearch,
                    priority: TaskPriority::High,
                    depends_on: vec![],
                    payload: payload.clone(),
                },
                TaskNode {
                    kind: TaskKind::ComputeBookEmbedding,
                    priority: TaskPriority::Low,
                    depends_on: vec![TaskKind::GenerateEmbeddings],
                    payload: payload.clone(),
                },
            ],
        }
    }
}
