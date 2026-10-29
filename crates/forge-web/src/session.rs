//! The browser session: the API token and the CSRF token live in encrypted, HttpOnly cookies.

use axum_extra::extract::PrivateCookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};

pub const SESSION_COOKIE: &str = "forge_session";
pub const CSRF_COOKIE: &str = "forge_csrf";

pub fn cookie(name: &'static str, value: String, secure: bool) -> Cookie<'static> {
    Cookie::build((name, value)).path("/").http_only(true).secure(secure).same_site(SameSite::Lax).build()
}

/// The API token of the logged-in visitor.
pub fn token(jar: &PrivateCookieJar) -> Option<String> {
    jar.get(SESSION_COOKIE).map(|c| c.value().to_string())
}

pub fn removal(name: &'static str) -> Cookie<'static> {
    Cookie::build((name, "")).path("/").build()
}

/// A `Set-Cookie` value that deletes the session, for answers built without a jar.
pub fn expired_session_header() -> String {
    format!("{SESSION_COOKIE}=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax")
}
