//! M10 acceptance tests — ERP integration (§12 M10, §13).
//!
//! A fixture "generic ERP" (a spawned REST mock) round-trips a work-order import
//! and a stock-level export purely through a configured field-mapping — pointing
//! at a different external shape is a mapping change only, never a code change.
//! Exporting procurement requests transitions them to SentToErp. Tokens are
//! encrypted at rest and never returned. Fresh schema per test, gated on
//! `DATABASE_URL`.

mod common;

use std::sync::{Arc, Mutex};

use axum::http::StatusCode;
use common::{call, seed_user_token, setup, teardown};
use serde_json::{json, Value};

fn num(v: &Value) -> f64 {
    match v {
        Value::String(s) => s.parse().expect("numeric string"),
        Value::Number(n) => n.as_f64().expect("f64"),
        other => panic!("not a number: {other}"),
    }
}

/// Spawn a mock ERP: it captures each POSTed body and answers with a reference.
async fn start_mock_erp() -> (String, Arc<Mutex<Vec<Value>>>) {
    let captured: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let cap = captured.clone();
    let app = axum::Router::new().route(
        "/erp",
        axum::routing::post(move |axum::Json(body): axum::Json<Value>| {
            let cap = cap.clone();
            async move {
                cap.lock().unwrap().push(body);
                axum::Json(json!({ "reference": "ERP-REF-1" }))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{addr}/erp"), captured)
}

#[tokio::test]
async fn import_work_order_via_mapping_and_remap() {
    let Some(ctx) = setup().await else {
        return;
    };
    let app = ctx.router();
    let planner = seed_user_token(&ctx, "planner_m10", mes_core::roles::PLANNER).await;

    let (_, part) = call(
        &app,
        "POST",
        "/v1/master/parts",
        Some(&planner),
        Some(json!({"code": "P-ERP", "name": "Widget"})),
    )
    .await;
    let part_id = part["id"].as_str().unwrap().to_string();

    // Connection A: the ERP calls its fields OrderNo/Item/Qty.
    let (status, conn) = call(
        &app,
        "POST",
        "/v1/erp/connections",
        Some(&planner),
        Some(json!({
            "name": "Acme ERP", "endpoint_url": "http://unused.local", "direction": "both",
            "auth_token": "secret-token",
            "field_mapping": {"fields": {
                "wo_number": "OrderNo", "part_id": "Item", "qty_ordered": "Qty"
            }}
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "conn={conn}");
    // The token is write-only: never echoed, only presence is reported.
    assert_eq!(conn["has_token"], true);
    assert!(conn.get("auth_token").is_none());
    let conn_id = conn["id"].as_str().unwrap().to_string();

    let (status, res) = call(
        &app,
        "POST",
        "/v1/erp/import",
        Some(&planner),
        Some(json!({
            "connection_id": conn_id, "entity": "work_order",
            "records": [{"OrderNo": "WO-IMP-1", "Item": part_id, "Qty": 5}]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "res={res}");
    assert_eq!(res["imported"], 1);
    let wo_id = res["ids"][0].as_str().unwrap().to_string();

    let (_, wo) = call(
        &app,
        "GET",
        &format!("/v1/orders/{wo_id}"),
        Some(&planner),
        None,
    )
    .await;
    assert_eq!(wo["wo_number"], "WO-IMP-1");
    assert!((num(&wo["qty_ordered"]) - 5.0).abs() < 1e-9);

    // Re-point at a *different* ERP shape (po/material/amount) — mapping change
    // ONLY, no code change — and the same import path still works.
    let (status, _) = call(
        &app,
        "PUT",
        &format!("/v1/erp/connections/{conn_id}"),
        Some(&planner),
        Some(json!({
            "name": "Acme ERP", "endpoint_url": "http://unused.local", "direction": "both",
            "field_mapping": {"fields": {
                "wo_number": "po", "part_id": "material", "qty_ordered": "amount"
            }}
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, res) = call(
        &app,
        "POST",
        "/v1/erp/import",
        Some(&planner),
        Some(json!({
            "connection_id": conn_id, "entity": "work_order",
            "records": [{"po": "WO-IMP-2", "material": part_id, "amount": 3}]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "remap import={res}");
    assert_eq!(res["imported"], 1);

    // Both imports are logged.
    let (_, log) = call(&app, "GET", "/v1/erp/sync-log", Some(&planner), None).await;
    let imports = log
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["direction"] == "import" && e["status"] == "success")
        .count();
    assert_eq!(imports, 2);

    teardown(ctx).await;
}

#[tokio::test]
async fn export_stock_level_pushes_mapped_payload() {
    let Some(ctx) = setup().await else {
        return;
    };
    let app = ctx.router();
    let planner = seed_user_token(&ctx, "planner_m10b", mes_core::roles::PLANNER).await;
    let maint = seed_user_token(&ctx, "maint_m10b", mes_core::roles::MAINTENANCE).await;

    let (mock_url, captured) = start_mock_erp().await;

    // A spare with some stock.
    let (_, spare) = call(
        &app,
        "POST",
        "/v1/cmms/spares",
        Some(&maint),
        Some(json!({"code": "BRG-9", "name": "Bearing"})),
    )
    .await;
    let spare_id = spare["id"].as_str().unwrap().to_string();
    call(
        &app,
        "POST",
        "/v1/cmms/spares/txns",
        Some(&maint),
        Some(json!({"spare_part_id": spare_id, "txn_type": "receive", "qty": 7})),
    )
    .await;

    // ERP wants sku/on_hand.
    let (_, conn) = call(
        &app,
        "POST",
        "/v1/erp/connections",
        Some(&planner),
        Some(json!({
            "name": "Stock ERP", "endpoint_url": mock_url, "direction": "export",
            "field_mapping": {"fields": {"code": "sku", "stock": "on_hand"}}
        })),
    )
    .await;
    let conn_id = conn["id"].as_str().unwrap().to_string();

    let (status, res) = call(
        &app,
        "POST",
        "/v1/erp/export",
        Some(&planner),
        Some(json!({"connection_id": conn_id, "entity": "stock_level"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "res={res}");
    assert_eq!(res["pushed"], true);
    let rec = &res["payload"][0];
    assert_eq!(rec["sku"], "BRG-9", "mapped to the ERP's field names");
    assert!((num(&rec["on_hand"]) - 7.0).abs() < 1e-9);

    // The mock actually received the mapped payload over HTTP.
    let got = captured.lock().unwrap().clone();
    assert_eq!(got.len(), 1, "mock received one export POST");
    assert_eq!(got[0][0]["sku"], "BRG-9");

    teardown(ctx).await;
}

#[tokio::test]
async fn export_procurement_marks_sent_to_erp() {
    let Some(ctx) = setup().await else {
        return;
    };
    let app = ctx.router();
    let planner = seed_user_token(&ctx, "planner_m10c", mes_core::roles::PLANNER).await;
    let maint = seed_user_token(&ctx, "maint_m10c", mes_core::roles::MAINTENANCE).await;
    let operator = seed_user_token(&ctx, "op_m10c", mes_core::roles::OPERATOR).await;

    let (mock_url, _captured) = start_mock_erp().await;

    // Create a spare and breach its reorder point → one requested procurement.
    let (_, spare) = call(
        &app,
        "POST",
        "/v1/cmms/spares",
        Some(&maint),
        Some(json!({"code": "BRG-R", "name": "Bearing", "reorder_point": 5, "reorder_qty": 20})),
    )
    .await;
    let spare_id = spare["id"].as_str().unwrap().to_string();
    call(
        &app,
        "POST",
        "/v1/cmms/spares/txns",
        Some(&maint),
        Some(json!({"spare_part_id": spare_id, "txn_type": "receive", "qty": 10})),
    )
    .await;
    call(
        &app,
        "POST",
        "/v1/cmms/spares/txns",
        Some(&maint),
        Some(json!({"spare_part_id": spare_id, "txn_type": "issue", "qty": 6})),
    )
    .await;

    let (_, conn) = call(
        &app,
        "POST",
        "/v1/erp/connections",
        Some(&planner),
        Some(json!({
            "name": "Proc ERP", "endpoint_url": mock_url, "direction": "both",
            "field_mapping": {"fields": {"request_id": "ref", "qty_requested": "qty"}}
        })),
    )
    .await;
    let conn_id = conn["id"].as_str().unwrap().to_string();

    // Operators cannot configure/sync ERP.
    let (status, _) = call(
        &app,
        "POST",
        "/v1/erp/export",
        Some(&operator),
        Some(json!({"connection_id": conn_id, "entity": "procurement_request"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, res) = call(
        &app,
        "POST",
        "/v1/erp/export",
        Some(&planner),
        Some(json!({"connection_id": conn_id, "entity": "procurement_request"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "res={res}");
    assert_eq!(res["pushed"], true);
    assert_eq!(res["record_count"], 1);

    // The request is now SentToErp with the ERP's reference recorded.
    let (_, queue) = call(&app, "GET", "/v1/cmms/procurement", Some(&maint), None).await;
    let reqrow = &queue.as_array().unwrap()[0];
    assert_eq!(reqrow["status"], "sent_to_erp");
    assert_eq!(reqrow["erp_reference"], "ERP-REF-1");
    assert!(!reqrow["pushed_at"].is_null());

    teardown(ctx).await;
}
