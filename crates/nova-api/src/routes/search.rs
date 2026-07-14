use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;

use crate::access::{ensure_book_access, visible_library_ids, LibraryAccess};
use crate::error::{ApiError, ApiResult};
use crate::extractors::AuthUser;
use crate::state::AppState;
use nova_core::domain::search::{SearchMode, SearchQuery};

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/search", post(search))
        .route("/search/similar", post(find_similar))
        .route("/search/similar/{chunk_id}", get(find_similar_by_id))
        .route("/search/suggest", get(suggest))
        .route("/search/facets", get(search_facets))
        .route("/search/context/{chunk_id}", get(get_chunk_context))
        .route("/search/graph", post(search_graph))
        .route("/search/cross-book", post(search_cross_book))
        .route("/search/global", post(search_global))
}

/// Perform a hybrid search across the library.
async fn search(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(query): Json<SearchQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let total_start = std::time::Instant::now();
    let scope = effective_book_scope(
        &state,
        &auth,
        query.book_ids.iter().map(|id| id.into_uuid()),
    )
    .await?;

    // Input validation
    if query.query.trim().is_empty() {
        return Ok(Json(serde_json::json!({
            "results": [],
            "total": 0,
            "query": query.query,
            "mode": query.mode,
        })));
    }
    if query.query.len() > 1000 {
        return Err(crate::error::ApiError::bad_request(
            "Query exceeds 1000 character limit",
        ));
    }

    let mode = query.mode;
    let limit = query.limit.min(50);
    if scope.is_empty_restriction() {
        return Ok(Json(serde_json::json!({
            "results": [],
            "total": 0,
            "query": query.query,
            "mode": mode,
            "intent": detect_query_intent(&query.query),
            "rerank_applied": false,
            "timing": {
                "total_ms": total_start.elapsed().as_millis() as u64,
                "bm25_ms": 0,
                "vector_ms": 0,
                "rerank_ms": 0,
            },
        })));
    }

    // Dynamic top_k based on query complexity
    let dynamic_k = compute_dynamic_top_k(&query.query, limit);

    // Load feature flags to check if semantic search / reranker are enabled
    let flags = crate::feature_flags::load_feature_flags(&state.db).await;

    // Meilisearch keyword search
    let bm25_start = std::time::Instant::now();
    let book_id_strings = scope.id_strings();
    let meili_results = if matches!(mode, SearchMode::Keyword | SearchMode::Hybrid) {
        search_meilisearch(
            &state,
            &query.query,
            limit,
            query.offset,
            &book_id_strings,
            matches!(mode, SearchMode::Hybrid),
        )
        .await
    } else {
        Vec::new()
    };
    let bm25_ms = bm25_start.elapsed().as_millis() as u64;

    // Semantic vector search via Qdrant (if enabled and mode allows)
    let vector_start = std::time::Instant::now();
    let semantic_results =
        if flags.semantic_search && matches!(mode, SearchMode::Semantic | SearchMode::Hybrid) {
            search_qdrant_semantic(&state, &query.query, dynamic_k, &book_id_strings).await
        } else if matches!(mode, SearchMode::Semantic | SearchMode::Hybrid) {
            // Fallback to DB ILIKE when semantic search is disabled
            search_database(&state, &query.query, limit, &book_id_strings).await
        } else {
            Vec::new()
        };
    let vector_ms = vector_start.elapsed().as_millis() as u64;

    // Merge with RRF (Reciprocal Rank Fusion) for hybrid mode
    let mut results = if matches!(mode, SearchMode::Hybrid)
        && !meili_results.is_empty()
        && !semantic_results.is_empty()
    {
        rrf_merge(meili_results, semantic_results, limit)
    } else if !meili_results.is_empty() {
        meili_results
    } else if !semantic_results.is_empty() {
        semantic_results
    } else {
        // Fallback to DB ILIKE search when no results from Meilisearch/Qdrant
        search_database(&state, &query.query, limit, &book_id_strings).await
    };

    // Apply reranker for precision (if enabled and we have enough results)
    let rerank_start = std::time::Instant::now();
    let rerank_applied = flags.reranker && results.len() > 1;
    if rerank_applied {
        results = rerank_results(&state, &query.query, results).await;
    }
    let rerank_ms = rerank_start.elapsed().as_millis() as u64;

    // Add breadcrumb to each result
    for result in results.iter_mut() {
        if let Some(obj) = result.as_object_mut() {
            let book_title = obj.get("book_title").and_then(|v| v.as_str()).unwrap_or("");
            let chapter_title = obj.get("chapter_title").and_then(|v| v.as_str());
            let chapter_index = obj
                .get("chapter_index")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let chunk_index = obj.get("chunk_index").and_then(|v| v.as_i64());

            let breadcrumb = if let Some(ch_title) = chapter_title {
                if ch_title.is_empty() {
                    format!("《{}》> 第{}章", book_title, chapter_index + 1)
                } else {
                    format!("《{}》> {}", book_title, ch_title)
                }
            } else {
                format!("《{}》> 第{}章", book_title, chapter_index + 1)
            };

            let breadcrumb = if let Some(ci) = chunk_index {
                format!("{} > #{}", breadcrumb, ci + 1)
            } else {
                breadcrumb
            };

            obj.insert("breadcrumb".to_string(), serde_json::json!(breadcrumb));
        }
    }

    let total_ms = total_start.elapsed().as_millis() as u64;

    Ok(Json(serde_json::json!({
        "results": results,
        "total": results.len(),
        "query": query.query,
        "mode": mode,
        "intent": detect_query_intent(&query.query),
        "rerank_applied": rerank_applied,
        "timing": {
            "total_ms": total_ms,
            "bm25_ms": bm25_ms,
            "vector_ms": vector_ms,
            "rerank_ms": rerank_ms,
        },
    })))
}

/// Search via Meilisearch full-text index
async fn search_meilisearch(
    state: &AppState,
    query: &str,
    limit: usize,
    offset: usize,
    book_ids: &[String],
    use_hybrid_embedder: bool,
) -> Vec<serde_json::Value> {
    let mut search_body = serde_json::json!({
        "q": query,
        "limit": limit,
        "offset": offset,
        "showRankingScore": true,
        "attributesToHighlight": ["content"],
        "highlightPreTag": "<mark>",
        "highlightPostTag": "</mark>",
        "attributesToCrop": ["content"],
        "cropLength": 200,
    });

    if use_hybrid_embedder {
        search_body["hybrid"] = serde_json::json!({
            "embedder": "qwen3",
            "semanticRatio": 0.5
        });
    }

    // Add book_id filter if specified
    if !book_ids.is_empty() {
        let filter_parts: Vec<String> = book_ids
            .iter()
            .map(|id| format!("book_id = \"{}\"", id))
            .collect();
        search_body["filter"] = serde_json::Value::String(filter_parts.join(" OR "));
    }

    let resp = state
        .http_client
        .post(format!("{}/indexes/chunks/search", state.config.meili_url))
        .header(
            "Authorization",
            format!("Bearer {}", state.config.meili_master_key),
        )
        .json(&search_body)
        .send()
        .await;

    match resp {
        Ok(r) if r.status().is_success() => {
            let data: serde_json::Value = r.json().await.unwrap_or_default();
            let hits = data
                .get("hits")
                .and_then(|h| h.as_array())
                .cloned()
                .unwrap_or_default();
            hits.into_iter().map(|hit| {
                serde_json::json!({
                    "book_id": hit.get("book_id").and_then(|v| v.as_str()).unwrap_or(""),
                    "book_title": hit.get("book_title").and_then(|v| v.as_str()).unwrap_or(""),
                    "chapter_title": hit.get("chapter_title").and_then(|v| v.as_str()),
                    "chapter_index": hit.get("chapter_index").and_then(|v| v.as_i64()).unwrap_or(0),
                    "chunk_index": hit.get("chunk_index").and_then(|v| v.as_i64()),
                    "content_snippet": hit.get("content").and_then(|v| v.as_str()).unwrap_or(""),
                    "score": hit.get("_rankingScore").and_then(|v| v.as_f64()).unwrap_or(0.0),
                    "source": "keyword",
                    "highlighted": hit.get("_formatted").and_then(|f| f.get("content")).and_then(|v| v.as_str()),
                })
            }).collect()
        }
        Ok(r) => {
            let status = r.status();
            tracing::warn!("Meilisearch returned error: {}", status);
            Vec::new()
        }
        Err(e) => {
            tracing::error!("Meilisearch unreachable: {}", e);
            Vec::new()
        }
    }
}

/// Fallback DB search — searches books by title/author and chapters by title
async fn search_database(
    state: &AppState,
    query: &str,
    limit: usize,
    book_ids: &[String],
) -> Vec<serde_json::Value> {
    let pattern = format!("%{}%", query);
    let book_uuid_filters: Vec<Uuid> = book_ids
        .iter()
        .filter_map(|id| Uuid::parse_str(id).ok())
        .collect();

    // Search books
    let rows = if book_uuid_filters.is_empty() {
        sqlx::query_as::<_, (uuid::Uuid, String, Option<String>)>(
            "SELECT id, title, author FROM books WHERE title ILIKE $1 OR author ILIKE $1 LIMIT $2",
        )
        .bind(&pattern)
        .bind(limit as i64)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default()
    } else {
        sqlx::query_as::<_, (uuid::Uuid, String, Option<String>)>(
            "SELECT id, title, author FROM books WHERE (title ILIKE $1 OR author ILIKE $1) AND id = ANY($2) LIMIT $3",
        )
        .bind(&pattern)
        .bind(&book_uuid_filters)
        .bind(limit as i64)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default()
    };

    let book_results: Vec<serde_json::Value> = rows
        .into_iter()
        .enumerate()
        .map(|(i, (id, title, author))| {
            serde_json::json!({
                "book_id": id,
                "book_title": title,
                "chapter_title": serde_json::Value::Null,
                "chapter_index": 0,
                "content_snippet": author.unwrap_or_default(),
                "score": 1.0 - (i as f64 * 0.05),
                "source": "database",
                "highlighted": serde_json::Value::Null,
            })
        })
        .collect();

    book_results
}

/// Reciprocal Rank Fusion: merge two ranked lists
fn rrf_merge(
    list_a: Vec<serde_json::Value>,
    list_b: Vec<serde_json::Value>,
    limit: usize,
) -> Vec<serde_json::Value> {
    use std::collections::{BTreeSet, HashMap};
    let k = 60.0_f64; // RRF constant

    // (rrf_score, item, sources, per-source raw scores)
    type Entry = (
        f64,
        serde_json::Value,
        BTreeSet<String>,
        HashMap<String, f64>,
    );
    let mut scores: HashMap<String, Entry> = HashMap::new();

    fn result_key(item: &serde_json::Value) -> String {
        let book_id = item.get("book_id").and_then(|v| v.as_str()).unwrap_or("");
        let chapter_index = item
            .get("chapter_index")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let chunk_key = item
            .get("chunk_id")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .or_else(|| {
                item.get("chunk_index")
                    .and_then(|v| v.as_i64())
                    .map(|i| i.to_string())
            })
            .unwrap_or_else(|| "chapter".to_string());

        format!("{}:{}:{}", book_id, chapter_index, chunk_key)
    }

    fn push_rrf_item(
        scores: &mut HashMap<String, Entry>,
        rank: usize,
        item: serde_json::Value,
        k: f64,
    ) {
        let key = result_key(&item);
        let source = item
            .get("source")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let item_score = item.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let rrf_score = 1.0 / (k + rank as f64 + 1.0);

        scores
            .entry(key)
            .and_modify(|(score, existing, sources, per_source)| {
                *score += rrf_score;
                sources.insert(source.clone());
                per_source
                    .entry(source.clone())
                    .and_modify(|s| {
                        if item_score > *s {
                            *s = item_score;
                        }
                    })
                    .or_insert(item_score);
                let existing_score = existing
                    .get("score")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                if item_score > existing_score {
                    *existing = item.clone();
                }
            })
            .or_insert_with(|| {
                let mut sources = BTreeSet::new();
                sources.insert(source.clone());
                let mut per_source = HashMap::new();
                per_source.insert(source, item_score);
                (rrf_score, item, sources, per_source)
            });
    }

    for (rank, item) in list_a.into_iter().enumerate() {
        push_rrf_item(&mut scores, rank, item, k);
    }

    for (rank, item) in list_b.into_iter().enumerate() {
        push_rrf_item(&mut scores, rank, item, k);
    }

    let mut merged: Vec<_> = scores.into_values().collect();
    merged.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    merged
        .into_iter()
        .take(limit)
        .map(|(score, mut item, sources, per_source)| {
            if let Some(obj) = item.as_object_mut() {
                let source_list: Vec<String> = sources.into_iter().collect();
                obj.insert("score".to_string(), serde_json::json!(score));
                obj.insert("fusion_score".to_string(), serde_json::json!(score));
                obj.insert("match_sources".to_string(), serde_json::json!(source_list));
                // Preserve each retriever's own raw score so the UI can show a breakdown.
                if let Some(s) = per_source.get("keyword") {
                    obj.insert("keyword_score".to_string(), serde_json::json!(s));
                }
                if let Some(s) = per_source.get("semantic") {
                    obj.insert("semantic_score".to_string(), serde_json::json!(s));
                }
                if obj
                    .get("match_sources")
                    .and_then(|v| v.as_array())
                    .map(|s| s.len())
                    .unwrap_or(0)
                    > 1
                {
                    obj.insert("source".to_string(), serde_json::json!("hybrid"));
                }
            }
            item
        })
        .collect()
}

/// Find content similar to a given passage (POST endpoint).
async fn find_similar(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<FindSimilarRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let scope = effective_book_scope(&state, &auth, body.book_ids.iter().copied()).await?;
    if scope.is_empty_restriction() {
        return Ok(Json(serde_json::json!({
            "similar": [],
            "source_text": body.text,
        })));
    }
    let book_id_strings = scope.id_strings();

    if !body.text.is_empty() {
        let results = search_qdrant_semantic(&state, &body.text, body.k, &book_id_strings).await;
        if !results.is_empty() {
            return Ok(Json(serde_json::json!({
                "similar": results,
                "source_text": body.text,
            })));
        }
    }
    Ok(Json(serde_json::json!({
        "similar": [],
        "source_text": body.text,
    })))
}

/// Find similar content by chunk ID (GET endpoint for frontend).
async fn find_similar_by_id(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(chunk_id): Path<String>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> ApiResult<Json<serde_json::Value>> {
    let limit = params
        .get("limit")
        .and_then(|l| l.parse::<usize>().ok())
        .unwrap_or(10);

    // Try to find the chunk in Qdrant and get similar vectors
    let resp = state
        .http_client
        .post(format!(
            "{}/collections/nova_chunks/points/scroll",
            state.config.qdrant_url
        ))
        .json(&serde_json::json!({
            "filter": {
                "must": [{ "key": "chunk_id", "match": { "value": chunk_id } }]
            },
            "limit": 1,
            "with_vectors": true,
        }))
        .send()
        .await;

    match resp {
        Ok(r) if r.status().is_success() => {
            let data: serde_json::Value = r.json().await.unwrap_or_default();
            let points = data["result"]["points"].as_array();
            if let Some(points) = points {
                if let Some(point) = points.first() {
                    let source_book_id = point["payload"]["book_id"]
                        .as_str()
                        .and_then(|id| Uuid::parse_str(id).ok())
                        .ok_or_else(|| ApiError::bad_request("Invalid chunk book_id"))?;
                    ensure_book_access(&state, &auth, source_book_id, LibraryAccess::Read).await?;
                    let scope =
                        effective_book_scope(&state, &auth, std::iter::empty::<Uuid>()).await?;
                    if scope.is_empty_restriction() {
                        return Ok(Json(serde_json::json!({
                            "results": [],
                            "chunk_id": chunk_id,
                        })));
                    }
                    let book_id_strings = scope.id_strings();

                    if let Some(vector) = point.get("vector") {
                        // Search for similar vectors
                        let mut search_body = serde_json::json!({
                            "vector": vector,
                            "limit": limit + 1, // +1 to exclude self
                            "with_payload": true,
                        });
                        add_qdrant_book_filter(&mut search_body, &book_id_strings);
                        let search_resp = state
                            .http_client
                            .post(format!(
                                "{}/collections/nova_chunks/points/search",
                                state.config.qdrant_url
                            ))
                            .json(&search_body)
                            .send()
                            .await;

                        if let Ok(sr) = search_resp {
                            if sr.status().is_success() {
                                let search_data: serde_json::Value =
                                    sr.json().await.unwrap_or_default();
                                let results: Vec<serde_json::Value> = search_data["result"]
                                    .as_array()
                                    .unwrap_or(&Vec::new())
                                    .iter()
                                    .filter(|r| {
                                        r["payload"]["chunk_id"].as_str() != Some(&chunk_id)
                                    })
                                    .take(limit)
                                    .map(|r| {
                                        serde_json::json!({
                                            "book_id": r["payload"]["book_id"],
                                            "book_title": r["payload"]["book_title"],
                                            "chapter_title": r["payload"]["chapter_title"],
                                            "content_snippet": r["payload"]["text"].as_str().or_else(|| r["payload"]["content"].as_str()).unwrap_or(""),
                                            "score": r["score"],
                                            "source": "semantic",
                                        })
                                    })
                                    .collect();

                                return Ok(Json(serde_json::json!({
                                    "results": results,
                                    "chunk_id": chunk_id,
                                })));
                            }
                        }
                    }
                }
            }
            Ok(Json(serde_json::json!({
                "results": [],
                "chunk_id": chunk_id,
                "status": "not_found",
            })))
        }
        _ => Ok(Json(serde_json::json!({
            "results": [],
            "chunk_id": chunk_id,
            "status": "unavailable",
            "message": "向量数据库不可用",
        }))),
    }
}

/// Search facets: aggregate counts by book, author, format, etc.
async fn search_facets(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> ApiResult<Json<serde_json::Value>> {
    let q = params.get("q").cloned().unwrap_or_default();
    if q.is_empty() {
        return Ok(Json(serde_json::json!({ "facets": {} })));
    }
    let scope = effective_book_scope(&state, &auth, std::iter::empty::<Uuid>()).await?;
    if scope.is_empty_restriction() {
        return Ok(Json(serde_json::json!({
            "facets": {
                "format": [],
                "author": [],
            }
        })));
    }

    let pattern = format!("%{}%", q);
    let book_uuid_filters = scope.ids;

    // Aggregate by format
    let format_counts: Vec<(String, i64)> = if book_uuid_filters.is_empty() {
        sqlx::query_as(
            "SELECT format::text, COUNT(*) FROM books WHERE (title ILIKE $1 OR author ILIKE $1) AND status != 'archived' GROUP BY format",
        )
        .bind(&pattern)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default()
    } else {
        sqlx::query_as(
            "SELECT format::text, COUNT(*) FROM books WHERE (title ILIKE $1 OR author ILIKE $1) AND status != 'archived' AND id = ANY($2) GROUP BY format",
        )
        .bind(&pattern)
        .bind(&book_uuid_filters)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default()
    };

    // Aggregate by author (top 10)
    let author_counts: Vec<(Option<String>, i64)> = if book_uuid_filters.is_empty() {
        sqlx::query_as(
            "SELECT author, COUNT(*) FROM books WHERE (title ILIKE $1 OR author ILIKE $1) AND status != 'archived' AND author IS NOT NULL GROUP BY author ORDER BY COUNT(*) DESC LIMIT 10",
        )
        .bind(&pattern)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default()
    } else {
        sqlx::query_as(
            "SELECT author, COUNT(*) FROM books WHERE (title ILIKE $1 OR author ILIKE $1) AND status != 'archived' AND author IS NOT NULL AND id = ANY($2) GROUP BY author ORDER BY COUNT(*) DESC LIMIT 10",
        )
        .bind(&pattern)
        .bind(&book_uuid_filters)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default()
    };

    Ok(Json(serde_json::json!({
        "facets": {
            "format": format_counts.into_iter().map(|(f, c)| serde_json::json!({"value": f, "count": c})).collect::<Vec<_>>(),
            "author": author_counts.into_iter().filter_map(|(a, c)| a.map(|a| serde_json::json!({"value": a, "count": c}))).collect::<Vec<_>>(),
        }
    })))
}

// ─── Semantic Search via Qdrant ──────────────────────────────────────────────

/// Embed query text and search Qdrant for similar chunks.
async fn search_qdrant_semantic(
    state: &AppState,
    query: &str,
    limit: usize,
    book_ids: &[String],
) -> Vec<serde_json::Value> {
    // Step 1: Embed the query
    let embedding = match embed_text(state, query).await {
        Some(v) => v,
        None => return search_database(state, query, limit, book_ids).await, // fallback to DB search
    };

    // Step 2: Search Qdrant with optional book_id filter
    let mut search_body = serde_json::json!({
        "vector": embedding,
        "limit": limit,
        "with_payload": true,
        "score_threshold": 0.3,
    });

    // Pre-filter by book_ids if specified (Qdrant indexed filter for efficiency)
    if !book_ids.is_empty() {
        search_body["filter"] = serde_json::json!({
            "should": book_ids.iter().map(|id| {
                serde_json::json!({ "key": "book_id", "match": { "value": id } })
            }).collect::<Vec<_>>()
        });
    }

    let resp = state
        .http_client
        .post(format!(
            "{}/collections/nova_chunks/points/search",
            state.config.qdrant_url
        ))
        .json(&search_body)
        .send()
        .await;

    match resp {
        Ok(r) if r.status().is_success() => {
            let data: serde_json::Value = r.json().await.unwrap_or_default();
            data["result"]
                .as_array()
                .unwrap_or(&Vec::new())
                .iter()
                .map(|hit| {
                    serde_json::json!({
                        "book_id": hit["payload"]["book_id"].as_str().unwrap_or(""),
                        "book_title": hit["payload"]["book_title"].as_str().unwrap_or(""),
                        "chapter_title": hit["payload"]["chapter_title"],
                        "chapter_index": hit["payload"]["chapter_index"].as_i64().unwrap_or(0),
                        "chunk_index": hit["payload"]["chunk_index"].as_i64().unwrap_or(0),
                        "chunk_id": hit["payload"]["chunk_id"].as_str().unwrap_or(""),
                        "content_snippet": hit["payload"]["text"].as_str().or_else(|| hit["payload"]["content"].as_str()).unwrap_or(""),
                        "score": hit["score"].as_f64().unwrap_or(0.0),
                        "source": "semantic",
                        "highlighted": serde_json::Value::Null,
                    })
                })
                .collect()
        }
        Ok(_) => {
            tracing::warn!("Qdrant search returned error, falling back to DB");
            search_database(state, query, limit, book_ids).await
        }
        Err(e) => {
            tracing::warn!("Qdrant unreachable: {}, falling back to DB", e);
            search_database(state, query, limit, book_ids).await
        }
    }
}

/// Get embedding vector for a text using the configured embedding model.
async fn embed_text(state: &AppState, text: &str) -> Option<Vec<f64>> {
    let mut request = state
        .http_client
        .post(format!("{}/v1/embeddings", state.config.embedding_endpoint))
        .header("X-Failover-Enabled", "true")
        .json(&serde_json::json!({
            "input": [text],
            "model": state.config.embedding_model,
            "dimensions": state.config.embedding_dimensions,
        }));

    // Add API key if configured
    if !state.config.embedding_api_key.is_empty() {
        request = request.header(
            "Authorization",
            format!("Bearer {}", state.config.embedding_api_key),
        );
    }

    let resp = request.send().await.ok()?;

    if !resp.status().is_success() {
        tracing::warn!("Embedding API returned {}", resp.status());
        return None;
    }

    let data: serde_json::Value = resp.json().await.ok()?;
    let embedding = data["data"][0]["embedding"]
        .as_array()?
        .iter()
        .filter_map(|v| v.as_f64())
        .collect::<Vec<f64>>();

    if embedding.is_empty() {
        None
    } else {
        Some(embedding)
    }
}

/// Rerank search results using the configured reranker model.
/// Dynamically selects instruction based on detected query intent.
async fn rerank_results(
    state: &AppState,
    query: &str,
    results: Vec<serde_json::Value>,
) -> Vec<serde_json::Value> {
    // Collect content snippets for reranking
    let documents: Vec<String> = results
        .iter()
        .map(|r| r["content_snippet"].as_str().unwrap_or("").to_string())
        .collect();

    if documents.is_empty() {
        return results;
    }

    // Dynamic instruction based on query intent
    let instruction = detect_reranker_instruction(query);

    let mut request_body = serde_json::json!({
        "model": state.config.reranker_model,
        "query": query,
        "documents": documents,
        "top_n": results.len(),
    });

    // Add instruction if applicable (Qwen3-Reranker custom instruction support)
    if !instruction.is_empty() {
        request_body["instruction"] = serde_json::json!(instruction);
    }

    let resp = state
        .http_client
        .post(format!("{}/v1/rerank", state.config.reranker_endpoint))
        .json(&request_body)
        .send()
        .await;

    match resp {
        Ok(r) if r.status().is_success() => {
            let data: serde_json::Value = r.json().await.unwrap_or_default();
            if let Some(ranked) = data["results"].as_array() {
                let mut reranked: Vec<serde_json::Value> = ranked
                    .iter()
                    .filter_map(|item| {
                        let idx = item["index"].as_u64()? as usize;
                        let score = item["relevance_score"].as_f64().unwrap_or(0.0);
                        let mut result = results.get(idx)?.clone();
                        let explanation = build_rerank_explanation(
                            query,
                            result["content_snippet"].as_str().unwrap_or(""),
                        );
                        if let Some(obj) = result.as_object_mut() {
                            obj.insert("rerank_score".to_string(), serde_json::json!(score));
                            if let Some(explanation) = explanation {
                                obj.insert(
                                    "rerank_explanation".to_string(),
                                    serde_json::json!(format!("命中句：{}", explanation.sentence)),
                                );
                                obj.insert(
                                    "rerank_matched_terms".to_string(),
                                    serde_json::json!(explanation.matched_terms),
                                );
                            }
                        }
                        Some(result)
                    })
                    .collect();
                // Sort by reranker score descending
                reranked.sort_by(|a, b| {
                    let sa = a["rerank_score"].as_f64().unwrap_or(0.0);
                    let sb = b["rerank_score"].as_f64().unwrap_or(0.0);
                    sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
                });
                reranked
            } else {
                results
            }
        }
        _ => {
            tracing::warn!("Reranker unavailable, returning results without reranking");
            results
        }
    }
}

/// Detect query intent and select appropriate reranker instruction.
/// Qwen3-Reranker benefits from task-specific instructions (1-5% improvement).
fn detect_reranker_instruction(query: &str) -> &'static str {
    let q = query.to_lowercase();

    // Entity/relationship queries (who, character names, relationships)
    if q.contains("关系")
        || q.contains("什么关系")
        || q.contains("谁是")
        || q.contains("谁和")
        || q.contains("认识")
        || q.contains("师父")
        || q.contains("relation")
        || q.contains("who is")
    {
        return "判断文档是否提到了查询中涉及的人物关系、人物互动或社交联系";
    }

    // Plot/event queries (what happened, timeline)
    if q.contains("什么时候")
        || q.contains("怎么")
        || q.contains("发生了什么")
        || q.contains("情节")
        || q.contains("剧情")
        || q.contains("结局")
        || q.contains("后来")
        || q.contains("之后")
        || q.contains("what happen")
        || q.contains("plot")
    {
        return "判断文档是否描述了查询所问的故事情节、事件经过或时间线";
    }

    // World-building/setting queries
    if q.contains("设定")
        || q.contains("世界观")
        || q.contains("规则")
        || q.contains("体系")
        || q.contains("等级")
        || q.contains("功法")
        || q.contains("修炼")
        || q.contains("system")
        || q.contains("world")
    {
        return "判断文档是否包含关于查询所问的设定、规则、体系或世界观信息";
    }

    // General novel search (default)
    "根据用户对小说内容的搜索查询，判断文档段落是否与查询相关"
}

/// A short, user-facing label describing the detected query intent.
/// Mirrors `detect_reranker_instruction` so the UI can explain why results were ranked.
fn detect_query_intent(query: &str) -> &'static str {
    let q = query.to_lowercase();
    if q.contains("关系")
        || q.contains("什么关系")
        || q.contains("谁是")
        || q.contains("谁和")
        || q.contains("认识")
        || q.contains("师父")
        || q.contains("relation")
        || q.contains("who is")
    {
        return "人物关系";
    }
    if q.contains("什么时候")
        || q.contains("怎么")
        || q.contains("发生了什么")
        || q.contains("情节")
        || q.contains("剧情")
        || q.contains("结局")
        || q.contains("后来")
        || q.contains("之后")
        || q.contains("what happen")
        || q.contains("plot")
    {
        return "情节事件";
    }
    if q.contains("设定")
        || q.contains("世界观")
        || q.contains("规则")
        || q.contains("体系")
        || q.contains("等级")
        || q.contains("功法")
        || q.contains("修炼")
        || q.contains("system")
        || q.contains("world")
    {
        return "设定世界观";
    }
    "通用检索"
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RerankExplanation {
    sentence: String,
    matched_terms: Vec<String>,
}

fn build_rerank_explanation(query: &str, document: &str) -> Option<RerankExplanation> {
    let terms = extract_query_terms(query);
    if terms.is_empty() || document.trim().is_empty() {
        return None;
    }

    split_explanation_sentences(document)
        .into_iter()
        .filter_map(|sentence| {
            let sentence_lower = sentence.to_lowercase();
            let matched_terms: Vec<String> = terms
                .iter()
                .filter(|term| sentence_lower.contains(&term.to_lowercase()))
                .cloned()
                .collect();
            if matched_terms.is_empty() {
                return None;
            }
            let matched_chars = matched_terms
                .iter()
                .map(|term| term.chars().count())
                .sum::<usize>();
            let score = matched_terms.len() as f64 + matched_chars as f64 / 100.0
                - sentence.chars().count().min(240) as f64 / 10_000.0;
            Some((score, sentence, matched_terms))
        })
        .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(_, sentence, matched_terms)| RerankExplanation {
            sentence: clamp_explanation_sentence(&sentence, 160),
            matched_terms,
        })
}

fn extract_query_terms(query: &str) -> Vec<String> {
    let mut terms = Vec::new();
    for term in query.split(|ch: char| {
        ch.is_whitespace()
            || matches!(
                ch,
                ',' | '.'
                    | '，'
                    | '。'
                    | '、'
                    | '!'
                    | '?'
                    | '！'
                    | '？'
                    | ';'
                    | '；'
                    | ':'
                    | '：'
                    | '('
                    | ')'
                    | '['
                    | ']'
                    | '【'
                    | '】'
                    | '《'
                    | '》'
                    | '"'
                    | '\''
            )
    }) {
        let term = term.trim();
        if term.chars().count() < 2 {
            continue;
        }
        if !terms.iter().any(|existing| existing == term) {
            terms.push(term.to_string());
        }
    }
    terms
}

fn split_explanation_sentences(document: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();

    for ch in document.chars() {
        current.push(ch);
        if matches!(ch, '。' | '！' | '？' | '!' | '?' | ';' | '；' | '\n') {
            let sentence = current.trim();
            if !sentence.is_empty() {
                sentences.push(sentence.to_string());
            }
            current.clear();
        }
    }

    let tail = current.trim();
    if !tail.is_empty() {
        sentences.push(tail.to_string());
    }

    sentences
}

fn clamp_explanation_sentence(sentence: &str, max_chars: usize) -> String {
    if sentence.chars().count() <= max_chars {
        return sentence.to_string();
    }
    let mut clipped: String = sentence.chars().take(max_chars.saturating_sub(1)).collect();
    clipped.push('…');
    clipped
}

// ─── Supporting Types ────────────────────────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
struct FindSimilarRequest {
    text: String,
    #[serde(default = "default_k")]
    k: usize,
    #[serde(default)]
    book_ids: Vec<uuid::Uuid>,
}

fn default_k() -> usize {
    10
}

#[derive(Debug, serde::Deserialize)]
struct SuggestQuery {
    q: String,
    #[serde(default = "default_suggest_limit")]
    limit: usize,
}

fn default_suggest_limit() -> usize {
    5
}

// ─── Suggest Endpoint ────────────────────────────────────────────────────────

/// Auto-complete / suggestion endpoint for search bar.
async fn suggest(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(params): Query<SuggestQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    if params.q.trim().is_empty() {
        return Ok(Json(serde_json::json!({ "suggestions": [] })));
    }
    let scope = effective_book_scope(&state, &auth, std::iter::empty::<Uuid>()).await?;
    if scope.is_empty_restriction() {
        return Ok(Json(serde_json::json!({ "suggestions": [] })));
    }
    let book_id_strings = scope.id_strings();

    // Query Meilisearch with limited results for speed
    let mut search_body = serde_json::json!({
        "q": params.q,
        "limit": params.limit,
        "attributesToRetrieve": ["book_title", "chapter_title"],
    });
    add_meili_book_filter(&mut search_body, &book_id_strings);
    let resp = state
        .http_client
        .post(format!("{}/indexes/chunks/search", state.config.meili_url))
        .header(
            "Authorization",
            format!("Bearer {}", state.config.meili_master_key),
        )
        .json(&search_body)
        .send()
        .await;

    let suggestions = match resp {
        Ok(r) if r.status().is_success() => {
            let data: serde_json::Value = r.json().await.unwrap_or_default();
            let hits = data
                .get("hits")
                .and_then(|h| h.as_array())
                .cloned()
                .unwrap_or_default();
            // Deduplicate by book_title
            let mut seen = std::collections::HashSet::new();
            hits.into_iter()
                .filter_map(|hit| {
                    let title = hit.get("book_title")?.as_str()?.to_string();
                    if seen.insert(title.clone()) {
                        Some(serde_json::json!({
                            "text": title,
                            "type": "book",
                        }))
                    } else {
                        None
                    }
                })
                .take(params.limit)
                .collect::<Vec<_>>()
        }
        _ => Vec::new(),
    };

    Ok(Json(serde_json::json!({ "suggestions": suggestions })))
}

// ─── Dynamic top_k ───────────────────────────────────────────────────────────

/// Compute optimal top_k for vector search based on query complexity.
/// Short/simple queries get fewer results; complex/multi-concept queries get more.
fn compute_dynamic_top_k(query: &str, base_limit: usize) -> usize {
    let char_count = query.chars().count();
    let word_count = query.split_whitespace().count();

    // Heuristic: complexity factors
    let has_question_mark = query.contains('?') || query.contains('？');
    let has_quotes = query.contains('"') || query.contains('"') || query.contains('"');
    let has_multiple_concepts = word_count >= 5;

    let complexity_multiplier = if char_count < 10 && word_count <= 2 {
        // Very short query (single term): retrieve more to cast a wide net
        1.5
    } else if has_question_mark || has_multiple_concepts {
        // Complex query: need more candidates for reranker to pick from
        2.0
    } else if has_quotes {
        // Quoted/exact: fewer results, precision-focused
        1.0
    } else {
        // Default
        1.2
    };

    let k = (base_limit as f64 * complexity_multiplier).round() as usize;
    k.clamp(5, 100)
}

// ─── Context Expansion ───────────────────────────────────────────────────────

/// GET /search/context/:chunk_id?window=3
/// Returns surrounding chunks for context expansion.
async fn get_chunk_context(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(chunk_id): Path<String>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> ApiResult<Json<serde_json::Value>> {
    let window = params
        .get("window")
        .and_then(|w| w.parse::<i32>().ok())
        .unwrap_or(2)
        .min(5);

    // Find the target chunk's chapter_id and chunk_index in Meilisearch/DB
    // First try Qdrant payload for metadata
    let scroll_resp = state
        .http_client
        .post(format!(
            "{}/collections/nova_chunks/points/scroll",
            state.config.qdrant_url
        ))
        .json(&serde_json::json!({
            "filter": {
                "must": [{ "key": "chunk_id", "match": { "value": chunk_id } }]
            },
            "limit": 1,
            "with_payload": true,
        }))
        .send()
        .await;

    let (book_id, chapter_index, chunk_index) = match scroll_resp {
        Ok(r) if r.status().is_success() => {
            let data: serde_json::Value = r.json().await.unwrap_or_default();
            let point = data["result"]["points"].as_array().and_then(|p| p.first());
            match point {
                Some(p) => {
                    let payload = &p["payload"];
                    (
                        payload["book_id"].as_str().unwrap_or("").to_string(),
                        payload["chapter_index"].as_i64().unwrap_or(0) as i32,
                        payload["chunk_index"].as_i64().unwrap_or(0) as i32,
                    )
                }
                None => {
                    return Ok(Json(serde_json::json!({
                        "chunk_id": chunk_id,
                        "context": [],
                        "error": "chunk_not_found",
                    })))
                }
            }
        }
        _ => {
            return Ok(Json(serde_json::json!({
                "chunk_id": chunk_id,
                "context": [],
                "error": "qdrant_unavailable",
            })))
        }
    };
    let parsed_book_id =
        Uuid::parse_str(&book_id).map_err(|_| ApiError::bad_request("Invalid chunk book_id"))?;
    ensure_book_access(&state, &auth, parsed_book_id, LibraryAccess::Read).await?;

    // Fetch surrounding chunks from Qdrant by filtering on book_id + chapter_index
    // and filtering chunk_index in range [chunk_index - window, chunk_index + window]
    let range_min = (chunk_index - window).max(0);
    let range_max = chunk_index + window;

    let search_resp = state
        .http_client
        .post(format!(
            "{}/collections/nova_chunks/points/scroll",
            state.config.qdrant_url
        ))
        .json(&serde_json::json!({
            "filter": {
                "must": [
                    { "key": "book_id", "match": { "value": book_id } },
                    { "key": "chapter_index", "match": { "value": chapter_index } },
                    { "key": "chunk_index", "range": { "gte": range_min, "lte": range_max } },
                ]
            },
            "limit": (window * 2 + 1) as u64,
            "with_payload": true,
        }))
        .send()
        .await;

    let context_chunks = match search_resp {
        Ok(r) if r.status().is_success() => {
            let data: serde_json::Value = r.json().await.unwrap_or_default();
            let mut points: Vec<serde_json::Value> = data["result"]["points"]
                .as_array()
                .unwrap_or(&Vec::new())
                .iter()
                .map(|p| {
                    let payload = &p["payload"];
                    serde_json::json!({
                        "chunk_id": payload["chunk_id"],
                        "chunk_index": payload["chunk_index"],
                        "content": payload["text"].as_str().or_else(|| payload["content"].as_str()).unwrap_or(""),
                        "is_target": payload["chunk_id"].as_str() == Some(&chunk_id),
                    })
                })
                .collect();
            points.sort_by_key(|p| p["chunk_index"].as_i64().unwrap_or(0));
            points
        }
        _ => Vec::new(),
    };

    Ok(Json(serde_json::json!({
        "chunk_id": chunk_id,
        "book_id": book_id,
        "chapter_index": chapter_index,
        "window": window,
        "context": context_chunks,
    })))
}

// ─── Graph Search Mode ───────────────────────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
struct GraphSearchRequest {
    query: String,
    #[serde(default)]
    book_id: Option<String>,
    #[serde(default = "default_graph_limit")]
    limit: usize,
}

fn default_graph_limit() -> usize {
    20
}

/// POST /search/graph — Graph-augmented search.
/// 1. Extract entities from query
/// 2. Find entity paths in Neo4j
/// 3. Retrieve related chunks
/// 4. Merge with vector search for comprehensive results
async fn search_graph(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<GraphSearchRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let total_start = std::time::Instant::now();
    let book_filter = parse_optional_book_id(body.book_id.as_deref())?;
    let scope = effective_book_scope(&state, &auth, book_filter.iter().copied()).await?;
    if scope.is_empty_restriction() {
        return Ok(Json(serde_json::json!({
            "results": [],
            "paths": [],
            "entities": [],
            "related_entities": [],
            "timing": { "total_ms": total_start.elapsed().as_millis() as u64 },
        })));
    }
    let book_filter_strings = scope.id_strings();

    if body.query.trim().is_empty() {
        return Ok(Json(serde_json::json!({
            "results": [],
            "paths": [],
            "entities": [],
        })));
    }

    // Step 1: Extract potential entity names from the query
    // Simple heuristic: look for known entities in the query
    let query_entities = find_entities_in_query(&state, &body.query, &book_filter_strings).await;

    // Step 2: Graph traversal — find paths between entities
    let mut paths = Vec::new();
    let mut related_entity_names: Vec<String> = Vec::new();

    if query_entities.len() >= 2 {
        // Multi-entity: find paths between them
        for i in 0..query_entities.len().min(3) {
            for j in (i + 1)..query_entities.len().min(3) {
                let path = find_path_between(
                    &state,
                    &query_entities[i],
                    &query_entities[j],
                    &book_filter_strings,
                )
                .await;
                if let Some(p) = path {
                    paths.push(p);
                }
            }
        }
    } else if query_entities.len() == 1 {
        // Single entity: find 1-hop neighbors
        let neighbors = find_neighbors(&state, &query_entities[0], &book_filter_strings).await;
        related_entity_names.extend(
            neighbors
                .iter()
                .map(|n| n["name"].as_str().unwrap_or("").to_string()),
        );
    }

    // Step 3: Enhance search with entity names as additional keywords
    let enhanced_query = if !related_entity_names.is_empty() {
        let extra = related_entity_names
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join(" ");
        format!("{} {}", body.query, extra)
    } else {
        body.query.clone()
    };

    // Step 4: Run semantic search with enhanced query
    let results =
        search_qdrant_semantic(&state, &enhanced_query, body.limit, &book_filter_strings).await;

    // Step 5: Also search for entity mentions in chunks
    let entity_chunks = if !query_entities.is_empty() {
        let entity_query = query_entities.join(" ");
        search_meilisearch(
            &state,
            &entity_query,
            body.limit / 2,
            0,
            &book_filter_strings,
            false,
        )
        .await
    } else {
        Vec::new()
    };

    // Merge (RRF-style but simple dedup)
    let mut all_results = results;
    for ec in entity_chunks {
        let key = format!(
            "{}:{}",
            ec.get("book_id").and_then(|v| v.as_str()).unwrap_or(""),
            ec.get("chapter_index")
                .and_then(|v| v.as_i64())
                .unwrap_or(0)
        );
        let exists = all_results.iter().any(|r| {
            format!(
                "{}:{}",
                r.get("book_id").and_then(|v| v.as_str()).unwrap_or(""),
                r.get("chapter_index").and_then(|v| v.as_i64()).unwrap_or(0)
            ) == key
        });
        if !exists {
            all_results.push(ec);
        }
    }
    all_results.truncate(body.limit);

    // Mark results with graph source
    for result in all_results.iter_mut() {
        if let Some(obj) = result.as_object_mut() {
            obj.insert("source".to_string(), serde_json::json!("graph"));
        }
    }

    let total_ms = total_start.elapsed().as_millis() as u64;

    // Rank paths by weighted relevance for explainability.
    paths = rank_graph_paths(paths, &query_entities);

    Ok(Json(serde_json::json!({
        "results": all_results,
        "paths": paths,
        "entities": query_entities,
        "related_entities": related_entity_names,
        "timing": { "total_ms": total_ms },
    })))
}

fn rank_graph_paths(
    mut paths: Vec<serde_json::Value>,
    query_entities: &[String],
) -> Vec<serde_json::Value> {
    for path in paths.iter_mut() {
        let score = graph_path_score(path, query_entities);
        let reason = graph_path_rank_reason(path, query_entities);
        if let Some(obj) = path.as_object_mut() {
            obj.insert("path_score".to_string(), serde_json::json!(score));
            obj.insert("rank_reason".to_string(), serde_json::json!(reason));
        }
    }

    paths.sort_by(|a, b| {
        let score_a = a["path_score"].as_f64().unwrap_or(0.0);
        let score_b = b["path_score"].as_f64().unwrap_or(0.0);
        score_b
            .partial_cmp(&score_a)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                let len_a = a["length"].as_i64().unwrap_or(i64::MAX);
                let len_b = b["length"].as_i64().unwrap_or(i64::MAX);
                len_a.cmp(&len_b)
            })
    });

    paths
}

fn graph_path_score(path: &serde_json::Value, query_entities: &[String]) -> f64 {
    let length = graph_path_length(path).max(1) as f64;
    let directness = 1.0 / (length + 1.0);
    let coverage = graph_path_query_coverage(path, query_entities);
    let relation_strength = graph_path_relation_strength(path);

    directness * 0.45 + coverage * 0.40 + relation_strength * 0.15
}

fn graph_path_rank_reason(path: &serde_json::Value, query_entities: &[String]) -> String {
    let mut parts = Vec::new();
    let covered = graph_path_covered_query_entities(path, query_entities);
    if !query_entities.is_empty() {
        parts.push(format!(
            "覆盖 {}/{} 个查询实体",
            covered,
            query_entities.len()
        ));
    }

    let length = graph_path_length(path);
    if length <= 1 {
        parts.push("直接路径".to_string());
    } else {
        parts.push(format!("{} 跳路径", length));
    }

    if let Some(relation) = strongest_graph_relation(path) {
        parts.push(format!("强关系：{}", relation));
    }

    parts.join(" · ")
}

fn graph_path_length(path: &serde_json::Value) -> usize {
    path.get("length")
        .and_then(|value| value.as_u64())
        .map(|length| length as usize)
        .or_else(|| {
            path.get("nodes")
                .and_then(|nodes| nodes.as_array())
                .map(|nodes| nodes.len().saturating_sub(1))
        })
        .unwrap_or(usize::MAX)
}

fn graph_path_query_coverage(path: &serde_json::Value, query_entities: &[String]) -> f64 {
    if query_entities.is_empty() {
        return 0.0;
    }
    graph_path_covered_query_entities(path, query_entities) as f64 / query_entities.len() as f64
}

fn graph_path_covered_query_entities(path: &serde_json::Value, query_entities: &[String]) -> usize {
    let node_names: Vec<String> = path
        .get("nodes")
        .and_then(|nodes| nodes.as_array())
        .map(|nodes| {
            nodes
                .iter()
                .filter_map(|node| node.as_str().map(|name| name.to_string()))
                .collect()
        })
        .unwrap_or_default();

    query_entities
        .iter()
        .filter(|entity| node_names.iter().any(|name| name == *entity))
        .count()
}

fn graph_path_relation_strength(path: &serde_json::Value) -> f64 {
    path.get("relationships")
        .and_then(|relationships| relationships.as_array())
        .map(|relationships| {
            relationships
                .iter()
                .filter_map(|relation| relation.as_str())
                .map(graph_relation_weight)
                .fold(0.0_f64, f64::max)
        })
        .unwrap_or(0.0)
}

fn strongest_graph_relation(path: &serde_json::Value) -> Option<String> {
    path.get("relationships")
        .and_then(|relationships| relationships.as_array())
        .and_then(|relationships| {
            relationships
                .iter()
                .filter_map(|relation| relation.as_str())
                .filter(|relation| graph_relation_weight(relation) >= 0.8)
                .max_by(|a, b| {
                    graph_relation_weight(a)
                        .partial_cmp(&graph_relation_weight(b))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|relation| relation.to_string())
        })
}

fn graph_relation_weight(relation: &str) -> f64 {
    let relation = relation.to_lowercase();
    if relation.contains("师徒")
        || relation.contains("亲属")
        || relation.contains("父")
        || relation.contains("母")
        || relation.contains("伴侣")
        || relation.contains("盟友")
        || relation.contains("敌")
        || relation.contains("隶属")
        || relation.contains("leader")
        || relation.contains("member")
        || relation.contains("ally")
        || relation.contains("enemy")
    {
        1.0
    } else if relation.contains("朋友")
        || relation.contains("同门")
        || relation.contains("合作")
        || relation.contains("friend")
        || relation.contains("mentor")
    {
        0.85
    } else if relation.contains("相关") || relation.contains("关联") || relation.contains("related")
    {
        0.25
    } else {
        0.5
    }
}

/// Find entity names that appear in the query by checking against the DB.
async fn find_entities_in_query(state: &AppState, query: &str, book_ids: &[String]) -> Vec<String> {
    let book_uuid_filters: Vec<Uuid> = book_ids
        .iter()
        .filter_map(|id| Uuid::parse_str(id).ok())
        .collect();
    let entities: Vec<(String,)> = if book_uuid_filters.is_empty() {
        sqlx::query_as(
            "SELECT DISTINCT name FROM entities WHERE $1 ILIKE '%' || name || '%' LIMIT 10",
        )
        .bind(query)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default()
    } else {
        sqlx::query_as(
            "SELECT DISTINCT name FROM entities WHERE $1 ILIKE '%' || name || '%' AND book_id = ANY($2) LIMIT 10",
        )
        .bind(query)
        .bind(&book_uuid_filters)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default()
    };

    entities.into_iter().map(|(name,)| name).collect()
}

/// Find shortest path between two entities in Neo4j.
async fn find_path_between(
    state: &AppState,
    entity_a: &str,
    entity_b: &str,
    book_ids: &[String],
) -> Option<serde_json::Value> {
    let (cypher, mut params) = graph_path_cypher(book_ids);
    if let Some(object) = params.as_object_mut() {
        object.insert("source".to_string(), serde_json::json!(entity_a));
        object.insert("target".to_string(), serde_json::json!(entity_b));
    }

    let result = state.neo4j.execute(&cypher, Some(params)).await.ok()?;

    // Parse Neo4j response
    let data = result["results"].as_array()?.first()?["data"]
        .as_array()?
        .first()?["row"]
        .as_array()?;

    let nodes = data.first()?.as_array()?;
    let rels = data.get(1)?.as_array()?;

    // Build a human-readable explanation: 张三 —(师徒)→ 李四 —(敌对)→ 王五
    let node_names: Vec<String> = nodes
        .iter()
        .map(|n| n.as_str().unwrap_or("?").to_string())
        .collect();
    let rel_names: Vec<String> = rels
        .iter()
        .map(|r| r.as_str().unwrap_or("关联").to_string())
        .collect();
    let mut explanation = String::new();
    for (i, name) in node_names.iter().enumerate() {
        explanation.push_str(name);
        if i < rel_names.len() {
            explanation.push_str(&format!(" —({})→ ", rel_names[i]));
        }
    }

    Some(serde_json::json!({
        "source": entity_a,
        "target": entity_b,
        "nodes": nodes,
        "relationships": rels,
        "length": nodes.len() - 1,
        "explanation": explanation,
    }))
}

/// Find 1-hop neighbors of an entity in Neo4j.
async fn find_neighbors(
    state: &AppState,
    entity: &str,
    book_ids: &[String],
) -> Vec<serde_json::Value> {
    let (cypher, mut params) = graph_neighbor_cypher(book_ids);
    if let Some(object) = params.as_object_mut() {
        object.insert("name".to_string(), serde_json::json!(entity));
    }

    match state.neo4j.execute(&cypher, Some(params)).await {
        Ok(result) => result["results"]
            .as_array()
            .and_then(|r| r.first())
            .and_then(|r| r["data"].as_array())
            .map(|data| {
                data.iter()
                    .filter_map(|row| {
                        let r = row["row"].as_array()?;
                        Some(serde_json::json!({
                            "name": r.first()?,
                            "relation": r.get(1)?,
                            "type": r.get(2)?,
                        }))
                    })
                    .collect()
            })
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

fn graph_path_cypher(book_ids: &[String]) -> (String, serde_json::Value) {
    if book_ids.is_empty() {
        return (
            "MATCH (a {name: $source}), (b {name: $target}) \
             MATCH p = shortestPath((a)-[*..5]->(b)) \
             RETURN [n IN nodes(p) | n.name] AS path_nodes, \
                    [r IN relationships(p) | type(r)] AS path_rels \
             LIMIT 1"
                .to_string(),
            serde_json::json!({}),
        );
    }

    (
        "MATCH (a {name: $source}), (b {name: $target}) \
         WHERE a.book_id IN $book_ids AND b.book_id IN $book_ids \
         MATCH p = shortestPath((a)-[*..5]->(b)) \
         WHERE all(n IN nodes(p) WHERE n.book_id IN $book_ids) \
         RETURN [n IN nodes(p) | n.name] AS path_nodes, \
                [r IN relationships(p) | type(r)] AS path_rels \
         LIMIT 1"
            .to_string(),
        serde_json::json!({ "book_ids": book_ids }),
    )
}

fn graph_neighbor_cypher(book_ids: &[String]) -> (String, serde_json::Value) {
    if book_ids.is_empty() {
        return (
            "MATCH (a {name: $name})-[r]-(b) \
             RETURN b.name AS name, type(r) AS relation, labels(b)[0] AS type \
             LIMIT 20"
                .to_string(),
            serde_json::json!({}),
        );
    }

    (
        "MATCH (a {name: $name})-[r]-(b) \
         WHERE a.book_id IN $book_ids AND b.book_id IN $book_ids \
         RETURN b.name AS name, type(r) AS relation, labels(b)[0] AS type \
         LIMIT 20"
            .to_string(),
        serde_json::json!({ "book_ids": book_ids }),
    )
}

// ─── Cross-Book Search ───────────────────────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
struct CrossBookSearchRequest {
    query: String,
    #[serde(default)]
    book_ids: Vec<String>,
    #[serde(default = "default_cross_limit")]
    limit: usize,
    #[serde(default, rename = "group_by_book")]
    _group_by_book: bool,
}

fn default_cross_limit() -> usize {
    30
}

/// POST /search/cross-book — Search across multiple books with grouped results.
async fn search_cross_book(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<CrossBookSearchRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let total_start = std::time::Instant::now();
    let book_filters = parse_book_ids(&body.book_ids)?;
    let scope = effective_book_scope(&state, &auth, book_filters.iter().copied()).await?;
    if scope.is_empty_restriction() {
        return Ok(Json(serde_json::json!({
            "results": [],
            "groups": [],
            "total": 0,
            "query": body.query,
            "timing": { "total_ms": total_start.elapsed().as_millis() as u64 },
        })));
    }
    let book_id_strings = scope.id_strings();

    if body.query.trim().is_empty() {
        return Ok(Json(serde_json::json!({
            "groups": [],
            "total": 0,
            "query": body.query,
            "timing": { "total_ms": total_start.elapsed().as_millis() as u64 },
        })));
    }

    // Perform search across all books (or filtered set)
    let mut search_body = serde_json::json!({
        "q": body.query,
        "limit": body.limit.min(50),
        "showRankingScore": true,
        "attributesToRetrieve": [
            "id",
            "chunk_id",
            "book_id",
            "book_title",
            "chapter_title",
            "chapter_index",
            "chunk_index",
            "content"
        ],
        "attributesToHighlight": ["content"],
        "highlightPreTag": "<mark>",
        "highlightPostTag": "</mark>",
    });

    // Add filter if specific book_ids requested
    if !book_id_strings.is_empty() {
        let filter_parts: Vec<String> = book_id_strings
            .iter()
            .map(|id| format!("book_id = \"{}\"", id))
            .collect();
        search_body["filter"] = serde_json::json!(filter_parts.join(" OR "));
    }

    let resp = state
        .http_client
        .post(format!("{}/indexes/chunks/search", state.config.meili_url))
        .header(
            "Authorization",
            format!("Bearer {}", state.config.meili_master_key),
        )
        .json(&search_body)
        .send()
        .await;

    let results = match resp {
        Ok(r) if r.status().is_success() => {
            let data: serde_json::Value = r.json().await.unwrap_or_default();
            data.get("hits")
                .and_then(|h| h.as_array())
                .cloned()
                .unwrap_or_default()
        }
        _ => Vec::new(),
    };

    // Group by book
    let mut book_groups: std::collections::HashMap<String, Vec<serde_json::Value>> =
        std::collections::HashMap::new();

    for hit in &results {
        let book_id = hit
            .get("book_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let book_title = hit
            .get("book_title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let key = format!("{}|{}", book_id, book_title);

        let entry = book_groups.entry(key).or_default();
        entry.push(serde_json::json!({
            "id": hit.get("id").or_else(|| hit.get("chunk_id")),
            "chunk_id": hit.get("chunk_id").or_else(|| hit.get("id")),
            "content": hit.get("content").and_then(|v| v.as_str()).unwrap_or(""),
            "highlighted": hit.get("_formatted").and_then(|f| f.get("content")).and_then(|v| v.as_str()),
            "chapter_title": hit.get("chapter_title"),
            "chapter_index": hit.get("chapter_index").and_then(|v| v.as_i64()).unwrap_or(0),
            "chunk_index": hit.get("chunk_index").and_then(|v| v.as_i64()),
            "score": hit.get("_rankingScore").and_then(|v| v.as_f64()).unwrap_or(0.0),
        }));
    }

    let mut groups: Vec<serde_json::Value> = book_groups
        .into_iter()
        .map(|(key, chunks)| {
            let parts: Vec<&str> = key.splitn(2, '|').collect();
            serde_json::json!({
                "book_id": parts.first().unwrap_or(&""),
                "book_title": parts.get(1).unwrap_or(&""),
                "count": chunks.len(),
                "top_score": chunks.first().and_then(|c| c["score"].as_f64()).unwrap_or(0.0),
                "chunks": chunks,
            })
        })
        .collect();
    groups.sort_by(|a, b| {
        b["top_score"]
            .as_f64()
            .partial_cmp(&a["top_score"].as_f64())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let total_ms = total_start.elapsed().as_millis() as u64;

    Ok(Json(serde_json::json!({
        "groups": groups,
        "total": results.len(),
        "query": body.query,
        "timing": { "total_ms": total_ms },
    })))
}

fn parse_book_ids(book_ids: &[String]) -> ApiResult<Vec<Uuid>> {
    book_ids
        .iter()
        .map(|id| Uuid::parse_str(id).map_err(|_| ApiError::bad_request("Invalid book_id filter")))
        .collect()
}

fn parse_optional_book_id(book_id: Option<&str>) -> ApiResult<Vec<Uuid>> {
    match book_id {
        Some(id) => parse_book_ids(&[id.to_string()]),
        None => Ok(Vec::new()),
    }
}

fn uuid_strings(ids: impl IntoIterator<Item = Uuid>) -> Vec<String> {
    ids.into_iter().map(|id| id.to_string()).collect()
}

struct SearchBookScope {
    restricted: bool,
    ids: Vec<Uuid>,
}

impl SearchBookScope {
    fn unrestricted() -> Self {
        Self {
            restricted: false,
            ids: Vec::new(),
        }
    }

    fn restricted(ids: Vec<Uuid>) -> Self {
        Self {
            restricted: true,
            ids,
        }
    }

    fn is_empty_restriction(&self) -> bool {
        self.restricted && self.ids.is_empty()
    }

    fn id_strings(&self) -> Vec<String> {
        uuid_strings(self.ids.iter().copied())
    }
}

async fn effective_book_scope(
    state: &AppState,
    auth: &AuthUser,
    requested_book_ids: impl IntoIterator<Item = Uuid>,
) -> ApiResult<SearchBookScope> {
    let requested: Vec<Uuid> = requested_book_ids.into_iter().collect();
    if !requested.is_empty() {
        ensure_book_filters_access(state, auth, requested.iter().copied()).await?;
        return Ok(SearchBookScope::restricted(requested));
    }

    match visible_library_ids(state, auth, LibraryAccess::Read).await? {
        None => Ok(SearchBookScope::unrestricted()),
        Some(library_ids) if library_ids.is_empty() => Ok(SearchBookScope::restricted(Vec::new())),
        Some(library_ids) => {
            let book_ids = sqlx::query_scalar("SELECT id FROM books WHERE library_id = ANY($1)")
                .bind(&library_ids)
                .fetch_all(&state.db)
                .await
                .map_err(ApiError::from)?;
            Ok(SearchBookScope::restricted(book_ids))
        }
    }
}

fn add_meili_book_filter(search_body: &mut serde_json::Value, book_ids: &[String]) {
    if book_ids.is_empty() {
        return;
    }
    let filter_parts: Vec<String> = book_ids
        .iter()
        .map(|id| format!("book_id = \"{}\"", id))
        .collect();
    search_body["filter"] = serde_json::Value::String(filter_parts.join(" OR "));
}

fn add_qdrant_book_filter(search_body: &mut serde_json::Value, book_ids: &[String]) {
    if book_ids.is_empty() {
        return;
    }
    search_body["filter"] = serde_json::json!({
        "should": book_ids.iter().map(|id| {
            serde_json::json!({ "key": "book_id", "match": { "value": id } })
        }).collect::<Vec<_>>()
    });
}

fn cypher_book_filter(field: &str, book_ids: &[String]) -> String {
    if book_ids.is_empty() {
        return String::new();
    }
    let list = book_ids
        .iter()
        .map(|id| format!("'{}'", id))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{field} IN [{list}]")
}

async fn ensure_book_filters_access(
    state: &AppState,
    auth: &AuthUser,
    book_ids: impl IntoIterator<Item = Uuid>,
) -> ApiResult<()> {
    for book_id in book_ids {
        ensure_book_access(state, auth, book_id, LibraryAccess::Read).await?;
    }
    Ok(())
}

// ─── GlobalSearch: Map-Reduce over Community Summaries ─────────────────────

#[derive(serde::Deserialize)]
struct GlobalSearchRequest {
    query: String,
    book_id: Option<String>,
    /// Community level to search (default: 1)
    level: Option<i32>,
}

/// POST /search/global — Answer broad questions using community summaries (GraphRAG GlobalSearch).
/// Implements map-reduce: fetch all community summaries → map (extract relevant info) → reduce (synthesize answer).
async fn search_global(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<GlobalSearchRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let total_start = std::time::Instant::now();
    let level = body.level.unwrap_or(1);
    let book_filter = parse_optional_book_id(body.book_id.as_deref())?;
    let scope = effective_book_scope(&state, &auth, book_filter.iter().copied()).await?;
    if scope.is_empty_restriction() {
        return Ok(Json(serde_json::json!({
            "answer": "暂无可访问的社区数据。",
            "sources": [],
            "timing": { "total_ms": total_start.elapsed().as_millis() as u64 },
        })));
    }
    let book_filter_strings = scope.id_strings();

    if body.query.trim().is_empty() {
        return Ok(Json(serde_json::json!({
            "answer": "",
            "sources": [],
        })));
    }

    // 1. Fetch community summaries from Neo4j
    let cypher = if !book_filter_strings.is_empty() {
        let book_filter = cypher_book_filter("c.book_id", &book_filter_strings);
        format!(
            "MATCH (c:Community {{level: {}}}) \
             WHERE {} AND c.summary IS NOT NULL AND c.summary <> '' \
             RETURN c.id, c.summary, c.key_findings, c.members \
             ORDER BY c.entity_count DESC LIMIT 30",
            level, book_filter
        )
    } else {
        format!(
            "MATCH (c:Community {{level: {}}}) \
             WHERE c.summary IS NOT NULL AND c.summary <> '' \
             RETURN c.id, c.summary, c.key_findings, c.members \
             ORDER BY c.entity_count DESC LIMIT 30",
            level
        )
    };

    let result = state
        .neo4j
        .execute(&cypher, None)
        .await
        .map_err(|e| ApiError::Internal(format!("Neo4j query failed: {}", e)))?;

    let communities: Vec<(String, String, Vec<String>, Vec<String>)> = result["results"]
        .as_array()
        .and_then(|r| r.first())
        .and_then(|r| r["data"].as_array())
        .map(|data| {
            data.iter()
                .filter_map(|row| {
                    let r = row["row"].as_array()?;
                    let id = r.first()?.as_str()?.to_string();
                    let summary = r.get(1)?.as_str()?.to_string();
                    let findings: Vec<String> = r
                        .get(2)?
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default();
                    let members: Vec<String> = r
                        .get(3)?
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default();
                    Some((id, summary, findings, members))
                })
                .collect()
        })
        .unwrap_or_default();

    if communities.is_empty() {
        return Ok(Json(serde_json::json!({
            "answer": "暂无社区数据。请先运行社区检测和摘要生成。",
            "sources": [],
            "timing": { "total_ms": total_start.elapsed().as_millis() as u64 },
        })));
    }

    // 2. MAP phase: build context from all relevant community summaries
    let context_parts: Vec<String> = communities
        .iter()
        .map(|(id, summary, findings, members)| {
            let findings_str = if findings.is_empty() {
                String::new()
            } else {
                format!("\n  关键发现: {}", findings.join("; "))
            };
            format!(
                "[社区{}] 成员: {} | 摘要: {}{}",
                id,
                members
                    .iter()
                    .take(5)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("、"),
                summary,
                findings_str,
            )
        })
        .collect();

    let map_context = context_parts.join("\n\n");

    // 3. REDUCE phase: synthesize answer using LLM
    let reduce_prompt = format!(
        "你是一个小说分析助手。基于以下社区摘要信息，回答用户的问题。\n\n\
         ## 社区摘要\n{}\n\n## 用户问题\n{}\n\n\
         请给出简洁准确的回答（200字以内）。如果信息不足，说明需要更多数据。\
         同时列出你参考了哪些社区（以ID形式）。\n\n\
         格式：\n```json\n{{\n  \"answer\": \"回答内容\",\n  \"source_communities\": [\"C0\", \"C1\"]\n}}\n```",
        map_context, body.query,
    );

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
            "messages": [{"role": "user", "content": reduce_prompt}],
            "temperature": 0.3,
            "max_tokens": 800,
        }))
        .send()
        .await;

    let (answer, source_communities) = match resp {
        Ok(r) if r.status().is_success() => {
            if let Ok(body) = r.json::<serde_json::Value>().await {
                let content = body["choices"][0]["message"]["content"]
                    .as_str()
                    .unwrap_or_default();

                let json_str = content
                    .trim()
                    .trim_start_matches("```json")
                    .trim_start_matches("```")
                    .trim_end_matches("```")
                    .trim();

                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_str) {
                    let answer = parsed["answer"].as_str().unwrap_or(content).to_string();
                    let sources: Vec<String> = parsed["source_communities"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default();
                    (answer, sources)
                } else {
                    (content.to_string(), Vec::new())
                }
            } else {
                ("LLM 响应解析失败".to_string(), Vec::new())
            }
        }
        _ => ("LLM 调用失败".to_string(), Vec::new()),
    };

    let total_ms = total_start.elapsed().as_millis() as u64;

    // Build source details for referenced communities
    let sources: Vec<serde_json::Value> = source_communities
        .iter()
        .filter_map(|cid| {
            communities
                .iter()
                .find(|(id, _, _, _)| id == cid)
                .map(|(id, summary, _, members)| {
                    serde_json::json!({
                        "community_id": id,
                        "summary": summary,
                        "members": members.iter().take(8).collect::<Vec<_>>(),
                    })
                })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "answer": answer,
        "sources": sources,
        "communities_analyzed": communities.len(),
        "timing": { "total_ms": total_ms },
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rerank_explanation_selects_sentence_with_query_terms() {
        let document = "风雪很大。主角终于知道师父真实身份，也明白旧约。另一段只是在写山路。";

        let explanation = build_rerank_explanation("师父 身份", document)
            .expect("query terms should match a sentence");

        assert_eq!(
            explanation.sentence,
            "主角终于知道师父真实身份，也明白旧约。"
        );
        assert_eq!(explanation.matched_terms, vec!["师父", "身份"]);
    }

    #[test]
    fn graph_paths_rank_by_weighted_relevance_not_only_length() {
        let indirect = serde_json::json!({
            "nodes": ["张三", "路人甲", "李四"],
            "relationships": ["相关", "相关"],
            "length": 2,
        });
        let direct = serde_json::json!({
            "nodes": ["张三", "李四"],
            "relationships": ["师徒"],
            "length": 1,
        });

        let ranked = rank_graph_paths(
            vec![indirect, direct],
            &["张三".to_string(), "李四".to_string()],
        );

        assert_eq!(ranked[0]["relationships"][0], "师徒");
        assert!(
            ranked[0]["path_score"].as_f64().unwrap() > ranked[1]["path_score"].as_f64().unwrap()
        );
        assert_eq!(
            ranked[0]["rank_reason"],
            "覆盖 2/2 个查询实体 · 直接路径 · 强关系：师徒"
        );
    }

    #[test]
    fn graph_path_query_constrains_every_node_in_shortest_path() {
        let book_ids = vec!["018f15ec-80f0-7000-9000-000000000001".to_string()];
        let (cypher, params) = graph_path_cypher(&book_ids);

        assert!(cypher.contains("all(n IN nodes(p)"));
        assert!(cypher.contains("n.book_id IN $book_ids"));
        assert_eq!(
            params
                .get("book_ids")
                .and_then(|value| value.as_array())
                .map(Vec::len),
            Some(1)
        );
    }

    #[test]
    fn graph_neighbor_query_constrains_both_sides_when_scoped() {
        let book_ids = vec!["018f15ec-80f0-7000-9000-000000000001".to_string()];
        let (cypher, params) = graph_neighbor_cypher(&book_ids);

        assert!(cypher.contains("a.book_id IN $book_ids"));
        assert!(cypher.contains("b.book_id IN $book_ids"));
        assert_eq!(
            params
                .get("book_ids")
                .and_then(|value| value.as_array())
                .map(Vec::len),
            Some(1)
        );
    }
}
