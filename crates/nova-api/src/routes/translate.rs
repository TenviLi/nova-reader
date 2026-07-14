use axum::{extract::State, routing::post, Json, Router};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    access::{ensure_book_access, LibraryAccess},
    error::{ApiError, ApiResult},
    extractors::AuthUser,
    state::AppState,
};

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/translate", post(translate))
        .route("/translate/batch", post(translate_batch))
        .route("/translate/detect-language", post(detect_language))
}

#[derive(Deserialize)]
struct TranslateRequest {
    text: String,
    source_language: String,
    target_language: String,
    book_id: Option<String>,
    use_glossary: Option<bool>,
}

#[derive(Serialize)]
struct TranslateResponse {
    translated_text: String,
    glossary_applied: Vec<GlossaryMatch>,
    confidence: f64,
}

#[derive(Serialize, Clone)]
struct GlossaryMatch {
    source_term: String,
    target_term: String,
    category: String,
}

fn parse_optional_book_id(book_id: Option<&str>) -> ApiResult<Option<Uuid>> {
    book_id
        .map(|value| {
            Uuid::parse_str(value)
                .map_err(|_| ApiError::bad_request("book_id must be a valid UUID"))
        })
        .transpose()
}

async fn matched_glossary(
    state: &AppState,
    book_id: Option<Uuid>,
    text: &str,
) -> Result<(String, Vec<GlossaryMatch>), ApiError> {
    let entries = if let Some(book_id) = book_id {
        sqlx::query_as::<_, (String, String, String)>(
            r#"
            SELECT source_term, target_term, COALESCE(category, '通用')
            FROM glossary_entries
            WHERE book_id = $1 OR is_global = true OR book_id IS NULL
            LIMIT 100
            "#,
        )
        .bind(book_id)
        .fetch_all(&state.db)
        .await
        .map_err(ApiError::from)?
    } else {
        sqlx::query_as::<_, (String, String, String)>(
            r#"
            SELECT source_term, target_term, COALESCE(category, '通用')
            FROM glossary_entries
            WHERE is_global = true OR book_id IS NULL
            LIMIT 100
            "#,
        )
        .fetch_all(&state.db)
        .await
        .map_err(ApiError::from)?
    };

    let mut glossary_context = String::new();
    let mut matched_terms = Vec::new();
    for (source, target, category) in entries {
        if text.contains(source.as_str()) {
            if glossary_context.is_empty() {
                glossary_context = "## 术语表 (必须严格使用以下翻译)\n".to_string();
            }
            glossary_context.push_str(&format!("- {} → {} ({})\n", source, target, category));
            matched_terms.push(GlossaryMatch {
                source_term: source,
                target_term: target,
                category,
            });
        }
    }

    Ok((glossary_context, matched_terms))
}

async fn translate(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<TranslateRequest>,
) -> Result<Json<TranslateResponse>, ApiError> {
    if body.text.is_empty() {
        return Err(ApiError::bad_request("text cannot be empty"));
    }
    if body.text.len() > 50_000 {
        return Err(ApiError::bad_request(
            "text exceeds 50k character limit for single translation",
        ));
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

    let book_id = parse_optional_book_id(body.book_id.as_deref())?;
    if let Some(book_id) = book_id {
        ensure_book_access(&state, &auth, book_id, LibraryAccess::Read).await?;
    }

    // 1. Load glossary entries for this book if requested
    let (glossary_context, matched_terms) = if body.use_glossary.unwrap_or(true) {
        matched_glossary(&state, book_id, &body.text).await?
    } else {
        (String::new(), Vec::new())
    };

    // 2. Build translation prompt using CREATE framework
    let system_prompt = crate::prompts::translation_prompt(
        &body.source_language,
        &body.target_language,
        &glossary_context,
    );

    // 3. Call DeepSeek
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
            "temperature": 0.3,
            "max_tokens": 4096,
        }))
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("DeepSeek error: {}", e)))?;

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ApiError::Internal(format!("Parse error: {}", e)))?;

    let translated = data["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_string();

    Ok(Json(TranslateResponse {
        translated_text: translated,
        glossary_applied: matched_terms,
        confidence: 0.95,
    }))
}

#[derive(Deserialize)]
struct BatchTranslateRequest {
    segments: Vec<String>,
    source_language: String,
    target_language: String,
    book_id: Option<String>,
    use_glossary: Option<bool>,
}

#[derive(Serialize)]
struct BatchTranslateResponse {
    translations: Vec<String>,
    glossary_applied: Vec<GlossaryMatch>,
}

async fn translate_batch(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<BatchTranslateRequest>,
) -> Result<Json<BatchTranslateResponse>, ApiError> {
    // Input validation: cap number and total size of segments
    if body.segments.is_empty() {
        return Err(ApiError::bad_request("segments must not be empty"));
    }
    if body.segments.len() > 100 {
        return Err(ApiError::bad_request("maximum 100 segments per batch"));
    }
    let total_chars: usize = body.segments.iter().map(|s| s.len()).sum();
    if total_chars > 50_000 {
        return Err(ApiError::bad_request(
            "total segment content exceeds 50k character limit",
        ));
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
    let book_id = parse_optional_book_id(body.book_id.as_deref())?;
    if let Some(book_id) = book_id {
        ensure_book_access(&state, &auth, book_id, LibraryAccess::Read).await?;
    }
    // Join segments with separator for batch translation
    let joined = body.segments.join("\n---SEG---\n");

    let mut system_prompt =
        crate::prompts::batch_translate_prompt(&body.source_language, &body.target_language);
    let (_, matched_terms) = if body.use_glossary.unwrap_or(true) {
        let (context, matches) = matched_glossary(&state, book_id, &joined).await?;
        if !context.is_empty() {
            system_prompt.push_str("\n\n");
            system_prompt.push_str(&context);
        }
        (context, matches)
    } else {
        (String::new(), Vec::new())
    };

    let client = &state.http_client;
    let resp = client
        .post(format!("{}/chat/completions", base_url))
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&serde_json::json!({
            "model": model,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": joined}
            ],
            "temperature": 0.3,
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
        .unwrap_or("")
        .to_string();

    let translations: Vec<String> = content
        .split("---SEG---")
        .map(|s| s.trim().to_string())
        .collect();

    Ok(Json(BatchTranslateResponse {
        translations,
        glossary_applied: matched_terms,
    }))
}

#[derive(Deserialize)]
struct DetectLanguageRequest {
    text: String,
}

#[derive(Serialize)]
struct DetectLanguageResponse {
    language: String,
    confidence: f64,
}

async fn detect_language(
    Json(body): Json<DetectLanguageRequest>,
) -> Result<Json<DetectLanguageResponse>, ApiError> {
    // Simple CJK detection heuristic
    let cjk_count = body
        .text
        .chars()
        .filter(|c| matches!(*c as u32, 0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0x20000..=0x2A6DF))
        .count();
    let total = body.text.chars().count().max(1);
    let cjk_ratio = cjk_count as f64 / total as f64;

    let (language, confidence) = if cjk_ratio > 0.3 {
        ("zh", cjk_ratio)
    } else {
        ("en", 1.0 - cjk_ratio)
    };

    Ok(Json(DetectLanguageResponse {
        language: language.to_string(),
        confidence,
    }))
}
