//! `/v1/sync` (cloud) — org/plant provisioning + enrollment, the idempotent
//! push/pull/ack protocol, remote work-order creation, and the multi-plant
//! dashboard list (§8.3, §10, §12 M12).
//!
//! Provisioning is gated by the optional cloud admin token; push/pull/ack are
//! authenticated by the plant's enrollment token (bearer). Apply is idempotent,
//! so a replayed batch after a long outage is a safe no-op.

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use mes_client::sync::{
    AckRequest, Org, OrgInput, PlantEnrollment, PlantInput, PlantSummary, PullResponse,
    PushRequest, PushResponse, RemoteWorkOrderInput,
};
use mes_db::repo_sync;
use serde::Deserialize;
use serde_json::json;

use crate::api::{err, repo_err, require_pool, ApiErr};
use crate::http::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/orgs", post(create_org))
        .route("/orgs/:id/plants", post(enroll_plant))
        .route("/plants", get(list_plants))
        .route("/plants/:id/work-orders", post(remote_work_order))
        .route("/push", post(push))
        .route("/pull", get(pull))
        .route("/ack", post(ack))
}

fn bearer(headers: &HeaderMap) -> Option<String> {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

/// Gate provisioning on the admin token when one is configured.
fn require_admin(state: &AppState, headers: &HeaderMap) -> Result<(), ApiErr> {
    match &state.admin_token {
        None => Ok(()),
        Some(expected) => {
            if bearer(headers).as_deref() == Some(expected.as_str()) {
                Ok(())
            } else {
                Err(err(
                    StatusCode::UNAUTHORIZED,
                    "unauthorized",
                    "admin token required",
                ))
            }
        }
    }
}

/// Authenticate a plant by its enrollment token (bearer) against a claimed id.
async fn require_plant(
    state: &AppState,
    headers: &HeaderMap,
    plant_id: &str,
) -> Result<(), ApiErr> {
    let pool = require_pool(state)?;
    let token = bearer(headers).ok_or_else(|| {
        err(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "missing plant token",
        )
    })?;
    if repo_sync::verify_plant_token(pool, plant_id, &token)
        .await
        .map_err(repo_err)?
    {
        Ok(())
    } else {
        Err(err(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "invalid plant token",
        ))
    }
}

// ---- Provisioning --------------------------------------------------------

async fn create_org(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<OrgInput>,
) -> Result<(StatusCode, Json<Org>), ApiErr> {
    require_admin(&state, &headers)?;
    let pool = require_pool(&state)?;
    let org = repo_sync::create_org(pool, &input.code, &input.name)
        .await
        .map_err(repo_err)?;
    Ok((StatusCode::CREATED, Json(org)))
}

async fn enroll_plant(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(org_id): Path<String>,
    Json(input): Json<PlantInput>,
) -> Result<(StatusCode, Json<PlantEnrollment>), ApiErr> {
    require_admin(&state, &headers)?;
    let pool = require_pool(&state)?;
    let (id, token) = repo_sync::enroll_plant(pool, &org_id, &input.code, &input.name)
        .await
        .map_err(repo_err)?;
    Ok((
        StatusCode::CREATED,
        Json(PlantEnrollment {
            id,
            org_id,
            code: input.code,
            name: input.name,
            token,
        }),
    ))
}

async fn list_plants(State(state): State<AppState>) -> Result<Json<Vec<PlantSummary>>, ApiErr> {
    let pool = require_pool(&state)?;
    Ok(Json(repo_sync::list_plants(pool).await.map_err(repo_err)?))
}

async fn remote_work_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(plant_id): Path<String>,
    Json(input): Json<RemoteWorkOrderInput>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiErr> {
    require_admin(&state, &headers)?;
    let pool = require_pool(&state)?;
    let wo_id = repo_sync::create_remote_work_order(
        pool,
        &plant_id,
        &input.wo_number,
        &input.part_id,
        input.qty_ordered,
        input.priority,
    )
    .await
    .map_err(repo_err)?;
    Ok((StatusCode::CREATED, Json(json!({ "work_order_id": wo_id }))))
}

// ---- Sync protocol -------------------------------------------------------

async fn push(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<PushRequest>,
) -> Result<Json<PushResponse>, ApiErr> {
    require_plant(&state, &headers, &req.plant_id).await?;
    if req.entries.len() > mes_sync::MAX_BATCH {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "batch_too_large",
            format!("batch exceeds {} entries", mes_sync::MAX_BATCH),
        ));
    }
    let pool = require_pool(&state)?;
    let (applied, skipped) = repo_sync::apply_batch(pool, &req.entries)
        .await
        .map_err(repo_err)?;
    repo_sync::touch_plant_sync(pool, &req.plant_id)
        .await
        .map_err(repo_err)?;
    Ok(Json(PushResponse { applied, skipped }))
}

#[derive(Debug, Deserialize)]
struct PullQuery {
    plant_id: String,
    limit: Option<i64>,
}

async fn pull(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<PullQuery>,
) -> Result<Json<PullResponse>, ApiErr> {
    require_plant(&state, &headers, &q.plant_id).await?;
    let pool = require_pool(&state)?;
    let limit = q
        .limit
        .unwrap_or(mes_sync::MAX_BATCH as i64)
        .min(mes_sync::MAX_BATCH as i64);
    let entries = repo_sync::pull_for_plant(pool, &q.plant_id, limit)
        .await
        .map_err(repo_err)?;
    Ok(Json(PullResponse { entries }))
}

async fn ack(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<AckRequest>,
) -> Result<StatusCode, ApiErr> {
    require_plant(&state, &headers, &req.plant_id).await?;
    let pool = require_pool(&state)?;
    repo_sync::mark_synced(pool, &req.ids)
        .await
        .map_err(repo_err)?;
    Ok(StatusCode::NO_CONTENT)
}
