//! M6 repositories — OEE inputs (Rust path), a pure-SQL OEE (SQL path), and a
//! per-shift breakdown. The two paths read the same raw data and must agree
//! within 0.1% (§12 M6); the SQL query mirrors `mes_core::oee`'s clamping/cap.

use chrono::{DateTime, Duration, NaiveTime, Utc};
use mes_core::oee::{OeeInputs, OeeResult};
use sqlx::PgPool;

use crate::repo::{RepoError, RepoResult};

fn map_sqlx(e: sqlx::Error) -> RepoError {
    match e {
        sqlx::Error::RowNotFound => RepoError::NotFound,
        other => RepoError::Db(other),
    }
}

/// Fetch the raw scalar inputs for an OEE calculation over `[start, end)`.
pub async fn oee_inputs(
    pool: &PgPool,
    work_center_id: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> RepoResult<OeeInputs> {
    // Running / planned-stop seconds, clamping intervals to the window.
    let (run_s, planned_s): (f64, f64) = sqlx::query_as(
        "SELECT
             COALESCE(SUM(EXTRACT(EPOCH FROM (LEAST(end_ts,$3) - GREATEST(start_ts,$2))))
                      FILTER (WHERE state = 'running'), 0)::float8,
             COALESCE(SUM(EXTRACT(EPOCH FROM (LEAST(end_ts,$3) - GREATEST(start_ts,$2))))
                      FILTER (WHERE state = 'planned_stop'), 0)::float8
         FROM machine_states
         WHERE work_center_id = $1 AND start_ts < $3 AND end_ts > $2",
    )
    .bind(work_center_id)
    .bind(start)
    .bind(end)
    .fetch_one(pool)
    .await
    .map_err(map_sqlx)?;

    let (good, total): (f64, f64) = sqlx::query_as(
        "SELECT COALESCE(SUM(good),0)::float8, COALESCE(SUM(good + scrap),0)::float8
         FROM production_counts
         WHERE work_center_id = $1 AND ts >= $2 AND ts < $3",
    )
    .bind(work_center_id)
    .bind(start)
    .bind(end)
    .fetch_one(pool)
    .await
    .map_err(map_sqlx)?;

    let ideal: f64 = sqlx::query_as::<_, (Option<f64>,)>(
        "SELECT ideal_cycle_seconds::float8 FROM work_centers WHERE id = $1",
    )
    .bind(work_center_id)
    .fetch_optional(pool)
    .await
    .map_err(map_sqlx)?
    .and_then(|(v,)| v)
    .unwrap_or(0.0);

    let window_s = (end - start).num_milliseconds() as f64 / 1000.0;

    Ok(OeeInputs {
        planned_production_s: window_s - planned_s,
        run_s,
        ideal_cycle_s: ideal,
        total_count: total,
        good_count: good,
    })
}

/// Compute OEE entirely in SQL over `[start, end)` — the cross-check path.
pub async fn oee_sql(
    pool: &PgPool,
    work_center_id: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> RepoResult<OeeResult> {
    let (availability, performance, quality, oee): (f64, f64, f64, f64) = sqlx::query_as(
        "WITH s AS (
             SELECT
                 COALESCE(SUM(EXTRACT(EPOCH FROM (LEAST(end_ts,$3) - GREATEST(start_ts,$2))))
                          FILTER (WHERE state = 'running'), 0)::float8 AS run_s,
                 COALESCE(SUM(EXTRACT(EPOCH FROM (LEAST(end_ts,$3) - GREATEST(start_ts,$2))))
                          FILTER (WHERE state = 'planned_stop'), 0)::float8 AS planned_s
             FROM machine_states
             WHERE work_center_id = $1 AND start_ts < $3 AND end_ts > $2
         ),
         c AS (
             SELECT COALESCE(SUM(good),0)::float8 AS good,
                    COALESCE(SUM(good + scrap),0)::float8 AS total
             FROM production_counts
             WHERE work_center_id = $1 AND ts >= $2 AND ts < $3
         ),
         w AS (
             SELECT COALESCE(ideal_cycle_seconds, 0)::float8 AS ideal
             FROM work_centers WHERE id = $1
         ),
         p AS (
             SELECT s.run_s,
                    EXTRACT(EPOCH FROM ($3::timestamptz - $2::timestamptz)) - s.planned_s AS planned_prod,
                    c.good, c.total, w.ideal
             FROM s, c, w
         )
         SELECT
             CASE WHEN planned_prod > 0 THEN run_s / planned_prod ELSE 0 END AS availability,
             LEAST(CASE WHEN run_s > 0 THEN (ideal * total) / run_s ELSE 0 END, 1.0) AS performance,
             CASE WHEN total > 0 THEN good / total ELSE 0 END AS quality,
             (CASE WHEN planned_prod > 0 THEN run_s / planned_prod ELSE 0 END)
               * LEAST(CASE WHEN run_s > 0 THEN (ideal * total) / run_s ELSE 0 END, 1.0)
               * (CASE WHEN total > 0 THEN good / total ELSE 0 END) AS oee
         FROM p",
    )
    .bind(work_center_id)
    .bind(start)
    .bind(end)
    .fetch_one(pool)
    .await
    .map_err(map_sqlx)?;

    Ok(OeeResult {
        availability,
        performance,
        quality,
        oee,
    })
}

/// A per-shift OEE row.
#[derive(Debug, Clone)]
pub struct ShiftOee {
    pub shift_name: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub result: OeeResult,
}

/// Break OEE down by the work center's site shifts across `[start, end)`.
/// Each shift occurrence per day is clamped to the window; empty slices are
/// skipped. Overnight shifts (end ≤ start) roll into the next day.
pub async fn oee_by_shift(
    pool: &PgPool,
    work_center_id: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> RepoResult<Vec<ShiftOee>> {
    let shifts: Vec<(String, NaiveTime, NaiveTime)> = sqlx::query_as(
        "SELECT sh.name, sh.start_time, sh.end_time
         FROM shifts sh
         JOIN sites si ON si.id = sh.site_id
         JOIN areas a ON a.site_id = si.id
         JOIN work_centers w ON w.area_id = a.id
         WHERE w.id = $1
         ORDER BY sh.start_time",
    )
    .bind(work_center_id)
    .fetch_all(pool)
    .await
    .map_err(map_sqlx)?;

    let mut out = Vec::new();
    let mut day = start.date_naive();
    let last = end.date_naive();
    while day <= last {
        for (name, st, et) in &shifts {
            let s = day.and_time(*st).and_utc();
            let mut e = day.and_time(*et).and_utc();
            if e <= s {
                e += Duration::days(1); // overnight shift
            }
            let clamped_start = s.max(start);
            let clamped_end = e.min(end);
            if clamped_start >= clamped_end {
                continue;
            }
            let result = oee_sql(pool, work_center_id, clamped_start, clamped_end).await?;
            out.push(ShiftOee {
                shift_name: name.clone(),
                start: clamped_start,
                end: clamped_end,
                result,
            });
        }
        day += Duration::days(1);
    }
    Ok(out)
}

// ---- Seed helpers (tests + future master-data wiring) --------------------

pub async fn set_work_center_ideal_cycle(
    pool: &PgPool,
    work_center_id: &str,
    seconds: f64,
) -> RepoResult<()> {
    sqlx::query(
        "UPDATE work_centers SET ideal_cycle_seconds = $2, updated_at = now() WHERE id = $1",
    )
    .bind(work_center_id)
    .bind(seconds)
    .execute(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(())
}

pub async fn insert_machine_state(
    pool: &PgPool,
    work_center_id: &str,
    state: &str,
    start_ts: DateTime<Utc>,
    end_ts: DateTime<Utc>,
) -> RepoResult<()> {
    sqlx::query(
        "INSERT INTO machine_states (id, work_center_id, state, start_ts, end_ts)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(mes_core::new_id())
    .bind(work_center_id)
    .bind(state)
    .bind(start_ts)
    .bind(end_ts)
    .execute(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(())
}

pub async fn insert_count(
    pool: &PgPool,
    work_center_id: &str,
    ts: DateTime<Utc>,
    good: i32,
    scrap: i32,
) -> RepoResult<()> {
    sqlx::query(
        "INSERT INTO production_counts (id, ts, work_center_id, good, scrap)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(mes_core::new_id())
    .bind(ts)
    .bind(work_center_id)
    .bind(good)
    .bind(scrap)
    .execute(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(())
}

pub async fn create_shift(
    pool: &PgPool,
    site_id: &str,
    name: &str,
    start_time: NaiveTime,
    end_time: NaiveTime,
) -> RepoResult<()> {
    sqlx::query(
        "INSERT INTO shifts (id, site_id, name, start_time, end_time)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(mes_core::new_id())
    .bind(site_id)
    .bind(name)
    .bind(start_time)
    .bind(end_time)
    .execute(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(())
}
