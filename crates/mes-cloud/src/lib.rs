//! `mes-cloud` library surface.
//!
//! The multi-tenant aggregator's modules live here so both the binary
//! (`main.rs`) and the integration tests can build the router in-process. The
//! binary is a thin wrapper over [`run`].

#![forbid(unsafe_code)]

pub mod agent;
pub mod api;
pub mod config;
pub mod copilot;
pub mod http;
pub mod sync;

use std::sync::Arc;

use anyhow::Context;

/// Boot the cloud service: load config, connect + migrate the database when
/// configured, build the router, and serve until shutdown.
pub async fn run() -> anyhow::Result<()> {
    let cfg = config::Config::from_env().context("loading configuration")?;
    tracing::info!(bind = %cfg.bind, "starting mes-cloud");

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

    // Real Anthropic backend when a key is configured; otherwise the copilot
    // degrades gracefully (the desktop panel shows its offline banner, §11).
    let backend: Arc<dyn copilot::LlmBackend> = match copilot::AnthropicBackend::from_env() {
        Some(b) => {
            tracing::info!("copilot: Anthropic backend enabled");
            Arc::new(b)
        }
        None => {
            tracing::warn!("copilot: no ANTHROPIC_API_KEY — copilot disabled (NullBackend)");
            Arc::new(copilot::NullBackend)
        }
    };

    let state = http::AppState {
        pool,
        admin_token: cfg.admin_token.clone(),
        backend,
    };
    let app = http::router(state);

    let listener = tokio::net::TcpListener::bind(&cfg.bind)
        .await
        .with_context(|| format!("binding {}", cfg.bind))?;
    tracing::info!(bind = %cfg.bind, "mes-cloud listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("server error")?;

    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutdown signal received");
}
