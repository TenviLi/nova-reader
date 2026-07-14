use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;

use crate::{
    access::{ensure_book_access, visible_library_ids, LibraryAccess},
    error::{ApiError, ApiResult},
    extractors::AuthUser,
    state::AppState,
};

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/entities", get(list_entities))
        .route("/entities/{id}", get(get_entity).delete(delete_entity))
        .route("/entities/graph", get(get_entity_graph))
        .route("/entities/{id}/mentions", get(get_entity_mentions))
        .route(
            "/entities/{id}/relationships",
            get(get_entity_relationships),
        )
        .route("/entities/{id}/relations", get(get_entity_relationships))
        .route("/entities/{id}/profile", get(get_entity_profile))
        .route(
            "/entities/{id}/profile/generate",
            axum::routing::post(generate_entity_profile),
        )
        .route("/entities/{id}/timeline", get(get_entity_timeline))
        .route(
            "/entities/sync-graph/{book_id}",
            axum::routing::post(sync_graph),
        )
        .route(
            "/entities/communities/{book_id}",
            axum::routing::post(detect_communities_endpoint),
        )
        .route("/entities/communities/{book_id}", get(get_communities))
        .route(
            "/entities/communities/{book_id}/summarize",
            axum::routing::post(summarize_communities),
        )
        .route(
            "/entities/disambiguate/{book_id}",
            axum::routing::post(disambiguate_entities),
        )
}

#[derive(Deserialize)]
struct ListEntitiesQuery {
    #[serde(rename = "type")]
    entity_type: Option<String>,
    book_id: Option<Uuid>,
    library_id: Option<Uuid>,
    series_id: Option<Uuid>,
    search: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(sqlx::FromRow, Serialize)]
struct EntityRow {
    id: Uuid,
    name: String,
    entity_type: String,
    description: Option<String>,
    aliases: Vec<String>,
    mention_count: i32,
    importance_score: f64,
    created_at: chrono::DateTime<chrono::Utc>,
}

async fn ensure_entity_book_read_access(
    state: &AppState,
    auth: &AuthUser,
    book_id: Uuid,
) -> ApiResult<()> {
    ensure_book_access(state, auth, book_id, LibraryAccess::Read).await
}

async fn ensure_entity_book_write_access(
    state: &AppState,
    auth: &AuthUser,
    book_id: Uuid,
) -> ApiResult<()> {
    ensure_book_access(state, auth, book_id, LibraryAccess::Write).await
}

async fn entity_book_id(state: &AppState, entity_id: Uuid) -> ApiResult<Uuid> {
    sqlx::query_scalar("SELECT book_id FROM entities WHERE id = $1")
        .bind(entity_id)
        .fetch_optional(&state.db)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound(format!("Entity {} not found", entity_id)))
}

async fn ensure_entity_access(
    state: &AppState,
    auth: &AuthUser,
    entity_id: Uuid,
    access: LibraryAccess,
) -> ApiResult<Uuid> {
    let book_id = entity_book_id(state, entity_id).await?;
    ensure_book_access(state, auth, book_id, access).await?;
    Ok(book_id)
}

async fn visible_entity_library_ids(
    state: &AppState,
    auth: &AuthUser,
) -> ApiResult<Option<Vec<Uuid>>> {
    visible_library_ids(state, auth, LibraryAccess::Read).await
}

fn add_entity_list_filters<'a>(
    query_builder: &mut QueryBuilder<'a, Postgres>,
    query: &'a ListEntitiesQuery,
    visible_ids: Option<&'a [Uuid]>,
) {
    query_builder.push(" WHERE TRUE");

    if let Some(entity_type) = query.entity_type.as_deref() {
        query_builder
            .push(" AND e.entity_type = ")
            .push_bind(entity_type);
    }
    if let Some(book_id) = query.book_id {
        query_builder.push(" AND e.book_id = ").push_bind(book_id);
    }
    if let Some(library_id) = query.library_id {
        query_builder
            .push(" AND b.library_id = ")
            .push_bind(library_id);
    }
    if let Some(series_id) = query.series_id {
        query_builder
            .push(" AND b.series_id = ")
            .push_bind(series_id);
    }
    if let Some(search) = query.search.as_deref().filter(|s| !s.is_empty()) {
        query_builder
            .push(" AND e.name ILIKE '%' || ")
            .push_bind(search)
            .push(" || '%'");
    }
    if let Some(ids) = visible_ids {
        query_builder
            .push(" AND b.library_id = ANY(")
            .push_bind(ids)
            .push("::uuid[])");
    }
}

async fn list_entities(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(query): Query<ListEntitiesQuery>,
) -> Result<Json<Vec<EntityRow>>, ApiError> {
    let limit = query.limit.unwrap_or(50).min(200);
    let offset = query.offset.unwrap_or(0);
    if let Some(book_id) = query.book_id {
        ensure_entity_book_read_access(&state, &auth, book_id).await?;
    }

    let visible_libraries = visible_entity_library_ids(&state, &auth).await?;
    if visible_libraries
        .as_ref()
        .is_some_and(|library_ids| library_ids.is_empty())
    {
        return Ok(Json(Vec::new()));
    }

    let mut query_builder = QueryBuilder::<Postgres>::new(
        "SELECT e.id, e.name, e.entity_type, e.description, e.aliases, \
         e.mention_count, e.importance_score, e.created_at \
         FROM entities e JOIN books b ON b.id = e.book_id",
    );
    add_entity_list_filters(&mut query_builder, &query, visible_libraries.as_deref());
    query_builder
        .push(" ORDER BY e.mention_count DESC LIMIT ")
        .push_bind(limit)
        .push(" OFFSET ")
        .push_bind(offset);

    let rows = query_builder
        .build_query_as::<EntityRow>()
        .fetch_all(&state.db)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(rows))
}

async fn get_entity(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<EntityRow>, ApiError> {
    ensure_entity_access(&state, &auth, id, LibraryAccess::Read).await?;
    let row = sqlx::query_as::<_, EntityRow>(
        "SELECT id, name, entity_type, description, aliases, mention_count, importance_score, created_at FROM entities WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::from)?
    .ok_or_else(|| ApiError::NotFound(format!("Entity {} not found", id)))?;

    Ok(Json(row))
}

async fn delete_entity(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<(), ApiError> {
    ensure_entity_access(&state, &auth, id, LibraryAccess::Write).await?;
    sqlx::query("DELETE FROM entities WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(ApiError::from)?;
    Ok(())
}

#[derive(Deserialize)]
struct GraphQuery {
    book_id: Option<Uuid>,
    library_id: Option<Uuid>,
    series_id: Option<Uuid>,
    limit: Option<i64>,
}

fn add_entity_graph_filters<'a>(
    query_builder: &mut QueryBuilder<'a, Postgres>,
    query: &'a GraphQuery,
    visible_ids: Option<&'a [Uuid]>,
) {
    query_builder.push(" WHERE TRUE");

    if let Some(book_id) = query.book_id {
        query_builder.push(" AND e.book_id = ").push_bind(book_id);
    }
    if let Some(library_id) = query.library_id {
        query_builder
            .push(" AND b.library_id = ")
            .push_bind(library_id);
    }
    if let Some(series_id) = query.series_id {
        query_builder
            .push(" AND b.series_id = ")
            .push_bind(series_id);
    }
    if let Some(ids) = visible_ids {
        query_builder
            .push(" AND b.library_id = ANY(")
            .push_bind(ids)
            .push("::uuid[])");
    }
}

#[derive(Serialize)]
struct GraphResponse {
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
}

#[derive(Serialize)]
struct GraphNode {
    id: String,
    label: String,
    #[serde(rename = "type")]
    node_type: String,
    size: f64,
}

#[derive(Serialize)]
struct GraphEdge {
    source: String,
    target: String,
    label: String,
    weight: f64,
}

async fn get_entity_graph(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(query): Query<GraphQuery>,
) -> Result<Json<GraphResponse>, ApiError> {
    let limit = query.limit.unwrap_or(100).min(500);
    if let Some(book_id) = query.book_id {
        ensure_entity_book_read_access(&state, &auth, book_id).await?;
    }

    let visible_libraries = visible_entity_library_ids(&state, &auth).await?;
    if visible_libraries
        .as_ref()
        .is_some_and(|library_ids| library_ids.is_empty())
    {
        return Ok(Json(GraphResponse {
            nodes: Vec::new(),
            edges: Vec::new(),
        }));
    }

    #[derive(sqlx::FromRow)]
    struct NodeRow {
        id: Uuid,
        name: String,
        entity_type: String,
        mention_count: i32,
    }

    let mut query_builder = QueryBuilder::<Postgres>::new(
        "SELECT e.id, e.name, e.entity_type, e.mention_count \
         FROM entities e JOIN books b ON b.id = e.book_id",
    );
    add_entity_graph_filters(&mut query_builder, &query, visible_libraries.as_deref());
    query_builder
        .push(" ORDER BY e.mention_count DESC LIMIT ")
        .push_bind(limit);

    let entities = query_builder
        .build_query_as::<NodeRow>()
        .fetch_all(&state.db)
        .await
        .map_err(ApiError::from)?;

    let entity_ids: Vec<Uuid> = entities.iter().map(|e| e.id).collect();

    let nodes: Vec<GraphNode> = entities
        .iter()
        .map(|e| GraphNode {
            id: e.id.to_string(),
            label: e.name.clone(),
            node_type: e.entity_type.clone(),
            size: (e.mention_count as f64).sqrt() * 5.0,
        })
        .collect();

    #[derive(sqlx::FromRow)]
    struct EdgeRow {
        source_entity_id: Uuid,
        target_entity_id: Uuid,
        relationship_type: Option<String>,
        weight: f64,
    }

    let relationships = sqlx::query_as::<_, EdgeRow>(
        r#"
        SELECT source_entity_id, target_entity_id, relationship_type, weight
        FROM entity_relationships
        WHERE source_entity_id = ANY($1) AND target_entity_id = ANY($1)
        "#,
    )
    .bind(&entity_ids)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;

    let edges: Vec<GraphEdge> = relationships
        .iter()
        .map(|r| GraphEdge {
            source: r.source_entity_id.to_string(),
            target: r.target_entity_id.to_string(),
            label: r.relationship_type.clone().unwrap_or_default(),
            weight: r.weight,
        })
        .collect();

    Ok(Json(GraphResponse { nodes, edges }))
}

async fn get_entity_mentions(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    let book_id = ensure_entity_access(&state, &auth, id, LibraryAccess::Read).await?;
    #[derive(sqlx::FromRow)]
    struct MentionRow {
        id: Uuid,
        book_id: Option<Uuid>,
        book_title: Option<String>,
        chapter_index: Option<i32>,
        context_snippet: Option<String>,
        position_start: Option<i64>,
        position_end: Option<i64>,
    }

    let mentions = sqlx::query_as::<_, MentionRow>(
        r#"
        SELECT em.id, c.book_id AS book_id, b.title as book_title,
               COALESCE(em.chapter_index, c.chapter_index) AS chapter_index,
               em.context_snippet, em.position_start, em.position_end
        FROM entity_mentions em
        JOIN chapters c ON c.id = em.chapter_id AND c.book_id = $2
        LEFT JOIN books b ON b.id = c.book_id
        WHERE em.entity_id = $1
        ORDER BY COALESCE(em.chapter_index, c.chapter_index)
        LIMIT 100
        "#,
    )
    .bind(id)
    .bind(book_id)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;

    let result: Vec<serde_json::Value> = mentions
        .into_iter()
        .map(|m| {
            serde_json::json!({
                "id": m.id,
                "book_id": m.book_id,
                "book_title": m.book_title,
                "chapter_index": m.chapter_index,
                "context_snippet": m.context_snippet,
                "position_start": m.position_start,
                "position_end": m.position_end,
            })
        })
        .collect();

    Ok(Json(result))
}

async fn get_entity_relationships(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    let book_id = ensure_entity_access(&state, &auth, id, LibraryAccess::Read).await?;
    #[derive(sqlx::FromRow)]
    struct RelRow {
        id: Uuid,
        relationship_type: Option<String>,
        weight: f64,
        description: Option<String>,
        related_id: Uuid,
        related_name: String,
        related_type: String,
        direction: String,
    }

    let relationships = sqlx::query_as::<_, RelRow>(
        r#"
        SELECT er.id, er.relationship_type, er.weight, er.description,
               CASE WHEN er.source_entity_id = $1 THEN er.target_entity_id ELSE er.source_entity_id END as related_id,
               CASE WHEN er.source_entity_id = $1 THEN te.name ELSE se.name END as related_name,
               CASE WHEN er.source_entity_id = $1 THEN te.entity_type ELSE se.entity_type END as related_type,
               CASE WHEN er.source_entity_id = $1 THEN 'outgoing' ELSE 'incoming' END as direction
        FROM entity_relationships er
        JOIN entities se ON se.id = er.source_entity_id
        JOIN entities te ON te.id = er.target_entity_id
        WHERE (er.source_entity_id = $1 OR er.target_entity_id = $1)
          AND se.book_id = $2 AND te.book_id = $2
        ORDER BY er.weight DESC
        "#,
    )
    .bind(id)
    .bind(book_id)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;

    let result: Vec<serde_json::Value> = relationships
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "relationship_type": r.relationship_type,
                "weight": r.weight,
                "description": r.description,
                "related_entity_id": r.related_id,
                "related_name": r.related_name,
                "related_type": r.related_type,
                "direction": r.direction,
            })
        })
        .collect();

    Ok(Json(result))
}

#[derive(Serialize)]
struct EntityProfileResponse {
    entity_id: Uuid,
    name: String,
    entity_type: String,
    appearance: Option<String>,
    personality: Option<String>,
    background: Option<String>,
    abilities: Option<String>,
    motivation: Option<String>,
    arc_summary: Option<String>,
    attributes: serde_json::Value,
    timeline: serde_json::Value,
    confidence_score: f64,
    last_updated_by: String,
}

#[derive(sqlx::FromRow)]
struct EntityProfileRow {
    appearance: Option<String>,
    personality: Option<String>,
    background: Option<String>,
    abilities: Option<String>,
    motivation: Option<String>,
    arc_summary: Option<String>,
    attributes: Option<serde_json::Value>,
    timeline: Option<serde_json::Value>,
    confidence_score: Option<f64>,
    last_updated_by: Option<String>,
}

fn empty_entity_profile(entity_id: Uuid, name: String, entity_type: String) -> EntityProfileResponse {
    EntityProfileResponse {
        entity_id,
        name,
        entity_type,
        appearance: None,
        personality: None,
        background: None,
        abilities: None,
        motivation: None,
        arc_summary: None,
        attributes: serde_json::json!({}),
        timeline: serde_json::json!([]),
        confidence_score: 0.0,
        last_updated_by: "none".to_string(),
    }
}

/// Get entity profile (rich AI-generated card when available).
async fn get_entity_profile(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<EntityProfileResponse>, ApiError> {
    ensure_entity_access(&state, &auth, id, LibraryAccess::Read).await?;

    let entity: (String, String) =
        sqlx::query_as("SELECT name, entity_type FROM entities WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await
            .map_err(ApiError::from)?
            .ok_or_else(|| ApiError::NotFound(format!("Entity {} not found", id)))?;

    let profile = sqlx::query_as::<_, EntityProfileRow>(
        r#"SELECT appearance, personality, background, abilities, motivation,
                  arc_summary, attributes, timeline, confidence_score, last_updated_by
           FROM entity_profiles WHERE entity_id = $1"#
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::from)?;

    match profile {
        Some(p) => Ok(Json(EntityProfileResponse {
            entity_id: id,
            name: entity.0,
            entity_type: entity.1,
            appearance: p.appearance,
            personality: p.personality,
            background: p.background,
            abilities: p.abilities,
            motivation: p.motivation,
            arc_summary: p.arc_summary,
            attributes: p.attributes.unwrap_or_else(|| serde_json::json!({})),
            timeline: p.timeline.unwrap_or_else(|| serde_json::json!([])),
            confidence_score: p.confidence_score.unwrap_or(0.0),
            last_updated_by: p.last_updated_by.unwrap_or_else(|| "ai".to_string()),
        })),
        None => Ok(Json(empty_entity_profile(id, entity.0, entity.1))),
    }
}

/// Generate entity profile using AI.
async fn generate_entity_profile(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<EntityProfileResponse>, ApiError> {
    crate::feature_flags::require_feature(&state.db, "ai_entities").await?;
    let book_id = ensure_entity_access(&state, &auth, id, LibraryAccess::Write).await?;

    let entity: (String, String, Option<String>) =
        sqlx::query_as("SELECT name, entity_type, description FROM entities WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await
            .map_err(ApiError::from)?
            .ok_or_else(|| ApiError::NotFound(format!("Entity {} not found", id)))?;

    let mentions: Vec<(Option<String>, Option<i32>)> = sqlx::query_as(
        r#"SELECT em.context_snippet, COALESCE(em.chapter_index, c.chapter_index) AS chapter_index
           FROM entity_mentions em
           JOIN chapters c ON c.id = em.chapter_id AND c.book_id = $2
           WHERE em.entity_id = $1
           ORDER BY COALESCE(em.chapter_index, c.chapter_index)
           LIMIT 30"#,
    )
    .bind(id)
    .bind(book_id)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;

    if mentions.is_empty() {
        return Err(ApiError::bad_request(
            "No mentions found for this entity. Run entity extraction first.",
        ));
    }

    let mentions_text = mentions
        .iter()
        .filter_map(|(snippet, chapter_index)| {
            let snippet = snippet.as_deref()?;
            Some(format!("[第{}章] {}", chapter_index.unwrap_or(0), snippet))
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    let api_key = &state.config.deepseek_api_key;
    let base_url = if state.config.deepseek_base_url.is_empty() {
        "https://api.deepseek.com/v1"
    } else {
        &state.config.deepseek_base_url
    };
    let model = if state.config.deepseek_model.is_empty() {
        "deepseek-chat"
    } else {
        &state.config.deepseek_model
    };

    let system_prompt = crate::prompts::entity_profile_prompt(&entity.0, &entity.1, &mentions_text);
    let start = std::time::Instant::now();
    let resp = state
        .http_client
        .post(format!("{}/chat/completions", base_url))
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&serde_json::json!({
            "model": model,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": format!("请为「{}」生成完整档案。已有描述：{}", entity.0, entity.2.as_deref().unwrap_or("无"))}
            ],
            "temperature": 0.3,
            "max_tokens": 2048,
        }))
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("AI error: {}", e)))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(ApiError::ServiceUnavailable(format!(
            "AI profile generation failed with {status}: {body}"
        )));
    }

    let latency = start.elapsed().as_millis() as i32;
    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ApiError::Internal(format!("Parse error: {}", e)))?;

    let content = data["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("{}");
    let parsed: serde_json::Value =
        serde_json::from_str(content).unwrap_or_else(|_| serde_json::json!({}));

    let appearance = parsed["appearance"].as_str().map(String::from);
    let personality = parsed["personality"].as_str().map(String::from);
    let background = parsed["background"].as_str().map(String::from);
    let abilities = parsed["abilities"].as_str().map(String::from);
    let motivation = parsed["motivation"].as_str().map(String::from);
    let arc_summary = parsed["arc_summary"].as_str().map(String::from);
    let attributes = parsed
        .get("attributes")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let confidence = parsed["confidence_score"].as_f64().unwrap_or(0.5);
    let timeline = serde_json::to_value(
        mentions
            .iter()
            .filter_map(|(snippet, chapter_index)| {
                Some(serde_json::json!({
                    "chapter": chapter_index.unwrap_or(0),
                    "snippet": snippet.as_deref()?,
                }))
            })
            .collect::<Vec<_>>(),
    )
    .unwrap_or_else(|_| serde_json::json!([]));

    sqlx::query(
        r#"INSERT INTO entity_profiles
           (entity_id, appearance, personality, background, abilities, motivation,
            arc_summary, attributes, timeline, confidence_score, last_updated_by, summary, traits, updated_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 'ai', $7, $8, NOW())
           ON CONFLICT (entity_id) DO UPDATE SET
             appearance = $2,
             personality = $3,
             background = $4,
             abilities = $5,
             motivation = $6,
             arc_summary = $7,
             attributes = $8,
             timeline = $9,
             confidence_score = $10,
             last_updated_by = 'ai',
             summary = $7,
             traits = $8,
             updated_at = NOW()"#,
    )
    .bind(id)
    .bind(&appearance)
    .bind(&personality)
    .bind(&background)
    .bind(&abilities)
    .bind(&motivation)
    .bind(&arc_summary)
    .bind(&attributes)
    .bind(&timeline)
    .bind(confidence)
    .execute(&state.db)
    .await
    .map_err(ApiError::from)?;

    let tracker = crate::ai_usage::AiUsageTracker::new(state.db.clone());
    tracker
        .log(crate::ai_usage::AiUsageRecord {
            user_id: Uuid::parse_str(&auth.id).ok(),
            book_id: Some(book_id),
            operation: "generate_entity_profile".to_string(),
            model: model.to_string(),
            provider: "deepseek".to_string(),
            prompt_tokens: data["usage"]["prompt_tokens"].as_i64().unwrap_or(0) as i32,
            completion_tokens: data["usage"]["completion_tokens"].as_i64().unwrap_or(0) as i32,
            total_tokens: data["usage"]["total_tokens"].as_i64().unwrap_or(0) as i32,
            latency_ms: latency,
            request_summary: Some(format!("Generate entity profile for {}", entity.0)),
            success: true,
            error_message: None,
            metadata: serde_json::json!({"entity_id": id.to_string()}),
        })
        .await;

    Ok(Json(EntityProfileResponse {
        entity_id: id,
        name: entity.0,
        entity_type: entity.1,
        appearance,
        personality,
        background,
        abilities,
        motivation,
        arc_summary,
        attributes,
        timeline,
        confidence_score: confidence,
        last_updated_by: "ai".to_string(),
    }))
}

/// Get entity timeline (appearances across chapters).
async fn get_entity_timeline(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let book_id = ensure_entity_access(&state, &auth, id, LibraryAccess::Read).await?;
    let mentions = sqlx::query_as::<_, TimelineRow>(
        r#"
        SELECT c.title as chapter_title, c.chapter_index,
               em.start_offset, em.context_snippet
        FROM entity_mentions em
        JOIN chapters c ON c.id = em.chapter_id
        WHERE em.entity_id = $1
          AND c.book_id = $2
        ORDER BY c.chapter_index, em.start_offset
        "#,
    )
    .bind(id)
    .bind(book_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let events: Vec<serde_json::Value> = mentions
        .into_iter()
        .map(|m| {
            serde_json::json!({
                "chapter_index": m.chapter_index,
                "chapter_title": m.chapter_title,
                "context": m.context_snippet,
                "position": m.start_offset,
            })
        })
        .collect();

    Ok(Json(
        serde_json::json!({ "entity_id": id, "timeline": events }),
    ))
}

#[derive(sqlx::FromRow)]
struct TimelineRow {
    chapter_title: Option<String>,
    chapter_index: Option<i32>,
    start_offset: i64,
    context_snippet: Option<String>,
}

/// POST /api/entities/sync-graph/:book_id
/// Sync entities and relationships from PG to Neo4j for a specific book.
async fn sync_graph(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(book_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    ensure_entity_book_write_access(&state, &auth, book_id).await?;
    let result = nova_graph::sync_book_to_neo4j(&state.db, &state.neo4j, book_id)
        .await
        .map_err(|e| ApiError::Internal(format!("Graph sync failed: {}", e)))?;

    Ok(Json(serde_json::json!({
        "status": "completed",
        "book_id": book_id,
        "entities_synced": result.entities_synced,
        "relationships_synced": result.relationships_synced,
        "errors": result.errors,
    })))
}

/// POST /entities/communities/:book_id — Run Leiden community detection.
async fn detect_communities_endpoint(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(book_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    ensure_entity_book_write_access(&state, &auth, book_id).await?;
    let book_id_str = book_id.to_string();

    // Extract graph from Neo4j
    let graph = nova_graph::extract_book_graph(&state.neo4j, &book_id_str)
        .await
        .map_err(|e| ApiError::Internal(format!("Graph extraction failed: {}", e)))?;

    if graph.node_count() == 0 {
        return Ok(Json(serde_json::json!({
            "status": "no_data",
            "book_id": book_id,
            "message": "No entities in graph. Run sync-graph first.",
        })));
    }

    // Run hierarchical community detection at multiple resolutions
    let resolutions = vec![0.5, 1.0, 1.5];
    let results = nova_graph::detect_hierarchical(&graph, &resolutions, 2);

    // Store the primary (resolution=1.0) result to Neo4j
    if let Some(primary) = results.get(1) {
        let stored = nova_graph::store_communities(&state.neo4j, &book_id_str, &graph, primary)
            .await
            .map_err(|e| ApiError::Internal(format!("Community storage failed: {}", e)))?;

        let communities = nova_graph::extract_community_data(&graph, primary);

        Ok(Json(serde_json::json!({
            "status": "completed",
            "book_id": book_id,
            "node_count": graph.node_count(),
            "communities_stored": stored,
            "levels": results.iter().map(|r| serde_json::json!({
                "level": r.level,
                "num_communities": r.num_communities,
                "modularity": r.modularity,
            })).collect::<Vec<_>>(),
            "communities": communities.iter().take(20).map(|c| serde_json::json!({
                "id": c.id,
                "members": c.members,
                "internal_edges": c.internal_edges,
            })).collect::<Vec<_>>(),
        })))
    } else {
        Err(ApiError::Internal(
            "Community detection produced no results".to_string(),
        ))
    }
}

/// GET /entities/communities/:book_id — Get stored communities for a book.
async fn get_communities(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(book_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    ensure_entity_book_read_access(&state, &auth, book_id).await?;
    let book_id_str = book_id.to_string();

    let cypher = "MATCH (c:Community {book_id: $book_id}) \
                  RETURN c.id AS id, c.level AS level, c.members AS members, \
                  c.entity_count AS entity_count, c.summary AS summary \
                  ORDER BY c.entity_count DESC";

    let result = state
        .neo4j
        .execute(
            cypher,
            Some(serde_json::json!({
                "book_id": book_id_str,
            })),
        )
        .await
        .map_err(|e| ApiError::Internal(format!("Neo4j query failed: {}", e)))?;

    let communities: Vec<serde_json::Value> = result["results"]
        .as_array()
        .and_then(|r| r.first())
        .and_then(|r| r["data"].as_array())
        .map(|data| {
            data.iter()
                .filter_map(|row| {
                    let r = row["row"].as_array()?;
                    Some(serde_json::json!({
                        "id": r.first()?,
                        "level": r.get(1)?,
                        "members": r.get(2)?,
                        "entity_count": r.get(3)?,
                        "summary": r.get(4)?,
                    }))
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(Json(serde_json::json!({
        "book_id": book_id,
        "communities": communities,
        "total": communities.len(),
    })))
}

/// POST /entities/disambiguate/:book_id — Entity disambiguation.
/// Finds potential duplicate entities and merges them based on name similarity.
async fn disambiguate_entities(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(book_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    ensure_entity_book_write_access(&state, &auth, book_id).await?;
    // Find entities with similar names (potential duplicates)
    let entities: Vec<(Uuid, String, String, Option<String>)> = sqlx::query_as(
        "SELECT id, name, entity_type, canonical_name FROM entities WHERE book_id = $1 ORDER BY name"
    )
    .bind(book_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    let mut merge_candidates: Vec<serde_json::Value> = Vec::new();
    let mut merged_count = 0;

    // Simple n² comparison for finding similar names
    for i in 0..entities.len() {
        for j in (i + 1)..entities.len() {
            let (id_a, name_a, type_a, _) = &entities[i];
            let (id_b, name_b, type_b, _) = &entities[j];

            // Same type and similar name
            if type_a == type_b && are_names_similar(name_a, name_b) {
                merge_candidates.push(serde_json::json!({
                    "entity_a": { "id": id_a, "name": name_a },
                    "entity_b": { "id": id_b, "name": name_b },
                    "type": type_a,
                    "similarity": compute_name_similarity(name_a, name_b),
                }));

                // Auto-merge: set canonical_name on the shorter-named entity
                let (canonical, alias_id) = if name_a.len() >= name_b.len() {
                    (name_a.as_str(), id_b)
                } else {
                    (name_b.as_str(), id_a)
                };

                let _ = sqlx::query(
                    "UPDATE entities SET canonical_name = $1 WHERE id = $2 AND canonical_name IS NULL"
                )
                .bind(canonical)
                .bind(alias_id)
                .execute(&state.db)
                .await;

                merged_count += 1;
            }
        }
    }

    Ok(Json(serde_json::json!({
        "status": "completed",
        "book_id": book_id,
        "total_entities": entities.len(),
        "merge_candidates": merge_candidates.len(),
        "auto_merged": merged_count,
        "candidates": merge_candidates,
    })))
}

/// Check if two entity names are likely the same entity.
fn are_names_similar(a: &str, b: &str) -> bool {
    // Exact substring match
    if a.contains(b) || b.contains(a) {
        return true;
    }

    // Normalized edit distance
    let similarity = compute_name_similarity(a, b);
    similarity > 0.7
}

/// Compute normalized similarity between two names (0.0 to 1.0).
fn compute_name_similarity(a: &str, b: &str) -> f64 {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();

    if a_chars.is_empty() || b_chars.is_empty() {
        return 0.0;
    }

    // Longest Common Subsequence ratio
    let lcs_len = lcs_length(&a_chars, &b_chars);
    let max_len = a_chars.len().max(b_chars.len()) as f64;

    lcs_len as f64 / max_len
}

/// Compute LCS length using DP.
fn lcs_length(a: &[char], b: &[char]) -> usize {
    let m = a.len();
    let n = b.len();
    let mut dp = vec![vec![0usize; n + 1]; m + 1];

    for i in 1..=m {
        for j in 1..=n {
            dp[i][j] = if a[i - 1] == b[j - 1] {
                dp[i - 1][j - 1] + 1
            } else {
                dp[i - 1][j].max(dp[i][j - 1])
            };
        }
    }

    dp[m][n]
}

/// POST /entities/communities/:book_id/summarize — Generate LLM summaries for communities.
async fn summarize_communities(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(book_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    ensure_entity_book_write_access(&state, &auth, book_id).await?;
    let book_id_str = book_id.to_string();

    // 1. Get communities from Neo4j
    let cypher = "MATCH (c:Community {book_id: $book_id}) \
                  WHERE c.summary IS NULL OR c.summary = '' \
                  RETURN c.id AS id, c.members AS members \
                  ORDER BY size(c.members) DESC LIMIT 20";

    let result = state
        .neo4j
        .execute(
            cypher,
            Some(serde_json::json!({
                "book_id": book_id_str,
            })),
        )
        .await
        .map_err(|e| ApiError::Internal(format!("Neo4j query failed: {}", e)))?;

    let communities: Vec<(String, Vec<String>)> = result["results"]
        .as_array()
        .and_then(|r| r.first())
        .and_then(|r| r["data"].as_array())
        .map(|data| {
            data.iter()
                .filter_map(|row| {
                    let r = row["row"].as_array()?;
                    let id = r.first()?.as_str()?.to_string();
                    let members: Vec<String> = r
                        .get(1)?
                        .as_array()?
                        .iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect();
                    Some((id, members))
                })
                .collect()
        })
        .unwrap_or_default();

    if communities.is_empty() {
        return Ok(Json(serde_json::json!({
            "status": "no_unsummarized_communities",
            "book_id": book_id,
        })));
    }

    let mut summarized = 0;
    let mut errors = Vec::new();

    for (community_id, members) in &communities {
        if members.is_empty() {
            continue;
        }

        // 2. Get relationships between community members from Neo4j
        let rel_cypher =
            "MATCH (a:Entity {book_id: $book_id})-[r]->(b:Entity {book_id: $book_id}) \
                          WHERE a.name IN $members AND b.name IN $members \
                          RETURN a.name AS source, type(r) AS rel_type, b.name AS target LIMIT 50";

        let rel_result = state
            .neo4j
            .execute(
                rel_cypher,
                Some(serde_json::json!({
                    "book_id": book_id_str,
                    "members": members,
                })),
            )
            .await
            .unwrap_or_default();

        let relationships: Vec<String> = rel_result["results"]
            .as_array()
            .and_then(|r| r.first())
            .and_then(|r| r["data"].as_array())
            .map(|data| {
                data.iter()
                    .filter_map(|row| {
                        let r = row["row"].as_array()?;
                        let src = r.first()?.as_str()?;
                        let rel = r.get(1)?.as_str()?;
                        let tgt = r.get(2)?.as_str()?;
                        Some(format!("{} --[{}]--> {}", src, rel, tgt))
                    })
                    .collect()
            })
            .unwrap_or_default();

        // 3. Build prompt from template
        let entities_str = members.join("、");
        let relationships_str = if relationships.is_empty() {
            "（暂无已知关系）".to_string()
        } else {
            relationships.join("\n")
        };

        let prompt = nova_graph::community::COMMUNITY_SUMMARY_PROMPT
            .replace("{entities}", &entities_str)
            .replace("{relationships}", &relationships_str);

        // 4. Call DeepSeek API
        let resp = state
            .http_client
            .post(format!(
                "{}/chat/completions",
                state.config.deepseek_base_url
            ))
            .header(
                "Authorization",
                format!("Bearer {}", state.config.deepseek_api_key),
            )
            .json(&serde_json::json!({
                "model": state.config.deepseek_model,
                "messages": [{"role": "user", "content": prompt}],
                "temperature": 0.3,
                "max_tokens": 500,
            }))
            .send()
            .await;

        match resp {
            Ok(r) if r.status().is_success() => {
                if let Ok(body) = r.json::<serde_json::Value>().await {
                    let content = body["choices"][0]["message"]["content"]
                        .as_str()
                        .unwrap_or_default();

                    // Parse JSON from response (may be wrapped in ```json blocks)
                    let json_str = content
                        .trim()
                        .trim_start_matches("```json")
                        .trim_start_matches("```")
                        .trim_end_matches("```")
                        .trim();

                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_str) {
                        let summary = parsed["summary"].as_str().unwrap_or_default();
                        let key_findings: Vec<String> = parsed["key_findings"]
                            .as_array()
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                    .collect()
                            })
                            .unwrap_or_default();

                        // 5. Store summary back to Neo4j
                        let update_cypher = "MATCH (c:Community {id: $id, book_id: $book_id}) \
                                             SET c.summary = $summary, c.key_findings = $findings";

                        let _ = state
                            .neo4j
                            .execute(
                                update_cypher,
                                Some(serde_json::json!({
                                    "id": community_id,
                                    "book_id": book_id_str,
                                    "summary": summary,
                                    "findings": key_findings,
                                })),
                            )
                            .await;

                        summarized += 1;
                    } else {
                        errors.push(format!(
                            "Community {}: failed to parse LLM response",
                            community_id
                        ));
                    }
                }
            }
            Ok(r) => {
                errors.push(format!(
                    "Community {}: LLM returned {}",
                    community_id,
                    r.status()
                ));
            }
            Err(e) => {
                errors.push(format!(
                    "Community {}: LLM call failed: {}",
                    community_id, e
                ));
            }
        }
    }

    Ok(Json(serde_json::json!({
        "status": "completed",
        "book_id": book_id,
        "total_communities": communities.len(),
        "summarized": summarized,
        "errors": errors,
    })))
}

#[cfg(test)]
mod tests {
    fn production_source() -> &'static str {
        include_str!("entities.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("production source should exist")
    }

    #[test]
    fn entity_routes_have_auth_and_acl_helpers() {
        let source = production_source();

        assert!(source.contains("extractors::AuthUser"));
        assert!(source.contains("ensure_entity_book_read_access"));
        assert!(source.contains("ensure_entity_book_write_access"));
        assert!(source.contains("ensure_entity_access"));
        assert!(source.contains("SELECT book_id FROM entities WHERE id = $1"));
        assert!(source.contains("ensure_book_access(state, auth, book_id, LibraryAccess::Read)"));
        assert!(source.contains("ensure_book_access(state, auth, book_id, LibraryAccess::Write)"));
    }

    #[test]
    fn entity_id_routes_require_object_acl() {
        let source = production_source();

        assert_eq!(
            source
                .matches("ensure_entity_access(&state, &auth, id, LibraryAccess::Read).await?")
                .count(),
            5
        );
        assert_eq!(
            source
                .matches("ensure_entity_access(&state, &auth, id, LibraryAccess::Write).await?")
                .count(),
            2
        );
    }

    #[test]
    fn book_bound_entity_routes_require_book_acl() {
        let source = production_source();

        assert!(source.contains("ensure_entity_book_read_access(&state, &auth, book_id).await?"));
        assert_eq!(
            source
                .matches("ensure_entity_book_write_access(&state, &auth, book_id).await?")
                .count(),
            4
        );
    }

    #[test]
    fn collection_entity_routes_scope_to_visible_libraries() {
        let source = production_source();

        assert!(source.contains("visible_entity_library_ids"));
        assert!(source.contains("visible_library_ids(state, auth, LibraryAccess::Read)"));
        assert!(source.contains("b.library_id = ANY("));
    }

    #[test]
    fn entity_mentions_do_not_trust_denormalized_book_id() {
        let source = production_source();

        assert!(source.contains("SELECT em.id, c.book_id AS book_id"));
        assert!(source.contains("LEFT JOIN books b ON b.id = c.book_id"));
        assert!(!source.contains("COALESCE(em.book_id, c.book_id) AS book_id"));
        assert!(!source.contains("LEFT JOIN books b ON b.id = COALESCE(em.book_id, c.book_id)"));
    }
}
