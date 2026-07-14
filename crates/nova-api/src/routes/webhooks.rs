use axum::{
    extract::{Path, State},
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::{error::ApiResult, state::AppState};

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(list_webhooks).post(create_webhook))
        .route("/{id}", get(get_webhook).put(update_webhook).delete(delete_webhook))
        .route("/{id}/test", post(test_webhook))
        .route("/{id}/deliveries", get(list_deliveries))
}

#[derive(Debug, Serialize)]
struct Webhook {
    id: Uuid,
    name: String,
    event: String,
    target_type: String,
    target_url: Option<String>,
    internal_action: Option<String>,
    enabled: bool,
    last_triggered_at: Option<String>,
    total_triggers: i32,
    total_failures: i32,
    created_at: String,
}

#[derive(Debug, Deserialize)]
struct CreateWebhookRequest {
    name: String,
    event: String,
    target_type: String,
    target_url: Option<String>,
    internal_action: Option<String>,
    config: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct UpdateWebhookRequest {
    name: Option<String>,
    event: Option<String>,
    target_url: Option<String>,
    config: Option<serde_json::Value>,
    enabled: Option<bool>,
}

/// Supported webhook events.
pub const SUPPORTED_EVENTS: &[&str] = &[
    "book.created",
    "book.processed",
    "book.deleted",
    "chapter.translated",
    "entity.extracted",
    "library.scanned",
    "task.completed",
    "task.failed",
];

/// Internal actions that can be triggered by webhooks.
pub const INTERNAL_ACTIONS: &[&str] = &[
    "ai_pipeline",      // Run full AI pipeline (summarize + entities + tags)
    "translate_all",    // Translate all chapters
    "embed_chunks",     // Generate embeddings for all chunks
    "notify_telegram",  // Send Telegram notification
    "notify_discord",   // Send Discord notification
];

async fn list_webhooks(
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<Vec<Webhook>>> {
    let rows = sqlx::query_as!(
        Webhook,
        r#"SELECT id, name, event, target_type, target_url, internal_action,
           enabled, last_triggered_at::text, total_triggers, total_failures,
           created_at::text as "created_at!"
           FROM webhooks ORDER BY created_at DESC"#
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(rows))
}

async fn create_webhook(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateWebhookRequest>,
) -> ApiResult<Json<Webhook>> {
    // Validate event type
    if !SUPPORTED_EVENTS.contains(&body.event.as_str()) {
        return Err(nova_core::Error::Validation(
            format!("Unsupported event: {}. Supported: {:?}", body.event, SUPPORTED_EVENTS)
        ).into());
    }

    // Validate target
    match body.target_type.as_str() {
        "url" => {
            if body.target_url.is_none() {
                return Err(nova_core::Error::Validation("target_url required for url type".into()).into());
            }
            // Basic URL validation
            let url = body.target_url.as_ref().unwrap();
            if !url.starts_with("http://") && !url.starts_with("https://") {
                return Err(nova_core::Error::Validation("target_url must start with http:// or https://".into()).into());
            }
        }
        "internal" => {
            if let Some(ref action) = body.internal_action {
                if !INTERNAL_ACTIONS.contains(&action.as_str()) {
                    return Err(nova_core::Error::Validation(
                        format!("Unsupported action: {}. Supported: {:?}", action, INTERNAL_ACTIONS)
                    ).into());
                }
            } else {
                return Err(nova_core::Error::Validation("internal_action required for internal type".into()).into());
            }
        }
        _ => {
            return Err(nova_core::Error::Validation("target_type must be 'url' or 'internal'".into()).into());
        }
    }

    let id = Uuid::now_v7();
    let config = body.config.unwrap_or(serde_json::json!({}));

    sqlx::query(
        "INSERT INTO webhooks (id, user_id, name, event, target_type, target_url, internal_action, config)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"
    )
    .bind(id)
    .bind(Uuid::nil()) // TODO: extract from auth context
    .bind(&body.name)
    .bind(&body.event)
    .bind(&body.target_type)
    .bind(&body.target_url)
    .bind(&body.internal_action)
    .bind(&config)
    .execute(&state.db)
    .await?;

    Ok(Json(Webhook {
        id,
        name: body.name,
        event: body.event,
        target_type: body.target_type,
        target_url: body.target_url,
        internal_action: body.internal_action,
        enabled: true,
        last_triggered_at: None,
        total_triggers: 0,
        total_failures: 0,
        created_at: chrono::Utc::now().to_rfc3339(),
    }))
}

async fn get_webhook(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let row = sqlx::query_as!(
        Webhook,
        r#"SELECT id, name, event, target_type, target_url, internal_action,
           enabled, last_triggered_at::text, total_triggers, total_failures,
           created_at::text as "created_at!"
           FROM webhooks WHERE id = $1"#,
        id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(nova_core::Error::NotFound { entity: "webhook", id: id.to_string() })?;

    Ok(Json(serde_json::to_value(row).unwrap_or_default()))
}

async fn update_webhook(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateWebhookRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    sqlx::query(
        "UPDATE webhooks SET
         name = COALESCE($2, name),
         event = COALESCE($3, event),
         target_url = COALESCE($4, target_url),
         config = COALESCE($5, config),
         enabled = COALESCE($6, enabled),
         updated_at = now()
         WHERE id = $1"
    )
    .bind(id)
    .bind(&body.name)
    .bind(&body.event)
    .bind(&body.target_url)
    .bind(&body.config)
    .bind(body.enabled)
    .execute(&state.db)
    .await?;

    Ok(Json(serde_json::json!({ "updated": true })))
}

async fn delete_webhook(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    sqlx::query("DELETE FROM webhooks WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;

    Ok(Json(serde_json::json!({ "deleted": true })))
}

/// Send a test payload to verify the webhook configuration.
async fn test_webhook(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let webhook = sqlx::query_as!(
        Webhook,
        r#"SELECT id, name, event, target_type, target_url, internal_action,
           enabled, last_triggered_at::text, total_triggers, total_failures,
           created_at::text as "created_at!"
           FROM webhooks WHERE id = $1"#,
        id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(nova_core::Error::NotFound { entity: "webhook", id: id.to_string() })?;

    let test_payload = serde_json::json!({
        "event": webhook.event,
        "test": true,
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "data": { "message": "This is a test delivery" },
    });

    if webhook.target_type == "url" {
        if let Some(ref url) = webhook.target_url {
            let resp = state.http_client
                .post(url)
                .json(&test_payload)
                .timeout(std::time::Duration::from_secs(10))
                .send()
                .await;

            match resp {
                Ok(r) => {
                    let status = r.status().as_u16();
                    return Ok(Json(serde_json::json!({
                        "success": r.status().is_success(),
                        "status_code": status,
                    })));
                }
                Err(e) => {
                    return Ok(Json(serde_json::json!({
                        "success": false,
                        "error": e.to_string(),
                    })));
                }
            }
        }
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Internal action would be triggered",
    })))
}

async fn list_deliveries(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<serde_json::Value>>> {
    let rows: Vec<serde_json::Value> = sqlx::query_scalar(
        "SELECT json_build_object(
            'id', id, 'event', event, 'status_code', status_code,
            'error', error, 'delivered_at', delivered_at::text
         ) FROM webhook_deliveries WHERE webhook_id = $1
         ORDER BY delivered_at DESC LIMIT 50"
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(rows))
}

/// Dispatch an event to all matching enabled webhooks.
/// Call this from any handler that triggers a lifecycle event.
pub async fn dispatch_event(state: &AppState, event: &str, payload: serde_json::Value) {
    let webhooks: Vec<(Uuid, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT id, target_type, target_url, internal_action FROM webhooks
         WHERE event = $1 AND enabled = true"
    )
    .bind(event)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    for (webhook_id, target_type, target_url, _internal_action) in webhooks {
        let state_http = state.http_client.clone();
        let db = state.db.clone();
        let event_name = event.to_string();
        let payload_clone = payload.clone();

        // Fire-and-forget delivery
        tokio::spawn(async move {
            let (status_code, error) = if target_type == "url" {
                if let Some(url) = target_url {
                    match state_http
                        .post(&url)
                        .json(&payload_clone)
                        .timeout(std::time::Duration::from_secs(30))
                        .send()
                        .await
                    {
                        Ok(r) => (Some(r.status().as_u16() as i32), None),
                        Err(e) => (None, Some(e.to_string())),
                    }
                } else {
                    (None, Some("No target URL configured".to_string()))
                }
            } else {
                // Internal actions would be handled here
                (None, None)
            };

            // Log delivery
            let _ = sqlx::query(
                "INSERT INTO webhook_deliveries (id, webhook_id, event, payload, status_code, error)
                 VALUES (gen_random_uuid(), $1, $2, $3, $4, $5)"
            )
            .bind(webhook_id)
            .bind(&event_name)
            .bind(&payload_clone)
            .bind(status_code)
            .bind(&error)
            .execute(&db)
            .await;

            // Update webhook stats
            let _ = sqlx::query(
                "UPDATE webhooks SET total_triggers = total_triggers + 1,
                 total_failures = total_failures + CASE WHEN $2 IS NOT NULL THEN 1 ELSE 0 END,
                 last_triggered_at = now()
                 WHERE id = $1"
            )
            .bind(webhook_id)
            .bind(&error)
            .execute(&db)
            .await;
        });
    }
}
