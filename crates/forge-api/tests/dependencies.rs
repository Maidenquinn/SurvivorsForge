mod common;

use axum::http::StatusCode;
use common::TestApp;
use serde_json::json;
use sqlx::PgPool;

/// Addons `ui`, `lib`, `core`, `media` of different owners; returns the owner token of `ui`.
async fn setup(app: &TestApp) -> Vec<String> {
    let mut tokens = Vec::new();
    for (i, slug) in ["ui", "lib", "core", "media"].iter().enumerate() {
        tokens.push(app.publish(i as i64 + 1, &format!("dev{i}"), slug).await);
    }
    tokens
}

#[sqlx::test]
async fn put_replaces_owner_only(pool: PgPool) {
    let app = TestApp::new(pool);
    let t = setup(&app).await;
    let reply = app.put("/v1/addons/ui/dependencies", Some(&t[1]), json!([{ "slug": "lib" }])).await;
    assert_eq!(reply.status, StatusCode::FORBIDDEN);

    let reply = app.put("/v1/addons/ui/dependencies", Some(&t[0]), json!([{ "slug": "lib" }, { "slug": "media", "required": false }])).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    let reply = app.put("/v1/addons/ui/dependencies", Some(&t[0]), json!([{ "slug": "core" }])).await;
    let view = reply.json();
    let deps: Vec<&str> = view["dependencies"].as_array().unwrap().iter().map(|d| d["slug"].as_str().unwrap()).collect();
    assert_eq!(deps, ["core"], "replaced, not added");
}

#[sqlx::test]
async fn invalid_targets_rejected(pool: PgPool) {
    let app = TestApp::new(pool);
    let t = setup(&app).await;
    assert_eq!(app.put("/v1/addons/ui/dependencies", Some(&t[0]), json!([{ "slug": "ui" }])).await.error(), "self_dependency");
    assert_eq!(app.put("/v1/addons/ui/dependencies", Some(&t[0]), json!([{ "slug": "ghost" }])).await.error(), "unknown_dependency");
    app.delete("/v1/addons/media", Some(&t[3])).await;
    assert_eq!(app.put("/v1/addons/ui/dependencies", Some(&t[0]), json!([{ "slug": "media" }])).await.error(), "unknown_dependency");
    let twice = json!([{ "slug": "lib" }, { "slug": "LIB" }]);
    assert_eq!(app.put("/v1/addons/ui/dependencies", Some(&t[0]), twice).await.error(), "duplicate_dependency");
}

#[sqlx::test]
async fn cycles_rejected_with_path(pool: PgPool) {
    let app = TestApp::new(pool);
    let t = setup(&app).await;
    app.put("/v1/addons/ui/dependencies", Some(&t[0]), json!([{ "slug": "lib" }])).await;
    app.put("/v1/addons/lib/dependencies", Some(&t[1]), json!([{ "slug": "core" }])).await;
    let reply = app.put("/v1/addons/core/dependencies", Some(&t[2]), json!([{ "slug": "ui", "required": false }])).await;
    assert_eq!((reply.status, reply.error().as_str()), (StatusCode::UNPROCESSABLE_ENTITY, "dependency_cycle"));
    assert!(reply.json()["message"].as_str().unwrap().contains("core -> ui -> lib -> core"), "{}", reply.body);
}

#[sqlx::test]
async fn view_includes_order_and_unavailable(pool: PgPool) {
    let app = TestApp::new(pool);
    let t = setup(&app).await;
    app.put("/v1/addons/lib/dependencies", Some(&t[1]), json!([{ "slug": "core" }])).await;
    app.put("/v1/addons/media/dependencies", Some(&t[3]), json!([{ "slug": "lib" }])).await;
    app.put("/v1/addons/ui/dependencies", Some(&t[0]), json!([{ "slug": "media" }, { "slug": "lib", "required": false }])).await;

    let view = app.get("/v1/addons/ui", None).await.json();
    assert_eq!(view["install_order"], json!(["core", "lib", "media"]));
    let first = &view["dependencies"][0];
    assert_eq!((first["slug"].as_str(), first["required"].as_bool(), first["available"].as_bool()), (Some("media"), Some(true), Some(true)));
    assert!(first.get("trust").is_some());

    sqlx::query("UPDATE addons SET blocked_at = now() WHERE slug = 'media'").execute(&app.pool).await.unwrap();
    let view = app.get("/v1/addons/ui", None).await.json();
    assert_eq!(view["dependencies"][0]["available"], false);
}
