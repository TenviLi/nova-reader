use serde::{Deserialize, Serialize};

use crate::Id;
use super::book::BookId;
use super::chapter::ChapterId;

/// A search query combining multiple strategies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    /// Natural language query text
    pub query: String,
    /// Filter by specific books
    #[serde(default)]
    pub book_ids: Vec<BookId>,
    /// Filter by language
    pub language: Option<super::book::Language>,
    /// Filter by genres
    #[serde(default)]
    pub genres: Vec<String>,
    /// Maximum results to return
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Offset for pagination
    #[serde(default)]
    pub offset: usize,
    /// Search mode
    #[serde(default)]
    pub mode: SearchMode,
}

fn default_limit() -> usize {
    20
}

/// Available search strategies.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchMode {
    /// Keyword-based full-text search (Meilisearch/PostgreSQL FTS)
    Keyword,
    /// Vector similarity search (Qdrant)
    Semantic,
    /// Combined keyword + semantic with RRF fusion
    #[default]
    Hybrid,
    /// Graph-based multi-hop reasoning (Neo4j GraphRAG)
    Graph,
}

/// A single search result with relevance scoring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub book_id: BookId,
    pub chapter_id: ChapterId,
    pub chunk_id: Option<Id<super::chapter::TextChunkMarker>>,
    pub book_title: String,
    pub chapter_title: Option<String>,
    pub content_snippet: String,
    /// Combined relevance score (0.0 - 1.0)
    pub score: f64,
    /// Which search strategy produced this result
    pub source: SearchMode,
    /// Highlighted matches in the snippet
    #[serde(default)]
    pub highlights: Vec<HighlightSpan>,
}

/// A highlighted span within search results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HighlightSpan {
    pub start: usize,
    pub end: usize,
}

/// Similar content discovery result (for creative writing assistance).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimilarContent {
    pub book_id: BookId,
    pub book_title: String,
    pub chapter_title: Option<String>,
    pub content_snippet: String,
    pub similarity_score: f64,
    /// What makes this similar (characters, plot, setting, etc.)
    pub similarity_aspects: Vec<String>,
}

/// A graph entity from the knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEntity {
    pub id: String,
    pub name: String,
    pub entity_type: EntityType,
    pub book_id: BookId,
    pub description: Option<String>,
    pub properties: serde_json::Value,
}

/// Types of entities in the knowledge graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    Character,
    Location,
    Organization,
    Event,
    Item,
    Concept,
    Technique,
    Timeline,
}

/// A relationship between two graph entities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphRelation {
    pub source: String,
    pub target: String,
    pub relation_type: String,
    pub chapter_id: Option<ChapterId>,
    pub weight: f64,
    pub description: Option<String>,
}
