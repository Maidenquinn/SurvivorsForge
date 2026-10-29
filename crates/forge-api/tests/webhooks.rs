mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::{Reply, TestApp, send};
use hmac::{Hmac, KeyInit, Mac};
use serde_json::{Value, json};
use sha2::Sha256;
use sqlx::PgPool;

async fn deliver(app: &TestApp, event: &str, delivery: &str, payload: &Value, secret: &str) -> Reply {
    let body = payload.to_string();
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(body.as_bytes());
    let signature = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));
    let request = Request::post("/v1/github/webhook")
        .header("x-github-event", event)
        .header("x-github-delivery", delivery)
        .header("x-hub-signature-256", signature)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    send(&app.router, request).await
}

async fn hook(app: &TestApp, event: &str, delivery: &str, payload: Value) -> Reply {
    deliver(app, event, delivery, &payload, "webhook-secret").await
}

async fn jobs(app: &TestApp, kind: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM jobs WHERE kind = $1").bind(kind).fetch_one(&app.pool).await.unwrap()
}

fn release_payload(action: &str) -> Value {
    json!({ "action": action, "repository": { "id": 100, "full_name": "alice/lucky" }, "release": { "id": 1 } })
}

#[sqlx::test]
async fn rejects_missing_or_bad_signature(pool: PgPool) {
    let app = TestApp::new(pool);
    app.publish(1, "alice", "lucky").await;
    let reply = deliver(&app, "release", "d1", &release_payload("published"), "wrong-secret").await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
    let request = Request::post("/v1/github/webhook").header("x-github-event", "release").body(Body::from("{}")).unwrap();
    assert_eq!(send(&app.router, request).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(jobs(&app, "sync_addon").await, 0);
}

#[sqlx::test]
async fn release_published_creates_pending_version_and_job(pool: PgPool) {
    let app = TestApp::new(pool);
    app.publish(1, "alice", "lucky").await;
    app.github.data().releases.insert(100, vec![common_release()]);
    assert_eq!(hook(&app, "release", "d1", release_payload("published")).await.status, StatusCode::OK);
    assert_eq!(jobs(&app, "sync_addon").await, 1);

    forge_api::worker::platform(app.state()).run_once(&app.pool).await.unwrap();
    let (tag, status): (String, String) = sqlx::query_as("SELECT tag, scan_status FROM versions").fetch_one(&app.pool).await.unwrap();
    assert_eq!((tag.as_str(), status.as_str()), ("v1", "pending"));
    assert_eq!(jobs(&app, "scan_version").await, 1);
}

fn common_release() -> forge_api::github::types::Release {
    serde_json::from_value(json!({
        "id": 1, "tag_name": "v1", "published_at": "2026-01-01T00:00:00Z",
        "assets": [{ "id": 11, "name": "lucky-v1.zip", "size": 10, "browser_download_url": "https://github.com/x/lucky-v1.zip" }]
    }))
    .unwrap()
}

#[sqlx::test]
async fn repository_events_update_or_lock_addon(pool: PgPool) {
    let app = TestApp::new(pool);
    app.publish(1, "alice", "lucky").await;
    sqlx::query(
        "INSERT INTO versions (addon_id, tag, published_at, asset_name, asset_url, created_at)
         SELECT id, 'v1', now(), 'a.zip', 'https://x', now() FROM addons",
    )
    .execute(&app.pool)
    .await
    .unwrap();

    let renamed = json!({ "action": "renamed", "repository": { "id": 100, "full_name": "alice/lucky-odds" } });
    hook(&app, "repository", "d1", renamed).await;
    assert_eq!(app.get("/v1/addons/lucky", None).await.json()["repo"], "alice/lucky-odds");

    let private = json!({ "action": "privatized", "repository": { "id": 100, "full_name": "alice/lucky-odds" } });
    hook(&app, "repository", "d2", private).await;
    let (locked, source): (bool, String) =
        sqlx::query_as("SELECT a.locked_at IS NOT NULL, v.source_status FROM addons a JOIN versions v ON v.addon_id = a.id").fetch_one(&app.pool).await.unwrap();
    assert_eq!((locked, source.as_str()), (true, "missing"));

    let public = json!({ "action": "publicized", "repository": { "id": 100, "full_name": "alice/lucky-odds" } });
    hook(&app, "repository", "d3", public).await;
    let (locked, source): (bool, String) =
        sqlx::query_as("SELECT a.locked_at IS NOT NULL, v.source_status FROM addons a JOIN versions v ON v.addon_id = a.id").fetch_one(&app.pool).await.unwrap();
    assert_eq!((locked, source.as_str()), (false, "available"));
    assert_eq!(jobs(&app, "sync_addon").await, 1, "unlocking syncs again");

    let removed = json!({ "action": "removed", "installation": { "id": 1 }, "repositories_removed": [{ "id": 100 }] });
    hook(&app, "installation_repositories", "d4", removed).await;
    let locked: bool = sqlx::query_scalar("SELECT locked_at IS NOT NULL FROM addons").fetch_one(&app.pool).await.unwrap();
    assert!(locked, "App removed from the repo");

    hook(&app, "installation_repositories", "d5", json!({ "action": "added", "installation": { "id": 2 }, "repositories_added": [{ "id": 100 }] })).await;
    hook(&app, "installation", "d6", json!({ "action": "deleted", "installation": { "id": 2 } })).await;
    let (locked, installation): (bool, i64) =
        sqlx::query_as("SELECT locked_at IS NOT NULL, installation_id FROM addons").fetch_one(&app.pool).await.unwrap();
    assert_eq!((locked, installation), (true, 2), "App uninstalled");
}

#[sqlx::test]
async fn duplicate_delivery_processed_once(pool: PgPool) {
    let app = TestApp::new(pool);
    app.publish(1, "alice", "lucky").await;
    assert_eq!(hook(&app, "release", "same", release_payload("published")).await.json()["status"], "ok");
    sqlx::query("UPDATE jobs SET done_at = now()").execute(&app.pool).await.unwrap();
    assert_eq!(hook(&app, "release", "same", release_payload("published")).await.json()["status"], "duplicate");
    assert_eq!(jobs(&app, "sync_addon").await, 1);
}

#[sqlx::test]
async fn handler_only_enqueues(pool: PgPool) {
    let app = TestApp::new(pool);
    app.publish(1, "alice", "lucky").await;
    let before = app.github.calls();
    hook(&app, "release", "d1", release_payload("published")).await;
    hook(&app, "repository", "d2", json!({ "action": "privatized", "repository": { "id": 100 } })).await;
    assert_eq!(app.github.calls(), before, "no GitHub call inside the webhook handler");
}
