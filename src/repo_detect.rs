//! Finds out whether an arbitrary GitHub repo is an addon and where its updates come from.
//! Order: zip assets of the latest release, then the newest tag, then the default branch.

use crate::archive::DetectedAddon;
use crate::github::{source_zip_url, GitHub};
use crate::registry::{Registry, SourceSpec};
use crate::updates::RemoteVersion;
use crate::{Error, Result};
use std::path::PathBuf;

/// Downloads a zip and analyzes it: the local path plus the addons inside.
pub type FetchFn<'a> = &'a mut dyn FnMut(&str) -> Result<(PathBuf, Vec<DetectedAddon>)>;

#[derive(Debug, Clone, PartialEq)]
pub struct Detection {
    pub repo: String,
    pub spec: SourceSpec,
    pub remote: RemoteVersion,
    /// The downloaded zip, ready to install.
    pub zip: PathBuf,
    pub addons: Vec<DetectedAddon>,
}

impl Detection {
    pub fn entry_id(&self) -> String {
        format!("gh:{}", self.repo.to_lowercase())
    }

    pub fn name(&self) -> String {
        self.addons.first().map(|a| a.toc.title.clone()).unwrap_or_else(|| self.repo.clone())
    }
}

/// Turns a concrete asset name into a pattern that also matches future versions:
/// `LuckyOdds-v0.6.0.zip` with tag `v0.6.0` becomes `LuckyOdds-v*.zip`.
pub fn asset_pattern(asset: &str, tag: &str) -> String {
    let version = tag.trim_start_matches(['v', 'V']);
    if !version.is_empty() && asset.contains(version) {
        asset.replacen(version, "*", 1)
    } else {
        asset.to_string()
    }
}

/// `NoAddonFound` and broken zips mean "try the next candidate"; anything else is a real error.
fn try_fetch(fetch: FetchFn, url: &str) -> Result<Option<(PathBuf, Vec<DetectedAddon>)>> {
    match fetch(url) {
        Ok(found) => Ok(Some(found)),
        Err(Error::NoAddonFound | Error::Zip(_)) => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn detect(gh: &dyn GitHub, registry: &Registry, repo: &str, fetch: FetchFn) -> Result<Detection> {
    if let Some(verified) = registry.by_repo(repo) {
        return Err(Error::AlreadyVerified(verified.name.clone()));
    }
    if let Some(release) = gh.latest_release(repo)? {
        for asset in release.assets.iter().filter(|a| a.name.to_lowercase().ends_with(".zip")) {
            if let Some((zip, addons)) = try_fetch(fetch, &asset.url)? {
                return Ok(Detection {
                    repo: repo.to_string(),
                    spec: SourceSpec::Release { asset: asset_pattern(&asset.name, &release.tag) },
                    remote: RemoteVersion { id: release.tag.clone(), label: release.tag.clone(), download: asset.url.clone() },
                    zip,
                    addons,
                });
            }
        }
    }
    if let Some(tag) = gh.tags(repo)?.into_iter().next() {
        let url = source_zip_url(repo, &tag.name);
        if let Some((zip, addons)) = try_fetch(fetch, &url)? {
            return Ok(Detection {
                repo: repo.to_string(),
                spec: SourceSpec::Tag,
                remote: RemoteVersion { id: tag.name.clone(), label: tag.name, download: url },
                zip,
                addons,
            });
        }
    }
    let branch = gh.default_branch(repo)?;
    let sha = gh.commit_sha(repo, &branch)?;
    let url = source_zip_url(repo, &sha);
    if let Some((zip, addons)) = try_fetch(fetch, &url)? {
        return Ok(Detection {
            repo: repo.to_string(),
            spec: SourceSpec::Branch,
            remote: RemoteVersion { label: sha.chars().take(7).collect(), id: sha, download: url },
            zip,
            addons,
        });
    }
    Err(Error::NoAddonFound)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toc::Toc;
    use crate::updates::fake::FakeGitHub;

    fn addon(name: &str) -> DetectedAddon {
        DetectedAddon { name: name.into(), prefix: name.into(), toc: Toc { title: name.into(), ..Toc::default() } }
    }

    /// Fake fetch: URLs containing a key of `zips` hold those addons, all others have none.
    fn fetcher(zips: &'static [(&'static str, &'static str)]) -> impl FnMut(&str) -> Result<(PathBuf, Vec<DetectedAddon>)> {
        move |url: &str| {
            zips.iter()
                .find(|(key, _)| url.contains(key))
                .map(|(_, name)| (PathBuf::from(url), vec![addon(name)]))
                .ok_or(Error::NoAddonFound)
        }
    }

    #[test]
    fn asset_pattern_from_name() {
        assert_eq!(asset_pattern("LuckyOdds-v0.6.0.zip", "v0.6.0"), "LuckyOdds-v*.zip");
        assert_eq!(asset_pattern("Azerloot-2.5.4.zip", "v2.5.4"), "Azerloot-*.zip");
        assert_eq!(asset_pattern("BordoUI.zip", "v2.0.0"), "BordoUI.zip");
    }

    #[test]
    fn detection_order_release_tag_branch() {
        let gh = FakeGitHub::default().release("cadoky/Lucky", "v0.6.0", &["Lucky-v0.6.0.zip"]);
        let d = detect(&gh, &Registry::embedded(), "cadoky/Lucky", &mut fetcher(&[("Lucky-v0.6.0.zip", "Lucky")])).unwrap();
        assert_eq!(d.spec, SourceSpec::Release { asset: "Lucky-v*.zip".into() });
        assert_eq!(d.remote.id, "v0.6.0");
        assert_eq!(d.entry_id(), "gh:cadoky/lucky");

        let gh = FakeGitHub::default().tag("o/tagged", "v1.2");
        let d = detect(&gh, &Registry::embedded(), "o/tagged", &mut fetcher(&[("zip/v1.2", "T")])).unwrap();
        assert_eq!(d.spec, SourceSpec::Tag);
        assert_eq!(d.remote.id, "v1.2");

        let gh = FakeGitHub::default().branch("o/plain", "main", "abcdef1234");
        let d = detect(&gh, &Registry::embedded(), "o/plain", &mut fetcher(&[("zip/abcdef1234", "P")])).unwrap();
        assert_eq!(d.spec, SourceSpec::Branch);
        assert_eq!(d.remote.label, "abcdef1");
    }

    #[test]
    fn skips_assets_without_addon() {
        let gh = FakeGitHub::default().release("m/ui", "v3.0.36", &["SurvivorUI-Installer.zip", "SurvivorUI-v3.0.36.zip"]);
        let d = detect(&gh, &Registry::embedded(), "m/ui", &mut fetcher(&[("SurvivorUI-v3", "SurvivorUI")])).unwrap();
        assert_eq!(d.spec, SourceSpec::Release { asset: "SurvivorUI-v*.zip".into() });
    }

    #[test]
    fn no_addon_anywhere() {
        let gh = FakeGitHub::default().tag("o/web", "v1");
        assert_eq!(detect(&gh, &Registry::embedded(), "o/web", &mut fetcher(&[])), Err(Error::NoAddonFound));
        assert_eq!(
            detect(&FakeGitHub::default(), &Registry::embedded(), "o/missing", &mut fetcher(&[])),
            Err(Error::NotFound)
        );
    }

    #[test]
    fn verified_repo_is_redirected() {
        let result = detect(&FakeGitHub::default(), &Registry::embedded(), "CADOKY/luckyodds", &mut fetcher(&[]));
        assert_eq!(result, Err(Error::AlreadyVerified("LuckyOdds".into())));
    }
}
