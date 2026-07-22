//! M7 acceptance tests — traceability (§12 M7, §13).
//!
//! Builds a 3-level assembly (raw → sub-assembly → finished good), traces both
//! directions, and confirms a held lot cannot be issued. Fresh schema per test,
//! gated on `DATABASE_URL`.

mod common;

use axum::http::StatusCode;
use common::{call, seed_user_token, setup, teardown};
use serde_json::{json, Value};

/// Create a part and a lot of it; return (part_id, lot_id, lot_no).
async fn make_lot(app: &axum::Router, token: &str, code: &str, lot_no: &str) -> (String, String) {
    let (_, part) = call(
        app,
        "POST",
        "/v1/master/parts",
        Some(token),
        Some(json!({"code": code, "name": code})),
    )
    .await;
    let part_id = part["id"].as_str().unwrap().to_string();
    let (status, lot) = call(
        app,
        "POST",
        "/v1/trace/lots",
        Some(token),
        Some(json!({"lot_no": lot_no, "part_id": part_id, "qty": 10})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "lot={lot}");
    (part_id, lot["id"].as_str().unwrap().to_string())
}

async fn edge(app: &axum::Router, token: &str, parent: &str, child: &str) {
    let (status, _) = call(
        app,
        "POST",
        "/v1/trace/genealogy",
        Some(token),
        Some(json!({
            "parent_type": "lot", "parent_id": parent,
            "child_type": "lot", "child_id": child
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
}

fn ref_nos(nodes: &Value) -> Vec<String> {
    let mut v: Vec<String> = nodes
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["ref_no"].as_str().unwrap().to_string())
        .collect();
    v.sort();
    v
}

#[tokio::test]
async fn three_level_assembly_traces_both_directions() {
    let Some(ctx) = setup().await else {
        return;
    };
    let app = ctx.router();
    let planner = seed_user_token(&ctx, "planner_m7", mes_core::roles::PLANNER).await;

    // Level 0 raw, level 1 sub-assembly, level 2 finished good.
    let (_p_ra, raw_a) = make_lot(&app, &planner, "RAW-A", "L-RAWA").await;
    let (_p_rb, raw_b) = make_lot(&app, &planner, "RAW-B", "L-RAWB").await;
    let (_p_sub, sub) = make_lot(&app, &planner, "SUB", "L-SUB").await;
    let (_p_fg, fg) = make_lot(&app, &planner, "FG", "L-FG").await;

    // Genealogy: FG consumes SUB; SUB consumes RAW-A and RAW-B.
    edge(&app, &planner, &fg, &sub).await;
    edge(&app, &planner, &sub, &raw_a).await;
    edge(&app, &planner, &sub, &raw_b).await;

    // Backward from FG → all components across 3 levels.
    let (status, back) = call(
        &app,
        "GET",
        &format!("/v1/trace/backward/lot/{fg}"),
        Some(&planner),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "back={back}");
    assert_eq!(ref_nos(&back), vec!["L-RAWA", "L-RAWB", "L-SUB"]);
    // SUB is depth 1, raws are depth 2.
    let by_ref: std::collections::HashMap<String, i64> = back
        .as_array()
        .unwrap()
        .iter()
        .map(|n| {
            (
                n["ref_no"].as_str().unwrap().to_string(),
                n["depth"].as_i64().unwrap(),
            )
        })
        .collect();
    assert_eq!(by_ref["L-SUB"], 1);
    assert_eq!(by_ref["L-RAWA"], 2);
    assert_eq!(by_ref["L-RAWB"], 2);

    // Forward from RAW-A → SUB (depth 1) then FG (depth 2).
    let (status, fwd) = call(
        &app,
        "GET",
        &format!("/v1/trace/forward/lot/{raw_a}"),
        Some(&planner),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "fwd={fwd}");
    assert_eq!(ref_nos(&fwd), vec!["L-FG", "L-SUB"]);
    let fwd_depth: std::collections::HashMap<String, i64> = fwd
        .as_array()
        .unwrap()
        .iter()
        .map(|n| {
            (
                n["ref_no"].as_str().unwrap().to_string(),
                n["depth"].as_i64().unwrap(),
            )
        })
        .collect();
    assert_eq!(fwd_depth["L-SUB"], 1);
    assert_eq!(fwd_depth["L-FG"], 2);

    teardown(ctx).await;
}

#[tokio::test]
async fn held_lot_blocks_issue() {
    let Some(ctx) = setup().await else {
        return;
    };
    let app = ctx.router();
    let planner = seed_user_token(&ctx, "planner_m7b", mes_core::roles::PLANNER).await;
    let quality = seed_user_token(&ctx, "qa_m7", mes_core::roles::QUALITY).await;
    let operator = seed_user_token(&ctx, "op_m7", mes_core::roles::OPERATOR).await;

    let (_part, lot) = make_lot(&app, &planner, "RAW", "L-RAW").await;

    // Issuing an un-held lot succeeds.
    let (status, _) = call(
        &app,
        "POST",
        "/v1/trace/material/issue",
        Some(&operator),
        Some(json!({"lot_id": lot, "qty": 2})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    // Operator cannot place a hold (quality action).
    let (status, _) = call(
        &app,
        "POST",
        "/v1/trace/holds",
        Some(&operator),
        Some(json!({"entity_type": "lot", "entity_id": lot, "reason": "suspect"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Quality places the hold.
    let (status, hold) = call(
        &app,
        "POST",
        "/v1/trace/holds",
        Some(&quality),
        Some(json!({"entity_type": "lot", "entity_id": lot, "reason": "suspect"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "hold={hold}");
    let hold_id = hold["hold_id"].as_str().unwrap().to_string();

    // Now issuing the held lot is blocked.
    let (status, body) = call(
        &app,
        "POST",
        "/v1/trace/material/issue",
        Some(&operator),
        Some(json!({"lot_id": lot, "qty": 1})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "body={body}");

    // Release the hold; issue succeeds again.
    let (status, _) = call(
        &app,
        "POST",
        &format!("/v1/trace/holds/{hold_id}/release"),
        Some(&quality),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _) = call(
        &app,
        "POST",
        "/v1/trace/material/issue",
        Some(&operator),
        Some(json!({"lot_id": lot, "qty": 1})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    teardown(ctx).await;
}

#[tokio::test]
async fn barcode_parse_roundtrip() {
    let Some(ctx) = setup().await else {
        return;
    };
    let app = ctx.router();
    let planner = seed_user_token(&ctx, "planner_m7c", mes_core::roles::PLANNER).await;

    let (status, parsed) = call(
        &app,
        "GET",
        "/v1/trace/barcode?code=EMX1%7CLOT%7C01HXYZ",
        Some(&planner),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "parsed={parsed}");
    assert_eq!(parsed["type_code"], "LOT");
    assert_eq!(parsed["id"], "01HXYZ");

    let (status, _) = call(
        &app,
        "GET",
        "/v1/trace/barcode?code=NOPE",
        Some(&planner),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    teardown(ctx).await;
}
