use openward_server::{api, auth, i18n, web};

use std::sync::Arc;
use axum::Router;
use tower_http::services::ServeDir;
use openward_facility::ServerConfig;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "openward_server=info,tower_http=info".parse().unwrap()),
        )
        .init();

    let config = ServerConfig::from_env();
    tracing::info!(db = %config.database_url, bind = %config.bind_address, "starting OpenWard");

    let mut state = openward_facility::init(&config)
        .await
        .expect("failed to initialize application");

    // Load legal term overrides (preset + runtime file)
    let preset = std::env::var("OPENWARD_LEGAL_PRESET").ok();
    let runtime_path = std::env::var("OPENWARD_LEGAL_TERMS").ok();
    state.legal_overrides = i18n::load_legal_overrides(
        preset.as_deref(),
        runtime_path.as_deref(),
    );
    if state.legal_overrides.is_some() {
        tracing::info!("legal term overrides loaded");
    }

    auth::seed_default_admin(state.registry.pool())
        .await
        .expect("failed to seed default admin");

    let static_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/static");

    let app = Router::new()
        .merge(api::api_router())
        .merge(web::html_router())
        .nest_service("/static", ServeDir::new(static_dir))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(Arc::new(state));

    let listener = tokio::net::TcpListener::bind(&config.bind_address)
        .await
        .expect("failed to bind");

    tracing::info!("listening on {}", config.bind_address);

    axum::serve(listener, app)
        .await
        .expect("server error");
}
