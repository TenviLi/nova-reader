use std::sync::Arc;

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    access::{ensure_book_access, LibraryAccess},
    error::{ApiError, ApiResult},
    extractors::AuthUser,
    state::AppState,
};

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/{id}/profile", get(get_entity_profile))
        .route("/{id}/profile/generate", post(generate_entity_profile))
        .route("/{id}/timeline", get(get_entity_timeline))
}

#[derive(Serialize)]
struct EntityProfile {
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

/// Get an entity's rich profile (AI-generated character/setting card).
async fn get_entity_profile(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<EntityProfile>, ApiError> {
    ensure_entity_access(&state, &auth, id, LibraryAccess::Read).await?;

    // First get the entity base info
    let entity = sqlx::query!(
        "SELECT id, name, entity_type FROM entities WHERE id = $1",
        id
    )
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::from)?
    .ok_or_else(|| ApiError::NotFound(format!("Entity {} not found", id)))?;

    // Then get the profile
    let profile = sqlx::query!(
        r#"SELECT appearance, personality, background, abilities, motivation,
                  arc_summary, attributes, timeline, confidence_score, last_updated_by
           FROM entity_profiles WHERE entity_id = $1"#,
        id
    )
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::from)?;

    match profile {
        Some(p) => Ok(Json(EntityProfile {
            entity_id: id,
            name: entity.name,
            entity_type: entity.entity_type,
            appearance: p.appearance,
            personality: p.personality,
            background: p.background,
            abilities: p.abilities,
            motivation: p.motivation,
            arc_summary: p.arc_summary,
            attributes: p.attributes.unwrap_or_default(),
            timeline: p.timeline.unwrap_or_default(),
            confidence_score: p.confidence_score.unwrap_or(0.0),
            last_updated_by: p.last_updated_by.unwrap_or_else(|| "ai".to_string()),
        })),
        None => Ok(Json(EntityProfile {
            entity_id: id,
            name: entity.name,
            entity_type: entity.entity_type,
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
        })),
    }
}

/// Generate/refresh an entity profile using AI analysis of all mentions.
async fn generate_entity_profile(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<EntityProfile>, ApiError> {
    let book_id = ensure_entity_access(&state, &auth, id, LibraryAccess::Write).await?;

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

    // Get entity info
    let entity = sqlx::query!(
        "SELECT id, name, entity_type, description FROM entities WHERE id = $1",
        id
    )
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::from)?
    .ok_or_else(|| ApiError::NotFound(format!("Entity {} not found", id)))?;

    // Get all mentions with context
    let mentions = sqlx::query!(
        r#"SELECT context_snippet, chapter_index
           FROM entity_mentions
           WHERE entity_id = $1
           ORDER BY chapter_index
           LIMIT 30"#,
        id
    )
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;

    if mentions.is_empty() {
        return Err(ApiError::BadRequest(
            "No mentions found for this entity. Run entity extraction first.".to_string(),
        ));
    }

    // Build context from mentions
    let mentions_text: String = mentions
        .iter()
        .filter_map(|m| {
            let snippet = m.context_snippet.as_deref()?;
            let ch = m.chapter_index.unwrap_or(0);
            Some(format!("[第{}章] {}", ch, snippet))
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    // Generate profile via AI
    let system_prompt =
        crate::prompts::entity_profile_prompt(&entity.name, &entity.entity_type, &mentions_text);

    let client = &state.http_client;
    let start = std::time::Instant::now();
    let resp = client
        .post(format!("{}/chat/completions", base_url))
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&serde_json::json!({
            "model": model,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": format!("请为「{}」生成完整档案。", entity.name)}
            ],
            "temperature": 0.3,
            "max_tokens": 2048,
        }))
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("AI error: {}", e)))?;
    let latency = start.elapsed().as_millis() as i32;

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ApiError::Internal(format!("Parse error: {}", e)))?;

    let content = data["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("{}");

    // Log AI usage
    let usage_tokens = data["usage"]["total_tokens"].as_i64().unwrap_or(0) as i32;
    let tracker = crate::ai_usage::AiUsageTracker::new(state.db.clone());
    tracker
        .log(crate::ai_usage::AiUsageRecord {
            user_id: None,
            book_id: Some(book_id),
            operation: "generate_entity_profile".to_string(),
            model: model.to_string(),
            provider: "deepseek".to_string(),
            prompt_tokens: data["usage"]["prompt_tokens"].as_i64().unwrap_or(0) as i32,
            completion_tokens: data["usage"]["completion_tokens"].as_i64().unwrap_or(0) as i32,
            total_tokens: usage_tokens,
            latency_ms: latency,
            request_summary: Some(format!("Generate profile for: {}", entity.name)),
            success: true,
            error_message: None,
            metadata: serde_json::json!({"entity_id": id.to_string()}),
        })
        .await;

    // Parse AI response
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

    // Build timeline from mentions
    let timeline: serde_json::Value = serde_json::to_value(
        mentions
            .iter()
            .filter_map(|m| {
                Some(serde_json::json!({
                    "chapter": m.chapter_index?,
                    "snippet": m.context_snippet.as_deref()?,
                }))
            })
            .collect::<Vec<_>>(),
    )
    .unwrap_or_default();

    // Upsert profile
    sqlx::query(
        r#"INSERT INTO entity_profiles (entity_id, appearance, personality, background, abilities, motivation, arc_summary, attributes, timeline, confidence_score, last_updated_by, updated_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 'ai', NOW())
           ON CONFLICT (entity_id) DO UPDATE SET
             appearance = $2, personality = $3, background = $4, abilities = $5,
             motivation = $6, arc_summary = $7, attributes = $8, timeline = $9,
             confidence_score = $10, last_updated_by = 'ai', updated_at = NOW()"#
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

    Ok(Json(EntityProfile {
        entity_id: id,
        name: entity.name,
        entity_type: entity.entity_type,
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

/// Get entity timeline: all appearances across chapters with context.
async fn get_entity_timeline(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<TimelineEntry>>, ApiError> {
    ensure_entity_access(&state, &auth, id, LibraryAccess::Read).await?;

    let entries = sqlx::query!(
        r#"SELECT em.chapter_index, c.title as "chapter_title?", em.context_snippet
           FROM entity_mentions em
           LEFT JOIN chapters c ON c.book_id = (SELECT book_id FROM entity_mentions WHERE entity_id = $1 LIMIT 1)
               AND c.chapter_index = em.chapter_index
           WHERE em.entity_id = $1
           ORDER BY em.chapter_index"#,
        id,
    )
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;

    let result: Vec<TimelineEntry> = entries
        .into_iter()
        .map(|e| TimelineEntry {
            chapter_index: e.chapter_index.unwrap_or(0),
            chapter_title: e.chapter_title.unwrap_or_default(),
            context: e.context_snippet.unwrap_or_default(),
        })
        .collect();

    Ok(Json(result))
}

#[derive(Serialize)]
struct TimelineEntry {
    chapter_index: i32,
    chapter_title: String,
    context: String,
}
