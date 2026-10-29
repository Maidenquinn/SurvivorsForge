//! The list of addons: the platform API (`/v1/registry`), else the last good cached copy, else
//! the copy compiled into the binary. Without the API the launcher is offline: it shows the cached
//! list but installs and updates nothing.

pub use forge_core::registry::*;

use crate::Result;
use std::path::Path;
use std::time::{Duration, SystemTime};

pub const EMBEDDED: &str = include_str!("../registry/verified.json");
const CACHE_FILE: &str = "verified.json";
/// Stand in until a release build sets the real addresses with `SURVIVORSFORGE_API_URL` and
/// `SURVIVORSFORGE_WEB_URL`.
pub const PLACEHOLDER_API_URL: &str = "https://api.survivorsforge.invalid";
pub const PLACEHOLDER_WEB_URL: &str = "https://survivorsforge.invalid";

/// The platform API: `SURVIVORSFORGE_API_URL` at run time (self-hosting, tests), else the address
/// compiled in by the release build, else the placeholder.
pub fn api_url() -> String {
    pick_url(std::env::var("SURVIVORSFORGE_API_URL").ok().as_deref(), option_env!("SURVIVORSFORGE_API_URL"), PLACEHOLDER_API_URL)
}

/// The platform website, for links the launcher opens in the browser; chosen like `api_url`.
pub fn web_url() -> String {
    pick_url(std::env::var("SURVIVORSFORGE_WEB_URL").ok().as_deref(), option_env!("SURVIVORSFORGE_WEB_URL"), PLACEHOLDER_WEB_URL)
}

fn pick_url(runtime: Option<&str>, build: Option<&str>, placeholder: &str) -> String {
    [runtime, build]
        .into_iter()
        .flatten()
        .map(|u| u.trim().trim_end_matches('/'))
        .find(|u| !u.is_empty())
        .unwrap_or(placeholder)
        .to_string()
}

pub fn registry_url(api: &str) -> String {
    format!("{api}/v1/registry")
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Origin {
    /// The platform API: every addon with its trust level.
    Api,
    Cache,
    Embedded,
}

pub fn embedded() -> Registry {
    Registry::parse(EMBEDDED).expect("embedded registry is valid")
}

/// The API's answer if it is a valid registry (it also refreshes the cache), else the cache, else
/// the built-in copy. `None` loads without asking the API (at start-up).
pub fn load(fetched: Option<Result<String>>, cache_dir: &Path) -> (Registry, Origin) {
    let cache = cache_dir.join(CACHE_FILE);
    if let Some(Ok(text)) = fetched
        && let Ok(registry) = Registry::parse(&text)
    {
        let _ = crate::write_atomic(&cache, text.as_bytes());
        return (registry, Origin::Api);
    }
    if let Some(registry) = std::fs::read_to_string(&cache).ok().and_then(|t| Registry::parse(&t).ok()) {
        return (registry, Origin::Cache);
    }
    (embedded(), Origin::Embedded)
}

/// How old the cached list is, for the offline notice.
pub fn cache_age(cache_dir: &Path, now: SystemTime) -> Option<Duration> {
    let modified = std::fs::metadata(cache_dir.join(CACHE_FILE)).and_then(|m| m.modified()).ok()?;
    now.duration_since(modified).ok()
}

/// "5 minutes", "3 hours", "2 days".
pub fn describe_age(age: Duration) -> String {
    let minutes = age.as_secs() / 60;
    let (n, unit) = match minutes {
        0..60 => (minutes, "minute"),
        60..1440 => (minutes / 60, "hour"),
        _ => (minutes / 1440, "day"),
    };
    format!("{n} {unit}{}", if n == 1 { "" } else { "s" })
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
    fn api_then_cache_then_embedded() {
        let dir = tempfile::tempdir().unwrap();
        let offline = || Some(Err(Error::Network("offline".into())));
        assert_eq!(load(offline(), dir.path()), (embedded(), Origin::Embedded));

        let api = r#"{"schema":1,"addons":[{"id":"api","name":"A","author":"a","repo":"o/a","folders":["A"],"source":{"type":"tag"},"trust":"scanned"}]}"#;
        let (registry, origin) = load(Some(Ok(api.into())), dir.path());
        assert_eq!((origin, registry.addons.len()), (Origin::Api, 1));
        assert_eq!(registry.addons[0].trust, Some(forge_core::trust::Trust::Scanned));

        let (registry, origin) = load(offline(), dir.path());
        assert_eq!((origin, registry.addons.len()), (Origin::Cache, 1), "the cached API answer");
        let (_, origin) = load(Some(Ok("<html>502</html>".into())), dir.path());
        assert_eq!(origin, Origin::Cache, "a broken API answer falls back to the cache");
        assert_eq!(load(None, dir.path()).1, Origin::Cache, "start-up reads the cache");
        assert!(cache_age(dir.path(), SystemTime::now() + Duration::from_secs(120)).unwrap() >= Duration::from_secs(119));
    }

    #[test]
    fn api_url_runtime_before_build_before_placeholder() {
        assert_eq!(pick_url(Some("http://localhost:8080/"), Some("https://api.real"), PLACEHOLDER_API_URL), "http://localhost:8080");
        assert_eq!(pick_url(Some(" "), Some("https://api.real"), PLACEHOLDER_API_URL), "https://api.real");
        assert_eq!(pick_url(None, None, PLACEHOLDER_WEB_URL), PLACEHOLDER_WEB_URL);
        assert_eq!(registry_url("https://api.test"), "https://api.test/v1/registry");
    }

    #[test]
    fn ages_read_naturally() {
        assert_eq!(describe_age(Duration::from_secs(30)), "0 minutes");
        assert_eq!(describe_age(Duration::from_secs(60)), "1 minute");
        assert_eq!(describe_age(Duration::from_secs(3 * 3600)), "3 hours");
        assert_eq!(describe_age(Duration::from_secs(2 * 86400)), "2 days");
    }
}
