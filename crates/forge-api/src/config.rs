//! Configuration from environment variables only. No config files, no secrets in the repo.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub public_api_url: String,
    pub bind_addr: SocketAddr,
    pub migrate_on_start: bool,
    pub rate_limit_per_minute: u32,
    pub trusted_proxies: Vec<IpAddr>,
    pub github_api_url: String,
    pub github_app_id: String,
    /// PEM text; `\n` escapes allowed. Read by `github::app_auth::load_private_key`.
    pub github_app_private_key: Option<String>,
    pub github_app_private_key_file: Option<String>,
}

#[derive(Debug, PartialEq)]
pub struct ConfigError(pub String);

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl Config {
    /// Every variable `from_map` reads. `.env.example` must list each of them.
    pub const VARIABLES: &[&str] = &[
        "DATABASE_URL",
        "PUBLIC_API_URL",
        "BIND_ADDR",
        "MIGRATE_ON_START",
        "RATE_LIMIT_PER_MINUTE",
        "TRUSTED_PROXIES",
        "GITHUB_API_URL",
        "GITHUB_APP_ID",
        "GITHUB_APP_PRIVATE_KEY",
        "GITHUB_APP_PRIVATE_KEY_FILE",
    ];

    pub fn from_env() -> Result<Config, ConfigError> {
        Config::from_map(&std::env::vars().collect())
    }

    pub fn from_map(env: &HashMap<String, String>) -> Result<Config, ConfigError> {
        let get = |name: &str| env.get(name).map(|v| v.trim()).filter(|v| !v.is_empty());
        let required = |name: &str| get(name).map(str::to_string).ok_or_else(|| ConfigError(format!("{name} is not set")));
        let parse = |name: &str, default: &str| -> Result<String, ConfigError> { Ok(get(name).unwrap_or(default).to_string()) };
        let invalid = |name: &str, value: &str| ConfigError(format!("{name} has an invalid value: {value}"));

        let bind = parse("BIND_ADDR", "0.0.0.0:8080")?;
        let migrate = parse("MIGRATE_ON_START", "false")?;
        let limit = parse("RATE_LIMIT_PER_MINUTE", "120")?;
        let proxies = parse("TRUSTED_PROXIES", "")?;
        Ok(Config {
            database_url: required("DATABASE_URL")?,
            public_api_url: required("PUBLIC_API_URL")?.trim_end_matches('/').to_string(),
            bind_addr: bind.parse().map_err(|_| invalid("BIND_ADDR", &bind))?,
            migrate_on_start: migrate.parse().map_err(|_| invalid("MIGRATE_ON_START", &migrate))?,
            rate_limit_per_minute: limit.parse().map_err(|_| invalid("RATE_LIMIT_PER_MINUTE", &limit))?,
            trusted_proxies: proxies
                .split(',')
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .map(|p| p.parse().map_err(|_| invalid("TRUSTED_PROXIES", p)))
                .collect::<Result<_, _>>()?,
            github_api_url: parse("GITHUB_API_URL", "https://api.github.com")?.trim_end_matches('/').to_string(),
            github_app_id: required("GITHUB_APP_ID")?,
            github_app_private_key: get("GITHUB_APP_PRIVATE_KEY").map(str::to_string),
            github_app_private_key_file: get("GITHUB_APP_PRIVATE_KEY_FILE").map(str::to_string),
        })
    }

    /// A config for tests: only the database differs between them.
    pub fn for_tests(database_url: &str) -> Config {
        Config {
            database_url: database_url.to_string(),
            public_api_url: "http://api.test".into(),
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            migrate_on_start: false,
            rate_limit_per_minute: 10_000,
            trusted_proxies: Vec::new(),
            github_api_url: "http://github.test".into(),
            github_app_id: "1".into(),
            github_app_private_key: None,
            github_app_private_key_file: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    const MINIMAL: &[(&str, &str)] = &[
        ("DATABASE_URL", "postgres://x"),
        ("PUBLIC_API_URL", "https://api.example.com/"),
        ("GITHUB_APP_ID", "1"),
    ];

    #[test]
    fn missing_database_url_names_variable() {
        let err = Config::from_map(&env(&[("PUBLIC_API_URL", "https://a"), ("GITHUB_APP_ID", "1")])).unwrap_err();
        assert!(err.0.contains("DATABASE_URL"), "{}", err.0);
        let err =
            Config::from_map(&env(&[("DATABASE_URL", "postgres://x"), ("PUBLIC_API_URL", " "), ("GITHUB_APP_ID", "1")])).unwrap_err();
        assert!(err.0.contains("PUBLIC_API_URL"), "{}", err.0);
    }

    #[test]
    fn migrate_on_start_defaults_to_false() {
        let config = Config::from_map(&env(MINIMAL)).unwrap();
        assert!(!config.migrate_on_start);
        assert_eq!(config.rate_limit_per_minute, 120);
        assert_eq!(config.public_api_url, "https://api.example.com");
        let mut on = env(MINIMAL);
        on.insert("MIGRATE_ON_START".into(), "true".into());
        assert!(Config::from_map(&on).unwrap().migrate_on_start);
    }

    #[test]
    fn invalid_values_are_named() {
        let mut bad = env(MINIMAL);
        bad.insert("TRUSTED_PROXIES".into(), "10.0.0.1, nope".into());
        assert!(Config::from_map(&bad).unwrap_err().0.contains("TRUSTED_PROXIES"));
    }

    #[test]
    fn env_example_lists_every_variable() {
        let example = include_str!("../../../.env.example");
        for name in Config::VARIABLES {
            assert!(example.lines().any(|l| l.starts_with(&format!("{name}="))), "{name} missing in .env.example");
        }
    }
}
