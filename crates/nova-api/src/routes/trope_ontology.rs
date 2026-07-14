//! Trope Ontology & Persona Drift routes.
//!
//! - Ontology Tree: HDBSCAN-based self-evolving taxonomy of user-defined story settings
//! - Persona Drift: Character voice vector tracking across chapters
//! - Rule Splicing: Cross-book setting rule extraction and combination

use std::{collections::HashSet, sync::Arc};

use axum::{
    extract::{Path, Query, State},
    routing::{get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;

use crate::access::{ensure_book_access, is_admin, visible_library_ids, LibraryAccess};
use crate::error::{ApiError, ApiResult};
use crate::extractors::AuthUser;
use crate::state::AppState;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        // Ontology Tree — Browse
        .route("/ontology/tree", get(get_ontology_tree))
        .route("/ontology/tree/{node_id}", get(get_node_detail))
        .route("/ontology/tree/{node_id}/children", get(get_node_children))
        .route("/ontology/tree/{node_id}/chunks", get(get_node_chunks))
        // Ontology Tree — Manual CRUD (user creates/edits/moves/deletes nodes freely)
        .route("/ontology/nodes", post(create_node))
        .route(
            "/ontology/nodes/{node_id}",
            put(update_node).delete(delete_node),
        )
        .route("/ontology/nodes/{node_id}/move", post(move_node))
        .route("/ontology/nodes/{node_id}/merge", post(merge_nodes))
        // Ontology Tree — AI-driven operations
        .route("/ontology/nodes/{node_id}/scan", post(scan_node))
        .route("/ontology/nodes/{node_id}/evolve", post(evolve_node))
        .route("/ontology/cluster", post(trigger_clustering))
        .route("/ontology/evolve", post(trigger_evolution))
        .route("/ontology/book/{book_id}/assign", post(assign_book_to_tree))
        .route("/ontology/search", post(search_by_attributes))
        .route("/ontology/events", get(list_ontology_events))
        // Persona Drift
        .route("/persona/track", post(track_persona))
        .route(
            "/persona/book/{book_id}/characters",
            get(list_tracked_characters),
        )
        .route(
            "/persona/book/{book_id}/{character}/timeline",
            get(get_drift_timeline),
        )
        .route(
            "/persona/book/{book_id}/{character}/events",
            get(get_drift_events),
        )
        .route("/persona/compare", post(compare_personas))
        // Rule Splicing
        .route("/rules/book/{book_id}", get(list_book_rules))
        .route("/rules/extract", post(extract_rules))
        .route("/rules/splice", post(splice_rules))
}

// ─── Data Types ──────────────────────────────────────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
struct TropeNode {
    id: Uuid,
    parent_id: Option<Uuid>,
    label: String,
    description: Option<String>,
    level: i32,
    cluster_size: i32,
    stability: f64,
    attributes: serde_json::Value,
    domain: String,
    is_leaf: bool,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize)]
struct TropeTreeResponse {
    nodes: Vec<TropeNodeWithChildren>,
    total_chunks: i64,
    max_depth: i32,
}

#[derive(Serialize)]
struct TropeNodeWithChildren {
    #[serde(flatten)]
    node: TropeNode,
    children_count: i64,
    sample_texts: Vec<String>,
}

#[derive(Serialize, sqlx::FromRow)]
struct PersonaSnapshot {
    id: Uuid,
    book_id: Uuid,
    character_name: String,
    chapter_index: i32,
    dialogue_count: i32,
    monologue_count: i32,
    drift_from_prev: Option<f64>,
    drift_from_baseline: Option<f64>,
    computed_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize, sqlx::FromRow)]
struct DriftEvent {
    id: Uuid,
    book_id: Uuid,
    character_name: String,
    chapter_index: i32,
    drift_magnitude: f64,
    drift_direction: Option<String>,
    evidence_text: Option<String>,
    target_persona: Option<String>,
    event_type: String,
    detected_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize, sqlx::FromRow)]
struct SettingRule {
    id: Uuid,
    book_id: Uuid,
    trope_node_id: Option<Uuid>,
    subject_type: String,
    subject_label: String,
    predicate: String,
    object_type: String,
    object_label: String,
    properties: serde_json::Value,
    constraints: Option<serde_json::Value>,
    source_text: Option<String>,
    chapter_index: Option<i32>,
    confidence: f64,
}

#[derive(Deserialize)]
struct TreeQuery {
    domain: Option<String>,
    max_depth: Option<i32>,
}

// ─── Manual CRUD Types ───────────────────────────────────────────────────────

#[derive(Deserialize)]
struct CreateNodeRequest {
    /// Human-readable label for the setting concept.
    label: String,
    /// Optional longer description
    description: Option<String>,
    /// Parent node ID (None = root node)
    parent_id: Option<Uuid>,
    /// Domain classification
    domain: Option<String>,
    /// Reference texts to help define this concept (optional, improves embedding)
    reference_texts: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct UpdateNodeRequest {
    label: Option<String>,
    description: Option<String>,
    domain: Option<String>,
    reference_texts: Option<Vec<String>>,
    /// Re-compute embedding with updated text (default true)
    recompute_embedding: Option<bool>,
}

#[derive(Deserialize)]
struct MoveNodeRequest {
    /// New parent ID (None = move to root)
    new_parent_id: Option<Uuid>,
    /// Position index among siblings (for ordering)
    #[allow(dead_code)]
    position: Option<i32>,
}

#[derive(Deserialize)]
struct MergeNodesRequest {
    /// Node to merge INTO the target (will be deleted after merge)
    source_node_id: Uuid,
}

#[derive(Deserialize)]
struct ScanNodeRequest {
    /// Max results to return
    limit: Option<usize>,
    /// Only scan specific books
    book_ids: Option<Vec<Uuid>>,
    /// Similarity threshold (default 0.45)
    threshold: Option<f32>,
}

#[derive(Deserialize)]
struct EvolveNodeRequest {
    /// How many sub-clusters to try to discover (default: auto)
    max_children: Option<usize>,
    /// Minimum chunks needed to form a sub-concept (default 3)
    min_evidence: Option<usize>,
}

#[derive(Serialize)]
struct ScanResult {
    node_id: Uuid,
    matched_chunks: Vec<MatchedChunk>,
    total_matched: usize,
    languages_found: Vec<String>,
}

#[derive(Serialize)]
struct MatchedChunk {
    point_id: i64,
    book_id: Uuid,
    book_title: String,
    chapter_index: i32,
    chunk_index: i32,
    text: String,
    score: f32,
    /// Detected language of the chunk (zh, ja, en, ko, etc.)
    language: Option<String>,
}

#[derive(Serialize)]
struct EvolveResult {
    parent_node_id: Uuid,
    proposed_children: Vec<ProposedNode>,
    evidence_chunks: usize,
}

#[derive(Serialize)]
struct ProposedNode {
    label: String,
    description: Option<String>,
    evidence_count: usize,
    sample_text: String,
    /// Auto-created if user doesn't reject
    auto_created: bool,
    created_id: Option<Uuid>,
}

#[derive(Deserialize)]
struct ClusterRequest {
    domain: Option<String>,
    /// Filter to specific books for initial clustering
    book_ids: Option<Vec<Uuid>>,
    /// HDBSCAN min_cluster_size (default 5)
    min_cluster_size: Option<usize>,
    /// Minimum samples for core point (default 3)
    min_samples: Option<usize>,
}

#[derive(Deserialize)]
struct AssignBookRequest {
    /// If true, only assign new chunks not already in tree
    incremental: Option<bool>,
}

#[derive(Deserialize)]
struct AttributeSearchRequest {
    /// Key-value filters, e.g. {"domain": "力量体系", "tone": "热血"}
    filters: serde_json::Value,
    domain: Option<String>,
    limit: Option<i64>,
}

#[derive(Deserialize)]
struct TrackPersonaRequest {
    book_id: Uuid,
    character_name: String,
    /// If provided, only compute for these chapters
    chapter_range: Option<(i32, i32)>,
}

#[derive(Deserialize)]
struct ComparePersonasRequest {
    /// Compare persona A (book_id, character_name) with persona B
    source: PersonaRef,
    target: PersonaRef,
}

#[derive(Deserialize)]
struct PersonaRef {
    book_id: Uuid,
    character_name: String,
}

#[derive(Serialize)]
struct DriftTimeline {
    character_name: String,
    book_id: Uuid,
    snapshots: Vec<PersonaSnapshot>,
    events: Vec<DriftEvent>,
    /// Overall drift score (0 = stable, 1 = completely transformed)
    total_drift: f64,
}

#[derive(Deserialize)]
struct ExtractRulesRequest {
    book_id: Uuid,
    /// Focus on specific chapters
    chapter_range: Option<(i32, i32)>,
    /// Target story-setting domain for extraction prompts.
    domain: Option<String>,
}

#[derive(Deserialize)]
struct SpliceRulesRequest {
    /// IDs of rules to combine for scenario generation
    rule_ids: Vec<Uuid>,
    /// Additional context/constraints for the splice
    scenario_prompt: Option<String>,
}

#[derive(Serialize)]
struct SpliceResult {
    /// Generated scenario text
    narrative: String,
    /// Rules that were combined
    source_rules: Vec<SettingRule>,
    /// Conflicts detected between rules
    conflicts: Vec<String>,
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn parse_user_id(auth: &AuthUser) -> ApiResult<Uuid> {
    auth.id
        .parse::<Uuid>()
        .map_err(|_| ApiError::unauthorized())
}

async fn ensure_trope_book_read_access(
    state: &AppState,
    auth: &AuthUser,
    book_id: Uuid,
) -> ApiResult<()> {
    ensure_book_access(state, auth, book_id, LibraryAccess::Read).await
}

async fn ensure_trope_book_write_access(
    state: &AppState,
    auth: &AuthUser,
    book_id: Uuid,
) -> ApiResult<()> {
    ensure_book_access(state, auth, book_id, LibraryAccess::Write).await
}

async fn ensure_trope_manage_access(state: &AppState, auth: &AuthUser) -> ApiResult<()> {
    if is_admin(state, auth).await? {
        Ok(())
    } else {
        Err(ApiError::forbidden())
    }
}

async fn visible_trope_book_ids(
    state: &AppState,
    auth: &AuthUser,
    requested_book_ids: Option<&[Uuid]>,
    access: LibraryAccess,
) -> ApiResult<Option<Vec<Uuid>>> {
    if let Some(book_ids) = requested_book_ids {
        if !book_ids.is_empty() {
            for &book_id in book_ids {
                ensure_book_access(state, auth, book_id, access).await?;
            }
            return Ok(Some(book_ids.to_vec()));
        }
    }

    match access {
        LibraryAccess::Read => match visible_library_ids(state, auth, LibraryAccess::Read).await? {
            None => Ok(None),
            Some(library_ids) if library_ids.is_empty() => Ok(Some(Vec::new())),
            Some(library_ids) => {
                let book_ids =
                    sqlx::query_scalar("SELECT id FROM books WHERE library_id = ANY($1::uuid[])")
                        .bind(&library_ids)
                        .fetch_all(&state.db)
                        .await
                        .map_err(ApiError::from)?;
                Ok(Some(book_ids))
            }
        },
        LibraryAccess::Write => match visible_library_ids(state, auth, LibraryAccess::Write).await?
        {
            None => Ok(None),
            Some(library_ids) if library_ids.is_empty() => Ok(Some(Vec::new())),
            Some(library_ids) => {
                let book_ids =
                    sqlx::query_scalar("SELECT id FROM books WHERE library_id = ANY($1::uuid[])")
                        .bind(&library_ids)
                        .fetch_all(&state.db)
                        .await
                        .map_err(ApiError::from)?;
                Ok(Some(book_ids))
            }
        },
        LibraryAccess::Manage => Err(ApiError::forbidden()),
    }
}

fn qdrant_book_filter(book_ids: Option<&[Uuid]>) -> serde_json::Value {
    match book_ids {
        Some(ids) if !ids.is_empty() => serde_json::json!({
            "must": [{"key": "book_id", "match": {"any": ids.iter().map(|id| id.to_string()).collect::<Vec<_>>()}}]
        }),
        _ => serde_json::json!({}),
    }
}

fn allowed_book_id_set(book_ids: Option<&[Uuid]>) -> Option<HashSet<Uuid>> {
    book_ids.map(|ids| ids.iter().copied().collect())
}

fn qdrant_payload_book_id(payload: &serde_json::Value) -> Option<Uuid> {
    payload["book_id"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
}

/// Embed a single text via the Gitee AI endpoint (2560-dim Qwen3-Embedding-4B)
async fn embed_text(state: &AppState, text: &str) -> ApiResult<Vec<f32>> {
    let client = &state.http_client;
    let api_key = std::env::var("EMBEDDING_API_KEY")
        .map_err(|_| ApiError::internal("EMBEDDING_API_KEY not set"))?;

    let resp = client
        .post("https://ai.gitee.com/v1/embeddings")
        .header("Authorization", format!("Bearer {api_key}"))
        .json(&serde_json::json!({
            "input": [text],
            "model": "Qwen3-Embedding-4B",
            "dimensions": 2560
        }))
        .send()
        .await
        .map_err(|e| ApiError::internal(format!("Embedding request failed: {e}")))?;

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ApiError::internal(format!("Embedding parse failed: {e}")))?;

    let embedding = body["data"][0]["embedding"]
        .as_array()
        .ok_or_else(|| ApiError::internal("No embedding in response"))?
        .iter()
        .map(|v| v.as_f64().unwrap_or(0.0) as f32)
        .collect::<Vec<f32>>();

    Ok(embedding)
}

/// Compute cosine similarity between two vectors
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

/// Serialize f32 vector to bytes for storage
fn vector_to_bytes(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// Deserialize bytes back to f32 vector
fn bytes_to_vector(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

/// Compute centroid of multiple vectors
fn compute_centroid(vectors: &[Vec<f32>]) -> Vec<f32> {
    if vectors.is_empty() {
        return vec![];
    }
    let dim = vectors[0].len();
    let mut centroid = vec![0.0f32; dim];
    for v in vectors {
        for (i, val) in v.iter().enumerate() {
            centroid[i] += val;
        }
    }
    let n = vectors.len() as f32;
    for val in &mut centroid {
        *val /= n;
    }
    // L2 normalize the centroid
    let norm: f32 = centroid.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for val in &mut centroid {
            *val /= norm;
        }
    }
    centroid
}

/// Call LLM to generate a label, description, and structured attributes for a cluster.
///
/// Returns: (label, description, attributes_json)
async fn call_llm_for_label(
    state: &AppState,
    sample_texts: &str,
) -> ApiResult<(String, Option<String>, serde_json::Value)> {
    let api_key = std::env::var("AI_API_KEY")
        .or_else(|_| std::env::var("DEEPSEEK_API_KEY"))
        .map_err(|_| ApiError::internal("AI_API_KEY not set"))?;
    let base_url =
        std::env::var("AI_API_BASE").unwrap_or_else(|_| "https://api.deepseek.com/v1".into());
    let model = std::env::var("AI_MODEL").unwrap_or_else(|_| "deepseek-chat".into());

    let system_prompt = r#"你是一个小说设定分析专家，专注于用户自定义的设定、桥段、人物关系、世界观机制和叙事模式。
你的任务是根据提供的同一聚类中的小说文本片段，生成：
1. 一个简短标签（10字以内），概括这个聚类的核心设定机制
2. 一段描述（50字以内）
3. 结构化属性JSON

请严格按照以下JSON格式输出（不要有其他文字）：
{
  "label": "标签",
  "description": "描述",
    "attributes": {
        "domain": "设定领域或桥段类型/null",
        "mechanism": "核心机制/null",
        "participants": ["涉及角色或势力"],
        "tone": "情绪或叙事调性/null",
        "constraints": ["规则或限制"],
        "consequences": ["后果或影响"]
    }
}"#;

    let user_prompt = format!(
        "以下是同一聚类中的文本片段，请分析并输出JSON：\n\n{}",
        sample_texts
    );

    let resp = state
        .http_client
        .post(format!("{base_url}/chat/completions"))
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": model,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": user_prompt}
            ],
            "temperature": 0.3,
            "max_tokens": 500,
            "response_format": {"type": "json_object"}
        }))
        .send()
        .await
        .map_err(|e| ApiError::internal(format!("LLM request failed: {e}")))?;

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ApiError::internal(format!("LLM parse failed: {e}")))?;

    let content = body["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("{}");

    // Parse LLM JSON response
    let parsed: serde_json::Value =
        serde_json::from_str(content).unwrap_or_else(|_| serde_json::json!({}));

    let label = parsed["label"].as_str().unwrap_or("未分类").to_string();
    let description = parsed["description"].as_str().map(|s| s.to_string());
    let attributes = parsed
        .get("attributes")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));

    Ok((label, description, attributes))
}

/// Call LLM to extract structured setting rules from text passages.
async fn call_llm_for_rules(
    state: &AppState,
    text: &str,
    domain: &str,
) -> ApiResult<Vec<serde_json::Value>> {
    let api_key = std::env::var("AI_API_KEY")
        .or_else(|_| std::env::var("DEEPSEEK_API_KEY"))
        .map_err(|_| ApiError::internal("AI_API_KEY not set"))?;
    let base_url =
        std::env::var("AI_API_BASE").unwrap_or_else(|_| "https://api.deepseek.com/v1".into());
    let model = std::env::var("AI_MODEL").unwrap_or_else(|_| "deepseek-chat".into());

    let system_prompt = format!(
        r#"你是一个小说设定提取专家，当前关注领域：{domain}。
从文本中提取设定规则，每条规则包含：主体、谓词、客体、属性、约束。

输出严格JSON数组格式：
[{{
    "subject_type": "character/faction/location/artifact/mechanism/setting",
  "subject_label": "主体名称",
    "predicate": "controls/teaches/conflicts_with/belongs_to/transforms/defines/enables/limits",
    "object_type": "character/faction/location/artifact/mechanism/setting",
  "object_label": "客体名称",
  "properties": {{
    "domain": "设定领域/null",
    "mechanism": "核心机制/null",
    "tone": "叙事调性/null"
  }},
  "constraints": ["约束条件1"],
  "confidence": 0.0-1.0
}}]

如果文本中没有明确的设定规则，返回空数组 []。"#
    );

    let resp = state
        .http_client
        .post(format!("{base_url}/chat/completions"))
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": model,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": format!("提取以下文本中的设定规则：\n\n{text}")}
            ],
            "temperature": 0.2,
            "max_tokens": 1000,
            "response_format": {"type": "json_object"}
        }))
        .send()
        .await
        .map_err(|e| ApiError::internal(format!("LLM request failed: {e}")))?;

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ApiError::internal(format!("LLM parse failed: {e}")))?;

    let content = body["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("[]");

    // Try to parse as array or object with "rules" key
    let rules: Vec<serde_json::Value> =
        if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(content) {
            arr
        } else if let Ok(obj) = serde_json::from_str::<serde_json::Value>(content) {
            obj.get("rules")
                .and_then(|r| r.as_array())
                .cloned()
                .unwrap_or_default()
        } else {
            vec![]
        };

    Ok(rules)
}

/// Fetch vectors from Qdrant with optional book_id filter
async fn fetch_qdrant_vectors(
    state: &AppState,
    book_id: Option<Uuid>,
    limit: usize,
) -> ApiResult<Vec<QdrantChunk>> {
    let qdrant_url = std::env::var("QDRANT_URL").unwrap_or_else(|_| "http://localhost:6333".into());
    let client = &state.http_client;

    let mut all_chunks = Vec::new();
    let mut offset: Option<serde_json::Value> = None;
    let batch_size = 100;

    loop {
        let mut body = serde_json::json!({
            "limit": batch_size,
            "with_payload": true,
            "with_vector": true,
        });

        if let Some(ref off) = offset {
            body["offset"] = off.clone();
        }

        if let Some(bid) = book_id {
            body["filter"] = serde_json::json!({
                "must": [{"key": "book_id", "match": {"value": bid.to_string()}}]
            });
        }

        let resp = client
            .post(format!(
                "{qdrant_url}/collections/nova_chunks/points/scroll"
            ))
            .json(&body)
            .send()
            .await
            .map_err(|e| ApiError::internal(format!("Qdrant scroll failed: {e}")))?;

        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ApiError::internal(format!("Qdrant parse failed: {e}")))?;

        let empty_arr = vec![];
        let points = data["result"]["points"].as_array().unwrap_or(&empty_arr);
        if points.is_empty() {
            break;
        }

        for point in points {
            let point_id = point["id"].as_u64().unwrap_or(0);
            let vector: Vec<f32> = point["vector"]
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .map(|v| v.as_f64().unwrap_or(0.0) as f32)
                .collect();
            let payload = &point["payload"];

            all_chunks.push(QdrantChunk {
                point_id,
                vector,
                book_id: payload["book_id"]
                    .as_str()
                    .and_then(|s| Uuid::parse_str(s).ok())
                    .unwrap_or_default(),
                book_title: payload["book_title"].as_str().unwrap_or("").to_string(),
                chapter_index: payload["chapter_index"].as_i64().unwrap_or(0) as i32,
                chunk_index: payload["chunk_index"].as_i64().unwrap_or(0) as i32,
                text: payload["text"]
                    .as_str()
                    .or_else(|| payload["content"].as_str())
                    .unwrap_or("")
                    .to_string(),
            });
        }

        // Check if we have enough or if there's a next page
        if all_chunks.len() >= limit {
            break;
        }

        offset = data["result"]["next_page_offset"].clone().into();
        if offset.as_ref().map_or(true, |o| o.is_null()) {
            break;
        }
    }

    Ok(all_chunks)
}

struct QdrantChunk {
    point_id: u64,
    vector: Vec<f32>,
    book_id: Uuid,
    book_title: String,
    chapter_index: i32,
    chunk_index: i32,
    text: String,
}

// ─── Ontology Tree Endpoints ─────────────────────────────────────────────────

/// GET /ontology/tree — Get the full ontology tree (or filtered by domain)
async fn get_ontology_tree(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(params): Query<TreeQuery>,
) -> ApiResult<Json<TropeTreeResponse>> {
    let max_depth = params.max_depth.unwrap_or(10);
    let visible_books = visible_trope_book_ids(&state, &auth, None, LibraryAccess::Read).await?;
    if matches!(visible_books, Some(ref ids) if ids.is_empty()) {
        return Ok(Json(TropeTreeResponse {
            nodes: Vec::new(),
            total_chunks: 0,
            max_depth: 0,
        }));
    }
    let visible_ids = visible_books.as_deref();

    let nodes: Vec<TropeNode> = match (params.domain.as_deref(), visible_ids) {
        (Some(domain), Some(book_ids)) => sqlx::query_as(
            "SELECT DISTINCT tn.id, tn.parent_id, tn.label, tn.description, tn.level, tn.cluster_size, tn.stability, \
             tn.attributes, tn.domain, tn.is_leaf, tn.created_at \
             FROM trope_nodes tn \
             JOIN trope_chunk_assignments tca ON tca.trope_node_id = tn.id \
             WHERE tca.book_id = ANY($1::uuid[]) AND tn.domain = $2 AND tn.level <= $3 \
             ORDER BY tn.level, tn.cluster_size DESC",
        )
        .bind(book_ids)
        .bind(domain)
        .bind(max_depth)
        .fetch_all(&state.db)
        .await?,
        (None, Some(book_ids)) => sqlx::query_as(
            "SELECT DISTINCT tn.id, tn.parent_id, tn.label, tn.description, tn.level, tn.cluster_size, tn.stability, \
             tn.attributes, tn.domain, tn.is_leaf, tn.created_at \
             FROM trope_nodes tn \
             JOIN trope_chunk_assignments tca ON tca.trope_node_id = tn.id \
             WHERE tca.book_id = ANY($1::uuid[]) AND tn.level <= $2 \
             ORDER BY tn.level, tn.cluster_size DESC",
        )
        .bind(book_ids)
        .bind(max_depth)
        .fetch_all(&state.db)
        .await?,
        (Some(domain), None) => sqlx::query_as(
            "SELECT id, parent_id, label, description, level, cluster_size, stability, \
             attributes, domain, is_leaf, created_at \
             FROM trope_nodes WHERE domain = $1 AND level <= $2 \
             ORDER BY level, cluster_size DESC",
        )
        .bind(domain)
        .bind(max_depth)
        .fetch_all(&state.db)
        .await?,
        (None, None) => sqlx::query_as(
            "SELECT id, parent_id, label, description, level, cluster_size, stability, \
             attributes, domain, is_leaf, created_at \
             FROM trope_nodes WHERE level <= $1 \
             ORDER BY level, cluster_size DESC",
        )
        .bind(max_depth)
        .fetch_all(&state.db)
        .await?,
    };

    let total_chunks: (i64,) = if let Some(book_ids) = visible_ids {
        sqlx::query_as(
            "SELECT COUNT(*) FROM trope_chunk_assignments WHERE book_id = ANY($1::uuid[])",
        )
        .bind(book_ids)
        .fetch_one(&state.db)
        .await?
    } else {
        sqlx::query_as("SELECT COUNT(*) FROM trope_chunk_assignments")
            .fetch_one(&state.db)
            .await?
    };

    let tree_max_depth = nodes.iter().map(|node| node.level).max().unwrap_or(0);

    // Enrich nodes with children count and sample texts
    let mut enriched = Vec::with_capacity(nodes.len());
    for node in nodes {
        let (children_count,): (i64,) = if let Some(book_ids) = visible_ids {
            sqlx::query_as(
                "SELECT COUNT(DISTINCT child.id) \
                 FROM trope_nodes child \
                 JOIN trope_chunk_assignments tca ON tca.trope_node_id = child.id \
                 WHERE child.parent_id = $1 AND tca.book_id = ANY($2::uuid[])",
            )
            .bind(node.id)
            .bind(book_ids)
            .fetch_one(&state.db)
            .await?
        } else {
            sqlx::query_as("SELECT COUNT(*) FROM trope_nodes WHERE parent_id = $1")
                .bind(node.id)
                .fetch_one(&state.db)
                .await?
        };

        // Get 3 sample texts from this cluster
        let samples: Vec<(String,)> = if let Some(book_ids) = visible_ids {
            sqlx::query_as(
                "SELECT tca.qdrant_point_id::TEXT FROM trope_chunk_assignments tca \
                 WHERE tca.trope_node_id = $1 AND tca.book_id = ANY($2::uuid[]) \
                 ORDER BY tca.membership_score DESC LIMIT 3",
            )
            .bind(node.id)
            .bind(book_ids)
            .fetch_all(&state.db)
            .await
            .unwrap_or_default()
        } else {
            sqlx::query_as(
                "SELECT tca.qdrant_point_id::TEXT FROM trope_chunk_assignments tca \
                 WHERE tca.trope_node_id = $1 ORDER BY tca.membership_score DESC LIMIT 3",
            )
            .bind(node.id)
            .fetch_all(&state.db)
            .await
            .unwrap_or_default()
        };

        enriched.push(TropeNodeWithChildren {
            node,
            children_count,
            sample_texts: samples.into_iter().map(|(s,)| s).collect(),
        });
    }

    Ok(Json(TropeTreeResponse {
        nodes: enriched,
        total_chunks: total_chunks.0,
        max_depth: tree_max_depth,
    }))
}

/// GET /ontology/tree/{node_id} — Get details of a specific node
async fn get_node_detail(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(node_id): Path<Uuid>,
) -> ApiResult<Json<TropeNode>> {
    let visible_books = visible_trope_book_ids(&state, &auth, None, LibraryAccess::Read).await?;
    if matches!(visible_books, Some(ref ids) if ids.is_empty()) {
        return Err(ApiError::not_found("Trope node not found"));
    }

    let node: TropeNode = if let Some(book_ids) = visible_books.as_deref() {
        sqlx::query_as(
            "SELECT tn.id, tn.parent_id, tn.label, tn.description, tn.level, tn.cluster_size, tn.stability, \
             tn.attributes, tn.domain, tn.is_leaf, tn.created_at \
             FROM trope_nodes tn \
             WHERE tn.id = $1 AND EXISTS ( \
                SELECT 1 FROM trope_chunk_assignments tca \
                WHERE tca.trope_node_id = tn.id AND tca.book_id = ANY($2::uuid[]) \
             )",
        )
        .bind(node_id)
        .bind(book_ids)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| ApiError::not_found("Trope node not found"))?
    } else {
        sqlx::query_as(
            "SELECT id, parent_id, label, description, level, cluster_size, stability, \
             attributes, domain, is_leaf, created_at \
             FROM trope_nodes WHERE id = $1",
        )
        .bind(node_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| ApiError::not_found("Trope node not found"))?
    };

    Ok(Json(node))
}

/// GET /ontology/tree/{node_id}/children — Get children of a node
async fn get_node_children(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(node_id): Path<Uuid>,
) -> ApiResult<Json<Vec<TropeNode>>> {
    let visible_books = visible_trope_book_ids(&state, &auth, None, LibraryAccess::Read).await?;
    if matches!(visible_books, Some(ref ids) if ids.is_empty()) {
        return Ok(Json(Vec::new()));
    }

    let children: Vec<TropeNode> = if let Some(book_ids) = visible_books.as_deref() {
        sqlx::query_as(
            "SELECT DISTINCT tn.id, tn.parent_id, tn.label, tn.description, tn.level, tn.cluster_size, tn.stability, \
             tn.attributes, tn.domain, tn.is_leaf, tn.created_at \
             FROM trope_nodes tn \
             JOIN trope_chunk_assignments tca ON tca.trope_node_id = tn.id \
             WHERE tn.parent_id = $1 AND tca.book_id = ANY($2::uuid[]) \
             ORDER BY tn.cluster_size DESC",
        )
        .bind(node_id)
        .bind(book_ids)
        .fetch_all(&state.db)
        .await?
    } else {
        sqlx::query_as(
            "SELECT id, parent_id, label, description, level, cluster_size, stability, \
             attributes, domain, is_leaf, created_at \
             FROM trope_nodes WHERE parent_id = $1 \
             ORDER BY cluster_size DESC",
        )
        .bind(node_id)
        .fetch_all(&state.db)
        .await?
    };

    Ok(Json(children))
}

/// GET /ontology/tree/{node_id}/chunks — Get chunks assigned to this node
async fn get_node_chunks(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(node_id): Path<Uuid>,
    Query(params): Query<PaginationQuery>,
) -> ApiResult<Json<Vec<ChunkAssignment>>> {
    let limit = params.limit.unwrap_or(20).min(100);
    let offset = params.offset.unwrap_or(0);
    let visible_books = visible_trope_book_ids(&state, &auth, None, LibraryAccess::Read).await?;
    if matches!(visible_books, Some(ref ids) if ids.is_empty()) {
        return Ok(Json(Vec::new()));
    }

    let chunks: Vec<ChunkAssignment> = if let Some(book_ids) = visible_books.as_deref() {
        sqlx::query_as(
            "SELECT tca.id, tca.book_id, tca.chapter_index, tca.chunk_index, \
             tca.membership_score, tca.qdrant_point_id \
             FROM trope_chunk_assignments tca \
             WHERE tca.trope_node_id = $1 AND tca.book_id = ANY($2::uuid[]) \
             ORDER BY tca.membership_score DESC \
             LIMIT $3 OFFSET $4",
        )
        .bind(node_id)
        .bind(book_ids)
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.db)
        .await?
    } else {
        sqlx::query_as(
            "SELECT tca.id, tca.book_id, tca.chapter_index, tca.chunk_index, \
             tca.membership_score, tca.qdrant_point_id \
             FROM trope_chunk_assignments tca \
             WHERE tca.trope_node_id = $1 \
             ORDER BY tca.membership_score DESC \
             LIMIT $2 OFFSET $3",
        )
        .bind(node_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.db)
        .await?
    };

    Ok(Json(chunks))
}

#[derive(Deserialize)]
struct PaginationQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Serialize, sqlx::FromRow)]
struct ChunkAssignment {
    id: Uuid,
    book_id: Uuid,
    chapter_index: i32,
    chunk_index: i32,
    membership_score: f64,
    qdrant_point_id: i64,
}

// ─── Manual CRUD Endpoints ───────────────────────────────────────────────────

/// POST /ontology/nodes — Create a new node (user-defined concept)
///
/// The user provides a label + optional description + reference texts.
/// System automatically computes the embedding vector for matching.
async fn create_node(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<CreateNodeRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_trope_manage_access(&state, &auth).await?;

    // Compute embedding from label + description + reference texts
    let embed_text = build_embed_text(
        &req.label,
        req.description.as_deref(),
        req.reference_texts.as_deref(),
    );
    let embedding = embed_text_single(&state, &embed_text).await?;
    let centroid_bytes = vector_to_bytes(&embedding);

    // Determine level based on parent
    let level = if let Some(parent_id) = req.parent_id {
        let (parent_level,): (i32,) = sqlx::query_as("SELECT level FROM trope_nodes WHERE id = $1")
            .bind(parent_id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| ApiError::not_found("Parent node not found"))?;
        parent_level + 1
    } else {
        0
    };

    let node_id = Uuid::new_v4();
    let domain = req.domain.unwrap_or_else(|| "general".to_string());

    sqlx::query(
        "INSERT INTO trope_nodes (id, parent_id, label, description, level, centroid, \
         cluster_size, stability, domain, is_leaf, attributes) \
         VALUES ($1, $2, $3, $4, $5, $6, 0, 1.0, $7, true, $8)",
    )
    .bind(node_id)
    .bind(req.parent_id)
    .bind(&req.label)
    .bind(&req.description)
    .bind(level)
    .bind(&centroid_bytes)
    .bind(&domain)
    .bind(serde_json::json!({
        "reference_texts": req.reference_texts.as_deref().unwrap_or(&[]),
        "user_created": true,
    }))
    .execute(&state.db)
    .await?;

    // Update parent's is_leaf flag
    if let Some(parent_id) = req.parent_id {
        sqlx::query("UPDATE trope_nodes SET is_leaf = false WHERE id = $1")
            .bind(parent_id)
            .execute(&state.db)
            .await?;
    }

    // Log event
    sqlx::query(
        "INSERT INTO ontology_events (event_type, trope_node_id, details, triggered_by) \
         VALUES ('node_created', $1, $2, 'manual')",
    )
    .bind(node_id)
    .bind(serde_json::json!({"label": req.label, "parent_id": req.parent_id}))
    .execute(&state.db)
    .await?;

    Ok(Json(serde_json::json!({
        "id": node_id,
        "label": req.label,
        "level": level,
        "domain": domain,
        "embedding_computed": true,
    })))
}

/// PUT /ontology/nodes/{node_id} — Update a node's label/description/domain
async fn update_node(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(node_id): Path<Uuid>,
    Json(req): Json<UpdateNodeRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_trope_manage_access(&state, &auth).await?;

    // Fetch current node
    let current: (String, Option<String>) =
        sqlx::query_as("SELECT label, description FROM trope_nodes WHERE id = $1")
            .bind(node_id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| ApiError::not_found("Node not found"))?;

    let new_label = req.label.as_deref().unwrap_or(&current.0);
    let new_desc = req.description.as_deref().or(current.1.as_deref());

    // Update basic fields
    sqlx::query(
        "UPDATE trope_nodes SET \
         label = COALESCE($1, label), \
         description = COALESCE($2, description), \
         domain = COALESCE($3, domain), \
         updated_at = now() \
         WHERE id = $4",
    )
    .bind(req.label.as_deref())
    .bind(req.description.as_deref())
    .bind(req.domain.as_deref())
    .bind(node_id)
    .execute(&state.db)
    .await?;

    // Recompute embedding if requested (default: true when label/desc changes)
    let should_recompute = req.recompute_embedding.unwrap_or(true);
    if should_recompute
        && (req.label.is_some() || req.description.is_some() || req.reference_texts.is_some())
    {
        let embed_text = build_embed_text(new_label, new_desc, req.reference_texts.as_deref());
        let embedding = embed_text_single(&state, &embed_text).await?;
        let centroid_bytes = vector_to_bytes(&embedding);

        sqlx::query("UPDATE trope_nodes SET centroid = $1 WHERE id = $2")
            .bind(&centroid_bytes)
            .bind(node_id)
            .execute(&state.db)
            .await?;
    }

    Ok(Json(serde_json::json!({
        "id": node_id,
        "updated": true,
        "embedding_recomputed": should_recompute,
    })))
}

/// DELETE /ontology/nodes/{node_id} — Delete a node (and optionally its subtree)
async fn delete_node(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(node_id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_trope_manage_access(&state, &auth).await?;

    // Check for children — re-parent them to this node's parent
    let (parent_id,): (Option<Uuid>,) =
        sqlx::query_as("SELECT parent_id FROM trope_nodes WHERE id = $1")
            .bind(node_id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| ApiError::not_found("Node not found"))?;

    // Re-parent children to the deleted node's parent
    let children_moved = sqlx::query("UPDATE trope_nodes SET parent_id = $1 WHERE parent_id = $2")
        .bind(parent_id)
        .bind(node_id)
        .execute(&state.db)
        .await?
        .rows_affected();

    // Delete chunk assignments
    sqlx::query("DELETE FROM trope_chunk_assignments WHERE trope_node_id = $1")
        .bind(node_id)
        .execute(&state.db)
        .await?;

    // Delete the node itself
    sqlx::query("DELETE FROM trope_nodes WHERE id = $1")
        .bind(node_id)
        .execute(&state.db)
        .await?;

    // Log event
    sqlx::query(
        "INSERT INTO ontology_events (event_type, trope_node_id, details, triggered_by) \
         VALUES ('node_deleted', $1, $2, 'manual')",
    )
    .bind(node_id)
    .bind(serde_json::json!({"children_reparented": children_moved}))
    .execute(&state.db)
    .await?;

    Ok(Json(serde_json::json!({
        "deleted": node_id,
        "children_reparented": children_moved,
    })))
}

/// POST /ontology/nodes/{node_id}/move — Move a node to a new parent (drag & drop)
async fn move_node(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(node_id): Path<Uuid>,
    Json(req): Json<MoveNodeRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_trope_manage_access(&state, &auth).await?;

    // Prevent circular references: ensure new_parent is not a descendant of node_id
    if let Some(new_parent_id) = req.new_parent_id {
        if new_parent_id == node_id {
            return Err(ApiError::bad_request("Cannot move a node under itself"));
        }
        // Walk up from new_parent to check it's not under node_id
        let mut current = Some(new_parent_id);
        while let Some(cur_id) = current {
            let parent: Option<(Option<Uuid>,)> =
                sqlx::query_as("SELECT parent_id FROM trope_nodes WHERE id = $1")
                    .bind(cur_id)
                    .fetch_optional(&state.db)
                    .await?;
            match parent {
                Some((Some(pid),)) if pid == node_id => {
                    return Err(ApiError::bad_request(
                        "Circular reference: target is a descendant",
                    ));
                }
                Some((pid,)) => current = pid,
                None => break,
            }
        }
    }

    // Compute new level
    let new_level = if let Some(parent_id) = req.new_parent_id {
        let (parent_level,): (i32,) = sqlx::query_as("SELECT level FROM trope_nodes WHERE id = $1")
            .bind(parent_id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| ApiError::not_found("Target parent not found"))?;
        parent_level + 1
    } else {
        0
    };

    sqlx::query(
        "UPDATE trope_nodes SET parent_id = $1, level = $2, updated_at = now() WHERE id = $3",
    )
    .bind(req.new_parent_id)
    .bind(new_level)
    .bind(node_id)
    .execute(&state.db)
    .await?;

    // Recursively update children's levels
    update_children_levels(&state.db, node_id, new_level).await?;

    Ok(Json(serde_json::json!({
        "moved": node_id,
        "new_parent": req.new_parent_id,
        "new_level": new_level,
    })))
}

/// Recursively update levels of all descendants.
async fn update_children_levels(
    db: &sqlx::PgPool,
    parent_id: Uuid,
    parent_level: i32,
) -> ApiResult<()> {
    let children: Vec<(Uuid,)> = sqlx::query_as("SELECT id FROM trope_nodes WHERE parent_id = $1")
        .bind(parent_id)
        .fetch_all(db)
        .await?;

    for (child_id,) in children {
        let child_level = parent_level + 1;
        sqlx::query("UPDATE trope_nodes SET level = $1 WHERE id = $2")
            .bind(child_level)
            .bind(child_id)
            .execute(db)
            .await?;
        // Recurse (box pin not needed for shallow trees)
        Box::pin(update_children_levels(db, child_id, child_level)).await?;
    }
    Ok(())
}

/// POST /ontology/nodes/{node_id}/merge — Merge another node into this one
async fn merge_nodes(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(target_id): Path<Uuid>,
    Json(req): Json<MergeNodesRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_trope_manage_access(&state, &auth).await?;

    let source_id = req.source_node_id;

    if source_id == target_id {
        return Err(ApiError::bad_request("Cannot merge a node with itself"));
    }

    // Move source's children to target
    let children_moved = sqlx::query("UPDATE trope_nodes SET parent_id = $1 WHERE parent_id = $2")
        .bind(target_id)
        .bind(source_id)
        .execute(&state.db)
        .await?
        .rows_affected();

    // Move source's chunk assignments to target
    let chunks_moved = sqlx::query(
        "UPDATE trope_chunk_assignments SET trope_node_id = $1 WHERE trope_node_id = $2",
    )
    .bind(target_id)
    .bind(source_id)
    .execute(&state.db)
    .await?
    .rows_affected();

    // Delete source node
    sqlx::query("DELETE FROM trope_nodes WHERE id = $1")
        .bind(source_id)
        .execute(&state.db)
        .await?;

    // Recompute target's cluster_size
    sqlx::query(
        "UPDATE trope_nodes SET cluster_size = (SELECT COUNT(*) FROM trope_chunk_assignments WHERE trope_node_id = $1) WHERE id = $1",
    )
    .bind(target_id)
    .execute(&state.db)
    .await?;

    // Log
    sqlx::query(
        "INSERT INTO ontology_events (event_type, trope_node_id, details, triggered_by) \
         VALUES ('node_merged', $1, $2, 'manual')",
    )
    .bind(target_id)
    .bind(serde_json::json!({"merged_from": source_id, "children_moved": children_moved, "chunks_moved": chunks_moved}))
    .execute(&state.db)
    .await?;

    Ok(Json(serde_json::json!({
        "target": target_id,
        "source_deleted": source_id,
        "children_moved": children_moved,
        "chunks_moved": chunks_moved,
    })))
}

// ─── AI-Driven: Scan & Evolve ────────────────────────────────────────────────

/// POST /ontology/nodes/{node_id}/scan — Scan the entire library for matching passages
///
/// Uses the node's embedding to search Qdrant. Supports multi-language:
/// Qwen3-Embedding-4B is multilingual, so a user-defined Chinese setting label
/// will naturally match Japanese passages about "憑依する時の快感".
async fn scan_node(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(node_id): Path<Uuid>,
    Json(req): Json<ScanNodeRequest>,
) -> ApiResult<Json<ScanResult>> {
    let limit = req.limit.unwrap_or(50).min(200);
    let threshold = req.threshold.unwrap_or(0.45);
    let allowed_book_ids =
        visible_trope_book_ids(&state, &auth, req.book_ids.as_deref(), LibraryAccess::Write)
            .await?;
    if matches!(allowed_book_ids, Some(ref ids) if ids.is_empty()) {
        return Ok(Json(ScanResult {
            node_id,
            matched_chunks: Vec::new(),
            total_matched: 0,
            languages_found: Vec::new(),
        }));
    }
    let allowed_book_ids = allowed_book_ids.as_deref();
    let allowed_book_set = allowed_book_id_set(allowed_book_ids);

    // Get node's centroid vector
    let (centroid_bytes,): (Option<Vec<u8>>,) =
        sqlx::query_as("SELECT centroid FROM trope_nodes WHERE id = $1")
            .bind(node_id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| ApiError::not_found("Node not found"))?;

    let centroid = centroid_bytes.ok_or_else(|| {
        ApiError::bad_request("Node has no embedding. Update it with a description first.")
    })?;
    let vector = bytes_to_vector(&centroid);

    // Search Qdrant
    let qdrant_url = std::env::var("QDRANT_URL").unwrap_or_else(|_| "http://localhost:6333".into());

    let filter = qdrant_book_filter(allowed_book_ids);

    let search_resp = state
        .http_client
        .post(format!(
            "{qdrant_url}/collections/nova_chunks/points/search"
        ))
        .json(&serde_json::json!({
            "vector": vector,
            "limit": limit,
            "with_payload": true,
            "score_threshold": threshold,
            "filter": filter,
        }))
        .send()
        .await
        .map_err(|e| ApiError::internal(format!("Qdrant search failed: {e}")))?;

    let search_data: serde_json::Value = search_resp
        .json()
        .await
        .map_err(|e| ApiError::internal(format!("Search parse failed: {e}")))?;

    let empty = vec![];
    let results = search_data["result"].as_array().unwrap_or(&empty);
    let mut matched_chunks = Vec::new();
    let mut languages_found = std::collections::HashSet::new();

    for result in results {
        let payload = &result["payload"];
        let text = payload["text"]
            .as_str()
            .or_else(|| payload["content"].as_str())
            .unwrap_or("");
        let score = result["score"].as_f64().unwrap_or(0.0) as f32;
        let point_id = result["id"].as_u64().unwrap_or(0) as i64;

        // Simple language detection heuristic
        let lang = detect_language(text);
        languages_found.insert(lang.clone());

        let Some(book_id) = qdrant_payload_book_id(payload) else {
            continue;
        };
        if let Some(ref allowed_book_set) = allowed_book_set {
            if !allowed_book_set.contains(&book_id) {
                continue;
            }
        }

        matched_chunks.push(MatchedChunk {
            point_id,
            book_id,
            book_title: payload["book_title"].as_str().unwrap_or("").to_string(),
            chapter_index: payload["chapter_index"].as_i64().unwrap_or(0) as i32,
            chunk_index: payload["chunk_index"].as_i64().unwrap_or(0) as i32,
            text: text.chars().take(500).collect(),
            score,
            language: Some(lang),
        });
    }

    // Store assignments for high-confidence matches
    for chunk in &matched_chunks {
        if chunk.score > 0.55 {
            let _ = sqlx::query(
                "INSERT INTO trope_chunk_assignments \
                 (trope_node_id, book_id, chapter_index, chunk_index, qdrant_point_id, membership_score) \
                 VALUES ($1, $2, $3, $4, $5, $6) \
                 ON CONFLICT (qdrant_point_id) DO UPDATE SET \
                 trope_node_id = $1, membership_score = $6",
            )
            .bind(node_id)
            .bind(chunk.book_id)
            .bind(chunk.chapter_index)
            .bind(chunk.chunk_index)
            .bind(chunk.point_id)
            .bind(chunk.score as f64)
            .execute(&state.db)
            .await;
        }
    }

    // Update cluster_size
    sqlx::query(
        "UPDATE trope_nodes SET cluster_size = (SELECT COUNT(*) FROM trope_chunk_assignments WHERE trope_node_id = $1) WHERE id = $1",
    )
    .bind(node_id)
    .execute(&state.db)
    .await?;

    let total = matched_chunks.len();
    Ok(Json(ScanResult {
        node_id,
        matched_chunks,
        total_matched: total,
        languages_found: languages_found.into_iter().collect(),
    }))
}

/// POST /ontology/nodes/{node_id}/evolve — AI-discover sub-concepts from matched chunks
///
/// Algorithm:
/// 1. Get all chunks assigned to this node
/// 2. Fetch their vectors from Qdrant
/// 3. Use Qdrant Distance Matrix to find sub-clusters within this node's space
/// 4. For each sub-cluster, use LLM to generate a label
/// 5. Create child nodes automatically
async fn evolve_node(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(node_id): Path<Uuid>,
    Json(req): Json<EvolveNodeRequest>,
) -> ApiResult<Json<EvolveResult>> {
    ensure_trope_manage_access(&state, &auth).await?;

    let max_children = req.max_children.unwrap_or(5);
    let min_evidence = req.min_evidence.unwrap_or(3);
    let writable_book_ids =
        visible_trope_book_ids(&state, &auth, None, LibraryAccess::Write).await?;
    if matches!(writable_book_ids, Some(ref ids) if ids.is_empty()) {
        return Err(ApiError::forbidden());
    }

    // Get all chunk point IDs assigned to this node
    let chunk_ids: Vec<(i64,)> = if let Some(book_ids) = writable_book_ids.as_deref() {
        sqlx::query_as(
            "SELECT qdrant_point_id FROM trope_chunk_assignments \
             WHERE trope_node_id = $1 AND book_id = ANY($2::uuid[]) \
             ORDER BY membership_score DESC LIMIT 200",
        )
        .bind(node_id)
        .bind(book_ids)
        .fetch_all(&state.db)
        .await?
    } else {
        sqlx::query_as(
            "SELECT qdrant_point_id FROM trope_chunk_assignments \
             WHERE trope_node_id = $1 ORDER BY membership_score DESC LIMIT 200",
        )
        .bind(node_id)
        .fetch_all(&state.db)
        .await?
    };

    if chunk_ids.len() < min_evidence * 2 {
        return Err(ApiError::bad_request(format!(
            "Not enough evidence to evolve. Need at least {} chunks, have {}. Run 'scan' first.",
            min_evidence * 2,
            chunk_ids.len()
        )));
    }

    // Fetch vectors from Qdrant for these specific points
    let qdrant_url = std::env::var("QDRANT_URL").unwrap_or_else(|_| "http://localhost:6333".into());
    let point_ids: Vec<u64> = chunk_ids.iter().map(|(id,)| *id as u64).collect();

    let points_resp = state
        .http_client
        .post(format!("{qdrant_url}/collections/nova_chunks/points"))
        .json(&serde_json::json!({
            "ids": point_ids,
            "with_payload": true,
            "with_vector": true,
        }))
        .send()
        .await
        .map_err(|e| ApiError::internal(format!("Qdrant fetch failed: {e}")))?;

    let points_data: serde_json::Value = points_resp
        .json()
        .await
        .map_err(|e| ApiError::internal(format!("Points parse failed: {e}")))?;

    let empty = vec![];
    let points = points_data["result"].as_array().unwrap_or(&empty);

    // Collect vectors and texts
    let mut vectors: Vec<Vec<f32>> = Vec::new();
    let mut texts: Vec<String> = Vec::new();

    for point in points {
        let vec: Vec<f32> = point["vector"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .map(|v| v.as_f64().unwrap_or(0.0) as f32)
            .collect();
        let text = point["payload"]["text"]
            .as_str()
            .or_else(|| point["payload"]["content"].as_str())
            .unwrap_or("")
            .to_string();

        if !vec.is_empty() && !text.is_empty() {
            vectors.push(vec);
            texts.push(text);
        }
    }

    if vectors.len() < min_evidence * 2 {
        return Err(ApiError::bad_request(
            "Not enough valid vectors for evolution",
        ));
    }

    // Simple k-means-like clustering using cosine similarity
    // (For the sub-space within a single node, k-means works well since we know max_children)
    let k = max_children.min(vectors.len() / min_evidence);
    let clusters = simple_kmeans_cosine(&vectors, k, 10);

    // For each cluster, gather sample texts and ask LLM to label
    let mut proposed_children = Vec::new();
    let parent_label: (String,) = sqlx::query_as("SELECT label FROM trope_nodes WHERE id = $1")
        .bind(node_id)
        .fetch_one(&state.db)
        .await?;

    for (cluster_idx, cluster_members) in clusters.iter().enumerate() {
        if cluster_members.len() < min_evidence {
            continue;
        }

        // Get sample texts from this sub-cluster
        let sample_texts: Vec<&str> = cluster_members
            .iter()
            .take(5)
            .filter_map(|&idx| texts.get(idx).map(|s| s.as_str()))
            .collect();

        if sample_texts.is_empty() {
            continue;
        }

        let combined = sample_texts.join("\n---\n");

        // Ask LLM to label this sub-cluster in context of the parent
        let (label, description, _attrs) =
            match call_llm_for_sublabel(&state, &parent_label.0, &combined).await {
                Ok(r) => r,
                Err(_) => {
                    let truncated: String = sample_texts[0].chars().take(20).collect();
                    (
                        format!("子类{}: {truncated}", cluster_idx + 1),
                        None,
                        serde_json::json!({}),
                    )
                }
            };

        // Auto-create the child node
        let child_id = Uuid::new_v4();
        let embed_text = build_embed_text(&label, description.as_deref(), None);
        let child_embedding = embed_text_single(&state, &embed_text)
            .await
            .unwrap_or_default();
        let centroid_bytes = if child_embedding.is_empty() {
            // Fallback: use mean of cluster vectors
            let cluster_vecs: Vec<Vec<f32>> = cluster_members
                .iter()
                .filter_map(|&idx| vectors.get(idx).cloned())
                .collect();
            let centroid = compute_centroid(&cluster_vecs);
            vector_to_bytes(&centroid)
        } else {
            vector_to_bytes(&child_embedding)
        };

        let (parent_level,): (i32,) = sqlx::query_as("SELECT level FROM trope_nodes WHERE id = $1")
            .bind(node_id)
            .fetch_one(&state.db)
            .await?;

        sqlx::query(
            "INSERT INTO trope_nodes (id, parent_id, label, description, level, centroid, \
             cluster_size, stability, domain, is_leaf, attributes) \
             SELECT $1, $2, $3, $4, $5, $6, $7, 1.0, domain, true, $8 \
             FROM trope_nodes WHERE id = $2",
        )
        .bind(child_id)
        .bind(node_id)
        .bind(&label)
        .bind(&description)
        .bind(parent_level + 1)
        .bind(&centroid_bytes)
        .bind(cluster_members.len() as i32)
        .bind(serde_json::json!({"auto_evolved": true, "evidence_count": cluster_members.len()}))
        .execute(&state.db)
        .await?;

        // Assign matching chunks to this child
        for &member_idx in cluster_members {
            if let Some(point_id) = point_ids.get(member_idx) {
                let _ = sqlx::query(
                    "UPDATE trope_chunk_assignments SET trope_node_id = $1 \
                     WHERE qdrant_point_id = $2 AND trope_node_id = $3",
                )
                .bind(child_id)
                .bind(*point_id as i64)
                .bind(node_id)
                .execute(&state.db)
                .await;
            }
        }

        proposed_children.push(ProposedNode {
            label: label.clone(),
            description,
            evidence_count: cluster_members.len(),
            sample_text: sample_texts.first().unwrap_or(&"").to_string(),
            auto_created: true,
            created_id: Some(child_id),
        });
    }

    // Update parent's is_leaf
    if !proposed_children.is_empty() {
        sqlx::query("UPDATE trope_nodes SET is_leaf = false WHERE id = $1")
            .bind(node_id)
            .execute(&state.db)
            .await?;
    }

    // Log evolution event
    sqlx::query(
        "INSERT INTO ontology_events (event_type, trope_node_id, details, triggered_by) \
         VALUES ('node_evolved', $1, $2, 'ai')",
    )
    .bind(node_id)
    .bind(serde_json::json!({
        "children_created": proposed_children.len(),
        "total_evidence": vectors.len(),
    }))
    .execute(&state.db)
    .await?;

    let total_evidence = vectors.len();
    Ok(Json(EvolveResult {
        parent_node_id: node_id,
        proposed_children,
        evidence_chunks: total_evidence,
    }))
}

/// Simple language detection based on Unicode ranges
fn detect_language(text: &str) -> String {
    let sample: String = text.chars().take(200).collect();
    let mut cjk = 0u32;
    let mut hiragana = 0u32;
    let mut katakana = 0u32;
    let mut hangul = 0u32;
    let mut latin = 0u32;

    for c in sample.chars() {
        match c {
            '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}' => cjk += 1,
            '\u{3040}'..='\u{309F}' => hiragana += 1,
            '\u{30A0}'..='\u{30FF}' => katakana += 1,
            '\u{AC00}'..='\u{D7AF}' | '\u{1100}'..='\u{11FF}' => hangul += 1,
            'a'..='z' | 'A'..='Z' => latin += 1,
            _ => {}
        }
    }

    if hiragana + katakana > 5 {
        "ja".to_string()
    } else if hangul > 5 {
        "ko".to_string()
    } else if cjk > latin {
        "zh".to_string()
    } else if latin > 0 {
        "en".to_string()
    } else {
        "unknown".to_string()
    }
}

/// Build text for embedding from label + description + reference texts
fn build_embed_text(
    label: &str,
    description: Option<&str>,
    reference_texts: Option<&[String]>,
) -> String {
    let mut parts = vec![label.to_string()];
    if let Some(desc) = description {
        parts.push(desc.to_string());
    }
    if let Some(refs) = reference_texts {
        for r in refs.iter().take(3) {
            parts.push(r.clone());
        }
    }
    parts.join(" | ")
}

/// Embed a single text (wrapper using the existing embed_text function)
async fn embed_text_single(state: &AppState, text: &str) -> ApiResult<Vec<f32>> {
    embed_text(state, text).await
}

/// Simple k-means clustering using cosine similarity.
/// Returns Vec<Vec<usize>> where each inner vec contains indices of vectors in that cluster.
fn simple_kmeans_cosine(vectors: &[Vec<f32>], k: usize, max_iter: usize) -> Vec<Vec<usize>> {
    if vectors.is_empty() || k == 0 {
        return vec![];
    }
    let k = k.min(vectors.len());
    let n = vectors.len();

    // Initialize centroids by picking evenly-spaced vectors
    let mut centroids: Vec<Vec<f32>> = (0..k).map(|i| vectors[i * n / k].clone()).collect();

    let mut assignments = vec![0usize; n];

    for _iter in 0..max_iter {
        // Assign each vector to nearest centroid
        let mut changed = false;
        for (i, vec) in vectors.iter().enumerate() {
            let mut best_cluster = 0;
            let mut best_sim = f32::NEG_INFINITY;
            for (c, centroid) in centroids.iter().enumerate() {
                let sim = cosine_similarity(vec, centroid);
                if sim > best_sim {
                    best_sim = sim;
                    best_cluster = c;
                }
            }
            if assignments[i] != best_cluster {
                assignments[i] = best_cluster;
                changed = true;
            }
        }

        if !changed {
            break;
        }

        // Recompute centroids
        for c in 0..k {
            let members: Vec<&Vec<f32>> = vectors
                .iter()
                .enumerate()
                .filter(|(i, _)| assignments[*i] == c)
                .map(|(_, v)| v)
                .collect();
            if !members.is_empty() {
                let owned: Vec<Vec<f32>> = members.into_iter().cloned().collect();
                centroids[c] = compute_centroid(&owned);
            }
        }
    }

    // Group by assignment
    let mut clusters: Vec<Vec<usize>> = vec![vec![]; k];
    for (i, &c) in assignments.iter().enumerate() {
        clusters[c].push(i);
    }
    clusters
}

/// Call LLM to label a sub-cluster in the context of its parent concept.
async fn call_llm_for_sublabel(
    state: &AppState,
    parent_label: &str,
    sample_texts: &str,
) -> ApiResult<(String, Option<String>, serde_json::Value)> {
    let api_key = std::env::var("AI_API_KEY")
        .or_else(|_| std::env::var("DEEPSEEK_API_KEY"))
        .map_err(|_| ApiError::internal("AI_API_KEY not set"))?;
    let base_url =
        std::env::var("AI_API_BASE").unwrap_or_else(|_| "https://api.deepseek.com/v1".into());
    let model = std::env::var("AI_MODEL").unwrap_or_else(|_| "deepseek-chat".into());

    let system_prompt = format!(
        r#"你是小说设定分析专家。用户有一个设定概念"{parent_label}"，以下文本是该概念下的一个子聚类。
请为这个子聚类生成：
1. 标签（10字以内，能区分于父概念的其他子类）
2. 简短描述（30字以内）

注意：文本可能是中文、日文、英文或韩文。无论语言，都请用中文输出标签和描述。

请严格输出JSON：{{"label": "...", "description": "..."}}"#
    );

    let resp = state
        .http_client
        .post(format!("{base_url}/chat/completions"))
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": model,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": format!("子聚类文本样本：\n\n{sample_texts}")}
            ],
            "temperature": 0.3,
            "max_tokens": 200,
            "response_format": {"type": "json_object"}
        }))
        .send()
        .await
        .map_err(|e| ApiError::internal(format!("LLM failed: {e}")))?;

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ApiError::internal(format!("LLM parse failed: {e}")))?;

    let content = body["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("{}");
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap_or_default();

    Ok((
        parsed["label"].as_str().unwrap_or("子概念").to_string(),
        parsed["description"].as_str().map(|s| s.to_string()),
        serde_json::json!({}),
    ))
}

/// POST /ontology/cluster — Trigger HDBSCAN clustering on embeddings
///
/// Implementation strategy:
/// 1. Fetch all vectors from Qdrant (optionally filtered by domain/books)
/// 2. Compute pairwise distance matrix using Qdrant Distance Matrix API
/// 3. Run simplified single-linkage hierarchical clustering in Rust
///    (HDBSCAN-lite: core distance → mutual reachability → MST → condensed tree)
/// 4. Extract stable clusters, compute centroids
/// 5. Use LLM to label each cluster based on representative texts
/// 6. Store results in trope_nodes + trope_chunk_assignments
async fn trigger_clustering(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<ClusterRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_trope_manage_access(&state, &auth).await?;
    let _user_id = parse_user_id(&auth)?;
    let min_cluster_size = req.min_cluster_size.unwrap_or(5);
    let _min_samples = req.min_samples.unwrap_or(3);
    let domain = req.domain.unwrap_or_else(|| "general".to_string());
    let writable_book_ids =
        visible_trope_book_ids(&state, &auth, req.book_ids.as_deref(), LibraryAccess::Write)
            .await?;
    if matches!(writable_book_ids, Some(ref ids) if ids.is_empty()) {
        return Ok(Json(serde_json::json!({
            "status": "completed",
            "clusters_found": 0,
            "nodes_created": 0,
            "total_points_processed": 0,
            "domain": domain,
        })));
    }
    let qdrant_filter = qdrant_book_filter(writable_book_ids.as_deref());
    let assignment_book_id = writable_book_ids
        .as_ref()
        .and_then(|ids| (ids.len() == 1).then_some(ids[0]))
        .unwrap_or_else(Uuid::nil);

    // Step 1: Fetch vectors from Qdrant
    // For initial implementation, we use Qdrant's Distance Matrix API
    let qdrant_url = std::env::var("QDRANT_URL").unwrap_or_else(|_| "http://localhost:6333".into());

    // Use Qdrant distance matrix for efficient pairwise computation
    let sample_size = 1000; // Start with manageable size
    let matrix_resp = state
        .http_client
        .post(format!(
            "{qdrant_url}/collections/nova_chunks/points/search/matrix/pairs"
        ))
        .json(&serde_json::json!({
            "sample": sample_size,
            "limit": min_cluster_size * 2,
            "filter": qdrant_filter
        }))
        .send()
        .await
        .map_err(|e| ApiError::internal(format!("Qdrant matrix failed: {e}")))?;

    let matrix_data: serde_json::Value = matrix_resp
        .json()
        .await
        .map_err(|e| ApiError::internal(format!("Matrix parse failed: {e}")))?;

    // Step 2: Build adjacency from distance matrix pairs
    let empty_pairs = vec![];
    let pairs = matrix_data["result"]["pairs"]
        .as_array()
        .unwrap_or(&empty_pairs);

    // Group by point, find dense neighborhoods
    let mut neighborhoods: std::collections::HashMap<u64, Vec<(u64, f64)>> =
        std::collections::HashMap::new();

    for pair in pairs {
        let a = pair["a"].as_u64().unwrap_or(0);
        let b = pair["b"].as_u64().unwrap_or(0);
        let score = pair["score"].as_f64().unwrap_or(0.0);
        neighborhoods.entry(a).or_default().push((b, score));
        neighborhoods.entry(b).or_default().push((a, score));
    }

    // Step 3: Simple density-based clustering (greedy connected components with threshold)
    // For production, this would use a proper HDBSCAN implementation.
    // Here we use a simplified approach: high-similarity connected components.
    let similarity_threshold = 0.6; // Cosine similarity threshold for same cluster
    let mut visited: std::collections::HashSet<u64> = std::collections::HashSet::new();
    let mut clusters: Vec<Vec<u64>> = Vec::new();

    for &point_id in neighborhoods.keys() {
        if visited.contains(&point_id) {
            continue;
        }

        // BFS to find connected component above threshold
        let mut cluster = Vec::new();
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(point_id);
        visited.insert(point_id);

        while let Some(current) = queue.pop_front() {
            cluster.push(current);
            if let Some(neighbors) = neighborhoods.get(&current) {
                for &(neighbor, score) in neighbors {
                    if !visited.contains(&neighbor) && score >= similarity_threshold {
                        visited.insert(neighbor);
                        queue.push_back(neighbor);
                    }
                }
            }
        }

        if cluster.len() >= min_cluster_size {
            clusters.push(cluster);
        }
    }

    // Step 4: For each cluster, fetch representative texts and use LLM to label
    let mut created_nodes = 0;
    for (idx, cluster_points) in clusters.iter().enumerate() {
        // Fetch a few representative texts from the cluster
        // (In production, retrieve from Qdrant by point IDs)
        let node_id = Uuid::new_v4();
        let label = format!("Cluster-{}-{}", domain, idx + 1); // Placeholder; LLM would label this

        sqlx::query(
            "INSERT INTO trope_nodes (id, label, level, cluster_size, stability, domain, is_leaf) \
             VALUES ($1, $2, 0, $3, $4, $5, true) \
             ON CONFLICT DO NOTHING",
        )
        .bind(node_id)
        .bind(&label)
        .bind(cluster_points.len() as i32)
        .bind(1.0) // Simplified stability
        .bind(&domain)
        .execute(&state.db)
        .await?;

        // Assign chunks to this node
        for &point_id in cluster_points {
            sqlx::query(
                "INSERT INTO trope_chunk_assignments \
                 (trope_node_id, book_id, chapter_index, chunk_index, qdrant_point_id, membership_score) \
                 VALUES ($1, $2, 0, 0, $3, 1.0) \
                 ON CONFLICT (qdrant_point_id) DO UPDATE SET trope_node_id = $1, membership_score = 1.0",
            )
            .bind(node_id)
            .bind(assignment_book_id) // Matrix pairs do not include payload metadata yet.
            .bind(point_id as i64)
            .execute(&state.db)
            .await?;
        }

        // Log evolution event
        sqlx::query(
            "INSERT INTO ontology_events (event_type, trope_node_id, details, triggered_by) \
             VALUES ('node_created', $1, $2, 'clustering')",
        )
        .bind(node_id)
        .bind(serde_json::json!({"cluster_size": cluster_points.len(), "domain": domain}))
        .execute(&state.db)
        .await?;

        created_nodes += 1;
    }

    Ok(Json(serde_json::json!({
        "status": "completed",
        "clusters_found": clusters.len(),
        "nodes_created": created_nodes,
        "total_points_processed": neighborhoods.len(),
        "domain": domain,
    })))
}

/// POST /ontology/evolve — Check for new emerging clusters and label existing ones
///
/// This endpoint:
/// 1. Identifies unlabeled or newly grown clusters
/// 2. Fetches representative texts from each
/// 3. Uses LLM to generate descriptive labels and extract structured attributes
/// 4. Updates the tree with proper labels and hierarchy
async fn trigger_evolution(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_trope_manage_access(&state, &auth).await?;
    let _user_id = parse_user_id(&auth)?;
    let writable_book_ids =
        visible_trope_book_ids(&state, &auth, None, LibraryAccess::Write).await?;
    if matches!(writable_book_ids, Some(ref ids) if ids.is_empty()) {
        return Ok(Json(serde_json::json!({
            "evolved_nodes": 0,
            "remaining_unlabeled": 0,
        })));
    }

    // Find nodes that need labeling (auto-generated labels starting with "Cluster-")
    let unlabeled: Vec<(Uuid, String)> = if let Some(book_ids) = writable_book_ids.as_deref() {
        sqlx::query_as(
            "SELECT DISTINCT tn.id, tn.label \
             FROM trope_nodes tn \
             JOIN trope_chunk_assignments tca ON tca.trope_node_id = tn.id \
             WHERE tn.label LIKE 'Cluster-%' AND tca.book_id = ANY($1::uuid[]) \
             LIMIT 10",
        )
        .bind(book_ids)
        .fetch_all(&state.db)
        .await?
    } else {
        sqlx::query_as("SELECT id, label FROM trope_nodes WHERE label LIKE 'Cluster-%' LIMIT 10")
            .fetch_all(&state.db)
            .await?
    };

    let mut evolved = 0;

    for (node_id, _old_label) in &unlabeled {
        // Get representative chunk texts for this node
        let chunk_ids: Vec<(i64,)> = if let Some(book_ids) = writable_book_ids.as_deref() {
            sqlx::query_as(
                "SELECT qdrant_point_id FROM trope_chunk_assignments \
                 WHERE trope_node_id = $1 AND book_id = ANY($2::uuid[]) \
                 ORDER BY membership_score DESC LIMIT 5",
            )
            .bind(node_id)
            .bind(book_ids)
            .fetch_all(&state.db)
            .await?
        } else {
            sqlx::query_as(
                "SELECT qdrant_point_id FROM trope_chunk_assignments \
                 WHERE trope_node_id = $1 ORDER BY membership_score DESC LIMIT 5",
            )
            .bind(node_id)
            .fetch_all(&state.db)
            .await?
        };

        if chunk_ids.is_empty() {
            continue;
        }

        // Fetch texts from Qdrant by point IDs
        let qdrant_url =
            std::env::var("QDRANT_URL").unwrap_or_else(|_| "http://localhost:6333".into());
        let point_ids: Vec<u64> = chunk_ids.iter().map(|(id,)| *id as u64).collect();

        let points_resp = state
            .http_client
            .post(format!("{qdrant_url}/collections/nova_chunks/points"))
            .json(&serde_json::json!({
                "ids": point_ids,
                "with_payload": true,
            }))
            .send()
            .await;

        let texts: Vec<String> = if let Ok(resp) = points_resp {
            if let Ok(data) = resp.json::<serde_json::Value>().await {
                data["result"]
                    .as_array()
                    .unwrap_or(&vec![])
                    .iter()
                    .filter_map(|p| {
                        p["payload"]["text"]
                            .as_str()
                            .or_else(|| p["payload"]["content"].as_str())
                            .map(|s| s.to_string())
                    })
                    .collect()
            } else {
                continue;
            }
        } else {
            continue;
        };

        if texts.is_empty() {
            continue;
        }

        // Use LLM to label this cluster with structured output
        let sample = texts
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n---\n");
        let (auto_label, description, attributes) = match call_llm_for_label(&state, &sample).await
        {
            Ok(result) => result,
            Err(_) => {
                // Fallback: use first 30 chars of first text
                let truncated: String = texts
                    .first()
                    .map(|t| t.chars().take(30).collect::<String>())
                    .unwrap_or_default();
                (format!("设定: {truncated}..."), None, serde_json::json!({}))
            }
        };

        sqlx::query(
            "UPDATE trope_nodes SET label = $1, description = $2, attributes = $3, updated_at = now() WHERE id = $4",
        )
        .bind(&auto_label)
        .bind(&description)
        .bind(&attributes)
        .bind(node_id)
        .execute(&state.db)
        .await?;

        evolved += 1;
    }

    Ok(Json(serde_json::json!({
        "evolved_nodes": evolved,
        "remaining_unlabeled": unlabeled.len().saturating_sub(evolved),
    })))
}

/// POST /ontology/book/{book_id}/assign — Assign a book's chunks to existing tree nodes
async fn assign_book_to_tree(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(book_id): Path<Uuid>,
    Json(req): Json<AssignBookRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_trope_book_write_access(&state, &auth, book_id).await?;
    let incremental = req.incremental.unwrap_or(true);

    // Fetch book's vectors from Qdrant
    let chunks = fetch_qdrant_vectors(&state, Some(book_id), 10000).await?;

    // Fetch all existing node centroids
    let nodes: Vec<(Uuid, Vec<u8>)> =
        sqlx::query_as("SELECT id, centroid FROM trope_nodes WHERE centroid IS NOT NULL")
            .fetch_all(&state.db)
            .await?;

    if nodes.is_empty() {
        return Ok(Json(serde_json::json!({
            "status": "no_nodes",
            "message": "No ontology nodes with centroids exist. Run clustering first.",
        })));
    }

    let mut assigned = 0;
    let mut assignment_samples = Vec::new();

    for chunk in &chunks {
        if chunk.vector.is_empty() {
            continue;
        }

        if incremental {
            // Skip if already assigned
            let existing: Option<(Uuid,)> =
                sqlx::query_as("SELECT id FROM trope_chunk_assignments WHERE qdrant_point_id = $1")
                    .bind(chunk.point_id as i64)
                    .fetch_optional(&state.db)
                    .await?;
            if existing.is_some() {
                continue;
            }
        }

        // Find best matching node by cosine similarity to centroid
        let mut best_node: Option<Uuid> = None;
        let mut best_score: f32 = 0.0;

        for (node_id, centroid_bytes) in &nodes {
            let centroid = bytes_to_vector(centroid_bytes);
            let score = cosine_similarity(&chunk.vector, &centroid);
            if score > best_score {
                best_score = score;
                best_node = Some(*node_id);
            }
        }

        // Only assign if similarity is above threshold
        if let Some(node_id) = best_node {
            if best_score > 0.45 {
                sqlx::query(
                    "INSERT INTO trope_chunk_assignments \
                     (trope_node_id, book_id, chapter_index, chunk_index, qdrant_point_id, membership_score) \
                     VALUES ($1, $2, $3, $4, $5, $6) \
                     ON CONFLICT (qdrant_point_id) DO UPDATE SET \
                     trope_node_id = $1, membership_score = $6",
                )
                .bind(node_id)
                .bind(chunk.book_id)
                .bind(chunk.chapter_index)
                .bind(chunk.chunk_index)
                .bind(chunk.point_id as i64)
                .bind(best_score as f64)
                .execute(&state.db)
                .await?;
                assigned += 1;
                if assignment_samples.len() < 10 {
                    assignment_samples.push(serde_json::json!({
                        "node_id": node_id,
                        "book_title": chunk.book_title,
                        "chapter_index": chunk.chapter_index,
                        "chunk_index": chunk.chunk_index,
                        "score": best_score,
                        "text": chunk.text.chars().take(240).collect::<String>(),
                    }));
                }
            }
        }
    }

    Ok(Json(serde_json::json!({
        "book_id": book_id,
        "total_chunks": chunks.len(),
        "assigned": assigned,
        "samples": assignment_samples,
    })))
}

/// POST /ontology/search — Search by structured attributes
async fn search_by_attributes(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<AttributeSearchRequest>,
) -> ApiResult<Json<Vec<serde_json::Value>>> {
    let limit = req.limit.unwrap_or(20).min(100);
    let visible_books = visible_trope_book_ids(&state, &auth, None, LibraryAccess::Read).await?;
    if matches!(visible_books, Some(ref ids) if ids.is_empty()) {
        return Ok(Json(Vec::new()));
    }

    // Build dynamic query based on attribute filters
    let mut filters = req.filters.as_object().cloned().unwrap_or_default();
    if let Some(domain) = req.domain {
        filters.insert("domain".to_string(), serde_json::Value::String(domain));
    }

    let mut query_builder = QueryBuilder::<Postgres>::new(
        "SELECT DISTINCT b.id, b.title, b.author, \
         (SELECT jsonb_object_agg(tba.attribute_key, tba.attribute_value) \
          FROM trope_book_attributes tba WHERE tba.book_id = b.id) as attributes \
         FROM books b WHERE ",
    );

    if let Some(book_ids) = visible_books.as_deref() {
        query_builder
            .push("b.id = ANY(")
            .push_bind(book_ids)
            .push("::uuid[])");
    } else {
        query_builder.push("TRUE");
    }

    for (key, value) in filters {
        query_builder.push(" AND ");
        query_builder
            .push("EXISTS (SELECT 1 FROM trope_book_attributes tba WHERE tba.book_id = b.id AND tba.attribute_key = ")
            .push_bind(key)
            .push(" AND tba.attribute_value @> ")
            .push_bind(sqlx::types::Json(value))
            .push(")");
    }

    query_builder.push(" LIMIT ").push_bind(limit);

    let results: Vec<(Uuid, String, Option<String>, Option<serde_json::Value>)> =
        query_builder.build_query_as().fetch_all(&state.db).await?;

    let response: Vec<serde_json::Value> = results
        .into_iter()
        .map(|(id, title, author, attrs)| {
            serde_json::json!({
                "book_id": id,
                "title": title,
                "author": author,
                "attributes": attrs,
            })
        })
        .collect();

    Ok(Json(response))
}

/// GET /ontology/events — List recent ontology evolution events
async fn list_ontology_events(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(params): Query<PaginationQuery>,
) -> ApiResult<Json<Vec<serde_json::Value>>> {
    ensure_trope_manage_access(&state, &auth).await?;
    let limit = params.limit.unwrap_or(50).min(200);

    let events: Vec<(
        Uuid,
        String,
        Option<Uuid>,
        serde_json::Value,
        Option<String>,
        chrono::DateTime<chrono::Utc>,
    )> = sqlx::query_as(
        "SELECT id, event_type, trope_node_id, details, triggered_by, created_at \
             FROM ontology_events ORDER BY created_at DESC LIMIT $1",
    )
    .bind(limit)
    .fetch_all(&state.db)
    .await?;

    let response: Vec<serde_json::Value> = events
        .into_iter()
        .map(
            |(id, event_type, node_id, details, triggered_by, created_at)| {
                serde_json::json!({
                    "id": id,
                    "event_type": event_type,
                    "trope_node_id": node_id,
                    "details": details,
                    "triggered_by": triggered_by,
                    "created_at": created_at,
                })
            },
        )
        .collect();

    Ok(Json(response))
}

// ─── Persona Drift Endpoints ─────────────────────────────────────────────────

/// POST /persona/track — Start tracking a character's persona across chapters
///
/// Algorithm:
/// 1. For each chapter, filter chunks containing character's dialogue/monologue
/// 2. Embed those chunks and compute per-chapter centroid
/// 3. Compare consecutive centroids to measure drift
/// 4. Detect significant drift events (> threshold)
async fn track_persona(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<TrackPersonaRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let book_id = req.book_id;
    ensure_trope_book_write_access(&state, &auth, book_id).await?;
    let character_name = &req.character_name;

    // Fetch all chapters for this book
    let chapters: Vec<(i32, String)> =
        sqlx::query_as("SELECT index, content FROM chapters WHERE book_id = $1 ORDER BY index")
            .bind(book_id)
            .fetch_all(&state.db)
            .await?;

    if chapters.is_empty() {
        return Err(ApiError::not_found("No chapters found for this book"));
    }

    let (start_ch, end_ch) = req.chapter_range.unwrap_or((0, chapters.len() as i32));
    let mut baseline_centroid: Option<Vec<f32>> = None;
    let mut prev_centroid: Option<Vec<f32>> = None;
    let mut processed = 0;
    let mut events_detected = 0;

    for &(ch_idx, ref content) in &chapters {
        if ch_idx < start_ch || ch_idx >= end_ch {
            continue;
        }

        // Extract dialogue/monologue lines for this character
        // Simple heuristic: lines in quotes or lines mentioning the character
        let lines: Vec<&str> = content
            .lines()
            .filter(|line| {
                let l = line.trim();
                // Dialogue: lines starting with quotes or containing character name in dialogue context
                (l.starts_with('"') || l.starts_with('「') || l.starts_with('"'))
                    && l.contains(character_name.as_str())
                    || l.contains(&format!("{}想", character_name))
                    || l.contains(&format!("{}心中", character_name))
                    || l.contains(&format!("{}暗道", character_name))
            })
            .collect();

        if lines.is_empty() {
            continue;
        }

        // Concatenate and embed
        let text = lines.join("\n");
        let truncated: String = text.chars().take(2000).collect(); // Limit for embedding
        let embedding = embed_text(&state, &truncated).await?;

        if embedding.is_empty() {
            continue;
        }

        // Compute drift
        let drift_from_prev = prev_centroid
            .as_ref()
            .map(|prev| 1.0 - cosine_similarity(prev, &embedding) as f64);

        let drift_from_baseline = baseline_centroid
            .as_ref()
            .map(|base| 1.0 - cosine_similarity(base, &embedding) as f64);

        // Store snapshot
        let centroid_bytes = vector_to_bytes(&embedding);
        sqlx::query(
            "INSERT INTO persona_snapshots \
             (book_id, character_name, chapter_index, dialogue_centroid, \
              dialogue_count, drift_from_prev, drift_from_baseline) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) \
             ON CONFLICT (book_id, character_name, chapter_index) DO UPDATE SET \
             dialogue_centroid = $4, dialogue_count = $5, \
             drift_from_prev = $6, drift_from_baseline = $7, computed_at = now()",
        )
        .bind(book_id)
        .bind(character_name)
        .bind(ch_idx)
        .bind(&centroid_bytes)
        .bind(lines.len() as i32)
        .bind(drift_from_prev)
        .bind(drift_from_baseline)
        .execute(&state.db)
        .await?;

        // Detect significant drift events
        if let Some(drift) = drift_from_prev {
            if drift > 0.15 {
                // Significant personality shift detected
                sqlx::query(
                    "INSERT INTO persona_drift_events \
                     (book_id, character_name, chapter_index, drift_magnitude, \
                      event_type, evidence_text) \
                     VALUES ($1, $2, $3, $4, $5, $6)",
                )
                .bind(book_id)
                .bind(character_name)
                .bind(ch_idx)
                .bind(drift)
                .bind(if drift > 0.3 { "fusion" } else { "drift" })
                .bind(lines.first().map(|l| l.to_string()))
                .execute(&state.db)
                .await?;
                events_detected += 1;
            }
        }

        // Update references
        if baseline_centroid.is_none() {
            baseline_centroid = Some(embedding.clone());
        }
        prev_centroid = Some(embedding);
        processed += 1;
    }

    Ok(Json(serde_json::json!({
        "book_id": book_id,
        "character": character_name,
        "chapters_processed": processed,
        "drift_events_detected": events_detected,
    })))
}

/// GET /persona/book/{book_id}/characters — List tracked characters
async fn list_tracked_characters(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(book_id): Path<Uuid>,
) -> ApiResult<Json<Vec<serde_json::Value>>> {
    ensure_trope_book_read_access(&state, &auth, book_id).await?;

    let characters: Vec<(String, i64, Option<f64>)> = sqlx::query_as(
        "SELECT character_name, COUNT(*) as chapter_count, \
         MAX(drift_from_baseline) as max_drift \
         FROM persona_snapshots WHERE book_id = $1 \
         GROUP BY character_name ORDER BY chapter_count DESC",
    )
    .bind(book_id)
    .fetch_all(&state.db)
    .await?;

    let response: Vec<serde_json::Value> = characters
        .into_iter()
        .map(|(name, count, max_drift)| {
            serde_json::json!({
                "character_name": name,
                "chapters_tracked": count,
                "max_drift_from_baseline": max_drift,
            })
        })
        .collect();

    Ok(Json(response))
}

/// GET /persona/book/{book_id}/{character}/timeline — Get drift timeline
async fn get_drift_timeline(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path((book_id, character)): Path<(Uuid, String)>,
) -> ApiResult<Json<DriftTimeline>> {
    ensure_trope_book_read_access(&state, &auth, book_id).await?;

    let snapshots: Vec<PersonaSnapshot> = sqlx::query_as(
        "SELECT id, book_id, character_name, chapter_index, dialogue_count, \
         monologue_count, drift_from_prev, drift_from_baseline, computed_at \
         FROM persona_snapshots WHERE book_id = $1 AND character_name = $2 \
         ORDER BY chapter_index",
    )
    .bind(book_id)
    .bind(&character)
    .fetch_all(&state.db)
    .await?;

    let events: Vec<DriftEvent> = sqlx::query_as(
        "SELECT id, book_id, character_name, chapter_index, drift_magnitude, \
         drift_direction, evidence_text, target_persona, event_type, detected_at \
         FROM persona_drift_events WHERE book_id = $1 AND character_name = $2 \
         ORDER BY chapter_index",
    )
    .bind(book_id)
    .bind(&character)
    .fetch_all(&state.db)
    .await?;

    let total_drift = snapshots
        .last()
        .and_then(|s| s.drift_from_baseline)
        .unwrap_or(0.0);

    Ok(Json(DriftTimeline {
        character_name: character,
        book_id,
        snapshots,
        events,
        total_drift,
    }))
}

/// GET /persona/book/{book_id}/{character}/events — Get drift events only
async fn get_drift_events(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path((book_id, character)): Path<(Uuid, String)>,
) -> ApiResult<Json<Vec<DriftEvent>>> {
    ensure_trope_book_read_access(&state, &auth, book_id).await?;

    let events: Vec<DriftEvent> = sqlx::query_as(
        "SELECT id, book_id, character_name, chapter_index, drift_magnitude, \
         drift_direction, evidence_text, target_persona, event_type, detected_at \
         FROM persona_drift_events WHERE book_id = $1 AND character_name = $2 \
         ORDER BY drift_magnitude DESC",
    )
    .bind(book_id)
    .bind(&character)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(events))
}

/// POST /persona/compare — Compare two characters' personas across books
async fn compare_personas(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<ComparePersonasRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_trope_book_read_access(&state, &auth, req.source.book_id).await?;
    ensure_trope_book_read_access(&state, &auth, req.target.book_id).await?;

    // Get average centroids for both characters
    let source_centroids: Vec<(Vec<u8>,)> = sqlx::query_as(
        "SELECT dialogue_centroid FROM persona_snapshots \
         WHERE book_id = $1 AND character_name = $2 AND dialogue_centroid IS NOT NULL",
    )
    .bind(req.source.book_id)
    .bind(&req.source.character_name)
    .fetch_all(&state.db)
    .await?;

    let target_centroids: Vec<(Vec<u8>,)> = sqlx::query_as(
        "SELECT dialogue_centroid FROM persona_snapshots \
         WHERE book_id = $1 AND character_name = $2 AND dialogue_centroid IS NOT NULL",
    )
    .bind(req.target.book_id)
    .bind(&req.target.character_name)
    .fetch_all(&state.db)
    .await?;

    if source_centroids.is_empty() || target_centroids.is_empty() {
        return Err(ApiError::not_found(
            "One or both characters have no tracked data",
        ));
    }

    // Compute average centroids
    let source_vecs: Vec<Vec<f32>> = source_centroids
        .iter()
        .map(|(b,)| bytes_to_vector(b))
        .collect();
    let target_vecs: Vec<Vec<f32>> = target_centroids
        .iter()
        .map(|(b,)| bytes_to_vector(b))
        .collect();

    let source_avg = compute_centroid(&source_vecs);
    let target_avg = compute_centroid(&target_vecs);

    let similarity = cosine_similarity(&source_avg, &target_avg);

    Ok(Json(serde_json::json!({
        "source": {
            "book_id": req.source.book_id,
            "character": req.source.character_name,
            "snapshots": source_centroids.len(),
        },
        "target": {
            "book_id": req.target.book_id,
            "character": req.target.character_name,
            "snapshots": target_centroids.len(),
        },
        "persona_similarity": similarity,
        "interpretation": if similarity > 0.85 {
            "极其相似的人格/说话风格"
        } else if similarity > 0.7 {
            "明显相似，可能是同一灵魂/意识"
        } else if similarity > 0.5 {
            "有一定相似性"
        } else {
            "截然不同的人格"
        },
    })))
}

// ─── Rule Splicing Endpoints ─────────────────────────────────────────────────

/// GET /rules/book/{book_id} — List extracted setting rules for a book
async fn list_book_rules(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(book_id): Path<Uuid>,
) -> ApiResult<Json<Vec<SettingRule>>> {
    ensure_trope_book_read_access(&state, &auth, book_id).await?;

    let rules: Vec<SettingRule> = sqlx::query_as(
        "SELECT id, book_id, trope_node_id, subject_type, subject_label, \
         predicate, object_type, object_label, properties, constraints, \
         source_text, chapter_index, confidence \
         FROM setting_rules WHERE book_id = $1 \
         ORDER BY confidence DESC",
    )
    .bind(book_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(rules))
}

/// POST /rules/extract — Extract setting rules from a book using LLM
async fn extract_rules(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<ExtractRulesRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let book_id = req.book_id;
    ensure_trope_book_write_access(&state, &auth, book_id).await?;
    let domain = req.domain.unwrap_or_else(|| "general".to_string());
    let chapter_range = req.chapter_range;

    // Fetch relevant chunks (mechanism-describing passages)
    let mechanism_keywords = match domain.as_str() {
        "worldbuilding" => vec!["世界", "势力", "地理", "历史", "规则"],
        "power_system" => vec!["修炼", "能力", "境界", "突破", "限制"],
        "relationship" => vec!["关系", "师徒", "盟友", "敌对", "羁绊"],
        "trope" => vec!["桥段", "伏笔", "反转", "冲突", "动机"],
        "tone" => vec!["氛围", "情绪", "压迫", "治愈", "热血"],
        _ => vec!["设定", "规则", "机制", "能力", "影响"],
    };

    // Search Qdrant for relevant chunks
    let qdrant_url = std::env::var("QDRANT_URL").unwrap_or_else(|_| "http://localhost:6333".into());
    let query_text = mechanism_keywords.join(" ");
    let query_embedding = embed_text(&state, &query_text).await?;
    let mut must_filters = vec![serde_json::json!({
        "key": "book_id",
        "match": {"value": book_id.to_string()}
    })];
    if let Some((start, end)) = chapter_range {
        must_filters.push(serde_json::json!({
            "key": "chapter_index",
            "range": {"gte": start, "lte": end}
        }));
    }

    let search_resp = state
        .http_client
        .post(format!(
            "{qdrant_url}/collections/nova_chunks/points/search"
        ))
        .json(&serde_json::json!({
            "vector": query_embedding,
            "limit": 20,
            "with_payload": true,
            "filter": {
                "must": must_filters
            }
        }))
        .send()
        .await
        .map_err(|e| ApiError::internal(format!("Qdrant search failed: {e}")))?;

    let search_data: serde_json::Value = search_resp
        .json()
        .await
        .map_err(|e| ApiError::internal(format!("Search parse failed: {e}")))?;

    let empty_results = vec![];
    let results = search_data["result"].as_array().unwrap_or(&empty_results);
    let mut extracted = 0;

    for result in results {
        let text = result["payload"]["text"]
            .as_str()
            .or_else(|| result["payload"]["content"].as_str())
            .unwrap_or("");
        let chapter_index = result["payload"]["chapter_index"]
            .as_i64()
            .map(|i| i as i32);
        let score = result["score"].as_f64().unwrap_or(0.0);

        if text.len() < 50 || score < 0.4 {
            continue;
        }

        // Use LLM to extract structured rules from this passage
        let llm_rules = call_llm_for_rules(&state, text, &domain)
            .await
            .unwrap_or_default();

        for llm_rule in &llm_rules {
            let rule_id = Uuid::new_v4();
            let subject_type = llm_rule["subject_type"].as_str().unwrap_or("mechanism");
            let subject_label = llm_rule["subject_label"].as_str().unwrap_or(&domain);
            let predicate = llm_rule["predicate"].as_str().unwrap_or("defines");
            let object_type = llm_rule["object_type"].as_str().unwrap_or("setting");
            let object_label = llm_rule["object_label"].as_str().unwrap_or("rule");
            let properties = llm_rule
                .get("properties")
                .cloned()
                .unwrap_or(serde_json::json!({}));
            let constraints = llm_rule.get("constraints").cloned();
            let confidence = llm_rule["confidence"].as_f64().unwrap_or(score);

            sqlx::query(
                "INSERT INTO setting_rules \
                 (id, book_id, subject_type, subject_label, predicate, object_type, object_label, \
                  properties, constraints, source_text, chapter_index, confidence) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
            )
            .bind(rule_id)
            .bind(book_id)
            .bind(subject_type)
            .bind(subject_label)
            .bind(predicate)
            .bind(object_type)
            .bind(object_label)
            .bind(&properties)
            .bind(&constraints)
            .bind(text)
            .bind(chapter_index)
            .bind(confidence)
            .execute(&state.db)
            .await?;

            extracted += 1;
        }
    }

    Ok(Json(serde_json::json!({
        "book_id": book_id,
        "domain": domain,
        "chapter_range": chapter_range,
        "chunks_searched": results.len(),
        "rules_extracted": extracted,
    })))
}

/// POST /rules/splice — Combine rules from different books into a scenario
async fn splice_rules(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<SpliceRulesRequest>,
) -> ApiResult<Json<SpliceResult>> {
    if req.rule_ids.is_empty() {
        return Err(ApiError::bad_request("No rule IDs provided"));
    }

    // Fetch the rules
    let _placeholders: String = req
        .rule_ids
        .iter()
        .enumerate()
        .map(|(i, _)| format!("${}", i + 1))
        .collect::<Vec<_>>()
        .join(",");

    let query = format!(
        "SELECT id, book_id, trope_node_id, subject_type, subject_label, \
         predicate, object_type, object_label, properties, constraints, \
         source_text, chapter_index, confidence \
         FROM setting_rules WHERE id = ANY($1)"
    );

    let rule_ids_arr: Vec<Uuid> = req.rule_ids.clone();
    let rules: Vec<SettingRule> = sqlx::query_as(&query)
        .bind(&rule_ids_arr)
        .fetch_all(&state.db)
        .await?;

    if rules.is_empty() {
        return Err(ApiError::not_found("No rules found with provided IDs"));
    }

    for rule in &rules {
        ensure_trope_book_read_access(&state, &auth, rule.book_id).await?;
    }

    // Detect potential conflicts between rules
    let mut conflicts = Vec::new();
    for i in 0..rules.len() {
        for j in (i + 1)..rules.len() {
            let ri = &rules[i];
            let rj = &rules[j];
            // Simple conflict: same predicate with different properties
            if ri.predicate == rj.predicate && ri.properties != rj.properties {
                conflicts.push(format!(
                    "规则冲突: 《{}》的设定 vs 另一规则的设定在 '{}' 上有不同定义",
                    ri.subject_label, ri.predicate
                ));
            }
        }
    }

    // Generate narrative by combining rule contexts
    // In production, this would call LLM with all rules as context
    let context_texts: Vec<String> = rules.iter().filter_map(|r| r.source_text.clone()).collect();

    let narrative = if let Some(prompt) = req.scenario_prompt {
        format!(
            "基于以下设定规则的融合场景：\n\n{}\n\n用户指定场景：{}",
            context_texts.join("\n---\n"),
            prompt
        )
    } else {
        format!(
            "融合场景生成（{}条规则）：\n{}",
            rules.len(),
            context_texts.join("\n---\n")
        )
    };

    Ok(Json(SpliceResult {
        narrative,
        source_rules: rules,
        conflicts,
    }))
}

#[cfg(test)]
mod tests {
    fn production_source() -> &'static str {
        include_str!("trope_ontology.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("production source should exist")
    }

    #[test]
    fn trope_book_routes_require_book_acl() {
        let source = production_source();

        assert!(source.contains("ensure_trope_book_read_access"));
        assert!(source.contains("ensure_trope_book_write_access"));
        assert!(source.contains("ensure_book_access(state, auth, book_id, LibraryAccess::Read)"));
        assert!(source.contains("ensure_book_access(state, auth, book_id, LibraryAccess::Write)"));
        assert!(source.contains("ensure_trope_book_write_access(&state, &auth, book_id).await?"));
    }

    #[test]
    fn trope_multi_book_routes_scope_requested_and_visible_books() {
        let source = production_source();

        assert!(source.contains("visible_trope_book_ids"));
        assert!(source.contains("visible_library_ids(state, auth, LibraryAccess::Read)"));
        assert!(source.contains("visible_library_ids(state, auth, LibraryAccess::Write)"));
        assert!(source.contains("qdrant_book_filter"));
        assert!(source.contains("allowed_book_id_set"));
        assert!(source.contains("id = ANY($1::uuid[])"));
    }

    #[test]
    fn trope_global_node_mutations_are_admin_only() {
        let source = production_source();

        assert!(source.contains("use crate::access::{ensure_book_access, is_admin"));
        assert!(source.contains("async fn ensure_trope_manage_access"));
        assert!(source.contains("if is_admin(state, auth).await?"));
        assert!(!source.contains("visible_library_ids(state, auth, LibraryAccess::Manage)"));

        for handler in [
            "create_node",
            "update_node",
            "delete_node",
            "move_node",
            "merge_nodes",
            "evolve_node",
            "trigger_clustering",
            "trigger_evolution",
            "list_ontology_events",
        ] {
            let body = source
                .split(&format!("async fn {handler}"))
                .nth(1)
                .unwrap_or_else(|| panic!("{handler} should exist"));
            assert!(
                body.contains("ensure_trope_manage_access(&state, &auth).await?"),
                "{handler} should require admin-only ontology management access"
            );
        }
    }

    #[test]
    fn trope_rule_splicing_checks_source_rule_books() {
        let source = production_source();

        assert!(source.contains("for rule in &rules"));
        assert!(
            source.contains("ensure_trope_book_read_access(&state, &auth, rule.book_id).await?")
        );
    }

    #[test]
    fn trope_persona_compare_checks_both_books() {
        let source = production_source();

        assert!(source
            .contains("ensure_trope_book_read_access(&state, &auth, req.source.book_id).await?"));
        assert!(source
            .contains("ensure_trope_book_read_access(&state, &auth, req.target.book_id).await?"));
    }
}
