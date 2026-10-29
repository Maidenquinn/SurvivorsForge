//! The HTTP router of the API.

use crate::config::Config;
use crate::error::ApiError;
use crate::rate_limit::{self, RateLimiter};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router, extract::State, middleware};
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::set_header::SetResponseHeaderLayer;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Arc<Config>,
}

pub fn router(state: AppState) -> Router {
    let config = state.config.clone();
    let routes = Router::new()
        .route("/healthz", get(healthz))
        .fallback(|| async { ApiError::not_found() })
        .with_state(state);
    with_layers(routes, &config)
}

/// The layers every response passes through: panic guard, rate limit, default headers.
pub fn with_layers(routes: Router, config: &Config) -> Router {
    let limiter = Arc::new(RateLimiter::new(config.rate_limit_per_minute, config.trusted_proxies.clone()));
    routes
        .layer(CatchPanicLayer::custom(|_| ApiError::internal().into_response()))
        .layer(middleware::from_fn_with_state(limiter, rate_limit::middleware))
        .layer(SetResponseHeaderLayer::if_not_present(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff")))
        .layer(SetResponseHeaderLayer::if_not_present(header::CACHE_CONTROL, HeaderValue::from_static("no-store")))
}

async fn healthz(State(state): State<AppState>) -> Response {
    let ping = tokio::time::timeout(Duration::from_secs(2), sqlx::query("SELECT 1").execute(&state.pool)).await;
    if matches!(ping, Ok(Ok(_))) {
        Json(serde_json::json!({ "status": "ok", "db": "ok" })).into_response()
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({ "status": "degraded", "db": "down" }))).into_response()
    }
}
