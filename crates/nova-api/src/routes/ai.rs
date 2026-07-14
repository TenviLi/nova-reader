use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionRequestAssistantMessage, ChatCompletionRequestAssistantMessageContent,
        ChatCompletionRequestMessage, ChatCompletionRequestSystemMessage,
        ChatCompletionRequestSystemMessageContent, ChatCompletionRequestUserMessage,
        ChatCompletionRequestUserMessageContent, CreateChatCompletionRequestArgs,
    },
    Client as OpenAIClient,
};
use axum::{
    extract::State,
    response::sse::{Event, Sse},
    routing::{get, post},
    Json, Router,
};
use futures::stream::{self, Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::convert::Infallible;
use std::sync::Arc;
use uuid::Uuid;

use nova_core::repo::book_repo::BookRepository;
use nova_core::repo::chapter_repo::ChapterRepository;

use crate::access::{auth_user_id, ensure_book_access, is_admin, LibraryAccess};
use crate::dedup::{
    delete_book_embedding_points, embedding_point_id, load_embedding_freshness_contract,
};
use crate::error::{ApiError, ApiResult};
use crate::extractors::AuthUser;
use crate::repo::pg_chapter::SearchableChapterRecord;
use crate::state::AppState;

/// Resolved AI model configuration using async-openai SDK.
struct AiConfig {
    client: OpenAIClient<OpenAIConfig>,
    model: String,
    api_key: String,
    base_url: String,
}

impl AiConfig {
    fn from_state(state: &AppState) -> Self {
        let base_url = if state.config.deepseek_base_url.is_empty() {
            "https://api.deepseek.com/v1".to_string()
        } else {
            let url = &state.config.deepseek_base_url;
            // Ensure URL ends with /v1 for OpenAI-compatible APIs
            if url.ends_with("/v1") {
                url.clone()
            } else {
                format!("{}/v1", url.trim_end_matches('/'))
            }
        };

        let config = OpenAIConfig::new()
            .with_api_key(&state.config.deepseek_api_key)
            .with_api_base(&base_url);

        let model = if state.config.deepseek_model.is_empty() {
            "deepseek-chat".to_string()
        } else {
            state.config.deepseek_model.clone()
        };

        Self {
            client: OpenAIClient::with_config(config),
            model,
            api_key: state.config.deepseek_api_key.clone(),
            base_url,
        }
    }

    /// Call the AI model and return the response content + usage.
    async fn complete(
        &self,
        messages: &[ChatMessage],
        temperature: f64,
        max_tokens: usize,
    ) -> Result<(String, TokenUsage), ApiError> {
        let openai_messages: Vec<ChatCompletionRequestMessage> = messages
            .iter()
            .map(|m| match m.role.as_str() {
                "system" => {
                    ChatCompletionRequestMessage::System(ChatCompletionRequestSystemMessage {
                        content: ChatCompletionRequestSystemMessageContent::Text(m.content.clone()),
                        name: None,
                    })
                }
                "assistant" => {
                    ChatCompletionRequestMessage::Assistant(ChatCompletionRequestAssistantMessage {
                        content: Some(ChatCompletionRequestAssistantMessageContent::Text(
                            m.content.clone(),
                        )),
                        ..Default::default()
                    })
                }
                _ => ChatCompletionRequestMessage::User(ChatCompletionRequestUserMessage {
                    content: ChatCompletionRequestUserMessageContent::Text(m.content.clone()),
                    ..Default::default()
                }),
            })
            .collect();

        let request = CreateChatCompletionRequestArgs::default()
            .model(&self.model)
            .messages(openai_messages)
            .temperature(temperature as f32)
            .max_tokens(max_tokens as u32)
            .build()
            .map_err(|e| ApiError::Internal(format!("Failed to build AI request: {}", e)))?;

        let response =
            self.client.chat().create(request).await.map_err(|e| {
                ApiError::ServiceUnavailable(format!("AI service unavailable: {}", e))
            })?;

        let choice = response
            .choices
            .first()
            .ok_or_else(|| ApiError::Internal("AI returned no choices".to_string()))?;

        let content = choice.message.content.clone().unwrap_or_default();

        let usage = response
            .usage
            .map(|u| TokenUsage {
                prompt_tokens: u.prompt_tokens as usize,
                completion_tokens: u.completion_tokens as usize,
                total_tokens: u.total_tokens as usize,
            })
            .unwrap_or_default();

        Ok((content, usage))
    }
}

fn parse_ai_book_id(book_id: &str) -> ApiResult<Uuid> {
    Uuid::parse_str(book_id).map_err(|_| ApiError::bad_request("Invalid book_id"))
}

async fn ensure_ai_book_read_access(
    state: &AppState,
    auth: &AuthUser,
    book_id: Uuid,
) -> ApiResult<()> {
    ensure_book_access(state, auth, book_id, LibraryAccess::Read).await
}

async fn ensure_ai_book_access_from_str(
    state: &AppState,
    auth: &AuthUser,
    book_id: &str,
    access: LibraryAccess,
) -> ApiResult<Uuid> {
    let book_id = parse_ai_book_id(book_id)?;
    ensure_book_access(state, auth, book_id, access).await?;
    Ok(book_id)
}

async fn ensure_optional_ai_book_read_access(
    state: &AppState,
    auth: &AuthUser,
    book_id: Option<&str>,
) -> ApiResult<()> {
    if let Some(book_id) = book_id.filter(|id| !id.trim().is_empty()) {
        ensure_ai_book_access_from_str(state, auth, book_id, LibraryAccess::Read).await?;
    }
    Ok(())
}

async fn ensure_ai_admin_access(state: &AppState, auth: &AuthUser) -> ApiResult<()> {
    if is_admin(state, auth).await? {
        Ok(())
    } else {
        Err(ApiError::forbidden())
    }
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/chat", post(chat))
        .route("/chat/stream", post(chat_stream))
        .route("/rag-context", post(rag_context))
        .route("/summarize", post(summarize))
        .route("/extract-entities", post(extract_entities))
        .route("/analyze-style", post(analyze_style))
        .route("/suggest-tags", post(suggest_tags))
        .route("/generate-outline", post(generate_outline))
        .route("/batch-process", post(batch_process_book))
        .route("/batch-process/stream", post(batch_process_stream))
        .route("/ingest-embeddings", post(ingest_embeddings))
        .route("/embeddings/status", get(embedding_index_status))
        .route("/embeddings/rebuild", post(embedding_index_rebuild))
        .route("/embeddings/delete-book", post(embedding_delete_book))
        .route("/extract-glossary", post(extract_glossary))
        .route("/detect-plot-holes", post(detect_plot_holes))
        .route("/generate-chapter-titles", post(generate_chapter_titles))
        .route("/cleanup-forum-text", post(cleanup_forum_text))
        .route("/sentiment-arc", post(analyze_sentiment_arc))
        .route("/generate-review", post(generate_book_review))
        .route("/text-quality", post(assess_text_quality))
        .route("/smart-bookmarks", post(detect_smart_bookmarks))
        .route("/character-evolution", post(analyze_character_evolution))
        .route("/compress-text", post(compress_text))
        .route("/style-transfer", post(style_transfer))
        .route("/world-building-graph", post(world_building_graph))
        .route("/plot-consistency", post(check_plot_consistency))
        .route("/generate-quiz", post(generate_quiz))
}

#[derive(Deserialize)]
struct ChatRequest {
    messages: Vec<ChatMessage>,
    book_id: Option<String>,
    include_rag: Option<bool>,
    temperature: Option<f64>,
    max_tokens: Option<usize>,
    /// If provided, loads previous messages from this conversation.
    conversation_id: Option<String>,
}

#[derive(Deserialize, Serialize, Clone)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct ChatResponse {
    message: ChatMessage,
    sources: Vec<SourceReference>,
    usage: TokenUsage,
}

#[derive(Serialize)]
struct SourceReference {
    book_title: String,
    chapter_title: String,
    content_snippet: String,
    relevance_score: f64,
}

#[derive(Serialize, Default)]
struct TokenUsage {
    prompt_tokens: usize,
    completion_tokens: usize,
    total_tokens: usize,
}

async fn chat(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, ApiError> {
    // Feature flag check
    crate::feature_flags::require_feature(&state.db, "ai_chat").await?;

    // Input validation: prevent abuse
    if body.messages.is_empty() {
        return Err(ApiError::bad_request("messages cannot be empty"));
    }
    let total_chars: usize = body.messages.iter().map(|m| m.content.len()).sum();
    if total_chars > 100_000 {
        return Err(ApiError::bad_request(
            "total message content exceeds 100k character limit",
        ));
    }
    ensure_optional_ai_book_read_access(&state, &auth, body.book_id.as_deref()).await?;
    let user_id = auth_user_id(&auth)?;

    let ai = AiConfig::from_state(&state);

    // Load conversation history if conversation_id is provided
    let mut messages = if let Some(ref conv_id) = body.conversation_id {
        let history = load_conversation_history(&state, user_id, conv_id).await;
        let mut combined = history;
        combined.extend(body.messages.clone());
        combined
    } else {
        body.messages.clone()
    };

    // Add RAG context if requested
    if body.include_rag.unwrap_or(false) {
        if let Some(book_id) = &body.book_id {
            let context = retrieve_rag_context(&state, book_id, &messages).await;
            if !context.is_empty() {
                messages.insert(
                    0,
                    ChatMessage {
                        role: "system".to_string(),
                        content: crate::prompts::rag_context_message(&context),
                    },
                );
            }
        }
    }

    let (content, usage) = ai
        .complete(
            &messages,
            body.temperature.unwrap_or(0.3),
            body.max_tokens.unwrap_or(4096),
        )
        .await?;

    // Save conversation history if conversation_id is provided
    if let Some(ref conv_id) = body.conversation_id {
        let mut to_save = body.messages.clone();
        to_save.push(ChatMessage {
            role: "assistant".to_string(),
            content: content.clone(),
        });
        save_conversation_messages(&state, user_id, conv_id, &to_save).await;
    }

    Ok(Json(ChatResponse {
        message: ChatMessage {
            role: "assistant".to_string(),
            content,
        },
        sources: vec![],
        usage,
    }))
}

async fn chat_stream(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<ChatRequest>,
) -> ApiResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    ensure_optional_ai_book_read_access(&state, &auth, body.book_id.as_deref()).await?;
    let ai = AiConfig::from_state(&state);
    let api_key = ai.api_key.to_string();
    let base_url = ai.base_url.to_string();
    let model = ai.model.to_string();

    let messages = body.messages.clone();
    let temperature = body.temperature.unwrap_or(0.3);
    let max_tokens = body.max_tokens.unwrap_or(4096);

    let stream = async_stream::stream! {
        let client = &state.http_client;
        let resp = client
            .post(format!("{}/chat/completions", base_url))
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&serde_json::json!({
                "model": model,
                "messages": messages,
                "temperature": temperature,
                "max_tokens": max_tokens,
                "stream": true,
            }))
            .send()
            .await;

        match resp {
            Ok(response) => {
                let mut byte_stream = response.bytes_stream();
                let mut buffer = String::new();

                while let Some(chunk) = byte_stream.next().await {
                    match chunk {
                        Ok(bytes) => {
                            buffer.push_str(&String::from_utf8_lossy(&bytes));
                            // Parse SSE lines from buffer
                            while let Some(line_end) = buffer.find('\n') {
                                let line = buffer[..line_end].trim().to_string();
                                buffer = buffer[line_end + 1..].to_string();

                                if line.starts_with("data: ") {
                                    let data = &line[6..];
                                    if data == "[DONE]" {
                                        yield Ok(Event::default().data("[DONE]"));
                                        return;
                                    }
                                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data) {
                                        if let Some(content) = parsed["choices"][0]["delta"]["content"].as_str() {
                                            if !content.is_empty() {
                                                yield Ok(Event::default().data(content));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Err(_) => break,
                    }
                }
            }
            Err(e) => {
                yield Ok(Event::default().data(format!("Error: {}", e)));
            }
        }
    };

    Ok(Sse::new(stream))
}

/// Returns RAG context (vector search + graph) without doing LLM call.
/// Used by the SvelteKit AI SDK server route to inject context into the system prompt.
#[derive(Deserialize)]
struct RagContextRequest {
    query: String,
    book_id: Option<String>,
}

#[derive(Serialize)]
struct RagContextResponse {
    context: String,
    sources: Vec<RagSource>,
}

#[derive(Serialize)]
struct RagSource {
    chapter_title: String,
    content: String,
    score: f64,
}

async fn rag_context(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<RagContextRequest>,
) -> ApiResult<Json<RagContextResponse>> {
    let book_id = body.book_id.as_deref().unwrap_or("");
    if book_id.is_empty() || body.query.trim().is_empty() {
        return Ok(Json(RagContextResponse {
            context: String::new(),
            sources: vec![],
        }));
    }
    ensure_ai_book_access_from_str(&state, &auth, book_id, LibraryAccess::Read).await?;

    let messages = vec![ChatMessage {
        role: "user".to_string(),
        content: body.query,
    }];
    let context = retrieve_rag_context(&state, book_id, &messages).await;

    Ok(Json(RagContextResponse {
        context,
        sources: vec![],
    }))
}

async fn retrieve_rag_context(state: &AppState, book_id: &str, messages: &[ChatMessage]) -> String {
    // Get the last user message as query
    let query = messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .map(|m| m.content.as_str())
        .unwrap_or("");

    if query.is_empty() {
        return String::new();
    }

    // 1. Generate query embedding via embedding API
    let embedding_endpoint = &state.config.embedding_endpoint;
    let embedding_model = &state.config.embedding_model;

    let embed_client = &state.http_client;
    let mut embed_req = embed_client
        .post(format!("{}/v1/embeddings", embedding_endpoint))
        .header("X-Failover-Enabled", "true")
        .json(&serde_json::json!({
            "input": [query],
            "model": embedding_model,
            "dimensions": state.config.embedding_dimensions,
        }));

    if !state.config.embedding_api_key.is_empty() {
        embed_req = embed_req.header(
            "Authorization",
            format!("Bearer {}", state.config.embedding_api_key),
        );
    }

    let embed_response = embed_req.send().await;

    let query_vector: Vec<f32> = match embed_response {
        Ok(resp) => {
            let data: serde_json::Value = match resp.json().await {
                Ok(d) => d,
                Err(_) => return String::new(),
            };
            // OpenAI-compatible format: data[0].embedding
            data["data"][0]["embedding"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_f64().map(|f| f as f32))
                        .collect()
                })
                .unwrap_or_default()
        }
        Err(_) => return String::new(),
    };

    if query_vector.is_empty() {
        return String::new();
    }

    // 2. Search Qdrant for relevant chunks (filtered by book_id)
    let qdrant_url = if state.config.qdrant_url.is_empty() {
        "http://localhost:6333"
    } else {
        &state.config.qdrant_url
    };

    let search_body = serde_json::json!({
        "vector": query_vector,
        "limit": 5,
        "with_payload": true,
        "filter": {
            "must": [
                { "key": "book_id", "match": { "value": book_id } }
            ]
        }
    });

    let search_response = embed_client
        .post(format!(
            "{}/collections/nova_chunks/points/search",
            qdrant_url
        ))
        .json(&search_body)
        .send()
        .await;

    let chunks: Vec<String> = match search_response {
        Ok(resp) => {
            let data: serde_json::Value = match resp.json().await {
                Ok(d) => d,
                Err(_) => return String::new(),
            };
            data["result"]
                .as_array()
                .map(|results| {
                    results
                        .iter()
                        .filter_map(|r| {
                            let score = r["score"].as_f64().unwrap_or(0.0);
                            if score < 0.3 {
                                return None;
                            } // Relevance threshold
                            let text = r["payload"]["text"].as_str()?;
                            let chapter =
                                r["payload"]["chapter_title"].as_str().unwrap_or("未知章节");
                            Some(format!("[{}] {}", chapter, text))
                        })
                        .collect()
                })
                .unwrap_or_default()
        }
        Err(_) => return String::new(),
    };

    if chunks.is_empty() {
        return String::new();
    }

    // 3. Rerank chunks using Qwen3-Reranker for better precision
    let reranked_chunks = rerank_chunks(state, query, &chunks).await;
    let final_chunks = if reranked_chunks.is_empty() {
        chunks
    } else {
        reranked_chunks
    };

    // 4. Also fetch graph context from Neo4j for entity relationships
    let graph_context = retrieve_graph_context(state, query, book_id).await;

    // 5. Compose final RAG context
    let mut context = String::from("## 相关文本片段\n\n");
    for (i, chunk) in final_chunks.iter().enumerate() {
        context.push_str(&format!("{}. {}\n\n", i + 1, chunk));
    }

    if !graph_context.is_empty() {
        context.push_str("\n## 知识图谱信息\n\n");
        context.push_str(&graph_context);
    }

    context
}

/// Rerank retrieved chunks using Qwen3-Reranker via OpenAI-compatible reranker API.
/// Falls back to original order if reranker is unavailable.
async fn rerank_chunks(state: &AppState, query: &str, chunks: &[String]) -> Vec<String> {
    let reranker_endpoint = if state.config.reranker_endpoint.is_empty() {
        return Vec::new(); // No reranker configured, skip
    } else {
        &state.config.reranker_endpoint
    };
    let reranker_model = if state.config.reranker_model.is_empty() {
        "qwen3-reranker-0.6b-mlx"
    } else {
        &state.config.reranker_model
    };

    // Use /v1/rerank endpoint (compatible with LM Studio / vLLM reranker API)
    let response = state
        .http_client
        .post(format!("{}/v1/rerank", reranker_endpoint))
        .json(&serde_json::json!({
            "model": reranker_model,
            "query": query,
            "documents": chunks,
            "top_n": 5
        }))
        .send()
        .await;

    match response {
        Ok(resp) => {
            if let Ok(data) = resp.json::<serde_json::Value>().await {
                // Parse reranker response: { results: [{ index, relevance_score }] }
                if let Some(results) = data["results"].as_array() {
                    let mut scored: Vec<(usize, f64)> = results
                        .iter()
                        .filter_map(|r| {
                            let idx = r["index"].as_u64()? as usize;
                            let score = r["relevance_score"]
                                .as_f64()
                                .or_else(|| r["score"].as_f64())?;
                            Some((idx, score))
                        })
                        .collect();
                    scored
                        .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                    // Filter by minimum relevance and return reranked
                    return scored
                        .iter()
                        .filter(|(_, score)| *score > 0.1)
                        .filter_map(|(idx, _)| chunks.get(*idx).cloned())
                        .collect();
                }
            }
            Vec::new()
        }
        Err(_) => Vec::new(), // Reranker unavailable, fallback to original
    }
}

/// Retrieve related entity/relationship info from Neo4j graph.
async fn retrieve_graph_context(state: &AppState, query: &str, book_id: &str) -> String {
    if state.config.neo4j_uri.is_empty() {
        return String::new();
    }

    // Simple entity name search in the query (extract likely entity names)
    let cypher = r#"
        MATCH (n {book_id: $book_id})
        WHERE toLower(n.name) CONTAINS toLower($query) OR
              ANY(alias IN coalesce(n.aliases, []) WHERE toLower(alias) CONTAINS toLower($query))
        OPTIONAL MATCH (n)-[r]->(m {book_id: $book_id})
        RETURN n.name AS entity, labels(n) AS types, n.description AS desc,
               type(r) AS rel_type, m.name AS related
        LIMIT 10
    "#;

    let params = serde_json::json!({ "query": query, "book_id": book_id });

    match state.neo4j.execute(cypher, Some(params)).await {
        Ok(data) => {
            let mut result = String::new();
            if let Some(rows) = data["results"][0]["data"].as_array() {
                for row in rows.iter().take(8) {
                    if let Some(values) = row["row"].as_array() {
                        let entity = values.first().and_then(|v| v.as_str()).unwrap_or("");
                        let desc = values.get(2).and_then(|v| v.as_str()).unwrap_or("");
                        let rel = values.get(3).and_then(|v| v.as_str()).unwrap_or("");
                        let related = values.get(4).and_then(|v| v.as_str()).unwrap_or("");

                        if !entity.is_empty() {
                            result.push_str(&format!("- {}", entity));
                            if !desc.is_empty() {
                                result.push_str(&format!(": {}", desc));
                            }
                            if !rel.is_empty() && !related.is_empty() {
                                result.push_str(&format!(" → {}({})", related, rel));
                            }
                            result.push('\n');
                        }
                    }
                }
            }
            result
        }
        Err(_) => String::new(),
    }
}

#[derive(Deserialize)]
struct SummarizeRequest {
    text: String,
    max_length: Option<usize>,
    style: Option<String>, // "brief", "detailed", "bullet_points"
}

#[derive(Serialize)]
struct SummarizeResponse {
    summary: String,
    key_points: Vec<String>,
}

async fn summarize(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SummarizeRequest>,
) -> Result<Json<SummarizeResponse>, ApiError> {
    crate::feature_flags::require_feature(&state.db, "ai_summarize").await?;

    if body.text.is_empty() {
        return Err(ApiError::bad_request("text cannot be empty"));
    }
    if body.text.len() > 200_000 {
        return Err(ApiError::bad_request("text exceeds 200k character limit"));
    }

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

    let style_prompt = crate::prompts::summarize_prompt(body.style.as_deref().unwrap_or("brief"));

    let client = &state.http_client;
    let resp = client
        .post(format!("{}/chat/completions", base_url))
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&serde_json::json!({
            "model": model,
            "messages": [
                {"role": "system", "content": style_prompt},
                {"role": "user", "content": body.text}
            ],
            "temperature": 0.2,
            "max_tokens": body.max_length.unwrap_or(2048),
        }))
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("DeepSeek error: {}", e)))?;

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ApiError::Internal(format!("Parse error: {}", e)))?;

    let content = data["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("{}");

    // Try to parse JSON from response
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(content) {
        Ok(Json(SummarizeResponse {
            summary: parsed["summary"].as_str().unwrap_or("").to_string(),
            key_points: parsed["key_points"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
        }))
    } else {
        Ok(Json(SummarizeResponse {
            summary: content.to_string(),
            key_points: vec![],
        }))
    }
}

#[derive(Deserialize)]
struct ExtractEntitiesRequest {
    text: String,
    book_id: Option<String>,
    chapter_index: Option<i32>,
}

#[derive(Serialize, Deserialize)]
struct ExtractEntitiesResponse {
    entities: Vec<ExtractedEntity>,
    relationships: Vec<ExtractedRelationship>,
}

#[derive(Serialize, Deserialize)]
struct ExtractedEntity {
    name: String,
    entity_type: String,
    description: String,
    aliases: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct ExtractedRelationship {
    source: String,
    target: String,
    relationship_type: String,
    description: String,
}

async fn extract_entities(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<ExtractEntitiesRequest>,
) -> Result<Json<ExtractEntitiesResponse>, ApiError> {
    crate::feature_flags::require_feature(&state.db, "ai_entities").await?;
    ensure_optional_ai_book_read_access(&state, &auth, body.book_id.as_deref()).await?;

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

    let system_prompt = crate::prompts::entity_extraction_prompt();
    let user_content = if let Some(chapter_index) = body.chapter_index {
        format!(
            "Book ID: {}\nChapter index: {}\n\n{}",
            body.book_id.as_deref().unwrap_or("unknown"),
            chapter_index,
            body.text
        )
    } else {
        body.text
    };

    let client = &state.http_client;
    let resp = client
        .post(format!("{}/chat/completions", base_url))
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&serde_json::json!({
            "model": model,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": user_content}
            ],
            "temperature": 0.1,
            "max_tokens": 4096,
        }))
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("DeepSeek error: {}", e)))?;

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ApiError::Internal(format!("Parse error: {}", e)))?;

    let content = data["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("{}");

    if let Ok(parsed) = serde_json::from_str::<ExtractEntitiesResponse>(content) {
        Ok(Json(parsed))
    } else {
        Ok(Json(ExtractEntitiesResponse {
            entities: vec![],
            relationships: vec![],
        }))
    }
}

#[derive(Deserialize)]
struct AnalyzeStyleRequest {
    text: String,
}

#[derive(Serialize, Deserialize)]
struct StyleAnalysis {
    tone: String,
    pov: String,
    avg_sentence_length: f64,
    vocabulary_richness: f64,
    dialogue_ratio: f64,
    description_style: String,
    pacing: String,
    suggestions: Vec<String>,
}

async fn analyze_style(
    State(state): State<Arc<AppState>>,
    Json(body): Json<AnalyzeStyleRequest>,
) -> Result<Json<StyleAnalysis>, ApiError> {
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

    let system_prompt = crate::prompts::analyze_style_prompt();

    let client = &state.http_client;
    let resp = client
        .post(format!("{}/chat/completions", base_url))
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&serde_json::json!({
            "model": model,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": body.text}
            ],
            "temperature": 0.2,
            "max_tokens": 2048,
        }))
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("DeepSeek error: {}", e)))?;

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ApiError::Internal(format!("Parse error: {}", e)))?;

    let content = data["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("{}");

    if let Ok(analysis) = serde_json::from_str::<StyleAnalysis>(content) {
        Ok(Json(analysis))
    } else {
        Ok(Json(StyleAnalysis {
            tone: "未知".to_string(),
            pov: "未知".to_string(),
            avg_sentence_length: 0.0,
            vocabulary_richness: 0.0,
            dialogue_ratio: 0.0,
            description_style: "未知".to_string(),
            pacing: "未知".to_string(),
            suggestions: vec![],
        }))
    }
}

#[derive(Deserialize)]
struct SuggestTagsRequest {
    title: String,
    description: Option<String>,
    content_sample: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct SuggestTagsResponse {
    genres: Vec<String>,
    tags: Vec<String>,
    themes: Vec<String>,
}

async fn suggest_tags(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SuggestTagsRequest>,
) -> Result<Json<SuggestTagsResponse>, ApiError> {
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

    let input = format!(
        "标题: {}\n描述: {}\n内容片段: {}",
        body.title,
        body.description.as_deref().unwrap_or("无"),
        body.content_sample.as_deref().unwrap_or("无")
    );

    let client = &state.http_client;
    let resp = client
        .post(format!("{}/chat/completions", base_url))
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&serde_json::json!({
            "model": model,
            "messages": [
                {"role": "system", "content": crate::prompts::suggest_tags_prompt()},
                {"role": "user", "content": input}
            ],
            "temperature": 0.3,
            "max_tokens": 512,
        }))
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("DeepSeek error: {}", e)))?;

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ApiError::Internal(format!("Parse error: {}", e)))?;

    let content = data["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("{}");

    if let Ok(tags) = serde_json::from_str::<SuggestTagsResponse>(content) {
        Ok(Json(tags))
    } else {
        Ok(Json(SuggestTagsResponse {
            genres: vec![],
            tags: vec![],
            themes: vec![],
        }))
    }
}

#[derive(Deserialize)]
struct GenerateOutlineRequest {
    premise: String,
    genre: Option<String>,
    chapter_count: Option<usize>,
}

#[derive(Serialize, Deserialize)]
struct GenerateOutlineResponse {
    title_suggestions: Vec<String>,
    chapters: Vec<OutlineChapter>,
}

#[derive(Serialize, Deserialize)]
struct OutlineChapter {
    title: String,
    summary: String,
    key_events: Vec<String>,
}

async fn generate_outline(
    State(state): State<Arc<AppState>>,
    Json(body): Json<GenerateOutlineRequest>,
) -> Result<Json<GenerateOutlineResponse>, ApiError> {
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

    let chapter_count = body.chapter_count.unwrap_or(10);
    let genre = body.genre.as_deref().unwrap_or("玄幻");

    let system_prompt = crate::prompts::generate_outline_prompt(chapter_count, genre);

    let client = &state.http_client;
    let resp = client
        .post(format!("{}/chat/completions", base_url))
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&serde_json::json!({
            "model": model,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": body.premise}
            ],
            "temperature": 0.7,
            "max_tokens": 8192,
        }))
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("DeepSeek error: {}", e)))?;

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ApiError::Internal(format!("Parse error: {}", e)))?;

    let content = data["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("{}");

    if let Ok(outline) = serde_json::from_str::<GenerateOutlineResponse>(content) {
        Ok(Json(outline))
    } else {
        Ok(Json(GenerateOutlineResponse {
            title_suggestions: vec![],
            chapters: vec![],
        }))
    }
}

// ─── Batch Processing: Full Novel AI Pipeline ────────────────────────────────

#[derive(Deserialize)]
struct BatchProcessRequest {
    book_id: String,
    /// Which operations to perform in the batch
    operations: Vec<String>, // "summarize", "entities", "tags", "style", "embeddings"
}

#[derive(Serialize)]
struct BatchProcessResponse {
    book_id: String,
    status: String,
    /// Results will be stored in DB; this is a summary
    chapters_processed: usize,
    entities_found: usize,
    tags_generated: Vec<String>,
    style: Option<serde_json::Value>,
}

/// Batch process an entire book: summarize + extract entities + tag + analyze style + generate embeddings
/// This is the "让 AI 一口气读完整本小说" endpoint.
async fn batch_process_book(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<BatchProcessRequest>,
) -> Result<Json<BatchProcessResponse>, ApiError> {
    crate::feature_flags::require_feature(&state.db, "ai_batch_process").await?;

    // Load all chapters for this book
    let book_uuid =
        ensure_ai_book_access_from_str(&state, &auth, &body.book_id, LibraryAccess::Write).await?;

    let chapters_rows = sqlx::query_as::<_, (String, String)>(
        "SELECT COALESCE(title, ''), content FROM chapters WHERE book_id = $1 ORDER BY index",
    )
    .bind(book_uuid)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("DB error: {}", e)))?;

    let embedding_contract = if body.operations.contains(&"embeddings".to_string()) {
        ensure_qdrant_collection(
            &state.http_client,
            &state.config.qdrant_url,
            state.config.embedding_dimensions,
        )
        .await?;
        let contract = load_embedding_freshness_contract(
            &state.chapters,
            book_uuid,
            &state.config.embedding_model,
            state.config.embedding_dimensions,
        )
        .await
        .map_err(ApiError::Internal)?;
        delete_book_embedding_points(&state.http_client, &state.config.qdrant_url, book_uuid)
            .await
            .map_err(ApiError::Internal)?;
        Some(contract)
    } else {
        None
    };

    if chapters_rows.is_empty() {
        return Err(ApiError::NotFound(
            "No chapters found for this book".to_string(),
        ));
    }

    let chapters: Vec<(String, String)> = chapters_rows;
    let mut entities_found = 0;
    let mut tags_generated: Vec<String> = Vec::new();
    let mut style_result: Option<serde_json::Value> = None;

    // Entity extraction (batch by 5 chapters)
    if body.operations.contains(&"entities".to_string()) {
        for batch in chapters.chunks(5) {
            let batch_text: String = batch
                .iter()
                .map(|(title, content)| {
                    format!("## {}\n{}", title, &content[..content.len().min(3000)])
                })
                .collect::<Vec<_>>()
                .join("\n\n");

            let prompt = format!(
                "Extract all named entities (characters, locations, organizations, items) from this text.\n\
                Output JSON array: [{{\"name\": \"...\", \"type\": \"person|location|organization|item\", \"description\": \"brief\", \"aliases\": []}}]\n\n{}",
                &batch_text[..batch_text.len().min(12000)]
            );

            if let Ok(response) = call_ai(
                &state,
                crate::prompts::entity_extraction_prompt(),
                &prompt,
                0.2,
                4096,
            )
            .await
            {
                if let Ok(entities) = serde_json::from_str::<Vec<serde_json::Value>>(&response) {
                    for entity in &entities {
                        let _ = sqlx::query(
                            "INSERT INTO entities (id, book_id, name, entity_type, description, aliases)
                             VALUES ($1, $2, $3, $4, $5, $6)
                             ON CONFLICT (book_id, name) DO UPDATE SET description = $5, aliases = $6"
                        )
                        .bind(Uuid::new_v4())
                        .bind(book_uuid)
                        .bind(entity["name"].as_str().unwrap_or(""))
                        .bind(entity["type"].as_str().unwrap_or("unknown"))
                        .bind(entity["description"].as_str().unwrap_or(""))
                        .bind(serde_json::to_value(entity["aliases"].as_array().unwrap_or(&vec![])).unwrap_or_default())
                        .execute(&state.db)
                        .await;
                        entities_found += 1;
                    }
                }
            }
        }

        // Sync entities to Neo4j after extraction
        match nova_graph::sync_book_to_neo4j(&state.db, &state.neo4j, book_uuid).await {
            Ok(sync_result) => {
                tracing::info!(
                    "Graph sync: {} entities, {} relationships synced",
                    sync_result.entities_synced,
                    sync_result.relationships_synced
                );
            }
            Err(e) => {
                tracing::warn!("Graph sync failed (non-fatal): {}", e);
            }
        }
    }

    // Tag generation
    if body.operations.contains(&"tags".to_string()) {
        let sample = chapters
            .iter()
            .take(5)
            .map(|(_, c)| &c[..c.len().min(2000)])
            .collect::<Vec<_>>()
            .join("\n");

        if let Ok(response) = call_ai(
            &state,
            crate::prompts::suggest_tags_prompt(),
            &sample,
            0.3,
            1024,
        )
        .await
        {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&response) {
                if let Some(tags) = parsed["tags"].as_array() {
                    tags_generated = tags
                        .iter()
                        .filter_map(|t| t.as_str().map(|s| s.to_string()))
                        .collect();
                    let tags_json = serde_json::to_value(&tags_generated).unwrap_or_default();
                    let _ = sqlx::query("UPDATE books SET tags = $1 WHERE id = $2")
                        .bind(tags_json)
                        .bind(book_uuid)
                        .execute(&state.db)
                        .await;
                }
            }
        }
    }

    // Style analysis
    if body.operations.contains(&"style".to_string()) {
        let sample = chapters
            .iter()
            .take(3)
            .map(|(_, c)| &c[..c.len().min(3000)])
            .collect::<Vec<_>>()
            .join("\n---\n");

        if let Ok(response) = call_ai(
            &state,
            crate::prompts::analyze_style_prompt(),
            &sample,
            0.3,
            2048,
        )
        .await
        {
            style_result = serde_json::from_str(&response).ok();
        }
    }

    // Embeddings (chunking + vector storage) — concurrent processing
    if let Some(embedding_contract) = embedding_contract.as_ref() {
        let embedding_chapters = state
            .chapters
            .list_searchable_by_book(book_uuid)
            .await
            .map_err(|error| ApiError::Internal(format!("DB error: {error}")))?;
        let embedding_endpoint = &state.config.embedding_endpoint;
        let embedding_model = &state.config.embedding_model;
        let qdrant_url = &state.config.qdrant_url;

        let chunker = nova_embed::chunker_v2::NovelChunker::new(
            nova_embed::chunker_v2::ChunkingConfigV2::default(),
        );

        // Concurrency limit: process up to 4 chapters at a time
        const EMBED_CONCURRENCY: usize = 4;

        let embed_futures: Vec<_> = embedding_chapters
            .iter()
            .map(|chapter| {
                let chapter_index = chapter.chapter_index;
                let chunks = chunker.chunk_document(&chapter.content, Some(chapter.title.as_str()));
                let chunk_texts: Vec<String> = chunks.iter().map(|c| c.content.clone()).collect();
                let client = &state.http_client;
                let title = chapter.title.clone();
                let endpoint = embedding_endpoint.clone();
                let model = embedding_model.clone();
                let qdrant = qdrant_url.clone();
                let api_key = state.config.embedding_api_key.clone();
                let dimensions = state.config.embedding_dimensions;
                let embedding_contract = embedding_contract.clone();

                async move {
                    // Process batches within this chapter concurrently (up to 3 at a time)
                    let batch_futures: Vec<_> = chunk_texts
                        .chunks(16)
                        .enumerate()
                        .map(|(batch_idx, batch)| {
                            let batch = batch.to_vec();
                            let client = client.clone();
                            let title = title.clone();
                            let endpoint = endpoint.clone();
                            let model = model.clone();
                            let qdrant = qdrant.clone();
                            let api_key = api_key.clone();
                            let embedding_contract = embedding_contract.clone();

                            async move {
                                let batch_refs: Vec<&str> =
                                    batch.iter().map(|s| s.as_str()).collect();
                                let mut embed_req = client
                                    .post(format!("{}/v1/embeddings", endpoint))
                                    .header("X-Failover-Enabled", "true")
                                    .json(&serde_json::json!({
                                        "input": batch_refs,
                                        "model": model,
                                        "dimensions": dimensions,
                                    }));

                                if !api_key.is_empty() {
                                    embed_req = embed_req
                                        .header("Authorization", format!("Bearer {}", api_key));
                                }

                                let embed_resp = embed_req.send().await;

                                if let Ok(resp) = embed_resp {
                                    if let Ok(data) = resp.json::<serde_json::Value>().await {
                                        if let Some(embeddings) = data["data"].as_array() {
                                            let mut points = Vec::new();
                                            for item in embeddings {
                                                let chunk_idx =
                                                    item["index"].as_u64().unwrap_or(0) as usize;
                                                let global_idx = batch_idx * 16 + chunk_idx;
                                                if let Some(vector) = item["embedding"].as_array() {
                                                    let vec: Vec<f64> = vector
                                                        .iter()
                                                        .filter_map(|v| v.as_f64())
                                                        .collect();
                                                    let point_id = embedding_point_id(
                                                        book_uuid,
                                                        chapter_index,
                                                        global_idx,
                                                    );
                                                    let text = batch
                                                        .get(chunk_idx)
                                                        .map(String::as_str)
                                                        .unwrap_or_default();
                                                    points.push(serde_json::json!({
                                                        "id": point_id,
                                                        "vector": vec,
                                                        "payload": embedding_contract.chunk_payload(
                                                            book_uuid,
                                                            None,
                                                            chapter_index,
                                                            &title,
                                                            global_idx,
                                                            text,
                                                        ),
                                                    }));
                                                }
                                            }
                                            if !points.is_empty() {
                                                let _ = client
                                                    .put(format!(
                                                        "{}/collections/nova_chunks/points",
                                                        qdrant
                                                    ))
                                                    .json(&serde_json::json!({ "points": points }))
                                                    .send()
                                                    .await;
                                            }
                                        }
                                    }
                                }
                            }
                        })
                        .collect();

                    // Process batches within a chapter with concurrency of 3
                    stream::iter(batch_futures)
                        .buffer_unordered(3)
                        .collect::<Vec<_>>()
                        .await;
                }
            })
            .collect();

        // Process chapters with concurrency limit
        stream::iter(embed_futures)
            .buffer_unordered(EMBED_CONCURRENCY)
            .collect::<Vec<_>>()
            .await;
    }

    Ok(Json(BatchProcessResponse {
        book_id: body.book_id,
        status: "completed".to_string(),
        chapters_processed: chapters.len(),
        entities_found,
        tags_generated,
        style: style_result,
    }))
}

// ─── SSE Batch Processing with Progress ─────────────────────────────────────

/// SSE-based batch processing that streams progress events to the client.
async fn batch_process_stream(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<BatchProcessRequest>,
) -> Result<
    axum::response::Sse<
        impl futures::stream::Stream<
            Item = Result<axum::response::sse::Event, std::convert::Infallible>,
        >,
    >,
    ApiError,
> {
    crate::feature_flags::require_feature(&state.db, "ai_batch_process").await?;

    let book_uuid =
        ensure_ai_book_access_from_str(&state, &auth, &body.book_id, LibraryAccess::Write).await?;

    let chapters: Vec<(String, String)> = sqlx::query_as::<_, (String, String)>(
        "SELECT COALESCE(title, ''), content FROM chapters WHERE book_id = $1 ORDER BY index",
    )
    .bind(book_uuid)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("DB error: {}", e)))?;

    let total_ops = body.operations.len();
    let total_chapters = chapters.len();
    let embedding_contract = if body.operations.contains(&"embeddings".to_string()) {
        let contract = load_embedding_freshness_contract(
            &state.chapters,
            book_uuid,
            &state.config.embedding_model,
            state.config.embedding_dimensions,
        )
        .await
        .map_err(ApiError::Internal)?;
        ensure_qdrant_collection(
            &state.http_client,
            &state.config.qdrant_url,
            state.config.embedding_dimensions,
        )
        .await?;
        delete_book_embedding_points(&state.http_client, &state.config.qdrant_url, book_uuid)
            .await
            .map_err(ApiError::Internal)?;
        Some(contract)
    } else {
        None
    };
    let embedding_chapters = if embedding_contract.is_some() {
        state
            .chapters
            .list_searchable_by_book(book_uuid)
            .await
            .map_err(|error| ApiError::Internal(format!("DB error: {error}")))?
    } else {
        Vec::new()
    };
    let total_embedding_chapters = embedding_chapters.len();

    if chapters.is_empty() {
        return Err(ApiError::NotFound(
            "No chapters found for this book".to_string(),
        ));
    }

    let stream = async_stream::stream! {
        let mut current_op = 0;

        // Initial event
        yield Ok(axum::response::sse::Event::default().data(
            serde_json::json!({
                "type": "start",
                "book_id": body.book_id,
                "total_chapters": total_chapters,
                "operations": body.operations,
            }).to_string()
        ));

        // Entity extraction
        if body.operations.contains(&"entities".to_string()) {
            current_op += 1;
            let mut entities_found = 0;
            for (idx, batch) in chapters.chunks(5).enumerate() {
                let progress = ((idx * 5) as f64 / total_chapters as f64 * 100.0).min(100.0);
                yield Ok(axum::response::sse::Event::default().data(
                    serde_json::json!({
                        "type": "progress",
                        "operation": "entities",
                        "op_index": current_op,
                        "total_ops": total_ops,
                        "percent": progress,
                        "message": format!("Extracting entities... batch {}/{}", idx + 1, (total_chapters + 4) / 5),
                    }).to_string()
                ));

                let batch_text: String = batch.iter()
                    .map(|(title, content)| format!("## {}\n{}", title, &content[..content.len().min(3000)]))
                    .collect::<Vec<_>>()
                    .join("\n\n");

                let prompt = format!(
                    "Extract all named entities from this text.\nOutput JSON array: [{{\"name\": \"...\", \"type\": \"person|location|organization|item\", \"description\": \"brief\", \"aliases\": []}}]\n\n{}",
                    &batch_text[..batch_text.len().min(12000)]
                );

                if let Ok(response) = call_ai(&state, crate::prompts::entity_extraction_prompt(), &prompt, 0.2, 4096).await {
                    if let Ok(entities) = serde_json::from_str::<Vec<serde_json::Value>>(&response) {
                        for entity in &entities {
                            let _ = sqlx::query(
                                "INSERT INTO entities (id, book_id, name, entity_type, description, aliases)
                                 VALUES ($1, $2, $3, $4, $5, $6)
                                 ON CONFLICT (book_id, name) DO UPDATE SET description = $5, aliases = $6"
                            )
                            .bind(Uuid::new_v4())
                            .bind(book_uuid)
                            .bind(entity["name"].as_str().unwrap_or(""))
                            .bind(entity["type"].as_str().unwrap_or("unknown"))
                            .bind(entity["description"].as_str().unwrap_or(""))
                            .bind(serde_json::to_value(entity["aliases"].as_array().unwrap_or(&vec![])).unwrap_or_default())
                            .execute(&state.db)
                            .await;
                            entities_found += 1;
                        }
                    }
                }
            }

            yield Ok(axum::response::sse::Event::default().data(
                serde_json::json!({
                    "type": "op_complete",
                    "operation": "entities",
                    "result": { "entities_found": entities_found },
                }).to_string()
            ));
        }

        // Tag generation
        if body.operations.contains(&"tags".to_string()) {
            current_op += 1;
            yield Ok(axum::response::sse::Event::default().data(
                serde_json::json!({
                    "type": "progress",
                    "operation": "tags",
                    "op_index": current_op,
                    "total_ops": total_ops,
                    "percent": 50.0,
                    "message": "Generating tags...",
                }).to_string()
            ));

            let sample = chapters.iter().take(5)
                .map(|(_, c)| &c[..c.len().min(2000)])
                .collect::<Vec<_>>()
                .join("\n");

            let mut tags_generated = Vec::new();
            if let Ok(response) = call_ai(&state, crate::prompts::suggest_tags_prompt(), &sample, 0.3, 1024).await {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&response) {
                    if let Some(tags) = parsed["tags"].as_array() {
                        tags_generated = tags.iter().filter_map(|t| t.as_str().map(|s| s.to_string())).collect();
                        let tags_json = serde_json::to_value(&tags_generated).unwrap_or_default();
                        let _ = sqlx::query("UPDATE books SET tags = $1 WHERE id = $2")
                            .bind(tags_json)
                            .bind(book_uuid)
                            .execute(&state.db)
                            .await;
                    }
                }
            }

            yield Ok(axum::response::sse::Event::default().data(
                serde_json::json!({
                    "type": "op_complete",
                    "operation": "tags",
                    "result": { "tags": tags_generated },
                }).to_string()
            ));
        }

        // Embeddings
        if let Some(embedding_contract) = embedding_contract.as_ref() {
            current_op += 1;
            let embedding_endpoint = &state.config.embedding_endpoint;
            let embedding_model = &state.config.embedding_model;
            let qdrant_url = &state.config.qdrant_url;

            let chunker = nova_embed::chunker_v2::NovelChunker::new(nova_embed::chunker_v2::ChunkingConfigV2::default());

            for (embedded_index, chapter) in embedding_chapters.iter().enumerate() {
                let progress = (embedded_index as f64 / total_embedding_chapters.max(1) as f64 * 100.0).min(100.0);
                yield Ok(axum::response::sse::Event::default().data(
                    serde_json::json!({
                        "type": "progress",
                        "operation": "embeddings",
                        "op_index": current_op,
                        "total_ops": total_ops,
                        "percent": progress,
                        "message": format!("Embedding chapter {}/{}", embedded_index + 1, total_embedding_chapters),
                    }).to_string()
                ));

                let chunks = chunker.chunk_document(&chapter.content, Some(chapter.title.as_str()));
                let chunk_texts: Vec<String> = chunks.iter().map(|c| c.content.clone()).collect();

                // Process batches concurrently (up to 3 at a time)
                let batch_futures: Vec<_> = chunk_texts.chunks(16).enumerate().map(|(batch_idx, batch)| {
                    let batch = batch.to_vec();
                    let client = state.http_client.clone();
                    let endpoint = embedding_endpoint.clone();
                    let model = embedding_model.clone();
                    let qdrant = qdrant_url.clone();
                    let api_key = state.config.embedding_api_key.clone();
                    let dimensions = state.config.embedding_dimensions;
                    let title = chapter.title.clone();
                    let chapter_index = chapter.chapter_index;
                    let embedding_contract = embedding_contract.clone();

                    async move {
                        let batch_refs: Vec<&str> = batch.iter().map(|s| s.as_str()).collect();
                        let mut embed_req = client
                            .post(format!("{}/v1/embeddings", endpoint))
                            .header("X-Failover-Enabled", "true")
                            .json(&serde_json::json!({
                                "input": batch_refs,
                                "model": model,
                                "dimensions": dimensions,
                            }));

                        if !api_key.is_empty() {
                            embed_req = embed_req.header("Authorization", format!("Bearer {}", api_key));
                        }

                        if let Ok(resp) = embed_req.send().await {
                            if let Ok(data) = resp.json::<serde_json::Value>().await {
                                if let Some(embeddings) = data["data"].as_array() {
                                    let mut points = Vec::new();
                                    for item in embeddings {
                                        let chunk_idx = item["index"].as_u64().unwrap_or(0) as usize;
                                        let global_idx = batch_idx * 16 + chunk_idx;
                                        if let Some(vector) = item["embedding"].as_array() {
                                            let vec: Vec<f64> = vector.iter()
                                                .filter_map(|v| v.as_f64())
                                                .collect();
                                            let point_id = embedding_point_id(
                                                book_uuid,
                                                chapter_index,
                                                global_idx,
                                            );
                                            let text = batch
                                                .get(chunk_idx)
                                                .map(String::as_str)
                                                .unwrap_or_default();
                                            points.push(serde_json::json!({
                                                "id": point_id,
                                                "vector": vec,
                                                "payload": embedding_contract.chunk_payload(
                                                    book_uuid,
                                                    None,
                                                    chapter_index,
                                                    &title,
                                                    global_idx,
                                                    text,
                                                ),
                                            }));
                                        }
                                    }
                                    if !points.is_empty() {
                                        let _ = client
                                            .put(format!("{}/collections/nova_chunks/points", qdrant))
                                            .json(&serde_json::json!({ "points": points }))
                                            .send()
                                            .await;
                                    }
                                }
                            }
                        }
                    }
                }).collect();

                stream::iter(batch_futures)
                    .buffer_unordered(3)
                    .collect::<Vec<_>>()
                    .await;
            }

            yield Ok(axum::response::sse::Event::default().data(
                serde_json::json!({
                    "type": "op_complete",
                    "operation": "embeddings",
                    "result": { "chapters_embedded": total_embedding_chapters },
                }).to_string()
            ));
        }

        // Done
        yield Ok(axum::response::sse::Event::default().data(
            serde_json::json!({
                "type": "done",
                "book_id": body.book_id,
            }).to_string()
        ));
    };

    Ok(axum::response::Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default()))
}

// ─── Embedding Ingestion ─────────────────────────────────────────────────────

#[derive(Deserialize)]
struct IngestEmbeddingsRequest {
    book_id: String,
    /// Legacy compatibility field. Freshness-safe rebuilds require this to be
    /// omitted or to contain every searchable chapter.
    chapter_indices: Option<Vec<usize>>,
}

#[derive(Serialize)]
struct IngestEmbeddingsResponse {
    chunks_indexed: usize,
    collection: String,
}

fn validate_full_embedding_chapter_selection(
    chapter_indices: Option<&[usize]>,
    chapters: &[SearchableChapterRecord],
) -> ApiResult<()> {
    let Some(chapter_indices) = chapter_indices else {
        return Ok(());
    };
    let requested: BTreeSet<_> = chapter_indices.iter().copied().collect();
    let expected: BTreeSet<_> = chapters
        .iter()
        .map(|chapter| {
            usize::try_from(chapter.chapter_index).map_err(|_| {
                ApiError::Internal(format!(
                    "negative searchable chapter index for embedding rebuild: {}",
                    chapter.chapter_index
                ))
            })
        })
        .collect::<ApiResult<_>>()?;
    if requested != expected {
        return Err(ApiError::bad_request(
            "chapter_indices must include every searchable chapter; partial embedding rebuilds cannot carry a full-book freshness hash",
        ));
    }
    Ok(())
}

/// Generate embeddings for book chapters and store in Qdrant.
async fn ingest_embeddings(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<IngestEmbeddingsRequest>,
) -> Result<Json<IngestEmbeddingsResponse>, ApiError> {
    let embedding_endpoint = &state.config.embedding_endpoint;
    let embedding_model = &state.config.embedding_model;
    let embedding_api_key = &state.config.embedding_api_key;
    let embedding_dimensions = state.config.embedding_dimensions;
    let qdrant_url = &state.config.qdrant_url;

    let book_uuid =
        ensure_ai_book_access_from_str(&state, &auth, &body.book_id, LibraryAccess::Write).await?;

    // Ensure Qdrant collection exists with correct dimensions
    ensure_qdrant_collection(&state.http_client, qdrant_url, embedding_dimensions).await?;
    let embedding_contract = load_embedding_freshness_contract(
        &state.chapters,
        book_uuid,
        embedding_model,
        embedding_dimensions,
    )
    .await
    .map_err(ApiError::Internal)?;

    // Fetch chapters
    let chapters = state
        .chapters
        .list_searchable_by_book(book_uuid)
        .await
        .map_err(|e| ApiError::Internal(format!("DB error: {}", e)))?;
    validate_full_embedding_chapter_selection(body.chapter_indices.as_deref(), &chapters)?;
    delete_book_embedding_points(&state.http_client, qdrant_url, book_uuid)
        .await
        .map_err(ApiError::Internal)?;

    // Fetch book title for metadata
    let book_title: String = sqlx::query_scalar("SELECT title FROM books WHERE id = $1")
        .bind(book_uuid)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "Unknown".to_string());

    let client = &state.http_client;
    let mut total_chunks = 0usize;

    // Use chunker_v2 for semantic chunking (dialogue protection, CJK-aware)
    let chunker = nova_embed::chunker_v2::NovelChunker::new(
        nova_embed::chunker_v2::ChunkingConfigV2::default(),
    );

    for chapter in &chapters {
        if chapter.content.trim().is_empty() {
            continue;
        }

        // Semantic chunking: respects paragraph boundaries, CJK-aware
        let chunks = chunker.chunk_document(&chapter.content, Some(chapter.title.as_str()));
        let chunk_texts: Vec<String> = chunks.iter().map(|c| c.content.clone()).collect();

        // Pre-split into owned batches to avoid lifetime issues
        let batches: Vec<Vec<String>> = chunk_texts.chunks(16).map(|b| b.to_vec()).collect();

        // Batch embed concurrently (up to 3 batches at a time)
        let batch_results: Vec<Result<usize, ApiError>> =
            stream::iter(batches.into_iter().enumerate().map(|(batch_idx, batch)| {
                let client = client.clone();
                let endpoint = embedding_endpoint.clone();
                let model = embedding_model.clone();
                let api_key = embedding_api_key.clone();
                let qdrant = qdrant_url.clone();
                let book_title = book_title.clone();
                let title = chapter.title.clone();
                let idx = chapter.chapter_index;
                let embedding_contract = embedding_contract.clone();

                async move {
                    let texts: Vec<&str> = batch.iter().map(|s| s.as_str()).collect();

                    let mut request = client
                        .post(format!("{}/v1/embeddings", endpoint))
                        .header("Content-Type", "application/json")
                        .header("X-Failover-Enabled", "true")
                        .json(&serde_json::json!({
                            "input": texts,
                            "model": model,
                            "dimensions": embedding_dimensions,
                        }));

                    if !api_key.is_empty() {
                        request = request.header("Authorization", format!("Bearer {}", api_key));
                    }

                    let embed_resp = request
                        .send()
                        .await
                        .map_err(|e| ApiError::Internal(format!("Embedding error: {}", e)))?;

                    if !embed_resp.status().is_success() {
                        let status = embed_resp.status();
                        let body_text = embed_resp.text().await.unwrap_or_default();
                        return Err(ApiError::Internal(format!(
                            "Embedding API error ({}): {}",
                            status, body_text
                        )));
                    }

                    let data: serde_json::Value = embed_resp
                        .json()
                        .await
                        .map_err(|e| ApiError::Internal(format!("Parse error: {}", e)))?;

                    let mut chunk_count = 0usize;
                    if let Some(embeddings) = data["data"].as_array() {
                        let mut points = Vec::new();
                        for item in embeddings.iter() {
                            let chunk_idx = item["index"].as_u64().unwrap_or(0) as usize;
                            let global_chunk_idx = batch_idx * 16 + chunk_idx;
                            if let Some(vec) = item["embedding"].as_array() {
                                let vector: Vec<f64> =
                                    vec.iter().filter_map(|v| v.as_f64()).collect();

                                let point_id = embedding_point_id(book_uuid, idx, global_chunk_idx);
                                let text =
                                    batch.get(chunk_idx).map(String::as_str).unwrap_or_default();

                                points.push(serde_json::json!({
                                    "id": point_id,
                                    "vector": vector,
                                    "payload": embedding_contract.chunk_payload(
                                        book_uuid,
                                        Some(&book_title),
                                        idx,
                                        &title,
                                        global_chunk_idx,
                                        text,
                                    ),
                                }));
                            }
                        }

                        if !points.is_empty() {
                            if let Err(e) = client
                                .put(format!("{}/collections/nova_chunks/points", qdrant))
                                .json(&serde_json::json!({ "points": points }))
                                .send()
                                .await
                            {
                                tracing::error!("Qdrant upsert failed: {}", e);
                            }
                        }

                        chunk_count = embeddings.len();
                    }
                    Ok(chunk_count)
                }
            }))
            .buffer_unordered(3)
            .collect()
            .await;

        for result in batch_results {
            match result {
                Ok(count) => total_chunks += count,
                Err(_e) => {
                    tracing::warn!(
                        "Batch embedding failed for chapter {}",
                        chapter.chapter_index
                    );
                    // Continue processing other chapters instead of failing entirely
                }
            }
        }
    }

    // Also index in Meilisearch for keyword search
    index_chunks_in_meilisearch(&state, &body.book_id, &book_title, &chapters, &chunker).await;

    tracing::info!(
        book_id = %body.book_id,
        chunks = total_chunks,
        "Embedding ingestion complete"
    );

    Ok(Json(IngestEmbeddingsResponse {
        chunks_indexed: total_chunks,
        collection: "nova_chunks".to_string(),
    }))
}

/// Ensure the Qdrant collection exists with the correct vector dimensions.
async fn ensure_qdrant_collection(
    client: &reqwest::Client,
    qdrant_url: &str,
    dimensions: usize,
) -> Result<(), ApiError> {
    // Check if collection exists
    let resp = client
        .get(format!("{}/collections/nova_chunks", qdrant_url))
        .send()
        .await;

    match resp {
        Ok(r) if r.status().is_success() => Ok(()), // Already exists
        _ => {
            // Create collection
            let create_resp = client
                .put(format!("{}/collections/nova_chunks", qdrant_url))
                .json(&serde_json::json!({
                    "vectors": {
                        "size": dimensions,
                        "distance": "Cosine"
                    },
                    "optimizers_config": {
                        "indexing_threshold": 20000
                    },
                    "on_disk_payload": true
                }))
                .send()
                .await
                .map_err(|e| ApiError::Internal(format!("Qdrant unreachable: {}", e)))?;

            if !create_resp.status().is_success() {
                let body = create_resp.text().await.unwrap_or_default();
                return Err(ApiError::Internal(format!(
                    "Failed to create Qdrant collection: {}",
                    body
                )));
            }

            tracing::info!(dimensions, "Created Qdrant collection nova_chunks");
            Ok(())
        }
    }
}

/// Index chunks in Meilisearch for keyword search (hybrid retrieval).
async fn index_chunks_in_meilisearch(
    state: &AppState,
    book_id: &str,
    book_title: &str,
    chapters: &[SearchableChapterRecord],
    chunker: &nova_embed::chunker_v2::NovelChunker,
) {
    let mut documents = Vec::new();

    for chapter in chapters {
        if chapter.content.trim().is_empty() {
            continue;
        }
        let chunks = chunker.chunk_document(&chapter.content, Some(book_title));
        for chunk in &chunks {
            documents.push(serde_json::json!({
                "id": format!("{}_{}_{}", book_id, chapter.chapter_index, chunk.index),
                "book_id": book_id,
                "book_title": book_title,
                "chapter_title": chapter.title,
                "chapter_index": chapter.chapter_index,
                "content": chunk.content,
                "chunk_index": chunk.index,
            }));
        }
    }

    if documents.is_empty() {
        return;
    }

    // Upsert to Meilisearch in batches of 1000
    for batch in documents.chunks(1000) {
        let resp = state
            .http_client
            .post(format!(
                "{}/indexes/chunks/documents",
                state.config.meili_url
            ))
            .header(
                "Authorization",
                format!("Bearer {}", state.config.meili_master_key),
            )
            .header("Content-Type", "application/json")
            .json(&batch)
            .send()
            .await;

        if let Err(e) = resp {
            tracing::warn!("Meilisearch indexing failed: {}", e);
        }
    }
}

// ─── Embedding Index Management ──────────────────────────────────────────────

/// GET /ai/embeddings/status - Get Qdrant collection info (point count, size, etc.)
async fn embedding_index_status(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_ai_admin_access(&state, &auth).await?;
    let qdrant_url = if state.config.qdrant_url.is_empty() {
        "http://localhost:6333"
    } else {
        &state.config.qdrant_url
    };

    let resp = state
        .http_client
        .get(format!("{}/collections/nova_chunks", qdrant_url))
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("Qdrant unreachable: {}", e)))?;

    if !resp.status().is_success() {
        return Ok(Json(serde_json::json!({
            "status": "not_initialized",
            "message": "Collection nova_chunks does not exist yet"
        })));
    }

    let data: serde_json::Value = resp.json().await.unwrap_or_default();
    let result = &data["result"];

    // Also count indexed books from DB
    let indexed_books: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT book_id)::bigint FROM chapters WHERE book_id IN (SELECT DISTINCT id FROM books)"
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    Ok(Json(serde_json::json!({
        "status": "active",
        "collection": "nova_chunks",
        "points_count": result["points_count"],
        "vectors_count": result["vectors_count"],
        "segments_count": result["segments_count"],
        "indexed_books": indexed_books,
        "config": result["config"],
    })))
}

#[derive(Deserialize)]
struct EmbeddingDeleteRequest {
    book_id: String,
}

/// POST /ai/embeddings/delete-book - Remove all vectors for a specific book
async fn embedding_delete_book(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<EmbeddingDeleteRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let book_id =
        ensure_ai_book_access_from_str(&state, &auth, &body.book_id, LibraryAccess::Write).await?;
    let qdrant_url = if state.config.qdrant_url.is_empty() {
        "http://localhost:6333"
    } else {
        &state.config.qdrant_url
    };

    delete_book_embedding_points(&state.http_client, qdrant_url, book_id)
        .await
        .map_err(ApiError::Internal)?;

    Ok(Json(serde_json::json!({
        "message": format!("Deleted vectors for book {}", body.book_id),
        "status": 200,
    })))
}

/// POST /ai/embeddings/rebuild - Drop and recreate the collection, then re-index all books
async fn embedding_index_rebuild(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_ai_admin_access(&state, &auth).await?;
    let qdrant_url = if state.config.qdrant_url.is_empty() {
        "http://localhost:6333"
    } else {
        &state.config.qdrant_url
    };

    // Delete existing collection
    let _ = state
        .http_client
        .delete(format!("{}/collections/nova_chunks", qdrant_url))
        .send()
        .await;

    // Recreate with the configured embedding contract (Qwen3 is 2,560d in
    // the default Nova deployment).
    let dim = state.config.embedding_dimensions;
    let create_resp = state
        .http_client
        .put(format!("{}/collections/nova_chunks", qdrant_url))
        .json(&serde_json::json!({
            "vectors": {
                "size": dim,
                "distance": "Cosine"
            }
        }))
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to create collection: {}", e)))?;

    if !create_resp.status().is_success() {
        let text = create_resp.text().await.unwrap_or_default();
        return Err(ApiError::Internal(format!(
            "Qdrant create failed: {}",
            text
        )));
    }

    // Count books that need indexing
    let book_count: i64 = sqlx::query_scalar("SELECT COUNT(*)::bigint FROM books")
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

    Ok(Json(serde_json::json!({
        "message": "Collection rebuilt. Use /ai/ingest-embeddings per book or /ai/batch-process to re-index.",
        "collection": "nova_chunks",
        "vector_dimension": dim,
        "books_to_index": book_count,
    })))
}

// ─── Auto-Glossary Extraction ────────────────────────────────────────────────

#[derive(Deserialize)]
struct ExtractGlossaryRequest {
    pairs: Vec<BilingualPair>,
    source_lang: String,
    target_lang: String,
}

#[derive(Deserialize)]
struct BilingualPair {
    source: String,
    target: String,
}

#[derive(Serialize)]
struct GlossaryExtractionResponse {
    terms: Vec<ExtractedTerm>,
}

#[derive(Serialize)]
struct ExtractedTerm {
    source: String,
    target: String,
    category: String,
    context: String,
}

async fn extract_glossary(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ExtractGlossaryRequest>,
) -> Result<Json<GlossaryExtractionResponse>, ApiError> {
    let prompt = crate::prompts::glossary_extraction_prompt(&body.source_lang, &body.target_lang);

    let pairs_text: String = body
        .pairs
        .iter()
        .take(20) // Limit to 20 pairs per request
        .map(|p| format!("源文: {}\n译文: {}\n", p.source, p.target))
        .collect::<Vec<_>>()
        .join("---\n");

    let response = call_ai(&state, &prompt, &pairs_text, 0.2, 4096).await?;
    let parsed: serde_json::Value =
        serde_json::from_str(&response).unwrap_or_else(|_| serde_json::json!({"terms": []}));

    let terms = parsed["terms"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|t| {
            Some(ExtractedTerm {
                source: t["source"].as_str()?.to_string(),
                target: t["target"].as_str()?.to_string(),
                category: t["category"].as_str().unwrap_or("concept").to_string(),
                context: t["context"].as_str().unwrap_or("").to_string(),
            })
        })
        .collect();

    Ok(Json(GlossaryExtractionResponse { terms }))
}

// ─── Plot Hole Detection ─────────────────────────────────────────────────────

#[derive(Deserialize)]
struct PlotHoleRequest {
    book_id: String,
}

#[derive(Serialize)]
struct PlotHoleResponse {
    issues: Vec<PlotIssue>,
    consistency_score: u8,
    summary: String,
}

#[derive(Serialize)]
struct PlotIssue {
    severity: String,
    #[serde(rename = "type")]
    issue_type: String,
    description: String,
    chapters: Vec<usize>,
    entities: Vec<String>,
    suggestion: String,
}

async fn detect_plot_holes(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<PlotHoleRequest>,
) -> Result<Json<PlotHoleResponse>, ApiError> {
    let book_id =
        ensure_ai_book_access_from_str(&state, &auth, &body.book_id, LibraryAccess::Read).await?;
    // Fetch chapter summaries from database
    let summaries: Vec<(i32, String)> = sqlx::query_as(
        "SELECT chapter_index, summary FROM chapter_summaries WHERE book_id = $1 ORDER BY chapter_index"
    )
    .bind(book_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    if summaries.is_empty() {
        return Ok(Json(PlotHoleResponse {
            issues: vec![],
            consistency_score: 100,
            summary: "暂无章节摘要数据，请先运行 AI 全书分析".to_string(),
        }));
    }

    let chapter_summaries: String = summaries
        .iter()
        .map(|(idx, s)| format!("第{}章: {}", idx + 1, s))
        .collect::<Vec<_>>()
        .join("\n");

    // Fetch entity timeline
    let entity_timeline = "（实体时间线数据来自知识图谱）".to_string();

    let prompt = crate::prompts::plot_hole_detection_prompt(&chapter_summaries, &entity_timeline);
    let response = call_ai(&state, &prompt, "请分析以上内容的情节一致性", 0.3, 4096).await?;
    let parsed: serde_json::Value = serde_json::from_str(&response).unwrap_or_else(
        |_| serde_json::json!({"issues": [], "consistency_score": 85, "summary": "分析完成"}),
    );

    let issues = parsed["issues"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|i| {
            Some(PlotIssue {
                severity: i["severity"].as_str()?.to_string(),
                issue_type: i["type"].as_str()?.to_string(),
                description: i["description"].as_str()?.to_string(),
                chapters: i["chapters"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_u64().map(|n| n as usize))
                            .collect()
                    })
                    .unwrap_or_default(),
                entities: i["entities"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default(),
                suggestion: i["suggestion"].as_str().unwrap_or("").to_string(),
            })
        })
        .collect();

    Ok(Json(PlotHoleResponse {
        issues,
        consistency_score: parsed["consistency_score"].as_u64().unwrap_or(85) as u8,
        summary: parsed["summary"].as_str().unwrap_or("分析完成").to_string(),
    }))
}

// ─── Smart Chapter Title Generation ─────────────────────────────────────────

#[derive(Deserialize)]
struct ChapterTitleRequest {
    book_id: String,
    chapter_index: usize,
}

#[derive(Serialize)]
struct ChapterTitleResponse {
    titles: Vec<TitleSuggestion>,
    recommended: usize,
}

#[derive(Serialize)]
struct TitleSuggestion {
    title: String,
    style: String,
}

async fn generate_chapter_titles(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<ChapterTitleRequest>,
) -> Result<Json<ChapterTitleResponse>, ApiError> {
    let book_id =
        ensure_ai_book_access_from_str(&state, &auth, &body.book_id, LibraryAccess::Read).await?;
    // Fetch chapter content (first and last 500 chars as preview)
    let content: Option<(String,)> =
        sqlx::query_as("SELECT content FROM chapters WHERE book_id = $1 AND chapter_index = $2")
            .bind(book_id)
            .bind(body.chapter_index as i32)
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None);

    let full_content = content.map(|(c,)| c).unwrap_or_default();
    let preview = if full_content.len() > 1200 {
        format!(
            "【开头】\n{}\n\n【结尾】\n{}",
            &full_content[..600.min(full_content.len())],
            &full_content[full_content.len().saturating_sub(600)..]
        )
    } else {
        full_content
    };

    let prompt = crate::prompts::chapter_title_prompt(&preview, body.chapter_index);
    let response = call_ai(&state, &prompt, "", 0.7, 1024).await?;
    let parsed: serde_json::Value = serde_json::from_str(&response).unwrap_or_else(
        |_| serde_json::json!({"titles": [{"title": "无题", "style": "默认"}], "recommended": 0}),
    );

    let titles = parsed["titles"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|t| {
            Some(TitleSuggestion {
                title: t["title"].as_str()?.to_string(),
                style: t["style"].as_str().unwrap_or("").to_string(),
            })
        })
        .collect::<Vec<_>>();

    Ok(Json(ChapterTitleResponse {
        recommended: parsed["recommended"].as_u64().unwrap_or(0) as usize,
        titles: if titles.is_empty() {
            vec![TitleSuggestion {
                title: "无题".to_string(),
                style: "默认".to_string(),
            }]
        } else {
            titles
        },
    }))
}

// ─── Forum/Discuz Text Cleanup ──────────────────────────────────────────────

#[derive(Deserialize)]
struct CleanupRequest {
    text: String,
}

#[derive(Serialize)]
struct CleanupResponse {
    cleaned_text: String,
    removed_count: usize,
}

async fn cleanup_forum_text(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CleanupRequest>,
) -> Result<Json<CleanupResponse>, ApiError> {
    let original_lines = body.text.lines().count();
    let prompt = crate::prompts::forum_cleanup_prompt();
    let cleaned = call_ai(&state, prompt, &body.text, 0.1, 8192).await?;
    let cleaned_lines = cleaned.lines().count();

    Ok(Json(CleanupResponse {
        removed_count: original_lines.saturating_sub(cleaned_lines),
        cleaned_text: cleaned,
    }))
}

/// Helper: call DeepSeek/OpenAI-compatible API with system prompt and user message.
async fn call_ai(
    state: &AppState,
    system_prompt: &str,
    user_message: &str,
    temperature: f64,
    max_tokens: usize,
) -> Result<String, ApiError> {
    let ai = AiConfig::from_state(state);
    let mut messages = vec![ChatMessage {
        role: "system".to_string(),
        content: system_prompt.to_string(),
    }];
    if !user_message.is_empty() {
        messages.push(ChatMessage {
            role: "user".to_string(),
            content: user_message.to_string(),
        });
    }
    let (content, _usage) = ai.complete(&messages, temperature, max_tokens).await?;
    Ok(content)
}

/// Call AI with raw JSON messages array (for endpoints that build messages dynamically).
async fn call_ai_json(
    state: &AppState,
    messages: &[serde_json::Value],
    temperature: f64,
    max_tokens: usize,
) -> Result<String, ApiError> {
    let ai = AiConfig::from_state(state);
    let chat_messages: Vec<ChatMessage> = messages
        .iter()
        .map(|m| ChatMessage {
            role: m["role"].as_str().unwrap_or("user").to_string(),
            content: m["content"].as_str().unwrap_or("").to_string(),
        })
        .collect();
    let (content, _usage) = ai.complete(&chat_messages, temperature, max_tokens).await?;
    Ok(content)
}

/// Alias for backward compatibility with `call_ai_api` pattern.
async fn call_ai_api(
    _client: &reqwest::Client,
    api_key: &str,
    model: &str,
    messages: &[serde_json::Value],
    temperature: f64,
    max_tokens: usize,
) -> Result<String, ApiError> {
    let base_url = "https://api.deepseek.com/v1";
    let config = OpenAIConfig::new()
        .with_api_key(api_key)
        .with_api_base(base_url);
    let client = OpenAIClient::with_config(config);

    let openai_messages: Vec<ChatCompletionRequestMessage> = messages
        .iter()
        .map(|m| {
            let role = m["role"].as_str().unwrap_or("user");
            let content = m["content"].as_str().unwrap_or("").to_string();
            match role {
                "system" => {
                    ChatCompletionRequestMessage::System(ChatCompletionRequestSystemMessage {
                        content: ChatCompletionRequestSystemMessageContent::Text(content),
                        name: None,
                    })
                }
                "assistant" => {
                    ChatCompletionRequestMessage::Assistant(ChatCompletionRequestAssistantMessage {
                        content: Some(ChatCompletionRequestAssistantMessageContent::Text(content)),
                        ..Default::default()
                    })
                }
                _ => ChatCompletionRequestMessage::User(ChatCompletionRequestUserMessage {
                    content: ChatCompletionRequestUserMessageContent::Text(content),
                    ..Default::default()
                }),
            }
        })
        .collect();

    let request = CreateChatCompletionRequestArgs::default()
        .model(model)
        .messages(openai_messages)
        .temperature(temperature as f32)
        .max_tokens(max_tokens as u32)
        .build()
        .map_err(|e| ApiError::Internal(format!("Failed to build AI request: {}", e)))?;

    // Retry with exponential backoff (max 3 attempts)
    let mut last_err = None;
    for attempt in 0..3u32 {
        if attempt > 0 {
            let delay = std::time::Duration::from_millis(500 * 2u64.pow(attempt - 1));
            tokio::time::sleep(delay).await;
            tracing::info!("AI call retry attempt {}/3", attempt + 1);
        }

        match client.chat().create(request.clone()).await {
            Ok(response) => {
                return Ok(response
                    .choices
                    .first()
                    .and_then(|c| c.message.content.clone())
                    .unwrap_or_default());
            }
            Err(e) => {
                let err_str = e.to_string();
                // Only retry on transient errors (rate limit, timeout, 5xx)
                let is_retryable = err_str.contains("429")
                    || err_str.contains("503")
                    || err_str.contains("502")
                    || err_str.contains("timeout")
                    || err_str.contains("connection");
                if !is_retryable {
                    return Err(ApiError::ServiceUnavailable(format!(
                        "AI service error: {}",
                        e
                    )));
                }
                last_err = Some(e);
            }
        }
    }

    Err(ApiError::ServiceUnavailable(format!(
        "AI service unavailable after 3 retries: {}",
        last_err.map(|e| e.to_string()).unwrap_or_default()
    )))
}

// ─── Sentiment Arc Analysis ─────────────────────────────────────────────

#[derive(Deserialize)]
struct SentimentArcRequest {
    book_id: Uuid,
    /// If provided, only analyze these chapters. Otherwise analyze all.
    #[serde(default)]
    chapter_range: Option<(usize, usize)>,
}

#[derive(Serialize)]
struct SentimentPoint {
    chapter_index: usize,
    chapter_title: String,
    /// Sentiment score: -1.0 (despair) to 1.0 (joy)
    score: f64,
    /// Dominant emotion
    emotion: String,
    /// Key event driving this sentiment
    key_event: String,
}

/// Analyze the emotional arc of a book across chapters.
/// Returns per-chapter sentiment scores for visualization as a curve.
async fn analyze_sentiment_arc(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<SentimentArcRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    ensure_ai_book_read_access(&state, &auth, body.book_id).await?;
    // Load chapter summaries (use existing summarize infrastructure)
    let chapters = state
        .chapters
        .list_by_book(body.book_id)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to load chapters: {e}")))?;

    if chapters.is_empty() {
        return Ok(Json(serde_json::json!({
            "book_id": body.book_id,
            "arc": [],
            "overall_sentiment": 0.0,
        })));
    }

    // Select chapter range
    let (start, end) = body.chapter_range.unwrap_or((0, chapters.len()));
    let selected_chapters = &chapters[start.min(chapters.len())..end.min(chapters.len())];

    // For each chapter, analyze sentiment using LLM
    // Batch up to 10 chapters per request for efficiency
    let mut arc: Vec<SentimentPoint> = Vec::new();

    for batch in selected_chapters.chunks(10) {
        let chapter_summaries: Vec<String> = batch
            .iter()
            .map(|ch| {
                format!(
                    "Chapter {}: {}",
                    ch.index + 1,
                    ch.title.as_deref().unwrap_or("Untitled")
                )
            })
            .collect();

        let prompt = format!(
            "Analyze the emotional sentiment for each chapter. For each chapter, provide:\n\
            1. A sentiment score from -1.0 (very negative/tragic) to 1.0 (very positive/joyful)\n\
            2. The dominant emotion (one word: joy, sadness, anger, fear, surprise, tension, relief, hope)\n\
            3. The key event driving this emotion (brief, max 10 words)\n\n\
            Chapters:\n{}\n\n\
            Respond in JSON array format:\n\
            [{{\"score\": 0.5, \"emotion\": \"joy\", \"key_event\": \"hero wins battle\"}}]",
            chapter_summaries.join("\n")
        );

        let api_key = &state.config.deepseek_api_key;
        let result = call_ai_api(
            &state.http_client,
            api_key,
            "deepseek-chat",
            &[serde_json::json!({"role": "user", "content": prompt})],
            0.3,
            2000,
        )
        .await;

        match result {
            Ok(response_text) => {
                // Try to parse JSON from response
                if let Ok(points) = serde_json::from_str::<Vec<serde_json::Value>>(&response_text) {
                    for (i, point) in points.iter().enumerate() {
                        let ch_idx = batch.get(i).map(|c| c.index as usize).unwrap_or(0);
                        let ch_title = batch
                            .get(i)
                            .and_then(|c| c.title.clone())
                            .unwrap_or_default();
                        arc.push(SentimentPoint {
                            chapter_index: ch_idx,
                            chapter_title: ch_title,
                            score: point["score"].as_f64().unwrap_or(0.0),
                            emotion: point["emotion"].as_str().unwrap_or("neutral").to_string(),
                            key_event: point["key_event"].as_str().unwrap_or("").to_string(),
                        });
                    }
                }
            }
            Err(_) => {
                // If AI call fails, fill with neutral sentiments
                for ch in batch {
                    arc.push(SentimentPoint {
                        chapter_index: ch.index as usize,
                        chapter_title: ch.title.clone().unwrap_or_default(),
                        score: 0.0,
                        emotion: "unknown".to_string(),
                        key_event: "analysis unavailable".to_string(),
                    });
                }
            }
        }
    }

    // Compute overall sentiment
    let overall = if arc.is_empty() {
        0.0
    } else {
        arc.iter().map(|p| p.score).sum::<f64>() / arc.len() as f64
    };

    Ok(Json(serde_json::json!({
        "book_id": body.book_id,
        "arc": arc,
        "overall_sentiment": overall,
        "chapter_count": arc.len(),
    })))
}

// ─── AI Book Review Generation ────────────────────────────────────────────────

#[derive(Deserialize)]
struct GenerateReviewRequest {
    book_id: String,
    /// "concise" (200 words), "standard" (500 words), "detailed" (1000+ words)
    length: Option<String>,
    /// "casual", "academic", "obsidian" (markdown with wikilinks)
    style: Option<String>,
    /// Include spoilers?
    include_spoilers: Option<bool>,
}

/// Generate a personalized AI book review with optional Obsidian export format.
async fn generate_book_review(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<GenerateReviewRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let book_id =
        ensure_ai_book_access_from_str(&state, &auth, &body.book_id, LibraryAccess::Read).await?;
    let book = state.books.get(book_id).await?;

    let length = body.length.as_deref().unwrap_or("standard");
    let style = body.style.as_deref().unwrap_or("casual");
    let include_spoilers = body.include_spoilers.unwrap_or(false);

    let word_target = match length {
        "concise" => 200,
        "detailed" => 1000,
        _ => 500,
    };

    let spoiler_instruction = if include_spoilers {
        "You may discuss plot details freely."
    } else {
        "Do NOT reveal major plot twists or ending. Keep it spoiler-free."
    };

    let format_instruction = match style {
        "obsidian" => "Format as Obsidian markdown: use [[wikilinks]] for character/place names, add YAML frontmatter with rating/tags/date, use callouts for key quotes.",
        "academic" => "Write in formal academic tone with structured analysis: thesis, supporting arguments, conclusion.",
        _ => "Write in a friendly, engaging tone as if recommending to a friend.",
    };

    // Gather context: first few chapters + last chapter for structure sense
    let chapters = state
        .chapters
        .list_by_book(book_id)
        .await
        .unwrap_or_default();
    let sample_text = chapters
        .iter()
        .take(3)
        .filter_map(|c| Some(&c.content))
        .map(|s| &s[..s.len().min(2000)])
        .collect::<Vec<_>>()
        .join("\n---\n");

    let system_prompt = format!(
        "You are a literary critic writing a book review. \
        Book: '{}' by {}. Total chapters: {}. \
        Target length: ~{} words. \
        {}\n{}\n\
        If style is 'obsidian', start with YAML frontmatter block.",
        book.title,
        book.author.as_deref().unwrap_or("Unknown"),
        chapters.len(),
        word_target,
        spoiler_instruction,
        format_instruction
    );

    let messages = vec![
        serde_json::json!({"role": "system", "content": system_prompt}),
        serde_json::json!({"role": "user", "content": format!(
            "Based on this book (sample from opening):\n\n{}\n\nWrite the review.",
            sample_text
        )}),
    ];

    let ai_response = call_ai_json(&state, &messages, 0.3, 4096).await?;

    Ok(Json(serde_json::json!({
        "book_id": body.book_id,
        "book_title": book.title,
        "review": ai_response,
        "style": style,
        "length": length,
        "exportable_markdown": style == "obsidian",
    })))
}

// ─── Text Quality Assessment ──────────────────────────────────────────────────

#[derive(Deserialize)]
struct TextQualityRequest {
    book_id: String,
    /// How many chapters to sample (default: 5)
    sample_size: Option<usize>,
}

/// Assess text quality: garbled text detection, OCR error rate, encoding issues.
async fn assess_text_quality(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<TextQualityRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let book_id =
        ensure_ai_book_access_from_str(&state, &auth, &body.book_id, LibraryAccess::Read).await?;

    let chapters = state
        .chapters
        .list_by_book(book_id)
        .await
        .unwrap_or_default();
    let sample_size = body.sample_size.unwrap_or(5).min(20);

    // Sample chapters evenly distributed across book
    let step = if chapters.len() > sample_size {
        chapters.len() / sample_size
    } else {
        1
    };
    let sampled: Vec<_> = chapters.iter().step_by(step).take(sample_size).collect();

    let mut issues: Vec<serde_json::Value> = Vec::new();
    let mut total_chars: usize = 0;
    let mut garbled_chars: usize = 0;
    let mut empty_chapters: usize = 0;

    for chapter in &sampled {
        let content = &chapter.content;
        if content.is_empty() {
            empty_chapters += 1;
            continue;
        }

        total_chars += content.len();

        // Detect common garbled text patterns
        let replacement_chars = content.chars().filter(|c| *c == '\u{FFFD}').count();
        let control_chars = content
            .chars()
            .filter(|c| c.is_control() && *c != '\n' && *c != '\r' && *c != '\t')
            .count();
        let mojibake_patterns = content.matches("\u{00e9}").count()
            + content.matches("\u{00e3}").count()
            + content.matches("\u{00e2}\u{20ac}").count()
            + content.matches("\u{00c3}").count();

        garbled_chars += replacement_chars + control_chars + mojibake_patterns;

        if replacement_chars > 0 || control_chars > 5 || mojibake_patterns > 3 {
            issues.push(serde_json::json!({
                "chapter_index": chapter.index,
                "chapter_title": chapter.title,
                "replacement_chars": replacement_chars,
                "control_chars": control_chars,
                "mojibake_patterns": mojibake_patterns,
            }));
        }
    }

    let quality_score = if total_chars > 0 {
        1.0 - (garbled_chars as f64 / total_chars as f64).min(1.0)
    } else {
        0.0
    };

    let grade = match quality_score {
        s if s >= 0.99 => "excellent",
        s if s >= 0.95 => "good",
        s if s >= 0.8 => "fair",
        _ => "poor",
    };

    Ok(Json(serde_json::json!({
        "book_id": body.book_id,
        "quality_score": quality_score,
        "grade": grade,
        "total_chars_sampled": total_chars,
        "garbled_chars_detected": garbled_chars,
        "empty_chapters": empty_chapters,
        "chapters_sampled": sampled.len(),
        "issues": issues,
        "recommendations": match grade {
            "poor" => vec!["Re-encode source file with correct charset", "Consider re-downloading from source"],
            "fair" => vec!["Run OCR cleanup pipeline", "Check encoding settings"],
            _ => vec![],
        },
    })))
}

// ─── Smart Bookmarks (AI-detected plot pivots, foreshadowing) ─────────────────

#[derive(Deserialize)]
struct SmartBookmarksRequest {
    book_id: String,
    /// Which chapters to analyze (default: all)
    chapter_range: Option<(usize, usize)>,
}

/// Detect narrative pivots, foreshadowing, and climax points using AI.
async fn detect_smart_bookmarks(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<SmartBookmarksRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let book_id =
        ensure_ai_book_access_from_str(&state, &auth, &body.book_id, LibraryAccess::Read).await?;

    let all_chapters = state
        .chapters
        .list_by_book(book_id)
        .await
        .unwrap_or_default();
    let chapters: Vec<_> = match body.chapter_range {
        Some((start, end)) => all_chapters
            .into_iter()
            .filter(|c| c.index >= start as i32 && c.index <= end as i32)
            .collect(),
        None => all_chapters,
    };

    if chapters.is_empty() {
        return Ok(Json(
            serde_json::json!({ "bookmarks": [], "book_id": body.book_id }),
        ));
    }

    // Batch process: 15 chapters at a time for context window efficiency
    let mut bookmarks: Vec<serde_json::Value> = Vec::new();
    let batch_size = 15;

    for batch in chapters.chunks(batch_size) {
        let batch_summaries: Vec<String> = batch
            .iter()
            .map(|ch| {
                let content = &ch.content;
                let preview = &content[..content.len().min(1500)];
                format!(
                    "Chapter {} ({}): {}",
                    ch.index,
                    ch.title.as_deref().unwrap_or("Untitled"),
                    preview
                )
            })
            .collect();

        let prompt = format!(
            "Analyze these chapters for narrative markers. For EACH significant moment found, output JSON:\n\
            {{\"chapter_index\": N, \"type\": \"pivot|foreshadowing|revelation|climax\", \
            \"description\": \"brief description in Chinese\", \"confidence\": 0.0-1.0, \
            \"related_chapter\": N_or_null}}\n\n\
            Only mark truly significant moments (max 3 per batch). Respond with a JSON array.\n\n{}",
            batch_summaries.join("\n\n---\n\n")
        );

        let messages = vec![
            serde_json::json!({"role": "system", "content": "You are a literary analysis AI. Detect plot pivots, foreshadowing, revelations, and climax moments. Output valid JSON array only."}),
            serde_json::json!({"role": "user", "content": prompt}),
        ];

        if let Ok(response) = call_ai_json(&state, &messages, 0.3, 4096).await {
            // Parse the AI response as JSON array
            if let Ok(parsed) = serde_json::from_str::<Vec<serde_json::Value>>(&response) {
                bookmarks.extend(parsed);
            }
        }
    }

    Ok(Json(serde_json::json!({
        "book_id": body.book_id,
        "bookmarks": bookmarks,
        "total": bookmarks.len(),
        "chapters_analyzed": chapters.len(),
    })))
}

// ─── Character Evolution Analysis ─────────────────────────────────────────────

#[derive(Deserialize)]
struct CharacterEvolutionRequest {
    book_id: String,
    /// Focus on specific characters (empty = auto-detect top characters)
    characters: Option<Vec<String>>,
    /// Granularity: how many chapters per data point
    granularity: Option<usize>,
}

/// Analyze character relationships and how they evolve over chapters.
async fn analyze_character_evolution(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<CharacterEvolutionRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let book_id =
        ensure_ai_book_access_from_str(&state, &auth, &body.book_id, LibraryAccess::Read).await?;

    let chapters = state
        .chapters
        .list_by_book(book_id)
        .await
        .unwrap_or_default();
    if chapters.is_empty() {
        return Ok(Json(
            serde_json::json!({ "characters": [], "relationships": [] }),
        ));
    }

    let granularity = body.granularity.unwrap_or(10).max(3).min(50);

    // Step 1: Identify main characters if not provided
    let character_focus = match &body.characters {
        Some(chars) if !chars.is_empty() => chars.join(", "),
        _ => String::from("auto-detect the top 8 most important characters"),
    };

    // Step 2: Analyze in batches of `granularity` chapters
    let mut evolution_data: Vec<serde_json::Value> = Vec::new();

    for batch in chapters.chunks(granularity) {
        let batch_text: String = batch
            .iter()
            .filter_map(|ch| Some(&ch.content))
            .map(|c| &c[..c.len().min(2000)])
            .collect::<Vec<_>>()
            .join("\n");

        let chapter_range = format!(
            "{}-{}",
            batch[0].index,
            batch.last().map(|c| c.index).unwrap_or(0)
        );

        let prompt = format!(
            "Analyze chapters {} of this novel. Characters to track: {}.\n\n\
            For this section, output JSON:\n\
            {{\"chapter_range\": \"{}\", \"characters\": [\n\
              {{\"name\": \"...\", \"status\": \"active|mentioned|absent\", \"development\": \"brief note\"}}\n\
            ], \"relationships\": [\n\
              {{\"source\": \"A\", \"target\": \"B\", \"type\": \"ally|rival|romantic|family|mentor\", \"strength\": 0.0-1.0, \"event\": \"key event or null\"}}\n\
            ]}}\n\nText:\n{}",
            chapter_range, character_focus, chapter_range,
            &batch_text[..batch_text.len().min(6000)]
        );

        let messages = vec![
            serde_json::json!({"role": "system", "content": "You are a character analysis AI. Track character appearances and relationship changes. Output valid JSON only."}),
            serde_json::json!({"role": "user", "content": prompt}),
        ];

        if let Ok(response) = call_ai_json(&state, &messages, 0.3, 4096).await {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&response) {
                evolution_data.push(parsed);
            }
        }
    }

    Ok(Json(serde_json::json!({
        "book_id": body.book_id,
        "total_chapters": chapters.len(),
        "granularity": granularity,
        "data_points": evolution_data.len(),
        "evolution": evolution_data,
    })))
}

// ─── AI Text Compression ("skip filler, keep plot") ───────────────────────────

#[derive(Deserialize)]
struct CompressTextRequest {
    book_id: String,
    chapter_range: Option<(usize, usize)>,
    /// Compression level: "light" (70% kept), "medium" (40%), "heavy" (20%)
    level: Option<String>,
    /// What to preserve: "plot", "dialogue", "action", "all_key"
    preserve: Option<String>,
}

/// Compress book text by removing filler while preserving plot-critical content.
async fn compress_text(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<CompressTextRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let book_id =
        ensure_ai_book_access_from_str(&state, &auth, &body.book_id, LibraryAccess::Read).await?;

    let chapters = state
        .chapters
        .list_by_book(book_id)
        .await
        .unwrap_or_default();
    let range = body
        .chapter_range
        .unwrap_or((0, chapters.len().saturating_sub(1)));
    let selected: Vec<_> = chapters
        .iter()
        .filter(|c| c.index >= range.0 as i32 && c.index <= range.1 as i32)
        .take(20)
        .collect();

    let level = body.level.as_deref().unwrap_or("medium");
    let preserve = body.preserve.as_deref().unwrap_or("plot");

    let target_ratio = match level {
        "light" => "Keep ~70% of the text",
        "heavy" => "Keep only ~20% — extreme summary",
        _ => "Keep ~40% of the text",
    };

    let preserve_instruction = match preserve {
        "dialogue" => "Prioritize preserving all dialogue and conversations.",
        "action" => "Prioritize action sequences and fight scenes.",
        "all_key" => "Preserve all plot points, character development, and world-building equally.",
        _ => "Focus on main plot advancement. Remove repetitive descriptions, inner monologue that doesn't advance plot, and filler paragraphs.",
    };

    let mut compressed_chapters: Vec<serde_json::Value> = Vec::new();

    for chapter in &selected {
        let content = &chapter.content;
        if content.len() < 500 {
            compressed_chapters.push(serde_json::json!({
                "chapter_index": chapter.index,
                "original_length": content.len(),
                "compressed": content,
                "compression_ratio": 1.0,
            }));
            continue;
        }

        let prompt = format!(
            "Compress this chapter text. {}. {}.\n\
            Rules:\n\
            - Keep the narrative voice and tense\n\
            - Preserve all proper nouns and terminology\n\
            - Mark removed sections with [...] \n\
            - Output the compressed text directly\n\n\
            Text:\n{}",
            target_ratio,
            preserve_instruction,
            &content[..content.len().min(8000)]
        );

        let messages = vec![
            serde_json::json!({"role": "system", "content": "You are a text compression AI for novels. Compress while maintaining readability and plot coherence. Output compressed text only."}),
            serde_json::json!({"role": "user", "content": prompt}),
        ];

        if let Ok(compressed) = call_ai_json(&state, &messages, 0.3, 4096).await {
            let ratio = compressed.len() as f64 / content.len() as f64;
            compressed_chapters.push(serde_json::json!({
                "chapter_index": chapter.index,
                "chapter_title": chapter.title,
                "original_length": content.len(),
                "compressed_length": compressed.len(),
                "compression_ratio": ratio,
                "compressed": compressed,
            }));
        }
    }

    Ok(Json(serde_json::json!({
        "book_id": body.book_id,
        "level": level,
        "preserve": preserve,
        "chapters": compressed_chapters,
        "total_chapters": compressed_chapters.len(),
    })))
}

// ─── Style Transfer ───────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct StyleTransferRequest {
    book_id: String,
    chapter_index: usize,
    /// Target style: "jin_yong", "lu_xun", "mo_yan", "hemingway", "tolkien", "poetic"
    target_style: String,
    /// Optional: paragraph range within the chapter
    paragraph_range: Option<(usize, usize)>,
}

fn select_paragraph_range(
    content: &str,
    paragraph_range: Option<(usize, usize)>,
) -> (String, Option<(usize, usize)>) {
    let Some((start, end)) = paragraph_range else {
        return (content.chars().take(6000).collect(), None);
    };

    if start >= end {
        return (content.chars().take(6000).collect(), None);
    }

    let paragraphs: Vec<&str> = content
        .split("\n\n")
        .map(str::trim)
        .filter(|paragraph| !paragraph.is_empty())
        .collect();

    if paragraphs.is_empty() || start >= paragraphs.len() {
        return (content.chars().take(6000).collect(), None);
    }

    let end = end.min(paragraphs.len());
    let selected = paragraphs[start..end].join("\n\n");
    (selected.chars().take(6000).collect(), Some((start, end)))
}

/// Rewrite text in a different literary style while preserving plot.
async fn style_transfer(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<StyleTransferRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let book_id =
        ensure_ai_book_access_from_str(&state, &auth, &body.book_id, LibraryAccess::Read).await?;

    let chapters = state
        .chapters
        .list_by_book(book_id)
        .await
        .unwrap_or_default();
    let chapter = chapters
        .iter()
        .find(|c| c.index == body.chapter_index as i32)
        .ok_or_else(|| nova_core::Error::Validation("Chapter not found".into()))?;

    let content = &chapter.content;
    let (text, applied_paragraph_range) = select_paragraph_range(content, body.paragraph_range);

    let style_description = match body.target_style.as_str() {
        "jin_yong" => "金庸风格：优雅的武侠文笔，诗意的景物描写，鲜明的人物对白",
        "lu_xun" => "鲁迅风格：犀利简洁，讽刺深刻，白描手法",
        "mo_yan" => "莫言风格：魔幻现实主义，浓烈的感官描写，乡土气息",
        "hemingway" => "海明威风格：极简主义，短句为主，冰山理论，留白感",
        "tolkien" => "托尔金风格：史诗般的世界观描写，古典英语韵味",
        "poetic" => "诗化散文：意境优美，韵律感强，大量比喻和通感",
        _ => "保持原文风格",
    };

    let prompt = format!(
        "将以下文本改写为{}。\n\
        要求：\n\
        - 保留所有情节内容和对话含义\n\
        - 保留人名、地名等专有名词\n\
        - 改变措辞、句式、描写方式以匹配目标风格\n\
        - 输出改写后的文本\n\n\
        原文：\n{}",
        style_description, text
    );

    let messages = vec![
        serde_json::json!({"role": "system", "content": "You are a literary style transfer AI. Rewrite text to match the target literary style while preserving all plot content."}),
        serde_json::json!({"role": "user", "content": prompt}),
    ];

    let rewritten = call_ai_json(&state, &messages, 0.3, 4096).await?;

    Ok(Json(serde_json::json!({
        "book_id": body.book_id,
        "chapter_index": body.chapter_index,
        "paragraph_range": applied_paragraph_range,
        "target_style": body.target_style,
        "style_description": style_description,
        "original_length": text.len(),
        "rewritten_length": rewritten.len(),
        "rewritten": rewritten,
    })))
}

// ─── Cross-book World-Building Graph ──────────────────────────────────────────

#[derive(Deserialize)]
struct WorldBuildingRequest {
    /// Book IDs to analyze for shared world-building elements
    book_ids: Vec<String>,
}

/// Detect shared universe elements across multiple books.
async fn world_building_graph(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<WorldBuildingRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    if body.book_ids.is_empty() || body.book_ids.len() > 10 {
        return Err(nova_core::Error::Validation("Provide 1-10 book_ids to analyze".into()).into());
    }

    let mut book_summaries: Vec<serde_json::Value> = Vec::new();

    for book_id_str in &body.book_ids {
        let book_id: Uuid = book_id_str.parse().map_err(|_| {
            nova_core::Error::Validation(format!("Invalid book_id: {}", book_id_str))
        })?;
        ensure_ai_book_read_access(&state, &auth, book_id).await?;
        let book = state.books.get(book_id).await?;
        let chapters = state
            .chapters
            .list_by_book(book_id)
            .await
            .unwrap_or_default();

        // Sample first 3 chapters for world-building elements
        let sample: String = chapters
            .iter()
            .take(3)
            .filter_map(|c| Some(&c.content))
            .map(|s| &s[..s.len().min(2000)])
            .collect::<Vec<_>>()
            .join("\n");

        book_summaries.push(serde_json::json!({
            "book_id": book_id_str,
            "title": book.title,
            "author": book.author,
            "sample": sample,
        }));
    }

    let prompt = format!(
        "Analyze these {} books for shared world-building elements.\n\
        For each shared element found, output JSON:\n\
        {{\"element\": \"name\", \"type\": \"location|magic_system|faction|race|artifact|concept\", \
        \"books\": [\"book_ids that share it\"], \"description\": \"brief\", \"confidence\": 0.0-1.0}}\n\n\
        Also identify if books share the same universe/series.\n\
        Output a JSON object with: {{\"shared_universe\": bool, \"confidence\": float, \"shared_elements\": [...]}}\n\n\
        Books:\n{}",
        book_summaries.len(),
        serde_json::to_string_pretty(&book_summaries).unwrap_or_default()
    );

    let messages = vec![
        serde_json::json!({"role": "system", "content": "You are a literary analysis AI specializing in world-building and cross-book universe detection. Output valid JSON only."}),
        serde_json::json!({"role": "user", "content": prompt}),
    ];

    let response = call_ai_json(&state, &messages, 0.3, 4096).await?;
    let parsed: serde_json::Value = serde_json::from_str(&response).unwrap_or(serde_json::json!({
        "shared_universe": false,
        "shared_elements": [],
        "raw_response": response,
    }));

    Ok(Json(serde_json::json!({
        "book_ids": body.book_ids,
        "analysis": parsed,
    })))
}

// ─── Plot Consistency Checker v2 ──────────────────────────────────────────────

#[derive(Deserialize)]
struct PlotConsistencyRequest {
    book_id: String,
    /// What to check: "abilities", "timeline", "geography", "character_knowledge", "all"
    check_type: Option<String>,
    chapter_range: Option<(usize, usize)>,
}

/// Advanced plot consistency checking: abilities, timelines, geography, character knowledge.
async fn check_plot_consistency(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<PlotConsistencyRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let book_id =
        ensure_ai_book_access_from_str(&state, &auth, &body.book_id, LibraryAccess::Read).await?;

    let chapters = state
        .chapters
        .list_by_book(book_id)
        .await
        .unwrap_or_default();
    let check_type = body.check_type.as_deref().unwrap_or("all");

    let range = body
        .chapter_range
        .unwrap_or((0, chapters.len().saturating_sub(1)));
    let selected: Vec<_> = chapters
        .iter()
        .filter(|c| c.index >= range.0 as i32 && c.index <= range.1 as i32)
        .collect();

    // Build context from chapter summaries
    let context: String = selected
        .iter()
        .take(30)
        .map(|ch| {
            let content = &ch.content;
            format!("Ch.{}: {}", ch.index, &content[..content.len().min(1000)])
        })
        .collect::<Vec<_>>()
        .join("\n---\n");

    let check_instruction = match check_type {
        "abilities" => "Focus on: character abilities/powers appearing/disappearing without explanation, power levels being inconsistent",
        "timeline" => "Focus on: chronological inconsistencies, characters being in two places at once, time paradoxes",
        "geography" => "Focus on: travel distances being inconsistent, locations described differently, impossible geography",
        "character_knowledge" => "Focus on: characters knowing things they shouldn't, forgetting things they should know",
        _ => "Check all: abilities consistency, timeline logic, geography consistency, and character knowledge bounds",
    };

    let prompt = format!(
        "Analyze this novel for plot consistency issues.\n{}\n\n\
        For each issue found, output JSON in an array:\n\
        {{\"type\": \"ability|timeline|geography|knowledge\", \
        \"severity\": \"minor|moderate|major\", \
        \"chapter\": N, \"description\": \"...\", \
        \"evidence\": \"quote or reference\", \
        \"suggestion\": \"how to fix\"}}\n\n\
        Text:\n{}",
        check_instruction,
        &context[..context.len().min(12000)]
    );

    let messages = vec![
        serde_json::json!({"role": "system", "content": "You are a continuity editor. Find plot holes, inconsistencies, and logical errors. Be thorough but avoid false positives. Output valid JSON array."}),
        serde_json::json!({"role": "user", "content": prompt}),
    ];

    let response = call_ai_json(&state, &messages, 0.3, 4096).await?;
    let issues: Vec<serde_json::Value> = serde_json::from_str(&response).unwrap_or_default();

    Ok(Json(serde_json::json!({
        "book_id": body.book_id,
        "check_type": check_type,
        "chapters_analyzed": selected.len(),
        "issues_found": issues.len(),
        "issues": issues,
    })))
}

// ─── AI Quiz Generation ───────────────────────────────────────────────────────

#[derive(Deserialize)]
struct GenerateQuizRequest {
    book_id: String,
    /// Chapters to generate questions from
    chapter_range: Option<(usize, usize)>,
    /// Number of questions (default: 5, max: 15)
    count: Option<usize>,
    /// Difficulty: "easy", "medium", "hard"
    difficulty: Option<String>,
}

/// Generate reading comprehension quiz questions for a book.
async fn generate_quiz(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<GenerateQuizRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let book_id =
        ensure_ai_book_access_from_str(&state, &auth, &body.book_id, LibraryAccess::Read).await?;

    let chapters = state
        .chapters
        .list_by_book(book_id)
        .await
        .unwrap_or_default();
    let range = body
        .chapter_range
        .unwrap_or((0, chapters.len().saturating_sub(1)));
    let selected: Vec<_> = chapters
        .iter()
        .filter(|c| c.index >= range.0 as i32 && c.index <= range.1 as i32)
        .collect();

    let count = body.count.unwrap_or(5).min(15);
    let difficulty = body.difficulty.as_deref().unwrap_or("medium");

    let difficulty_instruction = match difficulty {
        "easy" => "Ask straightforward factual questions about what happened.",
        "hard" => "Ask inference-based questions requiring deep understanding: themes, symbolism, character motivation, foreshadowing.",
        _ => "Mix factual recall with some inference questions.",
    };

    let context: String = selected
        .iter()
        .take(10)
        .filter_map(|ch| Some(&ch.content))
        .map(|c| &c[..c.len().min(2000)])
        .collect::<Vec<_>>()
        .join("\n---\n");

    let prompt = format!(
        "Generate {} multiple-choice reading comprehension questions.\n\
        {}\n\
        Each question must have exactly 4 options (A-D) with one correct answer.\n\n\
        Output as JSON array:\n\
        [{{\"question\": \"...\", \"options\": [\"A\", \"B\", \"C\", \"D\"], \
        \"correct_index\": 0-3, \"explanation\": \"why this is correct\", \
        \"chapter_ref\": chapter_number_or_null}}]\n\n\
        Questions should be in Chinese. Text:\n{}",
        count,
        difficulty_instruction,
        &context[..context.len().min(10000)]
    );

    let messages = vec![
        serde_json::json!({"role": "system", "content": "You are an educational AI generating reading comprehension questions. Output valid JSON array only."}),
        serde_json::json!({"role": "user", "content": prompt}),
    ];

    let response = call_ai_json(&state, &messages, 0.3, 4096).await?;
    let questions: Vec<serde_json::Value> = serde_json::from_str(&response).unwrap_or_default();

    Ok(Json(serde_json::json!({
        "book_id": body.book_id,
        "chapter_range": [range.0, range.1],
        "difficulty": difficulty,
        "questions": questions,
        "total": questions.len(),
    })))
}

// ─── Conversation Memory ────────────────────────────────────────────────────

fn conversation_history_key(user_id: Uuid, conversation_id: &str) -> String {
    format!("conv:user:{}:{}", user_id, conversation_id)
}

/// Load conversation history from Redis (fast ephemeral storage).
/// Falls back to empty if Redis is unavailable.
async fn load_conversation_history(
    state: &AppState,
    user_id: Uuid,
    conversation_id: &str,
) -> Vec<ChatMessage> {
    let key = conversation_history_key(user_id, conversation_id);
    let mut conn = match state.redis.get_multiplexed_async_connection().await {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let data: Option<String> = redis::AsyncCommands::get(&mut conn, &key)
        .await
        .unwrap_or(None);
    match data {
        Some(json_str) => serde_json::from_str(&json_str).unwrap_or_default(),
        None => Vec::new(),
    }
}

/// Save new messages to conversation history in Redis.
/// Keeps only the last 50 messages to prevent unbounded growth.
/// TTL: 24 hours (conversations expire after a day of inactivity).
async fn save_conversation_messages(
    state: &AppState,
    user_id: Uuid,
    conversation_id: &str,
    new_messages: &[ChatMessage],
) {
    let key = conversation_history_key(user_id, conversation_id);
    let mut conn = match state.redis.get_multiplexed_async_connection().await {
        Ok(c) => c,
        Err(_) => return,
    };

    // Load existing history
    let existing: Vec<ChatMessage> = {
        let data: Option<String> = redis::AsyncCommands::get(&mut conn, &key)
            .await
            .unwrap_or(None);
        data.and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    };

    // Append new messages, keep last 50
    let mut all = existing;
    all.extend(new_messages.iter().cloned());
    if all.len() > 50 {
        all = all.split_off(all.len() - 50);
    }

    // Save with 24h TTL
    let json = serde_json::to_string(&all).unwrap_or_default();
    let _: Result<(), _> = redis::AsyncCommands::set_ex(&mut conn, &key, &json, 86400).await;
}

#[cfg(test)]
mod tests {
    fn production_source() -> &'static str {
        include_str!("ai.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("production source should exist")
    }

    fn handler_source<'a>(source: &'a str, name: &str) -> &'a str {
        let marker = format!("async fn {name}(");
        let start = source
            .find(&marker)
            .unwrap_or_else(|| panic!("{name} handler should exist"));
        let rest = &source[start..];
        let end = rest.find("\nasync fn ").unwrap_or(rest.len());
        &rest[..end]
    }

    fn assert_handler_contains(source: &str, handler: &str, expected: &str) {
        let body = handler_source(source, handler);
        assert!(
            body.contains(expected),
            "{handler} should contain `{expected}`"
        );
    }

    #[test]
    fn ai_book_scoped_read_routes_require_object_acl() {
        let source = production_source();

        assert!(source.contains("ensure_book_access"));
        assert!(source.contains("is_admin"));
        assert!(source.contains("LibraryAccess"));
        assert!(source.contains("use crate::extractors::AuthUser;"));
        assert!(source.contains("async fn ensure_optional_ai_book_read_access("));
        assert!(source.contains("async fn ensure_ai_book_access_from_str("));
        assert!(source.contains("ensure_book_access(state, auth, book_id, access).await"));

        for handler in ["chat", "chat_stream", "extract_entities"] {
            assert_handler_contains(source, handler, "auth: AuthUser");
            assert_handler_contains(
                source,
                handler,
                "ensure_optional_ai_book_read_access(&state, &auth, body.book_id.as_deref()).await?;",
            );
        }

        assert_handler_contains(source, "rag_context", "auth: AuthUser");
        assert_handler_contains(
            source,
            "rag_context",
            "ensure_ai_book_access_from_str(&state, &auth, book_id, LibraryAccess::Read).await?;",
        );

        for handler in [
            "detect_plot_holes",
            "generate_chapter_titles",
            "generate_book_review",
            "assess_text_quality",
            "detect_smart_bookmarks",
            "analyze_character_evolution",
            "compress_text",
            "style_transfer",
            "check_plot_consistency",
            "generate_quiz",
        ] {
            assert_handler_contains(source, handler, "auth: AuthUser");
            assert_handler_contains(
                source,
                handler,
                "ensure_ai_book_access_from_str(&state, &auth, &body.book_id, LibraryAccess::Read).await?;",
            );
        }

        assert_handler_contains(source, "analyze_sentiment_arc", "auth: AuthUser");
        assert_handler_contains(
            source,
            "analyze_sentiment_arc",
            "ensure_ai_book_read_access(&state, &auth, body.book_id).await?;",
        );

        assert_handler_contains(source, "world_building_graph", "auth: AuthUser");
        assert_handler_contains(
            source,
            "world_building_graph",
            "ensure_ai_book_read_access(&state, &auth, book_id).await?;",
        );
    }

    #[test]
    fn conversation_memory_keys_are_user_scoped() {
        let source = production_source();

        assert!(source.contains("auth_user_id(&auth)?"));
        assert!(source.contains("load_conversation_history(&state, user_id, conv_id).await"));
        assert!(
            source.contains("save_conversation_messages(&state, user_id, conv_id, &to_save).await")
        );
        assert!(source.contains("format!(\"conv:user:{}:{}\", user_id, conversation_id)"));
        assert!(!source.contains("format!(\"conv:{}:{}\", user_id, conversation_id)"));
        assert!(!source.contains("format!(\"conv:{}\", conversation_id)"));
    }

    #[test]
    fn conversation_history_key_namespaces_by_user() {
        let user_id =
            uuid::Uuid::parse_str("11111111-1111-1111-1111-111111111111").expect("valid uuid");

        assert_eq!(
            super::conversation_history_key(user_id, "shared-conversation"),
            "conv:user:11111111-1111-1111-1111-111111111111:shared-conversation"
        );
    }

    #[test]
    fn ai_book_mutation_routes_require_write_acl() {
        let source = production_source();

        for handler in [
            "batch_process_book",
            "batch_process_stream",
            "ingest_embeddings",
            "embedding_delete_book",
        ] {
            assert_handler_contains(source, handler, "auth: AuthUser");
            assert_handler_contains(
                source,
                handler,
                "ensure_ai_book_access_from_str(&state, &auth, &body.book_id, LibraryAccess::Write).await?;",
            );
        }
    }

    #[test]
    fn global_embedding_admin_routes_are_admin_only() {
        let source = production_source();

        assert!(source.contains("async fn ensure_ai_admin_access("));
        for handler in ["embedding_index_status", "embedding_index_rebuild"] {
            assert_handler_contains(source, handler, "auth: AuthUser");
            assert_handler_contains(
                source,
                handler,
                "ensure_ai_admin_access(&state, &auth).await?;",
            );
        }
    }

    #[test]
    fn embedding_writers_share_freshness_payload_and_delete_before_empty_return() {
        let source = production_source();

        for handler in [
            "batch_process_book",
            "batch_process_stream",
            "ingest_embeddings",
        ] {
            let body = handler_source(source, handler);
            assert!(
                body.contains("load_embedding_freshness_contract("),
                "{handler} must hash the current full book snapshot"
            );
            assert!(
                body.contains("delete_book_embedding_points("),
                "{handler} must remove the previous book snapshot"
            );
            assert!(
                body.contains("embedding_contract.chunk_payload("),
                "{handler} must use the shared Qdrant payload contract"
            );
            assert!(
                body.contains("embedding_point_id("),
                "{handler} must use a valid shared Qdrant point ID"
            );
        }

        for (handler, empty_check) in [
            ("batch_process_book", "if chapters_rows.is_empty()"),
            ("batch_process_stream", "if chapters.is_empty()"),
        ] {
            let body = handler_source(source, handler);
            let delete = body
                .find("delete_book_embedding_points(")
                .expect("embedding delete should exist");
            let empty = body.find(empty_check).expect("empty check should exist");
            assert!(
                delete < empty,
                "{handler} must delete old points before returning for an empty snapshot"
            );
        }

        let ingest = handler_source(source, "ingest_embeddings");
        let validation = ingest
            .find("validate_full_embedding_chapter_selection(")
            .expect("full-snapshot validation should exist");
        let delete = ingest
            .find("delete_book_embedding_points(")
            .expect("embedding delete should exist");
        assert!(
            validation < delete,
            "partial requests must fail before deleting the current full index"
        );
    }

    #[test]
    fn embedding_ingest_rejects_partial_snapshots_but_accepts_full_selection() {
        let chapters = vec![
            super::SearchableChapterRecord {
                chapter_index: 2,
                title: "Two".to_string(),
                content: "Text".to_string(),
            },
            super::SearchableChapterRecord {
                chapter_index: 5,
                title: "Five".to_string(),
                content: "Text".to_string(),
            },
        ];

        assert!(super::validate_full_embedding_chapter_selection(None, &chapters).is_ok());
        assert!(super::validate_full_embedding_chapter_selection(Some(&[2, 5]), &chapters).is_ok());
        assert!(super::validate_full_embedding_chapter_selection(Some(&[2]), &chapters).is_err());
        assert!(super::validate_full_embedding_chapter_selection(Some(&[]), &[]).is_ok());
    }

    #[test]
    fn rag_graph_context_is_scoped_to_requested_book() {
        let source = production_source();

        assert!(source.contains("retrieve_graph_context(state, query, book_id).await"));
        assert!(source.contains("async fn retrieve_graph_context(state: &AppState, query: &str, book_id: &str) -> String"));
        assert!(source.contains("MATCH (n {book_id: $book_id})"));
        assert!(source.contains("serde_json::json!({ \"query\": query, \"book_id\": book_id })"));
    }
}
