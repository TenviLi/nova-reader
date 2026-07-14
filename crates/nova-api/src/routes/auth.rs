use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{header::SET_COOKIE, HeaderMap},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::auth::jwt::JwtService;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use nova_core::repo::user_repo::UserRepository;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/auth/login", post(login))
        .route("/auth/register", post(register))
        .route("/auth/refresh", post(refresh_token))
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(get_me))
        .route("/auth/profile", axum::routing::put(update_profile))
        .route("/auth/avatar", post(upload_avatar))
        .route("/auth/change-password", post(change_password))
        .route("/avatars/{filename}", get(serve_avatar))
}

#[derive(Debug, Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Debug, Serialize)]
struct AuthResponse {
    access_token: String,
    expires_in: i64,
    user: UserResponse,
}

#[derive(Debug, Serialize)]
struct UserResponse {
    id: String,
    username: String,
    display_name: Option<String>,
    avatar_url: Option<String>,
    role: String,
}

fn role_to_response_str(role: &nova_core::domain::user::UserRole) -> String {
    serde_json::to_value(role)
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "reader".to_string())
}

fn build_auth_cookies(
    access_token: &str,
    refresh_token: &str,
    secure: bool,
    access_max_age_secs: i64,
) -> HeaderMap {
    let mut headers = HeaderMap::new();
    let secure_flag = if secure { "Secure; " } else { "" };
    let cookie = format!(
        "nova_token={}; HttpOnly; {}SameSite=Strict; Path=/; Max-Age={}",
        access_token, secure_flag, access_max_age_secs
    );
    headers.insert(SET_COOKIE, cookie.parse().expect("valid cookie header"));

    let refresh_cookie = format!(
        "nova_refresh={}; HttpOnly; {}SameSite=Strict; Path=/api/auth/refresh; Max-Age=2592000",
        refresh_token, secure_flag
    );
    headers.append(
        SET_COOKIE,
        refresh_cookie.parse().expect("valid cookie header"),
    );
    headers
}

/// Login with username and password.
/// Returns an access token (in JSON and as HTTP-only cookie).
async fn login(
    State(state): State<Arc<AppState>>,
    Json(body): Json<LoginRequest>,
) -> ApiResult<(HeaderMap, Json<AuthResponse>)> {
    let user = state
        .users
        .get_by_username(&body.username)
        .await?
        .ok_or(nova_core::Error::InvalidCredentials)?;

    // Verify password with argon2
    use argon2::{Argon2, PasswordHash, PasswordVerifier};
    let parsed_hash = PasswordHash::new(&user.password_hash)
        .map_err(|_| nova_core::Error::Internal("invalid password hash".into()))?;

    Argon2::default()
        .verify_password(body.password.as_bytes(), &parsed_hash)
        .map_err(|_| nova_core::Error::InvalidCredentials)?;

    // Generate tokens
    let user_id = user.id.to_string();
    let role_str = role_to_response_str(&user.role);
    let access_token = state.jwt.create_access_token(&user_id, &role_str)?;
    let (refresh_token, expires_at) = state.jwt.create_refresh_token(&user_id)?;

    // Store hashed refresh token
    let token_hash = JwtService::hash_token(&refresh_token);
    state
        .users
        .store_refresh_token(user.id.into_uuid(), &token_hash, expires_at)
        .await?;

    // Update last login
    state.users.touch_login(user.id.into_uuid()).await?;

    let access_max_age_secs = state.jwt.access_ttl_seconds();
    let headers = build_auth_cookies(
        &access_token,
        &refresh_token,
        state.config.is_production(),
        access_max_age_secs,
    );

    Ok((
        headers,
        Json(AuthResponse {
            access_token,
            expires_in: access_max_age_secs,
            user: UserResponse {
                id: user_id,
                username: user.username,
                display_name: user.display_name,
                avatar_url: user.avatar_url,
                role: role_str,
            },
        }),
    ))
}

#[derive(Debug, Deserialize)]
struct RegisterRequest {
    username: String,
    password: String,
    display_name: Option<String>,
}

/// Register the initial admin user (only works if no users exist).
async fn register(
    State(state): State<Arc<AppState>>,
    Json(body): Json<RegisterRequest>,
) -> ApiResult<(HeaderMap, Json<AuthResponse>)> {
    let user_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&state.db)
        .await
        .map_err(ApiError::from)?;

    if user_count > 0 {
        return Err(nova_core::Error::Duplicate {
            entity: "user",
            detail: "Initial setup has already been completed".into(),
        }
        .into());
    }

    // Validate password strength
    if body.password.len() < 8 {
        return Err(
            nova_core::Error::Validation("Password must be at least 8 characters".into()).into(),
        );
    }

    // Hash password with argon2
    use argon2::{password_hash::SaltString, Argon2, PasswordHasher};
    use rand::rngs::OsRng;

    let salt = SaltString::generate(&mut OsRng);
    let password_hash = Argon2::default()
        .hash_password(body.password.as_bytes(), &salt)
        .map_err(|e| nova_core::Error::Internal(format!("password hash error: {e}")))?
        .to_string();

    let mut user = state.users.create(&body.username, &password_hash).await?;
    let user_uuid = user.id.into_uuid();

    sqlx::query(
        "UPDATE users SET role = 'admin'::user_role, display_name = COALESCE($2, display_name), updated_at = NOW() WHERE id = $1",
    )
    .bind(user_uuid)
    .bind(&body.display_name)
    .execute(&state.db)
    .await
    .map_err(ApiError::from)?;

    user = state.users.get(user_uuid).await?;
    let user_id = user.id.to_string();
    let role_str = role_to_response_str(&user.role);
    let access_token = state.jwt.create_access_token(&user_id, &role_str)?;
    let (refresh_token, expires_at) = state.jwt.create_refresh_token(&user_id)?;

    let token_hash = JwtService::hash_token(&refresh_token);
    state
        .users
        .store_refresh_token(user_uuid, &token_hash, expires_at)
        .await?;
    state.users.touch_login(user_uuid).await?;

    let access_max_age_secs = state.jwt.access_ttl_seconds();
    let headers = build_auth_cookies(
        &access_token,
        &refresh_token,
        state.config.is_production(),
        access_max_age_secs,
    );

    Ok((
        headers,
        Json(AuthResponse {
            access_token,
            expires_in: access_max_age_secs,
            user: UserResponse {
                id: user_id,
                username: user.username,
                display_name: user.display_name,
                avatar_url: user.avatar_url,
                role: role_str,
            },
        }),
    ))
}

#[derive(Debug, Deserialize)]
struct RefreshRequest {
    refresh_token: Option<String>,
}

/// Refresh the access token using a valid refresh token.
async fn refresh_token(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<RefreshRequest>,
) -> ApiResult<(HeaderMap, Json<AuthResponse>)> {
    // Get refresh token from body or cookie
    let token = body
        .refresh_token
        .or_else(|| {
            headers
                .get("cookie")
                .and_then(|c| c.to_str().ok())
                .and_then(|cookies| {
                    cookies
                        .split(';')
                        .find_map(|c| c.trim().strip_prefix("nova_refresh="))
                        .map(String::from)
                })
        })
        .ok_or(nova_core::Error::Unauthorized)?;

    // Validate claims
    let claims = state.jwt.validate_token(&token)?;
    if claims.token_type != "refresh" {
        return Err(nova_core::Error::Unauthorized.into());
    }

    // Validate against DB
    let token_hash = JwtService::hash_token(&token);
    let user_id = state
        .users
        .validate_refresh_token(&token_hash)
        .await?
        .ok_or(nova_core::Error::Unauthorized)?;

    // Generate new access token
    let user = state.users.get(user_id).await?;
    let uid_str = user.id.to_string();
    let role_str = role_to_response_str(&user.role);
    let access_token = state.jwt.create_access_token(&uid_str, &role_str)?;

    // Set new access token cookie
    let mut resp_headers = HeaderMap::new();
    let secure_flag = if state.config.is_production() {
        "Secure; "
    } else {
        ""
    };
    let cookie = format!(
        "nova_token={}; HttpOnly; {}SameSite=Strict; Path=/; Max-Age={}",
        access_token,
        secure_flag,
        state.jwt.access_ttl_seconds()
    );
    resp_headers.insert(SET_COOKIE, cookie.parse().expect("valid cookie header"));

    Ok((
        resp_headers,
        Json(AuthResponse {
            access_token,
            expires_in: state.jwt.access_ttl_seconds(),
            user: UserResponse {
                id: uid_str,
                username: user.username,
                display_name: user.display_name,
                avatar_url: user.avatar_url,
                role: role_str,
            },
        }),
    ))
}

/// Logout — revoke all refresh tokens.
async fn logout(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResult<Json<serde_json::Value>> {
    // Try to get user from token
    if let Some(auth) = headers.get("authorization") {
        if let Ok(value) = auth.to_str() {
            if let Some(token) = value.strip_prefix("Bearer ") {
                if let Ok(claims) = state.jwt.validate_token(token) {
                    if let Ok(user_id) = uuid::Uuid::parse_str(&claims.sub) {
                        state.users.revoke_all_tokens(user_id).await?;
                    }
                }
            }
        }
    }

    Ok(Json(
        serde_json::json!({ "message": "Logged out successfully" }),
    ))
}

/// Get current user info.
async fn get_me(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResult<Json<UserResponse>> {
    let token = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .or_else(|| {
            headers
                .get("cookie")
                .and_then(|c| c.to_str().ok())
                .and_then(|cookies| {
                    cookies
                        .split(';')
                        .find_map(|c| c.trim().strip_prefix("nova_token="))
                })
        })
        .ok_or(nova_core::Error::Unauthorized)?;

    let claims = state.jwt.validate_token(token)?;
    let user_id = uuid::Uuid::parse_str(&claims.sub).map_err(|_| nova_core::Error::Unauthorized)?;

    let user = state.users.get(user_id).await?;

    Ok(Json(UserResponse {
        id: user.id.to_string(),
        username: user.username,
        display_name: user.display_name,
        avatar_url: user.avatar_url,
        role: format!("{:?}", user.role).to_lowercase(),
    }))
}

/// Update user profile.
#[derive(Debug, Deserialize)]
struct UpdateProfileRequest {
    display_name: Option<String>,
    avatar_url: Option<String>,
}

async fn update_profile(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<UpdateProfileRequest>,
) -> ApiResult<Json<UserResponse>> {
    let token = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .or_else(|| {
            headers
                .get("cookie")
                .and_then(|c| c.to_str().ok())
                .and_then(|cookies| {
                    cookies
                        .split(';')
                        .find_map(|c| c.trim().strip_prefix("nova_token="))
                })
        })
        .ok_or(nova_core::Error::Unauthorized)?;

    let claims = state.jwt.validate_token(token)?;
    let user_id = uuid::Uuid::parse_str(&claims.sub).map_err(|_| nova_core::Error::Unauthorized)?;

    sqlx::query(
        "UPDATE users SET display_name = COALESCE($2, display_name), avatar_url = COALESCE($3, avatar_url), updated_at = NOW() WHERE id = $1"
    )
    .bind(user_id)
    .bind(&body.display_name)
    .bind(&body.avatar_url)
    .execute(&state.db)
    .await
    .map_err(ApiError::from)?;

    let user = state.users.get(user_id).await?;
    Ok(Json(UserResponse {
        id: user.id.to_string(),
        username: user.username,
        display_name: user.display_name,
        avatar_url: user.avatar_url,
        role: format!("{:?}", user.role).to_lowercase(),
    }))
}

/// Upload user avatar.
async fn upload_avatar(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> ApiResult<Json<serde_json::Value>> {
    let token = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .or_else(|| {
            headers
                .get("cookie")
                .and_then(|c| c.to_str().ok())
                .and_then(|cookies| {
                    cookies
                        .split(';')
                        .find_map(|c| c.trim().strip_prefix("nova_token="))
                })
        })
        .ok_or(nova_core::Error::Unauthorized)?;

    let claims = state.jwt.validate_token(token)?;
    let user_id = uuid::Uuid::parse_str(&claims.sub).map_err(|_| nova_core::Error::Unauthorized)?;

    // Save avatar to data directory
    let data_dir = std::path::Path::new(&state.config.data_dir).join("avatars");
    tokio::fs::create_dir_all(&data_dir)
        .await
        .map_err(|e| nova_core::Error::Internal(e.to_string()))?;

    let filename = format!("{}.jpg", user_id);
    let path = data_dir.join(&filename);
    tokio::fs::write(&path, &body)
        .await
        .map_err(|e| nova_core::Error::Internal(e.to_string()))?;

    let avatar_url = format!("/api/avatars/{}", filename);
    sqlx::query("UPDATE users SET avatar_url = $2, updated_at = NOW() WHERE id = $1")
        .bind(user_id)
        .bind(&avatar_url)
        .execute(&state.db)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::json!({ "avatar_url": avatar_url })))
}

/// Serve avatar image files.
async fn serve_avatar(
    State(state): State<Arc<AppState>>,
    Path(filename): Path<String>,
) -> Result<axum::response::Response, ApiError> {
    use axum::http::{header, StatusCode};
    use axum::response::IntoResponse;

    // Sanitize filename to prevent path traversal
    if filename.contains("..") || filename.contains('/') || filename.contains('\\') {
        return Ok((StatusCode::BAD_REQUEST, "Invalid filename").into_response());
    }

    let data_dir = std::path::Path::new(&state.config.data_dir).join("avatars");
    let path = data_dir.join(&filename);

    match tokio::fs::read(&path).await {
        Ok(bytes) => {
            let content_type = if filename.ends_with(".png") {
                "image/png"
            } else {
                "image/jpeg"
            };
            Ok((
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, content_type),
                    (header::CACHE_CONTROL, "public, max-age=86400"),
                ],
                bytes,
            )
                .into_response())
        }
        Err(_) => Ok((StatusCode::NOT_FOUND, "Avatar not found").into_response()),
    }
}

// ─── Change Password ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ChangePasswordBody {
    current_password: String,
    new_password: String,
}

async fn change_password(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<ChangePasswordBody>,
) -> ApiResult<Json<serde_json::Value>> {
    let token = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .or_else(|| {
            headers
                .get("cookie")
                .and_then(|c| c.to_str().ok())
                .and_then(|cookies| {
                    cookies
                        .split(';')
                        .find_map(|c| c.trim().strip_prefix("nova_token="))
                })
        })
        .ok_or(nova_core::Error::Unauthorized)?;

    let claims = state.jwt.validate_token(token)?;
    let user_id = uuid::Uuid::parse_str(&claims.sub).map_err(|_| nova_core::Error::Unauthorized)?;

    // Validate new password
    if body.new_password.len() < 8 {
        return Err(nova_core::Error::Validation(
            "New password must be at least 8 characters".into(),
        )
        .into());
    }

    // Get current hash
    let user = sqlx::query_as::<_, (String,)>("SELECT password_hash FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(&state.db)
        .await
        .map_err(ApiError::from)?;

    // Verify current password
    use argon2::{
        password_hash::SaltString, Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    };
    let parsed_hash = PasswordHash::new(&user.0)
        .map_err(|_| nova_core::Error::Internal("invalid password hash".into()))?;
    Argon2::default()
        .verify_password(body.current_password.as_bytes(), &parsed_hash)
        .map_err(|_| nova_core::Error::Unauthorized)?;

    // Hash new password
    let salt = SaltString::generate(&mut rand::rngs::OsRng);
    let new_hash = Argon2::default()
        .hash_password(body.new_password.as_bytes(), &salt)
        .map_err(|e| nova_core::Error::Internal(format!("password hash error: {e}")))?
        .to_string();

    // Update in DB
    sqlx::query("UPDATE users SET password_hash = $2, updated_at = NOW() WHERE id = $1")
        .bind(user_id)
        .bind(&new_hash)
        .execute(&state.db)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(
        serde_json::json!({ "message": "Password changed successfully" }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_auth_cookies_sets_access_and_refresh_cookies() {
        let headers = build_auth_cookies("access-token", "refresh-token", false, 1800);
        let cookies = headers
            .get_all(SET_COOKIE)
            .iter()
            .map(|value| value.to_str().expect("cookie header should be valid"))
            .collect::<Vec<_>>();

        assert_eq!(cookies.len(), 2);
        assert!(cookies[0].contains("nova_token=access-token"));
        assert!(cookies[0].contains("HttpOnly"));
        assert!(cookies[0].contains("SameSite=Strict"));
        assert!(cookies[0].contains("Path=/"));
        assert!(cookies[0].contains("Max-Age=1800"));
        assert!(!cookies[0].contains("Secure"));

        assert!(cookies[1].contains("nova_refresh=refresh-token"));
        assert!(cookies[1].contains("HttpOnly"));
        assert!(cookies[1].contains("SameSite=Strict"));
        assert!(cookies[1].contains("Path=/api/auth/refresh"));
        assert!(cookies[1].contains("Max-Age=2592000"));
        assert!(!cookies[1].contains("Secure"));
    }
}
