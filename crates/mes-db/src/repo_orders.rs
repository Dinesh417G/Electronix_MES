//! M3 repositories — work orders, operations, execution, and programs.
//!
//! Runtime-checked queries (see [`crate::repo`]). Multi-step writes (create WO +
//! ops, record count + roll up totals) run in a transaction so partial state is
//! never observable.

use chrono::{DateTime, Utc};
use mes_client::exec::DowntimeEventDto;
use mes_client::master::{Program, ProgramInput};
use mes_client::orders::{WoOperation, WorkOrder, WorkOrderDetail, WorkOrderInput};
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

// ===========================================================================
// Work orders + operations
// ===========================================================================

#[derive(sqlx::FromRow)]
struct WorkOrderRow {
    id: String,
    wo_number: String,
    part_id: String,
    routing_id: Option<String>,
    qty_ordered: Decimal,
    priority: i32,
    status: String,
    planned_start: Option<DateTime<Utc>>,
    planned_end: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<WorkOrderRow> for WorkOrder {
    fn from(r: WorkOrderRow) -> Self {
        WorkOrder {
            id: r.id,
            wo_number: r.wo_number,
            part_id: r.part_id,
            routing_id: r.routing_id,
            qty_ordered: r.qty_ordered,
            priority: r.priority,
            status: r.status,
            planned_start: r.planned_start,
            planned_end: r.planned_end,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct WoOperationRow {
    id: String,
    work_order_id: String,
    routing_op_id: Option<String>,
    op_no: i32,
    work_center_id: Option<String>,
    status: String,
    qty_good: i32,
    qty_scrap: i32,
    started_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<WoOperationRow> for WoOperation {
    fn from(r: WoOperationRow) -> Self {
        WoOperation {
            id: r.id,
            work_order_id: r.work_order_id,
            routing_op_id: r.routing_op_id,
            op_no: r.op_no,
            work_center_id: r.work_center_id,
            status: r.status,
            qty_good: r.qty_good,
            qty_scrap: r.qty_scrap,
            started_at: r.started_at,
            completed_at: r.completed_at,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

const WO_COLS: &str = "id, wo_number, part_id, routing_id, qty_ordered, priority, status, \
     planned_start, planned_end, created_at, updated_at";
const OP_COLS: &str = "id, work_order_id, routing_op_id, op_no, work_center_id, status, \
     qty_good, qty_scrap, started_at, completed_at, created_at, updated_at";

/// Create a work order and its operations atomically.
pub async fn create_work_order(
    pool: &PgPool,
    input: &WorkOrderInput,
) -> RepoResult<WorkOrderDetail> {
    let mut tx = pool.begin().await.map_err(map_sqlx)?;

    let wo_id = nid();
    let wo: WorkOrderRow = sqlx::query_as(&format!(
        "INSERT INTO work_orders (id, wo_number, part_id, routing_id, qty_ordered, priority,
             planned_start, planned_end)
         VALUES ($1, $2, $3, $4, $5, COALESCE($6, 100), $7, $8)
         RETURNING {WO_COLS}"
    ))
    .bind(&wo_id)
    .bind(&input.wo_number)
    .bind(&input.part_id)
    .bind(&input.routing_id)
    .bind(input.qty_ordered)
    .bind(input.priority)
    .bind(input.planned_start)
    .bind(input.planned_end)
    .fetch_one(&mut *tx)
    .await
    .map_err(map_sqlx)?;

    let mut operations = Vec::new();
    for op in &input.operations {
        let row: WoOperationRow = sqlx::query_as(&format!(
            "INSERT INTO wo_operations (id, work_order_id, routing_op_id, op_no, work_center_id)
             VALUES ($1, $2, $3, $4, $5)
             RETURNING {OP_COLS}"
        ))
        .bind(nid())
        .bind(&wo_id)
        .bind(&op.routing_op_id)
        .bind(op.op_no)
        .bind(&op.work_center_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_sqlx)?;
        operations.push(WoOperation::from(row));
    }

    tx.commit().await.map_err(map_sqlx)?;
    Ok(WorkOrderDetail {
        work_order: wo.into(),
        operations,
    })
}

pub async fn list_work_orders(pool: &PgPool) -> RepoResult<Vec<WorkOrder>> {
    let rows: Vec<WorkOrderRow> = sqlx::query_as(&format!(
        "SELECT {WO_COLS} FROM work_orders ORDER BY priority, wo_number"
    ))
    .fetch_all(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(rows.into_iter().map(Into::into).collect())
}

pub async fn get_work_order(pool: &PgPool, id: &str) -> RepoResult<WorkOrder> {
    let row: WorkOrderRow =
        sqlx::query_as(&format!("SELECT {WO_COLS} FROM work_orders WHERE id = $1"))
            .bind(id)
            .fetch_optional(pool)
            .await
            .map_err(map_sqlx)?
            .ok_or(RepoError::NotFound)?;
    Ok(row.into())
}

pub async fn get_work_order_detail(pool: &PgPool, id: &str) -> RepoResult<WorkOrderDetail> {
    let work_order = get_work_order(pool, id).await?;
    let ops: Vec<WoOperationRow> = sqlx::query_as(&format!(
        "SELECT {OP_COLS} FROM wo_operations WHERE work_order_id = $1 ORDER BY op_no"
    ))
    .bind(id)
    .fetch_all(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(WorkOrderDetail {
        work_order,
        operations: ops.into_iter().map(Into::into).collect(),
    })
}

/// Set the work-order status. Transition validity is enforced by the caller
/// (via `mes_core::work_order`); this only writes.
pub async fn set_work_order_status(pool: &PgPool, id: &str, status: &str) -> RepoResult<WorkOrder> {
    let row: WorkOrderRow = sqlx::query_as(&format!(
        "UPDATE work_orders SET status = $2, updated_at = now() WHERE id = $1 RETURNING {WO_COLS}"
    ))
    .bind(id)
    .bind(status)
    .fetch_optional(pool)
    .await
    .map_err(map_sqlx)?
    .ok_or(RepoError::NotFound)?;
    Ok(row.into())
}

pub async fn get_operation(pool: &PgPool, op_id: &str) -> RepoResult<WoOperation> {
    let row: WoOperationRow = sqlx::query_as(&format!(
        "SELECT {OP_COLS} FROM wo_operations WHERE id = $1"
    ))
    .bind(op_id)
    .fetch_optional(pool)
    .await
    .map_err(map_sqlx)?
    .ok_or(RepoError::NotFound)?;
    Ok(row.into())
}

/// Move an operation to `in_progress`, stamping `started_at`.
pub async fn start_operation(pool: &PgPool, op_id: &str) -> RepoResult<WoOperation> {
    let row: WoOperationRow = sqlx::query_as(&format!(
        "UPDATE wo_operations
         SET status = 'in_progress', started_at = COALESCE(started_at, now()), updated_at = now()
         WHERE id = $1 RETURNING {OP_COLS}"
    ))
    .bind(op_id)
    .fetch_optional(pool)
    .await
    .map_err(map_sqlx)?
    .ok_or(RepoError::NotFound)?;
    Ok(row.into())
}

/// Move an operation to `completed`, stamping `completed_at`.
pub async fn complete_operation(pool: &PgPool, op_id: &str) -> RepoResult<WoOperation> {
    let row: WoOperationRow = sqlx::query_as(&format!(
        "UPDATE wo_operations
         SET status = 'completed', completed_at = now(), updated_at = now()
         WHERE id = $1 RETURNING {OP_COLS}"
    ))
    .bind(op_id)
    .fetch_optional(pool)
    .await
    .map_err(map_sqlx)?
    .ok_or(RepoError::NotFound)?;
    Ok(row.into())
}

/// Record a good/scrap count against an operation: append a `production_counts`
/// row and roll the totals up onto the operation, atomically.
pub async fn record_count(
    pool: &PgPool,
    op_id: &str,
    work_center_id: &str,
    good: i32,
    scrap: i32,
    scrap_reason_id: Option<&str>,
    ts: DateTime<Utc>,
) -> RepoResult<WoOperation> {
    let mut tx = pool.begin().await.map_err(map_sqlx)?;

    sqlx::query(
        "INSERT INTO production_counts
             (id, ts, work_center_id, good, scrap, wo_operation_id, scrap_reason_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(nid())
    .bind(ts)
    .bind(work_center_id)
    .bind(good)
    .bind(scrap)
    .bind(op_id)
    .bind(scrap_reason_id)
    .execute(&mut *tx)
    .await
    .map_err(map_sqlx)?;

    let row: WoOperationRow = sqlx::query_as(&format!(
        "UPDATE wo_operations
         SET qty_good = qty_good + $2, qty_scrap = qty_scrap + $3, updated_at = now()
         WHERE id = $1 RETURNING {OP_COLS}"
    ))
    .bind(op_id)
    .bind(good)
    .bind(scrap)
    .fetch_one(&mut *tx)
    .await
    .map_err(map_sqlx)?;

    tx.commit().await.map_err(map_sqlx)?;
    Ok(row.into())
}

// ===========================================================================
// Downtime classification / split
// ===========================================================================

#[derive(sqlx::FromRow)]
struct DowntimeRow {
    id: String,
    work_center_id: String,
    state: String,
    start_ts: DateTime<Utc>,
    end_ts: DateTime<Utc>,
    reason_id: Option<String>,
}

impl From<DowntimeRow> for DowntimeEventDto {
    fn from(r: DowntimeRow) -> Self {
        DowntimeEventDto {
            id: r.id,
            work_center_id: r.work_center_id,
            state: r.state,
            start_ts: r.start_ts,
            end_ts: r.end_ts,
            reason_id: r.reason_id,
        }
    }
}

const DT_COLS: &str = "id, work_center_id, state, start_ts, end_ts, reason_id";

pub async fn get_downtime_event(pool: &PgPool, id: &str) -> RepoResult<DowntimeEventDto> {
    let row: DowntimeRow = sqlx::query_as(&format!(
        "SELECT {DT_COLS} FROM downtime_events WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(map_sqlx)?
    .ok_or(RepoError::NotFound)?;
    Ok(row.into())
}

/// Attach a reason to a downtime event (classification).
pub async fn classify_downtime(
    pool: &PgPool,
    id: &str,
    reason_id: &str,
    classified_by: &str,
) -> RepoResult<DowntimeEventDto> {
    let row: DowntimeRow = sqlx::query_as(&format!(
        "UPDATE downtime_events
         SET reason_id = $2, classified_by = $3, classified_at = now()
         WHERE id = $1 RETURNING {DT_COLS}"
    ))
    .bind(id)
    .bind(reason_id)
    .bind(classified_by)
    .fetch_optional(pool)
    .await
    .map_err(map_sqlx)?
    .ok_or(RepoError::NotFound)?;
    Ok(row.into())
}

/// Split a downtime event at `at` into two contiguous events, optionally
/// classifying each. Returns the two resulting events (earlier first).
pub async fn split_downtime(
    pool: &PgPool,
    id: &str,
    at: DateTime<Utc>,
    first_reason: Option<&str>,
    second_reason: Option<&str>,
    classified_by: &str,
) -> RepoResult<(DowntimeEventDto, DowntimeEventDto)> {
    let mut tx = pool.begin().await.map_err(map_sqlx)?;

    let original: DowntimeRow = sqlx::query_as(&format!(
        "SELECT {DT_COLS} FROM downtime_events WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(map_sqlx)?
    .ok_or(RepoError::NotFound)?;

    if at <= original.start_ts || at >= original.end_ts {
        return Err(RepoError::Conflict(
            "split point must lie strictly inside the event".to_string(),
        ));
    }

    // Shrink the original to [start, at] and classify it.
    let first: DowntimeRow = sqlx::query_as(&format!(
        "UPDATE downtime_events
         SET end_ts = $2, reason_id = $3,
             classified_by = CASE WHEN $3 IS NULL THEN classified_by ELSE $4 END,
             classified_at = CASE WHEN $3 IS NULL THEN classified_at ELSE now() END
         WHERE id = $1 RETURNING {DT_COLS}"
    ))
    .bind(id)
    .bind(at)
    .bind(first_reason)
    .bind(classified_by)
    .fetch_one(&mut *tx)
    .await
    .map_err(map_sqlx)?;

    // Insert the second half [at, end].
    let second: DowntimeRow = sqlx::query_as(&format!(
        "INSERT INTO downtime_events
             (id, work_center_id, state, start_ts, end_ts, reason_id, classified_by, classified_at)
         VALUES ($1, $2, $3, $4, $5, $6,
                 CASE WHEN $6 IS NULL THEN NULL ELSE $7 END,
                 CASE WHEN $6 IS NULL THEN NULL ELSE now() END)
         RETURNING {DT_COLS}"
    ))
    .bind(nid())
    .bind(&original.work_center_id)
    .bind(&original.state)
    .bind(at)
    .bind(original.end_ts)
    .bind(second_reason)
    .bind(classified_by)
    .fetch_one(&mut *tx)
    .await
    .map_err(map_sqlx)?;

    tx.commit().await.map_err(map_sqlx)?;
    Ok((first.into(), second.into()))
}

// ===========================================================================
// Reason seeds (used by exec + tests)
// ===========================================================================

pub async fn create_downtime_reason(pool: &PgPool, code: &str, label: &str) -> RepoResult<String> {
    let rid = nid();
    sqlx::query("INSERT INTO downtime_reasons (id, code, label) VALUES ($1, $2, $3)")
        .bind(&rid)
        .bind(code)
        .bind(label)
        .execute(pool)
        .await
        .map_err(map_sqlx)?;
    Ok(rid)
}

pub async fn create_scrap_reason(pool: &PgPool, code: &str, label: &str) -> RepoResult<String> {
    let rid = nid();
    sqlx::query("INSERT INTO scrap_reasons (id, code, label) VALUES ($1, $2, $3)")
        .bind(&rid)
        .bind(code)
        .bind(label)
        .execute(pool)
        .await
        .map_err(map_sqlx)?;
    Ok(rid)
}

// ===========================================================================
// Programs (routing_op ↔ DNC library, §7)
// ===========================================================================

#[derive(sqlx::FromRow)]
struct ProgramRow {
    id: String,
    routing_op_id: Option<String>,
    part_id: Option<String>,
    program_identifier: String,
    target_machine: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<ProgramRow> for Program {
    fn from(r: ProgramRow) -> Self {
        Program {
            id: r.id,
            routing_op_id: r.routing_op_id,
            part_id: r.part_id,
            program_identifier: r.program_identifier,
            target_machine: r.target_machine,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

const PROG_COLS: &str =
    "id, routing_op_id, part_id, program_identifier, target_machine, created_at, updated_at";

pub async fn create_program(pool: &PgPool, input: &ProgramInput) -> RepoResult<Program> {
    let row: ProgramRow = sqlx::query_as(&format!(
        "INSERT INTO programs (id, routing_op_id, part_id, program_identifier, target_machine)
         VALUES ($1, $2, $3, $4, $5) RETURNING {PROG_COLS}"
    ))
    .bind(nid())
    .bind(&input.routing_op_id)
    .bind(&input.part_id)
    .bind(&input.program_identifier)
    .bind(&input.target_machine)
    .fetch_one(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(row.into())
}

pub async fn list_programs(pool: &PgPool) -> RepoResult<Vec<Program>> {
    let rows: Vec<ProgramRow> = sqlx::query_as(&format!(
        "SELECT {PROG_COLS} FROM programs ORDER BY program_identifier"
    ))
    .fetch_all(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(rows.into_iter().map(Into::into).collect())
}
