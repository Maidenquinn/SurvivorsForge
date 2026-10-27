//! Decides per managed addon whether a newer version is available.
//!
//! Tags are not uniform (`v3.0.36`, `v2.6.5-azs`, `build-<sha>`), so a remote id that differs
//! from the installed one counts as an update. No semver comparison.

use crate::github::{source_zip_url, GitHub};
use crate::registry::{glob_match, Registry, SourceSpec};
use crate::state::{Source, State, TOC_PREFIX};
use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq)]
pub struct RemoteVersion {
    /// Tag or commit SHA; what gets stored as the installed version.
    pub id: String,
    /// Short form for display.
    pub label: String,
    pub download: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UpdateStatus {
    UpToDate,
    UpdateAvailable(RemoteVersion),
    /// Installed from a zip: no remote to check.
    Manual,
    CheckFailed(String),
}

pub fn resolve_remote(gh: &dyn GitHub, repo: &str, spec: &SourceSpec) -> Result<RemoteVersion> {
    match spec {
        SourceSpec::Release { asset } => {
            let release = gh.latest_release(repo)?.ok_or(Error::NoRelease)?;
            let asset = release.assets.iter().find(|a| glob_match(asset, &a.name)).ok_or(Error::NoMatchingAsset)?;
            Ok(RemoteVersion { id: release.tag.clone(), label: release.tag.clone(), download: asset.url.clone() })
        }
        SourceSpec::Tag => {
            let tag = gh.tags(repo)?.into_iter().next().ok_or(Error::NoTags)?;
            Ok(RemoteVersion { download: source_zip_url(repo, &tag.name), label: tag.name.clone(), id: tag.name })
        }
        SourceSpec::Branch => {
            let branch = gh.default_branch(repo)?;
            let sha = gh.commit_sha(repo, &branch)?;
            Ok(RemoteVersion { download: source_zip_url(repo, &sha), label: sha.chars().take(7).collect(), id: sha })
        }
    }
}

fn normalize(version: &str) -> String {
    version.trim().trim_start_matches(['v', 'V']).to_lowercase()
}

/// Compares the installed version with the remote one. For adopted addons (`toc:<version>`)
/// a matching version also returns the remote id to store from now on.
pub fn check(installed: &str, remote: RemoteVersion) -> (UpdateStatus, Option<String>) {
    if let Some(toc_version) = installed.strip_prefix(TOC_PREFIX) {
        if !toc_version.is_empty() && normalize(toc_version) == normalize(&remote.id) {
            return (UpdateStatus::UpToDate, Some(remote.id));
        }
        return (UpdateStatus::UpdateAvailable(remote), None);
    }
    if installed == remote.id {
        (UpdateStatus::UpToDate, None)
    } else {
        (UpdateStatus::UpdateAvailable(remote), None)
    }
}

/// The repo and source spec of an entry; `None` for zip installs.
pub fn remote_of(source: &Source, registry: &Registry) -> Option<Result<(String, SourceSpec)>> {
    match source {
        Source::Zip { .. } => None,
        Source::GitHub { repo, spec } => Some(Ok((repo.clone(), spec.clone()))),
        Source::Verified { registry_id } => Some(
            registry
                .get(registry_id)
                .map(|a| (a.repo.clone(), a.source.clone()))
                .ok_or_else(|| Error::Parse(format!("{registry_id} is no longer in the verified list"))),
        ),
    }
}

/// Checks every entry. One failing repo does not stop the others. Adopted versions that turn
/// out to be current are rewritten to the remote id in `state`.
pub fn check_all(gh: &dyn GitHub, state: &mut State, registry: &Registry) -> Vec<(String, UpdateStatus)> {
    let mut results = Vec::new();
    for entry in &mut state.entries {
        let status = match remote_of(&entry.source, registry) {
            None => UpdateStatus::Manual,
            Some(Err(e)) => UpdateStatus::CheckFailed(e.to_string()),
            Some(Ok((repo, spec))) => match resolve_remote(gh, &repo, &spec) {
                Err(e) => UpdateStatus::CheckFailed(e.to_string()),
                Ok(remote) => {
                    let (status, adopted) = check(&entry.version, remote);
                    if let Some(version) = adopted {
                        entry.version = version;
                    }
                    status
                }
            },
        };
        results.push((entry.id.clone(), status));
    }
    results
}

#[cfg(test)]
pub(crate) mod fake {
    use crate::github::{Asset, GitHub, Release, Tag};
    use crate::{Error, Result};
    use std::collections::HashMap;

    /// In-memory GitHub: repos missing from every map answer `NotFound`.
    #[derive(Default)]
    pub struct FakeGitHub {
        pub releases: HashMap<String, Release>,
        pub tags: HashMap<String, Vec<Tag>>,
        pub branches: HashMap<String, (String, String)>,
        pub failing: HashMap<String, Error>,
    }

    impl FakeGitHub {
        pub fn release(mut self, repo: &str, tag: &str, assets: &[&str]) -> Self {
            let assets = assets
                .iter()
                .map(|n| Asset { name: n.to_string(), url: format!("https://dl/{repo}/{tag}/{n}") })
                .collect();
            self.releases.insert(repo.into(), Release { tag: tag.into(), assets });
            self.branch(repo, "main", "0000000000000000000000000000000000000000")
        }

        pub fn tag(mut self, repo: &str, name: &str) -> Self {
            self.tags.entry(repo.into()).or_default().push(Tag { name: name.into(), sha: "abc".into() });
            self.branch(repo, "main", "0000000000000000000000000000000000000000")
        }

        pub fn branch(mut self, repo: &str, branch: &str, sha: &str) -> Self {
            self.branches.insert(repo.into(), (branch.into(), sha.into()));
            self
        }

        pub fn fail(mut self, repo: &str, error: Error) -> Self {
            self.failing.insert(repo.into(), error);
            self
        }

        fn exists(&self, repo: &str) -> Result<()> {
            if let Some(e) = self.failing.get(repo) {
                return Err(e.clone());
            }
            if self.branches.contains_key(repo) {
                Ok(())
            } else {
                Err(Error::NotFound)
            }
        }
    }

    impl GitHub for FakeGitHub {
        fn latest_release(&self, repo: &str) -> Result<Option<Release>> {
            self.exists(repo)?;
            Ok(self.releases.get(repo).cloned())
        }
        fn tags(&self, repo: &str) -> Result<Vec<Tag>> {
            self.exists(repo)?;
            Ok(self.tags.get(repo).cloned().unwrap_or_default())
        }
        fn default_branch(&self, repo: &str) -> Result<String> {
            self.exists(repo)?;
            Ok(self.branches[repo].0.clone())
        }
        fn commit_sha(&self, repo: &str, _git_ref: &str) -> Result<String> {
            self.exists(repo)?;
            Ok(self.branches[repo].1.clone())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::FakeGitHub;
    use super::*;
    use crate::state::Entry;

    fn remote(id: &str) -> RemoteVersion {
        RemoteVersion { id: id.into(), label: id.into(), download: "url".into() }
    }

    fn entry(id: &str, version: &str) -> Entry {
        Entry {
            id: id.into(),
            name: id.into(),
            source: Source::Verified { registry_id: id.into() },
            folders: vec![],
            version: version.into(),
            installed_at: 0,
        }
    }

    #[test]
    fn adopted_older_toc_needs_update() {
        assert_eq!(check("toc:0.4.0", remote("v0.6.0")).0, UpdateStatus::UpdateAvailable(remote("v0.6.0")));
        assert!(matches!(check("toc:", remote("v1")).0, UpdateStatus::UpdateAvailable(_)));
    }

    #[test]
    fn adopted_equal_toc_is_up_to_date() {
        assert_eq!(check("toc:3.0.36", remote("v3.0.36")), (UpdateStatus::UpToDate, Some("v3.0.36".into())));
    }

    #[test]
    fn differing_identifiers_need_update() {
        assert!(matches!(check("build-aaa", remote("build-bbb")).0, UpdateStatus::UpdateAvailable(_)));
        assert_eq!(check("v2.6.5-azs", remote("v2.6.5-azs")).0, UpdateStatus::UpToDate);
    }

    #[test]
    fn zip_source_is_manual() {
        let mut state = State::default();
        state.upsert(Entry { source: Source::Zip { file_name: "x.zip".into() }, ..entry("x", "zip:x.zip") });
        let results = check_all(&FakeGitHub::default(), &mut state, &Registry::embedded());
        assert_eq!(results, [("x".to_string(), UpdateStatus::Manual)]);
    }

    #[test]
    fn one_failure_does_not_block_others() {
        let gh = FakeGitHub::default()
            .fail("cadoky/LuckyOdds", Error::RateLimited)
            .release("kex0/AZSSpellMeter", "v1.31", &["AZSSpellMeter-1.31.zip"]);
        let mut state = State::default();
        state.upsert(entry("luckyodds", "toc:0.4.0"));
        state.upsert(entry("azsspellmeter", "toc:1.31"));
        let results = check_all(&gh, &mut state, &Registry::embedded());
        assert!(matches!(&results[0].1, UpdateStatus::CheckFailed(m) if m.contains("limiting requests")));
        assert_eq!(results[1].1, UpdateStatus::UpToDate);
        assert_eq!(state.get("azsspellmeter").unwrap().version, "v1.31");
    }

    #[test]
    fn missing_asset_is_reported() {
        let gh = FakeGitHub::default().release("cadoky/LuckyOdds", "v0.6.0", &["Something.zip"]);
        let spec = SourceSpec::Release { asset: "LuckyOdds-v*.zip".into() };
        assert_eq!(resolve_remote(&gh, "cadoky/LuckyOdds", &spec), Err(Error::NoMatchingAsset));
    }

    #[test]
    fn tag_and_branch_sources() {
        let gh = FakeGitHub::default().tag("threenox/BordoUI", "v2.8.6");
        let r = resolve_remote(&gh, "threenox/BordoUI", &SourceSpec::Tag).unwrap();
        assert_eq!(r.id, "v2.8.6");
        assert_eq!(r.download, "https://codeload.github.com/threenox/BordoUI/zip/v2.8.6");
        let gh = FakeGitHub::default().branch("o/r", "main", "1234567890abcdef");
        let r = resolve_remote(&gh, "o/r", &SourceSpec::Branch).unwrap();
        assert_eq!((r.id.as_str(), r.label.as_str()), ("1234567890abcdef", "1234567"));
    }
}
