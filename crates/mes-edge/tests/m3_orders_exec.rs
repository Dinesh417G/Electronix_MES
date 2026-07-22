//! M3 acceptance tests — work orders + execution (§12 M3, §13).
//!
//! Drives a full simulated order start→complete over the API and asserts the
//! derived state (statuses, counts, scrap-reason enforcement, transition
//! guards). A second test connects a real WebSocket client to a served instance
//! and observes the live execution events. Fresh schema per test, gated on
//! `DATABASE_URL`.

mod common;

use axum::http::StatusCode;
use common::{call, seed_user_token, setup, teardown};
use futures_util::StreamExt;
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::Message;

/// Seed site→area→work-center and a part; return (work_center_id, part_id).
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
async fn full_order_lifecycle() {
    let Some(ctx) = setup().await else {
        return;
    };
    let app = ctx.router();
    let pool = ctx.state.pool.as_ref().unwrap();
    let planner = seed_user_token(&ctx, "planner1", mes_core::roles::PLANNER).await;
    let operator = seed_user_token(&ctx, "op1", mes_core::roles::OPERATOR).await;

    let (wc_id, part_id) = seed_equipment_and_part(&app, &planner).await;
    let scrap_reason = mes_db::repo_orders::create_scrap_reason(pool, "BURR", "Burr")
        .await
        .unwrap();

    // Operator cannot create a work order (master-write role required).
    let (status, _) = call(
        &app,
        "POST",
        "/v1/orders",
        Some(&operator),
        Some(
            json!({"wo_number": "WO-1", "part_id": part_id, "qty_ordered": 10,
                    "operations": [{"op_no": 10, "work_center_id": wc_id}]}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Planner creates the order with one operation.
    let (status, detail) = call(
        &app,
        "POST",
        "/v1/orders",
        Some(&planner),
        Some(
            json!({"wo_number": "WO-1", "part_id": part_id, "qty_ordered": 10,
                    "operations": [{"op_no": 10, "work_center_id": wc_id}]}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "detail={detail}");
    let wo_id = detail["id"].as_str().unwrap().to_string();
    let op_id = detail["operations"][0]["id"].as_str().unwrap().to_string();
    assert_eq!(detail["status"], "draft");

    // Cannot start an operation before the order is released (WO still draft →
    // op is pending; starting the op is allowed, but the WO stays released-gated).
    // Release first.
    let (status, wo) = call(
        &app,
        "POST",
        &format!("/v1/orders/{wo_id}/release"),
        Some(&planner),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "wo={wo}");
    assert_eq!(wo["status"], "released");

    // Invalid transition: releasing again → 409.
    let (status, _) = call(
        &app,
        "POST",
        &format!("/v1/orders/{wo_id}/release"),
        Some(&planner),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    // Operator starts the operation → WO auto-advances to in_progress.
    let (status, op) = call(
        &app,
        "POST",
        &format!("/v1/exec/operations/{op_id}/start"),
        Some(&operator),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "op={op}");
    assert_eq!(op["status"], "in_progress");

    let (_, detail) = call(
        &app,
        "GET",
        &format!("/v1/orders/{wo_id}"),
        Some(&operator),
        None,
    )
    .await;
    assert_eq!(detail["status"], "in_progress", "WO advanced on op start");

    // Scrap without a reason → 400.
    let (status, body) = call(
        &app,
        "POST",
        &format!("/v1/exec/operations/{op_id}/count"),
        Some(&operator),
        Some(json!({"good": 0, "scrap": 1})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "scrap_reason_required");

    // Good count.
    let (status, op) = call(
        &app,
        "POST",
        &format!("/v1/exec/operations/{op_id}/count"),
        Some(&operator),
        Some(json!({"good": 8, "scrap": 0})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(op["qty_good"], 8);

    // Scrap with a reason.
    let (status, op) = call(
        &app,
        "POST",
        &format!("/v1/exec/operations/{op_id}/count"),
        Some(&operator),
        Some(json!({"good": 1, "scrap": 1, "scrap_reason_id": scrap_reason})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(op["qty_good"], 9);
    assert_eq!(op["qty_scrap"], 1);

    // Complete the operation, then the work order.
    let (status, op) = call(
        &app,
        "POST",
        &format!("/v1/exec/operations/{op_id}/complete"),
        Some(&operator),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(op["status"], "completed");

    let (status, wo) = call(
        &app,
        "POST",
        &format!("/v1/exec/work-orders/{wo_id}/complete"),
        Some(&operator),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(wo["status"], "completed");

    // Planner closes it.
    let (status, wo) = call(
        &app,
        "POST",
        &format!("/v1/orders/{wo_id}/close"),
        Some(&planner),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(wo["status"], "closed");

    // The production_counts ledger holds two rows for this operation.
    let (n,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM production_counts WHERE wo_operation_id = $1")
            .bind(&op_id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(n, 2);

    teardown(ctx).await;
}

#[tokio::test]
async fn ws_client_observes_execution_events() {
    let Some(ctx) = setup().await else {
        return;
    };
    let planner = seed_user_token(&ctx, "planner_ws", mes_core::roles::PLANNER).await;

    // Serve a real instance so a genuine WebSocket client can connect. Both the
    // served router and the oneshot router below share the SAME AppState — hence
    // the same broadcast bus — so HTTP-published events reach the WS client.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let served = mes_edge::http::router(ctx.state.clone());
    tokio::spawn(async move {
        let _ = axum::serve(listener, served).await;
    });

    let (ws_stream, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
        .await
        .expect("ws connect");
    let (_write, mut read) = ws_stream.split();

    // Give the server a moment to finish the upgrade + subscribe before we act.
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    let app = ctx.router();
    let (wc_id, part_id) = seed_equipment_and_part(&app, &planner).await;

    let (_, detail) = call(
        &app,
        "POST",
        "/v1/orders",
        Some(&planner),
        Some(
            json!({"wo_number": "WO-WS", "part_id": part_id, "qty_ordered": 5,
                    "operations": [{"op_no": 10, "work_center_id": wc_id}]}),
        ),
    )
    .await;
    let wo_id = detail["id"].as_str().unwrap().to_string();
    let op_id = detail["operations"][0]["id"].as_str().unwrap().to_string();

    // These actions each publish a WS event.
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
        &format!("/v1/exec/operations/{op_id}/start"),
        Some(&planner),
        None,
    )
    .await;
    call(
        &app,
        "POST",
        &format!("/v1/exec/operations/{op_id}/count"),
        Some(&planner),
        Some(json!({"good": 5, "scrap": 0})),
    )
    .await;
    call(
        &app,
        "POST",
        &format!("/v1/exec/operations/{op_id}/complete"),
        Some(&planner),
        None,
    )
    .await;

    // Collect events until we've seen the ones we expect, or time out.
    let mut seen: Vec<String> = Vec::new();
    let deadline = std::time::Duration::from_secs(3);
    let _ = tokio::time::timeout(deadline, async {
        while let Some(Ok(msg)) = read.next().await {
            if let Message::Text(txt) = msg {
                if let Ok(v) = serde_json::from_str::<Value>(&txt) {
                    if let Some(ev) = v["event"].as_str() {
                        seen.push(ev.to_string());
                    }
                }
            }
            if seen.iter().any(|e| e == "count_recorded")
                && seen.iter().any(|e| e == "operation_completed")
            {
                break;
            }
        }
    })
    .await;

    assert!(
        seen.contains(&"work_order_status_changed".to_string()),
        "seen={seen:?}"
    );
    assert!(
        seen.contains(&"operation_started".to_string()),
        "seen={seen:?}"
    );
    assert!(
        seen.contains(&"count_recorded".to_string()),
        "seen={seen:?}"
    );
    assert!(
        seen.contains(&"operation_completed".to_string()),
        "seen={seen:?}"
    );

    teardown(ctx).await;
}
