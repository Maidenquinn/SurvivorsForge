mod common;

use axum::Router;
use axum::body::Body;
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use common::{app, get as get_uri, send};
use forge_api::MIGRATOR;
use forge_api::app::with_layers;
use forge_api::config::Config;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use std::time::Duration;

#[sqlx::test]
async fn healthz_reports_ok_with_db(pool: PgPool) {
    let reply = get_uri(&app(pool), "/healthz").await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.json(), serde_json::json!({ "status": "ok", "db": "ok" }));
}

#[tokio::test]
async fn healthz_reports_down_without_db() {
    let pool = PgPoolOptions::new()
        .acquire_timeout(Duration::from_millis(300))
        .connect_lazy("postgres://forge:forge@127.0.0.1:1/forge")
        .unwrap();
    let reply = get_uri(&app(pool), "/healthz").await;
    assert_eq!(reply.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(reply.json()["db"], "down");
}

#[sqlx::test(migrations = false)]
async fn migrate_twice_is_noop(pool: PgPool) {
    MIGRATOR.run(&pool).await.unwrap();
    MIGRATOR.run(&pool).await.unwrap();
    let (trgm,): (bool,) = sqlx::query_as("SELECT EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'pg_trgm')")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(trgm);
}

#[sqlx::test]
async fn unknown_route_is_json_404(pool: PgPool) {
    let reply = get_uri(&app(pool), "/nope").await;
    assert_eq!(reply.status, StatusCode::NOT_FOUND);
    assert_eq!(reply.json()["error"], "not_found");
}

async fn boom() -> &'static str {
    panic!("secret detail")
}

#[tokio::test]
async fn panic_is_opaque_500() {
    let routes: Router = Router::new().route("/boom", get(boom));
    let app = with_layers(routes, &Config::for_tests("postgres://unused"));
    let reply = get_uri(&app, "/boom").await;
    assert_eq!(reply.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(reply.json()["error"], "internal");
    assert!(!reply.body.contains("secret detail"));
}

#[sqlx::test]
async fn default_security_headers(pool: PgPool) {
    let reply = get_uri(&app(pool), "/healthz").await;
    assert_eq!(reply.headers["x-content-type-options"], "nosniff");
    assert_eq!(reply.headers["cache-control"], "no-store");
}

fn limited_app(trusted: &str) -> Router {
    let mut config = Config::for_tests("postgres://unused");
    config.rate_limit_per_minute = 2;
    config.trusted_proxies = vec![trusted.parse().unwrap()];
    let routes = Router::new().route("/x", get(|| async { "ok" }));
    with_layers(routes, &config).layer(MockConnectInfo(SocketAddr::from(([203, 0, 113, 9], 4000))))
}

async fn hit(app: &Router, forwarded_for: &str) -> StatusCode {
    send(app, Request::get("/x").header("x-forwarded-for", forwarded_for).body(Body::empty()).unwrap()).await.status
}

#[tokio::test]
async fn limit_exceeded_returns_429() {
    let app = limited_app("10.0.0.2");
    assert_eq!(hit(&app, "1.1.1.1").await, StatusCode::OK);
    assert_eq!(hit(&app, "1.1.1.1").await, StatusCode::OK);
    assert_eq!(hit(&app, "1.1.1.1").await, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn forwarded_for_ignored_from_untrusted_peer() {
    // The peer 203.0.113.9 is not a trusted proxy, so changing the header does not reset the count.
    let app = limited_app("10.0.0.2");
    assert_eq!(hit(&app, "1.1.1.1").await, StatusCode::OK);
    assert_eq!(hit(&app, "2.2.2.2").await, StatusCode::OK);
    assert_eq!(hit(&app, "3.3.3.3").await, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn requests_of_the_website_are_not_counted_again() {
    // The website limits each visitor itself, and one of its pages asks the API several times.
    let mut config = Config::for_tests("postgres://unused");
    config.rate_limit_per_minute = 2;
    config.website_ip = Some("10.0.0.3".parse().unwrap());
    let routes = Router::new().route("/x", get(|| async { "ok" }));
    let app = with_layers(routes, &config).layer(MockConnectInfo(SocketAddr::from(([10, 0, 0, 3], 4000))));
    for _ in 0..5 {
        assert_eq!(hit(&app, "1.1.1.1").await, StatusCode::OK);
    }
}
