//! Configuration of the website, from environment variables only.

use std::collections::HashMap;
use std::net::SocketAddr;

#[derive(Debug, Clone)]
pub struct Config {
    /// Where the website reaches the API, e.g. `http://api:8080` inside Compose.
    pub api_internal_url: String,
    /// Where browsers reach the API, for the login redirect.
    pub public_api_url: String,
    /// The website's own public address, e.g. `https://forge.example.com`.
    pub public_web_url: String,
    /// At least 32 bytes; signs and encrypts the session cookies.
    pub cookie_key: String,
    pub bind_addr: SocketAddr,
    /// The GitHub App's URL name, for the "install the App" link.
    pub github_app_slug: String,
}

impl Config {
    /// Every variable `from_map` reads. `.env.example` must list each of them.
    pub const VARIABLES: &[&str] = &["API_INTERNAL_URL", "PUBLIC_API_URL", "PUBLIC_WEB_URL", "COOKIE_KEY", "WEB_BIND_ADDR", "GITHUB_APP_SLUG"];

    pub fn from_env() -> Result<Config, String> {
        Config::from_map(&std::env::vars().collect())
    }

    pub fn from_map(env: &HashMap<String, String>) -> Result<Config, String> {
        let get = |name: &str| env.get(name).map(|v| v.trim()).filter(|v| !v.is_empty());
        let required = |name: &str| get(name).map(|v| v.trim_end_matches('/').to_string()).ok_or_else(|| format!("{name} is not set"));
        let cookie_key = required("COOKIE_KEY")?;
        if cookie_key.len() < 32 {
            return Err("COOKIE_KEY must be at least 32 characters".into());
        }
        let bind = get("WEB_BIND_ADDR").unwrap_or("0.0.0.0:8080");
        Ok(Config {
            api_internal_url: required("API_INTERNAL_URL")?,
            public_api_url: required("PUBLIC_API_URL")?,
            public_web_url: required("PUBLIC_WEB_URL")?,
            cookie_key,
            bind_addr: bind.parse().map_err(|_| format!("WEB_BIND_ADDR has an invalid value: {bind}"))?,
            github_app_slug: required("GITHUB_APP_SLUG")?,
        })
    }

    /// The URL the API sends the browser back to after the GitHub login.
    pub fn callback_url(&self) -> String {
        format!("{}/auth/callback", self.public_web_url)
    }

    pub fn for_tests(api_url: &str) -> Config {
        Config {
            api_internal_url: api_url.into(),
            public_api_url: "https://api.test".into(),
            public_web_url: "https://web.test".into(),
            cookie_key: "test-cookie-key-that-is-long-enough-0123456789".into(),
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            github_app_slug: "survivorsforge-test".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_and_short_values_are_named() {
        let env: HashMap<String, String> = [("API_INTERNAL_URL", "http://api"), ("PUBLIC_API_URL", "https://a"), ("PUBLIC_WEB_URL", "https://w"), ("GITHUB_APP_SLUG", "app")]
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        assert!(Config::from_map(&env).unwrap_err().contains("COOKIE_KEY"));
        let mut short = env.clone();
        short.insert("COOKIE_KEY".into(), "short".into());
        assert!(Config::from_map(&short).unwrap_err().contains("32"));
        let mut ok = env;
        ok.insert("COOKIE_KEY".into(), "k".repeat(40));
        assert_eq!(Config::from_map(&ok).unwrap().callback_url(), "https://w/auth/callback");
    }

    #[test]
    fn env_example_lists_every_variable() {
        let example = include_str!("../../../.env.example");
        for name in Config::VARIABLES {
            assert!(example.lines().any(|l| l.starts_with(&format!("{name}="))), "{name} missing in .env.example");
        }
    }
}
