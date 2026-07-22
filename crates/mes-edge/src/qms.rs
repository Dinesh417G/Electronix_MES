//! `/v1/qms` — inspection plans/characteristics/results + NCR disposition
//! (§8, §10, §12 M8).
//!
//! Managing plans/characteristics and dispositioning NCRs are quality actions
//! (`roles::can_manage_quality`). Recording a measurement is open to any
//! authenticated user (operators/inspectors); a failing measurement auto-raises
//! an NCR + hold and broadcasts an andon event.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use mes_client::qms::{
    Characteristic, CharacteristicInput, DispositionInput, Ncr, Plan, PlanInput,
    RecordResultResponse, ResultInput,
};
use mes_client::ws::WsEvent;
use mes_core::qms::Disposition;
use mes_db::repo_qms;

use crate::api::{audit, err, repo_err, require_pool, ApiErr};
use crate::extract::AuthUser;
use crate::http::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/plans", post(create_plan))
        .route("/characteristics", post(create_characteristic))
        .route("/results", post(record_result))
        .route("/ncrs", get(list_ncrs))
        .route("/ncrs/:id", get(get_ncr))
        .route("/ncrs/:id/disposition", post(disposition_ncr))
}

fn require_quality(auth: &AuthUser) -> Result<(), ApiErr> {
    if mes_core::roles::can_manage_quality(&auth.role) {
        Ok(())
    } else {
        Err(err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "quality role required",
        ))
    }
}

async fn create_plan(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<PlanInput>,
) -> Result<(StatusCode, Json<Plan>), ApiErr> {
    require_quality(&auth)?;
    let pool = require_pool(&state)?;
    let plan = repo_qms::create_plan(pool, &input.part_id, &input.code, &input.name)
        .await
        .map_err(repo_err)?;
    Ok((StatusCode::CREATED, Json(plan)))
}

async fn create_characteristic(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<CharacteristicInput>,
) -> Result<(StatusCode, Json<Characteristic>), ApiErr> {
    require_quality(&auth)?;
    let pool = require_pool(&state)?;
    let ch = repo_qms::create_characteristic(
        pool,
        &input.plan_id,
        &input.name,
        input.uom.as_deref(),
        input.nominal,
        input.lower_limit,
        input.upper_limit,
    )
    .await
    .map_err(repo_err)?;
    Ok((StatusCode::CREATED, Json(ch)))
}

async fn record_result(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<ResultInput>,
) -> Result<(StatusCode, Json<RecordResultResponse>), ApiErr> {
    let pool = require_pool(&state)?;
    let (result, ncr) = repo_qms::record_result(
        pool,
        &input.characteristic_id,
        input.lot_id.as_deref(),
        input.serial_id.as_deref(),
        input.wo_operation_id.as_deref(),
        input.measured_value,
        &auth.user_id,
    )
    .await
    .map_err(repo_err)?;

    if let Some(n) = &ncr {
        audit(pool, Some(&auth.user_id), "raise", "ncr", Some(&n.id), None).await;
        state.publish(WsEvent::NcrRaised {
            ncr_id: n.id.clone(),
            ncr_no: n.ncr_no.clone(),
            lot_id: n.lot_id.clone(),
        });
    }

    Ok((
        StatusCode::CREATED,
        Json(RecordResultResponse { result, ncr }),
    ))
}

async fn list_ncrs(
    State(state): State<AppState>,
    _auth: AuthUser,
) -> Result<Json<Vec<Ncr>>, ApiErr> {
    let pool = require_pool(&state)?;
    Ok(Json(repo_qms::list_ncrs(pool).await.map_err(repo_err)?))
}

async fn get_ncr(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<Ncr>, ApiErr> {
    let pool = require_pool(&state)?;
    Ok(Json(repo_qms::get_ncr(pool, &id).await.map_err(repo_err)?))
}

async fn disposition_ncr(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(input): Json<DispositionInput>,
) -> Result<Json<Ncr>, ApiErr> {
    require_quality(&auth)?;
    let pool = require_pool(&state)?;
    let disposition = Disposition::parse(&input.disposition).ok_or_else(|| {
        err(
            StatusCode::BAD_REQUEST,
            "bad_disposition",
            "disposition must be rework|scrap|use_as_is|return",
        )
    })?;
    let ncr = repo_qms::disposition_ncr(
        pool,
        &id,
        disposition,
        input.reason.as_deref(),
        &auth.user_id,
    )
    .await
    .map_err(repo_err)?;
    audit(
        pool,
        Some(&auth.user_id),
        "disposition",
        "ncr",
        Some(&id),
        Some(serde_json::json!({ "disposition": disposition.as_str() })),
    )
    .await;
    Ok(Json(ncr))
}
