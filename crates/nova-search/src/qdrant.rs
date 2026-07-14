use nova_core::domain::search::{SearchMode, SearchResult};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::embedding::EmbeddingClient;

/// Client for Qdrant vector database operations.
pub struct QdrantClient {
    client: Client,
    base_url: String,
    collection_name: String,
    embedding: Option<EmbeddingClient>,
    score_threshold: Option<f64>,
}

#[derive(Debug, Serialize)]
struct SearchRequest {
    vector: Vec<f32>,
    limit: usize,
    with_payload: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    score_threshold: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    result: Vec<SearchHit>,
}

#[derive(Debug, Deserialize)]
struct SearchHit {
    id: serde_json::Value,
    score: f64,
    payload: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct UpsertRequest {
    points: Vec<PointStruct>,
}

#[derive(Debug, Serialize)]
struct PointStruct {
    id: String,
    vector: Vec<f32>,
    payload: serde_json::Value,
}

impl QdrantClient {
    pub fn new(base_url: String, collection_name: String) -> Self {
        Self {
            client: Client::new(),
            base_url,
            collection_name,
            embedding: None,
            score_threshold: Some(0.3),
        }
    }

    /// Attach the embedding client used to convert query text into Qdrant vectors.
    pub fn with_embedding_client(mut self, embedding: EmbeddingClient) -> Self {
        self.embedding = Some(embedding);
        self
    }

    /// Override the minimum Qdrant similarity score. Use `None` to disable thresholding.
    pub fn with_score_threshold(mut self, score_threshold: Option<f64>) -> Self {
        self.score_threshold = score_threshold;
        self
    }

    /// Search for similar vectors.
    pub async fn search(&self, query: &str, limit: usize) -> nova_core::Result<Vec<SearchResult>> {
        if query.trim().is_empty() || limit == 0 {
            return Ok(Vec::new());
        }

        let embedding = self.embedding.as_ref().ok_or_else(|| {
            nova_core::Error::AiService(
                "Qdrant text search requires QdrantClient::with_embedding_client".to_string(),
            )
        })?;

        let vector = embedding.embed_single(query).await?;
        self.search_by_vector(vector, limit).await
    }

    /// Search Qdrant directly with a precomputed vector.
    pub async fn search_by_vector(
        &self,
        vector: Vec<f32>,
        limit: usize,
    ) -> nova_core::Result<Vec<SearchResult>> {
        if vector.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }

        let url = format!(
            "{}/collections/{}/points/search",
            self.base_url, self.collection_name
        );

        let request = SearchRequest {
            vector,
            limit,
            with_payload: true,
            score_threshold: self.score_threshold,
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| nova_core::Error::VectorDb(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = match response.text().await {
                Ok(body) => body,
                Err(_) => String::new(),
            };
            return Err(nova_core::Error::VectorDb(format!(
                "Qdrant search returned {status}: {body}"
            )));
        }

        let data: SearchResponse = response
            .json()
            .await
            .map_err(|e| nova_core::Error::VectorDb(e.to_string()))?;

        Ok(data.result.into_iter().map(search_hit_to_result).collect())
    }

    /// Upsert a vector with payload.
    pub async fn upsert(
        &self,
        id: &str,
        vector: Vec<f32>,
        payload: serde_json::Value,
    ) -> nova_core::Result<()> {
        let url = format!(
            "{}/collections/{}/points",
            self.base_url, self.collection_name
        );

        let request = UpsertRequest {
            points: vec![PointStruct {
                id: id.to_string(),
                vector,
                payload,
            }],
        };

        self.client
            .put(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| nova_core::Error::VectorDb(e.to_string()))?;

        Ok(())
    }

    /// Create collection if it doesn't exist.
    pub async fn ensure_collection(&self, vector_size: usize) -> nova_core::Result<()> {
        let url = format!("{}/collections/{}", self.base_url, self.collection_name);

        let body = serde_json::json!({
            "vectors": {
                "size": vector_size,
                "distance": "Cosine"
            }
        });

        self.client
            .put(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| nova_core::Error::VectorDb(e.to_string()))?;

        Ok(())
    }
}

fn search_hit_to_result(hit: SearchHit) -> SearchResult {
    let payload = hit.payload.unwrap_or_default();

    let book_id = payload
        .get("book_id")
        .and_then(|value| value.as_str())
        .and_then(|value| Uuid::parse_str(value).ok())
        .map(nova_core::Id::from_uuid)
        .unwrap_or_else(nova_core::Id::new);

    let chapter_id = payload
        .get("chapter_id")
        .and_then(|value| value.as_str())
        .and_then(|value| Uuid::parse_str(value).ok())
        .map(nova_core::Id::from_uuid)
        .unwrap_or_else(nova_core::Id::new);

    let chunk_id = payload
        .get("chunk_id")
        .and_then(|value| value.as_str())
        .or_else(|| hit.id.as_str())
        .and_then(|value| Uuid::parse_str(value).ok())
        .map(nova_core::Id::from_uuid);

    let content_snippet = payload
        .get("text")
        .and_then(|value| value.as_str())
        .or_else(|| payload.get("content").and_then(|value| value.as_str()))
        .unwrap_or_default()
        .to_string();

    SearchResult {
        book_id,
        chapter_id,
        chunk_id,
        book_title: payload
            .get("book_title")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string(),
        chapter_title: payload
            .get("chapter_title")
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .map(ToString::to_string),
        content_snippet,
        score: hit.score,
        source: SearchMode::Semantic,
        highlights: Vec::new(),
    }
}
