//! M2 acceptance tests — ingestion + state machine end-to-end (§12 M2, §13).
//!
//! A scripted hour of cycle signals is pushed through the real `/v1/ingest`
//! pipeline (via the `SimSource`) and recomputed; the derived `machine_states`
//! must match the golden run→micro-stop→down→run sequence, and a signal from an
//! unregistered source must be dropped, not persisted. Fresh schema per test,
//! gated on `DATABASE_URL` (runs in CI, skipped locally).

mod common;

use axum::http::StatusCode;
use chrono::{DateTime, TimeZone, Utc};
use common::{call, seed_user_token, setup, teardown};
use mes_ingest::sim::SimSource;
use mes_ingest::SignalSource;
use serde_json::json;

fn ts(h: u32, m: u32, s: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, 1, h, m, s).single().unwrap()
}

/// Build the M2 scripted hour: run 10:00–10:20, micro-stop to 10:23, run to
/// 10:40, down to 10:50, run to 11:00. Returns the signals as JSON values.
async fn scripted_hour_signals(source_key: &str) -> Vec<serde_json::Value> {
    let mut src = SimSource::new(
        "sim",
        [
            SimSource::cycle_run(source_key, ts(10, 0, 0), ts(10, 20, 0), 30),
            SimSource::cycle_run(source_key, ts(10, 23, 0), ts(10, 40, 0), 30),
            SimSource::cycle_run(source_key, ts(10, 50, 0), ts(11, 0, 0), 30),
        ]
        .concat(),
    );
    let signals = src.poll().await.unwrap();
    signals
        .into_iter()
        .map(|s| serde_json::to_value(s).unwrap())
        .collect()
}

#[tokio::test]
async fn scripted_hour_matches_golden_states() {
    let Some(ctx) = setup().await else {
        return;
    };
    let app = ctx.router();
    let pool = ctx.state.pool.as_ref().unwrap();
    let admin = seed_user_token(&ctx, "admin_m2", mes_core::roles::ADMIN).await;

    // Master data: site → area → work center, and a registered cycle source.
    let (_, site) = call(
        &app,
        "POST",
        "/v1/master/sites",
        Some(&admin),
        Some(json!({"code": "S1", "name": "Plant"})),
    )
    .await;
    let (_, area) = call(
        &app,
        "POST",
        "/v1/master/areas",
        Some(&admin),
        Some(json!({"site_id": site["id"], "code": "A1", "name": "Machining"})),
    )
    .await;
    let (_, wc) = call(
        &app,
        "POST",
        "/v1/master/work-centers",
        Some(&admin),
        Some(json!({"area_id": area["id"], "code": "WC1", "name": "Lathe"})),
    )
    .await;
    let wc_id = wc["id"].as_str().unwrap().to_string();

    let source = mes_db::repo_ingest::create_signal_source(pool, &wc_id, "dev-1", "cycle", true)
        .await
        .unwrap();
    assert_eq!(source.work_center_id, wc_id);

    // Ingest the scripted hour.
    let signals = scripted_hour_signals("dev-1").await;
    let (status, body) = call(
        &app,
        "POST",
        "/v1/ingest/signals",
        Some(&admin),
        Some(serde_json::Value::Array(signals.clone())),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["accepted"], signals.len());
    assert_eq!(body["dropped"], 0);

    // Recompute derived state over the hour.
    let (status, body) = call(
        &app,
        "POST",
        "/v1/ingest/recompute",
        Some(&admin),
        Some(json!({
            "work_center_id": wc_id,
            "start": "2026-01-01T10:00:00Z",
            "end": "2026-01-01T11:00:00Z"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["states"], 5);
    assert_eq!(body["downtime"], 2);

    // Verify the persisted states match the golden sequence exactly.
    let states = mes_db::repo_ingest::list_machine_states(pool, &wc_id, ts(10, 0, 0), ts(11, 0, 0))
        .await
        .unwrap();
    let got: Vec<(String, DateTime<Utc>, DateTime<Utc>)> = states;
    let expected = vec![
        ("running".to_string(), ts(10, 0, 0), ts(10, 20, 0)),
        ("micro_stop".to_string(), ts(10, 20, 0), ts(10, 23, 0)),
        ("running".to_string(), ts(10, 23, 0), ts(10, 40, 0)),
        ("down".to_string(), ts(10, 40, 0), ts(10, 50, 0)),
        ("running".to_string(), ts(10, 50, 0), ts(11, 0, 0)),
    ];
    assert_eq!(got, expected);

    // Recompute again → idempotent (still 5 states, not 10).
    let _ = call(
        &app,
        "POST",
        "/v1/ingest/recompute",
        Some(&admin),
        Some(json!({
            "work_center_id": wc_id,
            "start": "2026-01-01T10:00:00Z",
            "end": "2026-01-01T11:00:00Z"
        })),
    )
    .await;
    let again = mes_db::repo_ingest::list_machine_states(pool, &wc_id, ts(10, 0, 0), ts(11, 0, 0))
        .await
        .unwrap();
    assert_eq!(again.len(), 5, "recompute is idempotent");

    teardown(ctx).await;
}

#[tokio::test]
async fn unknown_source_is_dropped() {
    let Some(ctx) = setup().await else {
        return;
    };
    let app = ctx.router();
    let pool = ctx.state.pool.as_ref().unwrap();
    let admin = seed_user_token(&ctx, "admin_m2b", mes_core::roles::ADMIN).await;

    // No source registered for "ghost". Two signals, both should be dropped and
    // ingest must not error (§9).
    let (status, body) = call(
        &app,
        "POST",
        "/v1/ingest/signals",
        Some(&admin),
        Some(json!([
            {"source_key": "ghost", "ts": "2026-01-01T10:00:00Z", "type": "cycle"},
            {"source_key": "ghost", "ts": "2026-01-01T10:00:30Z", "type": "cycle"}
        ])),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["accepted"], 0);
    assert_eq!(body["dropped"], 2);

    // Nothing was persisted.
    let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM machine_events")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(n, 0);

    teardown(ctx).await;
}

#[tokio::test]
async fn disabled_source_is_dropped() {
    let Some(ctx) = setup().await else {
        return;
    };
    let app = ctx.router();
    let pool = ctx.state.pool.as_ref().unwrap();
    let admin = seed_user_token(&ctx, "admin_m2c", mes_core::roles::ADMIN).await;

    let (_, site) = call(
        &app,
        "POST",
        "/v1/master/sites",
        Some(&admin),
        Some(json!({"code": "S1", "name": "Plant"})),
    )
    .await;
    let (_, area) = call(
        &app,
        "POST",
        "/v1/master/areas",
        Some(&admin),
        Some(json!({"site_id": site["id"], "code": "A1", "name": "M"})),
    )
    .await;
    let (_, wc) = call(
        &app,
        "POST",
        "/v1/master/work-centers",
        Some(&admin),
        Some(json!({"area_id": area["id"], "code": "WC1", "name": "Lathe"})),
    )
    .await;
    let wc_id = wc["id"].as_str().unwrap().to_string();

    // Registered but disabled → signals dropped.
    mes_db::repo_ingest::create_signal_source(pool, &wc_id, "dev-off", "cycle", false)
        .await
        .unwrap();

    let (status, body) = call(
        &app,
        "POST",
        "/v1/ingest/signals",
        Some(&admin),
        Some(json!([
            {"source_key": "dev-off", "ts": "2026-01-01T10:00:00Z", "type": "cycle"}
        ])),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["accepted"], 0);
    assert_eq!(body["dropped"], 1);

    teardown(ctx).await;
}
