//! State-derivation pipeline (§8.1, §12 M2).
//!
//! Reads the raw cycle-pulse stream for a work center, runs the pure `mes-core`
//! state machine (overlaying planned stops), and persists the resulting
//! machine_states + downtime_events idempotently. This is the bridge between
//! ingested signals and derived operational state.

use chrono::{DateTime, Utc};
use mes_core::state_machine::{apply_planned_stops, compute_states, downtime_events, StateConfig};
use mes_db::repo::RepoError;
use mes_db::repo_ingest;
use sqlx::PgPool;

/// Recompute derived state for `work_center_id` over `[start, end]`. Returns the
/// number of state intervals and downtime events written.
pub async fn recompute_states(
    pool: &PgPool,
    work_center_id: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<(usize, usize), RepoError> {
    let cfg = StateConfig::default();

    let pulses = repo_ingest::fetch_cycle_pulses(pool, work_center_id, start, end).await?;
    let planned = repo_ingest::fetch_planned_intervals(pool, work_center_id, start, end).await?;

    let raw = compute_states(&cfg, &pulses, start, end);
    let states = apply_planned_stops(&raw, &planned);
    let downtime = downtime_events(&states);

    repo_ingest::replace_derived(pool, work_center_id, start, end, &states, &downtime).await?;

    Ok((states.len(), downtime.len()))
}
