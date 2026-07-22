//! `/v1/ingest` — signal intake + state recompute (§9, §10, §12 M2).
//!
//! Devices POST batches of signals; unregistered sources are dropped, not
//! errored (§9). A separate recompute endpoint derives machine states from the
//! ingested cycle stream. Both require an authenticated caller; per-device
//! ingest tokens (§14) refine this later.

use std::collections::HashMap;

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use mes_client::ingest::{IngestResult, RawSignal, RecomputeRequest, RecomputeResult, SignalEvent};
use mes_db::repo_ingest::{self, SignalSourceRow};

use crate::api::{repo_err, require_pool, ApiErr};
use crate::extract::AuthUser;
use crate::http::AppState;
use crate::process;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/signals", post(ingest_signals))
        .route("/recompute", post(recompute))
}

/// Accept a batch of raw signals. Known sources are persisted; unknown or
/// disabled sources are counted as dropped and logged (§9).
async fn ingest_signals(
    State(state): State<AppState>,
    _auth: AuthUser,
    Json(signals): Json<Vec<RawSignal>>,
) -> Result<Json<IngestResult>, ApiErr> {
    let pool = require_pool(&state)?;

    let mut accepted = 0usize;
    let mut dropped = 0usize;
    // Cache source lookups within the batch to avoid a query per signal.
    let mut cache: HashMap<String, Option<SignalSourceRow>> = HashMap::new();

    for sig in signals {
        let source = match cache.get(&sig.source_key) {
            Some(s) => s.clone(),
            None => {
                let s = repo_ingest::resolve_source(pool, &sig.source_key)
                    .await
                    .map_err(repo_err)?;
                cache.insert(sig.source_key.clone(), s.clone());
                s
            }
        };

        let Some(source) = source.filter(|s| s.enabled) else {
            tracing::warn!(source_key = %sig.source_key, "dropping signal from unknown/disabled source");
            dropped += 1;
            continue;
        };

        match &sig.event {
            SignalEvent::Cycle | SignalEvent::Heartbeat => {
                repo_ingest::insert_machine_event(
                    pool,
                    sig.ts,
                    &source.work_center_id,
                    &source.id,
                    sig.event.event_type(),
                    None,
                )
                .await
                .map_err(repo_err)?;
            }
            SignalEvent::Count { good, scrap } => {
                repo_ingest::insert_production_count(
                    pool,
                    sig.ts,
                    &source.work_center_id,
                    &source.id,
                    *good,
                    *scrap,
                )
                .await
                .map_err(repo_err)?;
            }
        }
        accepted += 1;
    }

    Ok(Json(IngestResult { accepted, dropped }))
}

/// Derive machine states + downtime events for a work center over a window.
async fn recompute(
    State(state): State<AppState>,
    _auth: AuthUser,
    Json(req): Json<RecomputeRequest>,
) -> Result<Json<RecomputeResult>, ApiErr> {
    let pool = require_pool(&state)?;
    let (states, downtime) =
        process::recompute_states(pool, &req.work_center_id, req.start, req.end)
            .await
            .map_err(repo_err)?;
    Ok(Json(RecomputeResult { states, downtime }))
}
