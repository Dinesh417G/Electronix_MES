//! M5 acceptance tests — downtime analytics (§12 M5, §13).
//!
//! Seeds a week of classified downtime and asserts the Pareto ordering,
//! per-row share, and cumulative share match a hand-computed fixture, plus the
//! Six-Big-Losses rollup and the daily trend. Fresh schema per test, gated on
//! `DATABASE_URL`.

mod common;

use axum::http::StatusCode;
use chrono::{DateTime, TimeZone, Utc};
use common::{call, seed_user_token, setup, teardown};
use serde_json::json;

fn at(day: u32, h: u32, m: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 3, day, h, m, 0)
        .single()
        .unwrap()
}

#[tokio::test]
async fn downtime_pareto_matches_fixture() {
    let Some(ctx) = setup().await else {
        return;
    };
    let app = ctx.router();
    let pool = ctx.state.pool.as_ref().unwrap();
    let planner = seed_user_token(&ctx, "planner_m5", mes_core::roles::PLANNER).await;

    // Work center.
    let (_, site) = call(
        &app,
        "POST",
        "/v1/master/sites",
        Some(&planner),
        Some(json!({"code": "S1", "name": "Plant"})),
    )
    .await;
    let (_, area) = call(
        &app,
        "POST",
        "/v1/master/areas",
        Some(&planner),
        Some(json!({"site_id": site["id"], "code": "A1", "name": "M"})),
    )
    .await;
    let (_, wc) = call(
        &app,
        "POST",
        "/v1/master/work-centers",
        Some(&planner),
        Some(json!({"area_id": area["id"], "code": "WC1", "name": "Lathe"})),
    )
    .await;
    let wc_id = wc["id"].as_str().unwrap().to_string();

    // Reasons with Six-Big-Losses mappings.
    let r_break = mes_db::repo_analytics::create_downtime_reason(
        pool,
        "BRK",
        "Breakdown",
        Some("breakdown"),
        None,
    )
    .await
    .unwrap();
    let r_setup = mes_db::repo_analytics::create_downtime_reason(
        pool,
        "SET",
        "Setup",
        Some("setup_adjustment"),
        None,
    )
    .await
    .unwrap();
    let r_minor = mes_db::repo_analytics::create_downtime_reason(
        pool,
        "MIN",
        "Minor stop",
        Some("minor_stop"),
        None,
    )
    .await
    .unwrap();

    // Seed the week (durations chosen for a clean hand-computed Pareto):
    //   Breakdown 1800 + 3600 = 5400s
    //   Setup     1200 + 1200 = 2400s
    //   Minor      300 +  600 =  900s   → classified total 8700s
    //   Unclassified              600s  → excluded from Pareto, in trend
    let seed = |wc: &str, state: &str, s: DateTime<Utc>, e: DateTime<Utc>, r: Option<String>| {
        let wc = wc.to_string();
        let state = state.to_string();
        async move {
            mes_db::repo_analytics::insert_downtime_event(pool, &wc, &state, s, e, r.as_deref())
                .await
                .unwrap();
        }
    };
    seed(
        &wc_id,
        "down",
        at(2, 8, 0),
        at(2, 8, 30),
        Some(r_break.clone()),
    )
    .await;
    seed(
        &wc_id,
        "down",
        at(3, 9, 0),
        at(3, 10, 0),
        Some(r_break.clone()),
    )
    .await;
    seed(
        &wc_id,
        "down",
        at(4, 8, 0),
        at(4, 8, 20),
        Some(r_setup.clone()),
    )
    .await;
    seed(
        &wc_id,
        "down",
        at(5, 10, 0),
        at(5, 10, 20),
        Some(r_setup.clone()),
    )
    .await;
    seed(
        &wc_id,
        "micro_stop",
        at(6, 8, 0),
        at(6, 8, 5),
        Some(r_minor.clone()),
    )
    .await;
    seed(
        &wc_id,
        "micro_stop",
        at(6, 8, 10),
        at(6, 8, 20),
        Some(r_minor.clone()),
    )
    .await;
    seed(&wc_id, "down", at(7, 8, 0), at(7, 8, 10), None).await;

    let range = "start=2026-03-02T00:00:00Z&end=2026-03-09T00:00:00Z";

    // Pareto: ordered Breakdown > Setup > Minor with the fixture's shares.
    let (status, rows) = call(
        &app,
        "GET",
        &format!("/v1/analytics/downtime/pareto?{range}"),
        Some(&planner),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "rows={rows}");
    let rows = rows.as_array().unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0]["key"], "BRK");
    assert_eq!(rows[0]["seconds"], 5400);
    assert_eq!(rows[1]["key"], "SET");
    assert_eq!(rows[1]["seconds"], 2400);
    assert_eq!(rows[2]["key"], "MIN");
    assert_eq!(rows[2]["seconds"], 900);

    let pct0 = rows[0]["pct"].as_f64().unwrap();
    assert!((pct0 - 5400.0 * 100.0 / 8700.0).abs() < 1e-6, "pct0={pct0}");
    let cum_last = rows[2]["cumulative_pct"].as_f64().unwrap();
    assert!(
        (cum_last - 100.0).abs() < 1e-6,
        "cumulative must reach 100, got {cum_last}"
    );
    // Cumulative is monotonic non-decreasing.
    assert!(
        rows[0]["cumulative_pct"].as_f64().unwrap() <= rows[1]["cumulative_pct"].as_f64().unwrap()
    );

    // Six-Big-Losses rollup, ranked.
    let (status, losses) = call(
        &app,
        "GET",
        &format!("/v1/analytics/downtime/six-big-losses?{range}"),
        Some(&planner),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let losses = losses.as_array().unwrap();
    assert_eq!(losses.len(), 3);
    assert_eq!(losses[0]["key"], "breakdown");
    assert_eq!(losses[0]["seconds"], 5400);

    // Trend includes ALL downtime (classified + unclassified) = 9300s total.
    let (status, trend) = call(
        &app,
        "GET",
        &format!("/v1/analytics/downtime/trend?{range}"),
        Some(&planner),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let total: i64 = trend
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["seconds"].as_i64().unwrap())
        .sum();
    assert_eq!(total, 9300);

    teardown(ctx).await;
}
