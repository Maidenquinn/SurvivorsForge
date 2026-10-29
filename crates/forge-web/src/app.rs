//! The router of the website.

use crate::api_client::ApiClient;
use crate::config::Config;
use crate::csrf::{self, CsrfToken};
use crate::error::{WebError, WebResult, error_page};
use crate::session::{self, SESSION_COOKIE};
use crate::templates::{IndexPage, Visitor, render};
use axum::extract::{FromRef, Query, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router, middleware};
use axum_extra::extract::PrivateCookieJar;
use axum_extra::extract::cookie::Key;
use serde::Deserialize;
use serde_json::json;
use sha2::Digest;
use std::sync::Arc;
use tower_http::set_header::SetResponseHeaderLayer;

pub const CSP: &str = "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' https://avatars.githubusercontent.com; \
                       form-action 'self'; frame-ancestors 'none'; base-uri 'none'; connect-src 'self'";

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub api: ApiClient,
    pub key: Key,
}

impl AppState {
    pub fn new(config: Config) -> AppState {
        // Key wants 64 bytes; SHA-512 stretches COOKIE_KEY to exactly that.
        let key = Key::from(&sha2::Sha512::digest(config.cookie_key.as_bytes()));
        AppState { api: ApiClient::new(&config.api_internal_url), config: Arc::new(config), key }
    }

    pub fn secure_cookies(&self) -> bool {
        self.config.public_web_url.starts_with("https://")
    }
}

impl FromRef<AppState> for Key {
    fn from_ref(state: &AppState) -> Key {
        state.key.clone()
    }
}

/// The logged-in visitor and their API token, if there is a session.
pub async fn visitor(state: &AppState, jar: &PrivateCookieJar) -> WebResult<Option<(Visitor, String)>> {
    let Some(token) = session::token(jar) else { return Ok(None) };
    let me = state.api.get("/v1/me", Some(&token)).await?;
    Ok(Some((Visitor::from_me(&me), token)))
}

pub fn router(state: AppState) -> Router {
    let pages = Router::new()
        .route("/", get(index))
        .route("/login", get(login))
        .route("/auth/callback", get(callback))
        .route("/logout", post(logout))
        .route("/static/htmx.min.js", get(|| async { asset("text/javascript", include_str!("../static/htmx.min.js")) }))
        .route("/static/app.css", get(|| async { asset("text/css", include_str!("../static/app.css")) }))
        .fallback(|| async { WebError::NotFound })
        .layer(middleware::from_fn_with_state(state.clone(), csrf::middleware));
    Router::new()
        .route("/healthz", get(healthz))
        .merge(pages)
        .with_state(state)
        .layer(SetResponseHeaderLayer::if_not_present(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(CSP)))
        .layer(SetResponseHeaderLayer::overriding(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY")))
        .layer(SetResponseHeaderLayer::overriding(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff")))
        .layer(SetResponseHeaderLayer::overriding(header::REFERRER_POLICY, HeaderValue::from_static("strict-origin-when-cross-origin")))
}

fn asset(content_type: &'static str, body: &'static str) -> Response {
    ([(header::CONTENT_TYPE, content_type), (header::CACHE_CONTROL, "public, max-age=86400")], body).into_response()
}

async fn healthz(State(state): State<AppState>) -> Response {
    if state.api.healthy().await {
        Json(json!({ "status": "ok", "api": "ok" })).into_response()
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, Json(json!({ "status": "degraded", "api": "down" }))).into_response()
    }
}

async fn index(State(state): State<AppState>, jar: PrivateCookieJar, Extension(csrf): Extension<CsrfToken>) -> WebResult<Html<String>> {
    let user = visitor(&state, &jar).await?.map(|(v, _)| v);
    Ok(render(&IndexPage { user, csrf: csrf.0 }))
}

async fn login(State(state): State<AppState>) -> Redirect {
    let url = reqwest::Url::parse_with_params(
        &format!("{}/v1/auth/github/start", state.config.public_api_url),
        &[("return_to", state.config.callback_url())],
    )
    .expect("valid API URL");
    Redirect::to(url.as_str())
}

#[derive(Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    error: Option<String>,
}

// ponytail: no own login state, so a forged callback could log a visitor in as someone else; add a
// nonce cookie bound into return_to if that ever matters.
async fn callback(State(state): State<AppState>, jar: PrivateCookieJar, Query(query): Query<CallbackQuery>) -> Response {
    let message = match (query.code, query.error.as_deref()) {
        (Some(code), None) => {
            let body = json!({ "code": code, "return_to": state.config.callback_url() });
            match state.api.post("/v1/auth/exchange", None, &body).await {
                Ok(answer) => {
                    let token = answer["token"].as_str().unwrap_or_default().to_string();
                    let jar = jar.add(session::cookie(SESSION_COOKIE, token, state.secure_cookies()));
                    return (jar, Redirect::to("/")).into_response();
                }
                Err(e) => e.message(),
            }
        }
        (_, Some("banned")) => "This account is banned.".into(),
        (_, Some("denied")) => "The login was cancelled on GitHub.".into(),
        _ => "The login did not work. Please try again.".into(),
    };
    error_page(StatusCode::BAD_REQUEST, "Login failed", &message)
}

async fn logout(State(state): State<AppState>, jar: PrivateCookieJar) -> Response {
    if let Some(token) = session::token(&jar) {
        let _ = state.api.post("/v1/auth/logout", Some(&token), &json!({})).await;
    }
    (jar.remove(session::removal(SESSION_COOKIE)), Redirect::to("/")).into_response()
}
