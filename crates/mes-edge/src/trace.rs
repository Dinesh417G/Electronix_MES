//! `/v1/trace` — lots/serials, genealogy, holds, material issue, and recursive
//! forward/backward trace (§7, §10, §12 M7).
//!
//! Creating lots/serials/genealogy and issuing material are operator actions
//! (any authenticated user). Placing/releasing holds is a quality action
//! (`roles::can_manage_quality`). A held lot cannot be issued.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use mes_client::trace::{
    BarcodeParsed, GenealogyEdgeInput, HoldInput, IssueMaterialInput, Lot, LotInput, Serial,
    SerialInput, TraceNode,
};
use mes_db::repo_trace;
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::json;

use crate::api::{audit, err, repo_err, require_pool, ApiErr};
use crate::extract::AuthUser;
use crate::http::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/lots", post(create_lot))
        .route("/serials", post(create_serial))
        .route("/genealogy", post(add_genealogy))
        .route("/material/issue", post(issue_material))
        .route("/holds", post(place_hold))
        .route("/holds/:id/release", post(release_hold))
        .route("/backward/:entity_type/:entity_id", get(trace_backward))
        .route("/forward/:entity_type/:entity_id", get(trace_forward))
        .route("/barcode", get(parse_barcode))
}

async fn create_lot(
    State(state): State<AppState>,
    _auth: AuthUser,
    Json(input): Json<LotInput>,
) -> Result<(StatusCode, Json<Lot>), ApiErr> {
    let pool = require_pool(&state)?;
    let qty = input.qty.unwrap_or(Decimal::ZERO);
    let uom = input.uom.unwrap_or_else(|| "ea".to_string());
    let lot = repo_trace::create_lot(pool, &input.lot_no, &input.part_id, qty, &uom)
        .await
        .map_err(repo_err)?;
    Ok((StatusCode::CREATED, Json(lot)))
}

async fn create_serial(
    State(state): State<AppState>,
    _auth: AuthUser,
    Json(input): Json<SerialInput>,
) -> Result<(StatusCode, Json<Serial>), ApiErr> {
    let pool = require_pool(&state)?;
    let serial = repo_trace::create_serial(
        pool,
        &input.serial_no,
        &input.part_id,
        input.lot_id.as_deref(),
    )
    .await
    .map_err(repo_err)?;
    Ok((StatusCode::CREATED, Json(serial)))
}

async fn add_genealogy(
    State(state): State<AppState>,
    _auth: AuthUser,
    Json(input): Json<GenealogyEdgeInput>,
) -> Result<StatusCode, ApiErr> {
    let pool = require_pool(&state)?;
    repo_trace::add_genealogy(
        pool,
        &input.parent_type,
        &input.parent_id,
        &input.child_type,
        &input.child_id,
        input.qty,
    )
    .await
    .map_err(repo_err)?;
    Ok(StatusCode::CREATED)
}

async fn issue_material(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<IssueMaterialInput>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiErr> {
    let pool = require_pool(&state)?;
    let txn_id = repo_trace::issue_material(
        pool,
        input.lot_id.as_deref(),
        input.serial_id.as_deref(),
        input.qty,
        input.wo_operation_id.as_deref(),
        &auth.user_id,
    )
    .await
    .map_err(repo_err)?;
    Ok((StatusCode::CREATED, Json(json!({ "txn_id": txn_id }))))
}

async fn place_hold(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<HoldInput>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiErr> {
    if !mes_core::roles::can_manage_quality(&auth.role) {
        return Err(err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "role may not place holds",
        ));
    }
    let pool = require_pool(&state)?;
    let id = repo_trace::place_hold(
        pool,
        &input.entity_type,
        &input.entity_id,
        input.reason.as_deref(),
        &auth.user_id,
    )
    .await
    .map_err(repo_err)?;
    audit(
        pool,
        Some(&auth.user_id),
        "hold",
        &input.entity_type,
        Some(&input.entity_id),
        None,
    )
    .await;
    Ok((StatusCode::CREATED, Json(json!({ "hold_id": id }))))
}

async fn release_hold(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiErr> {
    if !mes_core::roles::can_manage_quality(&auth.role) {
        return Err(err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "role may not release holds",
        ));
    }
    let pool = require_pool(&state)?;
    repo_trace::release_hold(pool, &id, &auth.user_id)
        .await
        .map_err(repo_err)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn trace_backward(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path((entity_type, entity_id)): Path<(String, String)>,
) -> Result<Json<Vec<TraceNode>>, ApiErr> {
    let pool = require_pool(&state)?;
    Ok(Json(
        repo_trace::trace_backward(pool, &entity_type, &entity_id)
            .await
            .map_err(repo_err)?,
    ))
}

async fn trace_forward(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path((entity_type, entity_id)): Path<(String, String)>,
) -> Result<Json<Vec<TraceNode>>, ApiErr> {
    let pool = require_pool(&state)?;
    Ok(Json(
        repo_trace::trace_forward(pool, &entity_type, &entity_id)
            .await
            .map_err(repo_err)?,
    ))
}

#[derive(Debug, Deserialize)]
struct BarcodeQuery {
    code: String,
}

/// Parse an `EMX1|<type>|<id>` barcode into its parts.
async fn parse_barcode(
    _auth: AuthUser,
    axum::extract::Query(q): axum::extract::Query<BarcodeQuery>,
) -> Result<Json<BarcodeParsed>, ApiErr> {
    let (type_code, id) = mes_core::barcode::parse(&q.code)
        .ok_or_else(|| err(StatusCode::BAD_REQUEST, "bad_barcode", "malformed barcode"))?;
    Ok(Json(BarcodeParsed { type_code, id }))
}
