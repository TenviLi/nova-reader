use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Embedding generation service.
/// Supports OpenAI-compatible API format (Gitee AI, local MLX, etc.)
pub struct EmbeddingService {
    client: Client,
    endpoint: String,
    model: String,
    api_key: Option<String>,
    batch_size: usize,
    dimension: usize,
}

#[derive(Debug, Serialize)]
struct EmbeddingRequest {
    input: Vec<String>,
    model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    dimensions: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
    usage: Option<UsageInfo>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
    index: usize,
}

#[derive(Debug, Deserialize)]
struct UsageInfo {
    total_tokens: Option<u64>,
}

impl EmbeddingService {
    pub fn new(endpoint: String, model: String) -> Self {
        // Auto-detect dimension based on model name
        let dimension = Self::detect_dimension(&model);

        Self {
            client: Client::new(),
            endpoint,
            model,
            api_key: None,
            batch_size: 16,
            dimension,
        }
    }

    /// Create with explicit API key and dimension.
    pub fn with_config(endpoint: String, model: String, api_key: Option<String>, dimension: usize) -> Self {
        Self {
            client: Client::new(),
            endpoint,
            model,
            api_key,
            batch_size: 16,
            dimension,
        }
    }

    fn detect_dimension(model: &str) -> usize {
        let m = model.to_lowercase();
        if m.contains("nomic") {
            768
        } else if m.contains("qwen3") && (m.contains("4b") || m.contains("8b")) {
            2560
        } else if m.contains("qwen3") {
            1024
        } else {
            768
        }
    }

    /// Get the dimensionality of the embedding model.
    pub fn dimension(&self) -> usize {
        self.dimension
    }

    /// Generate embedding for a single text.
    pub async fn embed_one(&self, text: &str) -> nova_core::Result<Vec<f32>> {
        let results = self.embed_many(&[text.to_string()]).await?;
        results
            .into_iter()
            .next()
            .ok_or_else(|| nova_core::Error::AiService("Empty embedding response".into()))
    }

    /// Generate embeddings for multiple texts (batched).
    pub async fn embed_many(&self, texts: &[String]) -> nova_core::Result<Vec<Vec<f32>>> {
        let mut all_embeddings = Vec::with_capacity(texts.len());

        // Process in batches to avoid overwhelming the embedding server
        for chunk in texts.chunks(self.batch_size) {
            let request = EmbeddingRequest {
                input: chunk.to_vec(),
                model: self.model.clone(),
                dimensions: Some(self.dimension),
            };

            let mut req_builder = self
                .client
                .post(format!("{}/v1/embeddings", self.endpoint))
                .header("X-Failover-Enabled", "true")
                .json(&request);

            if let Some(ref key) = self.api_key {
                req_builder = req_builder.header("Authorization", format!("Bearer {}", key));
            }

            let response = req_builder
                .send()
                .await
                .map_err(|e| nova_core::Error::AiService(format!("Embedding request failed: {}", e)))?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                return Err(nova_core::Error::AiService(format!(
                    "Embedding service error ({}): {}",
                    status, body
                )));
            }

            let resp: EmbeddingResponse = response
                .json()
                .await
                .map_err(|e| nova_core::Error::AiService(format!("Failed to parse response: {}", e)))?;

            // Sort by index to maintain order
            let mut data = resp.data;
            data.sort_by_key(|d| d.index);

            all_embeddings.extend(data.into_iter().map(|d| d.embedding));

            if let Some(usage) = resp.usage {
                if let Some(tokens) = usage.total_tokens {
                    tracing::debug!(tokens, "Embedding batch processed");
                }
            }
        }

        Ok(all_embeddings)
    }

    /// Compute cosine similarity between two vectors.
    pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        assert_eq!(a.len(), b.len(), "Vector dimensions must match");

        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

        if norm_a == 0.0 || norm_b == 0.0 {
            return 0.0;
        }

        dot / (norm_a * norm_b)
    }
}
