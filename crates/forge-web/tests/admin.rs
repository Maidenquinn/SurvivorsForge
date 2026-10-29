mod common;

use axum::http::StatusCode;
use common::{TestWeb, me};
use serde_json::{Value, json};
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, ResponseTemplate};

async fn get_json(web: &TestWeb, route: &str, body: Value) {
    Mock::given(method("GET")).and(path(route)).respond_with(ResponseTemplate::new(200).set_body_json(body)).mount(&web.api).await;
}

async fn expect(web: &TestWeb, verb: &str, route: &str, body: Value) {
    Mock::given(method(verb))
        .and(path(route))
        .and(body_partial_json(body))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&web.api)
        .await;
}

async fn as_admin() -> TestWeb {
    let mut web = TestWeb::new().await;
    web.login_as("tok", me("root", "admin")).await;
    // Low priority: a test can mount its own list.
    Mock::given(method("GET"))
        .and(path("/v1/admin/admins"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{ "github_id": 99, "login": "root", "by_config": true, "granted": false }])))
        .with_priority(10)
        .mount(&web.api)
        .await;
    get_json(&web, "/v1/admin/reports", json!([
        { "id": 3, "target_type": "addon", "target_id": 1, "addon": "lucky", "reason": "malware", "text": "Deletes files", "reporter": "eve", "created_at": "2026-05-01T10:00:00Z" }
    ]))
    .await;
    get_json(&web, "/v1/admin/review-queue", json!([
        { "version_id": 9, "tag": "v2", "trust": "flagged", "slug": "lucky", "name": "Lucky", "compare_url": "https://github.com/a/lucky/compare/v1...v2",
          "findings": [{ "rule": "lua.url", "severity": "warn", "file": "a.lua", "line": 3, "message": "Contains a web address", "snippet": "x" }] }
    ]))
    .await;
    web
}

#[tokio::test]
async fn non_admin_forbidden_everywhere() {
    let mut web = TestWeb::new().await;
    web.login_as("tok", me("eve", "user")).await;
    for page in ["/admin", "/admin/review", "/admin/reports", "/admin/users", "/admin/addons/new", "/admin/list/categories", "/admin/audit"] {
        assert_eq!(web.get(page).await.status, StatusCode::FORBIDDEN, "{page}");
    }
    assert_eq!(web.post_form("/admin/versions/9/verify", &[("note", "x")]).await.status, StatusCode::FORBIDDEN);
    let mut anonymous = TestWeb::new().await;
    assert_eq!(anonymous.get("/admin").await.location(), "/login");
}

#[tokio::test]
async fn queue_renders_findings_and_diff() {
    let mut web = as_admin().await;
    let index = web.get("/admin").await;
    assert!(index.body.contains("<strong>1</strong> version to review") && index.body.contains("<strong>1</strong> open report"));
    let page = web.get("/admin/review").await;
    for text in ["Lucky", "v2", "a.lua:3", "Contains a web address", "https://github.com/a/lucky/compare/v1...v2", "required: why the findings are fine"] {
        assert!(page.body.contains(text), "missing {text}");
    }
}

#[tokio::test]
async fn actions_ask_for_note() {
    let mut web = as_admin().await;
    expect(&web, "POST", "/v1/admin/versions/9/verification", json!({ "note": "URL is the wiki" })).await;
    let page = web.post_form("/admin/versions/9/verify", &[("note", "URL is the wiki")]).await;
    assert_eq!(page.status, StatusCode::SEE_OTHER);
    assert!(page.location().starts_with("/admin/review?msg="), "{}", page.location());

    Mock::given(method("POST"))
        .and(path("/v1/admin/versions/9/block"))
        .respond_with(ResponseTemplate::new(422).set_body_json(json!({ "error": "note_required", "message": "Admin actions need a note" })))
        .mount(&web.api)
        .await;
    let page = web.post_form("/admin/versions/9/block", &[("note", "")]).await;
    let follow = web.get(page.location()).await;
    assert!(follow.body.contains("Admin actions need a note"), "error shown on the page");

    expect(&web, "POST", "/v1/admin/reports/3/resolve", json!({ "status": "dismissed", "note": "fine" })).await;
    web.post_form("/admin/reports/3/resolve", &[("status", "dismissed"), ("note", "fine")]).await;
    assert!(web.get("/admin/reports").await.body.contains("Deletes files"));

    get_json(&web, "/v1/admin/users/eve", json!({ "github_id": 5, "login": "eve", "role": "user", "banned": false, "addons": 2 })).await;
    let users = web.get("/admin/users?login=eve").await;
    assert!(users.body.contains("Owns 2 addons") && users.body.contains("Ban eve"));
    expect(&web, "POST", "/v1/admin/users/5/ban", json!({ "note": "spam" })).await;
    let page = web.post_form("/admin/users/5/ban", &[("note", "spam"), ("login", "eve")]).await;
    assert!(page.location().starts_with("/admin/users?login=eve&msg="), "{}", page.location());
}

#[tokio::test]
async fn game_versions_and_categories_crud() {
    let mut web = as_admin().await;
    get_json(&web, "/v1/game-versions", json!([{ "id": 4, "name": "0.69.0", "sort_key": 10 }])).await;
    get_json(&web, "/v1/categories", json!([{ "name": "Unit Frames", "parent": null, "selectable": true, "sort_key": 40 }, { "name": "Classes", "parent": null, "selectable": false, "sort_key": 10 }, { "name": "Miscellaneous", "parent": null, "selectable": true, "sort_key": 60 }])).await;
    assert!(web.get("/admin/list/game-versions").await.body.contains("value=\"0.69.0\""));
    let page = web.get("/admin/list/categories").await;
    assert!(page.body.contains("value=\"Miscellaneous\""), "{}", page.body);
    assert!(page.body.contains("<option value=\"Classes\">Classes</option>"), "top-level categories offered as parents");

    expect(&web, "POST", "/v1/admin/categories", json!({ "name": "Death Knight", "sort_key": 10, "parent": "Classes" })).await;
    web.post_form("/admin/list/categories", &[("name", "Death Knight"), ("sort_key", "10"), ("parent", "Classes")]).await;

    expect(&web, "POST", "/v1/admin/game-versions", json!({ "name": "0.70.0", "sort_key": 20 })).await;
    web.post_form("/admin/list/game-versions", &[("name", "0.70.0"), ("sort_key", "20")]).await;
    expect(&web, "PATCH", "/v1/admin/game-versions/4", json!({ "name": "0.69", "sort_key": 11 })).await;
    web.post_form("/admin/list/game-versions/4/save", &[("name", "0.69"), ("sort_key", "11")]).await;
    Mock::given(method("DELETE")).and(path("/v1/admin/categories/Miscellaneous")).respond_with(ResponseTemplate::new(204)).expect(1).mount(&web.api).await;
    let page = web.post_form("/admin/list/categories/Miscellaneous/delete", &[]).await;
    assert!(page.location().starts_with("/admin/list/categories?msg="));
}

#[tokio::test]
async fn owner_transfer_form() {
    let mut web = as_admin().await;
    get_json(&web, "/v1/addons/lucky", json!({ "slug": "lucky", "name": "Lucky", "owner": null, "blocked": false })).await;
    let page = web.get("/admin/addons/lucky").await;
    assert!(page.body.contains("Nobody yet"));
    expect(&web, "POST", "/v1/admin/addons/lucky/owner", json!({ "login": "cadoky", "note": "author" })).await;
    let page = web.post_form("/admin/addons/lucky/owner", &[("login", "cadoky"), ("note", "author")]).await;
    assert!(page.location().starts_with("/admin/addons/lucky?msg="));
}

#[tokio::test]
async fn admin_adds_addon_without_app() {
    let mut web = as_admin().await;
    get_json(&web, "/v1/categories", json!([{ "name": "Unit Frames", "parent": null, "selectable": true, "sort_key": 40 }, { "name": "Classes", "parent": null, "selectable": false, "sort_key": 10 }, { "name": "Miscellaneous", "parent": null, "selectable": true, "sort_key": 60 }])).await;
    Mock::given(method("POST"))
        .and(path("/v1/admin/addons"))
        .and(body_partial_json(json!({ "repo": "cadoky/LuckyOdds", "author_name": "cadoky", "folders": ["LuckyOdds"] })))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({ "slug": "luckyodds" })))
        .expect(1)
        .mount(&web.api)
        .await;
    let fields = [
        ("repo", "cadoky/LuckyOdds"), ("author_name", "cadoky"), ("name", "LuckyOdds"), ("slug", "luckyodds"), ("summary", "Odds"),
        ("category", "Miscellaneous"), ("source_type", "release"), ("asset_pattern", "LuckyOdds-v*.zip"), ("folders", "LuckyOdds"),
    ];
    let page = web.post_form("/admin/addons/new", &fields).await;
    assert_eq!((page.status, page.location()), (StatusCode::SEE_OTHER, "/addons/luckyodds"), "{}", page.body);
}

#[tokio::test]
async fn audit_filters() {
    let mut web = as_admin().await;
    Mock::given(method("GET"))
        .and(path("/v1/admin/audit"))
        .and(wiremock::matchers::query_param("action", "ban_user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "total": 1, "entries": [
            { "admin": "root", "action": "ban_user", "target_type": "user", "target_id": "5", "note": "spam", "created_at": "2026-05-01T10:00:00Z" }
        ] })))
        .mount(&web.api)
        .await;
    let page = web.get("/admin/audit?action=ban_user").await;
    assert!(page.body.contains("ban_user") && page.body.contains("user 5") && page.body.contains("spam"));
}

#[tokio::test]
async fn admins_are_listed_and_granted_on_the_users_page() {
    let mut web = as_admin().await;
    get_json(&web, "/v1/admin/admins", json!([
        { "github_id": 99, "login": "root", "by_config": true, "granted": false },
        { "github_id": 7, "login": "mia", "by_config": false, "granted": true },
    ]))
    .await;
    get_json(&web, "/v1/admin/users/eve", json!({ "github_id": 5, "login": "eve", "role": "user", "banned": false, "addons": 0 })).await;
    let page = web.get("/admin/users?login=eve").await;
    assert!(page.body.contains("root") && page.body.contains("ADMIN_GITHUB_IDS"), "{}", page.body);
    assert!(page.body.contains("/admin/users/7/remove-admin"), "granted admins can lose the right");
    assert!(!page.body.contains("/admin/users/99/remove-admin"), "config admins cannot");
    assert!(page.body.contains("Make eve an admin"));

    expect(&web, "PUT", "/v1/admin/users/5/role", json!({ "admin": true, "note": "helps out" })).await;
    let done = web.post_form("/admin/users/5/make-admin", &[("note", "helps out"), ("login", "eve")]).await;
    assert!(done.location().starts_with("/admin/users?login=eve&msg="), "{}", done.location());
    expect(&web, "PUT", "/v1/admin/users/7/role", json!({ "admin": false, "note": "left" })).await;
    web.post_form("/admin/users/7/remove-admin", &[("note", "left"), ("login", "")]).await;
}
