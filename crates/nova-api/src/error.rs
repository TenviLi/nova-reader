use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

/// API error type that implements `IntoResponse` for Axum.
pub struct ApiError(nova_core::Error);

impl ApiError {
    /// Resource not found with a message.
    #[allow(non_snake_case)]
    pub fn NotFound(msg: impl Into<String>) -> Self {
        Self(nova_core::Error::NotFound {
            entity: "resource",
            id: msg.into(),
        })
    }

    /// Internal server error.
    #[allow(non_snake_case)]
    pub fn Internal(msg: impl Into<String>) -> Self {
        Self(nova_core::Error::Internal(msg.into()))
    }

    /// Service unavailable (503) - used for AI offline fallback.
    #[allow(non_snake_case)]
    pub fn ServiceUnavailable(msg: impl Into<String>) -> Self {
        Self(nova_core::Error::AiService(msg.into()))
    }

    /// Resource not found (convenience lowercase).
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg)
    }

    /// Internal error (convenience lowercase).
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg)
    }

    /// Bad request / validation error.
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self(nova_core::Error::Validation(msg.into()))
    }

    /// Unauthorized access.
    pub fn unauthorized() -> Self {
        Self(nova_core::Error::Unauthorized)
    }

    /// Forbidden access.
    pub fn forbidden() -> Self {
        Self(nova_core::Error::Forbidden)
    }
}

impl From<nova_core::Error> for ApiError {
    fn from(err: nova_core::Error) -> Self {
        Self(err)
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        Self(nova_core::Error::Database(err))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match &self.0 {
            nova_core::Error::Database(error) if is_serialization_conflict(error) => {
                StatusCode::CONFLICT
            }
            // AI service errors → 503 Service Unavailable
            nova_core::Error::AiService(_) | nova_core::Error::VectorDb(_) => {
                StatusCode::SERVICE_UNAVAILABLE
            }
            other => StatusCode::from_u16(other.status_code())
                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
        };

        // Sanitize error messages: don't expose internal details to clients
        let message = match &self.0 {
            nova_core::Error::Database(error) if is_serialization_conflict(error) => {
                "Concurrent content update; retry the request".to_string()
            }
            nova_core::Error::Database(_) => "Database operation failed".to_string(),
            nova_core::Error::Redis(_) => "Cache service unavailable".to_string(),
            nova_core::Error::Io(_) => "File operation failed".to_string(),
            nova_core::Error::Internal(_) => "Internal server error".to_string(),
            nova_core::Error::AiService(msg) => msg.clone(), // Pass through AI offline messages to user
            other => other.to_string(),
        };

        // Log the full error for debugging (production would use tracing)
        if status.is_server_error() {
            tracing::error!("API error: {:?}", self.0);
        }

        let timestamp = chrono::Utc::now().timestamp();
        let code = status.as_u16();

        let body = json!({
            "code": code,
            "message": message,
            "timestamp": timestamp,
            "data": null,
            "error": {
                "code": code,
                "message": message,
                "retryable": self.0.is_retryable(),
            }
        });

        (status, Json(body)).into_response()
    }
}

fn is_serialization_conflict(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|database_error| database_error.code())
        .is_some_and(|code| matches!(code.as_ref(), "40001" | "55P03" | "40P01"))
}

/// Convenience type alias for API handler results.
pub type ApiResult<T> = std::result::Result<T, ApiError>;
