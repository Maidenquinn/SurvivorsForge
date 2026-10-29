//! The API talks to GitHub as a GitHub App: App JWT for `/app` and `/repos/.../installation`,
//! installation tokens (5000 requests/h per installation) for everything else.
//!
//! `GitHubApi` is the seam for tests; `GitHubClient` is the real thing.

pub mod app_auth;
pub mod types;

use async_trait::async_trait;
use chrono::{DateTime, Duration, TimeZone, Utc};
use jsonwebtoken::EncodingKey;
use reqwest::StatusCode;
use reqwest::header::{ACCEPT, ETAG, HeaderMap, IF_NONE_MATCH, USER_AGENT};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use types::{Installation, InstallationToken, Permission, Release, Repo, Tag, User};

#[derive(Debug, Clone, PartialEq)]
pub enum GitHubError {
    NotFound,
    RateLimited { reset_at: Option<DateTime<Utc>> },
    TooLarge,
    Http(u16),
    Network(String),
    Parse(String),
    Auth(String),
}

impl std::fmt::Display for GitHubError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GitHubError::NotFound => write!(f, "not found on GitHub"),
            GitHubError::RateLimited { reset_at } => write!(f, "GitHub rate limit reached (reset {reset_at:?})"),
            GitHubError::TooLarge => write!(f, "the download is larger than allowed"),
            GitHubError::Http(code) => write!(f, "GitHub answered with {code}"),
            GitHubError::Network(e) => write!(f, "cannot reach GitHub: {e}"),
            GitHubError::Parse(e) => write!(f, "unexpected answer from GitHub: {e}"),
            GitHubError::Auth(e) => write!(f, "GitHub App authentication failed: {e}"),
        }
    }
}

pub type GitHubResult<T> = Result<T, GitHubError>;

#[async_trait]
pub trait GitHubApi: Send + Sync {
    /// The installation of the App on `owner/name`; `None` when the App is not installed there.
    async fn installation_for_repo(&self, full_name: &str) -> GitHubResult<Option<i64>>;
    async fn repo(&self, installation: i64, repo_id: i64) -> GitHubResult<Repo>;
    async fn repo_by_name(&self, installation: i64, full_name: &str) -> GitHubResult<Repo>;
    /// `admin`, `maintain`, `write`, `triage`, `read` or `none`.
    async fn user_permission(&self, installation: i64, repo_id: i64, login: &str) -> GitHubResult<String>;
    async fn user_by_login(&self, installation: i64, login: &str) -> GitHubResult<Option<User>>;
    /// Newest first.
    async fn releases(&self, installation: i64, repo_id: i64) -> GitHubResult<Vec<Release>>;
    async fn release(&self, installation: i64, repo_id: i64, release_id: i64) -> GitHubResult<Option<Release>>;
    /// Newest first.
    async fn tags(&self, installation: i64, repo_id: i64) -> GitHubResult<Vec<Tag>>;
    /// Reads at most `max_bytes`; more is `TooLarge`, whatever `Content-Length` says.
    async fn download(&self, url: &str, max_bytes: u64) -> GitHubResult<Vec<u8>>;
}

pub type Clock = Arc<dyn Fn() -> DateTime<Utc> + Send + Sync>;

pub struct GitHubClient {
    http: reqwest::Client,
    api_url: String,
    app_id: String,
    key: EncodingKey,
    clock: Clock,
    tokens: Mutex<HashMap<i64, InstallationToken>>,
    /// URL -> (ETag, body) of the last `200`, for conditional requests.
    etags: Mutex<HashMap<String, (String, String)>>,
}

/// Installation tokens are renewed this long before they expire.
const TOKEN_MARGIN: Duration = Duration::minutes(5);

impl GitHubClient {
    pub fn new(api_url: &str, app_id: &str, key: EncodingKey, clock: Clock) -> GitHubClient {
        GitHubClient {
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()
                .expect("HTTP client"),
            api_url: api_url.trim_end_matches('/').to_string(),
            app_id: app_id.to_string(),
            key,
            clock,
            tokens: Mutex::new(HashMap::new()),
            etags: Mutex::new(HashMap::new()),
        }
    }

    fn app_jwt(&self) -> GitHubResult<String> {
        app_auth::app_jwt(&self.app_id, &self.key, (self.clock)()).map_err(|e| GitHubError::Auth(e.to_string()))
    }

    async fn installation_token(&self, installation: i64) -> GitHubResult<String> {
        let now = (self.clock)();
        if let Some(token) = self.tokens.lock().unwrap().get(&installation)
            && token.expires_at - TOKEN_MARGIN > now
        {
            return Ok(token.token.clone());
        }
        let url = format!("{}/app/installations/{installation}/access_tokens", self.api_url);
        let response = self.request(self.http.post(&url), &self.app_jwt()?).send().await.map_err(network)?;
        let token: InstallationToken = parse(check(response).await?).await?;
        self.tokens.lock().unwrap().insert(installation, token.clone());
        Ok(token.token)
    }

    fn request(&self, builder: reqwest::RequestBuilder, bearer: &str) -> reqwest::RequestBuilder {
        builder
            .bearer_auth(bearer)
            .header(ACCEPT, "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header(USER_AGENT, "SurvivorsForge")
    }

    /// GET with an installation token and `If-None-Match`; a `304` answers from the cache.
    async fn get_json<T: serde::de::DeserializeOwned>(&self, installation: i64, path: &str) -> GitHubResult<T> {
        let url = format!("{}{path}", self.api_url);
        let token = self.installation_token(installation).await?;
        let mut builder = self.request(self.http.get(&url), &token);
        let cached = self.etags.lock().unwrap().get(&url).cloned();
        if let Some((etag, _)) = &cached {
            builder = builder.header(IF_NONE_MATCH, etag);
        }
        let response = builder.send().await.map_err(network)?;
        let body = if let Some((_, body)) = cached.filter(|_| response.status() == StatusCode::NOT_MODIFIED) {
            body
        } else {
            let response = check(response).await?;
            let etag = response.headers().get(ETAG).and_then(|v| v.to_str().ok()).map(str::to_string);
            let body = response.text().await.map_err(network)?;
            if let Some(etag) = etag {
                self.etags.lock().unwrap().insert(url, (etag, body.clone()));
            }
            body
        };
        serde_json::from_str(&body).map_err(|e| GitHubError::Parse(e.to_string()))
    }
}

fn network(e: reqwest::Error) -> GitHubError {
    GitHubError::Network(e.to_string())
}

async fn parse<T: serde::de::DeserializeOwned>(response: reqwest::Response) -> GitHubResult<T> {
    response.json().await.map_err(|e| GitHubError::Parse(e.to_string()))
}

/// Maps error statuses; a `403`/`429` with no requests left is a rate limit.
async fn check(response: reqwest::Response) -> GitHubResult<reqwest::Response> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    Err(status_error(status, response.headers()))
}

fn status_error(status: StatusCode, headers: &HeaderMap) -> GitHubError {
    let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    let exhausted = header("x-ratelimit-remaining") == Some("0");
    match status {
        StatusCode::NOT_FOUND => GitHubError::NotFound,
        StatusCode::TOO_MANY_REQUESTS => GitHubError::RateLimited { reset_at: reset_at(header("x-ratelimit-reset")) },
        StatusCode::FORBIDDEN if exhausted => GitHubError::RateLimited { reset_at: reset_at(header("x-ratelimit-reset")) },
        other => GitHubError::Http(other.as_u16()),
    }
}

fn reset_at(epoch: Option<&str>) -> Option<DateTime<Utc>> {
    epoch?.parse::<i64>().ok().and_then(|s| Utc.timestamp_opt(s, 0).single())
}

fn optional<T>(result: GitHubResult<T>) -> GitHubResult<Option<T>> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(GitHubError::NotFound) => Ok(None),
        Err(e) => Err(e),
    }
}

#[async_trait]
impl GitHubApi for GitHubClient {
    async fn installation_for_repo(&self, full_name: &str) -> GitHubResult<Option<i64>> {
        let url = format!("{}/repos/{full_name}/installation", self.api_url);
        let response = self.request(self.http.get(&url), &self.app_jwt()?).send().await.map_err(network)?;
        let installation: GitHubResult<Installation> = match check(response).await {
            Ok(response) => parse(response).await,
            Err(e) => Err(e),
        };
        Ok(optional(installation)?.map(|i| i.id))
    }

    async fn repo(&self, installation: i64, repo_id: i64) -> GitHubResult<Repo> {
        self.get_json(installation, &format!("/repositories/{repo_id}")).await
    }

    async fn repo_by_name(&self, installation: i64, full_name: &str) -> GitHubResult<Repo> {
        self.get_json(installation, &format!("/repos/{full_name}")).await
    }

    async fn user_permission(&self, installation: i64, repo_id: i64, login: &str) -> GitHubResult<String> {
        let path = format!("/repositories/{repo_id}/collaborators/{login}/permission");
        match self.get_json::<Permission>(installation, &path).await {
            Ok(p) => Ok(p.permission),
            Err(GitHubError::NotFound) => Ok("none".into()),
            Err(e) => Err(e),
        }
    }

    async fn user_by_login(&self, installation: i64, login: &str) -> GitHubResult<Option<User>> {
        optional(self.get_json(installation, &format!("/users/{login}")).await)
    }

    async fn releases(&self, installation: i64, repo_id: i64) -> GitHubResult<Vec<Release>> {
        // ponytail: newest 100 releases only; paginate if an addon ever keeps more.
        self.get_json(installation, &format!("/repositories/{repo_id}/releases?per_page=100")).await
    }

    async fn release(&self, installation: i64, repo_id: i64, release_id: i64) -> GitHubResult<Option<Release>> {
        optional(self.get_json(installation, &format!("/repositories/{repo_id}/releases/{release_id}")).await)
    }

    async fn tags(&self, installation: i64, repo_id: i64) -> GitHubResult<Vec<Tag>> {
        self.get_json(installation, &format!("/repositories/{repo_id}/tags?per_page=100")).await
    }

    async fn download(&self, url: &str, max_bytes: u64) -> GitHubResult<Vec<u8>> {
        let response = self.http.get(url).header(USER_AGENT, "SurvivorsForge").send().await.map_err(network)?;
        let mut response = check(response).await?;
        if response.content_length().is_some_and(|len| len > max_bytes) {
            return Err(GitHubError::TooLarge);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(network)? {
            if bytes.len() as u64 + chunk.len() as u64 > max_bytes {
                return Err(GitHubError::TooLarge);
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }
}
