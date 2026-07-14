#[cfg(test)]
mod cosine_tests {
    use crate::client::EmbeddingService;

    #[test]
    fn identical_vectors_similarity_is_one() {
        let a = vec![1.0, 2.0, 3.0];
        let sim = EmbeddingService::cosine_similarity(&a, &a);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn orthogonal_vectors_similarity_is_zero() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let sim = EmbeddingService::cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-6);
    }

    #[test]
    fn opposite_vectors_similarity_is_negative_one() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![-1.0, 0.0, 0.0];
        let sim = EmbeddingService::cosine_similarity(&a, &b);
        assert!((sim + 1.0).abs() < 1e-6);
    }

    #[test]
    fn scaled_vectors_same_direction() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![2.0, 4.0, 6.0]; // Same direction, different magnitude
        let sim = EmbeddingService::cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn zero_vector_returns_zero() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![1.0, 2.0, 3.0];
        let sim = EmbeddingService::cosine_similarity(&a, &b);
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn known_similarity_value() {
        // cos([1,0], [1,1]) = 1/sqrt(2) ≈ 0.7071
        let a = vec![1.0, 0.0];
        let b = vec![1.0, 1.0];
        let sim = EmbeddingService::cosine_similarity(&a, &b);
        let expected = 1.0_f32 / 2.0_f32.sqrt();
        assert!((sim - expected).abs() < 1e-5);
    }

    #[test]
    fn embedding_service_dimension() {
        let service = EmbeddingService::new(
            "http://localhost:8000".to_string(),
            "test-model".to_string(),
        );
        assert_eq!(service.dimension(), 768); // default for unknown model

        let qwen_service = EmbeddingService::new(
            "http://localhost:8000".to_string(),
            "Qwen3-Embedding-4B".to_string(),
        );
        assert_eq!(qwen_service.dimension(), 2560);

        let nomic_service = EmbeddingService::new(
            "http://localhost:8000".to_string(),
            "text-embedding-nomic-embed-text-v1.5".to_string(),
        );
        assert_eq!(nomic_service.dimension(), 768);

        let config_service = EmbeddingService::with_config(
            "https://ai.gitee.com".to_string(),
            "Qwen3-Embedding-4B".to_string(),
            Some("test-key".to_string()),
            2560,
        );
        assert_eq!(config_service.dimension(), 2560);
    }
}

#[cfg(test)]
mod chunker_extended_tests {
    use crate::chunker::{TextChunker, Chunk};
    use nova_core::domain::chapter::ChunkingConfig;

    #[test]
    fn empty_text_produces_no_chunks() {
        let chunker = TextChunker::new(ChunkingConfig::default());
        let chunks = chunker.chunk("");
        assert!(chunks.is_empty());
    }

    #[test]
    fn single_sentence_below_limit() {
        let chunker = TextChunker::new(ChunkingConfig {
            chunk_size: 1000,
            overlap: 50,
            min_chunk_size: 10,
        });
        let chunks = chunker.chunk("Hello world, this is a test.");
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].index, 0);
        assert!(chunks[0].content.contains("Hello world"));
    }

    #[test]
    fn chunks_maintain_order_indices() {
        let chunker = TextChunker::new(ChunkingConfig {
            chunk_size: 50,
            overlap: 10,
            min_chunk_size: 20,
        });
        let text = (0..30)
            .map(|i| format!("第{}段文字包含一些有意义的内容。", i + 1))
            .collect::<Vec<_>>()
            .join("\n\n");

        let chunks = chunker.chunk(&text);
        for (i, chunk) in chunks.iter().enumerate() {
            assert_eq!(chunk.index, i);
        }
    }

    #[test]
    fn chunks_cover_entire_text() {
        let chunker = TextChunker::new(ChunkingConfig {
            chunk_size: 80,
            overlap: 15,
            min_chunk_size: 20,
        });
        let text = "这是第一段。\n\n这是第二段。\n\n这是第三段。\n\n这是第四段。\n\n这是第五段。";
        let chunks = chunker.chunk(text);

        // Every paragraph should appear in at least one chunk
        for paragraph in ["第一段", "第二段", "第三段", "第四段", "第五段"] {
            let found = chunks.iter().any(|c| c.content.contains(paragraph));
            assert!(found, "Missing paragraph: {}", paragraph);
        }
    }

    #[test]
    fn offset_tracking_is_valid() {
        let chunker = TextChunker::new(ChunkingConfig {
            chunk_size: 100,
            overlap: 20,
            min_chunk_size: 30,
        });
        let text = (0..20)
            .map(|i| format!("段落{}：这是一些用于测试偏移量追踪的文字内容。", i + 1))
            .collect::<Vec<_>>()
            .join("\n\n");

        let chunks = chunker.chunk(&text);
        for chunk in &chunks {
            assert!(chunk.start_offset <= chunk.end_offset);
        }
    }

    #[test]
    fn token_estimate_reasonable_for_chinese() {
        let chunker = TextChunker::new(ChunkingConfig {
            chunk_size: 512,
            overlap: 64,
            min_chunk_size: 50,
        });
        // 100 Chinese characters ≈ 100 tokens (each CJK char ≈ 1 token)
        let text: String = "测试".repeat(50); // 100 chars
        let chunks = chunker.chunk(&text);
        assert_eq!(chunks.len(), 1);
        // Token estimate should be around 100 (±20%)
        let est = chunks[0].token_estimate;
        assert!(est > 80 && est < 130, "Token estimate was: {}", est);
    }

    #[test]
    fn token_estimate_reasonable_for_english() {
        let chunker = TextChunker::new(ChunkingConfig {
            chunk_size: 512,
            overlap: 64,
            min_chunk_size: 50,
        });
        // ~50 English words
        let text = "The quick brown fox jumps over the lazy dog. ".repeat(6);
        let chunks = chunker.chunk(&text);
        assert_eq!(chunks.len(), 1);
        // 54 words × 1.1 ≈ 59 tokens
        let est = chunks[0].token_estimate;
        assert!(est > 30 && est < 100, "Token estimate was: {}", est);
    }
}

#[cfg(test)]
mod dedup_extended_tests {
    use crate::client::EmbeddingService;
    use crate::dedup::{DeduplicationEngine, MinHashSignature};

    fn make_engine() -> DeduplicationEngine {
        let service = EmbeddingService::new(
            "http://localhost:8999".to_string(),
            "test".to_string(),
        );
        DeduplicationEngine::new(service)
    }

    #[test]
    fn identical_texts_have_jaccard_one() {
        let engine = make_engine();
        let text = "完全相同的文本内容用于测试";
        let sig1 = engine.minhash_signature(text, 3);
        let sig2 = engine.minhash_signature(text, 3);
        let sim = DeduplicationEngine::estimate_jaccard(&sig1, &sig2);
        assert!((sim - 1.0).abs() < 1e-10);
    }

    #[test]
    fn empty_text_produces_signature() {
        let engine = make_engine();
        let sig = engine.minhash_signature("", 3);
        assert_eq!(sig.signature.len(), 128); // num_hashes default
    }

    #[test]
    fn short_text_below_shingle_size() {
        let engine = make_engine();
        let sig = engine.minhash_signature("ab", 5);
        // Should still produce a valid signature
        assert_eq!(sig.signature.len(), 128);
    }

    #[test]
    fn lsh_candidate_pair_identical() {
        let engine = make_engine();
        let text = "相同文本应该是候选对";
        let sig1 = engine.minhash_signature(text, 3);
        let sig2 = engine.minhash_signature(text, 3);
        assert!(engine.lsh_candidate_pair(&sig1, &sig2));
    }

    #[test]
    fn lsh_candidate_pair_very_different() {
        let engine = make_engine();
        let sig1 = engine.minhash_signature("武侠小说中的江湖恩怨情仇主角历经磨难终成大侠", 3);
        let sig2 = engine.minhash_signature("现代科幻太空歌剧星际帝国陨落量子纠缠超光速引擎", 3);
        // Very different texts are unlikely to be candidate pairs
        // (but LSH has false positives, so we can't assert false definitively)
        let jaccard = DeduplicationEngine::estimate_jaccard(&sig1, &sig2);
        assert!(jaccard < 0.5);
    }

    #[test]
    fn signature_length_matches_num_hashes() {
        let engine = make_engine();
        let sig = engine.minhash_signature("test text", 3);
        assert_eq!(sig.signature.len(), 128);
    }

    #[test]
    fn different_shingle_sizes_produce_different_signatures() {
        let engine = make_engine();
        let text = "This is a longer text for testing shingle size differences";
        let sig3 = engine.minhash_signature(text, 3);
        let sig5 = engine.minhash_signature(text, 5);
        // Different shingle sizes should generally produce different signatures
        assert_ne!(sig3.signature, sig5.signature);
    }
}
