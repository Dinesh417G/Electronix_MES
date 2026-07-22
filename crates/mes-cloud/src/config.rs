//! Environment-driven configuration for `mes-cloud` (§14).

use std::env;

/// Resolved runtime configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// `host:port` the HTTP server binds to.
    pub bind: String,
    /// Postgres/TimescaleDB connection string, if a database is wired.
    pub database_url: Option<String>,
    /// Upper bound on pooled DB connections.
    pub db_max_connections: u32,
}

impl Config {
    /// Read configuration from the process environment, applying defaults.
    pub fn from_env() -> anyhow::Result<Self> {
        let bind = env::var("MES_CLOUD_BIND").unwrap_or_else(|_| "0.0.0.0:8090".to_string());
        let database_url = env::var("DATABASE_URL").ok().filter(|s| !s.is_empty());
        let db_max_connections = env::var("MES_DB_MAX_CONN")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(20);

        Ok(Self {
            bind,
            database_url,
            db_max_connections,
        })
    }
}
