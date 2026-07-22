//! `/v1/analytics` — downtime Pareto, Six-Big-Losses, and trend (§10, §12 M5).
//!
//! Reads only; any authenticated user may view. SQL aggregates the raw events;
//! the ranking/cumulative maths come from `mes_core::analytics` so they match
//! the hand-computed fixture (§12 M5).

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use mes_client::analytics::{TimeRange, TrendPoint};
use mes_core::analytics::{pareto, ParetoRow};
use mes_db::repo_analytics;

use crate::api::{repo_err, require_pool, ApiErr};
use crate::extract::AuthUser;
use crate::http::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/downtime/pareto", get(downtime_pareto))
        .route("/downtime/six-big-losses", get(six_big_losses))
        .route("/downtime/trend", get(downtime_trend))
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
