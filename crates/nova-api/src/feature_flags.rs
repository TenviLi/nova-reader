use serde::{Deserialize, Serialize};
use sqlx::PgPool;

/// Feature flags that can be toggled from the admin AI settings panel.
/// Stored in the `user_settings` table as part of the settings JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureFlags {
    #[serde(default = "default_true")]
    pub ai_chat: bool,
    #[serde(default = "default_true")]
    pub ai_entities: bool,
    #[serde(default = "default_true")]
    pub ai_summarize: bool,
    #[serde(default = "default_true")]
    pub ai_translate: bool,
    #[serde(default = "default_true")]
    pub ai_style_analysis: bool,
    #[serde(default = "default_true")]
    pub ai_batch_process: bool,
    #[serde(default = "default_true")]
    pub semantic_search: bool,
    #[serde(default = "default_true")]
    pub knowledge_graph: bool,
    #[serde(default = "default_true")]
    pub reranker: bool,
}

fn default_true() -> bool {
    true
}

impl Default for FeatureFlags {
    fn default() -> Self {
        Self {
            ai_chat: true,
            ai_entities: true,
            ai_summarize: true,
            ai_translate: true,
            ai_style_analysis: true,
            ai_batch_process: true,
            semantic_search: true,
            knowledge_graph: true,
            reranker: true,
        }
    }
}

/// Load feature flags from the global settings in DB.
/// Falls back to all-enabled if settings are missing.
pub async fn load_feature_flags(db: &PgPool) -> FeatureFlags {
    let row: Option<(serde_json::Value,)> = sqlx::query_as(
        "SELECT data FROM user_settings WHERE user_id = '00000000-0000-0000-0000-000000000000'",
    )
    .fetch_optional(db)
    .await
    .unwrap_or(None);

    match row {
        Some((data,)) => {
            // Extract features from settings.ai.features or settings.features
            if let Some(features) = data.get("ai").and_then(|ai| ai.get("features")) {
                serde_json::from_value(features.clone()).unwrap_or_default()
            } else if let Some(features) = data.get("features") {
                serde_json::from_value(features.clone()).unwrap_or_default()
            } else {
                FeatureFlags::default()
            }
        }
        None => FeatureFlags::default(),
    }
}

/// Check if a specific feature is enabled. Returns an error if disabled.
pub async fn require_feature(db: &PgPool, feature: &str) -> Result<(), crate::error::ApiError> {
    let flags = load_feature_flags(db).await;
    let enabled = match feature {
        "ai_chat" => flags.ai_chat,
        "ai_entities" => flags.ai_entities,
        "ai_summarize" => flags.ai_summarize,
        "ai_translate" => flags.ai_translate,
        "ai_style_analysis" => flags.ai_style_analysis,
        "ai_batch_process" => flags.ai_batch_process,
        "semantic_search" => flags.semantic_search,
        "knowledge_graph" => flags.knowledge_graph,
        "reranker" => flags.reranker,
        _ => true,
    };

    if enabled {
        Ok(())
    } else {
        Err(crate::error::ApiError::bad_request(&format!(
            "Feature '{}' is currently disabled by administrator",
            feature
        )))
    }
}
