use nova_core::domain::search::{SearchMode, SearchQuery, SearchResult};

use crate::meilisearch::MeilisearchClient;
use crate::qdrant::QdrantClient;
use crate::rrf::reciprocal_rank_fusion;

/// Unified search engine that orchestrates multiple search backends.
pub struct HybridSearchEngine {
    meilisearch: MeilisearchClient,
    qdrant: QdrantClient,
}

impl HybridSearchEngine {
    pub fn new(meilisearch: MeilisearchClient, qdrant: QdrantClient) -> Self {
        Self {
            meilisearch,
            qdrant,
        }
    }

    /// Execute a search query using the specified mode.
    pub async fn search(&self, query: &SearchQuery) -> nova_core::Result<Vec<SearchResult>> {
        match query.mode {
            SearchMode::Keyword => self.keyword_search(query).await,
            SearchMode::Semantic => self.semantic_search(query).await,
            SearchMode::Hybrid => self.hybrid_search(query).await,
            SearchMode::Graph => self.graph_search(query).await,
        }
    }

    /// Pure keyword/full-text search.
    async fn keyword_search(&self, query: &SearchQuery) -> nova_core::Result<Vec<SearchResult>> {
        self.meilisearch.search(&query.query, query.limit).await
    }

    /// Pure semantic vector search.
    async fn semantic_search(&self, query: &SearchQuery) -> nova_core::Result<Vec<SearchResult>> {
        self.qdrant.search(&query.query, query.limit).await
    }

    /// Hybrid search with RRF fusion.
    /// Executes keyword and semantic search in parallel, then merges results.
    async fn hybrid_search(&self, query: &SearchQuery) -> nova_core::Result<Vec<SearchResult>> {
        let (keyword_results, semantic_results) = tokio::join!(
            self.keyword_search(query),
            self.semantic_search(query),
        );

        let keyword_results = keyword_results.unwrap_or_default();
        let semantic_results = semantic_results.unwrap_or_default();

        // Apply Reciprocal Rank Fusion
        let fused = reciprocal_rank_fusion(
            &[keyword_results, semantic_results],
            query.limit,
        );

        Ok(fused)
    }

    /// Graph-based search (placeholder for Neo4j integration).
    async fn graph_search(&self, query: &SearchQuery) -> nova_core::Result<Vec<SearchResult>> {
        Err(nova_core::Error::GraphDb(format!(
            "Graph search is not wired into nova-search yet; query length={}, limit={}",
            query.query.len(),
            query.limit
        )))
    }
}
