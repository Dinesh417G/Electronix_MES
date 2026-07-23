//! M13 acceptance tests — MCP + Copilot (§12 M13, §13).
//!
//! Tenant isolation is mandatory (§13): a tool call scoped to one org must never
//! see another org's data, via the MCP server *and* the copilot. The copilot
//! round-trips a real question using real (tenant-scoped) tool calls against
//! seeded data, driven by a scripted backend so no live model is needed. Fresh
//! schema per test, gated on `DATABASE_URL`.

mod common;

use std::sync::Arc;

use async_trait::async_trait;
use axum::http::StatusCode;
use common::{call, seed_part, setup, teardown, Ctx};
use mes_agent_tools::ToolDef;
use mes_cloud::copilot::CopilotError;
use mes_cloud::copilot::{AssistantReply, Block, LlmBackend, Message};
use serde_json::{json, Value};

/// Enroll an org+plant and push one tenant-tagged work order; return its token.
async fn seed_tenant(
    ctx: &Ctx,
    org_code: &str,
    plant_code: &str,
    wo_id: &str,
    wo_number: &str,
    part_id: &str,
) -> String {
    let app = ctx.router();
    let (_, org) = call(
        &app,
        "POST",
        "/v1/sync/orgs",
        None,
        Some(json!({"code": org_code, "name": org_code})),
    )
    .await;
    let org_id = org["id"].as_str().unwrap().to_string();
    let (_, plant) = call(
        &app,
        "POST",
        &format!("/v1/sync/orgs/{org_id}/plants"),
        None,
        Some(json!({"code": plant_code, "name": plant_code})),
    )
    .await;
    let plant_id = plant["id"].as_str().unwrap().to_string();
    let token = plant["token"].as_str().unwrap().to_string();

    let entry = json!({
        "id": format!("e_{wo_id}"), "aggregate": "work_order", "entity_id": wo_id, "op": "upsert",
        "payload": { "id": wo_id, "wo_number": wo_number, "part_id": part_id,
                     "qty_ordered": 5, "priority": 100, "status": "released" }
    });
    let (status, _) = call(
        &app,
        "POST",
        "/v1/sync/push",
        Some(&token),
        Some(json!({ "plant_id": plant_id, "entries": [entry] })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    token
}

#[tokio::test]
async fn mcp_is_tenant_scoped_and_lists_tools() {
    let Some(ctx) = setup().await else {
        return;
    };
    seed_part(ctx.pool(), "part_shared", "P-S").await;
    let token_a = seed_tenant(&ctx, "ORGA", "PA", "wo_a", "WO-A-1", "part_shared").await;
    let token_b = seed_tenant(&ctx, "ORGB", "PB", "wo_b", "WO-B-1", "part_shared").await;
    let app = ctx.router();

    // initialize + tools/list.
    let (status, init) = call(
        &app,
        "POST",
        "/mcp",
        Some(&token_a),
        Some(json!({"jsonrpc": "2.0", "id": 1, "method": "initialize"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(init["result"]["serverInfo"]["name"], "electronix-mes");

    let (_, list) = call(
        &app,
        "POST",
        "/mcp",
        Some(&token_a),
        Some(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})),
    )
    .await;
    let names: Vec<&str> = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"get_wo_status"));

    // tools/call get_wo_status as org A → sees only A's WO, never B's.
    let payload = json!({
        "jsonrpc": "2.0", "id": 3, "method": "tools/call",
        "params": { "name": "get_wo_status", "arguments": {} }
    });
    let (_, resa) = call(&app, "POST", "/mcp", Some(&token_a), Some(payload.clone())).await;
    let text_a = resa["result"]["content"][0]["text"].as_str().unwrap();
    let data_a: Value = serde_json::from_str(text_a).unwrap();
    assert_eq!(data_a["total"], 1);
    assert!(text_a.contains("WO-A-1"));
    assert!(
        !text_a.contains("WO-B-1"),
        "org A must not see org B's work order"
    );

    // Same call as org B → only B's WO.
    let (_, resb) = call(&app, "POST", "/mcp", Some(&token_b), Some(payload)).await;
    let text_b = resb["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text_b.contains("WO-B-1"));
    assert!(
        !text_b.contains("WO-A-1"),
        "org B must not see org A's work order"
    );

    // No token → 401.
    let (status, _) = call(
        &app,
        "POST",
        "/mcp",
        None,
        Some(json!({"jsonrpc": "2.0", "id": 4, "method": "tools/list"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    teardown(ctx).await;
}

/// A scripted backend: first turn calls `get_wo_status`; once it has the tool
/// result it answers, echoing the real (tenant-scoped) count it received.
struct ScriptedBackend;

#[async_trait]
impl LlmBackend for ScriptedBackend {
    async fn turn(
        &self,
        _system: &str,
        messages: &[Message],
        _tools: &[ToolDef],
    ) -> Result<AssistantReply, CopilotError> {
        // If a tool result is already in the transcript, answer from it.
        let last_result = messages.iter().rev().find_map(|m| {
            m.content.iter().find_map(|b| match b {
                Block::ToolResult { content, .. } => Some(content.clone()),
                _ => None,
            })
        });
        if let Some(result) = last_result {
            let data: Value = serde_json::from_str(&result).unwrap_or(json!({}));
            let total = data["total"].as_i64().unwrap_or(0);
            return Ok(AssistantReply {
                content: vec![Block::Text {
                    text: format!("You have {total} work order(s) across your plants."),
                }],
            });
        }
        // Otherwise, call the tool.
        Ok(AssistantReply {
            content: vec![Block::ToolUse {
                id: "call_1".to_string(),
                name: "get_wo_status".to_string(),
                input: json!({}),
            }],
        })
    }
}

#[tokio::test]
async fn copilot_round_trips_a_tool_call_tenant_scoped() {
    let Some(ctx) = setup().await else {
        return;
    };
    seed_part(ctx.pool(), "part_c", "P-C").await;
    let token = seed_tenant(&ctx, "ORGC", "PC", "wo_c", "WO-C-1", "part_c").await;
    // A second org's WO that must NOT leak into org C's answer.
    let _other = seed_tenant(&ctx, "ORGD", "PD", "wo_d", "WO-D-1", "part_c").await;

    let app = ctx.router_with_backend(Arc::new(ScriptedBackend));

    let (status, resp) = call(
        &app,
        "POST",
        "/v1/copilot",
        Some(&token),
        Some(json!({ "message": "How many work orders do we have?" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "resp={resp}");

    // The copilot actually invoked the read-only tool.
    let calls = resp["tool_calls"].as_array().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0]["name"], "get_wo_status");

    // And answered from the tenant-scoped result: exactly one WO (org C's), not two.
    assert_eq!(
        resp["reply"],
        "You have 1 work order(s) across your plants."
    );

    teardown(ctx).await;
}

#[tokio::test]
async fn copilot_degrades_without_a_backend() {
    let Some(ctx) = setup().await else {
        return;
    };
    seed_part(ctx.pool(), "part_e", "P-E").await;
    let token = seed_tenant(&ctx, "ORGE", "PE", "wo_e", "WO-E-1", "part_e").await;

    // Default harness router uses the NullBackend (no model configured).
    let app = ctx.router();
    let (status, _) = call(
        &app,
        "POST",
        "/v1/copilot",
        Some(&token),
        Some(json!({ "message": "hello" })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::SERVICE_UNAVAILABLE,
        "copilot unavailable offline"
    );

    teardown(ctx).await;
}
