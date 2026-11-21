pub mod auth;
pub mod backup;
pub mod csrf;
pub mod export;
pub mod i18n;
pub mod static_assets;
pub mod templates;
pub mod web;
pub mod api;

use std::sync::Arc;
use axum::{middleware, routing::get, Router};
use openward_facility::AppState;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The complete application: API, HTML UI, embedded static assets, and
/// cross-origin protection on every state-changing request.
pub fn app(state: Arc<AppState>) -> Router {
    Router::new()
        .merge(api::api_router())
        .merge(web::html_router())
        .route("/static/{file}", get(static_assets::serve))
        .layer(middleware::from_fn(csrf::protect))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state)
}
