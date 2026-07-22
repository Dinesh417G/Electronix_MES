//! `/v1/exec` — operator execution (§10, §12 M3).
//!
//! Any authenticated user (including Operators) may run work: start/complete
//! operations, record good/scrap counts, and classify or split downtime. Each
//! action publishes a live `/ws` event so kiosks and dashboards update.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use chrono::Utc;
use mes_client::exec::{ClassifyDowntimeInput, CountInput, DowntimeEventDto, SplitDowntimeInput};
use mes_client::orders::{WoOperation, WorkOrder};
use mes_client::ws::WsEvent;
use mes_core::work_order::{OpStatus, WoStatus};
use mes_db::repo_orders;

use crate::api::{audit, err, repo_err, require_pool, ApiErr};
use crate::extract::AuthUser;
use crate::http::AppState;
use crate::orders::transition_wo;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/operations/:op_id/start", post(start_op))
        .route("/operations/:op_id/count", post(record_count))
        .route("/operations/:op_id/complete", post(complete_op))
        .route("/work-orders/:id/complete", post(complete_wo))
        .route("/downtime/:id/classify", post(classify_downtime))
        .route("/downtime/:id/split", post(split_downtime))
}

fn parse_op_status(op: &WoOperation) -> Result<OpStatus, ApiErr> {
    OpStatus::parse(&op.status).ok_or_else(|| {
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            "unknown operation status",
        )
    })
}

async fn start_op(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(op_id): Path<String>,
) -> Result<Json<WoOperation>, ApiErr> {
    let pool = require_pool(&state)?;
    let op = repo_orders::get_operation(pool, &op_id)
        .await
        .map_err(repo_err)?;
    if !parse_op_status(&op)?.can_transition(OpStatus::InProgress) {
        return Err(err(
            StatusCode::CONFLICT,
            "invalid_transition",
            "operation cannot be started from its current status",
        ));
    }

    let updated = repo_orders::start_operation(pool, &op_id)
        .await
        .map_err(repo_err)?;

    // Advance the parent work order to InProgress on first operation start.
    let wo = repo_orders::get_work_order(pool, &op.work_order_id)
        .await
        .map_err(repo_err)?;
    if WoStatus::parse(&wo.status) == Some(WoStatus::Released) {
        transition_wo(
            &state,
            &auth.user_id,
            &op.work_order_id,
            WoStatus::InProgress,
        )
        .await?;
    }

    audit(
        pool,
        Some(&auth.user_id),
        "start",
        "wo_operation",
        Some(&op_id),
        None,
    )
    .await;
    state.publish(WsEvent::OperationStarted {
        work_order_id: op.work_order_id.clone(),
        wo_operation_id: op_id.clone(),
    });
    Ok(Json(updated))
}

async fn record_count(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(op_id): Path<String>,
    Json(input): Json<CountInput>,
) -> Result<Json<WoOperation>, ApiErr> {
    let pool = require_pool(&state)?;

    if input.good < 0 || input.scrap < 0 {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "counts must be non-negative",
        ));
    }
    if input.scrap > 0 && input.scrap_reason_id.is_none() {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "scrap_reason_required",
            "a scrap_reason_id is required when scrap > 0",
        ));
    }

    let op = repo_orders::get_operation(pool, &op_id)
        .await
        .map_err(repo_err)?;
    let Some(wc_id) = op.work_center_id.clone() else {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "no_work_center",
            "operation has no work center to attribute counts to",
        ));
    };

    let ts = input.ts.unwrap_or_else(Utc::now);
    let updated = repo_orders::record_count(
        pool,
        &op_id,
        &wc_id,
        input.good,
        input.scrap,
        input.scrap_reason_id.as_deref(),
        ts,
    )
    .await
    .map_err(repo_err)?;

    state.publish(WsEvent::CountRecorded {
        wo_operation_id: op_id.clone(),
        good: input.good,
        scrap: input.scrap,
    });
    Ok(Json(updated))
}

async fn complete_op(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(op_id): Path<String>,
) -> Result<Json<WoOperation>, ApiErr> {
    let pool = require_pool(&state)?;
    let op = repo_orders::get_operation(pool, &op_id)
        .await
        .map_err(repo_err)?;
    if !parse_op_status(&op)?.can_transition(OpStatus::Completed) {
        return Err(err(
            StatusCode::CONFLICT,
            "invalid_transition",
            "operation cannot be completed from its current status",
        ));
    }
    let updated = repo_orders::complete_operation(pool, &op_id)
        .await
        .map_err(repo_err)?;
    audit(
        pool,
        Some(&auth.user_id),
        "complete",
        "wo_operation",
        Some(&op_id),
        None,
    )
    .await;
    state.publish(WsEvent::OperationCompleted {
        work_order_id: op.work_order_id.clone(),
        wo_operation_id: op_id.clone(),
    });

    // Auto-schedule the next operation's DNC transfer, if any (§8.4). Best-effort.
    crate::dnc::on_job_complete(&state, &op_id).await;

    Ok(Json(updated))
}

async fn complete_wo(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<WorkOrder>, ApiErr> {
    Ok(Json(
        transition_wo(&state, &auth.user_id, &id, WoStatus::Completed).await?,
    ))
}

async fn classify_downtime(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(input): Json<ClassifyDowntimeInput>,
) -> Result<Json<DowntimeEventDto>, ApiErr> {
    let pool = require_pool(&state)?;
    let event = repo_orders::classify_downtime(pool, &id, &input.reason_id, &auth.user_id)
        .await
        .map_err(repo_err)?;
    state.publish(WsEvent::DowntimeClassified {
        downtime_event_id: id.clone(),
        reason_id: input.reason_id.clone(),
    });
    Ok(Json(event))
}

async fn split_downtime(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(input): Json<SplitDowntimeInput>,
) -> Result<Json<Vec<DowntimeEventDto>>, ApiErr> {
    let pool = require_pool(&state)?;
    let (first, second) = repo_orders::split_downtime(
        pool,
        &id,
        input.at,
        input.first_reason_id.as_deref(),
        input.second_reason_id.as_deref(),
        &auth.user_id,
    )
    .await
    .map_err(repo_err)?;

    for ev in [&first, &second] {
        if let Some(reason) = &ev.reason_id {
            state.publish(WsEvent::DowntimeClassified {
                downtime_event_id: ev.id.clone(),
                reason_id: reason.clone(),
            });
        }
    }
    Ok(Json(vec![first, second]))
}
