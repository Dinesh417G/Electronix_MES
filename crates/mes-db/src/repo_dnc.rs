//! M4 repositories — DNC transfer events, program revisions, and the
//! next-operation program resolution that drives auto-scheduling (§8.4).

use chrono::{DateTime, Utc};
use mes_client::dnc::{ProgramRevision, TransferEvent};
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
// Program resolution for auto-scheduling (§8.4)
// ===========================================================================

/// The program to transfer for the next queued operation after one completes.
#[derive(Debug, Clone)]
pub struct NextTransfer {
    pub next_op_id: String,
    pub program_id: String,
    pub program_identifier: String,
    pub target_machine: Option<String>,
}

/// After `completed_op_id` completes, find the next pending operation in the
/// same work order (by `op_no`) whose program resolves — preferring a program
/// bound to that operation's routing op, falling back to the part's program
/// (§8.4). Returns `None` when there's no next op or no resolvable program.
pub async fn resolve_next_transfer(
    pool: &PgPool,
    completed_op_id: &str,
) -> RepoResult<Option<NextTransfer>> {
    // Locate the completed op's order + position.
    let Some((work_order_id, op_no)): Option<(String, i32)> =
        sqlx::query_as("SELECT work_order_id, op_no FROM wo_operations WHERE id = $1")
            .bind(completed_op_id)
            .fetch_optional(pool)
            .await
            .map_err(map_sqlx)?
    else {
        return Ok(None);
    };

    // Next pending operation in the order.
    let Some((next_op_id, next_routing_op)): Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT id, routing_op_id FROM wo_operations
         WHERE work_order_id = $1 AND op_no > $2 AND status = 'pending'
         ORDER BY op_no LIMIT 1",
    )
    .bind(&work_order_id)
    .bind(op_no)
    .fetch_optional(pool)
    .await
    .map_err(map_sqlx)?
    else {
        return Ok(None);
    };

    // Resolve a program: prefer one bound to the routing op, else the part's.
    let program: Option<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT p.id, p.program_identifier, p.target_machine
         FROM programs p
         JOIN work_orders wo ON wo.id = $1
         WHERE ($2::text IS NOT NULL AND p.routing_op_id = $2)
            OR p.part_id = wo.part_id
         ORDER BY (p.routing_op_id IS NOT DISTINCT FROM $2) DESC
         LIMIT 1",
    )
    .bind(&work_order_id)
    .bind(&next_routing_op)
    .fetch_optional(pool)
    .await
    .map_err(map_sqlx)?;

    Ok(program.map(
        |(program_id, program_identifier, target_machine)| NextTransfer {
            next_op_id,
            program_id,
            program_identifier,
            target_machine,
        },
    ))
}

/// Minimal program lookup: `(program_identifier, target_machine)` by id.
pub async fn program_dispatch_info(
    pool: &PgPool,
    program_id: &str,
) -> RepoResult<Option<(String, Option<String>)>> {
    let row: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT program_identifier, target_machine FROM programs WHERE id = $1")
            .bind(program_id)
            .fetch_optional(pool)
            .await
            .map_err(map_sqlx)?;
    Ok(row)
}

/// Resolve a program id from the identifier the daemon reports (§8.4 —
/// `program_received`). Returns the most recent match if several exist.
pub async fn find_program_id_by_identifier(
    pool: &PgPool,
    identifier: &str,
) -> RepoResult<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM programs WHERE program_identifier = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(identifier)
    .fetch_optional(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(row.map(|(id,)| id))
}

// ===========================================================================
// Transfer events
// ===========================================================================

#[derive(sqlx::FromRow)]
struct TransferRow {
    id: String,
    wo_operation_id: Option<String>,
    program_id: String,
    direction: String,
    status: String,
    dnc_daemon_ref: Option<String>,
    triggered_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<TransferRow> for TransferEvent {
    fn from(r: TransferRow) -> Self {
        TransferEvent {
            id: r.id,
            wo_operation_id: r.wo_operation_id,
            program_id: r.program_id,
            direction: r.direction,
            status: r.status,
            dnc_daemon_ref: r.dnc_daemon_ref,
            triggered_at: r.triggered_at,
            completed_at: r.completed_at,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

const TRANSFER_COLS: &str = "id, wo_operation_id, program_id, direction, status, dnc_daemon_ref, \
     triggered_at, completed_at, created_at, updated_at";

pub async fn create_transfer(
    pool: &PgPool,
    wo_operation_id: Option<&str>,
    program_id: &str,
    direction: &str,
    dnc_daemon_ref: Option<&str>,
) -> RepoResult<TransferEvent> {
    let row: TransferRow = sqlx::query_as(&format!(
        "INSERT INTO dnc_transfer_events (id, wo_operation_id, program_id, direction, dnc_daemon_ref)
         VALUES ($1, $2, $3, $4, $5) RETURNING {TRANSFER_COLS}"
    ))
    .bind(nid())
    .bind(wo_operation_id)
    .bind(program_id)
    .bind(direction)
    .bind(dnc_daemon_ref)
    .fetch_one(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(row.into())
}

pub async fn get_transfer(pool: &PgPool, id: &str) -> RepoResult<TransferEvent> {
    let row: TransferRow = sqlx::query_as(&format!(
        "SELECT {TRANSFER_COLS} FROM dnc_transfer_events WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(map_sqlx)?
    .ok_or(RepoError::NotFound)?;
    Ok(row.into())
}

pub async fn list_transfers(pool: &PgPool) -> RepoResult<Vec<TransferEvent>> {
    let rows: Vec<TransferRow> = sqlx::query_as(&format!(
        "SELECT {TRANSFER_COLS} FROM dnc_transfer_events ORDER BY triggered_at DESC"
    ))
    .fetch_all(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// Find an open transfer by the daemon's correlation reference (for matching
/// completion/failure events back to the transfer MES scheduled).
pub async fn find_open_transfer_by_ref(
    pool: &PgPool,
    dnc_daemon_ref: &str,
) -> RepoResult<Option<TransferEvent>> {
    let row: Option<TransferRow> = sqlx::query_as(&format!(
        "SELECT {TRANSFER_COLS} FROM dnc_transfer_events
         WHERE dnc_daemon_ref = $1 AND status NOT IN ('completed', 'failed')
         ORDER BY triggered_at DESC LIMIT 1"
    ))
    .bind(dnc_daemon_ref)
    .fetch_optional(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(row.map(Into::into))
}

/// Set a transfer's status. `completed_at` is stamped for terminal states.
pub async fn set_transfer_status(
    pool: &PgPool,
    id: &str,
    status: &str,
) -> RepoResult<TransferEvent> {
    let terminal = matches!(status, "completed" | "failed");
    let row: TransferRow = sqlx::query_as(&format!(
        "UPDATE dnc_transfer_events
         SET status = $2,
             completed_at = CASE WHEN $3 THEN now() ELSE completed_at END,
             updated_at = now()
         WHERE id = $1 RETURNING {TRANSFER_COLS}"
    ))
    .bind(id)
    .bind(status)
    .bind(terminal)
    .fetch_optional(pool)
    .await
    .map_err(map_sqlx)?
    .ok_or(RepoError::NotFound)?;
    Ok(row.into())
}

// ===========================================================================
// Program revisions
// ===========================================================================

#[derive(sqlx::FromRow)]
struct RevisionRow {
    id: String,
    program_id: String,
    revision_no: i32,
    source: String,
    content_ref: Option<String>,
    status: String,
    submitted_by: Option<String>,
    submitted_at: DateTime<Utc>,
    promoted_by: Option<String>,
    promoted_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<RevisionRow> for ProgramRevision {
    fn from(r: RevisionRow) -> Self {
        ProgramRevision {
            id: r.id,
            program_id: r.program_id,
            revision_no: r.revision_no,
            source: r.source,
            content_ref: r.content_ref,
            status: r.status,
            submitted_by: r.submitted_by,
            submitted_at: r.submitted_at,
            promoted_by: r.promoted_by,
            promoted_at: r.promoted_at,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

const REV_COLS: &str = "id, program_id, revision_no, source, content_ref, status, submitted_by, \
     submitted_at, promoted_by, promoted_at, created_at, updated_at";

/// Create a **draft** program revision. `revision_no` auto-increments per
/// program. Never promoted here — a supervisor promotes it later (§3, §8.4).
pub async fn create_draft_revision(
    pool: &PgPool,
    program_id: &str,
    source: &str,
    content_ref: Option<&str>,
    submitted_by: Option<&str>,
) -> RepoResult<ProgramRevision> {
    let row: RevisionRow = sqlx::query_as(&format!(
        "INSERT INTO program_revisions (id, program_id, revision_no, source, content_ref, submitted_by)
         VALUES ($1, $2,
                 (SELECT COALESCE(MAX(revision_no), 0) + 1 FROM program_revisions WHERE program_id = $2),
                 $3, $4, $5)
         RETURNING {REV_COLS}"
    ))
    .bind(nid())
    .bind(program_id)
    .bind(source)
    .bind(content_ref)
    .bind(submitted_by)
    .fetch_one(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(row.into())
}

pub async fn get_revision(pool: &PgPool, id: &str) -> RepoResult<ProgramRevision> {
    let row: RevisionRow = sqlx::query_as(&format!(
        "SELECT {REV_COLS} FROM program_revisions WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(map_sqlx)?
    .ok_or(RepoError::NotFound)?;
    Ok(row.into())
}

pub async fn list_revisions(pool: &PgPool) -> RepoResult<Vec<ProgramRevision>> {
    let rows: Vec<RevisionRow> = sqlx::query_as(&format!(
        "SELECT {REV_COLS} FROM program_revisions ORDER BY submitted_at DESC"
    ))
    .fetch_all(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// Set a revision's status (promote/reject), stamping the promoter. Transition
/// validity is enforced by the caller via `mes_core::dnc::RevisionStatus`.
pub async fn set_revision_status(
    pool: &PgPool,
    id: &str,
    status: &str,
    actor: &str,
) -> RepoResult<ProgramRevision> {
    let row: RevisionRow = sqlx::query_as(&format!(
        "UPDATE program_revisions
         SET status = $2, promoted_by = $3, promoted_at = now(), updated_at = now()
         WHERE id = $1 RETURNING {REV_COLS}"
    ))
    .bind(id)
    .bind(status)
    .bind(actor)
    .fetch_optional(pool)
    .await
    .map_err(map_sqlx)?
    .ok_or(RepoError::NotFound)?;
    Ok(row.into())
}
