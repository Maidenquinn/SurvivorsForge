mod common;

use axum::http::StatusCode;
use common::TestApp;
use serde_json::json;
use sqlx::PgPool;

/// A version with a stored scan; `flagged` adds a warn finding. Returns the version id.
async fn version(app: &TestApp, tag: &str, scanned: bool, flagged: bool) -> i64 {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO versions (addon_id, tag, published_at, asset_name, asset_url, sha256, scan_status, created_at)
         SELECT id, $1, now(), 'a.zip', 'https://github.com/a.zip', 'abc', $2, now() FROM addons WHERE slug = 'lucky' RETURNING id",
    )
    .bind(tag)
    .bind(if scanned { "done" } else { "pending" })
    .fetch_one(&app.pool)
    .await
    .unwrap();
    if scanned {
        let scan: i64 = sqlx::query_scalar(
            "INSERT INTO scans (version_id, sha256, scanner_version, started_at, finished_at, result)
             VALUES ($1, 'abc', 'x', now(), now(), $2) RETURNING id",
        )
        .bind(id)
        .bind(if flagged { "flagged" } else { "clean" })
        .fetch_one(&app.pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO findings (scan_id, rule, severity, file, message) VALUES ($1, 'zip.hidden', 'info', '', 'm')")
            .bind(scan)
            .execute(&app.pool)
            .await
            .unwrap();
        if flagged {
            sqlx::query("INSERT INTO findings (scan_id, rule, severity, file, message) VALUES ($1, 'lua.url', 'warn', 'a.lua', 'm')")
                .bind(scan)
                .execute(&app.pool)
                .await
                .unwrap();
        }
    }
    id
}

async fn trust(app: &TestApp, id: i64) -> String {
    sqlx::query_scalar("SELECT trust FROM version_trust WHERE version_id = $1").bind(id).fetch_one(&app.pool).await.unwrap()
}

async fn admin_app(pool: PgPool) -> (TestApp, String, String) {
    let app = TestApp::with_config(pool, |c| c.admin_github_ids = vec![99]);
    let owner = app.publish(1, "alice", "lucky").await;
    let admin = app.login(99, "root").await;
    (app, owner, admin)
}

#[sqlx::test]
async fn priority_table(pool: PgPool) {
    let (app, _, _) = admin_app(pool).await;
    let admin_id = app.user_id(99).await;
    let mut n = 0;
    for bits in 0..32u32 {
        let [blocked, changed, verified, flagged, scanned] = [0, 1, 2, 3, 4].map(|b| bits & (1 << b) != 0);
        n += 1;
        let id = version(&app, &format!("v{n}"), scanned, flagged).await;
        if blocked {
            sqlx::query("UPDATE versions SET blocked_at = now() WHERE id = $1").bind(id).execute(&app.pool).await.unwrap();
        }
        if changed {
            sqlx::query("UPDATE versions SET source_changed_at = now() WHERE id = $1").bind(id).execute(&app.pool).await.unwrap();
        }
        if verified {
            sqlx::query("INSERT INTO verifications (version_id, sha256, admin_id, created_at) VALUES ($1, 'abc', $2, now())")
                .bind(id)
                .bind(admin_id)
                .execute(&app.pool)
                .await
                .unwrap();
        }
        let expected = if blocked {
            "blocked"
        } else if changed {
            "source_changed"
        } else if verified {
            "verified"
        } else if scanned && flagged {
            "flagged"
        } else if scanned {
            "scanned"
        } else {
            "unscanned"
        };
        assert_eq!(trust(&app, id).await, expected, "blocked={blocked} changed={changed} verified={verified} flagged={flagged} scanned={scanned}");
    }
}

#[sqlx::test]
async fn blocked_addon_blocks_its_versions(pool: PgPool) {
    let (app, _, _) = admin_app(pool).await;
    let id = version(&app, "v1", true, false).await;
    sqlx::query("UPDATE addons SET blocked_at = now()").execute(&app.pool).await.unwrap();
    assert_eq!(trust(&app, id).await, "blocked");
}

#[sqlx::test]
async fn verification_bound_to_hash(pool: PgPool) {
    let (app, _, admin) = admin_app(pool).await;
    let id = version(&app, "v1", true, false).await;
    let reply = app.post(&format!("/v1/admin/versions/{id}/verification"), Some(&admin), json!({})).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    assert_eq!(trust(&app, id).await, "verified");
    let stored: String = sqlx::query_scalar("SELECT sha256 FROM verifications").fetch_one(&app.pool).await.unwrap();
    assert_eq!(stored, "abc");

    sqlx::query("UPDATE versions SET sha256 = 'other'").execute(&app.pool).await.unwrap();
    assert_ne!(trust(&app, id).await, "verified", "a new file is not verified");
}

#[sqlx::test]
async fn flagged_needs_note_unscanned_rejected(pool: PgPool) {
    let (app, _, admin) = admin_app(pool).await;
    let pending = version(&app, "v1", false, false).await;
    let reply = app.post(&format!("/v1/admin/versions/{pending}/verification"), Some(&admin), json!({})).await;
    assert_eq!((reply.status, reply.error().as_str()), (StatusCode::CONFLICT, "not_scanned"));

    let flagged = version(&app, "v2", true, true).await;
    let reply = app.post(&format!("/v1/admin/versions/{flagged}/verification"), Some(&admin), json!({ "note": "  " })).await;
    assert_eq!((reply.status, reply.error().as_str()), (StatusCode::UNPROCESSABLE_ENTITY, "note_required"));
    let reply = app.post(&format!("/v1/admin/versions/{flagged}/verification"), Some(&admin), json!({ "note": "URL is the addon's wiki" })).await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(app.post("/v1/admin/versions/999999/verification", Some(&admin), json!({})).await.status, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn only_admin_verifies(pool: PgPool) {
    let (app, owner, admin) = admin_app(pool).await;
    let id = version(&app, "v1", true, false).await;
    assert_eq!(app.post(&format!("/v1/admin/versions/{id}/verification"), Some(&owner), json!({})).await.status, StatusCode::FORBIDDEN);
    assert_eq!(app.post(&format!("/v1/admin/versions/{id}/verification"), None, json!({})).await.status, StatusCode::UNAUTHORIZED);
    app.post(&format!("/v1/admin/versions/{id}/verification"), Some(&admin), json!({})).await;
    assert_eq!(app.delete(&format!("/v1/admin/versions/{id}/verification"), Some(&owner)).await.status, StatusCode::FORBIDDEN);
    assert_eq!(app.delete(&format!("/v1/admin/versions/{id}/verification"), Some(&admin)).await.status, StatusCode::NO_CONTENT);
    assert_eq!(trust(&app, id).await, "scanned");
}

#[sqlx::test]
async fn admin_actions_are_audited(pool: PgPool) {
    let (app, _, admin) = admin_app(pool).await;
    let id = version(&app, "v1", true, false).await;
    app.post(&format!("/v1/admin/versions/{id}/verification"), Some(&admin), json!({ "note": "looked at it" })).await;
    app.delete(&format!("/v1/admin/versions/{id}/verification"), Some(&admin)).await;
    let rows: Vec<(String, String, String, String)> =
        sqlx::query_as("SELECT action, target_type, target_id, note FROM audit_log ORDER BY id").fetch_all(&app.pool).await.unwrap();
    assert_eq!(rows, [
        ("verify".into(), "version".into(), id.to_string(), "looked at it".into()),
        ("unverify".into(), "version".into(), id.to_string(), "".into()),
    ]);
}

#[sqlx::test]
async fn audit_log_is_append_only(pool: PgPool) {
    let (app, _, admin) = admin_app(pool).await;
    let id = version(&app, "v1", true, false).await;
    app.post(&format!("/v1/admin/versions/{id}/verification"), Some(&admin), json!({})).await;
    assert!(sqlx::query("UPDATE audit_log SET note = 'x'").execute(&app.pool).await.is_err());
    assert!(sqlx::query("DELETE FROM audit_log").execute(&app.pool).await.is_err());
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_log").fetch_one(&app.pool).await.unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test]
async fn responses_include_trust(pool: PgPool) {
    let (app, _, admin) = admin_app(pool).await;
    assert_eq!(app.get("/v1/addons/lucky", None).await.json()["trust"], json!(null), "no version yet");
    let old = version(&app, "v1", true, false).await;
    sqlx::query("UPDATE versions SET published_at = now() - interval '1 day' WHERE id = $1").bind(old).execute(&app.pool).await.unwrap();
    app.post(&format!("/v1/admin/versions/{old}/verification"), Some(&admin), json!({})).await;
    version(&app, "v2", true, true).await;

    let view = app.get("/v1/addons/lucky", None).await.json();
    assert_eq!(view["trust"], "flagged", "the newest version counts");
    assert_eq!(view["latest_tag"], "v2");
    assert_eq!(view["verified_author"], true);
    assert_eq!(view["findings"], json!({ "critical": 0, "warn": 1, "info": 1 }));

    let versions = app.get("/v1/addons/lucky/versions", None).await.json();
    let trusts: Vec<&str> = versions.as_array().unwrap().iter().map(|v| v["trust"].as_str().unwrap()).collect();
    assert_eq!(trusts, ["flagged", "verified"]);
    assert!(versions[0]["id"].is_i64());
}

#[sqlx::test]
async fn prerelease_does_not_decide_addon_trust(pool: PgPool) {
    let (app, _, _) = admin_app(pool).await;
    let stable = version(&app, "v1", true, false).await;
    sqlx::query("UPDATE versions SET published_at = now() - interval '1 day' WHERE id = $1").bind(stable).execute(&app.pool).await.unwrap();
    let beta = version(&app, "v2-beta", true, true).await;
    sqlx::query("UPDATE versions SET prerelease = true WHERE id = $1").bind(beta).execute(&app.pool).await.unwrap();
    assert_eq!(app.get("/v1/addons/lucky", None).await.json()["trust"], "scanned");
}
