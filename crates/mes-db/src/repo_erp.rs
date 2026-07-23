//! M10 repositories — ERP connection config, the sync-log audit trail, and the
//! procurement→SentToErp transition (§7, §8, §12 M10).
//!
//! The stored `auth_token_enc` is ciphertext (§14); this layer never decrypts —
//! encryption/decryption is the caller's concern. Public reads return the
//! connection *without* the token (only `has_token`).

use chrono::{DateTime, Utc};
use mes_client::erp::{ErpConnection, ErpSyncLogEntry};
use serde_json::Value;
use sqlx::types::Json;
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

const CONN_COLS: &str = "id, site_id, name, endpoint_url, auth_token_enc, field_mapping, \
     direction, enabled, created_at, updated_at";

#[derive(sqlx::FromRow)]
struct ConnRow {
    id: String,
    site_id: Option<String>,
    name: String,
    endpoint_url: String,
    auth_token_enc: Option<String>,
    field_mapping: Json<Value>,
    direction: String,
    enabled: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl ConnRow {
    /// Public DTO — drops the token, exposing only whether one is set.
    fn into_public(self) -> ErpConnection {
        ErpConnection {
            id: self.id,
            site_id: self.site_id,
            name: self.name,
            endpoint_url: self.endpoint_url,
            has_token: self.auth_token_enc.is_some(),
            field_mapping: self.field_mapping.0,
            direction: self.direction,
            enabled: self.enabled,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

/// Everything a sync run needs, including the *encrypted* token for the caller
/// to decrypt.
pub struct ConnectionSecret {
    pub id: String,
    pub endpoint_url: String,
    pub auth_token_enc: Option<String>,
    pub field_mapping: Value,
    pub direction: String,
    pub enabled: bool,
}

// ---- Connection CRUD -----------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub async fn create_connection(
    pool: &PgPool,
    site_id: Option<&str>,
    name: &str,
    endpoint_url: &str,
    auth_token_enc: Option<&str>,
    field_mapping: &Value,
    direction: &str,
    enabled: bool,
) -> RepoResult<ErpConnection> {
    let row: ConnRow = sqlx::query_as(&format!(
        "INSERT INTO erp_connections
             (id, site_id, name, endpoint_url, auth_token_enc, field_mapping, direction, enabled)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING {CONN_COLS}"
    ))
    .bind(nid())
    .bind(site_id)
    .bind(name)
    .bind(endpoint_url)
    .bind(auth_token_enc)
    .bind(Json(field_mapping))
    .bind(direction)
    .bind(enabled)
    .fetch_one(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(row.into_public())
}

pub async fn get_connection(pool: &PgPool, id: &str) -> RepoResult<ErpConnection> {
    let row: ConnRow = sqlx::query_as(&format!(
        "SELECT {CONN_COLS} FROM erp_connections WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(map_sqlx)?
    .ok_or(RepoError::NotFound)?;
    Ok(row.into_public())
}

/// Internal: fetch the connection with its encrypted token, for a sync run.
pub async fn get_connection_secret(pool: &PgPool, id: &str) -> RepoResult<ConnectionSecret> {
    let row: ConnRow = sqlx::query_as(&format!(
        "SELECT {CONN_COLS} FROM erp_connections WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(map_sqlx)?
    .ok_or(RepoError::NotFound)?;
    Ok(ConnectionSecret {
        id: row.id,
        endpoint_url: row.endpoint_url,
        auth_token_enc: row.auth_token_enc,
        field_mapping: row.field_mapping.0,
        direction: row.direction,
        enabled: row.enabled,
    })
}

pub async fn list_connections(pool: &PgPool) -> RepoResult<Vec<ErpConnection>> {
    let rows: Vec<ConnRow> = sqlx::query_as(&format!(
        "SELECT {CONN_COLS} FROM erp_connections ORDER BY created_at"
    ))
    .fetch_all(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(rows.into_iter().map(ConnRow::into_public).collect())
}

/// Update a connection. A `None` `auth_token_enc` leaves the stored token
/// unchanged (so editing other fields never requires re-entering the token).
#[allow(clippy::too_many_arguments)]
pub async fn update_connection(
    pool: &PgPool,
    id: &str,
    site_id: Option<&str>,
    name: &str,
    endpoint_url: &str,
    auth_token_enc: Option<&str>,
    field_mapping: &Value,
    direction: &str,
    enabled: bool,
) -> RepoResult<ErpConnection> {
    let row: ConnRow = sqlx::query_as(&format!(
        "UPDATE erp_connections SET
             site_id = $2, name = $3, endpoint_url = $4,
             auth_token_enc = COALESCE($5, auth_token_enc),
             field_mapping = $6, direction = $7, enabled = $8, updated_at = now()
         WHERE id = $1
         RETURNING {CONN_COLS}"
    ))
    .bind(id)
    .bind(site_id)
    .bind(name)
    .bind(endpoint_url)
    .bind(auth_token_enc)
    .bind(Json(field_mapping))
    .bind(direction)
    .bind(enabled)
    .fetch_optional(pool)
    .await
    .map_err(map_sqlx)?
    .ok_or(RepoError::NotFound)?;
    Ok(row.into_public())
}

pub async fn delete_connection(pool: &PgPool, id: &str) -> RepoResult<()> {
    let res = sqlx::query("DELETE FROM erp_connections WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .map_err(map_sqlx)?;
    if res.rows_affected() == 0 {
        return Err(RepoError::NotFound);
    }
    Ok(())
}

// ---- Sync log ------------------------------------------------------------

pub async fn insert_sync_log(
    pool: &PgPool,
    connection_id: &str,
    direction: &str,
    entity: &str,
    record_count: i32,
    status: &str,
    detail: Option<&str>,
) -> RepoResult<String> {
    let id = nid();
    sqlx::query(
        "INSERT INTO erp_sync_log
             (id, connection_id, direction, entity, record_count, status, detail)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(&id)
    .bind(connection_id)
    .bind(direction)
    .bind(entity)
    .bind(record_count)
    .bind(status)
    .bind(detail)
    .execute(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(id)
}

#[derive(sqlx::FromRow)]
struct SyncLogRow {
    id: String,
    connection_id: Option<String>,
    direction: String,
    entity: String,
    record_count: i32,
    status: String,
    detail: Option<String>,
    ts: DateTime<Utc>,
}

impl From<SyncLogRow> for ErpSyncLogEntry {
    fn from(r: SyncLogRow) -> Self {
        ErpSyncLogEntry {
            id: r.id,
            connection_id: r.connection_id,
            direction: r.direction,
            entity: r.entity,
            record_count: r.record_count,
            status: r.status,
            detail: r.detail,
            ts: r.ts,
        }
    }
}

pub async fn list_sync_log(
    pool: &PgPool,
    connection_id: Option<&str>,
) -> RepoResult<Vec<ErpSyncLogEntry>> {
    let rows: Vec<SyncLogRow> = sqlx::query_as(
        "SELECT id, connection_id, direction, entity, record_count, status, detail, ts
         FROM erp_sync_log
         WHERE ($1::text IS NULL OR connection_id = $1)
         ORDER BY ts DESC",
    )
    .bind(connection_id)
    .fetch_all(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(rows.into_iter().map(Into::into).collect())
}

// ---- Procurement → SentToErp (§12 M10) -----------------------------------

/// Move the given procurement requests from `requested` to `sent_to_erp`,
/// stamping `pushed_at` and an optional ERP reference. Returns the number moved.
pub async fn mark_procurement_sent(
    pool: &PgPool,
    ids: &[String],
    erp_reference: Option<&str>,
) -> RepoResult<u64> {
    if ids.is_empty() {
        return Ok(0);
    }
    let res = sqlx::query(
        "UPDATE procurement_requests
         SET status = 'sent_to_erp', erp_reference = COALESCE($2, erp_reference),
             pushed_at = now(), updated_at = now()
         WHERE id = ANY($1) AND status = 'requested'",
    )
    .bind(ids)
    .bind(erp_reference)
    .execute(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(res.rows_affected())
}
