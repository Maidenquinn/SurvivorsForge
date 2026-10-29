//! The one error type of the API. Every error leaves as `{"error": "<code>", "message": "..."}`.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> ApiError {
        ApiError { status, code, message: message.into() }
    }

    pub fn not_found() -> ApiError {
        ApiError::new(StatusCode::NOT_FOUND, "not_found", "Not found")
    }

    pub fn internal() -> ApiError {
        ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", "Internal server error")
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = Json(serde_json::json!({ "error": self.code, "message": self.message }));
        (self.status, body).into_response()
    }
}

/// Database errors are logged, never shown to the client.
impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> ApiError {
        tracing::error!(error = %e, "database error");
        ApiError::internal()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
