//! The two agent front doors (§8.6, §12 M13): `/v1/copilot` (LLM tool-use loop)
//! and `/mcp` (a spec-compliant JSON-RPC MCP server). Both authenticate the
//! tenant by a plant enrollment token → org scope, and both reach the database
//! only through `mes-agent-tools`, so tenant scoping lives in exactly one place
//! (§14). Read-only tools only (§8.6, §16).

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use mes_agent_tools::{catalog, dispatch, TenantScope};
use mes_client::copilot::{CopilotRequest, CopilotResponse};
use mes_db::repo_sync;
use serde_json::{json, Value};

use crate::api::{err, repo_err, require_pool, ApiErr};
use crate::copilot::{run_copilot, CopilotError};
use crate::http::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/v1/copilot", post(copilot))
        .route("/mcp", post(mcp))
}

fn bearer(headers: &HeaderMap) -> Option<String> {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

/// Resolve the tenant scope from a plant enrollment token (§8.6). The scope can
/// never be widened by the caller.
async fn resolve_scope(state: &AppState, headers: &HeaderMap) -> Result<TenantScope, ApiErr> {
    let pool = require_pool(state)?;
    let token = bearer(headers).ok_or_else(|| {
        err(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "missing tenant token",
        )
    })?;
    match repo_sync::resolve_tenant_by_token(pool, &token)
        .await
        .map_err(repo_err)?
    {
        Some((_plant_id, org_id)) => Ok(TenantScope::new(org_id)),
        None => Err(err(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "invalid tenant token",
        )),
    }
}

// ---- Copilot -------------------------------------------------------------

async fn copilot(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CopilotRequest>,
) -> Result<Json<CopilotResponse>, ApiErr> {
    let scope = resolve_scope(&state, &headers).await?;
    let pool = require_pool(&state)?;

    let resp = run_copilot(pool, state.backend.as_ref(), &scope, &req.message)
        .await
        .map_err(copilot_err)?;

    // Audit only (§7 — the copilot is stateless; this is not a stored session).
    let _ = sqlx::query(
        "INSERT INTO copilot_messages (id, org_id, role, content) VALUES ($1, $2, 'user', $3)",
    )
    .bind(mes_core::new_id())
    .bind(&scope.org_id)
    .bind(&req.message)
    .execute(pool)
    .await;
    let _ = sqlx::query(
        "INSERT INTO copilot_messages (id, org_id, role, content, tool_calls)
         VALUES ($1, $2, 'assistant', $3, $4)",
    )
    .bind(mes_core::new_id())
    .bind(&scope.org_id)
    .bind(&resp.reply)
    .bind(sqlx::types::Json(json!(resp.tool_calls)))
    .execute(pool)
    .await;

    Ok(Json(resp))
}

fn copilot_err(e: CopilotError) -> ApiErr {
    match e {
        CopilotError::Unavailable(m) => {
            err(StatusCode::SERVICE_UNAVAILABLE, "copilot_unavailable", m)
        }
        CopilotError::Llm(m) => err(StatusCode::BAD_GATEWAY, "llm_error", m),
        CopilotError::NoConverge => err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "no_converge",
            "tool loop did not converge",
        ),
    }
}

// ---- MCP (JSON-RPC 2.0 over HTTP) ----------------------------------------

/// A single JSON-RPC request. The MCP Streamable-HTTP transport is JSON-RPC over
/// POST; this implements `initialize`, `tools/list`, and `tools/call`.
async fn mcp(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<Value>,
) -> Result<Json<Value>, ApiErr> {
    let scope = resolve_scope(&state, &headers).await?;
    let pool = require_pool(&state)?;

    let id = req.get("id").cloned().unwrap_or(Value::Null);
    let method = req.get("method").and_then(Value::as_str).unwrap_or("");

    let result: Result<Value, (i64, String)> = match method {
        "initialize" => Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "electronix-mes", "version": mes_core::VERSION }
        })),
        "notifications/initialized" => return Ok(Json(Value::Null)),
        "tools/list" => Ok(json!({
            "tools": catalog().iter().map(|t| json!({
                "name": t.name,
                "description": t.description,
                "inputSchema": t.input_schema,
            })).collect::<Vec<_>>()
        })),
        "tools/call" => {
            let params = req.get("params").cloned().unwrap_or(json!({}));
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            match dispatch(pool, &scope, name, &args).await {
                Ok(v) => Ok(json!({
                    "content": [ { "type": "text", "text": v.to_string() } ],
                    "isError": false
                })),
                Err(e) => Ok(json!({
                    "content": [ { "type": "text", "text": e.to_string() } ],
                    "isError": true
                })),
            }
        }
        other => Err((-32601, format!("method not found: {other}"))),
    };

    let envelope = match result {
        Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
        Err((code, message)) => json!({
            "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message }
        }),
    };
    Ok(Json(envelope))
}
