mod common;

use axum::http::StatusCode;
use common::TestWeb;
use serde_json::{Value, json};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

const ASSET: &str = "https://github.com/alice/lucky/releases/download/v1/lucky-v1.zip";

async fn mock(web: &TestWeb, route: &str, body: Value) {
    Mock::given(method("GET")).and(path(route)).respond_with(ResponseTemplate::new(200).set_body_json(body)).mount(&web.api).await;
}

fn addon(trust: &str) -> Value {
    json!({
        "slug": "lucky", "name": "Lucky Odds", "summary": "Odds for rare cards", "category": "Miscellaneous", "repo": "alice/lucky",
        "owner": { "login": "alice" }, "author": "alice", "claimed": true, "blocked": false,
        "description_html": "<h2>Features</h2><p>Shows odds.</p>", "description_md": "## Features",
        "icon_url": null, "images": [{ "id": 1, "url": "https://web.test/img/a.webp", "width": 800, "height": 450, "caption": "Card screen" }],
        "contributors": [{ "login": "bob" }],
        "dependencies": [{ "slug": "lib", "name": "Lib", "required": true, "trust": "verified", "available": true }],
        "trust": trust, "latest_tag": "v1", "verified_author": false, "findings": { "critical": 0, "warn": 1, "info": 0 }
    })
}

fn versions(trust: &str, source: &str) -> Value {
    json!([
        { "id": 2, "tag": "v1", "name": "First", "prerelease": false, "published_at": "2026-05-01T10:00:00Z", "asset_url": ASSET,
          "sha256": "ab".repeat(32), "trust": trust, "source": source, "deleted_on_github": false,
          "findings": { "critical": 0, "warn": if trust == "flagged" { 1 } else { 0 }, "info": 0 }, "game_min": "0.69.0", "game_max": null },
        { "id": 1, "tag": "v0", "name": "Old", "prerelease": true, "published_at": "2026-04-01T10:00:00Z", "asset_url": "https://github.com/old.zip",
          "sha256": null, "trust": "unscanned", "source": "available", "deleted_on_github": true,
          "findings": { "critical": 0, "warn": 0, "info": 0 }, "game_min": null, "game_max": null }
    ])
}

fn findings() -> Value {
    json!({ "tag": "v1", "scan": { "result": "flagged", "findings": [
        { "rule": "lua.url", "severity": "warn", "file": "Lucky/Lucky.lua", "line": 12, "message": "Contains a web address", "snippet": "local u = 'https://evil.test'" }
    ] } })
}

async fn with_addon(trust: &str, source: &str) -> TestWeb {
    let web = TestWeb::new().await;
    mock(&web, "/v1/addons/lucky", addon(trust)).await;
    mock(&web, "/v1/addons/lucky/versions", versions(trust, source)).await;
    mock(&web, "/v1/addons/lucky/versions/v1/findings", findings()).await;
    web
}

#[tokio::test]
async fn list_renders_items_without_js() {
    let mut web = TestWeb::new().await;
    mock(&web, "/v1/categories", categories()).await;
    Mock::given(method("GET"))
        .and(path("/v1/addons"))
        .and(query_param("q", "lucky"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "total": 1, "page": 1, "per_page": 20, "addons": [
            { "slug": "lucky", "name": "Lucky Odds", "summary": "Odds", "category": "Miscellaneous", "trust": "flagged", "icon_url": null,
              "owner": { "login": "alice" }, "author": "alice", "verified_author": false, "downloads": 42, "updated_at": "2026-05-01" }
        ] })))
        .mount(&web.api)
        .await;
    let page = web.get("/addons?q=lucky").await;
    assert_eq!(page.status, StatusCode::OK, "{}", page.body);
    // Only the results scroll; search and filters stay in view.
    for text in ["Lucky Odds", "by alice", "42 downloads", "Flagged", "<form class=\"search\" method=\"get\" action=\"/addons\"", "<div id=\"results\" class=\"list\">",
        "href=\"https://github.com/Rezondes/SurvivorsForge\"", "href=\"https://rezondes.github.io/SurvivorsForge/\""] {
        assert!(page.body.contains(text), "missing {text}");
    }

    Mock::given(method("GET")).and(path("/v1/addons")).respond_with(ResponseTemplate::new(200).set_body_json(json!({ "total": 0, "addons": [] }))).mount(&web.api).await;
    assert!(web.get("/addons?q=nothing").await.body.contains("No addon matches"));
}

#[tokio::test]
async fn addons_without_a_download_count_show_their_last_update() {
    let mut web = TestWeb::new().await;
    mock(&web, "/v1/categories", categories()).await;
    mock(&web, "/v1/addons", json!({ "total": 1, "page": 1, "per_page": 20, "addons": [
        { "slug": "tagged", "name": "Tagged", "summary": "From tags", "category": "Miscellaneous", "trust": "scanned", "icon_url": null,
          "owner": null, "author": "bob", "verified_author": false, "downloads": null, "updated_at": "2026-05-01T10:00:00Z" }
    ] }))
    .await;
    let page = web.get("/addons").await;
    assert!(page.body.contains("Updated 2026-05-01") && !page.body.contains("downloads</span>"), "{}", page.body);
}

fn categories() -> Value {
    json!([
        { "name": "Classes", "parent": null, "selectable": false, "sort_key": 10 },
        { "name": "Warrior", "parent": "Classes", "selectable": true, "sort_key": 1 },
        { "name": "Combat", "parent": null, "selectable": true, "sort_key": 20 },
    ])
}

#[tokio::test]
async fn category_filter_lists_subcategories_and_reaches_the_api() {
    let mut web = TestWeb::new().await;
    mock(&web, "/v1/categories", categories()).await;
    Mock::given(method("GET"))
        .and(path("/v1/addons"))
        .and(query_param("category", "Classes"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "total": 0, "addons": [] })))
        .expect(1)
        .mount(&web.api)
        .await;
    let page = web.get("/addons?category=Classes").await;
    assert_eq!(page.status, StatusCode::OK, "{}", page.body);
    assert!(page.body.contains(r#"<option value="Classes" selected>Classes</option>"#), "{}", page.body);
    assert!(page.body.contains(r#"<option value="Warrior" >Classes: Warrior</option>"#));
    assert!(page.body.contains("All categories"));
}

#[tokio::test]
async fn addon_page_sections() {
    let mut web = with_addon("scanned", "available").await;
    let page = web.get("/addons/lucky").await;
    assert_eq!(page.status, StatusCode::OK, "{}", page.body);
    for text in ["<h2>Features</h2>", "Card screen", "bob", "Lib", "(required)", "0.69.0 and later", "Version v1", "Miscellaneous"] {
        assert!(page.body.contains(text), "missing {text}");
    }
}

#[tokio::test]
async fn admins_get_a_manage_link_on_addon_pages() {
    let mut web = with_addon("scanned", "available").await;
    assert!(!web.get("/addons/lucky").await.body.contains("/admin/addons/lucky"));
    web.login_as("tok", common::me("root", "admin")).await;
    assert!(web.get("/addons/lucky").await.body.contains("/admin/addons/lucky"));
}

#[tokio::test]
async fn the_repo_owner_may_claim_an_unclaimed_addon() {
    async fn unclaimed() -> TestWeb {
        let web = TestWeb::new().await;
        let mut a = addon("scanned");
        a["claimed"] = json!(false);
        a["owner"] = Value::Null;
        mock(&web, "/v1/addons/lucky", a).await;
        mock(&web, "/v1/addons/lucky/versions", versions("scanned", "available")).await;
        web
    }
    let claim = r#"action="/addons/lucky/claim""#;
    let mut web = unclaimed().await;
    assert!(!web.get("/addons/lucky").await.body.contains(claim), "not for visitors");
    web.login_as("bob-token", common::me("bob", "user")).await;
    assert!(!web.get("/addons/lucky").await.body.contains(claim), "not for others");

    // GitHub logins ignore case; the repo is alice/lucky.
    let mut web = unclaimed().await;
    web.login_as("alice-token", common::me("ALICE", "user")).await;
    let page = web.get("/addons/lucky?error=Install+the+App+first").await;    assert!(page.body.contains(claim) && page.body.contains("Claim it"), "the repo alice/lucky is hers: {}", page.body);
    assert!(page.body.contains("Install the App first"), "the claim error shows");
}

#[tokio::test]
async fn trust_notice_per_level() {
    let cases = [
        ("unscanned", "available", "Not checked automatically yet", true),
        ("flagged", "available", "found something suspicious", true),
        ("scanned", "available", "Checked automatically, nothing found", true),
        ("verified", "available", "Verified by an admin", true),
        ("blocked", "available", "An admin blocked this version", false),
        ("source_changed", "available", "was changed after it was checked", false),
        ("scanned", "missing", "no longer available on GitHub", false),
    ];
    for (trust, source, text, download) in cases {
        let mut web = with_addon(trust, source).await;
        let page = web.get("/addons/lucky").await;
        assert!(page.body.contains(text), "{trust}/{source}: missing {text}");
        assert_eq!(page.body.contains("Download from GitHub"), download, "{trust}/{source}");
    }
    let mut web = with_addon("flagged", "available").await;
    let page = web.get("/addons/lucky").await;
    assert!(page.body.contains("Lucky/Lucky.lua:12") && page.body.contains("Contains a web address"), "findings listed");
}

#[tokio::test]
async fn risky_download_needs_confirmation() {
    let mut web = with_addon("flagged", "available").await;
    let page = web.get("/addons/lucky").await;
    assert!(page.body.contains("href=\"/addons/lucky/versions/v1/download\""));
    let confirm = web.get("/addons/lucky/versions/v1/download").await;
    assert_eq!(confirm.status, StatusCode::OK);
    assert!(confirm.body.contains("I understand, download from GitHub") && confirm.body.contains(ASSET));

    let mut safe = with_addon("verified", "available").await;
    assert!(safe.get("/addons/lucky").await.body.contains(&format!("href=\"{ASSET}\"")), "direct link");
    let direct = safe.get("/addons/lucky/versions/v1/download").await;
    assert_eq!((direct.status, direct.location()), (StatusCode::SEE_OTHER, ASSET));

    let mut blocked = with_addon("blocked", "available").await;
    assert_eq!(blocked.get("/addons/lucky/versions/v1/download").await.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn version_history() {
    let mut web = with_addon("scanned", "available").await;
    let page = web.get("/addons/lucky/versions").await;
    for text in ["v1", "v0", "2026-05-01", "Pre-release", "Removed on GitHub", &"ab".repeat(6), "0.69.0 and later", "Not stated"] {
        assert!(page.body.contains(text), "missing {text}");
    }
    let version = web.get("/addons/lucky/versions/v1").await;
    assert_eq!(version.status, StatusCode::OK);
    assert!(version.body.contains("lua.url"));
    assert!(version.body.contains(&"ab".repeat(32)), "full hash on the version page");
}

#[tokio::test]
async fn seo_meta() {
    let mut web = with_addon("scanned", "available").await;
    let page = web.get("/addons/lucky").await;
    assert!(page.body.contains("<title>Lucky Odds - SurvivorsForge</title>"));
    assert!(page.body.contains("<meta name=\"description\" content=\"Odds for rare cards\">"));
    assert!(page.body.contains("<link rel=\"canonical\" href=\"https://web.test/addons/lucky\">"));
}

#[tokio::test]
async fn unknown_slug_404_and_blocked_page() {
    let mut web = TestWeb::new().await;
    Mock::given(method("GET"))
        .and(path("/v1/addons/ghost"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({ "error": "not_found", "message": "Not found" })))
        .mount(&web.api)
        .await;
    assert_eq!(web.get("/addons/ghost").await.status, StatusCode::NOT_FOUND);

    mock(&web, "/v1/addons/bad", json!({ "slug": "bad", "name": "Bad", "blocked": true, "block_reason": "malware" })).await;
    let page = web.get("/addons/bad").await;
    assert!(page.body.contains("An admin blocked this addon") && page.body.contains("malware"));
    assert!(!page.body.contains("Download"));
}

#[tokio::test]
async fn api_down_shows_error_page() {
    let mut web = TestWeb::new().await;
    Mock::given(method("GET")).and(path("/v1/addons")).respond_with(ResponseTemplate::new(500).set_body_json(json!({ "error": "internal", "message": "Internal server error" }))).mount(&web.api).await;
    let page = web.get("/addons").await;
    assert_eq!(page.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(page.body.contains("Something went wrong"));
}

#[tokio::test]
async fn file_pickers_look_like_the_site() {
    // The browser's grey "Choose file" button gets the outlined gold look of the other buttons.
    let mut web = TestWeb::new().await;
    let css = web.get("/static/app.css").await.body;
    assert!(css.contains("::file-selector-button"), "no style for the file picker button");
}

#[tokio::test]
async fn scrollbars_use_the_theme() {
    // Thin gold-brown thumbs on the dark field colour, like the launcher's scroll panels.
    let mut web = TestWeb::new().await;
    let css = web.get("/static/app.css").await.body;
    assert!(css.contains("scrollbar-color: var(--outline) var(--field)"), "no themed scrollbars");
}

#[tokio::test]
async fn card_lists_switch_between_tiles_and_a_list() {
    // The choice lives in the browser (view.js, localStorage); the pages only offer the switch.
    let mut web = TestWeb::new().await;
    mock(&web, "/v1/categories", categories()).await;
    mock(&web, "/v1/addons", json!({ "total": 0, "addons": [] })).await;
    mock(&web, "/v1/packs", json!({ "total": 0, "packs": [] })).await;
    for page in ["/addons", "/packs"] {
        let body = web.get(page).await.body;
        assert!(body.contains(r#"data-set-view="list""#) && body.contains(r#"data-set-view="tiles""#), "{page}: {body}");
        assert!(body.contains(r#"<script src="/static/view.js"></script>"#), "{page}: before the page is drawn");
    }
    let script = web.get("/static/view.js").await;
    assert!(script.headers["content-type"].to_str().unwrap().starts_with("text/javascript") && script.body.contains("localStorage"));
}

#[tokio::test]
async fn searching_shows_that_it_loads() {
    // While htmx fetches new results the old ones are dimmed, so a slow answer does not look frozen.
    let mut web = TestWeb::new().await;
    mock(&web, "/v1/categories", categories()).await;
    mock(&web, "/v1/addons", json!({ "total": 0, "addons": [] })).await;
    mock(&web, "/v1/packs", json!({ "total": 0, "packs": [] })).await;
    for page in ["/addons", "/packs"] {
        assert!(web.get(page).await.body.contains(r##"hx-indicator="#results""##), "{page}");
    }
    assert!(web.get("/static/app.css").await.body.contains("#results.htmx-request"));
}
