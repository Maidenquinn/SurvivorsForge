//! The list of verified addons (`registry/verified.json`).
//!
//! Load order: the platform API (when this build knows its URL) -> remote JSON from the
//! SurvivorsForge repo -> last good cached copy -> the copy compiled into the binary.

pub use forge_core::registry::*;

use crate::Result;
use std::path::Path;

pub const EMBEDDED: &str = include_str!("../registry/verified.json");
pub const REMOTE_URL: &str =
    concat!("https://raw.githubusercontent.com/", "Rezondes/SurvivorsForge", "/main/registry/verified.json");
const CACHE_FILE: &str = "verified.json";

/// The platform API, set at build time with `SURVIVORSFORGE_API_URL`. Without it the launcher only
/// knows the verified list, exactly like before the platform existed.
pub fn api_url() -> Option<&'static str> {
    option_env!("SURVIVORSFORGE_API_URL").map(|u| u.trim_end_matches('/')).filter(|u| !u.is_empty())
}

/// Where to fetch the registry from, best first.
pub fn sources(api: Option<&str>) -> Vec<(Origin, String)> {
    let mut sources = Vec::new();
    if let Some(api) = api {
        sources.push((Origin::Api, format!("{api}/v1/registry")));
    }
    sources.push((Origin::Remote, REMOTE_URL.to_string()));
    sources
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Origin {
    /// The platform API: every addon with its trust level.
    Api,
    Remote,
    Cache,
    Embedded,
}

pub fn embedded() -> Registry {
    Registry::parse(EMBEDDED).expect("embedded registry is valid")
}

/// Picks the first usable registry of `fetched`, in order. A valid one also refreshes the cache.
pub fn load(fetched: Vec<(Origin, Result<String>)>, cache_dir: &Path) -> (Registry, Origin) {
    let cache = cache_dir.join(CACHE_FILE);
    for (origin, text) in fetched {
        if let Ok(text) = text
            && let Ok(registry) = Registry::parse(&text)
        {
            let _ = crate::write_atomic(&cache, text.as_bytes());
            return (registry, origin);
        }
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
        let (registry, origin) = load(vec![(Origin::Remote, Err(Error::Network("offline".into())))], dir.path());
        assert_eq!(origin, Origin::Embedded);
        assert_eq!(registry, embedded());

        let remote = r#"{"schema":1,"addons":[]}"#.to_string();
        let (registry, origin) = load(vec![(Origin::Remote, Ok(remote))], dir.path());
        assert_eq!(origin, Origin::Remote);
        assert!(registry.addons.is_empty());

        let (registry, origin) = load(vec![(Origin::Remote, Err(Error::Network("offline".into())))], dir.path());
        assert_eq!(origin, Origin::Cache);
        assert!(registry.addons.is_empty());

        let (_, origin) = load(vec![(Origin::Remote, Ok("broken".into()))], dir.path());
        assert_eq!(origin, Origin::Cache, "invalid remote falls back to cache");
    }


    #[test]
    fn api_first_then_fallbacks() {
        let dir = tempfile::tempdir().unwrap();
        let api = r#"{"schema":1,"addons":[{"id":"api","name":"A","author":"a","repo":"o/a","folders":["A"],"source":{"type":"tag"},"trust":"scanned"}]}"#;
        let raw = r#"{"schema":1,"addons":[]}"#;
        let offline = || Err(Error::Network("offline".into()));

        let (registry, origin) = load(vec![(Origin::Api, Ok(api.into())), (Origin::Remote, Ok(raw.into()))], dir.path());
        assert_eq!((origin, registry.addons.len()), (Origin::Api, 1));
        assert_eq!(registry.addons[0].trust, Some(forge_core::trust::Trust::Scanned));

        let (_, origin) = load(vec![(Origin::Api, Ok("<html>502</html>".into())), (Origin::Remote, Ok(raw.into()))], dir.path());
        assert_eq!(origin, Origin::Remote, "a broken API answer falls back to the raw file");
        let (registry, origin) = load(vec![(Origin::Api, offline()), (Origin::Remote, offline())], dir.path());
        assert_eq!((origin, registry.addons.len()), (Origin::Cache, 0), "then the cache");
    }

    #[test]
    fn no_api_url_keeps_old_behaviour() {
        assert_eq!(sources(None), [(Origin::Remote, REMOTE_URL.to_string())]);
        let both = sources(Some("https://api.test"));
        assert_eq!(both[0], (Origin::Api, "https://api.test/v1/registry".to_string()));
        assert_eq!(both[1].0, Origin::Remote);
    }
}
