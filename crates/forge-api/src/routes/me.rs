use crate::app::AppState;
use crate::auth::{CurrentUser, bearer, hash};
use crate::error::{ApiError, ApiResult};
use crate::github::GitHubError;
use serde::Deserialize;
use crate::scan_job::MAX_DOWNLOAD;
use crate::suggest;
use serde_json::{Value, json};
use axum::Json;
use axum::extract::{FromRequestParts, Path, Query, Request, State};
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

#[derive(Deserialize, Default)]
pub struct RepoSearch {
    owner: Option<String>,
    #[serde(default)]
    q: String,
}

/// The account whose repos the publish form offers: the user's own; admins may name any.
fn repo_owner(user: &CurrentUser, wanted: Option<&str>) -> ApiResult<String> {
    match wanted.map(str::trim).filter(|o| !o.is_empty()) {
        None => Ok(user.login.clone()),
        Some(owner) if !owner.eq_ignore_ascii_case(&user.login) && !user.is_admin() => {
            Err(ApiError::new(StatusCode::FORBIDDEN, "not_your_account", "You can publish repos of your own account only"))
        }
        Some(owner) if owner.len() <= 39 && owner.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') => Ok(owner.to_string()),
        Some(_) => Err(ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, "invalid_owner", "Not a GitHub account name")),
    }
}

/// `GET /v1/me/repos?owner=&q=`: up to ten public repos of the account whose name contains `q`,
/// without the ones that already have an addon. Read from GitHub's public list, so the App need
/// not be installed yet; publishing still needs it.
pub async fn repos(State(state): State<AppState>, user: CurrentUser, Query(search): Query<RepoSearch>) -> ApiResult<Json<Value>> {
    let owner = repo_owner(&user, search.owner.as_deref())?;
    let repos = match state.github.user_repos(&owner).await {
        Ok(repos) => repos,
        Err(GitHubError::NotFound) => Vec::new(),
        Err(e) => return Err(github_down(e)),
    };
    let ids: Vec<i64> = repos.iter().map(|r| r.id).collect();
    let taken: Vec<i64> =
        sqlx::query_scalar("SELECT repo_id FROM addons WHERE repo_id = ANY($1) AND deleted_at IS NULL").bind(&ids).fetch_all(&state.pool).await?;
    let q = search.q.trim().to_lowercase();
    let name = |r: &crate::github::types::Repo| r.full_name.rsplit('/').next().unwrap_or_default().to_lowercase();
    let list: Vec<Value> = repos
        .iter()
        .filter(|r| !r.private && !taken.contains(&r.id) && name(r).contains(&q))
        .take(10)
        .map(|r| json!({ "full_name": r.full_name, "description": r.description }))
        .collect();
    Ok(Json(json!({ "owner": owner, "repos": list })))
}

/// `GET /v1/me/repos/{owner}/{repo}/suggestion`: the publish form filled in from the newest
/// release with a zip, else the newest tag, all read from public GitHub data.
pub async fn suggestion(State(state): State<AppState>, user: CurrentUser, Path((owner, name)): Path<(String, String)>) -> ApiResult<Json<Value>> {
    repo_owner(&user, Some(&owner))?;
    let repo = match state.github.repo_by_name(None, &format!("{owner}/{name}")).await {
        Ok(repo) if !repo.private => repo,
        Ok(_) | Err(GitHubError::NotFound) => return Err(ApiError::not_found()),
        Err(e) => return Err(github_down(e)),
    };
    let releases = state.github.releases(None, repo.id).await.map_err(github_down)?;
    let (source_type, pattern, url, read_from) = match suggest::pick_release(&releases) {
        Some((release, asset)) => (
            "release",
            suggest::asset_pattern(&asset.name, &release.tag_name),
            Some(asset.browser_download_url.clone()),
            format!("release {}, {}", release.tag_name, asset.name),
        ),
        None => match suggest::newest_tag(&state.github.tags(None, repo.id).await.map_err(github_down)?) {
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
