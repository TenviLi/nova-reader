use nova_core::domain::search::{HighlightSpan, SearchMode, SearchResult};
use reqwest::Client;
use serde::Deserialize;
use uuid::Uuid;

/// Client for Meilisearch full-text search.
pub struct MeilisearchClient {
    client: Client,
    base_url: String,
    api_key: String,
}

#[derive(Debug, Deserialize)]
struct MeiliSearchResponse {
    hits: Vec<MeiliHit>,
    #[serde(rename = "estimatedTotalHits")]
    estimated_total_hits: Option<usize>,
    #[serde(rename = "processingTimeMs")]
    processing_time_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct MeiliHit {
    id: Option<String>,
    book_id: Option<String>,
    book_title: Option<String>,
    chapter_title: Option<String>,
    #[allow(dead_code)]
    // Kept for Meilisearch payload parity; SearchResult does not expose chapter_index yet.
    chapter_index: Option<i32>,
    content: Option<String>,
    #[serde(rename = "_rankingScore")]
    ranking_score: Option<f64>,
    #[serde(rename = "_formatted")]
    formatted: Option<serde_json::Value>,
}

impl MeilisearchClient {
    pub fn new(base_url: String, api_key: String) -> Self {
        Self {
            client: Client::new(),
            base_url,
            api_key,
        }
    }

    /// Search for documents matching the query.
    pub async fn search(&self, query: &str, limit: usize) -> nova_core::Result<Vec<SearchResult>> {
        let url = format!("{}/indexes/chunks/search", self.base_url);

        let body = serde_json::json!({
            "q": query,
            "limit": limit,
            "showRankingScore": true,
            "attributesToHighlight": ["content"],
            "highlightPreTag": "<mark>",
            "highlightPostTag": "</mark>",
            "attributesToCrop": ["content"],
            "cropLength": 200,
        });

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| nova_core::Error::Internal(format!("Meilisearch request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = match resp.text().await {
                Ok(text) => text,
                Err(_) => String::new(),
            };
            return Err(nova_core::Error::Internal(format!(
                "Meilisearch returned {status}: {text}"
            )));
        }

        let data: MeiliSearchResponse = resp.json().await.map_err(|e| {
            nova_core::Error::Internal(format!("Failed to parse Meilisearch response: {e}"))
        })?;

        tracing::debug!(
            hit_count = data.hits.len(),
            estimated_total_hits = ?data.estimated_total_hits,
            processing_time_ms = ?data.processing_time_ms,
            "Meilisearch search completed"
        );

        let results = data
            .hits
            .into_iter()
            .map(|hit| {
                let (content_snippet, highlights) =
                    content_snippet_and_highlights(hit.formatted.as_ref(), hit.content.as_deref());

                SearchResult {
                    book_id: hit
                        .book_id
                        .and_then(|s| Uuid::parse_str(&s).ok())
                        .map(nova_core::Id::from_uuid)
                        .unwrap_or_else(nova_core::Id::new),
                    chapter_id: hit
                        .id
                        .and_then(|s| Uuid::parse_str(&s).ok())
                        .map(nova_core::Id::from_uuid)
                        .unwrap_or_else(nova_core::Id::new),
                    chunk_id: None,
                    book_title: hit.book_title.unwrap_or_default(),
                    chapter_title: hit.chapter_title,
                    content_snippet,
                    score: hit.ranking_score.unwrap_or(0.0),
                    source: SearchMode::Keyword,
                    highlights,
                }
            })
            .collect();

        Ok(results)
    }

    /// Index a document for full-text search.
    pub async fn index_document(
        &self,
        index: &str,
        doc: serde_json::Value,
    ) -> nova_core::Result<()> {
        let url = format!("{}/indexes/{}/documents", self.base_url, index);

        self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&[doc])
            .send()
            .await
            .map_err(|e| nova_core::Error::Internal(e.to_string()))?;

        Ok(())
    }

    /// Batch index multiple documents.
    pub async fn index_documents(
        &self,
        index: &str,
        docs: Vec<serde_json::Value>,
    ) -> nova_core::Result<()> {
        let url = format!("{}/indexes/{}/documents", self.base_url, index);

        self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&docs)
            .send()
            .await
            .map_err(|e| nova_core::Error::Internal(e.to_string()))?;

        Ok(())
    }
}

fn content_snippet_and_highlights(
    formatted: Option<&serde_json::Value>,
    fallback: Option<&str>,
) -> (String, Vec<HighlightSpan>) {
    if let Some(formatted_content) = formatted
        .and_then(|value| value.get("content"))
        .and_then(|value| value.as_str())
    {
        let (snippet, highlights) = strip_mark_tags(formatted_content);
        if !snippet.is_empty() {
            return (snippet, highlights);
        }
    }

    (fallback.unwrap_or_default().to_string(), Vec::new())
}

fn strip_mark_tags(input: &str) -> (String, Vec<HighlightSpan>) {
    const OPEN: &str = "<mark>";
    const CLOSE: &str = "</mark>";

    let mut output = String::with_capacity(input.len());
    let mut highlights = Vec::new();
    let mut active_start = None;
    let mut index = 0;

    while index < input.len() {
        let rest = &input[index..];
        if rest.starts_with(OPEN) {
            active_start.get_or_insert(output.len());
            index += OPEN.len();
        } else if rest.starts_with(CLOSE) {
            if let Some(start) = active_start.take() {
                let end = output.len();
                if end > start {
                    highlights.push(HighlightSpan { start, end });
                }
            }
            index += CLOSE.len();
        } else if let Some(ch) = rest.chars().next() {
            output.push(ch);
            index += ch.len_utf8();
        } else {
            break;
        }
    }

    if let Some(start) = active_start {
        let end = output.len();
        if end > start {
            highlights.push(HighlightSpan { start, end });
        }
    }

    (output, highlights)
}
