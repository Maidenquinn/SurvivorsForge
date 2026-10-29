use crate::app::AppState;
use crate::auth::{CurrentUser, bearer, hash};
use crate::error::{ApiError, ApiResult};
use serde_json::{Value, json};
use axum::Json;
use axum::extract::{FromRequestParts, Request, State};
use axum::http::StatusCode;

pub async fn me(user: CurrentUser) -> Json<CurrentUser> {
    Json(user)
}

/// `GET /v1/me/installation`: whether the App is installed on the user's own account, and where
/// they choose its repos. The website hides the install step when it is.
pub async fn installation(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    let installation = state.github.installation_for_user(&user.login).await.map_err(|e| {
        tracing::warn!(error = %e, "cannot read the App installation");
        ApiError::new(StatusCode::BAD_GATEWAY, "github_unavailable", "GitHub cannot be reached right now")
    })?;
    let manage_url = installation.map(|id| format!("https://github.com/settings/installations/{id}"));
    Ok(Json(json!({ "installed": installation.is_some(), "manage_url": manage_url })))
}

/// Ends the session of the token in the request.
pub async fn logout(State(state): State<AppState>, request: Request) -> ApiResult<StatusCode> {
    let (mut parts, _) = request.into_parts();
    CurrentUser::from_request_parts(&mut parts, &state).await?;
    if let Some(token) = bearer(&parts) {
        sqlx::query("DELETE FROM sessions WHERE token_hash = $1").bind(hash(token)).execute(&state.pool).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}
