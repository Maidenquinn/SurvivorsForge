mod common;

use axum::http::StatusCode;
use common::TestApp;
use forge_api::github::types::Release;
use forge_api::source_check::check_sources;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::PgPool;

const URL: &str = "https://github.com/alice/lucky/releases/download/v1/lucky-v1.zip";

fn release(asset_id: i64, size: i64) -> Release {
    serde_json::from_value(json!({
        "id": 7, "tag_name": "v1", "published_at": "2026-01-01T00:00:00Z",
        "assets": [{ "id": asset_id, "name": "lucky-v1.zip", "size": size, "browser_download_url": URL }]
    }))
    .unwrap()
}

/// A scanned release version of `lucky` whose file is `bytes`.
async fn scanned(app: &TestApp, bytes: &[u8]) -> i64 {
    app.publish(1, "alice", "lucky").await;
    let sha = hex::encode(Sha256::digest(bytes));
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO versions (addon_id, github_release_id, tag, published_at, asset_id, asset_name, asset_url, asset_size, sha256,
                               scan_status, created_at)
         SELECT id, 7, 'v1', now(), 70, 'lucky-v1.zip', $1, $2, $3, 'done', now() FROM addons RETURNING id",
    )
    .bind(URL)
    .bind(bytes.len() as i64)
    .bind(sha)
    .fetch_one(&app.pool)
    .await
    .unwrap();
    app.github.data().releases.insert(100, vec![release(70, bytes.len() as i64)]);
    app.github.data().downloads.insert(URL.into(), bytes.to_vec());
    id
}

async fn state_of(app: &TestApp, id: i64) -> (String, bool) {
    sqlx::query_as("SELECT source_status, source_changed_at IS NOT NULL FROM versions WHERE id = $1").bind(id).fetch_one(&app.pool).await.unwrap()
}

async fn trust(app: &TestApp, id: i64) -> String {
    sqlx::query_scalar("SELECT trust FROM version_trust WHERE version_id = $1").bind(id).fetch_one(&app.pool).await.unwrap()
}

#[sqlx::test]
async fn missing_and_back(pool: PgPool) {
    let app = TestApp::new(pool);
    let id = scanned(&app, b"zip bytes").await;
    app.github.data().releases.insert(100, vec![]);
    check_sources(&app.state(), 200).await.unwrap();
    assert_eq!(state_of(&app, id).await, ("missing".into(), false));

    app.github.data().releases.insert(100, vec![release(70, 9)]);
    check_sources(&app.state(), 200).await.unwrap();
    assert_eq!(state_of(&app, id).await, ("available".into(), false));
}

#[sqlx::test]
async fn changed_hash_flags_and_rescans(pool: PgPool) {
    let app = TestApp::new(pool);
    let id = scanned(&app, b"zip bytes").await;
    app.github.data().releases.insert(100, vec![release(70, 10)]);
    app.github.data().downloads.insert(URL.into(), b"evil bytes".to_vec());
    check_sources(&app.state(), 200).await.unwrap();
    assert_eq!(state_of(&app, id).await, ("available".into(), true));
    assert_eq!(trust(&app, id).await, "source_changed");
    let scans: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs WHERE kind = 'scan_version'").fetch_one(&app.pool).await.unwrap();
    assert_eq!(scans, 1);
}

#[sqlx::test]
async fn admin_accepts_new_source(pool: PgPool) {
    let app = TestApp::with_config(pool, |c| c.admin_github_ids = vec![99]);
    let id = scanned(&app, b"zip bytes").await;
    let admin = app.login(99, "root").await;
    let owner = app.login(1, "alice").await;
    sqlx::query("UPDATE versions SET source_changed_at = now()").execute(&app.pool).await.unwrap();

    assert_eq!(app.post(&format!("/v1/admin/versions/{id}/accept-source"), Some(&owner), json!({})).await.status, StatusCode::FORBIDDEN);
    assert_eq!(trust(&app, id).await, "source_changed", "stays until an admin decides");
    let reply = app.post(&format!("/v1/admin/versions/{id}/accept-source"), Some(&admin), json!({ "note": "author rebuilt it" })).await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);
    assert_eq!(trust(&app, id).await, "unscanned", "judged by the new scan from now on");
    let action: String = sqlx::query_scalar("SELECT action FROM audit_log").fetch_one(&app.pool).await.unwrap();
    assert_eq!(action, "accept_source");
    assert_eq!(app.post(&format!("/v1/admin/versions/{id}/accept-source"), Some(&admin), json!({})).await.status, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn matching_metadata_skips_download(pool: PgPool) {
    let app = TestApp::new(pool);
    let id = scanned(&app, b"zip bytes").await;
    app.github.data().downloads.clear();
    check_sources(&app.state(), 200).await.unwrap();
    assert_eq!(state_of(&app, id).await, ("available".into(), false), "no download needed, so none failed");
}

#[sqlx::test]
async fn batch_oldest_first(pool: PgPool) {
    let app = TestApp::new(pool);
    let first = scanned(&app, b"zip bytes").await;
    let second: i64 = sqlx::query_scalar(
        "INSERT INTO versions (addon_id, github_release_id, tag, published_at, asset_id, asset_name, asset_url, asset_size, sha256,
                               scan_status, created_at)
         SELECT addon_id, 8, 'v2', now(), 80, 'b.zip', asset_url, 9, sha256, 'done', now() FROM versions RETURNING id",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    sqlx::query("UPDATE versions SET source_checked_at = now() - interval '1 day' WHERE id = $1").bind(first).execute(&app.pool).await.unwrap();

    check_sources(&app.state(), 1).await.unwrap();
    let checked: Vec<(i64, bool)> =
        sqlx::query_as("SELECT id, source_checked_at > now() - interval '1 hour' FROM versions ORDER BY id").fetch_all(&app.pool).await.unwrap();
    assert_eq!(checked, [(first, false), (second, true)], "never-checked first, one per run");
}
