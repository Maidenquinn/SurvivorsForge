//! Thin GitHub client on github.com web endpoints; the parsers live in `forge_core::github_web`.

pub use forge_core::github_web::*;

use crate::{Error, Result};
use std::io::Read;
use std::path::Path;
use std::time::Duration;

const WEB: &str = "https://github.com";

/// What the update and detection logic needs from GitHub. `Client` is the real one; tests use fakes.
pub trait GitHub {
    /// `None` when the repo has no release.
    fn latest_release(&self, repo: &str) -> Result<Option<Release>>;
    /// The release of `tag`, also a pre-release; no assets when the tag has no release.
    fn release(&self, repo: &str, tag: &str) -> Result<Release>;
    /// Newest first.
    fn tags(&self, repo: &str) -> Result<Vec<Tag>>;
    fn default_branch(&self, repo: &str) -> Result<String>;
    fn commit_sha(&self, repo: &str, git_ref: &str) -> Result<String>;
}

/// Accepts `owner/repo` and GitHub URLs (`.git`, `/tree/...`, trailing slash), returns `owner/repo`.
pub fn normalize_repo(input: &str) -> Result<String> {
    let invalid = || Error::InvalidRepo(input.trim().to_string());
    let mut s = input.trim();
    for prefix in ["https://", "http://"] {
        s = s.strip_prefix(prefix).unwrap_or(s);
    }
    s = s.strip_prefix("www.").unwrap_or(s);
    let had_host = s.len() != input.trim().len() || s.starts_with("github.com/");
    if let Some(rest) = s.strip_prefix("github.com/") {
        s = rest;
    } else if had_host {
        return Err(invalid());
    }
    let mut parts = s.split('/').filter(|p| !p.is_empty());
    let (Some(owner), Some(repo)) = (parts.next(), parts.next()) else { return Err(invalid()) };
    if !had_host && parts.next().is_some() {
        return Err(invalid());
    }
    let repo = repo.strip_suffix(".git").unwrap_or(repo);
    let valid = |p: &str| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c));
    if !valid(owner) || !valid(repo) {
        return Err(invalid());
    }
    Ok(format!("{owner}/{repo}"))
}

pub fn map_error(status: u16) -> Error {
    match status {
        // github.com answers 401 (asks for login) for repos that don't exist or are private.
        401 | 404 => Error::NotFound,
        429 => Error::RateLimited,
        _ => Error::Http(status),
    }
}

pub fn source_zip_url(repo: &str, git_ref: &str) -> String {
    format!("https://codeload.github.com/{repo}/zip/{git_ref}")
}

pub struct Client {
    agent: ureq::Agent,
    /// Same settings, but returns redirects instead of following them.
    no_redirect: ureq::Agent,
}

impl Default for Client {
    fn default() -> Self {
        Client::new()
    }
}

impl Client {
    pub fn new() -> Client {
        let builder = || {
            ureq::AgentBuilder::new()
                .timeout_connect(Duration::from_secs(10))
                .timeout_read(Duration::from_secs(60))
                .user_agent(&format!("{}/{}", crate::APP_NAME, crate::APP_VERSION))
        };
        Client { agent: builder().build(), no_redirect: builder().redirects(0).build() }
    }

    fn call(req: ureq::Request) -> Result<ureq::Response> {
        match req.call() {
            Ok(resp) => Ok(resp),
            Err(ureq::Error::Status(code, _)) => Err(map_error(code)),
            Err(e) => Err(Error::Network(e.to_string())),
        }
    }

    pub fn get_text(&self, url: &str) -> Result<String> {
        Self::call(self.agent.get(url))?.into_string().map_err(|e| Error::Network(e.to_string()))
    }

    fn refs(&self, repo: &str) -> Result<Refs> {
        Ok(parse_refs(&self.get_text(&format!("{WEB}/{repo}.git/info/refs?service=git-upload-pack"))?))
    }

    /// Downloads into a temp file next to `dest` and renames it only when complete.
    pub fn download(&self, url: &str, dest: &Path) -> Result<()> {
        let resp = Self::call(self.agent.get(url))?;
        let tmp = dest.with_extension("part");
        if let Some(dir) = dest.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let result = (|| -> std::io::Result<()> {
            let mut file = std::fs::File::create(&tmp)?;
            std::io::copy(&mut resp.into_reader().take(512 * 1024 * 1024), &mut file)?;
            file.sync_all()
        })();
        if let Err(e) = result {
            let _ = std::fs::remove_file(&tmp);
            return Err(Error::Network(e.to_string()));
        }
        std::fs::rename(&tmp, dest)?;
        Ok(())
    }
}

impl GitHub for Client {
    fn latest_release(&self, repo: &str) -> Result<Option<Release>> {
        let resp = Self::call(self.no_redirect.get(&format!("{WEB}/{repo}/releases/latest")))?;
        let Some(tag) = resp.header("location").and_then(parse_latest_location) else { return Ok(None) };
        self.release(repo, &tag).map(Some)
    }

    fn release(&self, repo: &str, tag: &str) -> Result<Release> {
        let html = self.get_text(&format!("{WEB}/{repo}/releases/expanded_assets/{tag}"))?;
        Ok(Release { assets: parse_assets(&html, WEB, repo, tag), tag: tag.to_string() })
    }

    fn tags(&self, repo: &str) -> Result<Vec<Tag>> {
        Ok(tags_from_refs(&self.refs(repo)?.refs))
    }

    fn default_branch(&self, repo: &str) -> Result<String> {
        self.refs(repo)?.head.ok_or_else(|| Error::Parse("the repo has no default branch".into()))
    }

    fn commit_sha(&self, repo: &str, git_ref: &str) -> Result<String> {
        let wanted = format!("refs/heads/{git_ref}");
        self.refs(repo)?
            .refs
            .into_iter()
            .find(|(_, name)| *name == wanted)
            .map(|(sha, _)| sha)
            .ok_or_else(|| Error::Parse(format!("branch {git_ref} not found")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_latest_location() {
        assert_eq!(
            parse_latest_location("https://github.com/cadoky/LuckyOdds/releases/tag/v0.6.0").as_deref(),
            Some("v0.6.0")
        );
        assert_eq!(parse_latest_location("https://github.com/o/r/releases/tag/a%2Bb").as_deref(), Some("a+b"));
        assert_eq!(parse_latest_location("https://github.com/Rezondes/SurvivorsForge/releases"), None);
    }

    #[test]
    fn parses_release_assets() {
        let html = include_str!("../tests/fixtures/github/assets_survivor_ui.html");
        let assets = parse_assets(html, WEB, "morderous/survivor-ui", "v3.0.36");
        let names: Vec<&str> = assets.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["SurvivorUI-Installer.zip", "SurvivorUI-v3.0.36.zip"]);
        assert_eq!(
            assets[1].url,
            "https://github.com/morderous/survivor-ui/releases/download/v3.0.36/SurvivorUI-v3.0.36.zip"
        );
        assert!(parse_assets("<p>nothing</p>", WEB, "a/b", "v1").is_empty());
    }

    #[test]
    fn parses_refs_tags_and_head() {
        let body = String::from_utf8_lossy(include_bytes!("../tests/fixtures/github/refs_bordoui.txt"));
        let Refs { refs, head } = parse_refs(&body);
        assert_eq!(head.as_deref(), Some("main"));
        assert!(refs.iter().any(|(sha, name)| name == "refs/heads/main" && sha.len() == 40));
        let tags = tags_from_refs(&refs);
        assert_eq!(tags[0].name, "v2.8.6");
        assert!(tags.iter().all(|t| !t.name.ends_with("^{}")));
    }

    #[test]
    fn tags_sort_by_version_numbers_and_use_peeled_sha() {
        let refs = [
            ("a".repeat(40), "refs/tags/v2.8.9".to_string()),
            ("b".repeat(40), "refs/tags/v2.8.10".to_string()),
            ("c".repeat(40), "refs/tags/v2.8.10^{}".to_string()),
            ("d".repeat(40), "refs/heads/main".to_string()),
        ];
        let tags = tags_from_refs(&refs);
        assert_eq!(tags, [
            Tag { name: "v2.8.10".into(), sha: "c".repeat(40) },
            Tag { name: "v2.8.9".into(), sha: "a".repeat(40) },
        ]);
    }

    #[test]
    fn tags_rank_release_above_its_pre_releases() {
        let names = ["v1.5.1-review.1", "v1.6.0-beta.2", "v1.6.0", "v1.6.0-beta.10", "v1.5.1"];
        let refs: Vec<(String, String)> = names.iter().map(|n| ("a".repeat(40), format!("refs/tags/{n}"))).collect();
        let sorted: Vec<String> = tags_from_refs(&refs).into_iter().map(|t| t.name).collect();
        assert_eq!(sorted, ["v1.6.0", "v1.6.0-beta.10", "v1.6.0-beta.2", "v1.5.1", "v1.5.1-review.1"]);
    }

    #[test]
    fn normalizes_repo_input() {
        for (input, expected) in [
            ("cadoky/LuckyOdds", "cadoky/LuckyOdds"),
            (" https://github.com/cadoky/LuckyOdds ", "cadoky/LuckyOdds"),
            ("https://github.com/cadoky/LuckyOdds.git", "cadoky/LuckyOdds"),
            ("https://github.com/cadoky/LuckyOdds/tree/main", "cadoky/LuckyOdds"),
            ("github.com/cadoky/LuckyOdds/", "cadoky/LuckyOdds"),
        ] {
            assert_eq!(normalize_repo(input).as_deref(), Ok(expected), "{input}");
        }
        for input in ["", "cadoky", "https://gitlab.com/a/b", "a/b/c", "a b/c"] {
            assert!(normalize_repo(input).is_err(), "{input}");
        }
    }

    #[test]
    fn maps_status_codes() {
        assert_eq!(map_error(404), Error::NotFound);
        assert_eq!(map_error(401), Error::NotFound);
        assert_eq!(map_error(429), Error::RateLimited);
        assert_eq!(map_error(500), Error::Http(500));
    }

    #[test]
    #[ignore = "needs network"]
    fn github_live() {
        let client = Client::new();
        let release = client.latest_release("cadoky/LuckyOdds").unwrap().unwrap();
        assert!(release.tag.starts_with('v'));
        let dir = tempfile::tempdir().unwrap();
        let zip = dir.path().join("a.zip");
        client.download(&release.assets[0].url, &zip).unwrap();
        assert!(!crate::archive::analyze(&zip).unwrap().is_empty());
        assert_eq!(client.default_branch("cadoky/LuckyOdds").unwrap(), "main");
        assert_eq!(client.commit_sha("cadoky/LuckyOdds", "main").unwrap().len(), 40);
        assert!(client.tags("threenox/BordoUI").unwrap()[0].name.starts_with('v'));
        let spec = crate::registry::SourceSpec::Release { asset: "AzerothFieldKit-*.zip".into() };
        let pre = crate::updates::resolve_remote(&client, "Maidenquinn/azeroth-fieldkit", &spec).unwrap();
        assert!(pre.id.starts_with('v') && pre.download.ends_with(".zip"));
        assert_eq!(client.latest_release("Maidenquinn/azeroth-fieldkit").unwrap(), None);
        assert_eq!(client.latest_release("nope-xyz-123/nothing"), Err(Error::NotFound));
        assert_eq!(client.tags("nope-xyz-123/nothing"), Err(Error::NotFound));
    }
}
