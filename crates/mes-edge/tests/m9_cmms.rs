//! M9 acceptance tests — CMMS (§12 M9, §13).
//!
//! Usage-hours PM triggers off simulated run-hours; the maintenance-WO lifecycle
//! advances forward-only; the spare ledger derives stock and a reorder-point
//! breach raises a (single) procurement request. Fresh schema per test, gated on
//! `DATABASE_URL`.

mod common;

use axum::http::StatusCode;
use common::{call, seed_user_token, setup, teardown, Ctx};
use serde_json::{json, Value};

/// Extract a numeric field regardless of whether `rust_decimal` serialised it as
/// a JSON number or string.
fn num(v: &Value) -> f64 {
    match v {
        Value::String(s) => s.parse().expect("numeric string"),
        Value::Number(n) => n.as_f64().expect("f64"),
        other => panic!("not a number: {other}"),
    }
}

/// Seed site → area → work center via the master API; return the work center id.
async fn seed_work_center(app: &axum::Router, planner: &str) -> String {
    let (_, site) = call(
        app,
        "POST",
        "/v1/master/sites",
        Some(planner),
        Some(json!({"code": "S1", "name": "Plant 1"})),
    )
    .await;
    let (_, area) = call(
        app,
        "POST",
        "/v1/master/areas",
        Some(planner),
        Some(json!({"site_id": site["id"], "code": "A1", "name": "Machining"})),
    )
    .await;
    let (status, wc) = call(
        app,
        "POST",
        "/v1/master/work-centers",
        Some(planner),
        Some(json!({"area_id": area["id"], "code": "WC1", "name": "CNC-1"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "wc={wc}");
    wc["id"].as_str().unwrap().to_string()
}

/// Insert a RUNNING machine_states interval of `hours` ending now.
async fn add_run_hours(ctx: &Ctx, wc: &str, hours: i64) {
    let pool = ctx.state.pool.as_ref().unwrap();
    sqlx::query(
        "INSERT INTO machine_states (id, work_center_id, state, start_ts, end_ts)
         VALUES ($1, $2, 'running', now() - ($3 * INTERVAL '1 hour'), now())",
    )
    .bind(mes_core::new_id())
    .bind(wc)
    .bind(hours as f64)
    .execute(pool)
    .await
    .expect("insert machine_states");
}

#[tokio::test]
async fn usage_pm_triggers_off_run_hours() {
    let Some(ctx) = setup().await else {
        return;
    };
    let app = ctx.router();
    let planner = seed_user_token(&ctx, "planner_m9", mes_core::roles::PLANNER).await;
    let maint = seed_user_token(&ctx, "maint_m9", mes_core::roles::MAINTENANCE).await;

    let wc = seed_work_center(&app, &planner).await;

    // Two usage-hours schedules with baseline 0 run-hours: due at 10h and 20h.
    let (status, sched_a) = call(
        &app,
        "POST",
        "/v1/cmms/pm-schedules",
        Some(&maint),
        Some(json!({"work_center_id": wc, "name": "Lube",
                    "trigger_type": "usage_hours", "interval_value": 10})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "sched_a={sched_a}");
    let sched_a_id = sched_a["id"].as_str().unwrap().to_string();

    let (_, sched_b) = call(
        &app,
        "POST",
        "/v1/cmms/pm-schedules",
        Some(&maint),
        Some(json!({"work_center_id": wc, "name": "Overhaul",
                    "trigger_type": "usage_hours", "interval_value": 20})),
    )
    .await;
    let sched_b_id = sched_b["id"].as_str().unwrap().to_string();

    // Accumulate 12 run-hours after the schedules were created.
    add_run_hours(&ctx, &wc, 12).await;

    let (status, due) = call(&app, "GET", "/v1/cmms/pm-schedules/due", Some(&maint), None).await;
    assert_eq!(status, StatusCode::OK);
    let ids: Vec<&str> = due
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["schedule"]["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&sched_a_id.as_str()), "10h schedule is due");
    assert!(
        !ids.contains(&sched_b_id.as_str()),
        "20h schedule not yet due at 12h"
    );

    // The due entry carries the run-hours used to judge it (~12).
    let entry = due
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["schedule"]["id"] == json!(sched_a_id))
        .unwrap();
    assert!((num(&entry["current_usage_h"]) - 12.0).abs() < 0.1);

    teardown(ctx).await;
}

#[tokio::test]
async fn maintenance_wo_lifecycle_forward_only() {
    let Some(ctx) = setup().await else {
        return;
    };
    let app = ctx.router();
    let planner = seed_user_token(&ctx, "planner_m9b", mes_core::roles::PLANNER).await;
    let maint = seed_user_token(&ctx, "maint_m9b", mes_core::roles::MAINTENANCE).await;
    let operator = seed_user_token(&ctx, "op_m9b", mes_core::roles::OPERATOR).await;

    let wc = seed_work_center(&app, &planner).await;

    let (status, wo) = call(
        &app,
        "POST",
        "/v1/cmms/work-orders",
        Some(&maint),
        Some(json!({"work_center_id": wc, "wo_type": "Corrective", "notes": "spindle noise"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "wo={wo}");
    assert_eq!(wo["status"], "requested");
    let wo_id = wo["id"].as_str().unwrap().to_string();

    // Advance one legal step at a time.
    for next in ["scheduled", "in_progress", "completed", "verified"] {
        let (status, body) = call(
            &app,
            "POST",
            &format!("/v1/cmms/work-orders/{wo_id}/transition"),
            Some(&maint),
            Some(json!({"status": next})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "-> {next}: {body}");
        assert_eq!(body["status"], next);
    }

    // A fresh WO cannot skip straight to completed.
    let (_, wo2) = call(
        &app,
        "POST",
        "/v1/cmms/work-orders",
        Some(&maint),
        Some(json!({"work_center_id": wc, "wo_type": "PM"})),
    )
    .await;
    let wo2_id = wo2["id"].as_str().unwrap().to_string();
    let (status, _) = call(
        &app,
        "POST",
        &format!("/v1/cmms/work-orders/{wo2_id}/transition"),
        Some(&maint),
        Some(json!({"status": "completed"})),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "cannot skip requested->completed"
    );

    // Operators cannot manage maintenance WOs.
    let (status, _) = call(
        &app,
        "POST",
        &format!("/v1/cmms/work-orders/{wo2_id}/transition"),
        Some(&operator),
        Some(json!({"status": "scheduled"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    teardown(ctx).await;
}

#[tokio::test]
async fn spare_ledger_and_reorder_point() {
    let Some(ctx) = setup().await else {
        return;
    };
    let app = ctx.router();
    let maint = seed_user_token(&ctx, "maint_m9c", mes_core::roles::MAINTENANCE).await;
    let operator = seed_user_token(&ctx, "op_m9c", mes_core::roles::OPERATOR).await;

    // Reorder when stock <= 5, ordering 20 each time.
    let (status, spare) = call(
        &app,
        "POST",
        "/v1/cmms/spares",
        Some(&maint),
        Some(json!({"code": "BRG-1", "name": "Bearing", "reorder_point": 5, "reorder_qty": 20})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "spare={spare}");
    let spare_id = spare["id"].as_str().unwrap().to_string();

    // Operators cannot create spares.
    let (status, _) = call(
        &app,
        "POST",
        "/v1/cmms/spares",
        Some(&operator),
        Some(json!({"code": "X", "name": "X"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Receive 10 → stock 10, above reorder point, no request.
    let (status, r) = call(
        &app,
        "POST",
        "/v1/cmms/spares/txns",
        Some(&maint),
        Some(json!({"spare_part_id": spare_id, "txn_type": "receive", "qty": 10})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "receive={r}");
    assert!((num(&r["stock"]) - 10.0).abs() < 1e-9);
    assert!(r["procurement_request"].is_null());

    // Issue 6 → stock 4 (<=5) → reorder request raised for 20.
    let (status, r) = call(
        &app,
        "POST",
        "/v1/cmms/spares/txns",
        Some(&maint),
        Some(json!({"spare_part_id": spare_id, "txn_type": "issue", "qty": 6})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "issue={r}");
    assert!((num(&r["stock"]) - 4.0).abs() < 1e-9);
    let req = &r["procurement_request"];
    assert!(req.is_object(), "reorder-point breach raises a request");
    assert_eq!(req["reason"], "reorder_point");
    assert_eq!(req["status"], "requested");
    assert!((num(&req["qty_requested"]) - 20.0).abs() < 1e-9);

    // Derived stock is visible on the spares list.
    let (_, spares) = call(&app, "GET", "/v1/cmms/spares", Some(&maint), None).await;
    let listed = spares
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == json!(spare_id))
        .unwrap();
    assert!((num(&listed["stock"]) - 4.0).abs() < 1e-9);

    // One open request in the queue.
    let (_, queue) = call(&app, "GET", "/v1/cmms/procurement", Some(&maint), None).await;
    assert_eq!(queue.as_array().unwrap().len(), 1);

    // A second breach does not raise a duplicate open request.
    let (status, r) = call(
        &app,
        "POST",
        "/v1/cmms/spares/txns",
        Some(&maint),
        Some(json!({"spare_part_id": spare_id, "txn_type": "issue", "qty": 1})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert!((num(&r["stock"]) - 3.0).abs() < 1e-9);
    assert!(
        r["procurement_request"].is_null(),
        "no duplicate open reorder request"
    );

    let (_, queue) = call(&app, "GET", "/v1/cmms/procurement", Some(&maint), None).await;
    assert_eq!(
        queue.as_array().unwrap().len(),
        1,
        "still exactly one request"
    );

    teardown(ctx).await;
}
