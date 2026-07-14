# nova-search

Hybrid search engine combining keyword search (Meilisearch), dense vector search (Qdrant), sparse BM25 (Qdrant), and graph-based reasoning (Neo4j). Uses Reciprocal Rank Fusion (RRF) to merge multi-source results.

## Architecture

```
src/
├── lib.rs          — Re-exports
├── hybrid.rs       — HybridSearchEngine (orchestrator)
├── meilisearch.rs  — MeilisearchClient (keyword/prefix search)
├── qdrant.rs       — QdrantClient (vector + sparse search)
├── rrf.rs          — Reciprocal Rank Fusion scoring
└── embedding.rs    — Embedding indexing helpers
```

## Key Types

```rust
pub struct HybridSearchEngine {
    pub async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>>;
}

// Modes:
// Keyword → Meilisearch only
// Semantic → Qdrant vector only
// Hybrid → Meilisearch + Qdrant + RRF fusion
// Graph → Neo4j entity reasoning (via nova-graph)
```

## RRF Algorithm

```
RRF(d) = Σ 1/(k + rank_i(d))  where k=60 (constant)
```

Merges keyword and semantic results with equal weighting. Documents appearing in both result sets get boosted scores.

## Key Patterns

- **Parallel execution**: `tokio::join!(keyword_search, vector_search)` for hybrid mode
- **Pluggable backends**: Each search engine is independently replaceable
- **Score normalization**: Results normalized to 0-1 range before fusion
- **Pagination**: `limit` + `offset` support for all modes
- **Fallback**: If one backend is down, returns partial results from available backends

## Dependencies

- **Internal**: nova-core
- **External**: reqwest (HTTP to Meilisearch/Qdrant), serde, serde_json

## Build & Test

```bash
cargo build -p nova-search
cargo test -p nova-search
```

## Environment

Requires: Meilisearch (port 7700), Qdrant (port 6333/6334)
