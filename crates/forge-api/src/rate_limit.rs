//! Per-IP request limit with a fixed one-minute window.

use crate::error::ApiError;
use axum::extract::{ConnectInfo, Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Instant;

pub use forge_core::rate_limit::RateLimiter;

pub async fn middleware(State(limiter): State<Arc<RateLimiter>>, request: Request, next: Next) -> Response {
    let peer = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip())
        .unwrap_or(IpAddr::from([0, 0, 0, 0]));
    let forwarded = request.headers().get("x-forwarded-for").and_then(|v| v.to_str().ok());
    let ip = limiter.client_ip(peer, forwarded);
    if !limiter.allow(ip, Instant::now()) {
        return ApiError::new(StatusCode::TOO_MANY_REQUESTS, "rate_limited", "Too many requests, try again in a minute")
            .into_response();
    }
    next.run(request).await
}
