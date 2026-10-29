mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::{TestWeb, WEB, me};
use forge_web::app::{AppState, router};
use forge_web::config::Config;
use serde_json::json;
use tower::ServiceExt;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn reports_api_state() {
    let down = router(AppState::new(Config::for_tests("http://127.0.0.1:1")));
    let response = down.oneshot(Request::get("/healthz").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

    let mut web = TestWeb::new().await;
    Mock::given(method("GET")).and(path("/healthz")).respond_with(ResponseTemplate::new(200)).mount(&web.api).await;
    assert_eq!(web.get("/healthz").await.status, StatusCode::OK);
}

#[tokio::test]
async fn login_redirects_to_the_api() {
    let mut web = TestWeb::new().await;
    let page = web.get("/login").await;
    assert_eq!(page.status, StatusCode::SEE_OTHER);
    assert_eq!(page.location(), "https://api.test/v1/auth/github/start?return_to=https%3A%2F%2Fweb.test%2Fauth%2Fcallback");
}

#[tokio::test]
async fn callback_sets_cookie_or_shows_error() {
    let mut web = TestWeb::new().await;
    Mock::given(method("POST"))
        .and(path("/v1/auth/exchange"))
        .and(body_partial_json(json!({ "code": "bad" })))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({ "error": "invalid_code", "message": "The login code is invalid or expired" })))
        .mount(&web.api)
        .await;
    let page = web.get("/auth/callback?code=bad").await;
    assert_eq!(page.status, StatusCode::BAD_REQUEST);
    assert!(page.body.contains("The login code is invalid or expired"));
    assert!(!web.cookies.contains_key("forge_session"));

    let page = web.get("/auth/callback?error=banned").await;
    assert!(page.body.contains("banned"));

    web.login_as("tok-1", me("alice", "user")).await;
    let cookie = web.cookies.get("forge_session").expect("session cookie");
    assert!(!cookie.contains("tok-1"), "the token is encrypted in the cookie");
    let page = web.get("/").await;
    assert!(page.body.contains("Log out alice"), "{}", page.body);
}

#[tokio::test]
async fn session_cookie_flags() {
    let mut web = TestWeb::new().await;
    Mock::given(method("POST"))
        .and(path("/v1/auth/exchange"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "token": "t" })))
        .mount(&web.api)
        .await;
    let page = web.get("/auth/callback?code=x").await;
    let session = page.set_cookies().into_iter().find(|c| c.starts_with("forge_session=")).unwrap();
    for flag in ["HttpOnly", "Secure", "SameSite=Lax", "Path=/"] {
        assert!(session.contains(flag), "{flag} missing in {session}");
    }
}

#[tokio::test]
async fn unsafe_methods_need_token_and_origin() {
    let mut web = TestWeb::new().await;
    let token = web.csrf().await;
    let cookie = web.cookies.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("; ");
    let post = |origin: Option<&str>, csrf: Option<&str>| {
        let mut r = Request::post("/logout").header("cookie", cookie.clone());
        if let Some(o) = origin {
            r = r.header("origin", o);
        }
        if let Some(t) = csrf {
            r = r.header("x-csrf-token", t);
        }
        r.body(Body::empty()).unwrap()
    };
    assert_eq!(web.request(post(Some(WEB), None)).await.status, StatusCode::FORBIDDEN, "no token");
    assert_eq!(web.request(post(Some(WEB), Some("wrong"))).await.status, StatusCode::FORBIDDEN, "wrong token");
    assert_eq!(web.request(post(Some("https://evil.test"), Some(&token))).await.status, StatusCode::FORBIDDEN, "foreign origin");
    assert_eq!(web.request(post(None, Some(&token))).await.status, StatusCode::FORBIDDEN, "no origin, no referer");
    assert_eq!(web.request(post(Some(WEB), Some(&token))).await.status, StatusCode::SEE_OTHER, "header token");
    assert_eq!(web.post_form("/logout", &[]).await.status, StatusCode::SEE_OTHER, "form field token");
}

#[tokio::test]
async fn security_headers_on_html() {
    let mut web = TestWeb::new().await;
    let page = web.get("/").await;
    assert_eq!(page.headers["content-security-policy"], forge_web::app::CSP);
    assert!(forge_web::app::CSP.contains("script-src 'self'") && !forge_web::app::CSP.contains("unsafe"));
    assert_eq!(page.headers["x-frame-options"], "DENY");
    assert_eq!(page.headers["referrer-policy"], "strict-origin-when-cross-origin");
    assert_eq!(page.headers["x-content-type-options"], "nosniff");
}

#[tokio::test]
async fn escapes_and_single_safe_use() {
    let mut web = TestWeb::new().await;
    web.login_as("tok", me("<script>alert(1)</script>", "user")).await;
    let page = web.get("/").await;
    assert!(!page.body.contains("<script>alert(1)"), "escaped");
    assert!(page.body.contains("&#60;script&#62;") || page.body.contains("&lt;script&gt;"));

    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let mut safe_uses = Vec::new();
    for entry in walk(&dir) {
        let text = std::fs::read_to_string(&entry).unwrap();
        for line in text.lines().filter(|l| l.contains("|safe")) {
            safe_uses.push(line.trim().to_string());
        }
    }
    assert!(safe_uses.iter().all(|l| l.contains("description_html|safe")), "{safe_uses:?}");
}

fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() { files.extend(walk(&path)) } else { files.push(path) }
    }
    files
}

#[tokio::test]
async fn logout_clears_cookie() {
    let mut web = TestWeb::new().await;
    web.login_as("tok", me("alice", "user")).await;
    Mock::given(method("POST")).and(path("/v1/auth/logout")).respond_with(ResponseTemplate::new(204)).expect(1).mount(&web.api).await;
    let page = web.post_form("/logout", &[]).await;
    assert_eq!(page.status, StatusCode::SEE_OTHER);
    assert!(!web.cookies.contains_key("forge_session"));
}

#[tokio::test]
async fn api_401_forces_relogin() {
    let mut web = TestWeb::new().await;
    web.cookies.clear();
    Mock::given(method("POST"))
        .and(path("/v1/auth/exchange"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "token": "stale" })))
        .mount(&web.api)
        .await;
    Mock::given(method("GET")).and(path("/v1/me")).respond_with(ResponseTemplate::new(401)).mount(&web.api).await;
    web.get("/auth/callback?code=x").await;
    assert!(web.cookies.contains_key("forge_session"));
    let page = web.get("/").await;
    assert_eq!((page.status, page.location()), (StatusCode::SEE_OTHER, "/login"));
    assert!(!web.cookies.contains_key("forge_session"), "the stale session is gone");
}

#[tokio::test]
async fn static_files_and_not_found() {
    let mut web = TestWeb::new().await;
    let js = web.get("/static/htmx.min.js").await;
    assert_eq!((js.status, js.headers["content-type"].to_str().unwrap()), (StatusCode::OK, "text/javascript"));
    assert_eq!(web.get("/nope").await.status, StatusCode::NOT_FOUND);
}
