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
async fn permissions_and_categories(pool: PgPool) {
    let app = TestApp::new(pool);
    let alice = app.publish(1, "alice", "lucky").await;
    let bob = app.login(2, "bob").await;
    assert_eq!(app.get("/v1/addons/lucky/permissions", Some(&alice)).await.json()["owner"], true);
    let p = app.get("/v1/addons/lucky/permissions", Some(&bob)).await.json();
    assert_eq!((p["owner"].as_bool(), p["edit_images"].as_bool()), (Some(false), Some(false)));
    assert_eq!(app.get("/v1/categories", None).await.json(), json!(["UI", "Info", "Build", "Loot", "Meter"]));
}
