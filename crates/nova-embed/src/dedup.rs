use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use crate::client::EmbeddingService;

/// Two-stage deduplication engine:
/// 1. MinHash LSH for fast syntactic near-duplicate detection (O(1))
/// 2. Semantic embedding similarity for cross-language dedup
pub struct DeduplicationEngine {
    embedding_service: EmbeddingService,
    /// Jaccard similarity threshold for LSH (0.85 = 85% similar)
    lsh_threshold: f64,
    /// Cosine similarity threshold for semantic dedup (0.95)
    semantic_threshold: f64,
    /// Number of hash functions for MinHash
    num_hashes: usize,
    /// Number of bands for LSH banding
    num_bands: usize,
}

/// Result of deduplication check.
#[derive(Debug, Clone)]
pub struct DedupResult {
    pub is_duplicate: bool,
    pub duplicate_of: Option<String>,
    pub similarity: f64,
    pub method: DedupMethod,
}

#[derive(Debug, Clone, Copy)]
pub enum DedupMethod {
    /// Detected via MinHash LSH (syntactic)
    MinHashLSH,
    /// Detected via embedding similarity (semantic)
    SemanticSimilarity,
    /// Not a duplicate
    None,
}

/// MinHash signature for a document.
#[derive(Debug, Clone)]
pub struct MinHashSignature {
    pub doc_id: String,
    pub signature: Vec<u64>,
}

/// Existing semantic embedding for a document being compared for duplicates.
#[derive(Debug, Clone)]
pub struct SemanticCandidate {
    pub doc_id: String,
    pub embedding: Vec<f32>,
}

impl DeduplicationEngine {
    pub fn new(embedding_service: EmbeddingService) -> Self {
        Self {
            embedding_service,
            lsh_threshold: 0.85,
            semantic_threshold: 0.95,
            num_hashes: 128,
            num_bands: 16,
        }
    }

    /// Generate MinHash signature for a document.
    /// Uses shingling (n-gram) to create a set representation.
    pub fn minhash_signature(&self, text: &str, shingle_size: usize) -> MinHashSignature {
        self.minhash_signature_for("", text, shingle_size)
    }

    /// Generate MinHash signature and attach the owning document ID.
    pub fn minhash_signature_for(
        &self,
        doc_id: impl Into<String>,
        text: &str,
        shingle_size: usize,
    ) -> MinHashSignature {
        // Generate shingles (character n-grams)
        let shingles = self.generate_shingles(text, shingle_size);

        // Compute MinHash signature
        let mut signature = vec![u64::MAX; self.num_hashes];

        for shingle in &shingles {
            for (i, sig_val) in signature.iter_mut().enumerate() {
                let hash = self.hash_with_seed(shingle, i as u64);
                if hash < *sig_val {
                    *sig_val = hash;
                }
            }
        }

        MinHashSignature {
            doc_id: doc_id.into(),
            signature,
        }
    }

    /// Estimate Jaccard similarity from two MinHash signatures.
    pub fn estimate_jaccard(sig_a: &MinHashSignature, sig_b: &MinHashSignature) -> f64 {
        assert_eq!(sig_a.signature.len(), sig_b.signature.len());

        let matches = sig_a
            .signature
            .iter()
            .zip(sig_b.signature.iter())
            .filter(|(a, b)| a == b)
            .count();

        matches as f64 / sig_a.signature.len() as f64
    }

    /// LSH banding technique: determine if two signatures should be
    /// considered candidate pairs (fall into the same bucket).
    pub fn lsh_candidate_pair(
        &self,
        sig_a: &MinHashSignature,
        sig_b: &MinHashSignature,
    ) -> bool {
        let rows_per_band = self.num_hashes / self.num_bands;

        for band in 0..self.num_bands {
            let start = band * rows_per_band;
            let end = start + rows_per_band;

            let band_a = &sig_a.signature[start..end];
            let band_b = &sig_b.signature[start..end];

            // If any band matches completely, they're a candidate pair
            if band_a == band_b {
                return true;
            }
        }

        false
    }

    /// Full deduplication check combining LSH and semantic analysis.
    pub async fn check_duplicate(
        &self,
        text: &str,
        doc_id: &str,
        existing_signatures: &[MinHashSignature],
    ) -> nova_core::Result<DedupResult> {
        // Stage 1: MinHash LSH (fast syntactic check)
        let new_sig = self.minhash_signature_for(doc_id, text, 5);

        if let Some(result) = self.find_lsh_duplicate(&new_sig, existing_signatures) {
            return Ok(result);
        }

        Ok(DedupResult::not_duplicate())
    }

    /// Full duplicate check with stored semantic embeddings as a second stage.
    pub async fn check_duplicate_with_semantic_candidates(
        &self,
        text: &str,
        doc_id: &str,
        existing_signatures: &[MinHashSignature],
        semantic_candidates: &[SemanticCandidate],
    ) -> nova_core::Result<DedupResult> {
        let new_sig = self.minhash_signature_for(doc_id, text, 5);

        if let Some(result) = self.find_lsh_duplicate(&new_sig, existing_signatures) {
            return Ok(result);
        }

        if semantic_candidates.is_empty() {
            return Ok(DedupResult::not_duplicate());
        }

        let query_embedding = self.embedding_service.embed_one(text).await?;
        let mut best_match: Option<(&str, f64)> = None;

        for candidate in semantic_candidates {
            if candidate.doc_id == doc_id || candidate.embedding.len() != query_embedding.len() {
                continue;
            }

            let similarity =
                EmbeddingService::cosine_similarity(&query_embedding, &candidate.embedding) as f64;

            if best_match.map_or(true, |(_, best)| similarity > best) {
                best_match = Some((candidate.doc_id.as_str(), similarity));
            }
        }

        if let Some((duplicate_of, similarity)) = best_match {
            if similarity >= self.semantic_threshold {
                return Ok(DedupResult {
                    is_duplicate: true,
                    duplicate_of: Some(duplicate_of.to_string()),
                    similarity,
                    method: DedupMethod::SemanticSimilarity,
                });
            }
        }

        Ok(DedupResult::not_duplicate())
    }

    fn find_lsh_duplicate(
        &self,
        new_sig: &MinHashSignature,
        existing_signatures: &[MinHashSignature],
    ) -> Option<DedupResult> {
        let lsh_index = self.build_lsh_index(existing_signatures);
        let mut checked_doc_ids = HashSet::new();

        for bucket in self.lsh_bucket_keys(new_sig) {
            let Some(candidates) = lsh_index.get(&bucket) else {
                continue;
            };

            for existing in candidates {
                if existing.doc_id == new_sig.doc_id
                    || !checked_doc_ids.insert(existing.doc_id.clone())
                {
                    continue;
                }

                let jaccard = Self::estimate_jaccard(new_sig, existing);
                if jaccard >= self.lsh_threshold {
                    return Some(DedupResult {
                        is_duplicate: true,
                        duplicate_of: Some(existing.doc_id.clone()),
                        similarity: jaccard,
                        method: DedupMethod::MinHashLSH,
                    });
                }
            }
        }

        None
    }

    fn build_lsh_index<'a>(
        &self,
        signatures: &'a [MinHashSignature],
    ) -> HashMap<u64, Vec<&'a MinHashSignature>> {
        let mut buckets: HashMap<u64, Vec<&MinHashSignature>> = HashMap::new();

        for signature in signatures {
            for bucket in self.lsh_bucket_keys(signature) {
                buckets.entry(bucket).or_default().push(signature);
            }
        }

        buckets
    }

    fn lsh_bucket_keys(&self, signature: &MinHashSignature) -> Vec<u64> {
        let rows_per_band = self.num_hashes / self.num_bands;
        if rows_per_band == 0 {
            return Vec::new();
        }

        (0..self.num_bands)
            .filter_map(|band| {
                let start = band * rows_per_band;
                let end = start + rows_per_band;
                signature
                    .signature
                    .get(start..end)
                    .map(|values| self.hash_band(band, values))
            })
            .collect()
    }

    fn hash_band(&self, band: usize, values: &[u64]) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        let mut hasher = DefaultHasher::new();
        band.hash(&mut hasher);
        values.hash(&mut hasher);
        hasher.finish()
    }

    /// Generate character n-grams (shingles) from text.
    fn generate_shingles(&self, text: &str, n: usize) -> Vec<String> {
        let chars: Vec<char> = text.chars().collect();
        if chars.len() < n {
            return vec![text.to_string()];
        }

        chars
            .windows(n)
            .map(|window| window.iter().collect::<String>())
            .collect()
    }

    /// Hash a string with a seed for MinHash.
    fn hash_with_seed(&self, s: &str, seed: u64) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        let mut hasher = DefaultHasher::new();
        seed.hash(&mut hasher);
        s.hash(&mut hasher);
        hasher.finish()
    }
}

impl DedupResult {
    fn not_duplicate() -> Self {
        Self {
            is_duplicate: false,
            duplicate_of: None,
            similarity: 0.0,
            method: DedupMethod::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minhash_similar_texts() {
        let service = EmbeddingService::new(
            "http://localhost:8999".to_string(),
            "test".to_string(),
        );
        let engine = DeduplicationEngine::new(service);

        let text1 = "这是一个测试文档，包含了一些内容用于验证MinHash算法";
        let text2 = "这是一个测试文档，包含了一些内容用于验证MinHash算法的正确性";

        let sig1 = engine.minhash_signature(text1, 3);
        let sig2 = engine.minhash_signature(text2, 3);

        let similarity = DeduplicationEngine::estimate_jaccard(&sig1, &sig2);
        // Very similar texts should have high Jaccard estimate
        assert!(similarity > 0.5, "Similarity was: {}", similarity);
    }

    #[test]
    fn test_minhash_different_texts() {
        let service = EmbeddingService::new(
            "http://localhost:8999".to_string(),
            "test".to_string(),
        );
        let engine = DeduplicationEngine::new(service);

        let text1 = "武侠小说中的江湖恩怨";
        let text2 = "现代都市中的商业竞争故事";

        let sig1 = engine.minhash_signature(text1, 3);
        let sig2 = engine.minhash_signature(text2, 3);

        let similarity = DeduplicationEngine::estimate_jaccard(&sig1, &sig2);
        // Different texts should have low similarity
        assert!(similarity < 0.5, "Similarity was: {}", similarity);
    }
}
