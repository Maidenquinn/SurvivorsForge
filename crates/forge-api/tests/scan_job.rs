mod common;

use common::TestApp;
use forge_api::scan_job::{MAX_DOWNLOAD, rescan_outdated, scan_version};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::io::{Cursor, Write};

const URL: &str = "https://github.com/alice/lucky/releases/download/v1/lucky-v1.zip";

fn zip(entries: &[(&str, &str)]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, content) in entries {
        writer.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        writer.write_all(content.as_bytes()).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

/// Publishes `lucky` with one version `v1` behind `url`; returns the version id.
async fn version(app: &TestApp, url: &str, digest: Option<&str>) -> i64 {
    app.publish(1, "alice", "lucky").await;
    sqlx::query_scalar(
        "INSERT INTO versions (addon_id, tag, published_at, asset_name, asset_url, github_digest, created_at)
         SELECT id, 'v1', now(), 'lucky-v1.zip', $1, $2, now() FROM addons RETURNING id",
    )
    .bind(url)
    .bind(digest)
    .fetch_one(&app.pool)
    .await
    .unwrap()
}

async fn status(app: &TestApp, id: i64) -> (String, Option<String>) {
    sqlx::query_as("SELECT scan_status, sha256 FROM versions WHERE id = $1").bind(id).fetch_one(&app.pool).await.unwrap()
}

async fn rules(app: &TestApp) -> Vec<String> {
    sqlx::query_scalar("SELECT rule FROM findings ORDER BY id").fetch_all(&app.pool).await.unwrap()
}

#[sqlx::test]
async fn scan_stores_hash_and_findings(pool: PgPool) {
    let app = TestApp::new(pool);
    let id = version(&app, URL, None).await;
    let bytes = zip(&[("Lucky/Lucky.toc", "## Interface: 11200\nLucky.lua\n"), ("Lucky/Lucky.lua", "os.execute('calc')")]);
    let sha = hex::encode(Sha256::digest(&bytes));
    app.github.data().downloads.insert(URL.into(), bytes);

    scan_version(&app.state(), id).await.unwrap();
    assert_eq!(status(&app, id).await, ("done".into(), Some(sha.clone())));
    assert!(rules(&app).await.contains(&"lua.forbidden_global".to_string()));
    let (result, scanned_sha, scanner): (String, String, String) =
        sqlx::query_as("SELECT result, sha256, scanner_version FROM scans").fetch_one(&app.pool).await.unwrap();
    assert_eq!((result.as_str(), scanned_sha), ("flagged", sha));
    assert_eq!(scanner, forge_scan::SCANNER_VERSION);

    let report = app.get("/v1/addons/lucky/versions/v1/findings", None).await.json();
    assert_eq!(report["scan"]["result"], "flagged");
    assert_eq!(report["scan"]["findings"][0]["severity"], "critical");
}

#[sqlx::test]
async fn clean_zip_is_clean(pool: PgPool) {
    let app = TestApp::new(pool);
    let id = version(&app, URL, None).await;
    app.github.data().downloads.insert(URL.into(), zip(&[("Lucky/Lucky.toc", "## Interface: 11200\nLucky.lua\n"), ("Lucky/Lucky.lua", "local x = 1")]));
    scan_version(&app.state(), id).await.unwrap();
    let result: String = sqlx::query_scalar("SELECT result FROM scans").fetch_one(&app.pool).await.unwrap();
    assert_eq!(result, "clean");
}

#[sqlx::test]
async fn digest_mismatch_is_critical(pool: PgPool) {
    let app = TestApp::new(pool);
    let id = version(&app, URL, Some(&"0".repeat(64))).await;
    app.github.data().downloads.insert(URL.into(), zip(&[("Lucky/Lucky.toc", "## Interface: 11200\n")]));
    scan_version(&app.state(), id).await.unwrap();
    assert_eq!(rules(&app).await, ["source.digest_mismatch"]);
    let (result, severity): (String, String) =
        sqlx::query_as("SELECT s.result, f.severity FROM scans s JOIN findings f ON f.scan_id = s.id").fetch_one(&app.pool).await.unwrap();
    assert_eq!((result.as_str(), severity.as_str()), ("error", "critical"));
}

#[sqlx::test]
async fn oversized_download_fails(pool: PgPool) {
    let app = TestApp::new(pool);
    let id = version(&app, URL, None).await;
    app.github.data().downloads.insert(URL.into(), vec![0u8; MAX_DOWNLOAD as usize + 1]);
    scan_version(&app.state(), id).await.unwrap();
    assert_eq!(status(&app, id).await.0, "failed");
    assert_eq!(rules(&app).await, ["source.too_large"]);
}

#[sqlx::test]
async fn network_error_retries_then_fails(pool: PgPool) {
    let app = TestApp::new(pool);
    let id = version(&app, URL, None).await;
    // No download registered: the fake answers NotFound, like an unreachable file.
    sqlx::query("DELETE FROM jobs").execute(&app.pool).await.unwrap();
    forge_api::jobs::enqueue(&app.pool, "scan_version", serde_json::json!({ "version_id": id }), chrono::Utc::now(), None).await.unwrap();
    sqlx::query("UPDATE jobs SET max_attempts = 2").execute(&app.pool).await.unwrap();
    let worker = forge_api::worker::platform(app.state());

    assert!(worker.run_once(&app.pool).await.unwrap());
    assert_eq!(status(&app, id).await.0, "pending", "first failure: retried later");
    sqlx::query("UPDATE jobs SET run_at = now() - interval '1 second'").execute(&app.pool).await.unwrap();
    assert!(worker.run_once(&app.pool).await.unwrap());
    assert_eq!(status(&app, id).await.0, "failed", "last attempt");
}

#[sqlx::test]
async fn foreign_host_is_rejected(pool: PgPool) {
    let app = TestApp::new(pool);
    let url = "https://evil.test/lucky.zip";
    let id = version(&app, url, None).await;
    app.github.data().downloads.insert(url.into(), zip(&[("Lucky/Lucky.toc", "## Interface: 11200\n")]));
    let before = app.github.calls();
    scan_version(&app.state(), id).await.unwrap();
    assert_eq!(app.github.calls(), before, "never downloaded");
    assert_eq!(rules(&app).await, ["source.forbidden_host"]);
    assert_eq!(status(&app, id).await.0, "failed");
}

#[sqlx::test]
async fn rescan_all_enqueues_outdated(pool: PgPool) {
    let app = TestApp::new(pool);
    let id = version(&app, URL, None).await;
    app.github.data().downloads.insert(URL.into(), zip(&[("Lucky/Lucky.toc", "## Interface: 11200\n")]));
    scan_version(&app.state(), id).await.unwrap();
    sqlx::query("DELETE FROM jobs").execute(&app.pool).await.unwrap();
    assert_eq!(rescan_outdated(&app.state()).await.unwrap(), 0, "scanned with this version");

    sqlx::query("UPDATE scans SET scanner_version = '0.0.0'").execute(&app.pool).await.unwrap();
    assert_eq!(rescan_outdated(&app.state()).await.unwrap(), 1);
    let jobs: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs WHERE kind = 'scan_version'").fetch_one(&app.pool).await.unwrap();
    assert_eq!(jobs, 1);
}
