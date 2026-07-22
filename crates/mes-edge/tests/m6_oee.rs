//! M6 acceptance tests — OEE (§12 M6, §13).
//!
//! Golden day: seed a known day and assert A/P/Q/OEE match the hand-computed
//! fixture *and* that the Rust path (`/v1/analytics/oee`) and the SQL path
//! (`repo_oee::oee_sql`) agree within 0.1%. Shift-boundary: OEE by shift splits
//! cleanly at the shift change with the right per-shift numbers. Fresh schema
//! per test, gated on `DATABASE_URL`.

mod common;

use axum::http::StatusCode;
use chrono::{DateTime, NaiveTime, TimeZone, Utc};
use common::{call, seed_user_token, setup, teardown};
use serde_json::json;

fn at(day: u32, h: u32, m: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 3, day, h, m, 0)
        .single()
        .unwrap()
}

async fn seed_wc(app: &axum::Router, token: &str) -> (String, String) {
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
    (
        site["id"].as_str().unwrap().to_string(),
        wc["id"].as_str().unwrap().to_string(),
    )
}

fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.001 // within 0.1%
}

#[tokio::test]
async fn golden_day_rust_and_sql_agree() {
    let Some(ctx) = setup().await else {
        return;
    };
    let app = ctx.router();
    let pool = ctx.state.pool.as_ref().unwrap();
    let planner = seed_user_token(&ctx, "planner_m6", mes_core::roles::PLANNER).await;

    let (_site, wc) = seed_wc(&app, &planner).await;
    mes_db::repo_oee::set_work_center_ideal_cycle(pool, &wc, 20.0)
        .await
        .unwrap();

    // Golden day 08:00–16:00 (planned production = 8h − 30min stop = 27000s):
    //   running 14400 + 7200 = 21600s ; planned_stop 1800s ; down 5400s
    //   counts good 729 / total 810
    // ⇒ A=0.80, P=0.75, Q=0.90, OEE=0.54
    for (s, e, st) in [
        (at(2, 8, 0), at(2, 12, 0), "running"),
        (at(2, 12, 0), at(2, 12, 30), "planned_stop"),
        (at(2, 12, 30), at(2, 14, 30), "running"),
        (at(2, 14, 30), at(2, 16, 0), "down"),
    ] {
        mes_db::repo_oee::insert_machine_state(pool, &wc, st, s, e)
            .await
            .unwrap();
    }
    mes_db::repo_oee::insert_count(pool, &wc, at(2, 10, 0), 729, 81)
        .await
        .unwrap();

    let start = at(2, 8, 0);
    let end = at(2, 16, 0);

    // Rust path via the API.
    let (status, body) = call(
        &app,
        "GET",
        &format!(
            "/v1/analytics/oee?work_center_id={wc}&start=2026-03-02T08:00:00Z&end=2026-03-02T16:00:00Z"
        ),
        Some(&planner),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    let a = body["availability"].as_f64().unwrap();
    let p = body["performance"].as_f64().unwrap();
    let q = body["quality"].as_f64().unwrap();
    let oee = body["oee"].as_f64().unwrap();
    assert!(approx(a, 0.80), "A={a}");
    assert!(approx(p, 0.75), "P={p}");
    assert!(approx(q, 0.90), "Q={q}");
    assert!(approx(oee, 0.54), "OEE={oee}");

    // SQL path must agree within 0.1%.
    let sql = mes_db::repo_oee::oee_sql(pool, &wc, start, end)
        .await
        .unwrap();
    assert!(
        approx(sql.availability, a),
        "sql A {} vs {}",
        sql.availability,
        a
    );
    assert!(
        approx(sql.performance, p),
        "sql P {} vs {}",
        sql.performance,
        p
    );
    assert!(approx(sql.quality, q), "sql Q {} vs {}", sql.quality, q);
    assert!(approx(sql.oee, oee), "sql OEE {} vs {}", sql.oee, oee);

    teardown(ctx).await;
}

#[tokio::test]
async fn oee_by_shift_respects_boundary() {
    let Some(ctx) = setup().await else {
        return;
    };
    let app = ctx.router();
    let pool = ctx.state.pool.as_ref().unwrap();
    let planner = seed_user_token(&ctx, "planner_m6b", mes_core::roles::PLANNER).await;

    let (site, wc) = seed_wc(&app, &planner).await;
    mes_db::repo_oee::set_work_center_ideal_cycle(pool, &wc, 20.0)
        .await
        .unwrap();

    // Two shifts back-to-back.
    mes_db::repo_oee::create_shift(
        pool,
        &site,
        "A",
        NaiveTime::from_hms_opt(8, 0, 0).unwrap(),
        NaiveTime::from_hms_opt(12, 0, 0).unwrap(),
    )
    .await
    .unwrap();
    mes_db::repo_oee::create_shift(
        pool,
        &site,
        "B",
        NaiveTime::from_hms_opt(12, 0, 0).unwrap(),
        NaiveTime::from_hms_opt(16, 0, 0).unwrap(),
    )
    .await
    .unwrap();

    // Both shifts fully running; different counts.
    //   A: running 08–12 (14400s), count good 600 / total 600 ⇒ A=1, P=20*600/14400=0.8333, Q=1
    //   B: running 12–16 (14400s), count good 360 / total 400 ⇒ A=1, P=20*400/14400=0.5556, Q=0.9
    mes_db::repo_oee::insert_machine_state(pool, &wc, "running", at(2, 8, 0), at(2, 12, 0))
        .await
        .unwrap();
    mes_db::repo_oee::insert_machine_state(pool, &wc, "running", at(2, 12, 0), at(2, 16, 0))
        .await
        .unwrap();
    mes_db::repo_oee::insert_count(pool, &wc, at(2, 10, 0), 600, 0)
        .await
        .unwrap();
    mes_db::repo_oee::insert_count(pool, &wc, at(2, 13, 0), 360, 40)
        .await
        .unwrap();

    let (status, rows) = call(
        &app,
        "GET",
        &format!(
            "/v1/analytics/oee/by-shift?work_center_id={wc}&start=2026-03-02T08:00:00Z&end=2026-03-02T16:00:00Z"
        ),
        Some(&planner),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "rows={rows}");
    let rows = rows.as_array().unwrap();
    assert_eq!(rows.len(), 2, "one row per shift");

    // Ordered by shift start; boundary is exactly 12:00 with no overlap.
    assert_eq!(rows[0]["shift_name"], "A");
    assert_eq!(rows[0]["end"], "2026-03-02T12:00:00Z");
    assert_eq!(rows[1]["shift_name"], "B");
    assert_eq!(rows[1]["start"], "2026-03-02T12:00:00Z");

    let a_oee = rows[0]["oee"].as_f64().unwrap();
    let b_oee = rows[1]["oee"].as_f64().unwrap();
    assert!(approx(a_oee, 0.8333333), "shift A OEE={a_oee}");
    assert!(approx(b_oee, 0.5), "shift B OEE={b_oee}");

    teardown(ctx).await;
}
