use std::sync::Arc;

use axum::{extract::State, routing::get, Json, Router};
use serde_json::Value;

use crate::access::auth_user_id;
use crate::error::{ApiError, ApiResult};
use crate::extractors::AuthUser;
use crate::state::AppState;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new().route("/settings", get(get_settings).put(update_settings))
}

const DEFAULT_SETTINGS: &str = r#"{
    "general": {
        "language": "zh",
        "theme": "dark",
        "autoScan": true,
        "scanInterval": 300
    },
    "libraries": { "paths": [] },
    "reader": {
        "fontSize": 18,
        "lineHeight": 1.8,
        "fontFamily": "system-ui",
        "paragraphSpacing": 1.5,
        "maxWidth": 720,
        "theme": "dark"
    },
    "ai": {
        "provider": "deepseek",
        "apiKey": "",
        "baseUrl": "https://api.deepseek.com/v1",
        "model": "deepseek-chat",
        "temperature": 0.3,
        "maxTokens": 4096
    },
    "embedding": {
        "provider": "local",
        "model": "qwen3-embedding",
        "dimensions": 1024,
        "batchSize": 32,
        "localEndpoint": "http://localhost:11434"
    },
    "translate": {
        "provider": "deepseek",
        "targetLanguage": "en",
        "useGlossary": true
    },
    "tasks": {
        "maxConcurrency": 4,
        "retryAttempts": 3,
        "retryDelay": 5000
    }
}"#;

fn default_settings() -> ApiResult<Value> {
    serde_json::from_str(DEFAULT_SETTINGS)
        .map_err(|err| ApiError::internal(format!("Invalid default settings JSON: {err}")))
}

async fn get_settings(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<Json<Value>> {
    let user_id = auth_user_id(&auth)?;
    let row: Option<(Value,)> = sqlx::query_as("SELECT data FROM user_settings WHERE user_id = $1")
        .bind(user_id)
        .fetch_optional(&state.db)
        .await
        .map_err(ApiError::from)?;

    match row {
        Some((data,)) => Ok(Json(data)),
        None => Ok(Json(default_settings()?)),
    }
}

async fn update_settings(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(data): Json<Value>,
) -> ApiResult<Json<Value>> {
    let user_id = auth_user_id(&auth)?;
    sqlx::query(
        r#"INSERT INTO user_settings (user_id, data, updated_at) VALUES ($1, $2, NOW())
           ON CONFLICT (user_id) DO UPDATE SET data = $2, updated_at = NOW()"#,
    )
    .bind(user_id)
    .bind(&data)
    .execute(&state.db)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(serde_json::json!({ "status": "ok" })))
}

#[cfg(test)]
mod tests {
    fn production_source() -> &'static str {
        include_str!("settings.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("production source should exist")
    }

    #[test]
    fn settings_are_scoped_to_authenticated_user() {
        let source = production_source();

        assert!(source.contains("extractors::AuthUser"));
        assert!(source.contains("auth_user_id(&auth)?"));
        assert!(source.contains("WHERE user_id = $1"));
        assert!(!source.contains("00000000-0000-0000-0000-000000000000"));
    }

    #[test]
    fn default_settings_parse_errors_are_returned() {
        let source = production_source();

        assert!(source.contains("default_settings()?"));
        assert!(!source.contains("serde_json::from_str(DEFAULT_SETTINGS).unwrap()"));
    }
}
