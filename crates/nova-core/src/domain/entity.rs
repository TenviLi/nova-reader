use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::Id;
use super::book::BookId;

/// Marker for Entity IDs (fictional characters, locations, etc.).
#[derive(Debug, Clone, Copy)]
pub struct EntityMarker;
pub type EntityId = Id<EntityMarker>;

/// A named entity extracted from novels via GraphRAG.
/// Characters, locations, organizations, artifacts, concepts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub id: EntityId,
    /// Primary name of the entity
    pub name: String,
    /// All known names/aliases for this entity
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Entity classification
    pub entity_type: EntityType,
    /// AI-generated or user-edited description
    pub description: Option<String>,
    /// Rich profile data (depends on entity_type)
    pub profile: EntityProfile,
    /// First book where this entity appeared
    pub source_book_id: Option<BookId>,
    /// Number of books this entity appears in
    pub appearance_count: i32,
    /// Number of times mentioned across all text
    pub mention_count: i64,
    /// Image/avatar path
    pub image_path: Option<String>,
    /// Importance score (computed from mention frequency + centrality)
    pub importance_score: f64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Type of extracted entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "entity_type", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    /// Character / person in the novel
    Character,
    /// Physical location / setting
    Location,
    /// Organization, faction, clan, sect (宗门)
    Organization,
    /// Item, weapon, artifact (法宝)
    Item,
    /// Skill, technique, power (功法)
    Skill,
    /// Event, battle, incident
    Event,
    /// Concept, power system, realm (境界)
    Concept,
}

/// Rich profile data for entities (type-dependent fields).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EntityProfile {
    // ─── Character fields ───
    /// Character gender
    pub gender: Option<String>,
    /// Character age (at last known point)
    pub age: Option<String>,
    /// Character title/rank (e.g., "斗帝", "仙帝")
    pub title: Option<String>,
    /// Affiliations (sects, families, organizations)
    #[serde(default)]
    pub affiliations: Vec<String>,
    /// Power level / cultivation realm
    pub power_level: Option<String>,
    /// Personality traits
    #[serde(default)]
    pub traits: Vec<String>,
    /// First appearance chapter
    pub first_appearance: Option<String>,
    /// Alive, deceased, unknown
    pub status: Option<String>,

    // ─── Location fields ───
    /// Geographic type (city, continent, realm, plane)
    pub location_type: Option<String>,
    /// Parent location (e.g., a city within a country)
    pub parent_location: Option<String>,

    // ─── Organization fields ───
    /// Organization type (sect, family, empire, guild)
    pub org_type: Option<String>,
    /// Leader/head of the organization
    pub leader: Option<String>,

    // ─── Concept / Power system fields ───
    /// Hierarchy/ranking system (e.g., cultivation stages)
    #[serde(default)]
    pub hierarchy: Vec<String>,

    // ─── Generic ───
    /// Free-form notes
    pub notes: Option<String>,
    /// Extra structured data
    #[serde(default)]
    pub extra: serde_json::Value,
}

/// Relationship between two entities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityRelationship {
    pub source_id: EntityId,
    pub target_id: EntityId,
    /// Type of relationship (e.g., "师徒", "父子", "敌对", "同门")
    pub relationship_type: String,
    /// Strength/weight of the relationship (0.0 - 1.0)
    pub weight: f64,
    /// Descriptive label
    pub description: Option<String>,
    /// Which book/chapter this was extracted from
    pub source_book_id: Option<BookId>,
    /// Chapter index where first observed
    pub first_chapter: Option<i32>,
}

/// A mention of an entity in the text (for highlighting).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityMention {
    pub entity_id: EntityId,
    pub book_id: BookId,
    pub chapter_index: i32,
    /// Character offset within chapter
    pub start_offset: i64,
    pub end_offset: i64,
    /// The exact text that matched
    pub surface_form: String,
}
