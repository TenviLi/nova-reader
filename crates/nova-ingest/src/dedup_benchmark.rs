//! Reproducible, database-independent capacity benchmark for deterministic
//! duplicate candidate generation and relation verification.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::process::Command;
use std::time::Instant;

use nova_core::domain::dedup::{
    BookFingerprint, ChapterInput, ClassificationThresholds, DuplicateRelation, Sha256Hash,
};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::dedup::{align_chapters, classify_pair, fingerprint_book, DEDUP_ALGORITHM_VERSION};
pub use crate::error::SyntheticBenchmarkError;

/// Version of the deterministic synthetic-corpus recipe.
pub const SYNTHETIC_CORPUS_VERSION: u16 = 2;

/// Capacity-run parameters. Defaults model the 1k manual smoke profile; use
/// `books = 10_000` for the larger profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SyntheticBenchmarkConfig {
    pub books: usize,
    pub chapters_per_book: usize,
    /// Book 1 in every stride is a variant of its immediately preceding book.
    pub related_pair_every: usize,
    pub contained_added_chapters: usize,
    pub max_hash_document_frequency: usize,
    pub min_shared_chapters: usize,
    pub seed: u64,
}

impl Default for SyntheticBenchmarkConfig {
    fn default() -> Self {
        Self {
            books: 1_000,
            chapters_per_book: 12,
            related_pair_every: 20,
            contained_added_chapters: 5,
            max_hash_document_frequency: 50,
            min_shared_chapters: 2,
            seed: 42,
        }
    }
}

/// Wall-clock durations from one process invocation. These values are
/// measurements, not stable expectations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BenchmarkTimings {
    pub fingerprint_ms: u128,
    pub candidate_generation_ms: u128,
    pub validation_ms: u128,
    pub total_ms: u128,
}

/// Process RSS samples. Linux additionally exposes VmHWM as peak RSS; on
/// other systems the benchmark reports snapshots and documents an external
/// peak-memory command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BenchmarkMemory {
    pub rss_start_bytes: Option<u64>,
    pub rss_after_fingerprints_bytes: Option<u64>,
    pub rss_with_candidate_indexes_bytes: Option<u64>,
    pub rss_after_validation_bytes: Option<u64>,
    pub peak_rss_bytes: Option<u64>,
    pub note: String,
}

/// Observable output of a complete in-process synthetic capacity run.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SyntheticBenchmarkReport {
    pub synthetic_corpus_version: u16,
    pub dedup_algorithm_version: u16,
    pub pipeline_scope: String,
    pub config: SyntheticBenchmarkConfig,
    pub books: usize,
    pub chapters: usize,
    pub corpus_digest_sha256: String,
    pub expected_related_pairs: usize,
    pub candidate_pairs: usize,
    pub false_candidate_pairs: usize,
    pub candidate_recall: f64,
    pub average_candidates_per_book: f64,
    pub p95_candidates_per_book: usize,
    pub suppressed_high_frequency_chapter_hashes: usize,
    pub expected_relation_correct: usize,
    pub verified_relation_counts: BTreeMap<String, usize>,
    pub timings: BenchmarkTimings,
    pub memory: BenchmarkMemory,
}

#[derive(Debug, Clone, Copy, Default)]
struct CandidateEvidence {
    exact_content: bool,
    shared_chapters: usize,
}

/// Run the same synthetic recipe for any requested size. Corpus shape,
/// candidate counts and classifications are deterministic for a fixed config;
/// timings and RSS remain machine measurements.
pub fn run_synthetic_benchmark(
    config: &SyntheticBenchmarkConfig,
) -> Result<SyntheticBenchmarkReport, SyntheticBenchmarkError> {
    validate_config(config)?;

    let total_started = Instant::now();
    let rss_start_bytes = current_rss_bytes();

    let fingerprint_started = Instant::now();
    let mut fingerprints = Vec::with_capacity(config.books);
    let mut expected_relations = BTreeMap::new();
    let mut corpus_hasher = Sha256::new();
    for book_index in 0..config.books {
        let synthetic = synthetic_book(book_index, config)?;
        corpus_hasher.update(
            u64::try_from(book_index)
                .map_err(|_| SyntheticBenchmarkError::IdentifierOverflow)?
                .to_le_bytes(),
        );
        corpus_hasher.update(synthetic.fingerprint.conservative_hash.as_bytes());
        corpus_hasher.update(synthetic.fingerprint.layout_hash.as_bytes());
        for chapter in &synthetic.fingerprint.chapters {
            corpus_hasher.update(chapter.conservative_hash.as_bytes());
            corpus_hasher.update(chapter.layout_hash.as_bytes());
        }
        if let Some((base_index, relation)) = synthetic.expected_relation {
            expected_relations.insert((base_index, book_index), relation);
        }
        fingerprints.push(synthetic.fingerprint);
    }
    let fingerprint_ms = fingerprint_started.elapsed().as_millis();
    let rss_after_fingerprints_bytes = current_rss_bytes();

    let candidate_started = Instant::now();
    let mut whole_hash_postings = HashMap::<(u16, Sha256Hash), Vec<usize>>::new();
    let mut chapter_hash_postings = HashMap::<Sha256Hash, Vec<usize>>::new();
    for (book_index, fingerprint) in fingerprints.iter().enumerate() {
        whole_hash_postings
            .entry((
                fingerprint.conservative_normalization_version,
                fingerprint.conservative_hash,
            ))
            .or_default()
            .push(book_index);
        let mut seen_chapter_hashes = HashSet::new();
        for chapter in &fingerprint.chapters {
            if seen_chapter_hashes.insert(chapter.layout_hash) {
                chapter_hash_postings
                    .entry(chapter.layout_hash)
                    .or_default()
                    .push(book_index);
            }
        }
    }

    let mut candidates = HashMap::<(usize, usize), CandidateEvidence>::new();
    for postings in whole_hash_postings.values() {
        for_each_pair(postings, |pair| {
            candidates.entry(pair).or_default().exact_content = true;
        });
    }
    let mut suppressed_high_frequency_chapter_hashes = 0_usize;
    for postings in chapter_hash_postings.values() {
        if postings.len() > config.max_hash_document_frequency {
            suppressed_high_frequency_chapter_hashes += 1;
            continue;
        }
        for_each_pair(postings, |pair| {
            let evidence = candidates.entry(pair).or_default();
            evidence.shared_chapters = evidence.shared_chapters.saturating_add(1);
        });
    }
    candidates.retain(|_, evidence| {
        evidence.exact_content || evidence.shared_chapters >= config.min_shared_chapters
    });
    let mut candidate_pairs: Vec<_> = candidates.keys().copied().collect();
    candidate_pairs.sort_unstable();
    let candidate_generation_ms = candidate_started.elapsed().as_millis();
    let rss_with_candidate_indexes_bytes = current_rss_bytes();
    drop(candidates);
    drop(chapter_hash_postings);
    drop(whole_hash_postings);

    let mut candidates_per_book = vec![0_usize; config.books];
    for &(book_a, book_b) in &candidate_pairs {
        candidates_per_book[book_a] = candidates_per_book[book_a].saturating_add(1);
        candidates_per_book[book_b] = candidates_per_book[book_b].saturating_add(1);
    }
    let average_candidates_per_book = ratio(candidate_pairs.len().saturating_mul(2), config.books);
    candidates_per_book.sort_unstable();
    let p95_candidates_per_book = percentile_95(&candidates_per_book);

    let validation_started = Instant::now();
    let thresholds = ClassificationThresholds::default();
    let mut verified_relation_counts = BTreeMap::<String, usize>::new();
    let mut expected_relation_correct = 0_usize;
    let mut false_candidate_pairs = 0_usize;
    for pair @ (book_a, book_b) in candidate_pairs.iter().copied() {
        let a = &fingerprints[book_a];
        let b = &fingerprints[book_b];
        let alignment = align_chapters(a, b);
        let relation = classify_pair(a, b, &alignment, &thresholds).relation;
        *verified_relation_counts
            .entry(relation.as_str().to_string())
            .or_default() += 1;
        match expected_relations.get(&pair) {
            Some(expected) if *expected == relation => expected_relation_correct += 1,
            Some(_) => {}
            None => false_candidate_pairs += 1,
        }
    }
    let validation_ms = validation_started.elapsed().as_millis();
    let rss_after_validation_bytes = current_rss_bytes();
    let found_expected = expected_relations
        .keys()
        .filter(|pair| candidate_pairs.binary_search(pair).is_ok())
        .count();

    Ok(SyntheticBenchmarkReport {
        synthetic_corpus_version: SYNTHETIC_CORPUS_VERSION,
        dedup_algorithm_version: DEDUP_ALGORITHM_VERSION,
        pipeline_scope: "in_process_fingerprint_chapter_hash_candidates_alignment_classification"
            .to_string(),
        config: config.clone(),
        books: fingerprints.len(),
        chapters: fingerprints
            .iter()
            .map(|fingerprint| fingerprint.chapters.len())
            .sum(),
        corpus_digest_sha256: hex::encode(corpus_hasher.finalize()),
        expected_related_pairs: expected_relations.len(),
        candidate_pairs: candidate_pairs.len(),
        false_candidate_pairs,
        candidate_recall: ratio(found_expected, expected_relations.len()),
        average_candidates_per_book,
        p95_candidates_per_book,
        suppressed_high_frequency_chapter_hashes,
        expected_relation_correct,
        verified_relation_counts,
        timings: BenchmarkTimings {
            fingerprint_ms,
            candidate_generation_ms,
            validation_ms,
            total_ms: total_started.elapsed().as_millis(),
        },
        memory: BenchmarkMemory {
            rss_start_bytes,
            rss_after_fingerprints_bytes,
            rss_with_candidate_indexes_bytes,
            rss_after_validation_bytes,
            peak_rss_bytes: proc_status_kib("VmHWM:").and_then(|kib| kib.checked_mul(1_024)),
            note: "RSS snapshots are process-wide; peak_rss_bytes is Linux /proc VmHWM or null"
                .to_string(),
        },
    })
}

struct SyntheticBook {
    fingerprint: BookFingerprint,
    expected_relation: Option<(usize, DuplicateRelation)>,
}

fn synthetic_book(
    book_index: usize,
    config: &SyntheticBenchmarkConfig,
) -> Result<SyntheticBook, SyntheticBenchmarkError> {
    let is_variant = book_index % config.related_pair_every == 1;
    let source_index = if is_variant {
        book_index.saturating_sub(1)
    } else {
        book_index
    };
    let variant_number = book_index / config.related_pair_every;
    let relation = is_variant.then_some(match variant_number % 3 {
        0 => DuplicateRelation::ExactContent,
        1 => DuplicateRelation::ContainedVersion,
        _ => DuplicateRelation::HighOverlap,
    });

    let mut contents = Vec::with_capacity(
        config
            .chapters_per_book
            .saturating_add(config.contained_added_chapters),
    );
    contents.push(shared_boilerplate(config.seed));
    for chapter_index in 1..config.chapters_per_book {
        contents.push(unique_chapter(config.seed, source_index, chapter_index));
    }
    if relation == Some(DuplicateRelation::ExactContent) {
        contents = contents
            .into_iter()
            .map(|content| conservative_variant(&content))
            .collect();
    } else if relation == Some(DuplicateRelation::HighOverlap) {
        contents = contents
            .into_iter()
            .map(|content| compatibility_layout_variant(&content))
            .collect();
    } else if relation == Some(DuplicateRelation::ContainedVersion) {
        for added in 0..config.contained_added_chapters {
            contents.push(unique_chapter(
                config.seed ^ 0xa5a5_a5a5_a5a5_a5a5,
                book_index,
                config.chapters_per_book.saturating_add(added),
            ));
        }
    }

    let mut inputs = Vec::with_capacity(contents.len());
    for (chapter_index, content) in contents.iter().enumerate() {
        inputs.push(ChapterInput {
            chapter_index: u32::try_from(chapter_index)
                .map_err(|_| SyntheticBenchmarkError::IdentifierOverflow)?,
            content,
        });
    }
    Ok(SyntheticBook {
        fingerprint: fingerprint_book(&inputs),
        expected_relation: relation.map(|value| (source_index, value)),
    })
}

fn shared_boilerplate(seed: u64) -> String {
    format!(
        "版权页与站点模板 seed={seed}。本段故意被所有合成书共享，用来验证高文档频率抑制。{}",
        "公共序言不应独自形成小说重复候选。".repeat(8)
    )
}

fn unique_chapter(seed: u64, source_book: usize, chapter: usize) -> String {
    let token = mixed_token(seed, source_book, chapter);
    format!(
        "第{chapter}章 合成正文。seed={seed}; source={source_book}; token={token:016x}。{}",
        format!("只属于作品{source_book}章节{chapter}的情节片段{token:016x}。").repeat(8)
    )
}

fn mixed_token(seed: u64, source_book: usize, chapter: usize) -> u64 {
    let source = u64::try_from(source_book).unwrap_or(u64::MAX);
    let chapter = u64::try_from(chapter).unwrap_or(u64::MAX);
    seed ^ source.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ chapter.wrapping_mul(0xbf58_476d_1ce4_e5b9)
}

fn conservative_variant(input: &str) -> String {
    let mut output = String::with_capacity(input.len().saturating_mul(2));
    output.push('\u{feff}');
    for character in input.chars() {
        if character == ' ' {
            output.push_str("\t  ");
        } else {
            output.push(character);
            output.push('\u{200b}');
        }
    }
    output.push_str("\r\n\t");
    output
}

fn compatibility_layout_variant(input: &str) -> String {
    let mut output = String::with_capacity(input.len().saturating_mul(2));
    for character in input.chars() {
        if character.is_ascii_alphanumeric() {
            let fullwidth =
                char::from_u32(u32::from(character).saturating_add(0xfee0)).unwrap_or(character);
            output.push(fullwidth);
        } else {
            output.push(character);
        }
    }
    output
}

fn for_each_pair(postings: &[usize], mut visit: impl FnMut((usize, usize))) {
    for (offset, &left) in postings.iter().enumerate() {
        for &right in &postings[offset.saturating_add(1)..] {
            visit((left.min(right), left.max(right)));
        }
    }
}

fn validate_config(config: &SyntheticBenchmarkConfig) -> Result<(), SyntheticBenchmarkError> {
    if config.books < 2 {
        return Err(SyntheticBenchmarkError::TooFewBooks);
    }
    if config.min_shared_chapters == 0 || config.chapters_per_book < config.min_shared_chapters {
        return Err(SyntheticBenchmarkError::TooFewChapters);
    }
    if config.related_pair_every < 2 {
        return Err(SyntheticBenchmarkError::InvalidRelatedPairStride);
    }
    if config.contained_added_chapters == 0 {
        return Err(SyntheticBenchmarkError::NoContainedGrowth);
    }
    if config.max_hash_document_frequency < 2 {
        return Err(SyntheticBenchmarkError::InvalidDocumentFrequency);
    }
    Ok(())
}

fn percentile_95(sorted_values: &[usize]) -> usize {
    if sorted_values.is_empty() {
        return 0;
    }
    let rank = sorted_values.len().saturating_mul(95).div_ceil(100);
    sorted_values[rank.saturating_sub(1)]
}

#[allow(clippy::cast_precision_loss)]
fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn current_rss_bytes() -> Option<u64> {
    if let Some(kib) = proc_status_kib("VmRSS:") {
        return kib.checked_mul(1_024);
    }

    let output = Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let kib = String::from_utf8(output.stdout)
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()?;
    kib.checked_mul(1_024)
}

fn proc_status_kib(field: &str) -> Option<u64> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    status.lines().find_map(|line| {
        line.strip_prefix(field)?
            .split_whitespace()
            .next()?
            .parse()
            .ok()
    })
}
