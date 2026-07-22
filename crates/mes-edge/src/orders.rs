//! `/v1/orders` — work-order management (§10, §12 M3).
//!
//! Order creation and lifecycle transitions (release/cancel/close) require the
//! master-write role (Planner/Admin). Reads are open to any authenticated user.
//! Operator execution lives in `/v1/exec`.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use mes_client::orders::{WorkOrder, WorkOrderDetail, WorkOrderInput};
use mes_client::ws::WsEvent;
use mes_core::work_order::WoStatus;
use mes_db::repo_orders;

use crate::api::{audit, err, repo_err, require_pool, ApiErr};
use crate::extract::{AuthUser, MasterWriter};
use crate::http::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_orders).post(create_order))
        .route("/:id", get(get_order))
        .route("/:id/release", post(release_order))
        .route("/:id/cancel", post(cancel_order))
        .route("/:id/close", post(close_order))
}

async fn create_order(
    State(state): State<AppState>,
    MasterWriter(actor): MasterWriter,
    Json(input): Json<WorkOrderInput>,
) -> Result<(StatusCode, Json<WorkOrderDetail>), ApiErr> {
    let pool = require_pool(&state)?;
    let detail = repo_orders::create_work_order(pool, &input)
        .await
        .map_err(repo_err)?;
    audit(
        pool,
        Some(&actor.user_id),
        "create",
        "work_order",
        Some(&detail.work_order.id),
        None,
    )
    .await;
    Ok((StatusCode::CREATED, Json(detail)))
}

async fn list_orders(
    State(state): State<AppState>,
    _auth: AuthUser,
) -> Result<Json<Vec<WorkOrder>>, ApiErr> {
    let pool = require_pool(&state)?;
    Ok(Json(
        repo_orders::list_work_orders(pool)
            .await
            .map_err(repo_err)?,
    ))
}

async fn get_order(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<WorkOrderDetail>, ApiErr> {
    let pool = require_pool(&state)?;
    Ok(Json(
        repo_orders::get_work_order_detail(pool, &id)
            .await
            .map_err(repo_err)?,
    ))
}

/// Validate and apply a work-order status transition, publishing a WS event.
/// Shared with `/v1/exec` (operator-driven InProgress/Completed transitions).
pub(crate) async fn transition_wo(
    state: &AppState,
    actor_id: &str,
    id: &str,
    target: WoStatus,
) -> Result<WorkOrder, ApiErr> {
    let pool = require_pool(state)?;
    let wo = repo_orders::get_work_order(pool, id)
        .await
        .map_err(repo_err)?;
    let current = WoStatus::parse(&wo.status).ok_or_else(|| {
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            "unknown stored status",
        )
    })?;
    if !current.can_transition(target) {
        return Err(err(
            StatusCode::CONFLICT,
            "invalid_transition",
            format!(
                "cannot move work order from {} to {}",
                current.as_str(),
                target.as_str()
            ),
        ));
    }
    let updated = repo_orders::set_work_order_status(pool, id, target.as_str())
        .await
        .map_err(repo_err)?;
    audit(
        pool,
        Some(actor_id),
        "status",
        "work_order",
        Some(id),
        Some(serde_json::json!({ "status": target.as_str() })),
    )
    .await;
    state.publish(WsEvent::WorkOrderStatusChanged {
        work_order_id: id.to_string(),
        status: target.as_str().to_string(),
    });
    Ok(updated)
}

async fn release_order(
    State(state): State<AppState>,
    MasterWriter(actor): MasterWriter,
    Path(id): Path<String>,
) -> Result<Json<WorkOrder>, ApiErr> {
    Ok(Json(
        transition_wo(&state, &actor.user_id, &id, WoStatus::Released).await?,
    ))
}

async fn cancel_order(
    State(state): State<AppState>,
    MasterWriter(actor): MasterWriter,
    Path(id): Path<String>,
) -> Result<Json<WorkOrder>, ApiErr> {
    Ok(Json(
        transition_wo(&state, &actor.user_id, &id, WoStatus::Cancelled).await?,
    ))
}

async fn close_order(
    State(state): State<AppState>,
    MasterWriter(actor): MasterWriter,
    Path(id): Path<String>,
) -> Result<Json<WorkOrder>, ApiErr> {
    Ok(Json(
        transition_wo(&state, &actor.user_id, &id, WoStatus::Closed).await?,
    ))
}
