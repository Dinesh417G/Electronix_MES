//! M8 acceptance tests — QMS (§12 M8, §13).
//!
//! A failed inspection auto-raises an NCR and places a hold that blocks issue; a
//! Rework disposition releases the hold; Quality-role gating is enforced. Fresh
//! schema per test, gated on `DATABASE_URL`.

mod common;

use axum::http::StatusCode;
use common::{call, seed_user_token, setup, teardown};
use serde_json::{json, Value};

/// Seed a part + a lot + an inspection plan with one characteristic
/// (limits 9.5–10.5). Returns (part_id, lot_id, characteristic_id).
async fn seed_qms(app: &axum::Router, planner: &str, quality: &str) -> (String, String, String) {
    let (_, part) = call(
        app,
        "POST",
        "/v1/master/parts",
        Some(planner),
        Some(json!({"code": "P-1", "name": "Widget"})),
    )
    .await;
    let part_id = part["id"].as_str().unwrap().to_string();

    let (_, lot) = call(
        app,
        "POST",
        "/v1/trace/lots",
        Some(planner),
        Some(json!({"lot_no": "L-1", "part_id": part_id, "qty": 10})),
    )
    .await;
    let lot_id = lot["id"].as_str().unwrap().to_string();

    let (status, plan) = call(
        app,
        "POST",
        "/v1/qms/plans",
        Some(quality),
        Some(json!({"part_id": part_id, "code": "PLAN-1", "name": "Dim check"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "plan={plan}");

    let (_, ch) = call(
        app,
        "POST",
        "/v1/qms/characteristics",
        Some(quality),
        Some(json!({"plan_id": plan["id"], "name": "Length", "uom": "mm",
                    "nominal": "10.0", "lower_limit": "9.5", "upper_limit": "10.5"})),
    )
    .await;
    (part_id, lot_id, ch["id"].as_str().unwrap().to_string())
}

async fn record(app: &axum::Router, token: &str, ch: &str, lot: &str, value: &str) -> Value {
    let (status, body) = call(
        app,
        "POST",
        "/v1/qms/results",
        Some(token),
        Some(json!({"characteristic_id": ch, "lot_id": lot, "measured_value": value})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body={body}");
    body
}

#[tokio::test]
async fn fail_raises_ncr_and_hold_rework_releases() {
    let Some(ctx) = setup().await else {
        return;
    };
    let app = ctx.router();
    let planner = seed_user_token(&ctx, "planner_m8", mes_core::roles::PLANNER).await;
    let quality = seed_user_token(&ctx, "qa_m8", mes_core::roles::QUALITY).await;
    let operator = seed_user_token(&ctx, "op_m8", mes_core::roles::OPERATOR).await;

    let (_part, lot, ch) = seed_qms(&app, &planner, &quality).await;

    // A passing measurement → no NCR.
    let pass = record(&app, &quality, &ch, &lot, "10.0").await;
    assert_eq!(pass["result"]["result"], "pass");
    assert!(pass["ncr"].is_null(), "no NCR on pass");

    // A failing measurement → NCR raised + hold placed.
    let fail = record(&app, &quality, &ch, &lot, "12.0").await;
    assert_eq!(fail["result"]["result"], "fail");
    let ncr = &fail["ncr"];
    assert!(ncr.is_object(), "NCR auto-raised on fail");
    assert_eq!(ncr["status"], "open");
    let ncr_id = ncr["id"].as_str().unwrap().to_string();

    // The hold blocks issue of the lot.
    let (status, _) = call(
        &app,
        "POST",
        "/v1/trace/material/issue",
        Some(&operator),
        Some(json!({"lot_id": lot, "qty": 1})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "held lot must block issue");

    // Operator cannot disposition (quality-gated).
    let (status, _) = call(
        &app,
        "POST",
        &format!("/v1/qms/ncrs/{ncr_id}/disposition"),
        Some(&operator),
        Some(json!({"disposition": "rework"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Quality dispositions as Rework → releases the hold.
    let (status, disp) = call(
        &app,
        "POST",
        &format!("/v1/qms/ncrs/{ncr_id}/disposition"),
        Some(&quality),
        Some(json!({"disposition": "rework"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "disp={disp}");
    assert_eq!(disp["status"], "dispositioned");
    assert_eq!(disp["disposition"], "rework");

    // Issue now succeeds — the Rework disposition released the hold.
    let (status, _) = call(
        &app,
        "POST",
        "/v1/trace/material/issue",
        Some(&operator),
        Some(json!({"lot_id": lot, "qty": 1})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "rework must release the hold");

    teardown(ctx).await;
}

#[tokio::test]
async fn scrap_disposition_keeps_hold() {
    let Some(ctx) = setup().await else {
        return;
    };
    let app = ctx.router();
    let planner = seed_user_token(&ctx, "planner_m8b", mes_core::roles::PLANNER).await;
    let quality = seed_user_token(&ctx, "qa_m8b", mes_core::roles::QUALITY).await;
    let operator = seed_user_token(&ctx, "op_m8b", mes_core::roles::OPERATOR).await;

    let (_part, lot, ch) = seed_qms(&app, &planner, &quality).await;
    let fail = record(&app, &quality, &ch, &lot, "1.0").await;
    let ncr_id = fail["ncr"]["id"].as_str().unwrap().to_string();

    let (status, _) = call(
        &app,
        "POST",
        &format!("/v1/qms/ncrs/{ncr_id}/disposition"),
        Some(&quality),
        Some(json!({"disposition": "scrap"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Scrap keeps the hold → issue still blocked.
    let (status, _) = call(
        &app,
        "POST",
        "/v1/trace/material/issue",
        Some(&operator),
        Some(json!({"lot_id": lot, "qty": 1})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "scrap keeps the hold");

    teardown(ctx).await;
}
