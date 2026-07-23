//! M12 acceptance tests — cloud + sync (§12 M12, §13).
//!
//! An edge's outbox batch pushes to the cloud and converges; replaying the same
//! batch is a no-op (idempotent apply); a wrong plant token is rejected; a work
//! order created remotely on the cloud is pulled and appears on the edge. Fresh
//! schema per test, gated on `DATABASE_URL`.

mod common;

use axum::http::StatusCode;
use common::{call, seed_part, setup, teardown};
use serde_json::json;

/// Provision an org + enroll a plant; return (plant_id, plant_token).
async fn enroll(app: &axum::Router) -> (String, String) {
    let (_, org) = call(
        app,
        "POST",
        "/v1/sync/orgs",
        None,
        Some(json!({"code": "ORG1", "name": "Acme"})),
    )
    .await;
    let org_id = org["id"].as_str().unwrap().to_string();
    let (status, plant) = call(
        app,
        "POST",
        &format!("/v1/sync/orgs/{org_id}/plants"),
        None,
        Some(json!({"code": "P1", "name": "Plant 1"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "plant={plant}");
    (
        plant["id"].as_str().unwrap().to_string(),
        plant["token"].as_str().unwrap().to_string(),
    )
}

#[tokio::test]
async fn push_converges_and_replay_is_noop() {
    let Some(ctx) = setup().await else {
        return;
    };
    let app = ctx.router();
    let (plant_id, token) = enroll(&app).await;

    // Master data is provisioned before WOs sync.
    seed_part(ctx.pool(), "part_1", "P-1").await;

    // An edge outbox batch: one work-order upsert.
    let entry = json!({
        "id": "entry_1",
        "aggregate": "work_order",
        "entity_id": "wo_1",
        "op": "upsert",
        "payload": {
            "id": "wo_1", "wo_number": "WO-SYNC-1", "part_id": "part_1",
            "qty_ordered": 10, "priority": 100, "status": "released"
        }
    });
    let batch = json!({ "plant_id": plant_id, "entries": [entry] });

    // Wrong token is rejected.
    let (status, _) = call(
        &app,
        "POST",
        "/v1/sync/push",
        Some("wrong"),
        Some(batch.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // First push applies the entry → cloud converges.
    let (status, res) = call(
        &app,
        "POST",
        "/v1/sync/push",
        Some(&token),
        Some(batch.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "res={res}");
    assert_eq!(res["applied"], 1);
    assert_eq!(res["skipped"], 0);

    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM work_orders WHERE id = 'wo_1'")
        .fetch_one(ctx.pool())
        .await
        .unwrap();
    assert_eq!(count, 1, "work order converged on the cloud");

    // Replaying the same batch (e.g. after a 24h outage + reconnect) is a no-op.
    let (status, res) = call(&app, "POST", "/v1/sync/push", Some(&token), Some(batch)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(res["applied"], 0, "duplicate batch applies nothing");
    assert_eq!(res["skipped"], 1);

    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM work_orders")
        .fetch_one(ctx.pool())
        .await
        .unwrap();
    assert_eq!(count, 1, "no duplicate work order");

    // The plant shows up on the multi-plant dashboard with a last-sync stamp.
    let (_, plants) = call(&app, "GET", "/v1/sync/plants", None, None).await;
    let row = &plants.as_array().unwrap()[0];
    assert_eq!(row["enrolled"], true);
    assert!(!row["last_sync_at"].is_null());

    teardown(ctx).await;
}

#[tokio::test]
async fn remote_work_order_is_pulled_to_the_edge() {
    // Two schemas: `ctx` is the cloud; `edge` is the plant applying pulled commands.
    let Some(ctx) = setup().await else {
        return;
    };
    let Some(edge) = setup().await else {
        teardown(ctx).await;
        return;
    };
    let app = ctx.router();
    let (plant_id, token) = enroll(&app).await;

    // The part exists on both sides (master data provisioned first).
    seed_part(ctx.pool(), "part_r", "P-R").await;
    seed_part(edge.pool(), "part_r", "P-R").await;

    // Create a work order remotely, on the cloud, destined for the plant.
    let (status, res) = call(
        &app,
        "POST",
        &format!("/v1/sync/plants/{plant_id}/work-orders"),
        None,
        Some(json!({"wo_number": "WO-REMOTE-1", "part_id": "part_r", "qty_ordered": 25})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "res={res}");

    // The edge pulls its pending commands.
    let (status, pull) = call(
        &app,
        "GET",
        &format!("/v1/sync/pull?plant_id={plant_id}"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "pull={pull}");
    let entries = pull["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 1, "one command to pull");

    // The edge applies the pulled entry locally → the remote WO appears on edge.
    let entry: mes_client::sync::SyncEntry = serde_json::from_value(entries[0].clone()).unwrap();
    let newly = mes_db::repo_sync::apply_entry(edge.pool(), &entry)
        .await
        .unwrap();
    assert!(newly, "entry newly applied on the edge");

    let (found,): (String,) = sqlx::query_as("SELECT wo_number FROM work_orders WHERE id = $1")
        .bind(&entry.entity_id)
        .fetch_one(edge.pool())
        .await
        .unwrap();
    assert_eq!(found, "WO-REMOTE-1", "remote WO appears on the edge");

    // Ack so the cloud stops re-sending; a second pull is then empty.
    let (status, _) = call(
        &app,
        "POST",
        "/v1/sync/ack",
        Some(&token),
        Some(json!({"plant_id": plant_id, "ids": [entry.id]})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, pull2) = call(
        &app,
        "GET",
        &format!("/v1/sync/pull?plant_id={plant_id}"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(
        pull2["entries"].as_array().unwrap().len(),
        0,
        "acked command not re-sent"
    );

    teardown(edge).await;
    teardown(ctx).await;
}
