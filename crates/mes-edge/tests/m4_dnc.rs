//! M4 acceptance tests — DNC orchestration (§12 M4, §13).
//!
//! Drives the flow against a **virtual dnc-daemon** (never real CNC hardware,
//! §13): job-complete → transfer scheduled → simulated ack → event clears; and
//! a simulated edited-program receive → a **draft** revision that is explicitly
//! not auto-promoted, then a supervisor promotes it. Fresh schema per test,
//! gated on `DATABASE_URL`.

mod common;

use std::sync::Arc;

use axum::http::StatusCode;
use common::{call, seed_user_token, setup, teardown};
use mes_dnc_bridge::{DncCommand, VirtualDaemon};
use serde_json::json;

async fn seed_equipment_and_part(app: &axum::Router, token: &str) -> (String, String) {
    let (_, site) = call(
        app,
        "POST",
        "/v1/master/sites",
        Some(token),
        Some(json!({"code": "S1", "name": "Plant"})),
    )
    .await;
    let (_, area) = call(
        app,
        "POST",
        "/v1/master/areas",
        Some(token),
        Some(json!({"site_id": site["id"], "code": "A1", "name": "M"})),
    )
    .await;
    let (_, wc) = call(
        app,
        "POST",
        "/v1/master/work-centers",
        Some(token),
        Some(json!({"area_id": area["id"], "code": "WC1", "name": "Lathe"})),
    )
    .await;
    let (_, part) = call(
        app,
        "POST",
        "/v1/master/parts",
        Some(token),
        Some(json!({"code": "P-1", "name": "Widget"})),
    )
    .await;
    (
        wc["id"].as_str().unwrap().to_string(),
        part["id"].as_str().unwrap().to_string(),
    )
}

#[tokio::test]
async fn dnc_orchestration_flow() {
    let Some(mut ctx) = setup().await else {
        return;
    };
    // Inject a virtual daemon we can inspect.
    let daemon = Arc::new(VirtualDaemon::new());
    ctx.state.dnc = daemon.clone();

    let app = ctx.router();
    let planner = seed_user_token(&ctx, "planner_m4", mes_core::roles::PLANNER).await;
    let operator = seed_user_token(&ctx, "op_m4", mes_core::roles::OPERATOR).await;
    let supervisor = seed_user_token(&ctx, "sup_m4", mes_core::roles::SUPERVISOR).await;

    let (wc_id, part_id) = seed_equipment_and_part(&app, &planner).await;

    // A program for the part, known to the (virtual) daemon as "O1000".
    let (status, _prog) = call(
        &app,
        "POST",
        "/v1/master/programs",
        Some(&planner),
        Some(json!({"part_id": part_id, "program_identifier": "O1000", "target_machine": "CNC-1"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    // Two operations so completing op #10 triggers a transfer for op #20.
    let (_, detail) = call(
        &app,
        "POST",
        "/v1/orders",
        Some(&planner),
        Some(
            json!({"wo_number": "WO-DNC", "part_id": part_id, "qty_ordered": 5,
            "operations": [
                {"op_no": 10, "work_center_id": wc_id},
                {"op_no": 20, "work_center_id": wc_id}
            ]}),
        ),
    )
    .await;
    let wo_id = detail["id"].as_str().unwrap().to_string();
    let op1 = detail["operations"][0]["id"].as_str().unwrap().to_string();

    call(
        &app,
        "POST",
        &format!("/v1/orders/{wo_id}/release"),
        Some(&planner),
        None,
    )
    .await;
    call(
        &app,
        "POST",
        &format!("/v1/exec/operations/{op1}/start"),
        Some(&operator),
        None,
    )
    .await;

    // Completing op #10 auto-schedules the transfer for op #20's program.
    let (status, _op) = call(
        &app,
        "POST",
        &format!("/v1/exec/operations/{op1}/complete"),
        Some(&operator),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // The daemon received a SendProgram command for "O1000".
    let sent = daemon.sent();
    assert_eq!(sent.len(), 1, "one transfer command staged");
    assert_eq!(
        sent[0],
        DncCommand::SendProgram {
            program: "O1000".to_string(),
            machine: Some("CNC-1".to_string()),
        }
    );

    // A Scheduled transfer exists.
    let (status, transfers) = call(&app, "GET", "/v1/dnc/transfers", Some(&operator), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(transfers.as_array().unwrap().len(), 1);
    assert_eq!(transfers[0]["status"], "scheduled");
    let daemon_ref = transfers[0]["dnc_daemon_ref"].as_str().unwrap().to_string();

    // Simulated daemon ack → the transfer clears (Completed).
    let (status, _) = call(
        &app,
        "POST",
        "/v1/dnc/daemon-events",
        Some(&operator),
        Some(json!({"event": "transfer_completed", "reference": daemon_ref})),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);

    let (_, transfers) = call(&app, "GET", "/v1/dnc/transfers", Some(&operator), None).await;
    assert_eq!(transfers[0]["status"], "completed");
    assert!(transfers[0]["completed_at"].is_string());

    // Simulated edited-program receive → a DRAFT revision, NOT auto-promoted.
    let (status, _) = call(
        &app,
        "POST",
        "/v1/dnc/daemon-events",
        Some(&operator),
        Some(json!({"event": "program_received", "program": "O1000", "content_ref": "blob/1"})),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);

    let (status, revs) = call(&app, "GET", "/v1/dnc/revisions", Some(&supervisor), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(revs.as_array().unwrap().len(), 1);
    assert_eq!(
        revs[0]["status"], "draft",
        "revision must NOT be auto-promoted"
    );
    let rev_id = revs[0]["id"].as_str().unwrap().to_string();

    // Operator cannot promote; supervisor can.
    let (status, _) = call(
        &app,
        "POST",
        &format!("/v1/dnc/revisions/{rev_id}/promote"),
        Some(&operator),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, rev) = call(
        &app,
        "POST",
        &format!("/v1/dnc/revisions/{rev_id}/promote"),
        Some(&supervisor),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(rev["status"], "promoted");

    // Re-promoting a promoted revision is an invalid transition.
    let (status, _) = call(
        &app,
        "POST",
        &format!("/v1/dnc/revisions/{rev_id}/promote"),
        Some(&supervisor),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    teardown(ctx).await;
}
