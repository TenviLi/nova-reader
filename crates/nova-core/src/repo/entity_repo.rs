use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::entity::*;
use crate::Result;
use super::book_repo::Paginated;

#[derive(Debug, Clone, Default)]
pub struct EntityFilter {
    pub book_id: Option<Uuid>,
    pub entity_type: Option<EntityType>,
    pub search: Option<String>,
    pub min_importance: Option<f64>,
    pub page: i64,
    pub per_page: i64,
}

#[async_trait]
pub trait EntityRepository: Send + Sync {
    /// List entities with filtering.
    async fn list(&self, filter: &EntityFilter) -> Result<Paginated<Entity>>;

    /// Get a single entity by ID.
    async fn get(&self, id: Uuid) -> Result<Entity>;

    /// Create or update an entity (upsert by name + type + source book).
    async fn upsert(&self, input: &UpsertEntity) -> Result<Entity>;

    /// Update entity profile.
    async fn update_profile(&self, id: Uuid, profile: &EntityProfile) -> Result<Entity>;

    /// Get relationships for an entity.
    async fn relationships(&self, entity_id: Uuid) -> Result<Vec<EntityRelationship>>;

    /// Create a relationship between entities.
    async fn create_relationship(&self, rel: &EntityRelationship) -> Result<()>;

    /// Get entity mentions in a chapter.
    async fn mentions_in_chapter(&self, book_id: Uuid, chapter_index: i32) -> Result<Vec<EntityMention>>;

    /// Batch insert mentions.
    async fn batch_insert_mentions(&self, mentions: &[EntityMention]) -> Result<()>;

    /// Merge two entity records.
    async fn merge(&self, keep_id: Uuid, remove_id: Uuid) -> Result<()>;

    /// Get the entity knowledge graph for a book (nodes + edges).
    async fn graph_for_book(&self, book_id: Uuid) -> Result<EntityGraph>;

    /// Get top entities (by importance) across all books.
    async fn top_entities(&self, limit: i64) -> Result<Vec<Entity>>;
}

/// Input for creating/updating an entity.
#[derive(Debug, Clone)]
pub struct UpsertEntity {
    pub name: String,
    pub aliases: Vec<String>,
    pub entity_type: EntityType,
    pub description: Option<String>,
    pub profile: EntityProfile,
    pub source_book_id: Option<Uuid>,
}

/// Graph representation for visualization.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EntityGraph {
    pub nodes: Vec<EntityNode>,
    pub edges: Vec<EntityEdge>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EntityNode {
    pub id: String,
    pub name: String,
    pub entity_type: EntityType,
    pub importance: f64,
    pub description: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EntityEdge {
    pub source: String,
    pub target: String,
    pub relationship_type: String,
    pub weight: f64,
}
