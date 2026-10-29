//! The list of verified addons (`registry/verified.json`).
//!
//! Load order: remote JSON from the SurvivorsForge repo -> last good cached copy -> the copy
//! compiled into the binary.

pub use forge_core::registry::*;

use crate::Result;
use std::path::Path;

pub const EMBEDDED: &str = include_str!("../registry/verified.json");
pub const REMOTE_URL: &str =
    concat!("https://raw.githubusercontent.com/", "Rezondes/SurvivorsForge", "/main/registry/verified.json");
const CACHE_FILE: &str = "verified.json";

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Origin {
    Remote,
    Cache,
    Embedded,
}

pub fn embedded() -> Registry {
    Registry::parse(EMBEDDED).expect("embedded registry is valid")
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
    (embedded(), Origin::Embedded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;

    #[test]
    fn embedded_registry_is_valid_and_complete() {
        let registry = embedded();
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
            "Maidenquinn/AzerothStats",
            "Maidenquinn/azeroth-fieldkit",
        ]);
        assert_eq!(registry.get("survivor-ui").unwrap().folders, ["SurvivorUI", "SurvivorPlates"]);
        assert_eq!(registry.get("bordoui").unwrap().source, SourceSpec::Tag);
    }

    #[test]
    fn fallback_order() {
        let dir = tempfile::tempdir().unwrap();
        let (registry, origin) = load(Err(Error::Network("offline".into())), dir.path());
        assert_eq!(origin, Origin::Embedded);
        assert_eq!(registry, embedded());

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

}
