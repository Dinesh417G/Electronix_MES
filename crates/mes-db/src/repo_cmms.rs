//! M9 repositories — CMMS: PM schedules (calendar + usage-hours off the existing
//! `machine_states` RUNNING intervals), maintenance work orders, the spare-parts
//! ledger, and procurement requests (§7, §8, §12 M9).
//!
//! The correctness-critical bit — *is a PM due?* — is decided by the pure,
//! unit-tested `mes_core::cmms` functions; this layer only supplies the current
//! clock / run-hours and applies them.

use chrono::{DateTime, Utc};
use mes_client::cmms::{MaintenanceWo, PmDue, PmSchedule, ProcurementRequest, SparePart};
use mes_core::cmms::{MaintenanceStatus, MaintenanceType, PmTrigger, ProcurementReason};
use rust_decimal::Decimal;
use sqlx::PgPool;

use crate::repo::{RepoError, RepoResult};

fn map_sqlx(e: sqlx::Error) -> RepoError {
    match &e {
        sqlx::Error::RowNotFound => RepoError::NotFound,
        sqlx::Error::Database(db) => match db.code().as_deref() {
            Some("23505") => RepoError::Conflict(db.message().to_string()),
            Some("23503") => RepoError::InvalidReference(db.message().to_string()),
            _ => RepoError::Db(e),
        },
        _ => RepoError::Db(e),
    }
}

fn nid() -> String {
    mes_core::new_id()
}

// ---- PM schedules --------------------------------------------------------

const PM_COLS: &str = "id, work_center_id, name, trigger_type, interval_value, last_done_at, \
     next_due_at, last_done_usage_h, next_due_usage_h, checklist_ref, enabled, created_at, \
     updated_at";

#[derive(sqlx::FromRow)]
struct PmRow {
    id: String,
    work_center_id: String,
    name: String,
    trigger_type: String,
    interval_value: Decimal,
    last_done_at: Option<DateTime<Utc>>,
    next_due_at: Option<DateTime<Utc>>,
    last_done_usage_h: Option<Decimal>,
    next_due_usage_h: Option<Decimal>,
    checklist_ref: Option<String>,
    enabled: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<PmRow> for PmSchedule {
    fn from(r: PmRow) -> Self {
        PmSchedule {
            id: r.id,
            work_center_id: r.work_center_id,
            name: r.name,
            trigger_type: r.trigger_type,
            interval_value: r.interval_value,
            last_done_at: r.last_done_at,
            next_due_at: r.next_due_at,
            last_done_usage_h: r.last_done_usage_h,
            next_due_usage_h: r.next_due_usage_h,
            checklist_ref: r.checklist_ref,
            enabled: r.enabled,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

/// Cumulative RUNNING hours for a work center, summed from `machine_states`
/// (§7 — usage-hours PM reuses this, no new raw data).
pub async fn work_center_run_hours(pool: &PgPool, work_center_id: &str) -> RepoResult<Decimal> {
    let (hours,): (Decimal,) = sqlx::query_as(
        "SELECT COALESCE(
             SUM(EXTRACT(EPOCH FROM (end_ts - start_ts)) / 3600.0), 0)::numeric
         FROM machine_states
         WHERE work_center_id = $1 AND state = 'running'",
    )
    .bind(work_center_id)
    .fetch_one(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(hours)
}

/// Create a PM schedule. For a calendar trigger the next-due time is anchored to
/// now + interval days; for a usage-hours trigger the baseline is the work
/// center's current run-hours and it becomes due after `interval` more hours.
pub async fn create_pm_schedule(
    pool: &PgPool,
    work_center_id: &str,
    name: &str,
    trigger_type: &str,
    interval_value: Decimal,
    checklist_ref: Option<&str>,
) -> RepoResult<PmSchedule> {
    let trigger = PmTrigger::parse(trigger_type)
        .ok_or_else(|| RepoError::InvalidReference("unknown trigger_type".to_string()))?;

    let row: PmRow = match trigger {
        PmTrigger::Calendar => sqlx::query_as(&format!(
            "INSERT INTO pm_schedules
                 (id, work_center_id, name, trigger_type, interval_value, checklist_ref,
                  last_done_at, next_due_at)
             VALUES ($1, $2, $3, 'calendar', $4, $5, now(), now() + ($4 * INTERVAL '1 day'))
             RETURNING {PM_COLS}"
        ))
        .bind(nid())
        .bind(work_center_id)
        .bind(name)
        .bind(interval_value)
        .bind(checklist_ref)
        .fetch_one(pool)
        .await
        .map_err(map_sqlx)?,

        PmTrigger::UsageHours => {
            let baseline = work_center_run_hours(pool, work_center_id).await?;
            let next_due = mes_core::cmms::usage_next_due(baseline, interval_value);
            sqlx::query_as(&format!(
                "INSERT INTO pm_schedules
                     (id, work_center_id, name, trigger_type, interval_value, checklist_ref,
                      last_done_usage_h, next_due_usage_h)
                 VALUES ($1, $2, $3, 'usage_hours', $4, $5, $6, $7)
                 RETURNING {PM_COLS}"
            ))
            .bind(nid())
            .bind(work_center_id)
            .bind(name)
            .bind(interval_value)
            .bind(checklist_ref)
            .bind(baseline)
            .bind(next_due)
            .fetch_one(pool)
            .await
            .map_err(map_sqlx)?
        }
    };
    Ok(row.into())
}

#[derive(sqlx::FromRow)]
struct PmDueRow {
    #[sqlx(flatten)]
    sched: PmRow,
    current_usage_h: Decimal,
}

/// List enabled PM schedules that are currently due. Calendar schedules are due
/// once `now` passes `next_due_at`; usage schedules are due once the work
/// center's cumulative run-hours reach `next_due_usage_h`. The due decision is
/// made by `mes_core::cmms` so it is exactly the unit-tested logic (§13).
pub async fn list_pm_due(pool: &PgPool) -> RepoResult<Vec<PmDue>> {
    // The subquery is scalar, so pm_schedules' own columns are unambiguous
    // unqualified — no aliasing needed.
    let rows: Vec<PmDueRow> = sqlx::query_as(&format!(
        "SELECT {PM_COLS},
             COALESCE((SELECT SUM(EXTRACT(EPOCH FROM (m.end_ts - m.start_ts)) / 3600.0)
                       FROM machine_states m
                       WHERE m.work_center_id = p.work_center_id AND m.state = 'running'),
                      0)::numeric AS current_usage_h
         FROM pm_schedules p
         WHERE p.enabled = TRUE
         ORDER BY p.created_at"
    ))
    .fetch_all(pool)
    .await
    .map_err(map_sqlx)?;

    let now = Utc::now();
    let mut due = Vec::new();
    for r in rows {
        let is_due = match PmTrigger::parse(&r.sched.trigger_type) {
            Some(PmTrigger::Calendar) => r
                .sched
                .next_due_at
                .is_some_and(|nd| mes_core::cmms::calendar_is_due(nd, now)),
            Some(PmTrigger::UsageHours) => r
                .sched
                .next_due_usage_h
                .is_some_and(|nd| mes_core::cmms::usage_is_due(nd, r.current_usage_h)),
            None => false,
        };
        if is_due {
            due.push(PmDue {
                schedule: r.sched.into(),
                current_usage_h: r.current_usage_h,
            });
        }
    }
    Ok(due)
}

// ---- Maintenance work orders ---------------------------------------------

const MWO_COLS: &str = "id, work_center_id, pm_schedule_id, wo_type, status, technician_id, \
     failure_code, notes, opened_at, closed_at, created_at, updated_at";

#[derive(sqlx::FromRow)]
struct MwoRow {
    id: String,
    work_center_id: String,
    pm_schedule_id: Option<String>,
    wo_type: String,
    status: String,
    technician_id: Option<String>,
    failure_code: Option<String>,
    notes: Option<String>,
    opened_at: DateTime<Utc>,
    closed_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<MwoRow> for MaintenanceWo {
    fn from(r: MwoRow) -> Self {
        MaintenanceWo {
            id: r.id,
            work_center_id: r.work_center_id,
            pm_schedule_id: r.pm_schedule_id,
            wo_type: r.wo_type,
            status: r.status,
            technician_id: r.technician_id,
            failure_code: r.failure_code,
            notes: r.notes,
            opened_at: r.opened_at,
            closed_at: r.closed_at,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

pub async fn create_maintenance_wo(
    pool: &PgPool,
    work_center_id: &str,
    pm_schedule_id: Option<&str>,
    wo_type: &str,
    notes: Option<&str>,
) -> RepoResult<MaintenanceWo> {
    let wo_type = MaintenanceType::parse(wo_type)
        .ok_or_else(|| RepoError::InvalidReference("unknown wo_type".to_string()))?;
    let row: MwoRow = sqlx::query_as(&format!(
        "INSERT INTO maintenance_work_orders
             (id, work_center_id, pm_schedule_id, wo_type, status, notes)
         VALUES ($1, $2, $3, $4, 'requested', $5)
         RETURNING {MWO_COLS}"
    ))
    .bind(nid())
    .bind(work_center_id)
    .bind(pm_schedule_id)
    .bind(wo_type.as_str())
    .bind(notes)
    .fetch_one(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(row.into())
}

pub async fn get_maintenance_wo(pool: &PgPool, id: &str) -> RepoResult<MaintenanceWo> {
    let row: MwoRow = sqlx::query_as(&format!(
        "SELECT {MWO_COLS} FROM maintenance_work_orders WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(map_sqlx)?
    .ok_or(RepoError::NotFound)?;
    Ok(row.into())
}

pub async fn list_maintenance_wos(pool: &PgPool) -> RepoResult<Vec<MaintenanceWo>> {
    let rows: Vec<MwoRow> = sqlx::query_as(&format!(
        "SELECT {MWO_COLS} FROM maintenance_work_orders ORDER BY opened_at DESC"
    ))
    .fetch_all(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// Advance a maintenance WO one lifecycle step. The transition is validated by
/// `mes_core::cmms::MaintenanceStatus::can_transition`; an illegal move is a
/// `Conflict`. The matching timestamp column is stamped as it advances.
pub async fn transition_maintenance_wo(
    pool: &PgPool,
    id: &str,
    next: &str,
    technician_id: Option<&str>,
    failure_code: Option<&str>,
) -> RepoResult<MaintenanceWo> {
    let next = MaintenanceStatus::parse(next)
        .ok_or_else(|| RepoError::InvalidReference("unknown status".to_string()))?;

    let mut tx = pool.begin().await.map_err(map_sqlx)?;

    let (cur_str,): (String,) =
        sqlx::query_as("SELECT status FROM maintenance_work_orders WHERE id = $1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(map_sqlx)?
            .ok_or(RepoError::NotFound)?;

    let cur = MaintenanceStatus::parse(&cur_str)
        .ok_or_else(|| RepoError::Conflict("corrupt status".to_string()))?;
    if !cur.can_transition(next) {
        return Err(RepoError::Conflict(format!(
            "cannot move {} -> {}",
            cur.as_str(),
            next.as_str()
        )));
    }

    let ts_col = match next {
        MaintenanceStatus::Scheduled => "scheduled_at",
        MaintenanceStatus::InProgress => "started_at",
        MaintenanceStatus::Completed => "closed_at",
        MaintenanceStatus::Verified => "verified_at",
        // Unreachable: nothing legally transitions *to* Requested.
        MaintenanceStatus::Requested => "opened_at",
    };

    let row: MwoRow = sqlx::query_as(&format!(
        "UPDATE maintenance_work_orders
         SET status = $2, {ts_col} = now(),
             technician_id = COALESCE($3, technician_id),
             failure_code = COALESCE($4, failure_code),
             updated_at = now()
         WHERE id = $1
         RETURNING {MWO_COLS}"
    ))
    .bind(id)
    .bind(next.as_str())
    .bind(technician_id)
    .bind(failure_code)
    .fetch_one(&mut *tx)
    .await
    .map_err(map_sqlx)?;

    tx.commit().await.map_err(map_sqlx)?;
    Ok(row.into())
}

// ---- Spare parts + ledger ------------------------------------------------

const SPARE_COLS: &str = "id, code, name, uom, reorder_point, reorder_qty";
const PROC_COLS: &str = "id, spare_part_id, qty_requested, reason, status, erp_reference, \
     pushed_at, created_at, updated_at";

#[derive(sqlx::FromRow)]
struct ProcRow {
    id: String,
    spare_part_id: String,
    qty_requested: Decimal,
    reason: String,
    status: String,
    erp_reference: Option<String>,
    pushed_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<ProcRow> for ProcurementRequest {
    fn from(r: ProcRow) -> Self {
        ProcurementRequest {
            id: r.id,
            spare_part_id: r.spare_part_id,
            qty_requested: r.qty_requested,
            reason: r.reason,
            status: r.status,
            erp_reference: r.erp_reference,
            pushed_at: r.pushed_at,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

pub async fn create_spare_part(
    pool: &PgPool,
    code: &str,
    name: &str,
    uom: &str,
    reorder_point: Decimal,
    reorder_qty: Decimal,
) -> RepoResult<SparePart> {
    let (id, code, name, uom, rp, rq): (String, String, String, String, Decimal, Decimal) =
        sqlx::query_as(&format!(
            "INSERT INTO spare_parts (id, code, name, uom, reorder_point, reorder_qty)
             VALUES ($1, $2, $3, $4, $5, $6)
             RETURNING {SPARE_COLS}"
        ))
        .bind(nid())
        .bind(code)
        .bind(name)
        .bind(uom)
        .bind(reorder_point)
        .bind(reorder_qty)
        .fetch_one(pool)
        .await
        .map_err(map_sqlx)?;
    Ok(SparePart {
        id,
        code,
        name,
        uom,
        reorder_point: rp,
        reorder_qty: rq,
        stock: Decimal::ZERO,
    })
}

/// Derived stock = sum of the (already-signed) txn ledger (§7).
pub async fn spare_stock(pool: &PgPool, spare_part_id: &str) -> RepoResult<Decimal> {
    let (stock,): (Decimal,) = sqlx::query_as(
        "SELECT COALESCE(SUM(qty), 0)::numeric FROM spare_txns WHERE spare_part_id = $1",
    )
    .bind(spare_part_id)
    .fetch_one(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(stock)
}

pub async fn list_spare_parts(pool: &PgPool) -> RepoResult<Vec<SparePart>> {
    let rows: Vec<(String, String, String, String, Decimal, Decimal, Decimal)> =
        sqlx::query_as(
            "SELECT s.id, s.code, s.name, s.uom, s.reorder_point, s.reorder_qty,
                 COALESCE((SELECT SUM(qty) FROM spare_txns t WHERE t.spare_part_id = s.id), 0)::numeric
             FROM spare_parts s
             ORDER BY s.code",
        )
        .fetch_all(pool)
        .await
        .map_err(map_sqlx)?;
    Ok(rows
        .into_iter()
        .map(|(id, code, name, uom, rp, rq, stock)| SparePart {
            id,
            code,
            name,
            uom,
            reorder_point: rp,
            reorder_qty: rq,
            stock,
        })
        .collect())
}

/// Record a spare movement and return the new stock plus any auto-raised
/// reorder-point procurement request. `qty` arrives as a positive magnitude;
/// the sign is applied from `txn_type` (issue decreases stock, receive
/// increases, adjust is passed through signed). A breach of the reorder point
/// raises at most one open request thanks to the partial unique index (§7).
pub async fn record_spare_txn(
    pool: &PgPool,
    spare_part_id: &str,
    maintenance_wo_id: Option<&str>,
    txn_type: &str,
    qty: Decimal,
    user_id: &str,
) -> RepoResult<(String, Decimal, Option<ProcurementRequest>)> {
    let signed = match txn_type {
        "issue" => -qty.abs(),
        "receive" => qty.abs(),
        "adjust" => qty,
        _ => return Err(RepoError::InvalidReference("unknown txn_type".to_string())),
    };

    // Reorder thresholds for the breach check.
    let (reorder_point, reorder_qty): (Decimal, Decimal) =
        sqlx::query_as("SELECT reorder_point, reorder_qty FROM spare_parts WHERE id = $1")
            .bind(spare_part_id)
            .fetch_optional(pool)
            .await
            .map_err(map_sqlx)?
            .ok_or(RepoError::NotFound)?;

    let mut tx = pool.begin().await.map_err(map_sqlx)?;

    let txn_id = nid();
    sqlx::query(
        "INSERT INTO spare_txns (id, spare_part_id, maintenance_wo_id, txn_type, qty, user_id)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(&txn_id)
    .bind(spare_part_id)
    .bind(maintenance_wo_id)
    .bind(txn_type)
    .bind(signed)
    .bind(user_id)
    .execute(&mut *tx)
    .await
    .map_err(map_sqlx)?;

    let (stock,): (Decimal,) = sqlx::query_as(
        "SELECT COALESCE(SUM(qty), 0)::numeric FROM spare_txns WHERE spare_part_id = $1",
    )
    .bind(spare_part_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(map_sqlx)?;

    let mut request: Option<ProcurementRequest> = None;
    if stock <= reorder_point && reorder_qty > Decimal::ZERO {
        // Idempotent: the partial unique index keeps at most one open auto
        // request per spare, so a repeat breach does not spam duplicates.
        let row: Option<ProcRow> = sqlx::query_as(&format!(
            "INSERT INTO procurement_requests (id, spare_part_id, qty_requested, reason, status)
             VALUES ($1, $2, $3, '{reason}', 'requested')
             ON CONFLICT (spare_part_id) WHERE status = 'requested' AND reason = 'reorder_point'
             DO NOTHING
             RETURNING {PROC_COLS}",
            reason = ProcurementReason::ReorderPoint.as_str()
        ))
        .bind(nid())
        .bind(spare_part_id)
        .bind(reorder_qty)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_sqlx)?;
        request = row.map(Into::into);
    }

    tx.commit().await.map_err(map_sqlx)?;
    Ok((txn_id, stock, request))
}

// ---- Procurement requests ------------------------------------------------

pub async fn create_procurement_request(
    pool: &PgPool,
    spare_part_id: &str,
    qty_requested: Decimal,
) -> RepoResult<ProcurementRequest> {
    let row: ProcRow = sqlx::query_as(&format!(
        "INSERT INTO procurement_requests (id, spare_part_id, qty_requested, reason, status)
         VALUES ($1, $2, $3, '{reason}', 'requested')
         RETURNING {PROC_COLS}",
        reason = ProcurementReason::Manual.as_str()
    ))
    .bind(nid())
    .bind(spare_part_id)
    .bind(qty_requested)
    .fetch_one(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(row.into())
}

pub async fn list_procurement_requests(pool: &PgPool) -> RepoResult<Vec<ProcurementRequest>> {
    let rows: Vec<ProcRow> = sqlx::query_as(&format!(
        "SELECT {PROC_COLS} FROM procurement_requests ORDER BY created_at DESC"
    ))
    .fetch_all(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(rows.into_iter().map(Into::into).collect())
}
