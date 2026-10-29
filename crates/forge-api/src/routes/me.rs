use crate::app::AppState;
use crate::auth::{CurrentUser, bearer, hash};
use crate::error::ApiResult;
use axum::Json;
use axum::extract::{FromRequestParts, Request, State};
use axum::http::StatusCode;

pub async fn me(user: CurrentUser) -> Json<CurrentUser> {
    Json(user)
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
