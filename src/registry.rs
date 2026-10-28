//! The list of verified addons (`registry/verified.json`).
//!
//! Load order: remote JSON from the SurvivorsForge repo -> last good cached copy -> the copy
//! compiled into the binary.

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const SCHEMA: u32 = 1;
pub const EMBEDDED: &str = include_str!("../registry/verified.json");
pub const REMOTE_URL: &str =
    concat!("https://raw.githubusercontent.com/", "Rezondes/SurvivorsForge", "/main/registry/verified.json");
const CACHE_FILE: &str = "verified.json";

/// Where a repo's updates come from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SourceSpec {
    /// The asset matching `asset` (`*` wildcard) of the release marked "latest".
    Release { asset: String },
    /// The source zip of the newest tag.
    Tag,
    /// The source zip of the default branch's newest commit.
    Branch,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VerifiedAddon {
    pub id: String,
    pub name: String,
    pub author: String,
    pub repo: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub category: String,
    pub folders: Vec<String>,
    pub source: SourceSpec,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Registry {
    pub schema: u32,
    pub addons: Vec<VerifiedAddon>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Origin {
    Remote,
    Cache,
    Embedded,
}

impl Registry {
    pub fn parse(text: &str) -> Result<Registry> {
        let registry: Registry = serde_json::from_str(text).map_err(|e| Error::Parse(e.to_string()))?;
        if registry.schema != SCHEMA {
            return Err(Error::Parse(format!("unknown schema version {}", registry.schema)));
        }
        let mut ids = std::collections::HashSet::new();
        let mut folders = std::collections::HashSet::new();
        for addon in &registry.addons {
            if !ids.insert(addon.id.to_lowercase()) {
                return Err(Error::Parse(format!("duplicate id {}", addon.id)));
            }
            if addon.folders.is_empty() {
                return Err(Error::Parse(format!("{} has no folders", addon.id)));
            }
            for folder in &addon.folders {
                if !folders.insert(folder.to_lowercase()) {
                    return Err(Error::Parse(format!("folder {folder} is used twice")));
                }
            }
        }
        Ok(registry)
    }

    pub fn embedded() -> Registry {
        Registry::parse(EMBEDDED).expect("embedded registry is valid")
    }

    pub fn get(&self, id: &str) -> Option<&VerifiedAddon> {
        self.addons.iter().find(|a| a.id == id)
    }

    pub fn by_repo(&self, repo: &str) -> Option<&VerifiedAddon> {
        self.addons.iter().find(|a| a.repo.eq_ignore_ascii_case(repo))
    }
}

/// Picks the first usable registry. A valid remote copy also refreshes the cache.
pub fn load(remote: Result<String>, cache_dir: &Path) -> (Registry, Origin) {
    let cache = cache_dir.join(CACHE_FILE);
    if let Ok(text) = remote
        && let Ok(registry) = Registry::parse(&text)
    {
        let _ = crate::write_atomic(&cache, text.as_bytes());
        return (registry, Origin::Remote);
    }
    if let Some(registry) = std::fs::read_to_string(&cache).ok().and_then(|t| Registry::parse(&t).ok()) {
        return (registry, Origin::Cache);
    }
    (Registry::embedded(), Origin::Embedded)
}

/// Case-insensitive match where `*` stands for any run of characters.
pub fn glob_match(pattern: &str, name: &str) -> bool {
    let pattern = pattern.to_lowercase();
    let name = name.to_lowercase();
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == name;
    }
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if !name.starts_with(first) || !name.ends_with(last) || name.len() < first.len() + last.len() {
        return false;
    }
    let mut rest = &name[first.len()..name.len() - last.len()];
    for middle in &parts[1..parts.len() - 1] {
        match rest.find(middle) {
            Some(i) => rest = &rest[i + middle.len()..],
            None => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_registry_is_valid_and_complete() {
        let registry = Registry::embedded();
        let repos: Vec<&str> = registry.addons.iter().map(|a| a.repo.as_str()).collect();
        assert_eq!(repos, [
            "morderous/survivor-ui",
            "cadoky/LuckyOdds",
            "cadoky/ASBuilder",
            "Kindagrey/Azerloot",
            "KeijinDE/KeijinUI_modern-as-ui",
            "threenox/BordoUI",
            "NORehabRocket/AetherHUD",
            "Blackwellcode/BlackwellUI",
            "migwynkriid13/SurvivorsDragonUi",
            "kex0/AZSSpellMeter",
        ]);
        assert_eq!(registry.get("survivor-ui").unwrap().folders, ["SurvivorUI", "SurvivorPlates"]);
        assert_eq!(registry.get("bordoui").unwrap().source, SourceSpec::Tag);
    }

    #[test]
    fn rejects_unknown_schema() {
        assert!(Registry::parse(r#"{"schema": 2, "addons": []}"#).is_err());
        assert!(Registry::parse("not json").is_err());
    }

    #[test]
    fn rejects_duplicate_ids_and_folders() {
        let addon = |id: &str, folder: &str| {
            format!(r#"{{"id":"{id}","name":"n","author":"a","repo":"o/r","folders":["{folder}"],"source":{{"type":"tag"}}}}"#)
        };
        let json = |a: String, b: String| format!(r#"{{"schema":1,"addons":[{a},{b}]}}"#);
        assert!(Registry::parse(&json(addon("a", "A"), addon("a", "B"))).is_err());
        assert!(Registry::parse(&json(addon("a", "A"), addon("b", "a"))).is_err());
        assert!(Registry::parse(&json(addon("a", "A"), addon("b", "B"))).is_ok());
    }

    #[test]
    fn fallback_order() {
        let dir = tempfile::tempdir().unwrap();
        let (registry, origin) = load(Err(Error::Network("offline".into())), dir.path());
        assert_eq!(origin, Origin::Embedded);
        assert_eq!(registry, Registry::embedded());

        let remote = r#"{"schema":1,"addons":[]}"#.to_string();
        let (registry, origin) = load(Ok(remote), dir.path());
        assert_eq!(origin, Origin::Remote);
        assert!(registry.addons.is_empty());

        let (registry, origin) = load(Err(Error::Network("offline".into())), dir.path());
        assert_eq!(origin, Origin::Cache);
        assert!(registry.addons.is_empty());

        let (_, origin) = load(Ok("broken".into()), dir.path());
        assert_eq!(origin, Origin::Cache, "invalid remote falls back to cache");
    }

    #[test]
    fn asset_glob_matching() {
        assert!(glob_match("SurvivorUI-v*.zip", "SurvivorUI-v3.0.36.zip"));
        assert!(!glob_match("SurvivorUI-v*.zip", "SurvivorUI-Installer.zip"));
        assert!(glob_match("AetherHUD.zip", "aetherhud.zip"));
        assert!(!glob_match("AetherHUD.zip", "AetherHUD2.zip"));
        assert!(glob_match("a*b*c", "a-x-b-y-c"));
        assert!(!glob_match("ab*ba", "aba"));
    }
}
