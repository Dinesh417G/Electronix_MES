//! M7 repositories — lots/serials, genealogy, material issue (hold-checked),
//! holds, and recursive forward/backward trace (§7, §8, §12 M7).

use chrono::{DateTime, Utc};
use mes_client::trace::{Lot, Serial, TraceNode};
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

// ---- Lots / serials ------------------------------------------------------

#[derive(sqlx::FromRow)]
struct LotRow {
    id: String,
    lot_no: String,
    part_id: String,
    qty: Decimal,
    uom: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<LotRow> for Lot {
    fn from(r: LotRow) -> Self {
        Lot {
            id: r.id,
            lot_no: r.lot_no,
            part_id: r.part_id,
            qty: r.qty,
            uom: r.uom,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

pub async fn create_lot(
    pool: &PgPool,
    lot_no: &str,
    part_id: &str,
    qty: Decimal,
    uom: &str,
) -> RepoResult<Lot> {
    let row: LotRow = sqlx::query_as(
        "INSERT INTO lots (id, lot_no, part_id, qty, uom)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING id, lot_no, part_id, qty, uom, created_at, updated_at",
    )
    .bind(nid())
    .bind(lot_no)
    .bind(part_id)
    .bind(qty)
    .bind(uom)
    .fetch_one(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(row.into())
}

#[derive(sqlx::FromRow)]
struct SerialRow {
    id: String,
    serial_no: String,
    part_id: String,
    lot_id: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<SerialRow> for Serial {
    fn from(r: SerialRow) -> Self {
        Serial {
            id: r.id,
            serial_no: r.serial_no,
            part_id: r.part_id,
            lot_id: r.lot_id,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

pub async fn create_serial(
    pool: &PgPool,
    serial_no: &str,
    part_id: &str,
    lot_id: Option<&str>,
) -> RepoResult<Serial> {
    let row: SerialRow = sqlx::query_as(
        "INSERT INTO serials (id, serial_no, part_id, lot_id)
         VALUES ($1, $2, $3, $4)
         RETURNING id, serial_no, part_id, lot_id, created_at, updated_at",
    )
    .bind(nid())
    .bind(serial_no)
    .bind(part_id)
    .bind(lot_id)
    .fetch_one(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(row.into())
}

// ---- Genealogy -----------------------------------------------------------

pub async fn add_genealogy(
    pool: &PgPool,
    parent_type: &str,
    parent_id: &str,
    child_type: &str,
    child_id: &str,
    qty: Option<Decimal>,
) -> RepoResult<()> {
    sqlx::query(
        "INSERT INTO genealogy (id, parent_type, parent_id, child_type, child_id, qty)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (parent_type, parent_id, child_type, child_id) DO NOTHING",
    )
    .bind(nid())
    .bind(parent_type)
    .bind(parent_id)
    .bind(child_type)
    .bind(child_id)
    .bind(qty)
    .execute(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(())
}

fn to_nodes(rows: Vec<(String, String, Option<String>, i32)>) -> Vec<TraceNode> {
    rows.into_iter()
        .map(|(entity_type, entity_id, ref_no, depth)| TraceNode {
            entity_type,
            entity_id,
            ref_no,
            depth,
        })
        .collect()
}

/// Backward trace: every component consumed (directly or transitively) by the
/// given assembly, deepest-first-friendly (ordered by depth). Cycle-guarded.
pub async fn trace_backward(
    pool: &PgPool,
    entity_type: &str,
    entity_id: &str,
) -> RepoResult<Vec<TraceNode>> {
    let rows: Vec<(String, String, Option<String>, i32)> = sqlx::query_as(
        "WITH RECURSIVE tr(entity_type, entity_id, depth) AS (
             SELECT child_type, child_id, 1
             FROM genealogy WHERE parent_type = $1 AND parent_id = $2
             UNION ALL
             SELECT g.child_type, g.child_id, tr.depth + 1
             FROM genealogy g
             JOIN tr ON g.parent_type = tr.entity_type AND g.parent_id = tr.entity_id
             WHERE tr.depth < 64
         )
         SELECT t.entity_type, t.entity_id,
                COALESCE(l.lot_no, s.serial_no) AS ref_no,
                MIN(t.depth)::int AS depth
         FROM tr t
         LEFT JOIN lots l ON t.entity_type = 'lot' AND l.id = t.entity_id
         LEFT JOIN serials s ON t.entity_type = 'serial' AND s.id = t.entity_id
         GROUP BY t.entity_type, t.entity_id, ref_no
         ORDER BY depth, ref_no",
    )
    .bind(entity_type)
    .bind(entity_id)
    .fetch_all(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(to_nodes(rows))
}

/// Forward trace: every assembly the given component ended up in, transitively.
pub async fn trace_forward(
    pool: &PgPool,
    entity_type: &str,
    entity_id: &str,
) -> RepoResult<Vec<TraceNode>> {
    let rows: Vec<(String, String, Option<String>, i32)> = sqlx::query_as(
        "WITH RECURSIVE tr(entity_type, entity_id, depth) AS (
             SELECT parent_type, parent_id, 1
             FROM genealogy WHERE child_type = $1 AND child_id = $2
             UNION ALL
             SELECT g.parent_type, g.parent_id, tr.depth + 1
             FROM genealogy g
             JOIN tr ON g.child_type = tr.entity_type AND g.child_id = tr.entity_id
             WHERE tr.depth < 64
         )
         SELECT t.entity_type, t.entity_id,
                COALESCE(l.lot_no, s.serial_no) AS ref_no,
                MIN(t.depth)::int AS depth
         FROM tr t
         LEFT JOIN lots l ON t.entity_type = 'lot' AND l.id = t.entity_id
         LEFT JOIN serials s ON t.entity_type = 'serial' AND s.id = t.entity_id
         GROUP BY t.entity_type, t.entity_id, ref_no
         ORDER BY depth, ref_no",
    )
    .bind(entity_type)
    .bind(entity_id)
    .fetch_all(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(to_nodes(rows))
}

// ---- Holds ---------------------------------------------------------------

/// Is the entity currently under an active hold?
pub async fn is_held(pool: &PgPool, entity_type: &str, entity_id: &str) -> RepoResult<bool> {
    let (held,): (bool,) = sqlx::query_as(
        "SELECT EXISTS(
             SELECT 1 FROM holds
             WHERE entity_type = $1 AND entity_id = $2 AND status = 'active'
         )",
    )
    .bind(entity_type)
    .bind(entity_id)
    .fetch_one(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(held)
}

pub async fn place_hold(
    pool: &PgPool,
    entity_type: &str,
    entity_id: &str,
    reason: Option<&str>,
    created_by: &str,
) -> RepoResult<String> {
    let id = nid();
    sqlx::query(
        "INSERT INTO holds (id, entity_type, entity_id, reason, created_by)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(&id)
    .bind(entity_type)
    .bind(entity_id)
    .bind(reason)
    .bind(created_by)
    .execute(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(id)
}

pub async fn release_hold(pool: &PgPool, hold_id: &str, released_by: &str) -> RepoResult<()> {
    let res = sqlx::query(
        "UPDATE holds SET status = 'released', released_by = $2, released_at = now()
         WHERE id = $1 AND status = 'active'",
    )
    .bind(hold_id)
    .bind(released_by)
    .execute(pool)
    .await
    .map_err(map_sqlx)?;
    if res.rows_affected() == 0 {
        return Err(RepoError::NotFound);
    }
    Ok(())
}

// ---- Material issue (hold-checked) --------------------------------------

/// Issue material. A lot (or serial) under an active hold cannot be issued
/// (§12 M7 acceptance) — returns `Conflict`.
#[allow(clippy::too_many_arguments)]
pub async fn issue_material(
    pool: &PgPool,
    lot_id: Option<&str>,
    serial_id: Option<&str>,
    qty: Decimal,
    wo_operation_id: Option<&str>,
    user_id: &str,
) -> RepoResult<String> {
    if let Some(l) = lot_id {
        if is_held(pool, "lot", l).await? {
            return Err(RepoError::Conflict("lot is on hold".to_string()));
        }
    }
    if let Some(s) = serial_id {
        if is_held(pool, "serial", s).await? {
            return Err(RepoError::Conflict("serial is on hold".to_string()));
        }
    }

    let id = nid();
    sqlx::query(
        "INSERT INTO material_txns (id, lot_id, serial_id, txn_type, qty, wo_operation_id, user_id)
         VALUES ($1, $2, $3, 'issue', $4, $5, $6)",
    )
    .bind(&id)
    .bind(lot_id)
    .bind(serial_id)
    .bind(qty)
    .bind(wo_operation_id)
    .bind(user_id)
    .execute(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(id)
}
