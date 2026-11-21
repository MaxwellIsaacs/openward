use openward_server::{auth, i18n};

use std::sync::Arc;
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

    let app = openward_server::app(Arc::new(state));

    let listener = tokio::net::TcpListener::bind(&config.bind_address)
        .await
        .expect("failed to bind");

    tracing::info!("listening on {}", config.bind_address);

    axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>())
        .await
        .expect("server error");
}
