//! Parsers for github.com web endpoints (not the REST API, which allows only 60 requests per hour
//! without a token):
//!
//! - latest release: the redirect of `/{repo}/releases/latest`
//! - release assets: the HTML fragment `/{repo}/releases/expanded_assets/{tag}`
//! - tags, default branch, commit SHAs: `/{repo}.git/info/refs` (what `git ls-remote` reads)
//!
//! forge-api fetches these and serves them to the launcher as JSON.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Asset {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Release {
    pub tag: String,
    pub assets: Vec<Asset>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tag {
    pub name: String,
    pub sha: String,
}


/// The tag from the `Location` of `/releases/latest`: `.../releases/tag/<tag>` when there is a
/// release, `.../releases` when there is none.
pub fn parse_latest_location(location: &str) -> Option<String> {
    location.split_once("/releases/tag/").map(|(_, tag)| percent_decode(tag.trim_end_matches('/')))
}

/// Asset links in the `expanded_assets` fragment: `href="/{repo}/releases/download/{tag}/{name}"`,
/// made absolute with `web` (`https://github.com`).
pub fn parse_assets(html: &str, web: &str, repo: &str, tag: &str) -> Vec<Asset> {
    let prefix = format!("/{repo}/releases/download/{tag}/").to_lowercase();
    let mut assets: Vec<Asset> = Vec::new();
    for chunk in html.split("href=\"").skip(1) {
        let Some(href) = chunk.split('"').next() else { continue };
        if !href.to_lowercase().starts_with(&prefix) {
            continue;
        }
        let name = percent_decode(&href[prefix.len()..]);
        if !name.is_empty() && !name.contains('/') && !assets.iter().any(|a| a.name == name) {
            assets.push(Asset { name, url: format!("{web}{href}") });
        }
    }
    assets
}

/// A smart-HTTP `info/refs` answer.
#[derive(Debug, Default, PartialEq)]
pub struct Refs {
    /// `(sha, ref name)`, e.g. `refs/tags/v2.8.6`.
    pub refs: Vec<(String, String)>,
    /// The branch HEAD points to.
    pub head: Option<String>,
}

pub fn parse_refs(body: &str) -> Refs {
    let mut refs = Vec::new();
    for line in body.lines() {
        // Each line is a pkt-line: 4 hex digits of length, then the ref. The first ref line
        // follows a flush packet ("0000") on the same line.
        let line = match line.strip_prefix("0000") {
            Some(rest) if rest.len() > 44 => rest,
            _ => line,
        };
        let line = line.get(4..).unwrap_or_default();
        let Some((sha, name)) = line.split_once(' ') else { continue };
        if sha.len() != 40 || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }
        let name = name.split('\0').next().unwrap_or_default().trim();
        refs.push((sha.to_string(), name.to_string()));
    }
    let head = body
        .split_once("symref=HEAD:refs/heads/")
        .and_then(|(_, rest)| rest.split([' ', '\0', '\n']).next())
        .filter(|b| !b.is_empty())
        .map(String::from);
    Refs { refs, head }
}

/// Tags newest first, compared by the numbers in their names (`v2.8.10` before `v2.8.9`,
/// `v1.6.0` before `v1.6.0-beta.2`).
/// Annotated tags point at their commit (the `^{}` line).
pub fn tags_from_refs(refs: &[(String, String)]) -> Vec<Tag> {
    let mut tags: Vec<Tag> = Vec::new();
    for (sha, name) in refs {
        let Some(tag) = name.strip_prefix("refs/tags/") else { continue };
        match tag.strip_suffix("^{}") {
            Some(peeled) => {
                if let Some(t) = tags.iter_mut().find(|t| t.name == peeled) {
                    t.sha = sha.clone();
                }
            }
            None => tags.push(Tag { name: tag.to_string(), sha: sha.clone() }),
        }
    }
    tags.sort_by(|a, b| version_key(&b.name).cmp(&version_key(&a.name)).then_with(|| a.name.cmp(&b.name)));
    tags
}

/// The release numbers first, then a plain release above its pre-releases (`v1.6.0` before
/// `v1.6.0-beta.2`), then the numbers in the suffix (`beta.2` before `beta.1`).
pub fn version_key(name: &str) -> (Vec<u64>, bool, Vec<u64>) {
    let numbers = |s: &str| -> Vec<u64> {
        s.split(|c: char| !c.is_ascii_digit()).filter(|p| !p.is_empty()).map(|p| p.parse().unwrap_or(u64::MAX)).collect()
    };
    let rest = &name[name.find(|c: char| c.is_ascii_digit()).unwrap_or(name.len())..];
    let (release, suffix) = rest.split_at(rest.find(|c: char| !c.is_ascii_digit() && c != '.').unwrap_or(rest.len()));
    (numbers(release), suffix.is_empty(), numbers(suffix))
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(b) = s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

