mod common;

use axum::http::StatusCode;
use common::TestApp;
use forge_api::github::types::User;
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test]
async fn my_addons_lists_own_and_shared(pool: PgPool) {
    let app = TestApp::new(pool);
    let alice = app.publish(1, "alice", "lucky").await;
    let bob = app.publish(2, "bob", "plates").await;
    app.github.data().users.insert("alice".into(), User { id: 1, login: "alice".into(), avatar_url: String::new() });
    app.post("/v1/addons/plates/members", Some(&bob), json!({ "login": "alice" })).await;
    app.publish(3, "carol", "other").await;

    let mine = app.get("/v1/me/addons", Some(&alice)).await.json();
    let rows: Vec<(String, String)> = mine.as_array().unwrap().iter().map(|a| (a["slug"].as_str().unwrap().into(), a["role"].as_str().unwrap().into())).collect();
    assert_eq!(rows, [("lucky".to_string(), "owner".to_string()), ("plates".to_string(), "contributor".to_string())]);
    assert_eq!(app.get("/v1/me/addons", None).await.status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn my_addons_offer_unclaimed_addons_of_my_repos(pool: PgPool) {
    // An addon an admin added without owner, from a repo of alice (GitHub logins ignore case).
    let app = TestApp::new(pool);
    app.publish(3, "carol", "orphan").await;
    app.publish(4, "dave", "foreign").await;
    sqlx::query("UPDATE addons SET owner_id = NULL, repo_full_name = 'ALICE/orphan' WHERE slug = 'orphan'").execute(&app.pool).await.unwrap();
    sqlx::query("UPDATE addons SET owner_id = NULL WHERE slug = 'foreign'").execute(&app.pool).await.unwrap();
    let alice = app.login(1, "alice").await;

    let mine = app.get("/v1/me/addons", Some(&alice)).await.json();
    let rows: Vec<(String, String)> = mine.as_array().unwrap().iter().map(|a| (a["slug"].as_str().unwrap().into(), a["role"].as_str().unwrap().into())).collect();
    assert_eq!(rows, [("orphan".to_string(), "claimable".to_string())], "only unclaimed addons of her own repos");
}

#[sqlx::test]
async fn permissions_and_categories(pool: PgPool) {
    let app = TestApp::new(pool);
    let alice = app.publish(1, "alice", "lucky").await;
    let bob = app.login(2, "bob").await;
    assert_eq!(app.get("/v1/addons/lucky/permissions", Some(&alice)).await.json()["owner"], true);
    let p = app.get("/v1/addons/lucky/permissions", Some(&bob)).await.json();
    assert_eq!((p["owner"].as_bool(), p["edit_images"].as_bool()), (Some(false), Some(false)));
    let list = app.get("/v1/categories", None).await.json();
    let names: Vec<String> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|c| match c["parent"].as_str() {
            Some(parent) => format!("{parent}/{}", c["name"].as_str().unwrap()),
            None => c["name"].as_str().unwrap().to_string(),
        })
        .collect();
    assert_eq!(names, [
        "Classes", "Classes/Warrior", "Classes/Paladin", "Classes/Hunter", "Classes/Rogue", "Classes/Priest", "Classes/Shaman",
        "Classes/Mage", "Classes/Warlock", "Classes/Druid", "Combat", "Libraries", "Unit Frames", "Action Bars", "Miscellaneous",
    ]);
    assert_eq!((list[0]["selectable"].as_bool(), list[1]["selectable"].as_bool(), list[10]["selectable"].as_bool()), (Some(false), Some(true), Some(true)));
}

#[sqlx::test]
async fn addons_go_into_categories_without_subcategories(pool: PgPool) {
    let app = TestApp::new(pool);
    let alice = app.publish(1, "alice", "lucky").await;
    assert_eq!(app.patch("/v1/addons/lucky", Some(&alice), json!({ "category": "Classes" })).await.error(), "category_has_subcategories");
    assert_eq!(app.patch("/v1/addons/lucky", Some(&alice), json!({ "category": "Nope" })).await.error(), "unknown_category");
    assert_eq!(app.patch("/v1/addons/lucky", Some(&alice), json!({ "category": "warlock" })).await.status, StatusCode::OK);
}

#[sqlx::test]
async fn whether_the_app_is_installed_on_my_account(pool: PgPool) {
    let app = TestApp::new(pool);
    let alice = app.login(1, "alice").await;
    assert_eq!(app.get("/v1/me/installation", None).await.status, StatusCode::UNAUTHORIZED);
    let none = app.get("/v1/me/installation", Some(&alice)).await.json();
    assert_eq!(none, json!({ "installed": false, "manage_url": null }));

    app.github.data().installations.insert("alice/lucky".into(), 77);
    let mine = app.get("/v1/me/installation", Some(&alice)).await.json();
    assert_eq!(mine, json!({ "installed": true, "manage_url": "https://github.com/settings/installations/77" }));
}

fn addon_zip(entries: &[(&str, &str)]) -> Vec<u8> {
    use std::io::Write;
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, text) in entries {
        zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(text.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

fn gh_release(tag: &str, asset: &str) -> forge_api::github::types::Release {
    serde_json::from_value(json!({
        "id": 1, "tag_name": tag, "draft": false, "prerelease": false, "published_at": "2026-05-01T00:00:00Z",
        "assets": [{ "id": 2, "name": asset, "size": 10, "browser_download_url": format!("https://github.com/dl/{asset}") }]
    }))
    .unwrap()
}

#[sqlx::test]
async fn my_repos_and_what_the_form_can_fill_in(pool: PgPool) {
    let app = TestApp::with_config(pool, |c| c.admin_github_ids = vec![99]);
    let alice = app.publish(1, "alice", "taken").await;
    app.github.add_repo(101, "alice/LuckyOdds", "alice");
    app.github.add_repo(102, "alice/TagAddon", "alice");
    app.github.add_repo(103, "bob/Other", "bob");
    // GitHub's public list: the App need not be installed on a repo to find it.
    app.github.data().installations.clear();

    let names = |reply: serde_json::Value| -> Vec<String> { reply["repos"].as_array().unwrap().iter().map(|r| r["full_name"].as_str().unwrap().to_string()).collect() };
    assert_eq!(names(app.get("/v1/me/repos", Some(&alice)).await.json()), ["alice/LuckyOdds", "alice/TagAddon"], "own account, without repos that already have an addon");
    assert_eq!(names(app.get("/v1/me/repos?q=tag", Some(&alice)).await.json()), ["alice/TagAddon"], "by part of the name");
    assert_eq!(app.get("/v1/me/repos?owner=bob", Some(&alice)).await.status, StatusCode::FORBIDDEN, "only admins search other accounts");
    let root = app.login(99, "root").await;
    assert_eq!(names(app.get("/v1/me/repos?owner=bob", Some(&root)).await.json()), ["bob/Other"], "admins add addons of any account");

    {
        let mut data = app.github.data();
        data.releases.insert(101, vec![gh_release("v0.7.0", "LuckyOdds-v0.7.0.zip")]);
        data.downloads.insert(
            "https://github.com/dl/LuckyOdds-v0.7.0.zip".into(),
            addon_zip(&[("LuckyOdds/LuckyOdds.toc", "## Title: Lucky Odds\n## Notes: Odds for rare cards\n")]),
        );
        data.tags.insert(102, vec![serde_json::from_value(json!({ "name": "v2.0", "commit": { "sha": "abc" } })).unwrap()]);
        data.downloads.insert(
            "https://codeload.github.com/alice/TagAddon/zip/v2.0".into(),
            addon_zip(&[("TagAddon-2.0/TagAddon.toc", "## Title: Tag Addon\n")]),
        );
    }
    let s = app.get("/v1/me/repos/alice/LuckyOdds/suggestion", Some(&alice)).await.json();
    assert_eq!(s["name"], "Lucky Odds");
    assert_eq!(s["slug"], "luckyodds");
    assert_eq!(s["summary"], "Odds for rare cards");
    assert_eq!((s["source_type"].as_str(), s["asset_pattern"].as_str()), (Some("release"), Some("LuckyOdds-v*.zip")));
    assert_eq!(s["folders"], json!(["LuckyOdds"]));
    assert_eq!(s["repo"], "alice/LuckyOdds");

    let t = app.get("/v1/me/repos/alice/TagAddon/suggestion", Some(&alice)).await.json();
    assert_eq!((t["source_type"].as_str(), t["folders"].clone()), (Some("tag"), json!(["TagAddon"])));

    assert_eq!(app.get("/v1/me/repos/bob/Other/suggestion", Some(&alice)).await.status, StatusCode::FORBIDDEN, "not her account");
    let carol = app.login(3, "carol").await;
    assert_eq!(app.get("/v1/me/repos", Some(&carol)).await.json()["repos"], json!([]));
}
