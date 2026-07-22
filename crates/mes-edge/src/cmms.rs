//! `/v1/cmms` — PM schedules, maintenance work orders, spares, and procurement
//! requests (§7, §10, §12 M9).
//!
//! Managing CMMS records is maintenance work (`roles::can_manage_maintenance`
//! → Maintenance/Supervisor/Admin). Reads (due list, WO board, spare stock,
//! procurement queue) are open to any authenticated user so operators and
//! planners can see them.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use mes_client::cmms::{
    MaintenanceTransitionInput, MaintenanceWo, MaintenanceWoInput, PmDue, PmSchedule,
    PmScheduleInput, ProcurementRequest, ProcurementRequestInput, SparePart, SparePartInput,
    SpareTxnInput, SpareTxnResponse,
};
use mes_db::repo_cmms;
use rust_decimal::Decimal;

use crate::api::{audit, err, repo_err, require_pool, ApiErr};
use crate::extract::AuthUser;
use crate::http::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/pm-schedules", post(create_pm_schedule))
        .route("/pm-schedules/due", get(list_pm_due))
        .route(
            "/work-orders",
            get(list_maintenance_wos).post(create_maintenance_wo),
        )
        .route("/work-orders/:id", get(get_maintenance_wo))
        .route(
            "/work-orders/:id/transition",
            post(transition_maintenance_wo),
        )
        .route("/spares", get(list_spare_parts).post(create_spare_part))
        .route("/spares/txns", post(record_spare_txn))
        .route(
            "/procurement",
            get(list_procurement_requests).post(create_procurement_request),
        )
}

fn require_maintenance(auth: &AuthUser) -> Result<(), ApiErr> {
    if mes_core::roles::can_manage_maintenance(&auth.role) {
        Ok(())
    } else {
        Err(err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "maintenance role required",
        ))
    }
}

// ---- PM schedules --------------------------------------------------------

async fn create_pm_schedule(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<PmScheduleInput>,
) -> Result<(StatusCode, Json<PmSchedule>), ApiErr> {
    require_maintenance(&auth)?;
    let pool = require_pool(&state)?;
    let sched = repo_cmms::create_pm_schedule(
        pool,
        &input.work_center_id,
        &input.name,
        &input.trigger_type,
        input.interval_value,
        input.checklist_ref.as_deref(),
    )
    .await
    .map_err(repo_err)?;
    audit(
        pool,
        Some(&auth.user_id),
        "create",
        "pm_schedule",
        Some(&sched.id),
        None,
    )
    .await;
    Ok((StatusCode::CREATED, Json(sched)))
}

async fn list_pm_due(
    State(state): State<AppState>,
    _auth: AuthUser,
) -> Result<Json<Vec<PmDue>>, ApiErr> {
    let pool = require_pool(&state)?;
    Ok(Json(repo_cmms::list_pm_due(pool).await.map_err(repo_err)?))
}

// ---- Maintenance work orders ---------------------------------------------

async fn create_maintenance_wo(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<MaintenanceWoInput>,
) -> Result<(StatusCode, Json<MaintenanceWo>), ApiErr> {
    require_maintenance(&auth)?;
    let pool = require_pool(&state)?;
    let wo = repo_cmms::create_maintenance_wo(
        pool,
        &input.work_center_id,
        input.pm_schedule_id.as_deref(),
        &input.wo_type,
        input.notes.as_deref(),
    )
    .await
    .map_err(repo_err)?;
    audit(
        pool,
        Some(&auth.user_id),
        "create",
        "maintenance_wo",
        Some(&wo.id),
        None,
    )
    .await;
    Ok((StatusCode::CREATED, Json(wo)))
}

async fn list_maintenance_wos(
    State(state): State<AppState>,
    _auth: AuthUser,
) -> Result<Json<Vec<MaintenanceWo>>, ApiErr> {
    let pool = require_pool(&state)?;
    Ok(Json(
        repo_cmms::list_maintenance_wos(pool)
            .await
            .map_err(repo_err)?,
    ))
}

async fn get_maintenance_wo(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<MaintenanceWo>, ApiErr> {
    let pool = require_pool(&state)?;
    Ok(Json(
        repo_cmms::get_maintenance_wo(pool, &id)
            .await
            .map_err(repo_err)?,
    ))
}

async fn transition_maintenance_wo(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(input): Json<MaintenanceTransitionInput>,
) -> Result<Json<MaintenanceWo>, ApiErr> {
    require_maintenance(&auth)?;
    let pool = require_pool(&state)?;
    let wo = repo_cmms::transition_maintenance_wo(
        pool,
        &id,
        &input.status,
        input.technician_id.as_deref(),
        input.failure_code.as_deref(),
    )
    .await
    .map_err(repo_err)?;
    audit(
        pool,
        Some(&auth.user_id),
        "transition",
        "maintenance_wo",
        Some(&id),
        Some(serde_json::json!({ "status": input.status })),
    )
    .await;
    Ok(Json(wo))
}

// ---- Spares --------------------------------------------------------------

async fn create_spare_part(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<SparePartInput>,
) -> Result<(StatusCode, Json<SparePart>), ApiErr> {
    require_maintenance(&auth)?;
    let pool = require_pool(&state)?;
    let uom = input.uom.unwrap_or_else(|| "ea".to_string());
    let spare = repo_cmms::create_spare_part(
        pool,
        &input.code,
        &input.name,
        &uom,
        input.reorder_point.unwrap_or(Decimal::ZERO),
        input.reorder_qty.unwrap_or(Decimal::ZERO),
    )
    .await
    .map_err(repo_err)?;
    Ok((StatusCode::CREATED, Json(spare)))
}

async fn list_spare_parts(
    State(state): State<AppState>,
    _auth: AuthUser,
) -> Result<Json<Vec<SparePart>>, ApiErr> {
    let pool = require_pool(&state)?;
    Ok(Json(
        repo_cmms::list_spare_parts(pool).await.map_err(repo_err)?,
    ))
}

async fn record_spare_txn(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<SpareTxnInput>,
) -> Result<(StatusCode, Json<SpareTxnResponse>), ApiErr> {
    require_maintenance(&auth)?;
    let pool = require_pool(&state)?;
    let (txn_id, stock, procurement_request) = repo_cmms::record_spare_txn(
        pool,
        &input.spare_part_id,
        input.maintenance_wo_id.as_deref(),
        &input.txn_type,
        input.qty,
        &auth.user_id,
    )
    .await
    .map_err(repo_err)?;
    Ok((
        StatusCode::CREATED,
        Json(SpareTxnResponse {
            txn_id,
            stock,
            procurement_request,
        }),
    ))
}

// ---- Procurement requests ------------------------------------------------

async fn list_procurement_requests(
    State(state): State<AppState>,
    _auth: AuthUser,
) -> Result<Json<Vec<ProcurementRequest>>, ApiErr> {
    let pool = require_pool(&state)?;
    Ok(Json(
        repo_cmms::list_procurement_requests(pool)
            .await
            .map_err(repo_err)?,
    ))
}

async fn create_procurement_request(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<ProcurementRequestInput>,
) -> Result<(StatusCode, Json<ProcurementRequest>), ApiErr> {
    require_maintenance(&auth)?;
    let pool = require_pool(&state)?;
    let req =
        repo_cmms::create_procurement_request(pool, &input.spare_part_id, input.qty_requested)
            .await
            .map_err(repo_err)?;
    audit(
        pool,
        Some(&auth.user_id),
        "create",
        "procurement_request",
        Some(&req.id),
        None,
    )
    .await;
    Ok((StatusCode::CREATED, Json(req)))
}
