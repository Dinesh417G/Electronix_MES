//! `mes-edge` — per-plant server binary (§1, §4).
//!
//! Offline-first source of truth for one plant: Axum API + WS, ingest adapters,
//! state machine, OEE, CMMS, DNC bridge, ERP adapter, and the sync client. The
//! process is a thin wrapper over [`mes_edge::run`]; the feature surface lives
//! in the library so tests can exercise it in-process.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    mes_edge::run().await
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
