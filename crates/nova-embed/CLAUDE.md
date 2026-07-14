# nova-embed

Embedding generation service supporting local MLX models (Qwen3-Embedding-8B on Apple Silicon) and OpenAI-compatible APIs. Includes batch processing, intelligent text chunking, and MinHash LSH for near-duplicate detection.

## Architecture

```
src/
├── lib.rs       — Re-exports
├── client.rs    — EmbeddingService (OpenAI-compatible client)
├── chunker.rs   — TextChunker (sentence-boundary splitting)
└── dedup.rs     — DeduplicationEngine (MinHash LSH)
```

## Key Types

```rust
pub struct EmbeddingService {
    pub fn dimension(&self) -> usize;  // 1024 (Qwen3) or 1536 (OpenAI)
    pub async fn embed_one(&self, text: &str) -> Result<Vec<f32>>;
    pub async fn embed_many(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
}

pub struct TextChunker {
    pub fn chunk(text: &str, chunk_size: usize) -> Vec<String>;
}

pub struct DeduplicationEngine {
    pub fn add_document(&mut self, id: Uuid, embedding: &[f32]);
    pub fn find_duplicates(&self, embedding: &[f32], threshold: f32) -> Vec<Uuid>;
}
```

## Embedding Flow

1. Text → `TextChunker::chunk()` (512 tokens, sentence boundaries, 50-token overlap)
2. Chunks → `EmbeddingService::embed_many()` (batch of 32)
3. Embeddings → stored in PostgreSQL (linked to chapter)
4. Embeddings → indexed in Qdrant (for semantic search)
5. Optional: `DeduplicationEngine::find_duplicates()` for near-duplicate detection

## Key Patterns

- **OpenAI-compatible protocol**: Works with MLX server, LM Studio, Ollama, vLLM
- **Batch processing**: Configurable batch size (default 32) to avoid OOM
- **Sentence boundary preservation**: Chunks never split mid-sentence
- **Overlap**: 50-token overlap between chunks for semantic continuity
- **LSH deduplication**: MinHash with 128 hash functions, O(1) lookup
- **Graceful fallback**: Returns error with `retryable=true` if service unavailable

## Configuration

```env
EMBEDDING_MODEL=Qwen/Qwen3-Embedding-8B
EMBEDDING_ENDPOINT=http://localhost:8999
```

## Dependencies

- **Internal**: nova-core
- **External**: reqwest, serde, serde_json

## Build & Test

```bash
cargo build -p nova-embed
cargo test -p nova-embed
```

## Environment

Requires: Embedding server (MLX on localhost:8999 or compatible endpoint)
