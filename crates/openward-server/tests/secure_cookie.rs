//! Session cookie carries the `Secure` attribute when the request reached
//! the server over HTTPS (detected via the `X-Forwarded-Proto` header set
//! by Caddy in front), and omits it otherwise so local plain-HTTP dev
//! continues to work.

use std::sync::Arc;
use std::time::Instant;

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use tower::ServiceExt;

use openward_db::{apply_schema, create_pool};
use openward_facility::AppState;
use openward_registry::{FacilityConfig, SqliteRegistry};
use openward_server::{auth, web};

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
        session_secret: "test-secret".to_string(),
    };

    auth::seed_default_admin(state.registry.pool())
        .await
        .expect("seed admin");

    Router::new()
        .merge(web::html_router())
        .with_state(Arc::new(state))
}

async fn post_login(app: Router, xfp: Option<&str>) -> Vec<String> {
    let mut req = Request::builder()
        .method(Method::POST)
        .uri("/login")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
    if let Some(v) = xfp {
        req = req.header("x-forwarded-proto", v);
    }
    let req = req
        .body(Body::from("username=admin&password=changeme&language=en"))
        .unwrap();

    let response = app.oneshot(req).await.expect("oneshot");
    assert_eq!(
        response.status(),
        StatusCode::SEE_OTHER,
        "login should redirect on success"
    );
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|v| v.to_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn login_over_http_sets_no_secure_attribute() {
    let app = build_app().await;
    let cookies = post_login(app, None).await;

    let session = cookies
        .iter()
        .find(|c| c.starts_with("openward_session="))
        .expect("session cookie present");
    assert!(
        !session.to_lowercase().contains("secure"),
        "plain HTTP must not set Secure (browser would drop the cookie): {session}"
    );
}

#[tokio::test]
async fn login_over_https_sets_secure_attribute() {
    let app = build_app().await;
    let cookies = post_login(app, Some("https")).await;

    let session = cookies
        .iter()
        .find(|c| c.starts_with("openward_session="))
        .expect("session cookie present");
    assert!(
        session.contains("Secure"),
        "HTTPS request must set Secure on the session cookie: {session}"
    );

    let lang = cookies
        .iter()
        .find(|c| c.starts_with("openward_lang="))
        .expect("language cookie present");
    assert!(
        lang.contains("Secure"),
        "HTTPS request must set Secure on the language cookie: {lang}"
    );
}

#[tokio::test]
async fn login_over_https_chained_proxies_sets_secure() {
    // X-Forwarded-Proto may be a comma-separated list when multiple
    // proxies are chained; the originating scheme is the first entry.
    let app = build_app().await;
    let cookies = post_login(app, Some("https, http")).await;

    let session = cookies
        .iter()
        .find(|c| c.starts_with("openward_session="))
        .expect("session cookie present");
    assert!(
        session.contains("Secure"),
        "first entry of chained X-Forwarded-Proto governs: {session}"
    );
}
