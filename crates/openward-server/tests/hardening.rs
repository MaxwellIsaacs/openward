//! Cross-origin request rejection, login throttling, and embedded static
//! assets — all exercised through the full `openward_server::app` router.

use std::sync::Arc;
use std::time::Instant;

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use tower::ServiceExt;

use openward_db::{apply_schema, create_pool};
use openward_facility::AppState;
use openward_registry::{FacilityConfig, SqliteRegistry};
use openward_server::auth;

async fn build_app() -> Router {
    let pool = create_pool(":memory:").await.expect("pool");
    apply_schema(&pool).await.expect("migrations");
    let registry = SqliteRegistry::new(pool, FacilityConfig::default());
    registry.seed_default_housing().await.expect("seed housing");

    let state = AppState {
        registry,
        legal_overrides: None,
        database_path: ":memory:".to_string(),
        backup_dir: std::env::temp_dir()
            .join(format!("openward-test-{}", uuid::Uuid::new_v4()))
            .to_string_lossy()
            .to_string(),
        backup_retention: 5,
        started_at: Instant::now(),
    };

    auth::seed_default_admin(state.registry.pool())
        .await
        .expect("seed admin");

    openward_server::app(Arc::new(state))
}

fn login_request(username: &str, password: &str, extra: &[(&str, &str)]) -> Request<Body> {
    let mut req = Request::builder()
        .method(Method::POST)
        .uri("/login")
        .header(header::HOST, "openward.example")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
    for (k, v) in extra {
        req = req.header(*k, *v);
    }
    req.body(Body::from(format!(
        "username={username}&password={password}&language=en"
    )))
    .unwrap()
}

async fn status(app: &Router, req: Request<Body>) -> StatusCode {
    app.clone().oneshot(req).await.unwrap().status()
}

// === Cross-origin protection ===

#[tokio::test]
async fn cross_site_post_is_rejected() {
    let app = build_app().await;
    let req = login_request("admin", "changeme", &[("sec-fetch-site", "cross-site")]);
    assert_eq!(status(&app, req).await, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn same_origin_post_is_allowed() {
    let app = build_app().await;
    let req = login_request("admin", "changeme", &[("sec-fetch-site", "same-origin")]);
    assert_eq!(status(&app, req).await, StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn foreign_origin_without_fetch_metadata_is_rejected() {
    let app = build_app().await;
    let req = login_request("admin", "changeme", &[("origin", "https://evil.example")]);
    assert_eq!(status(&app, req).await, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn matching_origin_without_fetch_metadata_is_allowed() {
    let app = build_app().await;
    let req = login_request("admin", "changeme", &[("origin", "https://openward.example")]);
    assert_eq!(status(&app, req).await, StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn cross_site_get_is_allowed() {
    let app = build_app().await;
    let req = Request::builder()
        .uri("/login")
        .header("sec-fetch-site", "cross-site")
        .body(Body::empty())
        .unwrap();
    assert_eq!(status(&app, req).await, StatusCode::OK);
}

// === Login throttling ===

#[tokio::test]
async fn repeated_failures_lock_the_username() {
    let app = build_app().await;
    // Unique per test run: throttle state is process-global.
    let user = format!("victim-{}", uuid::Uuid::new_v4().simple());

    for _ in 0..5 {
        let req = login_request(&user, "wrong", &[]);
        assert_eq!(status(&app, req).await, StatusCode::OK, "bad login re-renders the form");
    }
    let req = login_request(&user, "wrong", &[]);
    assert_eq!(status(&app, req).await, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn repeated_failures_lock_the_client_ip() {
    let app = build_app().await;
    let ip = "203.0.113.77";

    for i in 0..5 {
        let req = login_request(&format!("spray-{i}"), "wrong", &[("x-forwarded-for", ip)]);
        status(&app, req).await;
    }
    // Even the correct admin password is refused from that IP now.
    let req = login_request("admin", "changeme", &[("x-forwarded-for", ip)]);
    assert_eq!(status(&app, req).await, StatusCode::TOO_MANY_REQUESTS);

    // Another IP is unaffected.
    let req = login_request("admin", "changeme", &[("x-forwarded-for", "203.0.113.78")]);
    assert_eq!(status(&app, req).await, StatusCode::SEE_OTHER);
}

// === Embedded static assets ===

#[tokio::test]
async fn static_assets_are_served_from_the_binary() {
    let app = build_app().await;
    for (path, ct) in [
        ("/static/htmx.min.js", "text/javascript"),
        ("/static/pico.min.css", "text/css"),
        ("/static/style.css", "text/css"),
    ] {
        let resp = app
            .clone()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK, "{path}");
        let got = resp.headers()[header::CONTENT_TYPE].to_str().unwrap();
        assert!(got.starts_with(ct), "{path}: {got}");
    }
    let resp = app
        .oneshot(Request::builder().uri("/static/missing.js").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
