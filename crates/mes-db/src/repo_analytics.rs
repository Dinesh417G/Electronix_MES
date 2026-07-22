//! M5 repositories — downtime aggregation for Pareto, Six-Big-Losses, and trend.
//!
//! SQL does the grouping/summing; the ranking + cumulative maths run in
//! `mes_core::analytics` so they stay pure and fixture-testable (§12 M5).

use chrono::{DateTime, Utc};
use mes_core::analytics::ParetoInput;
use sqlx::PgPool;

use crate::repo::{RepoError, RepoResult};

fn map_sqlx(e: sqlx::Error) -> RepoError {
    match e {
        sqlx::Error::RowNotFound => RepoError::NotFound,
        other => RepoError::Db(other),
    }
}

/// Total classified-downtime seconds per reason over `[start, end)`, as Pareto
/// inputs (unranked). Unclassified downtime is excluded.
pub async fn downtime_pareto(
    pool: &PgPool,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> RepoResult<Vec<ParetoInput>> {
    let rows: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT r.code, r.label,
                SUM(EXTRACT(EPOCH FROM (de.end_ts - de.start_ts)))::bigint AS seconds
         FROM downtime_events de
         JOIN downtime_reasons r ON r.id = de.reason_id
         WHERE de.start_ts >= $1 AND de.start_ts < $2 AND de.reason_id IS NOT NULL
         GROUP BY r.code, r.label",
    )
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await
    .map_err(map_sqlx)?;

    Ok(rows
        .into_iter()
        .map(|(key, label, seconds)| ParetoInput {
            key,
            label,
            seconds,
        })
        .collect())
}

/// Total downtime seconds per Six-Big-Losses bucket over `[start, end)`.
pub async fn downtime_by_loss(
    pool: &PgPool,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> RepoResult<Vec<ParetoInput>> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT r.six_big_loss,
                SUM(EXTRACT(EPOCH FROM (de.end_ts - de.start_ts)))::bigint AS seconds
         FROM downtime_events de
         JOIN downtime_reasons r ON r.id = de.reason_id
         WHERE de.start_ts >= $1 AND de.start_ts < $2 AND r.six_big_loss IS NOT NULL
         GROUP BY r.six_big_loss",
    )
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await
    .map_err(map_sqlx)?;

    Ok(rows
        .into_iter()
        .map(|(loss, seconds)| ParetoInput {
            key: loss.clone(),
            label: loss,
            seconds,
        })
        .collect())
}

/// Total downtime seconds per day over `[start, end)`, ascending by day.
pub async fn downtime_trend(
    pool: &PgPool,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> RepoResult<Vec<(DateTime<Utc>, i64)>> {
    let rows: Vec<(DateTime<Utc>, i64)> = sqlx::query_as(
        "SELECT date_trunc('day', de.start_ts) AS day,
                SUM(EXTRACT(EPOCH FROM (de.end_ts - de.start_ts)))::bigint AS seconds
         FROM downtime_events de
         WHERE de.start_ts >= $1 AND de.start_ts < $2
         GROUP BY day ORDER BY day",
    )
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(rows)
}

// ---- Seed helpers (master-data wiring + tests) ---------------------------

/// Create a downtime reason with an optional Six-Big-Losses mapping and parent.
pub async fn create_downtime_reason(
    pool: &PgPool,
    code: &str,
    label: &str,
    six_big_loss: Option<&str>,
    parent_id: Option<&str>,
) -> RepoResult<String> {
    let id = mes_core::new_id();
    sqlx::query(
        "INSERT INTO downtime_reasons (id, code, label, six_big_loss, parent_id)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(&id)
    .bind(code)
    .bind(label)
    .bind(six_big_loss)
    .bind(parent_id)
    .execute(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(id)
}

/// Insert a (possibly classified) downtime event — used to seed analytics tests.
pub async fn insert_downtime_event(
    pool: &PgPool,
    work_center_id: &str,
    state: &str,
    start_ts: DateTime<Utc>,
    end_ts: DateTime<Utc>,
    reason_id: Option<&str>,
) -> RepoResult<String> {
    let id = mes_core::new_id();
    sqlx::query(
        "INSERT INTO downtime_events (id, work_center_id, state, start_ts, end_ts, reason_id)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(&id)
    .bind(work_center_id)
    .bind(state)
    .bind(start_ts)
    .bind(end_ts)
    .bind(reason_id)
    .execute(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(id)
}
