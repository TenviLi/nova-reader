//! PG → Neo4j synchronization.
//!
//! Reads entities and relationships from PostgreSQL and upserts them into Neo4j.

use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::neo4j::Neo4jClient;

/// Result of a sync operation.
#[derive(Debug, Serialize)]
pub struct SyncResult {
    pub entities_synced: usize,
    pub relationships_synced: usize,
    pub errors: Vec<String>,
}

/// Sync all entities and relationships for a book from PG to Neo4j.
pub async fn sync_book_to_neo4j(
    db: &PgPool,
    neo4j: &Neo4jClient,
    book_id: Uuid,
) -> nova_core::Result<SyncResult> {
    let mut result = SyncResult {
        entities_synced: 0,
        relationships_synced: 0,
        errors: Vec::new(),
    };

    let book_id_str = book_id.to_string();

    // 1. Fetch entities from PG
    let entities = sqlx::query_as::<_, EntityRow>(
        "SELECT name, entity_type, description, aliases, attributes \
         FROM entities WHERE book_id = $1"
    )
    .bind(book_id)
    .fetch_all(db)
    .await
    .map_err(|e| nova_core::Error::GraphDb(format!("PG query failed: {}", e)))?;

    tracing::info!("Syncing {} entities to Neo4j for book {}", entities.len(), book_id_str);

    // 2. Upsert entities to Neo4j
    for entity in &entities {
        let label = neo4j_label(&entity.entity_type);
        let props = serde_json::json!({
            "description": entity.description.as_deref().unwrap_or(""),
            "aliases": entity.aliases,
        });

        match neo4j.upsert_entity(&label, &entity.name, &book_id_str, &props).await {
            Ok(_) => result.entities_synced += 1,
            Err(e) => {
                let msg = format!("Failed to upsert entity '{}': {}", entity.name, e);
                tracing::warn!("{}", msg);
                result.errors.push(msg);
            }
        }
    }

    // 3. Fetch relationships from PG (join to get entity names)
    let relationships = sqlx::query_as::<_, RelationshipRow>(
        "SELECT s.name AS source_name, t.name AS target_name, \
                r.relation_type, r.description, r.weight \
         FROM entity_relationships r \
         JOIN entities s ON r.source_entity_id = s.id \
         JOIN entities t ON r.target_entity_id = t.id \
         WHERE s.book_id = $1"
    )
    .bind(book_id)
    .fetch_all(db)
    .await
    .map_err(|e| nova_core::Error::GraphDb(format!("PG query failed: {}", e)))?;

    tracing::info!("Syncing {} relationships to Neo4j for book {}", relationships.len(), book_id_str);

    // 4. Create relationships in Neo4j
    for rel in &relationships {
        let rel_label = neo4j_rel_label(&rel.relation_type);
        let props = serde_json::json!({
            "description": rel.description.as_deref().unwrap_or(""),
            "weight": rel.weight,
            "book_id": book_id_str,
        });

        match neo4j.create_relationship(&rel.source_name, &rel.target_name, &rel_label, &props).await {
            Ok(_) => result.relationships_synced += 1,
            Err(e) => {
                let msg = format!(
                    "Failed to create relationship '{}' -[{}]-> '{}': {}",
                    rel.source_name, rel.relation_type, rel.target_name, e
                );
                tracing::warn!("{}", msg);
                result.errors.push(msg);
            }
        }
    }

    tracing::info!(
        "Neo4j sync complete: {} entities, {} relationships ({} errors)",
        result.entities_synced,
        result.relationships_synced,
        result.errors.len()
    );

    Ok(result)
}

/// Convert entity_type to a Neo4j node label (PascalCase, alphanumeric only).
fn neo4j_label(entity_type: &str) -> String {
    match entity_type.to_lowercase().as_str() {
        "person" | "character" => "Character".to_string(),
        "location" | "place" => "Location".to_string(),
        "organization" | "org" | "faction" => "Organization".to_string(),
        "item" | "artifact" | "weapon" => "Item".to_string(),
        "event" => "Event".to_string(),
        "concept" | "technique" | "skill" => "Concept".to_string(),
        other => {
            // Capitalize first letter, keep rest
            let mut chars = other.chars();
            match chars.next() {
                None => "Entity".to_string(),
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
            }
        }
    }
}

/// Convert relation_type to a Neo4j relationship type (UPPER_SNAKE_CASE).
fn neo4j_rel_label(relation_type: &str) -> String {
    // Replace common separators with underscore and uppercase
    relation_type
        .replace([' ', '-', '.'], "_")
        .to_uppercase()
}

// ─── Row types for sqlx ──────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct EntityRow {
    name: String,
    entity_type: String,
    description: Option<String>,
    aliases: Vec<String>,
    #[allow(dead_code)]
    attributes: serde_json::Value,
}

#[derive(sqlx::FromRow)]
struct RelationshipRow {
    source_name: String,
    target_name: String,
    relation_type: String,
    description: Option<String>,
    weight: f64,
}

// ─── Ontology Tree Sync ──────────────────────────────────────────────────────

/// Result of syncing ontology tree to Neo4j.
#[derive(Debug, Serialize)]
pub struct OntologySyncResult {
    pub nodes_synced: usize,
    pub relationships_synced: usize,
    pub errors: Vec<String>,
}

/// Sync the trope ontology tree from PostgreSQL to Neo4j.
///
/// Creates TropeCategory nodes and PART_OF relationships for hierarchy.
/// Also creates HAS_TROPE relationships from books to trope categories.
pub async fn sync_ontology_to_neo4j(
    db: &PgPool,
    neo4j: &Neo4jClient,
) -> nova_core::Result<OntologySyncResult> {
    let mut result = OntologySyncResult {
        nodes_synced: 0,
        relationships_synced: 0,
        errors: Vec::new(),
    };

    // 1. Fetch all trope nodes from PG
    let nodes = sqlx::query_as::<_, TropeNodeRow>(
        "SELECT id, parent_id, label, description, level, cluster_size, stability, domain \
         FROM trope_nodes ORDER BY level ASC"
    )
    .fetch_all(db)
    .await
    .map_err(|e| nova_core::Error::GraphDb(format!("PG query failed: {e}")))?;

    tracing::info!("Syncing {} trope nodes to Neo4j", nodes.len());

    // 2. Upsert TropeCategory nodes
    for node in &nodes {
        let cypher = "\
            MERGE (t:TropeCategory {id: $id}) \
            SET t.label = $label, \
                t.description = $description, \
                t.level = $level, \
                t.cluster_size = $cluster_size, \
                t.stability = $stability, \
                t.domain = $domain";

        let params = serde_json::json!({
            "id": node.id.to_string(),
            "label": node.label,
            "description": node.description.as_deref().unwrap_or(""),
            "level": node.level,
            "cluster_size": node.cluster_size,
            "stability": node.stability,
            "domain": node.domain,
        });

        match neo4j.execute(cypher, Some(params)).await {
            Ok(_) => result.nodes_synced += 1,
            Err(e) => {
                result.errors.push(format!("Failed to sync node '{}': {}", node.label, e));
            }
        }
    }

    // 3. Create PART_OF relationships for hierarchy
    for node in &nodes {
        if let Some(parent_id) = &node.parent_id {
            let cypher = "\
                MATCH (child:TropeCategory {id: $child_id}) \
                MATCH (parent:TropeCategory {id: $parent_id}) \
                MERGE (child)-[:PART_OF]->(parent)";

            let params = serde_json::json!({
                "child_id": node.id.to_string(),
                "parent_id": parent_id.to_string(),
            });

            match neo4j.execute(cypher, Some(params)).await {
                Ok(_) => result.relationships_synced += 1,
                Err(e) => {
                    result.errors.push(format!("Failed to create PART_OF for '{}': {}", node.label, e));
                }
            }
        }
    }

    // 4. Create HAS_TROPE relationships from books
    let book_tropes = sqlx::query_as::<_, BookTropeRow>(
        "SELECT DISTINCT tca.book_id, tn.id AS trope_node_id, \
         AVG(tca.membership_score) AS avg_score \
         FROM trope_chunk_assignments tca \
         JOIN trope_nodes tn ON tca.trope_node_id = tn.id \
         GROUP BY tca.book_id, tn.id"
    )
    .fetch_all(db)
    .await
    .map_err(|e| nova_core::Error::GraphDb(format!("PG query failed: {e}")))?;

    for bt in &book_tropes {
        // Book nodes should already exist (created by entity sync)
        let cypher = "\
            MATCH (b) WHERE b.book_id = $book_id \
            MATCH (t:TropeCategory {id: $trope_id}) \
            MERGE (b)-[r:HAS_TROPE]->(t) \
            SET r.score = $score";

        let params = serde_json::json!({
            "book_id": bt.book_id.to_string(),
            "trope_id": bt.trope_node_id.to_string(),
            "score": bt.avg_score,
        });

        match neo4j.execute(cypher, Some(params)).await {
            Ok(_) => result.relationships_synced += 1,
            Err(e) => {
                result.errors.push(format!("Failed to create HAS_TROPE: {}", e));
            }
        }
    }

    tracing::info!(
        "Ontology sync complete: {} nodes, {} rels ({} errors)",
        result.nodes_synced,
        result.relationships_synced,
        result.errors.len()
    );

    Ok(result)
}

#[derive(sqlx::FromRow)]
struct TropeNodeRow {
    id: Uuid,
    parent_id: Option<Uuid>,
    label: String,
    description: Option<String>,
    level: i32,
    cluster_size: i32,
    stability: f64,
    domain: String,
}

#[derive(sqlx::FromRow)]
struct BookTropeRow {
    book_id: Uuid,
    trope_node_id: Uuid,
    avg_score: Option<f64>,
}
