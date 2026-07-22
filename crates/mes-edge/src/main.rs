//! `mes-edge` — per-plant server binary (§1, §4).
//!
//! Offline-first source of truth for one plant: Axum API + WS, ingest adapters,
//! state machine, OEE, CMMS, DNC bridge, ERP adapter, and the sync client.
//! M0 stands up the process skeleton: config, tracing, DB connect + migrate,
//! and the health endpoints. Feature surface is layered on from M1.

mod config;
mod http;

use anyhow::Context;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let cfg = config::Config::from_env().context("loading configuration")?;
    tracing::info!(bind = %cfg.bind, "starting mes-edge");

    // Connect + migrate when a database is configured. M0 keeps this optional so
    // the binary boots for local smoke tests without Postgres; docker-compose
    // always provides DATABASE_URL, satisfying the M0 acceptance (§12 M0).
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
    tracing::info!(bind = %cfg.bind, "mes-edge listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("server error")?;

    Ok(())
}

/// Initialise structured tracing. `RUST_LOG` controls verbosity; default `info`.
fn init_tracing() {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};

    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,mes_edge=debug"));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer())
        .init();
}

/// Resolve when the process receives Ctrl-C so Axum can drain in-flight requests.
async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutdown signal received");
}
