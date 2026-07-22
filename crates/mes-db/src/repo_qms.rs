//! M8 repositories — inspection plans/characteristics/results, auto-NCR + hold
//! on fail, and the NCR disposition lifecycle (§8, §12 M8).

use chrono::{DateTime, Utc};
use mes_client::qms::{Characteristic, InspectionResult, Ncr, Plan};
use mes_core::qms::{evaluate, Disposition, InspectionOutcome};
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

// ---- Plans / characteristics --------------------------------------------

#[derive(sqlx::FromRow)]
struct PlanRow {
    id: String,
    part_id: String,
    code: String,
    name: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<PlanRow> for Plan {
    fn from(r: PlanRow) -> Self {
        Plan {
            id: r.id,
            part_id: r.part_id,
            code: r.code,
            name: r.name,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

pub async fn create_plan(pool: &PgPool, part_id: &str, code: &str, name: &str) -> RepoResult<Plan> {
    let row: PlanRow = sqlx::query_as(
        "INSERT INTO inspection_plans (id, part_id, code, name)
         VALUES ($1, $2, $3, $4)
         RETURNING id, part_id, code, name, created_at, updated_at",
    )
    .bind(nid())
    .bind(part_id)
    .bind(code)
    .bind(name)
    .fetch_one(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(row.into())
}

#[derive(sqlx::FromRow)]
struct CharRow {
    id: String,
    plan_id: String,
    name: String,
    uom: Option<String>,
    lower_limit: Option<Decimal>,
    upper_limit: Option<Decimal>,
}

impl From<CharRow> for Characteristic {
    fn from(r: CharRow) -> Self {
        Characteristic {
            id: r.id,
            plan_id: r.plan_id,
            name: r.name,
            uom: r.uom,
            lower_limit: r.lower_limit,
            upper_limit: r.upper_limit,
        }
    }
}

pub async fn create_characteristic(
    pool: &PgPool,
    plan_id: &str,
    name: &str,
    uom: Option<&str>,
    nominal: Option<Decimal>,
    lower: Option<Decimal>,
    upper: Option<Decimal>,
) -> RepoResult<Characteristic> {
    let row: CharRow = sqlx::query_as(
        "INSERT INTO characteristics (id, plan_id, name, uom, nominal, lower_limit, upper_limit)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING id, plan_id, name, uom, lower_limit, upper_limit",
    )
    .bind(nid())
    .bind(plan_id)
    .bind(name)
    .bind(uom)
    .bind(nominal)
    .bind(lower)
    .bind(upper)
    .fetch_one(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(row.into())
}

// ---- Inspection results + auto-NCR --------------------------------------

#[derive(sqlx::FromRow)]
struct NcrRow {
    id: String,
    ncr_no: String,
    inspection_result_id: Option<String>,
    lot_id: Option<String>,
    serial_id: Option<String>,
    part_id: Option<String>,
    status: String,
    disposition: Option<String>,
    reason: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<NcrRow> for Ncr {
    fn from(r: NcrRow) -> Self {
        Ncr {
            id: r.id,
            ncr_no: r.ncr_no,
            inspection_result_id: r.inspection_result_id,
            lot_id: r.lot_id,
            serial_id: r.serial_id,
            part_id: r.part_id,
            status: r.status,
            disposition: r.disposition,
            reason: r.reason,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

const NCR_COLS: &str = "id, ncr_no, inspection_result_id, lot_id, serial_id, part_id, status, \
     disposition, reason, created_at, updated_at";

/// Record an inspection measurement. Pass/fail is evaluated server-side against
/// the characteristic's limits; a fail atomically raises an NCR and places a
/// hold on the associated lot/serial (§8). Returns the result and any NCR.
#[allow(clippy::too_many_arguments)]
pub async fn record_result(
    pool: &PgPool,
    characteristic_id: &str,
    lot_id: Option<&str>,
    serial_id: Option<&str>,
    wo_operation_id: Option<&str>,
    measured_value: Decimal,
    inspected_by: &str,
) -> RepoResult<(InspectionResult, Option<Ncr>)> {
    // Limits for evaluation.
    let (lower, upper): (Option<Decimal>, Option<Decimal>) =
        sqlx::query_as("SELECT lower_limit, upper_limit FROM characteristics WHERE id = $1")
            .bind(characteristic_id)
            .fetch_optional(pool)
            .await
            .map_err(map_sqlx)?
            .ok_or(RepoError::NotFound)?;

    let outcome = evaluate(measured_value, lower, upper);

    let mut tx = pool.begin().await.map_err(map_sqlx)?;

    let result_id = nid();
    sqlx::query(
        "INSERT INTO inspection_results
             (id, characteristic_id, lot_id, serial_id, wo_operation_id, measured_value, result, inspected_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(&result_id)
    .bind(characteristic_id)
    .bind(lot_id)
    .bind(serial_id)
    .bind(wo_operation_id)
    .bind(measured_value)
    .bind(outcome.as_str())
    .bind(inspected_by)
    .execute(&mut *tx)
    .await
    .map_err(map_sqlx)?;

    let mut ncr: Option<Ncr> = None;
    if outcome == InspectionOutcome::Fail {
        // Resolve part from the lot when present (best-effort).
        let part_id: Option<String> = match lot_id {
            Some(l) => sqlx::query_as::<_, (String,)>("SELECT part_id FROM lots WHERE id = $1")
                .bind(l)
                .fetch_optional(&mut *tx)
                .await
                .map_err(map_sqlx)?
                .map(|(p,)| p),
            None => None,
        };

        let ncr_id = nid();
        let ncr_no = format!("NCR-{}", &ncr_id[..10]);
        let row: NcrRow = sqlx::query_as(&format!(
            "INSERT INTO ncrs (id, ncr_no, inspection_result_id, lot_id, serial_id, part_id, reason)
             VALUES ($1, $2, $3, $4, $5, $6, 'inspection failure')
             RETURNING {NCR_COLS}"
        ))
        .bind(&ncr_id)
        .bind(&ncr_no)
        .bind(&result_id)
        .bind(lot_id)
        .bind(serial_id)
        .bind(&part_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_sqlx)?;

        // Place a hold (linked to the NCR) on the lot and/or serial.
        for (etype, eid) in [("lot", lot_id), ("serial", serial_id)] {
            if let Some(id) = eid {
                sqlx::query(
                    "INSERT INTO holds (id, entity_type, entity_id, reason, created_by, ncr_id)
                     VALUES ($1, $2, $3, 'NCR auto-hold', $4, $5)",
                )
                .bind(nid())
                .bind(etype)
                .bind(id)
                .bind(inspected_by)
                .bind(&ncr_id)
                .execute(&mut *tx)
                .await
                .map_err(map_sqlx)?;
            }
        }
        ncr = Some(row.into());
    }

    tx.commit().await.map_err(map_sqlx)?;

    let result = InspectionResult {
        id: result_id,
        characteristic_id: characteristic_id.to_string(),
        lot_id: lot_id.map(str::to_string),
        serial_id: serial_id.map(str::to_string),
        measured_value,
        result: outcome.as_str().to_string(),
        created_at: Utc::now(),
    };
    Ok((result, ncr))
}

// ---- NCR queries + disposition ------------------------------------------

pub async fn get_ncr(pool: &PgPool, id: &str) -> RepoResult<Ncr> {
    let row: NcrRow = sqlx::query_as(&format!("SELECT {NCR_COLS} FROM ncrs WHERE id = $1"))
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(map_sqlx)?
        .ok_or(RepoError::NotFound)?;
    Ok(row.into())
}

pub async fn list_ncrs(pool: &PgPool) -> RepoResult<Vec<Ncr>> {
    let rows: Vec<NcrRow> = sqlx::query_as(&format!(
        "SELECT {NCR_COLS} FROM ncrs ORDER BY created_at DESC"
    ))
    .fetch_all(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// Apply a disposition to an open NCR. If the disposition releases the hold
/// (Rework / Use-As-Is), the NCR's active holds are released atomically (§8 —
/// "Rework disposition releases correctly").
pub async fn disposition_ncr(
    pool: &PgPool,
    id: &str,
    disposition: Disposition,
    reason: Option<&str>,
    actor: &str,
) -> RepoResult<Ncr> {
    let mut tx = pool.begin().await.map_err(map_sqlx)?;

    let row: NcrRow = sqlx::query_as(&format!(
        "UPDATE ncrs
         SET status = 'dispositioned', disposition = $2, reason = COALESCE($3, reason),
             dispositioned_by = $4, dispositioned_at = now(), updated_at = now()
         WHERE id = $1 AND status = 'open'
         RETURNING {NCR_COLS}"
    ))
    .bind(id)
    .bind(disposition.as_str())
    .bind(reason)
    .bind(actor)
    .fetch_optional(&mut *tx)
    .await
    .map_err(map_sqlx)?
    .ok_or(RepoError::NotFound)?;

    if disposition.releases_hold() {
        sqlx::query(
            "UPDATE holds SET status = 'released', released_by = $2, released_at = now()
             WHERE ncr_id = $1 AND status = 'active'",
        )
        .bind(id)
        .bind(actor)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;
    }

    tx.commit().await.map_err(map_sqlx)?;
    Ok(row.into())
}
