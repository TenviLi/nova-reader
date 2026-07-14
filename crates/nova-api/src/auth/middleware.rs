use std::sync::Arc;

use axum::{extract::Request, http::header::COOKIE, middleware::Next, response::Response};

use crate::auth::jwt::JwtService;
use crate::state::AppState;

/// Extract the authenticated user ID from the request.
/// Checks: 1) Authorization: Bearer <token> header, 2) `nova_token` cookie.
pub async fn require_auth(
    state: Arc<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, crate::error::ApiError> {
    let token = extract_token(&req);

    let token = token.ok_or(nova_core::Error::Unauthorized)?;

    let claims = state.jwt.validate_token(&token)?;

    if claims.token_type != "access" {
        return Err(nova_core::Error::Unauthorized.into());
    }

    // Inject user_id into request extensions
    req.extensions_mut().insert(AuthUser {
        user_id: claims.sub.clone(),
    });

    Ok(next.run(req).await)
}

/// Authenticated user info injected into request extensions.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: String,
}

/// Extract token from Authorization header or cookie.
fn extract_token(req: &Request) -> Option<String> {
    // Try Authorization: Bearer <token>
    if let Some(auth_header) = req.headers().get("authorization") {
        if let Ok(value) = auth_header.to_str() {
            if let Some(token) = value.strip_prefix("Bearer ") {
                return Some(token.to_string());
            }
        }
    }

    // Try cookie
    if let Some(cookie_header) = req.headers().get(COOKIE) {
        if let Ok(cookies) = cookie_header.to_str() {
            for cookie in cookies.split(';') {
                let cookie = cookie.trim();
                if let Some(token) = cookie.strip_prefix("nova_token=") {
                    return Some(token.to_string());
                }
            }
        }
    }

    None
}
