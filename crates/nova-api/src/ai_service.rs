//! AI service using reqwest for DeepSeek/OpenAI-compatible backends.
//!
//! Benefits:
//! - Lightweight (no async-openai dependency)
//! - Compatible with any OpenAI-format API (DeepSeek, LM Studio, vLLM)
//! - Type-safe request/response models
//! - Token usage tracking

use serde::{Deserialize, Serialize};
use tracing::info;

/// Unified AI service wrapping reqwest client.
pub struct AiService {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    model: String,
}

/// Configuration for the AI service.
#[derive(Debug, Clone)]
pub struct AiConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
}

/// Result from an AI batch pipeline (full-novel processing).
#[derive(Debug, Serialize, Deserialize)]
pub struct BatchPipelineResult {
    pub book_id: String,
    pub summaries: Vec<ChapterSummary>,
    pub entities: Vec<ExtractedEntity>,
    pub relationships: Vec<ExtractedRelationship>,
    pub tags: Vec<String>,
    pub style_analysis: StyleAnalysis,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChapterSummary {
    pub chapter_index: usize,
    pub title: String,
    pub summary: String,
    pub key_events: Vec<String>,
    pub entities_mentioned: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExtractedEntity {
    pub name: String,
    pub entity_type: String,
    pub description: String,
    pub aliases: Vec<String>,
    pub first_appearance: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExtractedRelationship {
    pub source: String,
    pub target: String,
    pub relation_type: String,
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StyleAnalysis {
    pub genre: String,
    pub tone: String,
    pub pov: String,
    pub writing_style: String,
    pub vocabulary_level: String,
}

impl AiService {
    /// Create a new AI service with DeepSeek/OpenAI-compatible config.
    pub fn new(config: AiConfig) -> Self {
        let client = reqwest::Client::new();
        Self {
            client,
            base_url: config.base_url,
            api_key: config.api_key,
            model: config.model,
        }
    }

    /// Simple completion (non-streaming).
    pub async fn complete(
        &self,
        system_prompt: &str,
        user_message: &str,
        temperature: f32,
        max_tokens: u32,
    ) -> Result<String, AiError> {
        let body = serde_json::json!({
            "model": self.model,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": user_message }
            ],
            "temperature": temperature,
            "max_tokens": max_tokens,
        });

        let resp = self.client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| AiError::ApiCall(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AiError::ApiCall(format!("HTTP {}: {}", status, text)));
        }

        let data: serde_json::Value = resp.json().await
            .map_err(|e| AiError::ParseResponse(e.to_string()))?;

        let content = data["choices"][0]["message"]["content"]
            .as_str()
            .ok_or(AiError::EmptyResponse)?;

        Ok(content.to_string())
    }

    /// Batch pipeline: Process an entire novel in one pass.
    /// Combines: summarize + extract entities + tag + analyze style for each chapter.
    /// Uses a sliding window approach to maintain context continuity.
    pub async fn process_full_novel(
        &self,
        book_id: &str,
        chapters: &[(String, String)], // (title, content)
    ) -> Result<BatchPipelineResult, AiError> {
        info!(book_id, chapter_count = chapters.len(), "Starting full-novel AI pipeline");

        let mut summaries = Vec::new();
        let mut all_entities: Vec<ExtractedEntity> = Vec::new();
        let mut all_relationships: Vec<ExtractedRelationship> = Vec::new();
        let mut running_context = String::new();

        // Process chapters in batches (context window aware)
        for (idx, (title, content)) in chapters.iter().enumerate() {
            // Truncate content to fit context window (keep first 6000 chars per chapter)
            let chunk = if content.len() > 6000 {
                &content[..6000]
            } else {
                content.as_str()
            };

            let system_prompt = crate::prompts::chapter_analysis_prompt(
                idx + 1,
                &running_context,
            );

            let user_msg = format!("## 第 {} 章: {}\n\n{}", idx + 1, title, chunk);

            match self.complete(&system_prompt, &user_msg, 0.2, 2048).await {
                Ok(response) => {
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&response) {
                        // Extract summary
                        let summary = parsed["summary"].as_str().unwrap_or("").to_string();
                        let key_events: Vec<String> = parsed["key_events"]
                            .as_array()
                            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                            .unwrap_or_default();

                        summaries.push(ChapterSummary {
                            chapter_index: idx,
                            title: title.clone(),
                            summary: summary.clone(),
                            key_events,
                            entities_mentioned: vec![],
                        });

                        // Extract entities
                        if let Some(entities) = parsed["entities"].as_array() {
                            for e in entities {
                                let name = e["name"].as_str().unwrap_or("").to_string();
                                if !name.is_empty() && !all_entities.iter().any(|x| x.name == name) {
                                    all_entities.push(ExtractedEntity {
                                        name,
                                        entity_type: e["type"].as_str().unwrap_or("其他").to_string(),
                                        description: e["description"].as_str().unwrap_or("").to_string(),
                                        aliases: e["aliases"].as_array()
                                            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                                            .unwrap_or_default(),
                                        first_appearance: idx,
                                    });
                                }
                            }
                        }

                        // Extract relationships
                        if let Some(rels) = parsed["relationships"].as_array() {
                            for r in rels {
                                all_relationships.push(ExtractedRelationship {
                                    source: r["source"].as_str().unwrap_or("").to_string(),
                                    target: r["target"].as_str().unwrap_or("").to_string(),
                                    relation_type: r["type"].as_str().unwrap_or("").to_string(),
                                    description: r["description"].as_str().unwrap_or("").to_string(),
                                });
                            }
                        }

                        // Update running context for continuity
                        running_context = format!(
                            "第{}章摘要: {}",
                            idx + 1,
                            summary
                        );
                        // Keep context manageable (last 3 chapters)
                        if running_context.len() > 2000 {
                            running_context = running_context[running_context.len() - 2000..].to_string();
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(chapter = idx, error = %e, "Failed to process chapter");
                }
            }
        }

        // Final pass: style analysis on first + middle + last chapters
        let style_analysis = self.analyze_book_style(chapters).await.unwrap_or(StyleAnalysis {
            genre: "未知".to_string(),
            tone: "未知".to_string(),
            pov: "未知".to_string(),
            writing_style: "未知".to_string(),
            vocabulary_level: "未知".to_string(),
        });

        // Final pass: auto-tag based on all collected data
        let tags = self.generate_tags_from_analysis(&all_entities, &summaries).await
            .unwrap_or_default();

        Ok(BatchPipelineResult {
            book_id: book_id.to_string(),
            summaries,
            entities: all_entities,
            relationships: all_relationships,
            tags,
            style_analysis,
        })
    }

    /// Analyze writing style from sample chapters.
    async fn analyze_book_style(&self, chapters: &[(String, String)]) -> Result<StyleAnalysis, AiError> {
        // Sample from beginning, middle, end
        let sample_indices: Vec<usize> = if chapters.len() <= 3 {
            (0..chapters.len()).collect()
        } else {
            vec![0, chapters.len() / 2, chapters.len() - 1]
        };

        let samples: String = sample_indices.iter()
            .map(|&i| format!("--- 第{}章片段 ---\n{}", i + 1, &chapters[i].1[..chapters[i].1.len().min(1500)]))
            .collect::<Vec<_>>()
            .join("\n\n");

        let system = crate::prompts::style_analysis_prompt();

        let response = self.complete(system, &samples, 0.2, 512).await?;
        let parsed: StyleAnalysis = serde_json::from_str(&response)
            .map_err(|e| AiError::ParseResponse(e.to_string()))?;
        Ok(parsed)
    }

    /// Generate tags from analysis results.
    async fn generate_tags_from_analysis(
        &self,
        entities: &[ExtractedEntity],
        summaries: &[ChapterSummary],
    ) -> Result<Vec<String>, AiError> {
        let entity_summary: String = entities.iter()
            .take(20)
            .map(|e| format!("{}({})", e.name, e.entity_type))
            .collect::<Vec<_>>()
            .join(", ");

        let chapter_summary: String = summaries.iter()
            .take(5)
            .map(|s| s.summary.as_str())
            .collect::<Vec<_>>()
            .join(" | ");

        let prompt = crate::prompts::tag_generation_prompt(&entity_summary, &chapter_summary);

        let response = self.complete("你是一个小说分类专家。根据以下分析信息推荐标签。", &prompt, 0.3, 256).await?;
        let tags: Vec<String> = serde_json::from_str(&response)
            .unwrap_or_default();
        Ok(tags)
    }
}

/// Errors from AI service operations.
#[derive(Debug, thiserror::Error)]
pub enum AiError {
    #[error("API call failed: {0}")]
    ApiCall(String),
    #[error("Empty response from AI")]
    EmptyResponse,
    #[error("Failed to parse response: {0}")]
    ParseResponse(String),
}
