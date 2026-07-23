//! `/v1/erp` — the admin ERP integration surface (§7, §10, §12 M10).
//!
//! Connection config CRUD (endpoint, encrypted token, JSONB field-mapping,
//! direction), a generic import and a generic export ("sync now") driven purely
//! by the stored mapping, and the sync-log audit trail. No per-customer code
//! (§3): a different ERP shape is a mapping change only. All actions are gated
//! to master-writers (Admin/Planner) since they carry credentials and config.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use mes_client::erp::{
    ErpConnection, ErpConnectionInput, ErpExportRequest, ErpExportResult, ErpImportRequest,
    ErpImportResult, ErpSyncLogEntry,
};
use mes_client::orders::WorkOrderInput;
use mes_db::{repo_cmms, repo_erp, repo_orders};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::api::{audit, err, repo_err, require_pool, ApiErr};
use crate::extract::MasterWriter;
use crate::http::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/connections",
            get(list_connections).post(create_connection),
        )
        .route(
            "/connections/:id",
            get(get_connection)
                .put(update_connection)
                .delete(delete_connection),
        )
        .route("/import", post(import))
        .route("/export", post(export))
        .route("/sync-log", get(list_sync_log))
}

fn normalize_direction(d: Option<&str>) -> Result<String, ApiErr> {
    match d.unwrap_or("both") {
        v @ ("import" | "export" | "both") => Ok(v.to_string()),
        _ => Err(err(
            StatusCode::BAD_REQUEST,
            "bad_direction",
            "direction must be import|export|both",
        )),
    }
}

fn field_mapping_or_default(v: Value) -> Value {
    if v.is_null() {
        json!({})
    } else {
        v
    }
}

/// Encrypt a non-empty plaintext token; an empty/absent token yields `None`.
fn encrypt_token(state: &AppState, plain: Option<&str>) -> Result<Option<String>, ApiErr> {
    match plain.filter(|t| !t.is_empty()) {
        Some(t) => mes_erp::crypto::encrypt(&state.erp_key, t)
            .map(Some)
            .map_err(|_| {
                err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal",
                    "token encryption failed",
                )
            }),
        None => Ok(None),
    }
}

// ---- Connection CRUD -----------------------------------------------------

async fn create_connection(
    State(state): State<AppState>,
    MasterWriter(actor): MasterWriter,
    Json(input): Json<ErpConnectionInput>,
) -> Result<(StatusCode, Json<ErpConnection>), ApiErr> {
    let pool = require_pool(&state)?;
    let direction = normalize_direction(input.direction.as_deref())?;
    let token_enc = encrypt_token(&state, input.auth_token.as_deref())?;
    let mapping = field_mapping_or_default(input.field_mapping);
    let conn = repo_erp::create_connection(
        pool,
        input.site_id.as_deref(),
        &input.name,
        &input.endpoint_url,
        token_enc.as_deref(),
        &mapping,
        &direction,
        input.enabled.unwrap_or(true),
    )
    .await
    .map_err(repo_err)?;
    audit(
        pool,
        Some(&actor.user_id),
        "create",
        "erp_connection",
        Some(&conn.id),
        None,
    )
    .await;
    Ok((StatusCode::CREATED, Json(conn)))
}

async fn list_connections(
    State(state): State<AppState>,
    _actor: MasterWriter,
) -> Result<Json<Vec<ErpConnection>>, ApiErr> {
    let pool = require_pool(&state)?;
    Ok(Json(
        repo_erp::list_connections(pool).await.map_err(repo_err)?,
    ))
}

async fn get_connection(
    State(state): State<AppState>,
    _actor: MasterWriter,
    Path(id): Path<String>,
) -> Result<Json<ErpConnection>, ApiErr> {
    let pool = require_pool(&state)?;
    Ok(Json(
        repo_erp::get_connection(pool, &id)
            .await
            .map_err(repo_err)?,
    ))
}

async fn update_connection(
    State(state): State<AppState>,
    MasterWriter(actor): MasterWriter,
    Path(id): Path<String>,
    Json(input): Json<ErpConnectionInput>,
) -> Result<Json<ErpConnection>, ApiErr> {
    let pool = require_pool(&state)?;
    let direction = normalize_direction(input.direction.as_deref())?;
    let token_enc = encrypt_token(&state, input.auth_token.as_deref())?;
    let mapping = field_mapping_or_default(input.field_mapping);
    let conn = repo_erp::update_connection(
        pool,
        &id,
        input.site_id.as_deref(),
        &input.name,
        &input.endpoint_url,
        token_enc.as_deref(),
        &mapping,
        &direction,
        input.enabled.unwrap_or(true),
    )
    .await
    .map_err(repo_err)?;
    audit(
        pool,
        Some(&actor.user_id),
        "update",
        "erp_connection",
        Some(&id),
        None,
    )
    .await;
    Ok(Json(conn))
}

async fn delete_connection(
    State(state): State<AppState>,
    MasterWriter(actor): MasterWriter,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiErr> {
    let pool = require_pool(&state)?;
    repo_erp::delete_connection(pool, &id)
        .await
        .map_err(repo_err)?;
    audit(
        pool,
        Some(&actor.user_id),
        "delete",
        "erp_connection",
        Some(&id),
        None,
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

// ---- Import --------------------------------------------------------------

async fn import(
    State(state): State<AppState>,
    MasterWriter(actor): MasterWriter,
    Json(req): Json<ErpImportRequest>,
) -> Result<(StatusCode, Json<ErpImportResult>), ApiErr> {
    let pool = require_pool(&state)?;
    let conn = repo_erp::get_connection_secret(pool, &req.connection_id)
        .await
        .map_err(repo_err)?;
    if !conn.enabled {
        return Err(err(
            StatusCode::CONFLICT,
            "disabled",
            "connection is disabled",
        ));
    }
    if !matches!(conn.direction.as_str(), "import" | "both") {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "bad_direction",
            "connection does not allow import",
        ));
    }
    let mapping = mes_erp::FieldMapping::from_json(&conn.field_mapping)
        .map_err(|e| err(StatusCode::BAD_REQUEST, "bad_mapping", e.to_string()))?;

    let ids = match req.entity.as_str() {
        "work_order" => import_work_orders(pool, &mapping, &req.records).await,
        other => {
            return Err(err(
                StatusCode::BAD_REQUEST,
                "bad_entity",
                format!("import not supported for entity '{other}'"),
            ))
        }
    };

    match ids {
        Ok(ids) => {
            let log_id = repo_erp::insert_sync_log(
                pool,
                &req.connection_id,
                "import",
                &req.entity,
                ids.len() as i32,
                "success",
                None,
            )
            .await
            .map_err(repo_err)?;
            audit(
                pool,
                Some(&actor.user_id),
                "import",
                "erp",
                Some(&req.connection_id),
                Some(json!({ "entity": req.entity, "count": ids.len() })),
            )
            .await;
            Ok((
                StatusCode::CREATED,
                Json(ErpImportResult {
                    entity: req.entity,
                    imported: ids.len(),
                    ids,
                    sync_log_id: log_id,
                }),
            ))
        }
        Err(detail) => {
            let _ = repo_erp::insert_sync_log(
                pool,
                &req.connection_id,
                "import",
                &req.entity,
                0,
                "error",
                Some(&detail),
            )
            .await;
            Err(err(StatusCode::BAD_REQUEST, "import_failed", detail))
        }
    }
}

/// Map each external record to a canonical `WorkOrderInput` and create it.
/// Fails (with a human detail) on the first record that does not map cleanly.
async fn import_work_orders(
    pool: &sqlx::PgPool,
    mapping: &mes_erp::FieldMapping,
    records: &[Value],
) -> Result<Vec<String>, String> {
    let mut ids = Vec::with_capacity(records.len());
    for (i, rec) in records.iter().enumerate() {
        let canonical = mapping.to_canonical(rec);
        let input: WorkOrderInput =
            serde_json::from_value(canonical).map_err(|e| format!("record {i}: {e}"))?;
        let detail = repo_orders::create_work_order(pool, &input)
            .await
            .map_err(|e| format!("record {i}: {e:?}"))?;
        ids.push(detail.work_order.id);
    }
    Ok(ids)
}

// ---- Export ("sync now") -------------------------------------------------

async fn export(
    State(state): State<AppState>,
    MasterWriter(actor): MasterWriter,
    Json(req): Json<ErpExportRequest>,
) -> Result<Json<ErpExportResult>, ApiErr> {
    let pool = require_pool(&state)?;
    let conn = repo_erp::get_connection_secret(pool, &req.connection_id)
        .await
        .map_err(repo_err)?;
    if !conn.enabled {
        return Err(err(
            StatusCode::CONFLICT,
            "disabled",
            "connection is disabled",
        ));
    }
    if !matches!(conn.direction.as_str(), "export" | "both") {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "bad_direction",
            "connection does not allow export",
        ));
    }
    let mapping = mes_erp::FieldMapping::from_json(&conn.field_mapping)
        .map_err(|e| err(StatusCode::BAD_REQUEST, "bad_mapping", e.to_string()))?;

    // Gather canonical records for the entity (+ ids of any that a successful
    // push should transition, e.g. procurement requests → SentToErp).
    let (canonical, procurement_ids): (Vec<Value>, Vec<String>) = match req.entity.as_str() {
        "stock_level" => {
            let spares = repo_cmms::list_spare_parts(pool).await.map_err(repo_err)?;
            let recs = spares
                .iter()
                .map(|s| {
                    json!({
                        "code": s.code, "name": s.name,
                        "stock": s.stock, "reorder_point": s.reorder_point
                    })
                })
                .collect();
            (recs, Vec::new())
        }
        "procurement_request" => {
            let all = repo_cmms::list_procurement_requests(pool)
                .await
                .map_err(repo_err)?;
            let requested: Vec<_> = all
                .into_iter()
                .filter(|r| r.status == "requested")
                .collect();
            let ids = requested.iter().map(|r| r.id.clone()).collect();
            let recs = requested
                .iter()
                .map(|r| {
                    json!({
                        "request_id": r.id, "spare_part_id": r.spare_part_id,
                        "qty_requested": r.qty_requested
                    })
                })
                .collect();
            (recs, ids)
        }
        other => {
            return Err(err(
                StatusCode::BAD_REQUEST,
                "bad_entity",
                format!("export not supported for entity '{other}'"),
            ))
        }
    };

    let payload: Vec<Value> = canonical.iter().map(|c| mapping.to_external(c)).collect();

    // Decrypt the token (if any) and push to the ERP endpoint.
    let token = match &conn.auth_token_enc {
        Some(enc) => Some(mes_erp::crypto::decrypt(&state.erp_key, enc).map_err(|_| {
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                "token decrypt failed",
            )
        })?),
        None => None,
    };

    let push = state
        .erp
        .post_json(&conn.endpoint_url, token.as_deref(), &json!(payload))
        .await;

    match push {
        Ok(resp) => {
            // Transition procurement requests that were just pushed (§12 M10).
            if !procurement_ids.is_empty() {
                let erp_ref = resp.get("reference").and_then(Value::as_str);
                repo_erp::mark_procurement_sent(pool, &procurement_ids, erp_ref)
                    .await
                    .map_err(repo_err)?;
            }
            let log_id = repo_erp::insert_sync_log(
                pool,
                &req.connection_id,
                "export",
                &req.entity,
                payload.len() as i32,
                "success",
                None,
            )
            .await
            .map_err(repo_err)?;
            audit(
                pool,
                Some(&actor.user_id),
                "export",
                "erp",
                Some(&req.connection_id),
                Some(json!({ "entity": req.entity, "count": payload.len() })),
            )
            .await;
            Ok(Json(ErpExportResult {
                entity: req.entity,
                record_count: payload.len(),
                pushed: true,
                payload,
                sync_log_id: log_id,
            }))
        }
        Err(e) => {
            let detail = e.to_string();
            let _ = repo_erp::insert_sync_log(
                pool,
                &req.connection_id,
                "export",
                &req.entity,
                payload.len() as i32,
                "error",
                Some(&detail),
            )
            .await;
            Err(err(StatusCode::BAD_GATEWAY, "export_failed", detail))
        }
    }
}

// ---- Sync log ------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct SyncLogQuery {
    connection_id: Option<String>,
}

async fn list_sync_log(
    State(state): State<AppState>,
    _actor: MasterWriter,
    Query(q): Query<SyncLogQuery>,
) -> Result<Json<Vec<ErpSyncLogEntry>>, ApiErr> {
    let pool = require_pool(&state)?;
    Ok(Json(
        repo_erp::list_sync_log(pool, q.connection_id.as_deref())
            .await
            .map_err(repo_err)?,
    ))
}
