//! Facility composition layer — owns the database pool, constructs domain
//! modules, and exposes shared application state for the HTTP server.

use openward_db::{apply_schema, create_pool};
use openward_registry::{FacilityConfig, SqliteRegistry};
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use std::time::Instant;

/// Shared application state, wrapped in `Arc` by the server.
pub struct AppState {
    pub registry: SqliteRegistry,
    /// Legal term overrides (preset + runtime file), keyed as "lang:fluent-key".
    pub legal_overrides: Option<Arc<HashMap<String, String>>>,
    /// Absolute path to the SQLite database file.
    pub database_path: String,
    /// Directory where backups are stored.
    pub backup_dir: String,
    /// Number of backup files to keep before pruning.
    pub backup_retention: u32,
    /// Server start time, used for uptime reporting in /health.
    pub started_at: Instant,
}

/// Server and facility configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    /// SQLite database path. Defaults to "openward.db".
    pub database_url: String,
    /// Address to bind the HTTP server to. Defaults to "0.0.0.0:3000".
    pub bind_address: String,
    /// Facility-specific thresholds for flag computation.
    pub facility: FacilityConfig,
    /// Directory where backups are stored. Defaults to "./backups/".
    pub backup_dir: String,
    /// Number of backup files to keep before pruning. Defaults to 30.
    pub backup_retention: u32,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            database_url: "openward.db".to_string(),
            bind_address: "0.0.0.0:3000".to_string(),
            facility: FacilityConfig::default(),
            backup_dir: "./backups/".to_string(),
            backup_retention: 30,
        }
    }
}

impl ServerConfig {
    /// Load configuration from environment variables, falling back to defaults.
    ///
    /// Recognized variables:
    /// - `OPENWARD_DB` — database path (default: "openward.db")
    /// - `OPENWARD_BIND` — bind address (default: "0.0.0.0:3000")
    /// - `OPENWARD_CAPACITY` — facility capacity (default: 500)
    /// - `OPENWARD_JUVENILE_AGE` — juvenile cutoff age (default: 18)
    pub fn from_env() -> Self {
        let mut config = Self::default();

        if let Ok(db) = env::var("OPENWARD_DB") {
            config.database_url = db;
        }
        if let Ok(bind) = env::var("OPENWARD_BIND") {
            config.bind_address = bind;
        }
        if let Ok(cap) = env::var("OPENWARD_CAPACITY") {
            if let Ok(n) = cap.parse() {
                config.facility.capacity = n;
            }
        }
        if let Ok(age) = env::var("OPENWARD_JUVENILE_AGE") {
            if let Ok(n) = age.parse() {
                config.facility.juvenile_cutoff_age = n;
            }
        }
        if let Ok(dir) = env::var("OPENWARD_BACKUP_DIR") {
            config.backup_dir = dir;
        }
        if let Ok(ret) = env::var("OPENWARD_BACKUP_RETENTION") {
            if let Ok(n) = ret.parse() {
                config.backup_retention = n;
            }
        }

        config
    }
}

/// Initialize the application: create the database pool, apply the schema,
/// and construct the registry.
pub async fn init(config: &ServerConfig) -> Result<AppState, Box<dyn std::error::Error>> {
    let pool = create_pool(&config.database_url).await?;
    apply_schema(&pool).await?;
    let registry = SqliteRegistry::new(pool, config.facility.clone());
    registry.seed_default_housing().await?;
    Ok(AppState {
        registry,
        legal_overrides: None,
        database_path: config.database_url.clone(),
        backup_dir: config.backup_dir.clone(),
        backup_retention: config.backup_retention,
        started_at: Instant::now(),
    })
}
