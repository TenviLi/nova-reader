//! Custom middleware: rate limiting, request ID injection, auth guard.

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use governor::{
    clock::DefaultClock,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter,
};
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::state::AppState;

/// Rate limiter for AI endpoints (shared across all users for now).
/// Default: 30 requests per minute for AI, 100 for general.
pub type AiRateLimiter = Arc<RateLimiter<NotKeyed, InMemoryState, DefaultClock>>;

/// Create a rate limiter for AI endpoints.
/// burst: max requests in the window.
pub fn create_ai_rate_limiter(requests_per_minute: u32) -> AiRateLimiter {
    let quota = Quota::per_minute(
        NonZeroU32::new(requests_per_minute).unwrap_or(NonZeroU32::new(30).expect("30 > 0")),
    );
    Arc::new(RateLimiter::direct(quota))
}

/// Create a general rate limiter.
#[cfg(test)]
pub fn create_general_rate_limiter(requests_per_minute: u32) -> AiRateLimiter {
    let quota = Quota::per_minute(
        NonZeroU32::new(requests_per_minute).unwrap_or(NonZeroU32::new(200).expect("200 > 0")),
    );
    Arc::new(RateLimiter::direct(quota))
}

/// Per-user rate limiter that tracks limits by user ID (extracted from JWT sub claim).
/// Uses a HashMap of individual rate limiters, one per user.
/// Implements LRU-style eviction: removes oldest entries when threshold is reached.
pub struct PerUserRateLimiter {
    limiters: Mutex<
        HashMap<
            String,
            (
                Arc<RateLimiter<NotKeyed, InMemoryState, DefaultClock>>,
                std::time::Instant,
            ),
        >,
    >,
    quota: Quota,
    max_entries: usize,
}

impl PerUserRateLimiter {
    pub fn new(requests_per_minute: u32) -> Self {
        let rpm =
            NonZeroU32::new(requests_per_minute).unwrap_or(NonZeroU32::new(60).expect("60 > 0"));
        Self {
            limiters: Mutex::new(HashMap::new()),
            quota: Quota::per_minute(rpm),
            max_entries: 10_000,
        }
    }

    /// Check if a user is within their rate limit. Returns Ok(()) or Err(()).
    pub async fn check(&self, user_id: &str) -> Result<(), ()> {
        let mut map = self.limiters.lock().await;

        // Evict oldest 25% of entries when at capacity (avoids thundering herd from full clear)
        if map.len() >= self.max_entries {
            let mut entries: Vec<(String, std::time::Instant)> =
                map.iter().map(|(k, (_, t))| (k.clone(), *t)).collect();
            entries.sort_by_key(|(_, t)| *t);
            let evict_count = self.max_entries / 4;
            for (key, _) in entries.into_iter().take(evict_count) {
                map.remove(&key);
            }
        }

        let now = std::time::Instant::now();
        let entry = map
            .entry(user_id.to_string())
            .or_insert_with(|| (Arc::new(RateLimiter::direct(self.quota)), now));
        entry.1 = now; // Update last-seen time
        entry.0.check().map_err(|_| ())
    }
}

/// Create a per-user rate limiter.
pub fn create_per_user_rate_limiter(requests_per_minute: u32) -> Arc<PerUserRateLimiter> {
    Arc::new(PerUserRateLimiter::new(requests_per_minute))
}

/// Axum middleware for per-user rate limiting on AI endpoints.
/// Extracts user ID from the JWT `sub` claim in the Authorization header.
pub async fn per_user_ai_rate_limit(
    State(state): State<Arc<AppState>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    // Extract user_id from JWT (best-effort, fall back to IP if no JWT)
    let user_id = extract_user_id(&request, &state).unwrap_or_else(|| "anonymous".to_string());

    match state.per_user_rate_limiter.check(&user_id).await {
        Ok(_) => next.run(request).await,
        Err(_) => (
            StatusCode::TOO_MANY_REQUESTS,
            "Rate limit exceeded for your account. Please try again later.",
        )
            .into_response(),
    }
}

/// Helper: extract user_id from JWT in the request.
fn extract_user_id(request: &Request<Body>, state: &AppState) -> Option<String> {
    let claims = decode_jwt_claims(request, state)?;
    claims
        .get("sub")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Shared helper: extract bearer token from Authorization header or cookie.
fn extract_token(request: &Request<Body>) -> Option<String> {
    request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
        .or_else(|| {
            request
                .headers()
                .get("cookie")
                .and_then(|v| v.to_str().ok())
                .and_then(|cookies| {
                    cookies.split(';').find_map(|c| {
                        let c = c.trim();
                        c.strip_prefix("nova_token=").map(|s| s.to_string())
                    })
                })
        })
}

/// Shared helper: decode JWT and return claims. Used by auth, admin, and rate-limit middleware.
fn decode_jwt_claims(request: &Request<Body>, state: &AppState) -> Option<serde_json::Value> {
    let token = extract_token(request)?;
    let data = jsonwebtoken::decode::<serde_json::Value>(
        &token,
        &jsonwebtoken::DecodingKey::from_secret(state.config.jwt_secret.as_bytes()),
        &jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256),
    )
    .ok()?;
    Some(data.claims)
}

/// Axum middleware that checks the AI rate limiter before proceeding.
pub async fn ai_rate_limit(
    State(state): State<Arc<AppState>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    match state.ai_rate_limiter.check() {
        Ok(_) => next.run(request).await,
        Err(_) => (
            StatusCode::TOO_MANY_REQUESTS,
            "AI rate limit exceeded. Please try again later.",
        )
            .into_response(),
    }
}

/// Auth guard middleware — rejects requests without a valid JWT.
/// Apply via `.layer(axum::middleware::from_fn_with_state(state.clone(), require_auth))`
pub async fn require_auth(
    State(state): State<Arc<AppState>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let Some(claims) = decode_jwt_claims(&request, &state) else {
        return (StatusCode::UNAUTHORIZED, "Authentication required").into_response();
    };

    // Check token type
    if claims.get("token_type").and_then(|v| v.as_str()) != Some("access") {
        return (StatusCode::UNAUTHORIZED, "Invalid token type").into_response();
    }

    next.run(request).await
}

/// Request validation middleware: rejects oversized or malformed Content-Type headers.
pub async fn validate_request(request: Request<Body>, next: Next) -> Response {
    // Validate Content-Type for POST/PUT requests with body
    let method = request.method().clone();
    if method == axum::http::Method::POST || method == axum::http::Method::PUT {
        if let Some(content_type) = request.headers().get("content-type") {
            let ct = content_type.to_str().unwrap_or("");
            // Only allow known content types
            if !ct.is_empty()
                && !ct.starts_with("application/json")
                && !ct.starts_with("multipart/form-data")
                && !ct.starts_with("application/octet-stream")
                && !ct.starts_with("text/")
                && !ct.starts_with("image/")
            {
                return (
                    StatusCode::UNSUPPORTED_MEDIA_TYPE,
                    "Unsupported Content-Type",
                )
                    .into_response();
            }
        }
    }

    next.run(request).await
}

/// Health-aware middleware: when database is unreachable, degrade gracefully.
/// Returns 503 for write operations but allows cached reads to proceed.
pub async fn graceful_degradation(
    State(state): State<Arc<AppState>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    // Quick pool health check (non-blocking)
    let pool_idle = state.db.num_idle();
    let pool_size = state.db.size();

    // If pool is completely exhausted, reject write operations
    if pool_idle == 0 && pool_size >= 20 {
        let method = request.method().clone();
        if method == axum::http::Method::POST
            || method == axum::http::Method::PUT
            || method == axum::http::Method::DELETE
        {
            tracing::warn!("Connection pool exhausted, rejecting write request");
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "Service temporarily unavailable. Please retry.",
            )
                .into_response();
        }
    }

    next.run(request).await
}

/// Response wrapper middleware: wraps successful JSON responses in a standard envelope.
/// Format: `{ "code": 200, "message": "SUCCESS", "timestamp": ..., "data": <original_body> }`
/// Error responses (status >= 400) are left unchanged since `ApiError::into_response` already wraps them.
/// Non-JSON responses (SSE, file downloads) are passed through untouched.
pub async fn wrap_response(request: Request<Body>, next: Next) -> Response {
    use http_body_util::BodyExt;

    let response = next.run(request).await;
    let status = response.status();

    // Don't wrap error responses (ApiError already produces the envelope)
    if status.is_client_error() || status.is_server_error() {
        return response;
    }

    // Don't wrap non-JSON responses (SSE, file downloads, etc.)
    let is_json = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|ct| ct.starts_with("application/json"))
        .unwrap_or(false);

    if !is_json {
        return response;
    }

    // Extract body bytes
    let (parts, body) = response.into_parts();
    let bytes = match body.collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(_) => return Response::from_parts(parts, Body::empty()),
    };

    // Parse original JSON
    let data: serde_json::Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => return Response::from_parts(parts, Body::from(bytes)),
    };

    // If the response already has our envelope format (has "code" + "timestamp"), skip wrapping
    if data.is_object() && data.get("code").is_some() && data.get("timestamp").is_some() {
        return Response::from_parts(parts, Body::from(bytes));
    }

    let timestamp = chrono::Utc::now().timestamp();
    let wrapped = serde_json::json!({
        "code": status.as_u16(),
        "message": "SUCCESS",
        "timestamp": timestamp,
        "data": data,
    });

    let wrapped_bytes = serde_json::to_vec(&wrapped).unwrap_or_else(|_| bytes.to_vec());

    // Preserve original headers (e.g. Set-Cookie) and just replace body + content-type
    let mut new_parts = parts;
    new_parts.headers.insert(
        axum::http::header::CONTENT_TYPE,
        "application/json".parse().unwrap(),
    );
    // Update content-length to match the new body
    new_parts.headers.insert(
        axum::http::header::CONTENT_LENGTH,
        wrapped_bytes.len().into(),
    );
    Response::from_parts(new_parts, Body::from(wrapped_bytes))
}
