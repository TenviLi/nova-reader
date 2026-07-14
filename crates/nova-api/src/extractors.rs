use std::sync::Arc;

use axum::{
    extract::FromRequestParts,
    http::request::Parts,
};
use axum_extra::extract::CookieJar;

use crate::error::ApiError;
use crate::state::AppState;

/// Extracted authenticated user from JWT token.
/// Use this as a handler parameter to require authentication.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id: String,
}

impl FromRequestParts<Arc<AppState>> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        // Try Authorization: Bearer <token> header first
        let token = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(|t| t.to_string());

        // Fallback to HTTP-only cookie
        let token = match token {
            Some(t) => t,
            None => {
                let jar = CookieJar::from_headers(&parts.headers);
                jar.get("nova_token")
                    .map(|c| c.value().to_string())
                    .ok_or_else(ApiError::unauthorized)?
            }
        };

        // Validate JWT
        let claims = state
            .jwt
            .validate_token(&token)
            .map_err(|_| ApiError::unauthorized())?;

        // Ensure it's an access token
        if claims.token_type != "access" {
            return Err(ApiError::unauthorized());
        }

        Ok(AuthUser { id: claims.sub })
    }
}

/// Optional authentication extractor - does not reject unauthenticated requests.
/// Kept for public share routes that should work anonymously but can personalize
/// when a signed-in user is present.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct OptionalAuth(pub Option<AuthUser>);

impl FromRequestParts<Arc<AppState>> for OptionalAuth {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let auth = AuthUser::from_request_parts(parts, state).await.ok();
        Ok(OptionalAuth(auth))
    }
}

/// Extracted authenticated user that is verified (against the database) to have the
/// `admin` role. Use this on privileged admin handlers to enforce access control.
///
/// The DB check (rather than trusting the JWT `role` claim alone) ensures that a
/// demoted user cannot keep acting as admin until their token expires.
#[derive(Debug, Clone)]
pub struct AdminUser {
    pub id: String,
}

impl FromRequestParts<Arc<AppState>> for AdminUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let auth = AuthUser::from_request_parts(parts, state).await?;
        let uuid = uuid::Uuid::parse_str(&auth.id).map_err(|_| ApiError::unauthorized())?;

        let role: Option<String> = sqlx::query_scalar("SELECT role::text FROM users WHERE id = $1")
            .bind(uuid)
            .fetch_optional(&state.db)
            .await
            .map_err(|_| ApiError::unauthorized())?;

        if role.as_deref() == Some("admin") {
            Ok(AdminUser { id: auth.id })
        } else {
            Err(ApiError::forbidden())
        }
    }
}
