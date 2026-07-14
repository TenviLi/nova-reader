//! Error types exposed by the ingestion crate.

use thiserror::Error;

/// Fatal errors reported by library discovery.
#[derive(Debug, Error)]
pub enum ScannerError {
    #[error("failed to access library files: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to compile a filename ordering pattern: {0}")]
    FilenamePattern(#[from] regex::Error),
}

pub type ScannerResult<T> = std::result::Result<T, ScannerError>;

/// Configuration errors reported by the deterministic synthetic benchmark.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SyntheticBenchmarkError {
    #[error("synthetic benchmark requires at least two books")]
    TooFewBooks,
    #[error("chapters_per_book must be at least min_shared_chapters and both must be positive")]
    TooFewChapters,
    #[error("related_pair_every must be at least two")]
    InvalidRelatedPairStride,
    #[error("contained_added_chapters must be positive")]
    NoContainedGrowth,
    #[error("max_hash_document_frequency must be at least two")]
    InvalidDocumentFrequency,
    #[error("synthetic identifiers exceed supported chapter index range")]
    IdentifierOverflow,
}

/// Input and consistency errors reported by relation evaluation.
#[derive(Debug, Error)]
pub enum EvaluationError {
    #[error("failed to read JSONL input: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid JSON on line {line}: {source}")]
    InvalidJson {
        line: usize,
        #[source]
        source: serde_json::Error,
    },
    #[error("duplicate label for pair_id {pair_id}")]
    DuplicateLabel { pair_id: String },
    #[error("duplicate prediction for pair_id {pair_id}")]
    DuplicatePrediction { pair_id: String },
    #[error("no labeled pairs were supplied")]
    EmptyLabels,
    #[error("missing prediction for pair_id {pair_id}")]
    MissingPrediction { pair_id: String },
    #[error("prediction has no matching label: {pair_id}")]
    UnexpectedPrediction { pair_id: String },
}
