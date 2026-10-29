//! Askama templates. Every variable is HTML-escaped; the only exception is an addon's
//! `description_html`, which the API sanitized.

use askama::Template;
use axum::response::Html;
use serde_json::Value;

/// The logged-in visitor as the API describes them (`/v1/me`).
#[derive(Debug, Clone, Default)]
pub struct Visitor {
    pub login: String,
    pub avatar_url: String,
    pub admin: bool,
}

impl Visitor {
    pub fn from_me(me: &Value) -> Visitor {
        Visitor {
            login: me["login"].as_str().unwrap_or_default().to_string(),
            avatar_url: me["avatar_url"].as_str().unwrap_or_default().to_string(),
            admin: me["role"] == "admin",
        }
    }
}

pub fn render(template: &impl Template) -> Html<String> {
    Html(template.render().unwrap_or_else(|e| {
        tracing::error!(error = %e, "template failed");
        "Internal error".into()
    }))
}

#[derive(Template)]
#[template(path = "error.html")]
pub struct ErrorPage {
    pub title: String,
    pub message: String,
    pub user: Option<Visitor>,
    pub csrf: String,
}

#[derive(Template)]
#[template(path = "index.html")]
pub struct IndexPage {
    pub user: Option<Visitor>,
    pub csrf: String,
}

pub use crate::trust::Notice;

pub struct Card {
    pub slug: String,
    pub name: String,
    pub summary: String,
    pub author: String,
    pub icon_url: Option<String>,
    pub category: String,
    pub notice: Notice,
    pub downloads: i64,
}

#[derive(Template)]
#[template(path = "addons.html")]
pub struct AddonsPage {
    pub user: Option<Visitor>,
    pub csrf: String,
    pub q: String,
    pub category: String,
    pub trust: String,
    pub sort: String,
    pub total: i64,
    pub cards: Vec<Card>,
    pub prev: Option<String>,
    pub next: Option<String>,
}

pub struct FindingRow {
    pub severity: String,
    pub rule: String,
    pub place: String,
    pub message: String,
    pub snippet: String,
}

pub struct VersionRow {
    pub tag: String,
    pub name: String,
    pub published: String,
    pub prerelease: bool,
    pub deleted: bool,
    pub notice: Notice,
    pub sha256: String,
    pub asset_url: String,
    pub game: String,
    pub critical: i64,
    pub warn: i64,
}

pub struct Shot {
    pub url: String,
    pub caption: String,
    pub width: i64,
    pub height: i64,
}

pub struct Dep {
    pub slug: String,
    pub name: String,
    pub required: bool,
    pub available: bool,
    pub notice: Notice,
}

pub struct LatestBlock {
    pub row: VersionRow,
    /// GitHub directly, or the confirmation page for risky versions.
    pub href: String,
    pub findings: Vec<FindingRow>,
}

#[derive(Template)]
#[template(path = "addon.html")]
pub struct AddonPage {
    pub user: Option<Visitor>,
    pub csrf: String,
    pub canonical: String,
    pub slug: String,
    pub name: String,
    pub summary: String,
    pub author: String,
    pub claimed: bool,
    pub category: String,
    pub repo: String,
    /// Sanitized by the API (forge_core::markdown); the one value rendered unescaped.
    pub description_html: String,
    pub icon_url: Option<String>,
    pub shots: Vec<Shot>,
    pub contributors: Vec<String>,
    pub deps: Vec<Dep>,
    pub latest: Option<LatestBlock>,
}

#[derive(Template)]
#[template(path = "blocked.html")]
pub struct BlockedPage {
    pub user: Option<Visitor>,
    pub csrf: String,
    pub name: String,
    pub reason: String,
}

#[derive(Template)]
#[template(path = "versions.html")]
pub struct VersionsPage {
    pub user: Option<Visitor>,
    pub csrf: String,
    pub slug: String,
    pub name: String,
    /// (download link, version)
    pub rows: Vec<(String, VersionRow)>,
}

#[derive(Template)]
#[template(path = "version.html")]
pub struct VersionPage {
    pub user: Option<Visitor>,
    pub csrf: String,
    pub slug: String,
    pub name: String,
    pub row: VersionRow,
    pub href: String,
    pub findings: Vec<FindingRow>,
}

#[derive(Template)]
#[template(path = "download.html")]
pub struct DownloadPage {
    pub user: Option<Visitor>,
    pub csrf: String,
    pub slug: String,
    pub name: String,
    pub row: VersionRow,
    pub findings: Vec<FindingRow>,
}
