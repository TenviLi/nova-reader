//! Deterministic, database-independent primitives for novel deduplication.

pub use nova_core::domain::dedup::*;
pub use nova_core::DedupError;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use unicode_normalization::UnicodeNormalization;

/// Version of the conservative NFC normalization rules.
pub const CONSERVATIVE_NORMALIZATION_VERSION: u16 = 1;

/// Version of the layout-insensitive NFKC normalization rules.
pub const LAYOUT_NORMALIZATION_VERSION: u16 = 1;

/// Version of alignment and classification behavior.
pub const DEDUP_ALGORITHM_VERSION: u16 = 5;

/// Hard ceiling for the sparse Hunt-Szymanski graph. Repeated chapter hashes
/// can otherwise expand as occurrences(A) * occurrences(B) before any scan
/// budget has a chance to reject the pair.
const MAX_ALIGNMENT_NODES: usize = 1_000_000;
const MAX_ALIGNMENT_CROSS_PRODUCT_PER_HASH: usize = 65_536;

/// Compute a SHA-256 digest without applying text normalization.
pub fn sha256(input: &[u8]) -> Sha256Hash {
    Sha256Hash::from_bytes(Sha256::digest(input).into())
}

/// Hash an ordered set of source chapters without normalization. Including
/// the stable chapter index and byte length prevents boundary ambiguity. Scan
/// cache validation and resolution freshness checks must share this exact
/// algorithm so their safety decisions cannot drift.
pub struct SourceContentHasher {
    hasher: Sha256,
}

impl SourceContentHasher {
    pub fn new() -> Self {
        Self {
            hasher: Sha256::new(),
        }
    }

    pub fn update(&mut self, chapter_index: i32, content: &str) {
        self.hasher.update(chapter_index.to_be_bytes());
        self.hasher.update((content.len() as u64).to_be_bytes());
        self.hasher.update(content.as_bytes());
    }

    pub fn finalize(self) -> Sha256Hash {
        Sha256Hash::from_bytes(self.hasher.finalize().into())
    }
}

impl Default for SourceContentHasher {
    fn default() -> Self {
        Self::new()
    }
}

pub fn source_content_hash<'a>(chapters: impl IntoIterator<Item = (i32, &'a str)>) -> Sha256Hash {
    let mut hasher = SourceContentHasher::new();
    for (chapter_index, content) in chapters {
        hasher.update(chapter_index, content);
    }
    hasher.finalize()
}

/// Fingerprint one chapter using both normalization profiles.
pub fn fingerprint_chapter(input: ChapterInput<'_>) -> ChapterFingerprint {
    let conservative = normalize_conservative(input.content);
    let layout = normalize_layout(input.content);

    ChapterFingerprint {
        chapter_index: input.chapter_index,
        conservative_hash: sha256(conservative.as_bytes()),
        layout_hash: sha256(layout.as_bytes()),
        char_count: count_chars(&layout),
        conservative_normalization_version: CONSERVATIVE_NORMALIZATION_VERSION,
        layout_normalization_version: LAYOUT_NORMALIZATION_VERSION,
    }
}

/// Fingerprint a book while keeping its chapter-level evidence.
///
/// Whole-book digests concatenate normalized content without chapter framing,
/// so splitting or merging chapters does not change the content fingerprint.
pub fn fingerprint_book(inputs: &[ChapterInput<'_>]) -> BookFingerprint {
    let mut conservative_hasher = Sha256::new();
    let mut layout_hasher = Sha256::new();
    let mut char_count = 0_u64;
    let mut chapters = Vec::with_capacity(inputs.len());

    for input in inputs {
        let conservative = normalize_conservative(input.content);
        let layout = normalize_layout(input.content);
        conservative_hasher.update(conservative.as_bytes());
        layout_hasher.update(layout.as_bytes());
        char_count = char_count.saturating_add(count_chars(&layout));
        chapters.push(ChapterFingerprint {
            chapter_index: input.chapter_index,
            conservative_hash: sha256(conservative.as_bytes()),
            layout_hash: sha256(layout.as_bytes()),
            char_count: count_chars(&layout),
            conservative_normalization_version: CONSERVATIVE_NORMALIZATION_VERSION,
            layout_normalization_version: LAYOUT_NORMALIZATION_VERSION,
        });
    }

    BookFingerprint {
        conservative_hash: Sha256Hash::from_bytes(conservative_hasher.finalize().into()),
        layout_hash: Sha256Hash::from_bytes(layout_hasher.finalize().into()),
        char_count,
        conservative_normalization_version: CONSERVATIVE_NORMALIZATION_VERSION,
        layout_normalization_version: LAYOUT_NORMALIZATION_VERSION,
        chapters,
    }
}

/// Align equal chapter content in order without constructing a dense LCS
/// matrix. Repeated chapters are matched at most once on each side.
pub fn align_chapters(a: &BookFingerprint, b: &BookFingerprint) -> ChapterAlignment {
    let a_positions = positions_by_layout_hash(&a.chapters);
    let b_positions = positions_by_layout_hash(&b.chapters);
    let candidates = bounded_alignment_candidates(&a_positions, &b_positions);
    let mut nodes = Vec::<AlignmentNode>::new();
    let mut tails = Vec::<usize>::new();

    // Candidates are ordered by A ascending and B descending. Descending B
    // positions prevent an increasing subsequence from using one A chapter
    // more than once.
    for (a_position, b_position) in candidates {
        let insertion =
            tails.partition_point(|node_index| nodes[*node_index].b_position < b_position);
        let previous = insertion
            .checked_sub(1)
            .and_then(|prior| tails.get(prior))
            .copied();
        let node_index = nodes.len();
        nodes.push(AlignmentNode {
            a_position,
            b_position,
            previous,
        });

        if insertion == tails.len() {
            tails.push(node_index);
        } else if let Some(tail) = tails.get_mut(insertion) {
            *tail = node_index;
        }
    }

    let aligned_positions = reconstruct_alignment(&nodes, tails.last().copied());
    alignment_evidence(a, b, &aligned_positions, &b_positions)
}

fn bounded_alignment_candidates(
    a_positions: &HashMap<(u16, Sha256Hash), Vec<usize>>,
    b_positions: &HashMap<(u16, Sha256Hash), Vec<usize>>,
) -> Vec<(usize, usize)> {
    let mut shared_keys: Vec<_> = a_positions
        .keys()
        .filter(|key| b_positions.contains_key(key))
        .copied()
        .collect();
    shared_keys.sort_unstable_by(|left, right| {
        let left_first = a_positions
            .get(left)
            .and_then(|positions| positions.first())
            .copied()
            .unwrap_or(usize::MAX);
        let right_first = a_positions
            .get(right)
            .and_then(|positions| positions.first())
            .copied()
            .unwrap_or(usize::MAX);
        left_first
            .cmp(&right_first)
            .then_with(|| left.0.cmp(&right.0))
            .then_with(|| left.1.as_bytes().cmp(right.1.as_bytes()))
    });

    // Every shared hash first receives prefix, suffix, and proportional rank
    // mappings. This preserves up to min(count_a, count_b) matches even when a
    // pathological repeated hash cannot receive its full Cartesian product.
    let mut candidates = Vec::new();
    for key in &shared_keys {
        let (Some(a), Some(b)) = (a_positions.get(key), b_positions.get(key)) else {
            continue;
        };
        append_rank_alignment_candidates(&mut candidates, a, b);
    }
    candidates.sort_unstable();
    candidates.dedup();
    if candidates.len() > MAX_ALIGNMENT_NODES {
        candidates = coverage_sample_pairs(&candidates, MAX_ALIGNMENT_NODES);
    }

    // Small/ordinary occurrence groups keep the full Hunt-Szymanski graph for
    // exact LCS behaviour. Extra alternatives are bounded both per hash and
    // globally; the rank mappings above remain available when the budget ends.
    for key in shared_keys {
        if candidates.len() >= MAX_ALIGNMENT_NODES {
            break;
        }
        let (Some(a), Some(b)) = (a_positions.get(&key), b_positions.get(&key)) else {
            continue;
        };
        let product = a.len().saturating_mul(b.len());
        if product > MAX_ALIGNMENT_CROSS_PRODUCT_PER_HASH {
            continue;
        }
        'positions: for &a_position in a {
            for &b_position in b {
                if candidates.len() >= MAX_ALIGNMENT_NODES {
                    break 'positions;
                }
                candidates.push((a_position, b_position));
            }
        }
    }

    candidates
        .sort_unstable_by(|left, right| left.0.cmp(&right.0).then_with(|| right.1.cmp(&left.1)));
    candidates.dedup();
    candidates
}

fn append_rank_alignment_candidates(
    candidates: &mut Vec<(usize, usize)>,
    a: &[usize],
    b: &[usize],
) {
    let shared = a.len().min(b.len());
    if shared == 0 {
        return;
    }
    for rank in 0..shared {
        candidates.push((a[rank], b[rank]));
        candidates.push((a[a.len() - shared + rank], b[b.len() - shared + rank]));
        candidates.push((
            a[quantile_index(a.len(), shared, rank)],
            b[quantile_index(b.len(), shared, rank)],
        ));
    }
}

fn quantile_index(length: usize, sample_count: usize, rank: usize) -> usize {
    if length == 0 || sample_count == 0 {
        return 0;
    }
    let numerator = (rank as u128)
        .saturating_mul(2)
        .saturating_add(1)
        .saturating_mul(length as u128);
    let denominator = (sample_count as u128).saturating_mul(2);
    usize::try_from(numerator / denominator)
        .unwrap_or(length - 1)
        .min(length - 1)
}

fn coverage_sample_pairs(pairs: &[(usize, usize)], limit: usize) -> Vec<(usize, usize)> {
    if pairs.len() <= limit {
        return pairs.to_vec();
    }
    (0..limit)
        .map(|rank| pairs[quantile_index(pairs.len(), limit, rank)])
        .collect()
}

/// Classify a pair from whole-content hashes and alignment evidence.
pub fn classify_pair(
    a: &BookFingerprint,
    b: &BookFingerprint,
    alignment: &ChapterAlignment,
    thresholds: &ClassificationThresholds,
) -> DuplicateClassification {
    let comparable_conservative_hashes = a.conservative_normalization_version
        == b.conservative_normalization_version
        && a.conservative_hash == b.conservative_hash;
    if comparable_conservative_hashes
        && a.char_count >= thresholds.exact_min_chars
        && b.char_count >= thresholds.exact_min_chars
    {
        return DuplicateClassification {
            relation: DuplicateRelation::ExactContent,
            contained: None,
        };
    }

    classify_metrics(
        &DuplicateMetrics {
            chapter_count_a: a.chapters.len(),
            chapter_count_b: b.chapters.len(),
            equivalent_chapters: alignment.shared_chapters,
            coverage_a: alignment.coverage_a.chapters,
            coverage_b: alignment.coverage_b.chapters,
            character_coverage_a: alignment.coverage_a.chars,
            character_coverage_b: alignment.coverage_b.chars,
            longest_run: alignment.longest_run,
            order_score: alignment.order_score,
            added_in_a: alignment.unique_in_a.len(),
            added_in_b: alignment.unique_in_b.len(),
            total_chars_a: a.char_count,
            total_chars_b: b.char_count,
            unique_chars_a: a
                .char_count
                .saturating_sub((alignment.coverage_a.chars * a.char_count as f64) as u64),
            unique_chars_b: b
                .char_count
                .saturating_sub((alignment.coverage_b.chars * b.char_count as f64) as u64),
            verified_passage_fingerprints: 0,
        },
        thresholds,
    )
}

/// Classify normalized overlap evidence. This is shared by exact chapter-hash
/// alignment and source-verified winnowing matches so threshold semantics do
/// not diverge between ingestion and API orchestration.
pub fn classify_metrics(
    metrics: &DuplicateMetrics,
    thresholds: &ClassificationThresholds,
) -> DuplicateClassification {
    if is_contained_side(
        metrics.chapter_count_a,
        metrics.equivalent_chapters,
        metrics.coverage_a,
        metrics.character_coverage_a,
        metrics.added_in_b,
        metrics.total_chars_a,
        metrics.total_chars_b,
        metrics.unique_chars_b,
        metrics.order_score,
        thresholds,
    ) {
        return DuplicateClassification {
            relation: DuplicateRelation::ContainedVersion,
            contained: Some(BookSide::A),
        };
    }
    if is_contained_side(
        metrics.chapter_count_b,
        metrics.equivalent_chapters,
        metrics.coverage_b,
        metrics.character_coverage_b,
        metrics.added_in_a,
        metrics.total_chars_b,
        metrics.total_chars_a,
        metrics.unique_chars_a,
        metrics.order_score,
        thresholds,
    ) {
        return DuplicateClassification {
            relation: DuplicateRelation::ContainedVersion,
            contained: Some(BookSide::B),
        };
    }

    let high_overlap = metrics.equivalent_chapters >= thresholds.high_overlap_min_shared_chapters
        && metrics.coverage_a >= thresholds.high_overlap_min_chapter_coverage
        && metrics.coverage_b >= thresholds.high_overlap_min_chapter_coverage
        && metrics.character_coverage_a >= thresholds.high_overlap_min_char_coverage
        && metrics.character_coverage_b >= thresholds.high_overlap_min_char_coverage
        && metrics.order_score >= thresholds.high_overlap_min_order_score;
    if high_overlap {
        return DuplicateClassification {
            relation: DuplicateRelation::HighOverlap,
            contained: None,
        };
    }

    if metrics.equivalent_chapters >= thresholds.partial_min_shared_chapters
        || metrics.longest_run >= thresholds.partial_min_longest_run
        || metrics.verified_passage_fingerprints
            >= thresholds.partial_min_verified_passage_fingerprints
    {
        return DuplicateClassification {
            relation: DuplicateRelation::PartialOverlap,
            contained: None,
        };
    }

    DuplicateClassification {
        relation: DuplicateRelation::NotDuplicate,
        contained: None,
    }
}

fn is_contained_side(
    candidate_chapters: usize,
    equivalent_chapters: usize,
    chapter_coverage: f64,
    character_coverage: f64,
    other_unique_chapters: usize,
    candidate_total_chars: u64,
    other_total_chars: u64,
    other_unique_chars: u64,
    order_score: f64,
    thresholds: &ClassificationThresholds,
) -> bool {
    let enough_shared = equivalent_chapters >= thresholds.contained_min_shared_chapters
        || (candidate_chapters < thresholds.contained_short_book_chapter_limit
            && equivalent_chapters >= thresholds.contained_short_book_min_shared_chapters);

    candidate_total_chars < other_total_chars
        && (other_unique_chapters >= thresholds.contained_min_added_chapters
            || other_unique_chars >= thresholds.contained_min_added_chars)
        && enough_shared
        && chapter_coverage >= thresholds.contained_min_chapter_coverage
        && character_coverage >= thresholds.contained_min_char_coverage
        && order_score >= thresholds.contained_min_order_score
}

#[derive(Debug, Clone, Copy)]
struct AlignmentNode {
    a_position: usize,
    b_position: usize,
    previous: Option<usize>,
}

fn positions_by_layout_hash(
    chapters: &[ChapterFingerprint],
) -> HashMap<(u16, Sha256Hash), Vec<usize>> {
    let mut positions = HashMap::<(u16, Sha256Hash), Vec<usize>>::new();
    for (position, chapter) in chapters.iter().enumerate() {
        positions
            .entry((chapter.layout_normalization_version, chapter.layout_hash))
            .or_default()
            .push(position);
    }
    positions
}

fn reconstruct_alignment(
    nodes: &[AlignmentNode],
    mut current: Option<usize>,
) -> Vec<(usize, usize)> {
    let mut positions = Vec::new();
    while let Some(node_index) = current {
        let Some(node) = nodes.get(node_index) else {
            break;
        };
        positions.push((node.a_position, node.b_position));
        current = node.previous;
    }
    positions.reverse();
    positions
}

fn alignment_evidence(
    a: &BookFingerprint,
    b: &BookFingerprint,
    positions: &[(usize, usize)],
    b_positions: &HashMap<(u16, Sha256Hash), Vec<usize>>,
) -> ChapterAlignment {
    let mut matched_a = vec![false; a.chapters.len()];
    let mut matched_b = vec![false; b.chapters.len()];
    let mut matched_chars_a = 0_u64;
    let mut matched_chars_b = 0_u64;
    let mut matches = Vec::with_capacity(positions.len());

    for &(a_position, b_position) in positions {
        let (Some(chapter_a), Some(chapter_b)) =
            (a.chapters.get(a_position), b.chapters.get(b_position))
        else {
            continue;
        };
        matched_a[a_position] = true;
        matched_b[b_position] = true;
        matched_chars_a = matched_chars_a.saturating_add(chapter_a.char_count);
        matched_chars_b = matched_chars_b.saturating_add(chapter_b.char_count);
        let conservative_match = chapter_a.conservative_normalization_version
            == chapter_b.conservative_normalization_version
            && chapter_a.conservative_hash == chapter_b.conservative_hash;
        matches.push(ChapterMatch {
            a_index: chapter_a.chapter_index,
            b_index: chapter_b.chapter_index,
            kind: if conservative_match {
                ChapterMatchKind::Conservative
            } else {
                ChapterMatchKind::Layout
            },
        });
    }

    let possible_matches = unordered_match_count(&a.chapters, b_positions);
    let unique_in_a = unmatched_chapter_indices(&a.chapters, &matched_a);
    let unique_in_b = unmatched_chapter_indices(&b.chapters, &matched_b);

    ChapterAlignment {
        shared_chapters: matches.len(),
        coverage_a: Coverage {
            chapters: usize_ratio(matches.len(), a.chapters.len()),
            chars: u64_ratio(matched_chars_a, a.char_count),
        },
        coverage_b: Coverage {
            chapters: usize_ratio(matches.len(), b.chapters.len()),
            chars: u64_ratio(matched_chars_b, b.char_count),
        },
        longest_run: longest_contiguous_run(positions),
        order_score: usize_ratio(matches.len(), possible_matches),
        matches,
        unique_in_a,
        unique_in_b,
    }
}

fn unordered_match_count(
    chapters: &[ChapterFingerprint],
    other_positions: &HashMap<(u16, Sha256Hash), Vec<usize>>,
) -> usize {
    let mut counts = HashMap::<(u16, Sha256Hash), usize>::new();
    for chapter in chapters {
        *counts
            .entry((chapter.layout_normalization_version, chapter.layout_hash))
            .or_default() += 1;
    }

    counts
        .into_iter()
        .map(|(key, count)| count.min(other_positions.get(&key).map_or(0, Vec::len)))
        .sum()
}

fn unmatched_chapter_indices(chapters: &[ChapterFingerprint], matched: &[bool]) -> Vec<u32> {
    chapters
        .iter()
        .zip(matched)
        .filter_map(|(chapter, is_matched)| (!is_matched).then_some(chapter.chapter_index))
        .collect()
}

fn longest_contiguous_run(positions: &[(usize, usize)]) -> usize {
    let Some(_) = positions.first() else {
        return 0;
    };
    let mut longest = 1;
    let mut current = 1;
    for pair in positions.windows(2) {
        if pair[1].0 == pair[0].0 + 1 && pair[1].1 == pair[0].1 + 1 {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 1;
        }
    }
    longest
}

#[allow(clippy::cast_precision_loss)]
fn usize_ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

#[allow(clippy::cast_precision_loss)]
fn u64_ratio(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn count_chars(input: &str) -> u64 {
    u64::try_from(input.chars().count()).unwrap_or(u64::MAX)
}

/// Normalize textual differences that do not change authored content.
///
/// This profile uses NFC, canonical line endings and horizontal whitespace,
/// and removes byte-order marks and zero-width spaces. Text and punctuation
/// are otherwise preserved.
pub fn normalize_conservative(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut pending_space = false;
    let mut pending_newline = false;

    for character in input
        .chars()
        .filter(|character| !is_ignored_format(*character))
        .nfc()
    {
        match character {
            '\r' | '\n' => {
                pending_space = false;
                pending_newline = !output.is_empty();
            }
            whitespace if whitespace.is_whitespace() => {
                if !pending_newline && !output.is_empty() {
                    pending_space = true;
                }
            }
            visible => {
                if pending_newline {
                    if !output.ends_with('\n') {
                        output.push('\n');
                    }
                } else if pending_space && !output.ends_with([' ', '\n']) {
                    output.push(' ');
                }

                output.push(visible);
                pending_space = false;
                pending_newline = false;
            }
        }
    }

    output
}

/// Normalize compatibility forms and remove whitespace used only for layout.
///
/// Punctuation and visible text remain part of the resulting content.
pub fn normalize_layout(input: &str) -> String {
    input
        .chars()
        .filter(|character| !is_ignored_format(*character))
        .nfkc()
        .filter(|character| !character.is_whitespace())
        .collect()
}

fn is_ignored_format(character: char) -> bool {
    matches!(character, '\u{feff}' | '\u{200b}')
}

/// Generate stable winnowing fingerprints over normalized Unicode scalars.
///
/// The hash is a wrapping polynomial over scalar values with base 257. For
/// equal minima, standard winnowing's rightmost-minimum rule is used.
pub fn winnow(
    input: &str,
    level: NormalizationLevel,
    config: WinnowingConfig,
) -> Result<Vec<WinnowingFingerprint>, DedupError> {
    config.validate()?;

    let normalized = match level {
        NormalizationLevel::Conservative => normalize_conservative(input),
        NormalizationLevel::Layout => normalize_layout(input),
    };
    let characters: Vec<char> = normalized.chars().collect();
    if characters.len() < config.gram_size {
        return Ok(Vec::new());
    }

    let gram_hashes = rolling_hashes(&characters, config.gram_size);
    if gram_hashes.len() < config.window_size {
        return Ok(Vec::new());
    }

    let mut candidates = VecDeque::<usize>::new();
    let mut selected = Vec::new();
    let mut last_selected = None;

    for (index, hash) in gram_hashes.iter().copied().enumerate() {
        while candidates
            .back()
            .is_some_and(|candidate| gram_hashes[*candidate] >= hash)
        {
            candidates.pop_back();
        }
        candidates.push_back(index);

        if index + 1 < config.window_size {
            continue;
        }

        let window_start = index + 1 - config.window_size;
        while candidates
            .front()
            .is_some_and(|candidate| *candidate < window_start)
        {
            candidates.pop_front();
        }

        if let Some(&minimum) = candidates.front() {
            if last_selected != Some(minimum) {
                selected.push(WinnowingFingerprint {
                    hash: gram_hashes[minimum],
                    position: minimum,
                });
                last_selected = Some(minimum);
            }
        }
    }

    Ok(selected)
}

fn rolling_hashes(characters: &[char], gram_size: usize) -> Vec<u64> {
    const BASE: u64 = 257;

    let mut leading_power = 1_u64;
    for _ in 1..gram_size {
        leading_power = leading_power.wrapping_mul(BASE);
    }

    let mut hash = 0_u64;
    for &character in &characters[..gram_size] {
        hash = hash
            .wrapping_mul(BASE)
            .wrapping_add(character_value(character));
    }

    let gram_count = characters.len() - gram_size + 1;
    let mut hashes = Vec::with_capacity(gram_count);
    hashes.push(hash);

    for position in 1..gram_count {
        let outgoing = character_value(characters[position - 1]);
        let incoming = character_value(characters[position + gram_size - 1]);
        hash = hash
            .wrapping_sub(outgoing.wrapping_mul(leading_power))
            .wrapping_mul(BASE)
            .wrapping_add(incoming);
        hashes.push(hash);
    }

    hashes
}

fn character_value(character: char) -> u64 {
    u64::from(u32::from(character)) + 1
}
