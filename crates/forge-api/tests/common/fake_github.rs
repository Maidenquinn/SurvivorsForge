//! An in-memory GitHub for tests. Counts every call, so tests can prove that nothing hit GitHub.

use async_trait::async_trait;
use forge_api::github::types::{Release, Repo, Tag, User};
use forge_api::github::{GitHubApi, GitHubError, GitHubResult};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};

#[derive(Default)]
pub struct FakeData {
    /// OAuth code -> user.
    pub oauth_users: HashMap<String, User>,
    /// `owner/name` -> installation ID.
    pub installations: HashMap<String, i64>,
    pub repos: HashMap<i64, Repo>,
    /// (repo ID, login) -> permission.
    pub permissions: HashMap<(i64, String), String>,
    /// Login (lowercase) -> user.
    pub users: HashMap<String, User>,
    pub releases: HashMap<i64, Vec<Release>>,
    pub tags: HashMap<i64, Vec<Tag>>,
    /// URL -> bytes.
    pub downloads: HashMap<String, Vec<u8>>,
}

#[derive(Default)]
pub struct FakeGitHub {
    data: Mutex<FakeData>,
    calls: AtomicUsize,
}

impl FakeGitHub {
    pub fn data(&self) -> MutexGuard<'_, FakeData> {
        self.data.lock().unwrap()
    }

    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    fn call(&self) -> MutexGuard<'_, FakeData> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.data()
    }

    /// Adds a public repo `owner/name` with the App installed and `admin_login` as admin.
    pub fn add_repo(&self, id: i64, full_name: &str, admin_login: &str) {
        let mut data = self.data();
        let owner = full_name.split('/').next().unwrap().to_string();
        data.repos.insert(
            id,
            Repo { id, full_name: full_name.into(), private: false, owner: forge_api::github::types::Owner { login: owner } },
        );
        data.installations.insert(full_name.to_lowercase(), 1);
        data.permissions.insert((id, admin_login.to_lowercase()), "admin".into());
    }
}

#[async_trait]
impl GitHubApi for FakeGitHub {
    async fn installation_for_repo(&self, full_name: &str) -> GitHubResult<Option<i64>> {
        Ok(self.call().installations.get(&full_name.to_lowercase()).copied())
    }

    async fn repo(&self, _installation: i64, repo_id: i64) -> GitHubResult<Repo> {
        self.call().repos.get(&repo_id).cloned().ok_or(GitHubError::NotFound)
    }

    async fn repo_by_name(&self, _installation: i64, full_name: &str) -> GitHubResult<Repo> {
        let data = self.call();
        data.repos.values().find(|r| r.full_name.eq_ignore_ascii_case(full_name)).cloned().ok_or(GitHubError::NotFound)
    }

    async fn user_permission(&self, _installation: i64, repo_id: i64, login: &str) -> GitHubResult<String> {
        Ok(self.call().permissions.get(&(repo_id, login.to_lowercase())).cloned().unwrap_or_else(|| "none".into()))
    }

    async fn user_by_login(&self, _installation: i64, login: &str) -> GitHubResult<Option<User>> {
        Ok(self.call().users.get(&login.to_lowercase()).cloned())
    }

    async fn releases(&self, _installation: i64, repo_id: i64) -> GitHubResult<Vec<Release>> {
        Ok(self.call().releases.get(&repo_id).cloned().unwrap_or_default())
    }

    async fn release(&self, _installation: i64, repo_id: i64, release_id: i64) -> GitHubResult<Option<Release>> {
        Ok(self.call().releases.get(&repo_id).and_then(|rs| rs.iter().find(|r| r.id == release_id).cloned()))
    }

    async fn tags(&self, _installation: i64, repo_id: i64) -> GitHubResult<Vec<Tag>> {
        Ok(self.call().tags.get(&repo_id).cloned().unwrap_or_default())
    }

    async fn oauth_user(&self, code: &str, _verifier: &str, _redirect_uri: &str) -> GitHubResult<User> {
        self.call().oauth_users.get(code).cloned().ok_or_else(|| GitHubError::Auth("bad_verification_code".into()))
    }

    async fn download(&self, url: &str, max_bytes: u64) -> GitHubResult<Vec<u8>> {
        let bytes = self.call().downloads.get(url).cloned().ok_or(GitHubError::NotFound)?;
        if bytes.len() as u64 > max_bytes {
            return Err(GitHubError::TooLarge);
        }
        Ok(bytes)
    }
}
