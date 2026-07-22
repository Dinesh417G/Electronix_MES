//! Environment-driven configuration for `mes-edge`.
//!
//! Secrets and connection details come from the environment only (§14). M0
//! needs just a bind address and an optional database URL.

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
    /// Secret used to sign/verify bearer tokens (§14 — secrets via env only).
    pub jwt_secret: String,
    /// Token lifetime in seconds.
    pub jwt_ttl_secs: i64,
}

impl Config {
    /// Read configuration from the process environment, applying defaults.
    pub fn from_env() -> anyhow::Result<Self> {
        let bind = env::var("MES_EDGE_BIND").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
        let database_url = env::var("DATABASE_URL").ok().filter(|s| !s.is_empty());
        let db_max_connections = env::var("MES_DB_MAX_CONN")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10);

        // MES_JWT_SECRET should always be set in a real deployment. When absent
        // (local dev/smoke), fall back to an ephemeral random secret and warn —
        // tokens then simply don't survive a restart, which is acceptable there.
        let jwt_secret = match env::var("MES_JWT_SECRET") {
            Ok(s) if !s.is_empty() => s,
            _ => {
                tracing::warn!(
                    "MES_JWT_SECRET not set — using an ephemeral secret; tokens won't survive restart"
                );
                mes_core::new_id()
            }
        };
        let jwt_ttl_secs = env::var("MES_JWT_TTL_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(43_200); // 12h

        Ok(Self {
            bind,
            database_url,
            db_max_connections,
            jwt_secret,
            jwt_ttl_secs,
        })
    }
}
