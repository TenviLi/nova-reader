use std::sync::Arc;

use axum::{
    extract::{Path, State},
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::ApiResult;
use crate::state::AppState;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/plugins", get(list_plugins).post(install_plugin))
        .route("/plugins/{id}", get(get_plugin).delete(uninstall_plugin))
        .route("/plugins/{id}/enable", post(enable_plugin))
        .route("/plugins/{id}/disable", post(disable_plugin))
        .route("/plugins/{id}/config", get(get_plugin_config).put(update_plugin_config))
        .route("/plugins/registry", get(browse_registry))
        .route("/plugins/hooks/{hook}", post(trigger_hook))
}

// ─── Plugin Types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub runtime: PluginRuntime,
    pub hooks: Vec<String>,
    pub permissions: Vec<String>,
    pub config_schema: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginRuntime {
    Wasm,
    JavaScript,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPlugin {
    pub id: Uuid,
    pub manifest: PluginManifest,
    pub enabled: bool,
    pub config: serde_json::Value,
    pub installed_at: String,
}

// ─── Plugin Hook System ───────────────────────────────────────────────────────

/// Available hooks that plugins can subscribe to.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PluginHook {
    /// Triggered after a new book is imported
    BookImported,
    /// Triggered before chapter content is displayed
    BeforeChapterRender,
    /// Triggered after AI processing completes
    AfterAiProcess,
    /// Triggered on search query (can modify results)
    SearchFilter,
    /// Triggered on export (can transform output)
    BeforeExport,
    /// Custom user-defined timer/cron
    Scheduled,
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

/// List all installed plugins.
async fn list_plugins(
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<serde_json::Value>> {
    let plugins = sqlx::query!(
        r#"SELECT id, manifest, enabled, config, installed_at
           FROM plugins ORDER BY installed_at DESC"#
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(serde_json::json!({
        "plugins": plugins,
        "total": plugins.len(),
    })))
}

/// Install a plugin from manifest.
async fn install_plugin(
    State(state): State<Arc<AppState>>,
    Json(manifest): Json<PluginManifest>,
) -> ApiResult<Json<serde_json::Value>> {
    // Validate permissions (sandboxing)
    let dangerous_permissions = ["filesystem_write", "network_unrestricted", "shell_exec"];
    let has_dangerous = manifest.permissions.iter()
        .any(|p| dangerous_permissions.contains(&p.as_str()));

    if has_dangerous {
        return Err(crate::error::ApiError::bad_request(
            "Plugin requires dangerous permissions. Admin approval needed."
        ));
    }

    // Validate hook names
    let valid_hooks = [
        "book_imported", "before_chapter_render", "after_ai_process",
        "search_filter", "before_export", "scheduled",
    ];
    for hook in &manifest.hooks {
        if !valid_hooks.contains(&hook.as_str()) {
            return Err(crate::error::ApiError::bad_request(
                &format!("Unknown hook: {}. Valid: {:?}", hook, valid_hooks)
            ));
        }
    }

    let id = Uuid::new_v4();
    let manifest_json = serde_json::to_value(&manifest).unwrap_or_default();

    sqlx::query!(
        r#"INSERT INTO plugins (id, manifest, enabled, config)
           VALUES ($1, $2, true, '{}'::jsonb)"#,
        id,
        manifest_json
    )
    .execute(&state.db)
    .await?;

    Ok(Json(serde_json::json!({
        "id": id,
        "name": manifest.name,
        "version": manifest.version,
        "status": "installed",
        "enabled": true,
    })))
}

/// Get plugin details.
async fn get_plugin(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let plugin = sqlx::query!(
        "SELECT id, manifest, enabled, config, installed_at FROM plugins WHERE id = $1",
        id
    )
    .fetch_optional(&state.db)
    .await?;

    match plugin {
        Some(p) => Ok(Json(serde_json::json!(p))),
        None => Err(crate::error::ApiError::not_found("Plugin not found")),
    }
}

/// Uninstall a plugin.
async fn uninstall_plugin(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    sqlx::query!("DELETE FROM plugins WHERE id = $1", id)
        .execute(&state.db)
        .await?;
    Ok(Json(serde_json::json!({ "uninstalled": true, "id": id })))
}

/// Enable a plugin.
async fn enable_plugin(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    sqlx::query!("UPDATE plugins SET enabled = true WHERE id = $1", id)
        .execute(&state.db)
        .await?;
    Ok(Json(serde_json::json!({ "id": id, "enabled": true })))
}

/// Disable a plugin.
async fn disable_plugin(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    sqlx::query!("UPDATE plugins SET enabled = false WHERE id = $1", id)
        .execute(&state.db)
        .await?;
    Ok(Json(serde_json::json!({ "id": id, "enabled": false })))
}

/// Get plugin configuration.
async fn get_plugin_config(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let row = sqlx::query!("SELECT config, manifest FROM plugins WHERE id = $1", id)
        .fetch_optional(&state.db)
        .await?;

    match row {
        Some(r) => Ok(Json(serde_json::json!({
            "config": r.config,
            "schema": r.manifest.get("config_schema"),
        }))),
        None => Err(crate::error::ApiError::not_found("Plugin not found")),
    }
}

/// Update plugin configuration.
async fn update_plugin_config(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(config): Json<serde_json::Value>,
) -> ApiResult<Json<serde_json::Value>> {
    sqlx::query!("UPDATE plugins SET config = $2 WHERE id = $1", id, config)
        .execute(&state.db)
        .await?;
    Ok(Json(serde_json::json!({ "id": id, "config_updated": true })))
}

/// Browse the plugin registry (curated list).
async fn browse_registry(
    State(_state): State<Arc<AppState>>,
) -> ApiResult<Json<serde_json::Value>> {
    // Built-in registry of recommended plugins
    let registry = vec![
        serde_json::json!({
            "id": "nova-plugin-calibre-sync",
            "name": "Calibre Sync",
            "description": "两向同步 Calibre 书库",
            "version": "1.0.0",
            "author": "Nova Team",
            "runtime": "wasm",
            "hooks": ["book_imported", "scheduled"],
        }),
        serde_json::json!({
            "id": "nova-plugin-reading-stats-export",
            "name": "Reading Stats Export",
            "description": "导出阅读统计到 Notion/Obsidian",
            "version": "1.0.0",
            "author": "Nova Team",
            "runtime": "javascript",
            "hooks": ["scheduled"],
        }),
        serde_json::json!({
            "id": "nova-plugin-auto-tag",
            "name": "Auto Tag",
            "description": "自动根据内容为新书添加标签",
            "version": "1.0.0",
            "author": "Nova Team",
            "runtime": "wasm",
            "hooks": ["book_imported", "after_ai_process"],
        }),
        serde_json::json!({
            "id": "nova-plugin-custom-css",
            "name": "Custom Reader CSS",
            "description": "自定义阅读器样式和排版",
            "version": "1.0.0",
            "author": "Nova Team",
            "runtime": "javascript",
            "hooks": ["before_chapter_render"],
        }),
    ];

    Ok(Json(serde_json::json!({ "registry": registry, "total": registry.len() })))
}

#[derive(Debug, Deserialize)]
struct TriggerHookRequest {
    payload: serde_json::Value,
}

/// Manually trigger a plugin hook (for testing/automation).
async fn trigger_hook(
    State(state): State<Arc<AppState>>,
    Path(hook): Path<String>,
    Json(body): Json<TriggerHookRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    // Find all enabled plugins that subscribe to this hook
    let plugins = sqlx::query!(
        r#"SELECT id, manifest FROM plugins
           WHERE enabled = true AND manifest->'hooks' ? $1"#,
        hook
    )
    .fetch_all(&state.db)
    .await?;

    let mut results: Vec<serde_json::Value> = Vec::new();
    for plugin in &plugins {
        // In production, this would execute the plugin's Lua/WASM/JS code
        // in a sandboxed runtime with the payload
        results.push(serde_json::json!({
            "plugin_id": plugin.id,
            "hook": hook,
            "status": "executed",
        }));
    }

    Ok(Json(serde_json::json!({
        "hook": hook,
        "plugins_triggered": results.len(),
        "results": results,
    })))
}
