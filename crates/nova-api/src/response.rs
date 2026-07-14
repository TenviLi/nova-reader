use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

/// Standard API response envelope.
///
/// All endpoints should eventually migrate to this format:
/// ```json
/// { "data": <T>, "meta": { "total": 100, "page": 1, ... } }
/// ```
///
/// Error responses remain:
/// ```json
/// { "error": { "code": 400, "message": "...", "retryable": false } }
/// ```
#[derive(Debug, Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub data: T,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<ResponseMeta>,
}

/// Response metadata for paginated or enriched responses.
#[derive(Debug, Serialize, Default)]
pub struct ResponseMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_page: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_pages: Option<i32>,
    /// Request processing time in milliseconds (for observability).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub took_ms: Option<u64>,
}

impl<T: Serialize> ApiResponse<T> {
    /// Wrap a value in the standard envelope.
    pub fn ok(data: T) -> Self {
        Self { data, meta: None }
    }

    /// Wrap a value with pagination metadata.
    pub fn paginated(data: T, total: i64, page: i32, per_page: i32) -> Self {
        let total_pages = ((total as f64) / (per_page as f64)).ceil() as i32;
        Self {
            data,
            meta: Some(ResponseMeta {
                total: Some(total),
                page: Some(page),
                per_page: Some(per_page),
                total_pages: Some(total_pages),
                took_ms: None,
            }),
        }
    }

    /// Add timing metadata.
    pub fn with_timing(mut self, ms: u64) -> Self {
        self.meta.get_or_insert_with(Default::default).took_ms = Some(ms);
        self
    }
}

impl<T: Serialize> IntoResponse for ApiResponse<T> {
    fn into_response(self) -> Response {
        (StatusCode::OK, Json(self)).into_response()
    }
}

/// Created response (201) with envelope.
pub struct Created<T: Serialize>(pub ApiResponse<T>);

impl<T: Serialize> IntoResponse for Created<T> {
    fn into_response(self) -> Response {
        (StatusCode::CREATED, Json(self.0)).into_response()
    }
}
