use crate::app::AppState;
use crate::auth::{CurrentUser, bearer, hash};
use crate::error::{ApiError, ApiResult};
use crate::scan_job::MAX_DOWNLOAD;
use crate::suggest;
use serde_json::{Value, json};
use axum::Json;
use axum::extract::{FromRequestParts, Path, Request, State};
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

fn github_down(e: crate::github::GitHubError) -> ApiError {
    tracing::warn!(error = %e, "GitHub request failed");
    ApiError::new(StatusCode::BAD_GATEWAY, "github_unavailable", "GitHub cannot be reached right now")
}

/// The public repos of the user's App installation that have no addon yet.
async fn my_repos_of(state: &AppState, user: &CurrentUser) -> ApiResult<Option<(i64, Vec<crate::github::types::Repo>)>> {
    let Some(installation) = state.github.installation_for_user(&user.login).await.map_err(github_down)? else { return Ok(None) };
    let repos = state.github.installation_repos(installation).await.map_err(github_down)?;
    let ids: Vec<i64> = repos.iter().map(|r| r.id).collect();
    let taken: Vec<i64> =
        sqlx::query_scalar("SELECT repo_id FROM addons WHERE repo_id = ANY($1) AND deleted_at IS NULL").bind(&ids).fetch_all(&state.pool).await?;
    let mut free: Vec<_> = repos.into_iter().filter(|r| !r.private && !taken.contains(&r.id)).collect();
    free.sort_by_key(|r| r.full_name.to_lowercase());
    Ok(Some((installation, free)))
}

/// `GET /v1/me/repos`: the repos the publish form offers.
pub async fn repos(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    let Some((_, repos)) = my_repos_of(&state, &user).await? else { return Ok(Json(json!({ "installed": false, "repos": [] }))) };
    let list: Vec<Value> = repos.iter().map(|r| json!({ "full_name": r.full_name, "description": r.description })).collect();
    Ok(Json(json!({ "installed": true, "repos": list })))
}

/// `GET /v1/me/repos/{owner}/{repo}/suggestion`: the publish form filled in from the newest
/// release with a zip, else the newest tag. Only for repos of the user's installation.
pub async fn suggestion(State(state): State<AppState>, user: CurrentUser, Path((owner, name)): Path<(String, String)>) -> ApiResult<Json<Value>> {
    let full_name = format!("{owner}/{name}");
    let (installation, repos) = my_repos_of(&state, &user).await?.ok_or_else(ApiError::not_found)?;
    let repo = repos.into_iter().find(|r| r.full_name.eq_ignore_ascii_case(&full_name)).ok_or_else(ApiError::not_found)?;
    let releases = state.github.releases(Some(installation), repo.id).await.map_err(github_down)?;
    let (source_type, pattern, url, read_from) = match suggest::pick_release(&releases) {
        Some((release, asset)) => (
            "release",
            suggest::asset_pattern(&asset.name, &release.tag_name),
            Some(asset.browser_download_url.clone()),
            format!("release {}, {}", release.tag_name, asset.name),
        ),
        None => match suggest::newest_tag(&state.github.tags(Some(installation), repo.id).await.map_err(github_down)?) {
            Some(tag) => ("tag", String::new(), Some(format!("https://codeload.github.com/{}/zip/{}", repo.full_name, tag.name)), format!("tag {}", tag.name)),
            None => ("release", String::new(), None, "no release or tag yet".to_string()),
        },
    };
    // Without the zip the form still gets the names; the folders are then left to the author.
    let zip = match url {
        Some(url) => state.github.download(&url, MAX_DOWNLOAD).await.ok(),
        None => None,
    };
    let mut answer = serde_json::to_value(suggest::suggest(&repo, source_type, &pattern, zip.as_deref(), read_from)).map_err(|_| ApiError::internal())?;
    answer["repo"] = json!(repo.full_name);
    Ok(Json(answer))
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
