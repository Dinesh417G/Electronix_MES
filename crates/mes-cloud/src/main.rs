//! `mes-cloud` — multi-tenant aggregator binary (§1, §4).
//!
//! Read-mostly aggregator over many plants: sync push/pull, ERP export, alerts,
//! the `/v1/copilot` endpoint, and the `rmcp` MCP server. Edge remains the
//! source of truth (§4). M0 stands up the process skeleton (config, tracing,
//! DB connect + migrate, health endpoints); feature surface lands from M12.

mod config;
mod http;

use anyhow::Context;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

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

    let state = http::AppState { pool };
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

fn init_tracing() {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,mes_cloud=debug"));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer())
        .init();
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutdown signal received");
}
