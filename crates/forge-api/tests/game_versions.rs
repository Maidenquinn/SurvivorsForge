mod common;

use axum::http::StatusCode;
use common::TestApp;
use forge_api::github::types::Release;
use serde_json::{Value, json};
use sqlx::PgPool;

async fn setup(pool: PgPool) -> (TestApp, String, String) {
    let app = TestApp::with_config(pool, |c| c.admin_github_ids = vec![99]);
    let owner = app.publish(1, "alice", "lucky").await;
    let admin = app.login(99, "root").await;
    for (name, key) in [("0.68.0", 10), ("0.69.0", 20), ("0.69.1", 21), ("0.70.0", 30)] {
        let reply = app.post("/v1/admin/game-versions", Some(&admin), json!({ "name": name, "sort_key": key })).await;
        assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
    }
    sqlx::query(
        "INSERT INTO versions (addon_id, tag, published_at, asset_name, asset_url, created_at)
         SELECT id, 'v1', now() - interval '1 day', 'a.zip', 'https://github.com/a.zip', now() FROM addons",
    )
    .execute(&app.pool)
    .await
    .unwrap();
    (app, owner, admin)
}

async fn game_id(app: &TestApp, name: &str) -> i32 {
    sqlx::query_scalar("SELECT id FROM game_versions WHERE name = $1").bind(name).fetch_one(&app.pool).await.unwrap()
}

#[sqlx::test]
async fn admin_only_crud_and_in_use_delete(pool: PgPool) {
    let (app, owner, admin) = setup(pool).await;
    assert_eq!(app.post("/v1/admin/game-versions", Some(&owner), json!({ "name": "1.0", "sort_key": 99 })).await.status, StatusCode::FORBIDDEN);
    let dup = app.post("/v1/admin/game-versions", Some(&admin), json!({ "name": "0.69.1", "sort_key": 1 })).await;
    assert_eq!(dup.error(), "name_taken");

    let list = app.get("/v1/game-versions", None).await.json();
    let names: Vec<&str> = list.as_array().unwrap().iter().map(|g| g["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["0.68.0", "0.69.0", "0.69.1", "0.70.0"]);

    let id = game_id(&app, "0.68.0").await;
    assert_eq!(app.patch(&format!("/v1/admin/game-versions/{id}"), Some(&admin), json!({ "name": "0.68" })).await.status, StatusCode::NO_CONTENT);
    app.patch("/v1/addons/lucky/versions/v1", Some(&owner), json!({ "game_min": "0.68" })).await;
    let reply = app.delete(&format!("/v1/admin/game-versions/{id}"), Some(&admin)).await;
    assert_eq!((reply.status, reply.error().as_str()), (StatusCode::CONFLICT, "game_version_in_use"));
    let unused = game_id(&app, "0.70.0").await;
    assert_eq!(app.delete(&format!("/v1/admin/game-versions/{unused}"), Some(&admin)).await.status, StatusCode::NO_CONTENT);
    let audited: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_log WHERE target_type = 'game_version'").fetch_one(&app.pool).await.unwrap();
    assert_eq!(audited, 4 + 1 + 1, "four creates, one update, one delete");
}

#[sqlx::test]
async fn owner_sets_range_validated(pool: PgPool) {
    let (app, owner, _) = setup(pool).await;
    let other = app.login(2, "bob").await;
    let range = json!({ "game_min": "0.69.0", "game_max": "0.69.1" });
    assert_eq!(app.patch("/v1/addons/lucky/versions/v1", Some(&other), range.clone()).await.status, StatusCode::FORBIDDEN);
    assert_eq!(app.patch("/v1/addons/lucky/versions/v1", Some(&owner), range).await.status, StatusCode::OK);
    let reply = app.patch("/v1/addons/lucky/versions/v1", Some(&owner), json!({ "game_min": "0.69.1", "game_max": "0.68.0" })).await;
    assert_eq!(reply.error(), "invalid_game_range");
    let reply = app.patch("/v1/addons/lucky/versions/v1", Some(&owner), json!({ "game_min": "9.9.9" })).await;
    assert_eq!(reply.error(), "unknown_game_version");
    assert_eq!(app.patch("/v1/addons/lucky/versions/nope", Some(&owner), json!({})).await.status, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn new_version_inherits_range(pool: PgPool) {
    let (app, owner, _) = setup(pool).await;
    app.patch("/v1/addons/lucky/versions/v1", Some(&owner), json!({ "game_min": "0.69.0" })).await;
    let release: Release = serde_json::from_value(json!({
        "id": 2, "tag_name": "v2", "published_at": "2030-01-01T00:00:00Z",
        "assets": [{ "id": 21, "name": "lucky-v2.zip", "size": 1, "browser_download_url": "https://github.com/v2.zip" }]
    }))
    .unwrap();
    app.github.data().releases.insert(100, vec![release]);
    let addon: i64 = sqlx::query_scalar("SELECT id FROM addons").fetch_one(&app.pool).await.unwrap();
    forge_api::sync::sync_addon(&app.state(), addon).await.unwrap();
    let versions = app.get("/v1/addons/lucky/versions", None).await.json();
    let v2 = versions.as_array().unwrap().iter().find(|v| v["tag"] == "v2").unwrap();
    assert_eq!(v2["game_min"], "0.69.0");
    assert_eq!(v2["game_max"], Value::Null);
}

#[sqlx::test]
async fn addon_view_reports_compat(pool: PgPool) {
    let (app, owner, _) = setup(pool).await;
    assert_eq!(app.get("/v1/addons/lucky?game=0.69.1", None).await.json()["compatible"], Value::Null, "nothing stated");
    app.patch("/v1/addons/lucky/versions/v1", Some(&owner), json!({ "game_min": "0.69.0", "game_max": "0.69.1" })).await;
    assert_eq!(app.get("/v1/addons/lucky?game=0.69.1", None).await.json()["compatible"], true);
    assert_eq!(app.get("/v1/addons/lucky?game=0.70.0", None).await.json()["compatible"], false);
    assert_eq!(app.get("/v1/addons/lucky/versions?game=0.68.0", None).await.json()[0]["compatible"], false);
    assert_eq!(app.get("/v1/addons/lucky?game=nope", None).await.error(), "unknown_game_version");
}
