//! M2 repositories — signal ingestion, derived states, and downtime events.
//!
//! Runtime-checked queries (see the note on [`crate::repo`]). Recompute is
//! idempotent: re-deriving a window replaces its machine_states and its
//! *unclassified* downtime_events, leaving operator-classified events intact.

use chrono::{DateTime, Utc};
use mes_core::state_machine::{DowntimeEvent, MachineState, PlannedInterval, StateInterval};
use sqlx::PgPool;

use crate::repo::{RepoError, RepoResult};

fn map_sqlx(e: sqlx::Error) -> RepoError {
    match e {
        sqlx::Error::RowNotFound => RepoError::NotFound,
        other => RepoError::Db(other),
    }
}

/// The DB string for a machine state.
fn state_str(s: MachineState) -> &'static str {
    match s {
        MachineState::Running => "running",
        MachineState::MicroStop => "micro_stop",
        MachineState::Down => "down",
        MachineState::PlannedStop => "planned_stop",
    }
}

/// A registered signal source.
#[derive(sqlx::FromRow, Debug, Clone)]
pub struct SignalSourceRow {
    pub id: String,
    pub work_center_id: String,
    pub source_key: String,
    pub kind: String,
    pub enabled: bool,
}

/// Resolve a source by its key. Returns `None` when unregistered (§9 — the
/// caller then drops the signal).
pub async fn resolve_source(
    pool: &PgPool,
    source_key: &str,
) -> RepoResult<Option<SignalSourceRow>> {
    sqlx::query_as::<_, SignalSourceRow>(
        "SELECT id, work_center_id, source_key, kind, enabled
         FROM signal_sources WHERE source_key = $1",
    )
    .bind(source_key)
    .fetch_optional(pool)
    .await
    .map_err(map_sqlx)
}

/// Register a signal source (used by master-data wiring and tests).
pub async fn create_signal_source(
    pool: &PgPool,
    work_center_id: &str,
    source_key: &str,
    kind: &str,
    enabled: bool,
) -> RepoResult<SignalSourceRow> {
    sqlx::query_as::<_, SignalSourceRow>(
        "INSERT INTO signal_sources (id, work_center_id, source_key, kind, enabled)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING id, work_center_id, source_key, kind, enabled",
    )
    .bind(mes_core::new_id())
    .bind(work_center_id)
    .bind(source_key)
    .bind(kind)
    .bind(enabled)
    .fetch_one(pool)
    .await
    .map_err(map_sqlx)
}

/// Append a raw machine event (hypertable insert).
pub async fn insert_machine_event(
    pool: &PgPool,
    ts: DateTime<Utc>,
    work_center_id: &str,
    source_id: &str,
    event_type: &str,
    payload: Option<serde_json::Value>,
) -> RepoResult<()> {
    sqlx::query(
        "INSERT INTO machine_events (id, ts, work_center_id, source_id, event_type, payload)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(mes_core::new_id())
    .bind(ts)
    .bind(work_center_id)
    .bind(source_id)
    .bind(event_type)
    .bind(payload)
    .execute(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(())
}

/// Append a production count (hypertable insert).
pub async fn insert_production_count(
    pool: &PgPool,
    ts: DateTime<Utc>,
    work_center_id: &str,
    source_id: &str,
    good: i32,
    scrap: i32,
) -> RepoResult<()> {
    sqlx::query(
        "INSERT INTO production_counts (id, ts, work_center_id, source_id, good, scrap)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(mes_core::new_id())
    .bind(ts)
    .bind(work_center_id)
    .bind(source_id)
    .bind(good)
    .bind(scrap)
    .execute(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(())
}

/// Fetch cycle-pulse timestamps for a work center in `[start, end]`, ascending.
pub async fn fetch_cycle_pulses(
    pool: &PgPool,
    work_center_id: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> RepoResult<Vec<DateTime<Utc>>> {
    let rows: Vec<(DateTime<Utc>,)> = sqlx::query_as(
        "SELECT ts FROM machine_events
         WHERE work_center_id = $1 AND event_type = 'cycle' AND ts >= $2 AND ts <= $3
         ORDER BY ts",
    )
    .bind(work_center_id)
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(rows.into_iter().map(|(ts,)| ts).collect())
}

/// Sum good/scrap counts for a work center in `[start, end]`.
pub async fn sum_counts(
    pool: &PgPool,
    work_center_id: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> RepoResult<(i64, i64)> {
    let row: (Option<i64>, Option<i64>) = sqlx::query_as(
        "SELECT COALESCE(SUM(good),0), COALESCE(SUM(scrap),0)
         FROM production_counts
         WHERE work_center_id = $1 AND ts >= $2 AND ts <= $3",
    )
    .bind(work_center_id)
    .bind(start)
    .bind(end)
    .fetch_one(pool)
    .await
    .map_err(map_sqlx)?;
    Ok((row.0.unwrap_or(0), row.1.unwrap_or(0)))
}

/// Fetch planned-stop intervals overlapping `[start, end]` for a work center.
pub async fn fetch_planned_intervals(
    pool: &PgPool,
    work_center_id: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> RepoResult<Vec<PlannedInterval>> {
    let rows: Vec<(DateTime<Utc>, DateTime<Utc>)> = sqlx::query_as(
        "SELECT starts_at, ends_at FROM planned_stops
         WHERE work_center_id = $1 AND starts_at < $3 AND ends_at > $2
         ORDER BY starts_at",
    )
    .bind(work_center_id)
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(rows
        .into_iter()
        .map(|(start, end)| PlannedInterval { start, end })
        .collect())
}

/// Replace derived machine states and unclassified downtime events for a window
/// with a freshly computed set, atomically. Classified downtime is preserved.
pub async fn replace_derived(
    pool: &PgPool,
    work_center_id: &str,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
    states: &[StateInterval],
    downtime: &[DowntimeEvent],
) -> RepoResult<()> {
    let mut tx = pool.begin().await.map_err(map_sqlx)?;

    sqlx::query(
        "DELETE FROM machine_states
         WHERE work_center_id = $1 AND start_ts < $3 AND end_ts > $2",
    )
    .bind(work_center_id)
    .bind(window_start)
    .bind(window_end)
    .execute(&mut *tx)
    .await
    .map_err(map_sqlx)?;

    for iv in states {
        sqlx::query(
            "INSERT INTO machine_states (id, work_center_id, state, start_ts, end_ts)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(mes_core::new_id())
        .bind(work_center_id)
        .bind(state_str(iv.state))
        .bind(iv.start)
        .bind(iv.end)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;
    }

    // Only unclassified downtime is regenerated; operator-classified rows stay.
    sqlx::query(
        "DELETE FROM downtime_events
         WHERE work_center_id = $1 AND reason_id IS NULL AND start_ts < $3 AND end_ts > $2",
    )
    .bind(work_center_id)
    .bind(window_start)
    .bind(window_end)
    .execute(&mut *tx)
    .await
    .map_err(map_sqlx)?;

    for dt in downtime {
        sqlx::query(
            "INSERT INTO downtime_events (id, work_center_id, state, start_ts, end_ts)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(&dt.id)
        .bind(work_center_id)
        .bind(state_str(dt.state))
        .bind(dt.start)
        .bind(dt.end)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;
    }

    tx.commit().await.map_err(map_sqlx)?;
    Ok(())
}

/// Read back derived states for a work center in a window, ascending — for
/// verification and analytics.
pub async fn list_machine_states(
    pool: &PgPool,
    work_center_id: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> RepoResult<Vec<(String, DateTime<Utc>, DateTime<Utc>)>> {
    let rows: Vec<(String, DateTime<Utc>, DateTime<Utc>)> = sqlx::query_as(
        "SELECT state, start_ts, end_ts FROM machine_states
         WHERE work_center_id = $1 AND start_ts >= $2 AND end_ts <= $3
         ORDER BY start_ts",
    )
    .bind(work_center_id)
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(rows)
}

/// Count downtime events for a work center in a window.
pub async fn count_downtime(
    pool: &PgPool,
    work_center_id: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> RepoResult<i64> {
    let (n,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM downtime_events
         WHERE work_center_id = $1 AND start_ts >= $2 AND end_ts <= $3",
    )
    .bind(work_center_id)
    .bind(start)
    .bind(end)
    .fetch_one(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(n)
}
