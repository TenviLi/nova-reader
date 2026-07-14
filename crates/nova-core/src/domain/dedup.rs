//! Shared domain types for deterministic novel deduplication.

use serde::{Deserialize, Serialize};
use std::str::FromStr;

use crate::error::DedupError;

/// Text normalization applied before fragment fingerprinting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NormalizationLevel {
    Conservative,
    Layout,
}

/// Parameters for stable k-gram winnowing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WinnowingConfig {
    /// Number of normalized Unicode scalar values in each gram.
    pub gram_size: usize,
    /// Number of consecutive gram hashes in each selection window.
    pub window_size: usize,
}

impl Default for WinnowingConfig {
    fn default() -> Self {
        Self {
            gram_size: 16,
            window_size: 8,
        }
    }
}

impl WinnowingConfig {
    /// Reject parameters for which the algorithm is undefined.
    pub const fn validate(self) -> Result<(), DedupError> {
        if self.gram_size == 0 || self.window_size == 0 {
            Err(DedupError::InvalidWinnowingConfig)
        } else {
            Ok(())
        }
    }
}

/// One selected stable fragment hash and its normalized character offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WinnowingFingerprint {
    pub hash: u64,
    pub position: usize,
}

/// A SHA-256 digest used by the deduplication pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Sha256Hash([u8; 32]);

impl Sha256Hash {
    /// Construct a digest from its raw bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Parse a lowercase or uppercase hexadecimal digest.
    pub fn from_hex(encoded: &str) -> Result<Self, DedupError> {
        let mut bytes = [0_u8; 32];
        hex::decode_to_slice(encoded, &mut bytes).map_err(|_| DedupError::InvalidSha256Hex)?;
        Ok(Self(bytes))
    }

    /// Return the digest as lowercase hexadecimal.
    pub fn to_hex(self) -> String {
        hex::encode(self.0)
    }

    /// Borrow the digest bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Chapter content supplied at the deduplication boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChapterInput<'a> {
    /// Stable chapter position in the source book.
    pub chapter_index: u32,
    /// Extracted chapter body (and, if desired by the caller, its title).
    pub content: &'a str,
}

/// Persistable hashes and length for one chapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChapterFingerprint {
    pub chapter_index: u32,
    pub conservative_hash: Sha256Hash,
    pub layout_hash: Sha256Hash,
    /// Visible characters after layout normalization.
    pub char_count: u64,
    pub conservative_normalization_version: u16,
    pub layout_normalization_version: u16,
}

/// Persistable whole-book content hashes and its ordered chapter fingerprints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookFingerprint {
    pub conservative_hash: Sha256Hash,
    pub layout_hash: Sha256Hash,
    pub char_count: u64,
    pub conservative_normalization_version: u16,
    pub layout_normalization_version: u16,
    pub chapters: Vec<ChapterFingerprint>,
}

/// Strength of an aligned chapter hash match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChapterMatchKind {
    Conservative,
    Layout,
}

/// A one-to-one ordered chapter match between two books.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChapterMatch {
    pub a_index: u32,
    pub b_index: u32,
    pub kind: ChapterMatchKind,
}

/// Coverage of one side of a chapter alignment.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Coverage {
    pub chapters: f64,
    pub chars: f64,
}

/// Explainable evidence produced by sparse chapter-sequence alignment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChapterAlignment {
    pub matches: Vec<ChapterMatch>,
    pub shared_chapters: usize,
    pub coverage_a: Coverage,
    pub coverage_b: Coverage,
    pub longest_run: usize,
    /// Ordered matches divided by all possible one-to-one hash matches.
    pub order_score: f64,
    pub unique_in_a: Vec<u32>,
    pub unique_in_b: Vec<u32>,
}

/// Side of a compared book pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BookSide {
    A,
    B,
}

/// Content relationship inferred from deterministic evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DuplicateRelation {
    ExactFile,
    ExactContent,
    ContainedVersion,
    HighOverlap,
    PartialOverlap,
    SemanticRelation,
    NotDuplicate,
}

impl DuplicateRelation {
    /// Stable database and API wire representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ExactFile => "exact_file",
            Self::ExactContent => "exact_content",
            Self::ContainedVersion => "contained_version",
            Self::HighOverlap => "high_overlap",
            Self::PartialOverlap => "partial_overlap",
            Self::SemanticRelation => "semantic_relation",
            Self::NotDuplicate => "not_duplicate",
        }
    }

    /// Whether this relation is persisted as a reviewable candidate.
    pub const fn is_reviewable(self) -> bool {
        !matches!(self, Self::NotDuplicate)
    }
}

impl FromStr for DuplicateRelation {
    type Err = DedupError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "exact_file" => Ok(Self::ExactFile),
            "exact_content" => Ok(Self::ExactContent),
            "contained_version" => Ok(Self::ContainedVersion),
            "high_overlap" => Ok(Self::HighOverlap),
            "partial_overlap" => Ok(Self::PartialOverlap),
            "semantic_relation" => Ok(Self::SemanticRelation),
            "not_duplicate" => Ok(Self::NotDuplicate),
            _ => Err(DedupError::InvalidDuplicateRelation(value.to_string())),
        }
    }
}

/// Human-review state for one persisted duplicate candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DuplicateReviewStatus {
    Pending,
    Confirmed,
    Dismissed,
    Deferred,
}

/// Review-queue partition. Semantic relations are intentionally kept out of
/// the deterministic content-duplicate workflow because they never authorize
/// an automatic merge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DuplicateCandidateKind {
    Content,
    Semantic,
}

/// Persisted/API schema generation for explainable duplicate evidence.
///
/// This discriminator is deliberately independent from the matching algorithm
/// version: the algorithm may change without forcing every evidence consumer
/// to learn a new wire shape.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DuplicateEvidenceSchemaVersion {
    #[default]
    V2,
}

/// Cardinality of a source-verified alignment group after chapter boundaries
/// from both concrete versions have been projected onto one text sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DuplicateAlignmentMappingShape {
    OneToOne,
    OneToMany,
    ManyToOne,
    ManyToMany,
}

/// One ordered semantic chapter pair retained as explainable evidence.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DuplicateSemanticChapterMatch {
    pub chapter_a_index: u32,
    pub chapter_b_index: u32,
    pub score: f64,
}

/// Evidence required before a semantic relation is exposed for review.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DuplicateSemanticEvidence {
    pub score: f64,
    pub independent_chunk_matches: i32,
    pub independent_chapter_matches: i32,
    pub ordered_chapter_matches: Vec<DuplicateSemanticChapterMatch>,
    pub matched_chapters_a: i32,
    pub matched_chapters_b: i32,
    pub order_score: f64,
    pub sampled_chapters_a: i32,
    pub sampled_chapters_b: i32,
    pub sample_coverage_a: Option<f64>,
    pub sample_coverage_b: Option<f64>,
    pub book_chapters_a: usize,
    pub book_chapters_b: usize,
    pub observed_book_coverage_a: f64,
    pub observed_book_coverage_b: f64,
}

/// Source-verified segment group spanning one or more chapters on either side.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DuplicateAlignmentGroupEvidence {
    pub id: i32,
    pub mapping_shape: DuplicateAlignmentMappingShape,
    pub chapters_a: Vec<u32>,
    pub chapters_b: Vec<u32>,
    pub matched_characters: i64,
    pub segment_count: usize,
    pub source_verified: bool,
}

/// Public, user-independent quality signals for one concrete book version.
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DuplicatePrimaryVersionEvidence {
    #[serde(default)]
    pub content_chars: u64,
    #[serde(default)]
    pub unique_informative_chars: u64,
    #[serde(default)]
    pub total_chapters: usize,
    #[serde(default)]
    pub informative_chapters: usize,
    #[serde(default)]
    pub unique_informative_chapters: usize,
    #[serde(default)]
    pub repeated_informative_chapters: usize,
    #[serde(default)]
    pub informative_chapter_ratio: f64,
    #[serde(default)]
    pub unique_informative_ratio: f64,
    #[serde(default)]
    pub word_count: i64,
    #[serde(default)]
    pub metadata_quality: i32,
    #[serde(default)]
    pub format_quality: i32,
    #[serde(default)]
    pub file_size_bytes: i64,
    #[serde(default)]
    pub text_integrity_score: f64,
}

/// Explainable primary-version recommendation for the compared pair.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DuplicatePrimaryRecommendationEvidence {
    pub recommended_book_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub unique_informative_content_dominates: bool,
    #[serde(default)]
    pub reader_assets_considered: bool,
    pub book_a: DuplicatePrimaryVersionEvidence,
    pub book_b: DuplicatePrimaryVersionEvidence,
}

/// Versioned evidence persisted with a duplicate pair and returned unchanged
/// through the duplicate-review interface.
///
/// Defaults on fields added during the v2 rollout keep already-persisted
/// algorithm evidence readable; every newly produced value is fully populated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DuplicatePairEvidence {
    #[serde(default)]
    pub schema_version: DuplicateEvidenceSchemaVersion,
    #[serde(default)]
    pub exact_file: bool,
    #[serde(default)]
    pub exact_content: bool,
    #[serde(default)]
    pub shared_chapter_hashes: i32,
    #[serde(default)]
    pub shared_passage_hashes: i32,
    #[serde(default)]
    pub semantic_hits: i32,
    #[serde(default)]
    pub semantic: Option<DuplicateSemanticEvidence>,
    #[serde(default)]
    pub primary_recommendation: Option<DuplicatePrimaryRecommendationEvidence>,
    #[serde(default)]
    pub equivalent_chapters: i32,
    #[serde(default)]
    pub matched_chapters_a: i32,
    #[serde(default)]
    pub matched_chapters_b: i32,
    #[serde(default)]
    pub shared_characters: u64,
    #[serde(default)]
    pub unique_characters_a: u64,
    #[serde(default)]
    pub unique_characters_b: u64,
    #[serde(default = "duplicate_alignment_schema_version")]
    pub alignment_schema_version: u16,
    #[serde(default)]
    pub chapter_boundary_groups: Vec<DuplicateAlignmentGroupEvidence>,
    #[serde(default)]
    pub unique_chapters_a: Vec<u32>,
    #[serde(default)]
    pub unique_chapters_b: Vec<u32>,
    #[serde(default)]
    pub book_a_layout_hash: String,
    #[serde(default)]
    pub book_b_layout_hash: String,
    #[serde(default)]
    pub algorithm_version: i32,
}

const fn duplicate_alignment_schema_version() -> u16 {
    2
}

/// Explicit human decision for a reviewed duplicate pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolveAction {
    KeepA,
    KeepB,
    SameWork,
    Dismiss,
    Defer,
}

impl ResolveAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::KeepA => "keep_a",
            Self::KeepB => "keep_b",
            Self::SameWork => "same_work",
            Self::Dismiss => "dismiss",
            Self::Defer => "defer",
        }
    }

    pub const fn groups_versions(self) -> bool {
        matches!(self, Self::KeepA | Self::KeepB | Self::SameWork)
    }
}

/// Maintenance operation carried by a deduplication task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DedupTaskOperation {
    Scan,
    CleanupSecondaryIndexes,
}

/// Discriminator constrained to the scan payload shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DedupScanOperation {
    Scan,
}

/// Discriminator constrained to the index-cleanup payload shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DedupIndexCleanupOperation {
    CleanupSecondaryIndexes,
}

/// Minimal typed discriminator used when a malformed task still needs its
/// persistent scan state synchronized with retry/dead-letter handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct DedupTaskEnvelope {
    pub operation: DedupTaskOperation,
}

/// Stable user-visible phase codes for a duplicate scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DedupScanPhase {
    Recovering,
    Retrying,
    Failed,
    Fingerprinting,
    CandidateGeneration,
    Verifying,
    Completed,
}

impl DedupScanPhase {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Recovering => "recovering",
            Self::Retrying => "retrying",
            Self::Failed => "failed",
            Self::Fingerprinting => "fingerprinting",
            Self::CandidateGeneration => "candidate_generation",
            Self::Verifying => "verifying",
            Self::Completed => "completed",
        }
    }

    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "recovering" => Some(Self::Recovering),
            "retrying" => Some(Self::Retrying),
            "failed" => Some(Self::Failed),
            "fingerprinting" => Some(Self::Fingerprinting),
            "candidate_generation" => Some(Self::CandidateGeneration),
            "verifying" => Some(Self::Verifying),
            "completed" => Some(Self::Completed),
            _ => None,
        }
    }
}

/// Typed payload for a full-library or targeted incremental duplicate scan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DedupScanTask {
    pub operation: DedupScanOperation,
    pub scan_run_id: uuid::Uuid,
    pub library_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub include_semantic: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_book_ids: Option<Vec<uuid::Uuid>>,
}

impl DedupScanTask {
    pub fn new(
        scan_run_id: uuid::Uuid,
        library_id: Option<uuid::Uuid>,
        include_semantic: bool,
        target_book_ids: Option<Vec<uuid::Uuid>>,
    ) -> Self {
        Self {
            operation: DedupScanOperation::Scan,
            scan_run_id,
            library_id,
            include_semantic,
            target_book_ids,
        }
    }
}

/// Typed payload for idempotent cleanup after a version is marked duplicate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DedupIndexCleanupTask {
    pub operation: DedupIndexCleanupOperation,
    /// Pair whose source-verified exact chapter matches authorize cleanup.
    /// Optional only so pre-v3 queued maintenance tasks deserialize safely;
    /// workers treat a missing pair as an unsafe no-op.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pair_id: Option<uuid::Uuid>,
    pub secondary_book_id: uuid::Uuid,
    pub primary_book_id: uuid::Uuid,
    /// Durable Meilisearch delete task accepted for this cleanup. Once set,
    /// retries resume polling this task instead of enqueueing another delete.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meilisearch_task_uid: Option<u64>,
}

/// Closed set of payloads accepted by the deduplication worker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DedupTaskPayload {
    Scan(DedupScanTask),
    IndexCleanup(DedupIndexCleanupTask),
}

impl DedupIndexCleanupTask {
    pub const fn new(
        pair_id: uuid::Uuid,
        secondary_book_id: uuid::Uuid,
        primary_book_id: uuid::Uuid,
    ) -> Self {
        Self {
            operation: DedupIndexCleanupOperation::CleanupSecondaryIndexes,
            pair_id: Some(pair_id),
            secondary_book_id,
            primary_book_id,
            meilisearch_task_uid: None,
        }
    }
}

impl DuplicateReviewStatus {
    /// Stable database and API wire representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Confirmed => "confirmed",
            Self::Dismissed => "dismissed",
            Self::Deferred => "deferred",
        }
    }
}

impl FromStr for DuplicateReviewStatus {
    type Err = DedupError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "pending" => Ok(Self::Pending),
            "confirmed" => Ok(Self::Confirmed),
            "dismissed" => Ok(Self::Dismissed),
            "deferred" => Ok(Self::Deferred),
            _ => Err(DedupError::InvalidDuplicateReviewStatus(value.to_string())),
        }
    }
}

/// Normalized evidence consumed by both exact-hash and near-match classifiers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DuplicateMetrics {
    pub chapter_count_a: usize,
    pub chapter_count_b: usize,
    pub equivalent_chapters: usize,
    pub coverage_a: f64,
    pub coverage_b: f64,
    pub character_coverage_a: f64,
    pub character_coverage_b: f64,
    pub longest_run: usize,
    pub order_score: f64,
    pub added_in_a: usize,
    pub added_in_b: usize,
    /// Informative normalized characters on each concrete version.
    pub total_chars_a: u64,
    pub total_chars_b: u64,
    /// Informative characters not covered by the selected non-crossing
    /// alignment. Unlike chapter counts, these remain meaningful when chapter
    /// boundaries differ between versions.
    pub unique_chars_a: u64,
    pub unique_chars_b: u64,
    /// Source-verified matching Winnowing grams across approximate chapter pairs.
    pub verified_passage_fingerprints: usize,
}

/// Calibratable thresholds used by deterministic pair classification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassificationThresholds {
    pub exact_min_chars: u64,
    pub contained_min_shared_chapters: usize,
    pub contained_short_book_chapter_limit: usize,
    pub contained_short_book_min_shared_chapters: usize,
    pub contained_min_chapter_coverage: f64,
    pub contained_min_char_coverage: f64,
    pub contained_min_order_score: f64,
    pub contained_min_added_chapters: usize,
    pub contained_min_added_chars: u64,
    pub high_overlap_min_shared_chapters: usize,
    pub high_overlap_min_chapter_coverage: f64,
    pub high_overlap_min_char_coverage: f64,
    pub high_overlap_min_order_score: f64,
    pub partial_min_shared_chapters: usize,
    pub partial_min_longest_run: usize,
    pub partial_min_verified_passage_fingerprints: usize,
}

impl Default for ClassificationThresholds {
    fn default() -> Self {
        Self {
            exact_min_chars: 1,
            contained_min_shared_chapters: 10,
            contained_short_book_chapter_limit: 10,
            contained_short_book_min_shared_chapters: 2,
            contained_min_chapter_coverage: 0.8,
            contained_min_char_coverage: 0.8,
            contained_min_order_score: 0.8,
            contained_min_added_chapters: 1,
            contained_min_added_chars: 120,
            high_overlap_min_shared_chapters: 3,
            high_overlap_min_chapter_coverage: 0.6,
            high_overlap_min_char_coverage: 0.6,
            high_overlap_min_order_score: 0.8,
            partial_min_shared_chapters: 10,
            partial_min_longest_run: 3,
            partial_min_verified_passage_fingerprints: 8,
        }
    }
}

/// Relation plus directionality needed for contained-version handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DuplicateClassification {
    pub relation: DuplicateRelation,
    pub contained: Option<BookSide>,
}
