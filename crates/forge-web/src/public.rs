//! The public pages: addon list and search, addon page, version history, version page and the
//! download confirmation for risky versions.

use crate::app::{AppState, visitor};
use crate::csrf::CsrfToken;
use crate::error::{WebError, WebResult, error_page};
use crate::templates::{
    AddonPage, AddonsPage, BlockedPage, Card, Dep, DownloadPage, FindingRow, LatestBlock, Shot, VersionPage, VersionRow, VersionsPage, render,
};
use crate::trust::{Notice, notice};
use axum::Extension;
use axum::extract::{Path, RawQuery, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum_extra::extract::PrivateCookieJar;
use serde_json::Value;

fn s(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn date(v: &Value) -> String {
    s(v).chars().take(10).collect()
}

/// Escapes a path segment for the API.
fn seg(text: &str) -> String {
    text.bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect()
}

fn range(min: &Value, max: &Value) -> String {
    match (min.as_str(), max.as_str()) {
        (Some(a), Some(b)) => format!("{a} to {b}"),
        (Some(a), None) => format!("{a} and later"),
        (None, Some(b)) => format!("up to {b}"),
        (None, None) => "Not stated".into(),
    }
}

fn version_row(v: &Value, verified_author: bool) -> VersionRow {
    VersionRow {
        tag: s(&v["tag"]),
        name: s(&v["name"]),
        published: date(&v["published_at"]),
        prerelease: v["prerelease"] == true,
        deleted: v["deleted_on_github"] == true,
        notice: notice(v["trust"].as_str().unwrap_or("unscanned"), verified_author, v["source"] == "missing"),
        sha256: s(&v["sha256"]),
        asset_url: s(&v["asset_url"]),
        game: range(&v["game_min"], &v["game_max"]),
        critical: v["findings"]["critical"].as_i64().unwrap_or(0),
        warn: v["findings"]["warn"].as_i64().unwrap_or(0),
    }
}

/// Where a download button points: GitHub directly, or the confirmation page first.
fn download_href(slug: &str, row: &VersionRow) -> String {
    if row.notice.confirm { format!("/addons/{slug}/versions/{}/download", seg(&row.tag)) } else { row.asset_url.clone() }
}

async fn findings(state: &AppState, slug: &str, tag: &str) -> WebResult<Vec<FindingRow>> {
    let report = state.api.get(&format!("/v1/addons/{}/versions/{}/findings", seg(slug), seg(tag)), None).await?;
    Ok(report["scan"]["findings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|f| f["severity"] != "info")
        .map(|f| FindingRow {
            severity: s(&f["severity"]),
            rule: s(&f["rule"]),
            place: match f["line"].as_i64() {
                Some(line) => format!("{}:{line}", s(&f["file"])),
                None => s(&f["file"]),
            },
            message: s(&f["message"]),
            snippet: s(&f["snippet"]),
        })
        .collect())
}

pub async fn addons(State(state): State<AppState>, jar: PrivateCookieJar, Extension(csrf): Extension<CsrfToken>, RawQuery(raw): RawQuery) -> WebResult<Html<String>> {
    let user = visitor(&state, &jar).await?.map(|(v, _)| v);
    let raw = raw.unwrap_or_default();
    let params: Vec<(String, String)> =
        reqwest::Url::parse(&format!("http://x/?{raw}")).map(|u| u.query_pairs().into_owned().collect()).unwrap_or_default();
    let param = |k: &str| params.iter().find(|(key, v)| key == k && !v.is_empty()).map(|(_, v)| v.clone());
    let query: Vec<(&str, String)> =
        ["q", "category", "trust", "sort", "page"].iter().filter_map(|k| param(k).map(|v| (*k, v))).collect();
    let api_query = reqwest::Url::parse_with_params("http://x/v1/addons", &query).map(|u| u.query().unwrap_or_default().to_string()).unwrap_or_default();
    let list = state.api.get(&format!("/v1/addons?{api_query}"), None).await?;

    let cards: Vec<Card> = list["addons"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|a| Card {
            slug: s(&a["slug"]),
            name: s(&a["name"]),
            summary: s(&a["summary"]),
            author: if a["owner"].is_object() { s(&a["owner"]["login"]) } else { s(&a["author"]) },
            icon_url: a["icon_url"].as_str().map(str::to_string),
            category: s(&a["category"]),
            notice: match a["trust"].as_str() {
                Some(trust) => notice(trust, a["verified_author"] == true, false),
                None => Notice { class: "", badge: "No release", text: "", downloadable: false, confirm: false },
            },
            downloads: a["downloads"].as_i64(),
            updated: s(&a["updated_at"]).chars().take(10).collect(),
        })
        .collect();
    let page = param("page").and_then(|p| p.parse().ok()).unwrap_or(1i64).max(1);
    let total = list["total"].as_i64().unwrap_or(0);
    let per_page = list["per_page"].as_i64().unwrap_or(20).max(1);
    let link = |p: i64| {
        let mut q: Vec<(&str, String)> = query.iter().filter(|(k, _)| *k != "page").cloned().collect();
        q.push(("page", p.to_string()));
        reqwest::Url::parse_with_params("http://x/addons", &q).map(|u| format!("/addons?{}", u.query().unwrap_or_default())).unwrap_or_default()
    };
    Ok(render(&AddonsPage {
        user,
        csrf: csrf.0,
        q: param("q").unwrap_or_default(),
        category: param("category").unwrap_or_default(),
        trust: param("trust").unwrap_or_default(),
        sort: param("sort").unwrap_or_default(),
        total,
        categories: crate::app::categories(&state).await?,
        cards,
        prev: (page > 1).then(|| link(page - 1)),
        next: (page * per_page < total).then(|| link(page + 1)),
    }))
}

pub async fn addon(State(state): State<AppState>, jar: PrivateCookieJar, Extension(csrf): Extension<CsrfToken>, Path(slug): Path<String>) -> WebResult<Response> {
    let user = visitor(&state, &jar).await?.map(|(v, _)| v);
    let a = state.api.get(&format!("/v1/addons/{}", seg(&slug)), None).await?;
    if a["blocked"] == true {
        let page = BlockedPage { user, csrf: csrf.0, name: s(&a["name"]), reason: s(&a["block_reason"]) };
        return Ok((StatusCode::OK, render(&page)).into_response());
    }
    let verified_author = a["verified_author"] == true;
    let versions = state.api.get(&format!("/v1/addons/{}/versions", seg(&slug)), None).await?;
    let latest = match a["latest_tag"].as_str() {
        Some(tag) => {
            let v = versions.as_array().into_iter().flatten().find(|v| v["tag"] == tag);
            match v {
                Some(v) => {
                    let row = version_row(v, verified_author);
                    let found = if row.critical + row.warn > 0 { findings(&state, &slug, tag).await? } else { Vec::new() };
                    Some(LatestBlock { href: download_href(&slug, &row), row, findings: found })
                }
                None => None,
            }
        }
        None => None,
    };
    let page = AddonPage {
        user,
        csrf: csrf.0,
        canonical: format!("{}/addons/{slug}", state.config.public_web_url),
        slug: slug.clone(),
        name: s(&a["name"]),
        summary: s(&a["summary"]),
        author: if a["owner"].is_object() { s(&a["owner"]["login"]) } else { s(&a["author"]) },
        claimed: a["claimed"] != false,
        category: s(&a["category"]),
        repo: s(&a["repo"]),
        description_html: s(&a["description_html"]),
        icon_url: a["icon_url"].as_str().map(str::to_string),
        shots: a["images"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|i| Shot { url: s(&i["url"]), caption: s(&i["caption"]), width: i["width"].as_i64().unwrap_or(0), height: i["height"].as_i64().unwrap_or(0) })
            .collect(),
        contributors: a["contributors"].as_array().into_iter().flatten().map(|c| s(&c["login"])).collect(),
        deps: a["dependencies"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|d| Dep {
                slug: s(&d["slug"]),
                name: s(&d["name"]),
                required: d["required"] == true,
                available: d["available"] == true,
                notice: notice(d["trust"].as_str().unwrap_or("unscanned"), false, false),
            })
            .collect(),
        latest,
    };
    Ok(render(&page).into_response())
}

pub async fn versions(State(state): State<AppState>, jar: PrivateCookieJar, Extension(csrf): Extension<CsrfToken>, Path(slug): Path<String>) -> WebResult<Html<String>> {
    let user = visitor(&state, &jar).await?.map(|(v, _)| v);
    let a = state.api.get(&format!("/v1/addons/{}", seg(&slug)), None).await?;
    if a["blocked"] == true {
        return Err(WebError::NotFound);
    }
    let list = state.api.get(&format!("/v1/addons/{}/versions", seg(&slug)), None).await?;
    let verified_author = a["verified_author"] == true;
    let rows = list.as_array().into_iter().flatten().map(|v| version_row(v, verified_author)).collect::<Vec<_>>();
    let rows = rows.into_iter().map(|r| (download_href(&slug, &r), r)).collect();
    Ok(render(&VersionsPage { user, csrf: csrf.0, slug, name: s(&a["name"]), rows }))
}

async fn version_data(state: &AppState, slug: &str, tag: &str) -> WebResult<(Value, VersionRow)> {
    let a = state.api.get(&format!("/v1/addons/{}", seg(slug)), None).await?;
    if a["blocked"] == true {
        return Err(WebError::NotFound);
    }
    let list = state.api.get(&format!("/v1/addons/{}/versions", seg(slug)), None).await?;
    let v = list.as_array().into_iter().flatten().find(|v| v["tag"] == tag).ok_or(WebError::NotFound)?;
    let row = version_row(v, a["verified_author"] == true);
    Ok((a, row))
}

pub async fn version(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Extension(csrf): Extension<CsrfToken>,
    Path((slug, tag)): Path<(String, String)>,
) -> WebResult<Html<String>> {
    let user = visitor(&state, &jar).await?.map(|(v, _)| v);
    let (a, row) = version_data(&state, &slug, &tag).await?;
    let found = findings(&state, &slug, &tag).await?;
    Ok(render(&VersionPage { user, csrf: csrf.0, href: download_href(&slug, &row), slug, name: s(&a["name"]), row, findings: found }))
}

pub async fn download(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Extension(csrf): Extension<CsrfToken>,
    Path((slug, tag)): Path<(String, String)>,
) -> WebResult<Response> {
    let user = visitor(&state, &jar).await?.map(|(v, _)| v);
    let (a, row) = version_data(&state, &slug, &tag).await?;
    if !row.notice.downloadable {
        return Ok(error_page(StatusCode::FORBIDDEN, "Not downloadable", row.notice.text));
    }
    if !row.notice.confirm {
        return Ok(Redirect::to(&row.asset_url).into_response());
    }
    let found = findings(&state, &slug, &tag).await?;
    Ok(render(&DownloadPage { user, csrf: csrf.0, slug, name: s(&a["name"]), row, findings: found }).into_response())
}
