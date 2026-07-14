use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Client for the local embedding service.
/// Connects to a local MLX-based embedding model server.
#[derive(Clone)]
pub struct EmbeddingClient {
    client: Client,
    endpoint: String,
    model: String,
    api_key: Option<String>,
    dimensions: Option<usize>,
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
}

#[derive(Debug, Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
    index: usize,
}

impl EmbeddingClient {
    pub fn new(endpoint: String, model: String) -> Self {
        let dimensions = Some(Self::detect_dimension(&model));

        Self {
            client: Client::new(),
            endpoint,
            model,
            api_key: None,
            dimensions,
        }
    }

    /// Create an embedding client with API auth and explicit vector dimensions.
    pub fn with_config(
        endpoint: String,
        model: String,
        api_key: Option<String>,
        dimensions: usize,
    ) -> Self {
        Self {
            client: Client::new(),
            endpoint,
            model,
            api_key,
            dimensions: Some(dimensions),
        }
    }

    /// Generate embedding for a single text.
    pub async fn embed_single(&self, text: &str) -> nova_core::Result<Vec<f32>> {
        let embeddings = self.embed_batch(vec![text.to_string()]).await?;
        embeddings
            .into_iter()
            .next()
            .ok_or_else(|| nova_core::Error::AiService("Empty embedding response".to_string()))
    }

    /// Generate embeddings for a batch of texts.
    pub async fn embed_batch(&self, texts: Vec<String>) -> nova_core::Result<Vec<Vec<f32>>> {
        let request = EmbeddingRequest {
            input: texts,
            model: self.model.clone(),
            dimensions: self.dimensions,
        };

        let mut request = self
            .client
            .post(&format!("{}/v1/embeddings", self.endpoint))
            .header("X-Failover-Enabled", "true")
            .json(&request);

        if let Some(api_key) = self.api_key.as_deref() {
            request = request.header("Authorization", format!("Bearer {api_key}"));
        }

        let response = request
            .send()
            .await
            .map_err(|e| nova_core::Error::AiService(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = match response.text().await {
                Ok(body) => body,
                Err(_) => String::new(),
            };
            return Err(nova_core::Error::AiService(format!(
                "Embedding service returned {}: {}",
                status, body
            )));
        }

        let resp: EmbeddingResponse = response
            .json()
            .await
            .map_err(|e| nova_core::Error::AiService(e.to_string()))?;

        let mut data = resp.data;
        data.sort_by_key(|d| d.index);

        Ok(data.into_iter().map(|d| d.embedding).collect())
    }

    /// Get the dimensionality of the embedding model.
    pub fn dimension(&self) -> usize {
        self.dimensions
            .unwrap_or_else(|| Self::detect_dimension(&self.model))
    }

    fn detect_dimension(model: &str) -> usize {
        let model = model.to_lowercase();
        if model.contains("qwen3") && (model.contains("4b") || model.contains("8b")) {
            2560
        } else if model.contains("qwen3") {
            1024
        } else if model.contains("nomic") {
            768
        } else {
            1024
        }
    }
}
