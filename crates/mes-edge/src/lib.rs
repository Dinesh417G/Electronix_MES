//! `mes-edge` library surface.
//!
//! The plant server's modules live here so both the binary (`main.rs`) and the
//! integration tests can build the router and exercise handlers in-process. The
//! binary is a thin wrapper over [`run`].

#![forbid(unsafe_code)]

pub mod analytics;
pub mod api;
pub mod auth;
pub mod auth_routes;
pub mod config;
pub mod dnc;
pub mod exec;
pub mod extract;
pub mod http;
pub mod ingest;
pub mod master;
pub mod orders;
pub mod process;
pub mod qms;
pub mod trace;
pub mod ws;

use anyhow::Context;

/// Boot the edge service: load config, connect + migrate the database when
/// configured, build the router, and serve until shutdown.
pub async fn run() -> anyhow::Result<()> {
    let cfg = config::Config::from_env().context("loading configuration")?;
    tracing::info!(bind = %cfg.bind, "starting mes-edge");

    // Connect + migrate when a database is configured. Kept optional so the
    // binary boots for local smoke tests without Postgres; docker-compose always
    // provides DATABASE_URL (§12 M0).
    let pool = match &cfg.database_url {
        Some(url) => {
            let pool = mes_db::connect(url, cfg.db_max_connections)
                .await
                .context("connecting to database")?;
            mes_db::run_migrations(&pool)
                .await
                .context("running migrations")?;
            Some(pool)
        }
        None => {
            tracing::warn!("DATABASE_URL not set — starting without a database (liveness only)");
            None
        }
    };

    let auth = auth::AuthConfig::new(cfg.jwt_secret.clone(), cfg.jwt_ttl_secs);
    let mut state = http::AppState::new(pool, auth);
    // Wire a real dnc-daemon client when an address is configured; otherwise the
    // default disconnected stub degrades gracefully (§8.4).
    if let Ok(addr) = std::env::var("MES_DNC_ADDR") {
        if !addr.is_empty() {
            state = state.with_dnc(std::sync::Arc::new(mes_dnc_bridge::TcpDncClient::new(addr)));
        }
    }
    let app = http::router(state);

    let listener = tokio::net::TcpListener::bind(&cfg.bind)
        .await
        .with_context(|| format!("binding {}", cfg.bind))?;
    tracing::info!(bind = %cfg.bind, "mes-edge listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("server error")?;

    Ok(())
}

/// Resolve when the process receives Ctrl-C so Axum can drain in-flight requests.
async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutdown signal received");
}
