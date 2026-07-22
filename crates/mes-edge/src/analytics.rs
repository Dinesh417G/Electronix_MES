//! `/v1/analytics` — downtime Pareto, Six-Big-Losses, and trend (§10, §12 M5).
//!
//! Reads only; any authenticated user may view. SQL aggregates the raw events;
//! the ranking/cumulative maths come from `mes_core::analytics` so they match
//! the hand-computed fixture (§12 M5).

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use mes_client::analytics::{ShiftOee, TimeRange, TrendPoint};
use mes_client::ws::WsEvent;
use mes_core::analytics::{pareto, ParetoRow};
use mes_core::oee::OeeResult;
use mes_db::{repo_analytics, repo_oee};
use serde::Deserialize;

use crate::api::{repo_err, require_pool, ApiErr};
use crate::extract::AuthUser;
use crate::http::AppState;

/// Compute the work center's day-to-date OEE and broadcast it (§8.2 live OEE).
/// Best-effort — a failure is logged, never propagated to the caller's action.
pub async fn publish_oee_snapshot(state: &AppState, work_center_id: &str) {
    let Ok(pool) = require_pool(state) else {
        return;
    };
    let now = chrono::Utc::now();
    let start = now
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .map(|d| d.and_utc())
        .unwrap_or(now);
    match repo_oee::oee_inputs(pool, work_center_id, start, now).await {
        Ok(inputs) => {
            let r = mes_core::oee::compute(inputs);
            state.publish(WsEvent::OeeSnapshot {
                work_center_id: work_center_id.to_string(),
                availability: r.availability,
                performance: r.performance,
                quality: r.quality,
                oee: r.oee,
            });
        }
        Err(e) => tracing::warn!(error = %e, "oee snapshot failed"),
    }
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/downtime/pareto", get(downtime_pareto))
        .route("/downtime/six-big-losses", get(six_big_losses))
        .route("/downtime/trend", get(downtime_trend))
        .route("/oee", get(oee))
        .route("/oee/by-shift", get(oee_by_shift))
}

/// Query for OEE endpoints: a work center + time window. Flat (not flattened)
/// because `serde_urlencoded` — what `axum::Query` uses — doesn't support
/// `#[serde(flatten)]`.
#[derive(Debug, Deserialize)]
struct OeeQuery {
    work_center_id: String,
    start: chrono::DateTime<chrono::Utc>,
    end: chrono::DateTime<chrono::Utc>,
}

/// OEE for a work center over a window (Rust path: `mes_core::oee::compute`).
async fn oee(
    State(state): State<AppState>,
    _auth: AuthUser,
    Query(q): Query<OeeQuery>,
) -> Result<Json<OeeResult>, ApiErr> {
    let pool = require_pool(&state)?;
    let inputs = repo_oee::oee_inputs(pool, &q.work_center_id, q.start, q.end)
        .await
        .map_err(repo_err)?;
    Ok(Json(mes_core::oee::compute(inputs)))
}

/// OEE broken down by the work center's site shifts over a window.
async fn oee_by_shift(
    State(state): State<AppState>,
    _auth: AuthUser,
    Query(q): Query<OeeQuery>,
) -> Result<Json<Vec<ShiftOee>>, ApiErr> {
    let pool = require_pool(&state)?;
    let rows = repo_oee::oee_by_shift(pool, &q.work_center_id, q.start, q.end)
        .await
        .map_err(repo_err)?;
    Ok(Json(
        rows.into_iter()
            .map(|r| ShiftOee {
                shift_name: r.shift_name,
                start: r.start,
                end: r.end,
                availability: r.result.availability,
                performance: r.result.performance,
                quality: r.result.quality,
                oee: r.result.oee,
            })
            .collect(),
    ))
}

/// Downtime Pareto: reasons ranked by total duration, with cumulative share.
async fn downtime_pareto(
    State(state): State<AppState>,
    _auth: AuthUser,
    Query(range): Query<TimeRange>,
) -> Result<Json<Vec<ParetoRow>>, ApiErr> {
    let pool = require_pool(&state)?;
    let inputs = repo_analytics::downtime_pareto(pool, range.start, range.end)
        .await
        .map_err(repo_err)?;
    Ok(Json(pareto(inputs)))
}

/// Downtime rolled up by Six-Big-Losses bucket, ranked Pareto-style.
async fn six_big_losses(
    State(state): State<AppState>,
    _auth: AuthUser,
    Query(range): Query<TimeRange>,
) -> Result<Json<Vec<ParetoRow>>, ApiErr> {
    let pool = require_pool(&state)?;
    let inputs = repo_analytics::downtime_by_loss(pool, range.start, range.end)
        .await
        .map_err(repo_err)?;
    Ok(Json(pareto(inputs)))
}

/// Daily downtime totals over the window.
async fn downtime_trend(
    State(state): State<AppState>,
    _auth: AuthUser,
    Query(range): Query<TimeRange>,
) -> Result<Json<Vec<TrendPoint>>, ApiErr> {
    let pool = require_pool(&state)?;
    let rows = repo_analytics::downtime_trend(pool, range.start, range.end)
        .await
        .map_err(repo_err)?;
    Ok(Json(
        rows.into_iter()
            .map(|(day, seconds)| TrendPoint { day, seconds })
            .collect(),
    ))
}
