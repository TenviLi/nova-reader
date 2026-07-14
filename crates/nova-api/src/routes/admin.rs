use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    routing::{get, patch, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{error::ApiError, extractors::AdminUser, state::AppState};

#[derive(Deserialize)]
struct LogsQuery {
    level: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/admin/users", get(list_users))
        .route("/admin/users/{id}", patch(update_user).delete(delete_user))
        .route("/admin/users/batch-role", post(batch_update_role))
        .route("/admin/groups", get(list_groups).post(create_group))
        .route(
            "/admin/groups/{id}",
            patch(update_group).delete(delete_group),
        )
        .route("/admin/groups/{id}/members", post(set_group_members))
        .route(
            "/admin/permission-templates",
            get(list_permission_templates).post(create_permission_template),
        )
        .route(
            "/admin/permission-templates/{id}",
            patch(update_permission_template).delete(delete_permission_template),
        )
        .route("/admin/logs", get(get_system_logs))
        .route("/admin/jobs", get(get_scheduled_jobs))
        .route("/admin/jobs/{id}", patch(toggle_job))
        .route("/admin/health-check", get(books_health_check))
        .route("/admin/orphans", get(detect_orphan_books))
        .route("/admin/recalculate", post(recalculate_metadata))
        .route("/admin/reindex-meilisearch", post(reindex_meilisearch))
}

/// List all users (admin only).
async fn list_users(
    _admin: AdminUser,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let users: Vec<serde_json::Value> = sqlx::query_as::<_, UserRow>(
        r#"
        SELECT u.id, u.username, u.display_name, u.role::text as role,
               u.created_at, u.updated_at,
               COALESCE((SELECT COUNT(*) FROM books), 0)::bigint as books_count,
               COALESCE((SELECT SUM(reading_time_secs) FROM reading_progress), 0)::bigint as reading_time_secs
        FROM users u
        ORDER BY u.created_at
        "#
    )
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?
    .into_iter()
    .map(|u| serde_json::json!({
        "id": u.id,
        "username": u.username,
        "display_name": u.display_name,
        "role": u.role,
        "created_at": u.created_at,
        "last_login_at": u.updated_at,
        "books_count": u.books_count,
        "reading_time_hours": u.reading_time_secs as f64 / 3600.0,
    }))
    .collect();

    Ok(Json(serde_json::json!(users)))
}

#[derive(sqlx::FromRow)]
struct UserRow {
    id: uuid::Uuid,
    username: String,
    display_name: Option<String>,
    role: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    books_count: i64,
    reading_time_secs: i64,
}

#[derive(Deserialize)]
struct UpdateUserRequest {
    display_name: Option<String>,
    role: Option<String>,
}

/// Update a user's admin-managed fields.
async fn update_user(
    _admin: AdminUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateUserRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let current_role: Option<String> =
        sqlx::query_scalar("SELECT role::text FROM users WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await
            .map_err(ApiError::from)?;

    let Some(current_role) = current_role else {
        return Err(ApiError::not_found("User not found"));
    };

    if let Some(role) = body.role.as_deref() {
        if !matches!(role, "admin" | "reader" | "guest") {
            return Err(ApiError::bad_request("Invalid user role"));
        }

        if current_role == "admin" && role != "admin" {
            let admin_count: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE role = 'admin'")
                    .fetch_one(&state.db)
                    .await
                    .map_err(ApiError::from)?;
            if admin_count <= 1 {
                return Err(ApiError::bad_request("Cannot demote the last admin user"));
            }
        }
    }

    let updated = sqlx::query_as::<_, UserRow>(
        r#"
        UPDATE users
        SET display_name = COALESCE($2, display_name),
            role = COALESCE($3::user_role, role),
            updated_at = NOW()
        WHERE id = $1
        RETURNING id, username, display_name, role::text as role,
                  created_at, updated_at,
                  COALESCE((SELECT COUNT(*) FROM books), 0)::bigint as books_count,
                  COALESCE((SELECT SUM(reading_time_secs) FROM reading_progress), 0)::bigint as reading_time_secs
        "#,
    )
    .bind(id)
    .bind(body.display_name.as_deref())
    .bind(body.role.as_deref())
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(serde_json::json!({
        "id": updated.id,
        "username": updated.username,
        "display_name": updated.display_name,
        "role": updated.role,
        "created_at": updated.created_at,
        "last_login_at": updated.updated_at,
        "books_count": updated.books_count,
        "reading_time_hours": updated.reading_time_secs as f64 / 3600.0,
    })))
}

/// Delete a user by ID (cannot delete self or last admin).
async fn delete_user(
    admin: AdminUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let admin_id = Uuid::parse_str(&admin.id).map_err(|_| ApiError::unauthorized())?;
    if admin_id == id {
        return Err(ApiError::bad_request("Cannot delete your own user account"));
    }

    // Check user exists
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)")
        .bind(id)
        .fetch_one(&state.db)
        .await
        .map_err(ApiError::from)?;

    if !exists {
        return Err(ApiError::not_found("User not found"));
    }

    // Don't delete the last admin
    let admin_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE role = 'admin'")
        .fetch_one(&state.db)
        .await
        .map_err(ApiError::from)?;

    let user_role: Option<String> =
        sqlx::query_scalar("SELECT role::text FROM users WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await
            .map_err(ApiError::from)?;

    if user_role.as_deref() == Some("admin") && admin_count <= 1 {
        return Err(ApiError::bad_request("Cannot delete the last admin user"));
    }

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::json!({ "status": "deleted" })))
}

#[derive(Deserialize)]
struct BatchRoleRequest {
    user_ids: Vec<Uuid>,
    role: String,
}

/// Batch-update the role of multiple users at once (admin only).
async fn batch_update_role(
    _admin: AdminUser,
    State(state): State<Arc<AppState>>,
    Json(body): Json<BatchRoleRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !matches!(body.role.as_str(), "admin" | "reader" | "guest") {
        return Err(ApiError::bad_request("Invalid user role"));
    }
    if body.user_ids.is_empty() {
        return Ok(Json(serde_json::json!({ "updated": 0 })));
    }

    // Guard: never let a batch demotion remove the last admin.
    if body.role != "admin" {
        let total_admins: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE role = 'admin'")
                .fetch_one(&state.db)
                .await
                .map_err(ApiError::from)?;
        let demoted_admins: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM users WHERE role = 'admin' AND id = ANY($1::uuid[])",
        )
        .bind(&body.user_ids)
        .fetch_one(&state.db)
        .await
        .map_err(ApiError::from)?;
        if total_admins - demoted_admins < 1 {
            return Err(ApiError::bad_request("Cannot demote the last admin user"));
        }
    }

    let updated = sqlx::query(
        "UPDATE users SET role = $1::user_role, updated_at = NOW() WHERE id = ANY($2::uuid[])",
    )
    .bind(&body.role)
    .bind(&body.user_ids)
    .execute(&state.db)
    .await
    .map_err(ApiError::from)?
    .rows_affected();

    Ok(Json(serde_json::json!({ "updated": updated })))
}

#[derive(sqlx::FromRow)]
struct GroupRow {
    id: Uuid,
    name: String,
    description: String,
    color: String,
    created_at: chrono::DateTime<chrono::Utc>,
    member_count: i64,
}

/// List all user groups with member counts and member ids.
async fn list_groups(
    _admin: AdminUser,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let groups = sqlx::query_as::<_, GroupRow>(
        r#"
        SELECT g.id, g.name, g.description, g.color, g.created_at,
               COALESCE((SELECT COUNT(*) FROM group_members gm WHERE gm.group_id = g.id), 0)::bigint as member_count
        FROM user_groups g
        ORDER BY g.created_at
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;

    let mut result = Vec::with_capacity(groups.len());
    for g in groups {
        let members: Vec<Uuid> =
            sqlx::query_scalar("SELECT user_id FROM group_members WHERE group_id = $1")
                .bind(g.id)
                .fetch_all(&state.db)
                .await
                .map_err(ApiError::from)?;

        result.push(serde_json::json!({
            "id": g.id,
            "name": g.name,
            "description": g.description,
            "color": g.color,
            "created_at": g.created_at,
            "member_count": g.member_count,
            "member_ids": members,
        }));
    }

    Ok(Json(serde_json::json!(result)))
}

#[derive(Deserialize)]
struct CreateGroupRequest {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default = "default_group_color")]
    color: String,
}

fn default_group_color() -> String {
    "slate".to_string()
}

/// Create a new user group.
async fn create_group(
    _admin: AdminUser,
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateGroupRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let name = body.name.trim();
    if name.is_empty() {
        return Err(ApiError::bad_request("Group name is required"));
    }

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO user_groups (name, description, color) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(name)
    .bind(&body.description)
    .bind(&body.color)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(serde_json::json!({
        "id": id,
        "name": name,
        "description": body.description,
        "color": body.color,
        "member_count": 0,
        "member_ids": [],
    })))
}

#[derive(Deserialize)]
struct UpdateGroupRequest {
    name: Option<String>,
    description: Option<String>,
    color: Option<String>,
}

/// Update a user group's metadata.
async fn update_group(
    _admin: AdminUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateGroupRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let updated = sqlx::query(
        r#"
        UPDATE user_groups
        SET name = COALESCE($2, name),
            description = COALESCE($3, description),
            color = COALESCE($4, color),
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(body.name.as_deref())
    .bind(body.description.as_deref())
    .bind(body.color.as_deref())
    .execute(&state.db)
    .await
    .map_err(ApiError::from)?;

    if updated.rows_affected() == 0 {
        return Err(ApiError::not_found("Group not found"));
    }
    Ok(Json(serde_json::json!({ "status": "ok" })))
}

/// Delete a user group (memberships cascade).
async fn delete_group(
    _admin: AdminUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let deleted = sqlx::query("DELETE FROM user_groups WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(ApiError::from)?;
    if deleted.rows_affected() == 0 {
        return Err(ApiError::not_found("Group not found"));
    }
    Ok(Json(serde_json::json!({ "status": "deleted" })))
}

#[derive(Deserialize)]
struct SetMembersRequest {
    user_ids: Vec<Uuid>,
}

/// Replace the full membership of a group with the provided user list.
async fn set_group_members(
    _admin: AdminUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(body): Json<SetMembersRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM user_groups WHERE id = $1)")
        .bind(id)
        .fetch_one(&state.db)
        .await
        .map_err(ApiError::from)?;
    if !exists {
        return Err(ApiError::not_found("Group not found"));
    }

    let mut tx = state.db.begin().await.map_err(ApiError::from)?;
    sqlx::query("DELETE FROM group_members WHERE group_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(ApiError::from)?;

    if !body.user_ids.is_empty() {
        sqlx::query(
            r#"
            INSERT INTO group_members (group_id, user_id)
            SELECT $1, uid FROM UNNEST($2::uuid[]) AS uid
            ON CONFLICT (group_id, user_id) DO NOTHING
            "#,
        )
        .bind(id)
        .bind(&body.user_ids)
        .execute(&mut *tx)
        .await
        .map_err(ApiError::from)?;
    }

    tx.commit().await.map_err(ApiError::from)?;

    Ok(Json(serde_json::json!({
        "status": "ok",
        "member_count": body.user_ids.len(),
    })))
}

// ─── Permission Templates ───────────────────────────────────────────────────

#[derive(sqlx::FromRow, Serialize)]
struct PermissionTemplateRow {
    id: Uuid,
    name: String,
    description: Option<String>,
    can_read: bool,
    can_write: bool,
    can_manage: bool,
    is_system: bool,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Deserialize)]
struct CreatePermissionTemplateRequest {
    name: String,
    description: Option<String>,
    #[serde(default)]
    can_read: bool,
    #[serde(default)]
    can_write: bool,
    #[serde(default)]
    can_manage: bool,
}

#[derive(Deserialize)]
struct UpdatePermissionTemplateRequest {
    name: Option<String>,
    description: Option<String>,
    can_read: Option<bool>,
    can_write: Option<bool>,
    can_manage: Option<bool>,
}

fn normalize_template_permissions(read: bool, write: bool, manage: bool) -> (bool, bool, bool) {
    let mut can_read = read;
    let mut can_write = write;
    if manage {
        can_read = true;
        can_write = true;
    }
    if can_write {
        can_read = true;
    }
    (can_read, can_write, manage)
}

fn validate_template_name(name: &str) -> Result<String, ApiError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(ApiError::bad_request("Template name is required"));
    }
    Ok(trimmed.to_string())
}

/// List built-in and custom permission templates.
async fn list_permission_templates(
    _admin: AdminUser,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let templates = sqlx::query_as::<_, PermissionTemplateRow>(
        r#"
        SELECT id, name, description, can_read, can_write, can_manage,
               is_system, created_at, updated_at
        FROM permission_templates
        ORDER BY is_system DESC, can_manage ASC, can_write ASC, name ASC
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(serde_json::json!(templates)))
}

/// Create a custom permission template.
async fn create_permission_template(
    _admin: AdminUser,
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreatePermissionTemplateRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let name = validate_template_name(&body.name)?;
    let (can_read, can_write, can_manage) =
        normalize_template_permissions(body.can_read, body.can_write, body.can_manage);

    let created = sqlx::query_as::<_, PermissionTemplateRow>(
        r#"
        INSERT INTO permission_templates
            (id, name, description, can_read, can_write, can_manage, is_system)
        VALUES ($1, $2, $3, $4, $5, $6, false)
        RETURNING id, name, description, can_read, can_write, can_manage,
                  is_system, created_at, updated_at
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(name)
    .bind(body.description.as_deref())
    .bind(can_read)
    .bind(can_write)
    .bind(can_manage)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(serde_json::json!(created)))
}

/// Update a custom permission template. Built-in templates are immutable.
async fn update_permission_template(
    _admin: AdminUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdatePermissionTemplateRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let current = sqlx::query_as::<_, PermissionTemplateRow>(
        r#"
        SELECT id, name, description, can_read, can_write, can_manage,
               is_system, created_at, updated_at
        FROM permission_templates
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::from)?;

    let Some(current) = current else {
        return Err(ApiError::not_found("Permission template not found"));
    };
    if current.is_system {
        return Err(ApiError::bad_request(
            "Built-in permission templates cannot be modified",
        ));
    }

    let next_name = match body.name.as_deref() {
        Some(name) => Some(validate_template_name(name)?),
        None => None,
    };
    let (can_read, can_write, can_manage) = normalize_template_permissions(
        body.can_read.unwrap_or(current.can_read),
        body.can_write.unwrap_or(current.can_write),
        body.can_manage.unwrap_or(current.can_manage),
    );

    sqlx::query(
        r#"
        UPDATE permission_templates
        SET name = COALESCE($2, name),
            description = COALESCE($3, description),
            can_read = $4,
            can_write = $5,
            can_manage = $6,
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(next_name.as_deref())
    .bind(body.description.as_deref())
    .bind(can_read)
    .bind(can_write)
    .bind(can_manage)
    .execute(&state.db)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(serde_json::json!({ "status": "ok" })))
}

/// Delete a custom permission template. Built-in templates are immutable.
async fn delete_permission_template(
    _admin: AdminUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let is_system: Option<bool> =
        sqlx::query_scalar("SELECT is_system FROM permission_templates WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await
            .map_err(ApiError::from)?;

    let Some(is_system) = is_system else {
        return Err(ApiError::not_found("Permission template not found"));
    };
    if is_system {
        return Err(ApiError::bad_request(
            "Built-in permission templates cannot be deleted",
        ));
    }

    sqlx::query("DELETE FROM permission_templates WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::json!({ "status": "deleted" })))
}

/// Get system logs (recent activity from tasks/errors).
async fn get_system_logs(
    _admin: AdminUser,
    State(state): State<Arc<AppState>>,
    Query(params): Query<LogsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let limit = params.limit.unwrap_or(100).min(500);
    let offset = params.offset.unwrap_or(0);

    // Pull recent tasks as "logs"
    let logs: Vec<serde_json::Value> = sqlx::query_as::<_, LogRow>(
        r#"
        SELECT id, kind::text as level, status::text as message,
               created_at as timestamp
        FROM tasks
        ORDER BY created_at DESC
        LIMIT $1 OFFSET $2
        "#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|l| {
        let level_str = if l.message.as_deref() == Some("failed") {
            "error"
        } else {
            "info"
        };
        serde_json::json!({
            "id": l.id,
            "level": level_str,
            "target": &l.level,
            "message": format!("[{}] {}", l.level, l.message.unwrap_or_default()),
            "timestamp": l.timestamp,
        })
    })
    .filter(|log| {
        if let Some(ref level_filter) = params.level {
            log["level"].as_str() == Some(level_filter.as_str())
        } else {
            true
        }
    })
    .collect();

    Ok(Json(serde_json::json!(logs)))
}

#[derive(sqlx::FromRow)]
struct LogRow {
    id: Uuid,
    level: String,
    message: Option<String>,
    timestamp: chrono::DateTime<chrono::Utc>,
}

/// Get scheduled background jobs.
async fn get_scheduled_jobs(
    _admin: AdminUser,
    State(_state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Return static job definitions (no scheduled_jobs table yet)
    Ok(Json(serde_json::json!([
        {
            "id": "library-scan",
            "name": "Library Auto-Scan",
            "cron": "0 */6 * * *",
            "status": "active",
            "last_run": null,
            "next_run": null,
            "last_duration_ms": null,
            "logs": [],
        },
        {
            "id": "metadata-refresh",
            "name": "Metadata Refresh",
            "cron": "0 2 * * 0",
            "status": "active",
            "last_run": null,
            "next_run": null,
            "last_duration_ms": null,
            "logs": [],
        },
        {
            "id": "cleanup-temp",
            "name": "Temp Files Cleanup",
            "cron": "0 3 * * *",
            "status": "active",
            "last_run": null,
            "next_run": null,
            "last_duration_ms": null,
            "logs": [],
        },
    ])))
}

/// Toggle a scheduled job on/off.
async fn toggle_job(
    _admin: AdminUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Stub: no real job scheduler yet
    Ok(Json(serde_json::json!({
        "id": id,
        "enabled": true,
        "message": "Job toggled",
    })))
}

/// Detect orphan books (DB records whose file no longer exists on disk).
async fn detect_orphan_books(
    _admin: AdminUser,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    #[derive(sqlx::FromRow)]
    struct BookFileRow {
        id: Uuid,
        title: String,
        file_path: String,
        library_id: Option<Uuid>,
    }

    let books = sqlx::query_as::<_, BookFileRow>(
        "SELECT id, title, file_path, library_id FROM books WHERE file_path IS NOT NULL AND file_path != ''"
    )
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;

    let mut orphans = Vec::new();
    for book in &books {
        if !std::path::Path::new(&book.file_path).exists() {
            orphans.push(serde_json::json!({
                "id": book.id,
                "title": book.title,
                "file_path": book.file_path,
                "library_id": book.library_id,
            }));
        }
    }

    Ok(Json(serde_json::json!({
        "total_checked": books.len(),
        "orphans_found": orphans.len(),
        "orphans": orphans,
    })))
}

/// Book health check: detect books with missing covers, empty chapters, or abnormal progress.
async fn books_health_check(
    _admin: AdminUser,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Books without covers
    let no_cover: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM books WHERE cover_path IS NULL OR cover_path = ''",
    )
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::from)?;

    // Books with zero chapters
    let no_chapters: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM books WHERE id NOT IN (SELECT DISTINCT book_id FROM chapters)",
    )
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::from)?;

    // Books with abnormal progress (> 1.0 or negative)
    let bad_progress: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM reading_progress WHERE progress < 0 OR progress > 1.0",
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    // Books marked as 'ready' but with 0 word count
    let zero_words: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM books WHERE status = 'ready' AND (word_count IS NULL OR word_count = 0)"
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    let total_issues = no_cover + no_chapters + bad_progress + zero_words;

    Ok(Json(serde_json::json!({
        "total_issues": total_issues,
        "issues": {
            "missing_cover": no_cover,
            "no_chapters": no_chapters,
            "abnormal_progress": bad_progress,
            "zero_word_count": zero_words,
        },
        "status": if total_issues == 0 { "healthy" } else { "has_issues" },
    })))
}

/// Recalculate word counts from chapter content and extract/link authors for all books.
async fn recalculate_metadata(
    _admin: AdminUser,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // 1. Recalculate word counts from chapter content
    let word_count_updated: i64 = sqlx::query_scalar(
        r#"
        WITH chapter_sums AS (
            SELECT book_id, SUM(
                LENGTH(REGEXP_REPLACE(content, '\s', '', 'g'))
            )::bigint AS total_words
            FROM chapters
            GROUP BY book_id
        ),
        updated AS (
            UPDATE books
            SET word_count = chapter_sums.total_words
            FROM chapter_sums
            WHERE books.id = chapter_sums.book_id
              AND (books.word_count IS NULL OR books.word_count = 0 OR books.word_count != chapter_sums.total_words)
            RETURNING books.id
        )
        SELECT COUNT(*)::bigint FROM updated
        "#,
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    // 2. Extract and link authors from book metadata (author field -> persons table)
    let books_with_authors: Vec<(Uuid, String)> = sqlx::query_as(
        r#"
        SELECT b.id, b.author::text
        FROM books b
        WHERE b.author IS NOT NULL AND b.author != ''
          AND NOT EXISTS (
            SELECT 1 FROM book_persons bp WHERE bp.book_id = b.id
          )
        "#,
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let mut authors_linked = 0i64;
    for (book_id, author_name) in &books_with_authors {
        // Find or create person
        let person_id: Option<Uuid> = sqlx::query_scalar("SELECT id FROM persons WHERE name = $1")
            .bind(author_name)
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None);

        let person_id = match person_id {
            Some(id) => id,
            None => {
                let new_id = Uuid::now_v7();
                let _ = sqlx::query(
                    "INSERT INTO persons (id, name, sort_name) VALUES ($1, $2, $2) ON CONFLICT (name) DO NOTHING"
                )
                .bind(new_id)
                .bind(author_name)
                .execute(&state.db)
                .await;
                // Fetch again in case of race condition
                sqlx::query_scalar("SELECT id FROM persons WHERE name = $1")
                    .bind(author_name)
                    .fetch_optional(&state.db)
                    .await
                    .unwrap_or(None)
                    .unwrap_or(new_id)
            }
        };

        let _ = sqlx::query(
            "INSERT INTO book_persons (book_id, person_id, role) VALUES ($1, $2, 'author'::person_role) ON CONFLICT DO NOTHING"
        )
        .bind(book_id)
        .bind(person_id)
        .execute(&state.db)
        .await;

        authors_linked += 1;
    }

    Ok(Json(serde_json::json!({
        "word_count_updated": word_count_updated,
        "authors_linked": authors_linked,
        "message": format!("Recalculated {} book word counts, linked {} authors", word_count_updated, authors_linked),
    })))
}

/// POST /admin/reindex-meilisearch — Rebuild the Meilisearch chunks index from all books
async fn reindex_meilisearch(
    _admin: AdminUser,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Fetch all books
    let books: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id, title FROM books WHERE status != 'archived' ORDER BY title")
            .fetch_all(&state.db)
            .await
            .map_err(ApiError::from)?;

    let chunker = nova_embed::chunker_v2::NovelChunker::new(
        nova_embed::chunker_v2::ChunkingConfigV2::default(),
    );

    let mut total_chunks = 0usize;
    let mut books_indexed = 0usize;

    for (book_id, book_title) in &books {
        let chapters = state
            .chapters
            .list_searchable_by_book(*book_id)
            .await
            .unwrap_or_default();

        if chapters.is_empty() {
            continue;
        }

        let mut documents = Vec::new();
        let book_id_str = book_id.to_string();

        for chapter in &chapters {
            if chapter.content.trim().is_empty() {
                continue;
            }
            let chunks = chunker.chunk_document(&chapter.content, Some(book_title.as_str()));
            for chunk in &chunks {
                documents.push(serde_json::json!({
                    "id": format!("{}_{}_{}", book_id_str, chapter.chapter_index, chunk.index),
                    "book_id": book_id_str,
                    "book_title": book_title,
                    "chapter_title": chapter.title,
                    "chapter_index": chapter.chapter_index,
                    "content": chunk.content,
                    "chunk_index": chunk.index,
                }));
            }
        }

        if documents.is_empty() {
            continue;
        }

        total_chunks += documents.len();
        books_indexed += 1;

        // Upsert to Meilisearch in batches of 1000
        for batch in documents.chunks(1000) {
            let _ = state
                .http_client
                .post(format!(
                    "{}/indexes/chunks/documents",
                    state.config.meili_url
                ))
                .header(
                    "Authorization",
                    format!("Bearer {}", state.config.meili_master_key),
                )
                .header("Content-Type", "application/json")
                .json(&batch)
                .send()
                .await;
        }
    }

    // Ensure searchable attributes are configured
    let _ = state
        .http_client
        .put(format!(
            "{}/indexes/chunks/settings/searchable-attributes",
            state.config.meili_url
        ))
        .header(
            "Authorization",
            format!("Bearer {}", state.config.meili_master_key),
        )
        .json(&serde_json::json!([
            "content",
            "book_title",
            "chapter_title"
        ]))
        .send()
        .await;

    let _ = state
        .http_client
        .put(format!(
            "{}/indexes/chunks/settings/filterable-attributes",
            state.config.meili_url
        ))
        .header(
            "Authorization",
            format!("Bearer {}", state.config.meili_master_key),
        )
        .json(&serde_json::json!([
            "book_id",
            "chapter_index",
            "chunk_index"
        ]))
        .send()
        .await;

    Ok(Json(serde_json::json!({
        "books_indexed": books_indexed,
        "total_chunks": total_chunks,
        "message": format!("Indexed {} chunks from {} books into Meilisearch", total_chunks, books_indexed),
    })))
}
