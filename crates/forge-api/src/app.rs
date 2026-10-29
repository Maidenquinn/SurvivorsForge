//! The HTTP router of the API.

use crate::config::Config;
use crate::auth::oauth;
use crate::error::ApiError;
use crate::github::{Clock, GitHubApi};
use crate::routes;
use crate::rate_limit::{self, RateLimiter};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
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
    pub github: Arc<dyn GitHubApi>,
    pub clock: Clock,
}

pub fn router(state: AppState) -> Router {
    let config = state.config.clone();
    let routes = Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/auth/github/start", get(oauth::start))
        .route("/v1/auth/github/callback", get(oauth::callback))
        .route("/v1/auth/exchange", post(oauth::exchange))
        .route("/v1/auth/logout", post(routes::me::logout))
        .route("/v1/me", get(routes::me::me))
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
