use thiserror::Error;

/// Validation errors emitted by deterministic deduplication primitives.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DedupError {
    #[error("invalid SHA-256 hexadecimal digest")]
    InvalidSha256Hex,
    #[error("winnowing gram_size and window_size must both be greater than zero")]
    InvalidWinnowingConfig,
    #[error("unknown duplicate relation: {0}")]
    InvalidDuplicateRelation(String),
    #[error("unknown duplicate review status: {0}")]
    InvalidDuplicateReviewStatus(String),
}

/// The canonical error type for the Nova Reader platform.
#[derive(Debug, Error)]
pub enum Error {
    // ─── Infrastructure ────────────────────────────────────────────
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("redis error: {0}")]
    Redis(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    // ─── Domain ────────────────────────────────────────────────────
    #[error("entity not found: {entity} with id {id}")]
    NotFound { entity: &'static str, id: String },

    #[error("duplicate entity: {entity} ({detail})")]
    Duplicate {
        entity: &'static str,
        detail: String,
    },

    #[error("validation failed: {0}")]
    Validation(String),

    #[error("retryable conflict: {0}")]
    RetryableConflict(String),

    // ─── Authentication ────────────────────────────────────────────
    #[error("authentication required")]
    Unauthorized,

    #[error("insufficient permissions")]
    Forbidden,

    #[error("invalid credentials")]
    InvalidCredentials,

    // ─── External Services ─────────────────────────────────────────
    #[error("AI service error: {0}")]
    AiService(String),

    #[error("vector database error: {0}")]
    VectorDb(String),

    #[error("graph database error: {0}")]
    GraphDb(String),

    // ─── Processing ────────────────────────────────────────────────
    #[error("parse error: {0}")]
    Parse(String),

    #[error("task failed: {task} - {reason}")]
    TaskFailed { task: String, reason: String },

    #[error("rate limited: retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },

    // ─── Internal ──────────────────────────────────────────────────
    #[error("internal error: {0}")]
    Internal(String),
}

impl Error {
    /// HTTP status code for this error variant.
    #[must_use]
    pub fn status_code(&self) -> u16 {
        match self {
            Self::NotFound { .. } => 404,
            Self::Duplicate { .. } | Self::Validation(_) => 400,
            Self::RetryableConflict(_) => 409,
            Self::Unauthorized | Self::InvalidCredentials => 401,
            Self::Forbidden => 403,
            Self::RateLimited { .. } => 429,
            _ => 500,
        }
    }

    /// Whether this error is retryable.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Database(_)
                | Self::Redis(_)
                | Self::AiService(_)
                | Self::VectorDb(_)
                | Self::GraphDb(_)
                | Self::RateLimited { .. }
                | Self::RetryableConflict(_)
        )
    }
}

/// Convenience type alias.
pub type Result<T> = std::result::Result<T, Error>;
