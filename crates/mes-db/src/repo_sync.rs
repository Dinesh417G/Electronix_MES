//! M12 repositories — the outbox change-feed, idempotent apply, org/plant
//! multi-tenancy + enrollment, and remote work-order commands (§7, §8.3, §12 M12).
//!
//! Apply is idempotent: an entry id present in `applied_entries` is a no-op, so
//! a replayed batch (after a 24h+ outage) never double-applies. The edge writes
//! outbox rows in the same transaction as the write they describe (see
//! `repo_orders::create_work_order`).

use chrono::{DateTime, Utc};
use mes_client::sync::{Org, PlantSummary, SyncEntry};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgExecutor, PgPool, Postgres, Transaction};

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

/// SHA-256 hex of an enrollment token (§14 — hashed at rest).
pub fn hash_token(token: &str) -> String {
    let mut h = Sha256::new();
    h.update(token.as_bytes());
    hex(&h.finalize())
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

// ---- Outbox (source side) ------------------------------------------------

/// Append an outbox entry within an existing transaction (§8.3 — same tx as the
/// write it describes). `destination` NULL = bound for the cloud.
pub async fn enqueue<'e, E>(
    exec: E,
    aggregate: &str,
    entity_id: &str,
    op: &str,
    payload: &Value,
    destination: Option<&str>,
) -> RepoResult<String>
where
    E: PgExecutor<'e>,
{
    let id = nid();
    sqlx::query(
        "INSERT INTO outbox (id, aggregate, entity_id, op, payload, destination)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(&id)
    .bind(aggregate)
    .bind(entity_id)
    .bind(op)
    .bind(sqlx::types::Json(payload))
    .bind(destination)
    .execute(exec)
    .await
    .map_err(map_sqlx)?;
    Ok(id)
}

#[derive(sqlx::FromRow)]
struct OutboxRow {
    id: String,
    aggregate: String,
    entity_id: String,
    op: String,
    payload: sqlx::types::Json<Value>,
}

impl From<OutboxRow> for SyncEntry {
    fn from(r: OutboxRow) -> Self {
        SyncEntry {
            id: r.id,
            aggregate: r.aggregate,
            entity_id: r.entity_id,
            op: r.op,
            payload: r.payload.0,
        }
    }
}

/// Fetch a batch of un-synced, cloud-bound entries (edge → cloud), oldest first.
pub async fn fetch_to_cloud(pool: &PgPool, limit: i64) -> RepoResult<Vec<SyncEntry>> {
    let rows: Vec<OutboxRow> = sqlx::query_as(
        "SELECT id, aggregate, entity_id, op, payload FROM outbox
         WHERE destination IS NULL AND synced_at IS NULL
         ORDER BY created_at
         LIMIT $1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// Mark outbox entries as synced (acked by the peer).
pub async fn mark_synced(pool: &PgPool, ids: &[String]) -> RepoResult<u64> {
    if ids.is_empty() {
        return Ok(0);
    }
    let res = sqlx::query("UPDATE outbox SET synced_at = now() WHERE id = ANY($1)")
        .bind(ids)
        .execute(pool)
        .await
        .map_err(map_sqlx)?;
    Ok(res.rows_affected())
}

// ---- Idempotent apply (destination side) ---------------------------------

#[derive(Debug, Deserialize)]
struct WoPayload {
    id: String,
    wo_number: String,
    part_id: String,
    routing_id: Option<String>,
    qty_ordered: Decimal,
    #[serde(default = "default_priority")]
    priority: i32,
    #[serde(default = "draft")]
    status: String,
    planned_start: Option<DateTime<Utc>>,
    planned_end: Option<DateTime<Utc>>,
}

fn default_priority() -> i32 {
    100
}
fn draft() -> String {
    "draft".to_string()
}

async fn apply_work_order(tx: &mut Transaction<'_, Postgres>, payload: &Value) -> RepoResult<()> {
    let wo: WoPayload = serde_json::from_value(payload.clone())
        .map_err(|e| RepoError::InvalidReference(format!("bad work_order payload: {e}")))?;
    sqlx::query(
        "INSERT INTO work_orders
             (id, wo_number, part_id, routing_id, qty_ordered, priority, status,
              planned_start, planned_end)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         ON CONFLICT (id) DO UPDATE SET
             wo_number = EXCLUDED.wo_number,
             qty_ordered = EXCLUDED.qty_ordered,
             priority = EXCLUDED.priority,
             status = EXCLUDED.status,
             planned_start = EXCLUDED.planned_start,
             planned_end = EXCLUDED.planned_end,
             updated_at = now()",
    )
    .bind(&wo.id)
    .bind(&wo.wo_number)
    .bind(&wo.part_id)
    .bind(&wo.routing_id)
    .bind(wo.qty_ordered)
    .bind(wo.priority)
    .bind(&wo.status)
    .bind(wo.planned_start)
    .bind(wo.planned_end)
    .execute(&mut **tx)
    .await
    .map_err(map_sqlx)?;
    Ok(())
}

/// Apply a single entry idempotently. Returns `true` if newly applied, `false`
/// if its id was already applied (a no-op replay). An unknown aggregate is
/// recorded as applied and otherwise ignored (forward-compatible).
pub async fn apply_entry(pool: &PgPool, entry: &SyncEntry) -> RepoResult<bool> {
    let mut tx = pool.begin().await.map_err(map_sqlx)?;

    let claimed =
        sqlx::query("INSERT INTO applied_entries (id) VALUES ($1) ON CONFLICT DO NOTHING")
            .bind(&entry.id)
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx)?;
    if claimed.rows_affected() == 0 {
        tx.commit().await.map_err(map_sqlx)?;
        return Ok(false);
    }

    match entry.aggregate.as_str() {
        "work_order" => apply_work_order(&mut tx, &entry.payload).await?,
        _ => {
            tracing::warn!(aggregate = %entry.aggregate, "unknown sync aggregate; recorded, ignored");
        }
    }

    tx.commit().await.map_err(map_sqlx)?;
    Ok(true)
}

/// Apply a whole batch, returning (applied, skipped).
pub async fn apply_batch(pool: &PgPool, entries: &[SyncEntry]) -> RepoResult<(usize, usize)> {
    let mut applied = 0;
    let mut skipped = 0;
    for e in entries {
        if apply_entry(pool, e).await? {
            applied += 1;
        } else {
            skipped += 1;
        }
    }
    Ok((applied, skipped))
}

// ---- Orgs / plants / enrollment ------------------------------------------

pub async fn create_org(pool: &PgPool, code: &str, name: &str) -> RepoResult<Org> {
    let (id, code, name, created_at): (String, String, String, DateTime<Utc>) = sqlx::query_as(
        "INSERT INTO orgs (id, code, name) VALUES ($1, $2, $3)
         RETURNING id, code, name, created_at",
    )
    .bind(nid())
    .bind(code)
    .bind(name)
    .fetch_one(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(Org {
        id,
        code,
        name,
        created_at,
    })
}

/// Enroll a plant under an org: mints a token, stores its hash, returns the
/// plaintext once. Returns (plant_id, code, name, token).
pub async fn enroll_plant(
    pool: &PgPool,
    org_id: &str,
    code: &str,
    name: &str,
) -> RepoResult<(String, String)> {
    let id = nid();
    let token = format!("plant_{}", mes_core::new_id());
    sqlx::query(
        "INSERT INTO plants (id, org_id, code, name, enrollment_token_hash, enrolled_at)
         VALUES ($1, $2, $3, $4, $5, now())",
    )
    .bind(&id)
    .bind(org_id)
    .bind(code)
    .bind(name)
    .bind(hash_token(&token))
    .execute(pool)
    .await
    .map_err(map_sqlx)?;
    Ok((id, token))
}

/// Verify a plant's bearer token against the stored hash.
pub async fn verify_plant_token(pool: &PgPool, plant_id: &str, token: &str) -> RepoResult<bool> {
    let row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT enrollment_token_hash FROM plants WHERE id = $1")
            .bind(plant_id)
            .fetch_optional(pool)
            .await
            .map_err(map_sqlx)?;
    Ok(match row {
        Some((Some(hash),)) => hash == hash_token(token),
        _ => false,
    })
}

pub async fn touch_plant_sync(pool: &PgPool, plant_id: &str) -> RepoResult<()> {
    sqlx::query("UPDATE plants SET last_sync_at = now(), updated_at = now() WHERE id = $1")
        .bind(plant_id)
        .execute(pool)
        .await
        .map_err(map_sqlx)?;
    Ok(())
}

type PlantRow = (
    String,
    String,
    String,
    String,
    Option<String>,
    Option<DateTime<Utc>>,
);

pub async fn list_plants(pool: &PgPool) -> RepoResult<Vec<PlantSummary>> {
    let rows: Vec<PlantRow> = sqlx::query_as(
        "SELECT id, org_id, code, name, enrollment_token_hash, last_sync_at
             FROM plants ORDER BY created_at",
    )
    .fetch_all(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(rows
        .into_iter()
        .map(
            |(id, org_id, code, name, hash, last_sync_at)| PlantSummary {
                id,
                org_id,
                code,
                name,
                enrolled: hash.is_some(),
                last_sync_at,
            },
        )
        .collect())
}

// ---- Remote commands (cloud → edge) --------------------------------------

/// Pull pending command entries destined for a plant, oldest first.
pub async fn pull_for_plant(
    pool: &PgPool,
    plant_id: &str,
    limit: i64,
) -> RepoResult<Vec<SyncEntry>> {
    let rows: Vec<OutboxRow> = sqlx::query_as(
        "SELECT id, aggregate, entity_id, op, payload FROM outbox
         WHERE destination = $1 AND synced_at IS NULL
         ORDER BY created_at
         LIMIT $2",
    )
    .bind(plant_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// Create a work order on the cloud and enqueue it as a command for a plant.
/// The cloud gets the row immediately; the edge receives it on its next pull.
#[allow(clippy::too_many_arguments)]
pub async fn create_remote_work_order(
    pool: &PgPool,
    plant_id: &str,
    wo_number: &str,
    part_id: &str,
    qty_ordered: Decimal,
    priority: Option<i32>,
) -> RepoResult<String> {
    let mut tx = pool.begin().await.map_err(map_sqlx)?;

    let wo_id = nid();
    sqlx::query(
        "INSERT INTO work_orders (id, wo_number, part_id, qty_ordered, priority)
         VALUES ($1, $2, $3, $4, COALESCE($5, 100))",
    )
    .bind(&wo_id)
    .bind(wo_number)
    .bind(part_id)
    .bind(qty_ordered)
    .bind(priority)
    .execute(&mut *tx)
    .await
    .map_err(map_sqlx)?;

    let payload = serde_json::json!({
        "id": wo_id,
        "wo_number": wo_number,
        "part_id": part_id,
        "qty_ordered": qty_ordered,
        "priority": priority.unwrap_or(100),
        "status": "draft",
    });
    enqueue(
        &mut *tx,
        "work_order",
        &wo_id,
        "upsert",
        &payload,
        Some(plant_id),
    )
    .await?;

    tx.commit().await.map_err(map_sqlx)?;
    Ok(wo_id)
}
