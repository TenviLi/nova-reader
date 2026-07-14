use nova_core::domain::chapter::ChunkingConfig;

/// Recursive text chunker that respects semantic boundaries.
/// Implements overlapping windows for RAG retrieval quality.
pub struct TextChunker {
    config: ChunkingConfig,
}

/// A single text chunk with metadata.
#[derive(Debug, Clone)]
pub struct Chunk {
    pub index: usize,
    pub content: String,
    pub token_estimate: usize,
    pub start_offset: usize,
    pub end_offset: usize,
}

impl TextChunker {
    pub fn new(config: ChunkingConfig) -> Self {
        Self { config }
    }

    /// Split text into overlapping chunks suitable for embedding.
    /// Uses recursive character splitting with paragraph-aware boundaries.
    pub fn chunk(&self, text: &str) -> Vec<Chunk> {
        let separators = ["\n\n", "\n", "。", ".", "！", "!", "？", "?", "；", ";", " "];
        self.recursive_split(text, &separators, 0)
    }

    fn recursive_split(
        &self,
        text: &str,
        separators: &[&str],
        base_offset: usize,
    ) -> Vec<Chunk> {
        if text.is_empty() {
            return Vec::new();
        }

        let token_est = estimate_tokens(text);
        if token_est <= self.config.chunk_size {
            return vec![Chunk {
                index: 0,
                content: text.to_string(),
                token_estimate: token_est,
                start_offset: base_offset,
                end_offset: base_offset + text.len(),
            }];
        }

        // Find the best separator for this text
        let separator = separators
            .iter()
            .find(|sep| text.contains(**sep))
            .copied()
            .unwrap_or(" ");

        let splits: Vec<&str> = text.split(separator).collect();
        let mut chunks = Vec::new();
        let mut current_chunk = String::new();
        let mut current_offset = base_offset;
        let mut chunk_start = base_offset;

        for (i, split) in splits.iter().enumerate() {
            let candidate = if current_chunk.is_empty() {
                split.to_string()
            } else {
                format!("{}{}{}", current_chunk, separator, split)
            };

            let candidate_tokens = estimate_tokens(&candidate);

            if candidate_tokens > self.config.chunk_size && !current_chunk.is_empty() {
                // Emit current chunk
                chunks.push(Chunk {
                    index: chunks.len(),
                    content: current_chunk.clone(),
                    token_estimate: estimate_tokens(&current_chunk),
                    start_offset: chunk_start,
                    end_offset: current_offset,
                });

                // Start new chunk with overlap
                let overlap_text = self.get_overlap_text(&current_chunk);
                current_chunk = format!("{}{}{}", overlap_text, separator, split);
                chunk_start = current_offset.saturating_sub(overlap_text.len());
            } else {
                current_chunk = candidate;
            }

            current_offset += split.len();
            if i + 1 < splits.len() {
                current_offset += separator.len();
            }
        }

        // Don't forget the last chunk
        if !current_chunk.is_empty() {
            let tokens = estimate_tokens(&current_chunk);
            if tokens >= self.config.min_chunk_size {
                chunks.push(Chunk {
                    index: chunks.len(),
                    content: current_chunk,
                    token_estimate: tokens,
                    start_offset: chunk_start,
                    end_offset: current_offset,
                });
            } else if let Some(last) = chunks.last_mut() {
                // Merge small trailing chunk into previous
                last.content.push_str(separator);
                last.content.push_str(&current_chunk);
                last.token_estimate = estimate_tokens(&last.content);
                last.end_offset = current_offset;
            }
        }

        chunks
    }

    /// Get the overlap portion from the end of a chunk.
    fn get_overlap_text(&self, text: &str) -> String {
        let target_tokens = self.config.overlap;
        let chars: Vec<char> = text.chars().collect();

        // Estimate: ~1.5 chars per token for Chinese, ~4 chars per token for English
        let estimated_chars = target_tokens * 2; // Conservative estimate
        let start = chars.len().saturating_sub(estimated_chars);

        chars[start..].iter().collect()
    }
}

/// Rough token count estimation.
/// CJK characters ≈ 1 token each, English words ≈ 1.3 tokens.
fn estimate_tokens(text: &str) -> usize {
    let mut tokens = 0usize;
    let mut in_word = false;

    for c in text.chars() {
        if c as u32 >= 0x4E00 {
            // CJK character ≈ 1 token
            tokens += 1;
            in_word = false;
        } else if c.is_alphanumeric() {
            if !in_word {
                tokens += 1;
                in_word = true;
            }
        } else {
            in_word = false;
        }
    }

    // Rough adjustment factor
    (tokens as f64 * 1.1) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunking_short_text() {
        let config = ChunkingConfig {
            chunk_size: 512,
            overlap: 64,
            min_chunk_size: 50,
        };
        let chunker = TextChunker::new(config);

        let text = "这是一段短文本。";
        let chunks = chunker.chunk(text);
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn test_chunking_long_text() {
        let config = ChunkingConfig {
            chunk_size: 100,
            overlap: 20,
            min_chunk_size: 30,
        };
        let chunker = TextChunker::new(config);

        // Generate a long text with clear paragraph boundaries
        let paragraphs: Vec<String> = (0..20)
            .map(|i| format!("这是第{}段文本，包含一些有意义的内容用于测试分块算法。", i + 1))
            .collect();
        let text = paragraphs.join("\n\n");

        let chunks = chunker.chunk(&text);
        assert!(chunks.len() > 1, "Should produce multiple chunks");

        // Verify no chunk exceeds target size by too much
        for chunk in &chunks {
            assert!(
                chunk.token_estimate <= 150, // Allow some slack
                "Chunk too large: {} tokens",
                chunk.token_estimate
            );
        }
    }
}
