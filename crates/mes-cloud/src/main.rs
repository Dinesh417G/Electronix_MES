//! `mes-cloud` — multi-tenant aggregator binary (§1, §4).
//!
//! Read-mostly aggregator over many plants: sync push/pull, ERP export, alerts,
//! the `/v1/copilot` endpoint, and the `rmcp` MCP server. Edge remains the
//! source of truth (§4). This binary is a thin wrapper over [`mes_cloud::run`].

use anyhow::Context;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    mes_cloud::run().await.context("mes-cloud failed")
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
