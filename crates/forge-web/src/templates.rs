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

/// A category as lists and selects show it: subcategories read "Classes: Warrior".
#[derive(Debug, Clone, PartialEq)]
pub struct CategoryOption {
    pub name: String,
    pub label: String,
    pub parent: String,
    pub sort_key: String,
    /// Addons can go into it (it has no subcategories).
    pub selectable: bool,
}

impl CategoryOption {
    /// The API's `/v1/categories`, in its order.
    pub fn list(api: &Value) -> Vec<CategoryOption> {
        let text = |v: &Value| v.as_str().unwrap_or_default().to_string();
        api.as_array()
            .into_iter()
            .flatten()
            .map(|c| {
                let (name, parent) = (text(&c["name"]), text(&c["parent"]));
                CategoryOption {
                    label: if parent.is_empty() { name.clone() } else { format!("{parent}: {name}") },
                    sort_key: c["sort_key"].as_i64().map(|k| k.to_string()).unwrap_or_default(),
                    selectable: c["selectable"] != false,
                    name,
                    parent,
                }
            })
            .collect()
    }
}

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
    /// For the filter, all of them.
    pub categories: Vec<CategoryOption>,
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

/// Form errors by field name; `form` holds the ones that belong to no field.
#[derive(Debug, Default)]
pub struct Errors(pub std::collections::HashMap<String, String>);

impl Errors {
    pub fn get(&self, field: &str) -> &str {
        self.0.get(field).map(String::as_str).unwrap_or_default()
    }
}

/// What the visitor may change on one addon.
#[derive(Debug, Clone, Copy, Default)]
pub struct Perms {
    pub owner: bool,
    pub edit_description: bool,
    pub edit_images: bool,
}

pub struct MyAddon {
    pub slug: String,
    pub name: String,
    pub role: String,
    pub notice: Notice,
    pub has_version: bool,
    pub findings: i64,
    pub blocked: bool,
    pub locked: bool,
}

#[derive(Template)]
#[template(path = "creator/my_addons.html")]
pub struct MyAddonsPage {
    pub user: Option<Visitor>,
    pub csrf: String,
    pub rows: Vec<MyAddon>,
}

#[derive(Template)]
#[template(path = "creator/new.html")]
pub struct NewAddonPage {
    pub user: Option<Visitor>,
    pub csrf: String,
    pub install_url: String,
    pub categories: Vec<CategoryOption>,
    pub form: crate::creator::NewAddonForm,
    pub errors: Errors,
}

#[derive(Template)]
#[template(path = "creator/edit.html")]
pub struct EditAddonPage {
    pub user: Option<Visitor>,
    pub csrf: String,
    pub slug: String,
    pub name: String,
    pub category: String,
    pub categories: Vec<CategoryOption>,
    pub source_type: String,
    pub asset_pattern: String,
    pub folders: String,
    pub summary: String,
    pub description: String,
    /// The preview, sanitized by forge_core::markdown like the API does it.
    pub description_html: String,
    pub perms: Perms,
    pub errors: Errors,
    pub saved: bool,
}

pub struct ImageRow {
    pub id: i64,
    pub url: String,
    pub caption: String,
}

#[derive(Template)]
#[template(path = "creator/images.html")]
pub struct ImagesPage {
    pub user: Option<Visitor>,
    pub csrf: String,
    pub slug: String,
    pub name: String,
    pub icon_url: Option<String>,
    pub images: Vec<ImageRow>,
    pub message: Option<String>,
    pub error: Option<String>,
}

pub struct MemberRow {
    pub github_id: i64,
    pub login: String,
    pub description: bool,
    pub images: bool,
}

#[derive(Template)]
#[template(path = "creator/members.html")]
pub struct MembersPage {
    pub user: Option<Visitor>,
    pub csrf: String,
    pub slug: String,
    pub members: Vec<MemberRow>,
    pub error: Option<String>,
}

#[derive(Template)]
#[template(path = "creator/dependencies.html")]
pub struct DepsPage {
    pub user: Option<Visitor>,
    pub csrf: String,
    pub slug: String,
    /// (slug, name, required)
    pub deps: Vec<(String, String, bool)>,
    pub error: Option<String>,
}

#[derive(Template)]
#[template(path = "creator/version.html")]
pub struct VersionEditPage {
    pub user: Option<Visitor>,
    pub csrf: String,
    pub slug: String,
    pub tag: String,
    pub game_min: String,
    pub game_max: String,
    pub games: Vec<String>,
    pub error: Option<String>,
}

#[derive(Template)]
#[template(path = "creator/delete.html")]
pub struct DeleteAddonPage {
    pub user: Option<Visitor>,
    pub csrf: String,
    pub slug: String,
    pub error: Option<String>,
}

#[derive(Template)]
#[template(path = "admin/index.html")]
pub struct AdminIndexPage {
    pub user: Option<Visitor>,
    pub csrf: String,
    pub queue: usize,
    pub reports: usize,
}

pub struct ReviewItem {
    pub version_id: i64,
    pub slug: String,
    pub name: String,
    pub tag: String,
    pub trust: String,
    pub compare_url: Option<String>,
    pub findings: Vec<FindingRow>,
}

#[derive(Template)]
#[template(path = "admin/review.html")]
pub struct AdminReviewPage {
    pub user: Option<Visitor>,
    pub csrf: String,
    pub items: Vec<ReviewItem>,
    pub msg: Option<String>,
    pub error: Option<String>,
}

pub struct ReportRow {
    pub id: i64,
    pub target: String,
    pub addon: String,
    pub reason: String,
    pub text: String,
    pub reporter: String,
    pub created: String,
}

#[derive(Template)]
#[template(path = "admin/reports.html")]
pub struct AdminReportsPage {
    pub user: Option<Visitor>,
    pub csrf: String,
    pub rows: Vec<ReportRow>,
    pub msg: Option<String>,
    pub error: Option<String>,
}

pub struct UserRow {
    pub github_id: i64,
    pub login: String,
    pub role: String,
    pub banned: bool,
    pub ban_reason: String,
    pub addons: i64,
}

#[derive(Template)]
#[template(path = "admin/users.html")]
pub struct AdminUsersPage {
    pub user: Option<Visitor>,
    pub csrf: String,
    pub admins: Vec<AdminRow>,
    pub login: String,
    pub found: Option<UserRow>,
    pub not_found: bool,
    pub msg: Option<String>,
    pub error: Option<String>,
}

pub struct AdminRow {
    pub github_id: i64,
    pub login: String,
    /// From ADMIN_GITHUB_IDS: only the configuration can take it away.
    pub by_config: bool,
}

#[derive(Template)]
#[template(path = "admin/addon.html")]
pub struct AdminAddonPage {
    pub user: Option<Visitor>,
    pub csrf: String,
    pub slug: String,
    pub name: String,
    pub owner: String,
    pub blocked: bool,
    pub block_reason: String,
    pub msg: Option<String>,
    pub error: Option<String>,
}

#[derive(Template)]
#[template(path = "admin/add_addon.html")]
pub struct AdminAddAddonPage {
    pub user: Option<Visitor>,
    pub csrf: String,
    pub categories: Vec<CategoryOption>,
    pub form: crate::admin::AdminAddonForm,
    pub error: Option<String>,
}

#[derive(Template)]
#[template(path = "admin/list.html")]
pub struct AdminListPage {
    pub user: Option<Visitor>,
    pub csrf: String,
    /// `game-versions` or `categories`.
    pub kind: String,
    pub title: String,
    /// (key, name, sort key, parent); the parent only for categories.
    pub rows: Vec<(String, String, String, String)>,
    /// Categories a new or moved category can sit under.
    pub parents: Vec<String>,
    pub msg: Option<String>,
    pub error: Option<String>,
}

pub struct AuditRow {
    pub when: String,
    pub admin: String,
    pub action: String,
    pub target: String,
    pub note: String,
}

#[derive(Template)]
#[template(path = "admin/audit.html")]
pub struct AdminAuditPage {
    pub user: Option<Visitor>,
    pub csrf: String,
    pub filter_admin: String,
    pub filter_action: String,
    pub filter_target_type: String,
    pub filter_target_id: String,
    pub entries: Vec<AuditRow>,
    pub prev: Option<String>,
    pub next: Option<String>,
}

/// An addon in the pack picker: its current version.
pub struct PackChoice {
    pub slug: String,
    pub name: String,
    pub tag: String,
    /// Empty when the release size is unknown (source zips).
    pub size: String,
    pub notice: Notice,
    pub checked: bool,
}

#[derive(Template)]
#[template(path = "packs/new.html")]
pub struct NewPackPage {
    pub user: Option<Visitor>,
    pub csrf: String,
    pub name: String,
    pub choices: Vec<PackChoice>,
    pub error: Option<String>,
}

pub struct PackEntry {
    pub slug: String,
    pub name: String,
    pub tag: String,
    pub available: bool,
    pub notice: Notice,
    pub folders: String,
}

#[derive(Template)]
#[template(path = "packs/show.html")]
pub struct PackPage {
    pub user: Option<Visitor>,
    pub csrf: String,
    pub id: String,
    /// The full address to share.
    pub link: String,
    pub name: String,
    pub created_by: String,
    pub created: String,
    pub available: bool,
    pub size: Option<String>,
    pub entries: Vec<PackEntry>,
}

#[derive(Template)]
#[template(path = "packs/building.html")]
pub struct PackBuildingPage {
    pub user: Option<Visitor>,
    pub csrf: String,
    pub id: String,
}
