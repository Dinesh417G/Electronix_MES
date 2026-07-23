//! `/v1/diagnostics` — the supervisor console's "Send Diagnostics" button (§8.5,
//! §12 M14). Builds a manual report and ships it **opt-in** to the configured
//! private GitHub repo, redacted at the single choke point
//! (`mes_diagnostics::send_diagnostics`). Master-writer gated.

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use mes_diagnostics::{send_diagnostics, GitHubShipper, SendOutcome, ShipConfig};
use serde::Deserialize;
use serde_json::json;

use crate::api::{err, ApiErr};
use crate::extract::MasterWriter;
use crate::http::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/send", post(send))
}

#[derive(Debug, Deserialize)]
struct SendInput {
    note: Option<String>,
}

async fn send(
    _writer: MasterWriter,
    State(_state): State<AppState>,
    body: Option<Json<SendInput>>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiErr> {
    let note = body
        .and_then(|Json(b)| b.note)
        .unwrap_or_else(|| "manual diagnostics from supervisor console".to_string());

    let config = ShipConfig::from_env();
    let payload = mes_diagnostics::manual::report("mes-edge", mes_core::VERSION, &note, &[]);
    let shipper = GitHubShipper::new(config.repo.clone(), config.token.clone());

    let outcome = send_diagnostics(&shipper, &config, &payload)
        .await
        .map_err(|e| err(StatusCode::BAD_GATEWAY, "diagnostics_failed", e.to_string()))?;

    let label = match outcome {
        SendOutcome::Shipped => "shipped",
        // Opt-in is off — nothing left the box (§8.5).
        SendOutcome::Skipped => "skipped",
    };
    Ok((StatusCode::OK, Json(json!({ "outcome": label }))))
}
