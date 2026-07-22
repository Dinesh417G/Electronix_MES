//! Repositories — the only place SQL is written for M1 master data + auth.
//!
//! Queries are runtime-checked (`query_as`) rather than compile-checked
//! (`query!`) so `cargo build` stays hermetic without a database (the Dockerfile
//! and offline dev depend on this). Correctness is proven by the integration
//! suite against dockerized TimescaleDB (§13). When a prepared-query cache is
//! introduced these can migrate to compile-checked form (§14).
//!
//! Every repo returns `mes-client` DTOs directly so the wire contract has one
//! definition; secret material (password/PIN hashes) never leaves this crate
//! except through the dedicated auth row below.

use chrono::{DateTime, Utc};
use mes_client::master::{Area, Part, Site, User, WorkCenter};
use sqlx::PgPool;
use thiserror::Error;

/// Errors from repository operations, mapped from raw sqlx errors so handlers
/// can translate them to HTTP status codes without matching on driver detail.
#[derive(Debug, Error)]
pub enum RepoError {
    #[error("not found")]
    NotFound,
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("invalid reference: {0}")]
    InvalidReference(String),
    #[error(transparent)]
    Db(sqlx::Error),
}

/// Map a raw sqlx error into a semantic `RepoError`.
fn map_sqlx(e: sqlx::Error) -> RepoError {
    match &e {
        sqlx::Error::RowNotFound => RepoError::NotFound,
        sqlx::Error::Database(db) => match db.code().as_deref() {
            // 23505 unique_violation, 23503 foreign_key_violation (PostgreSQL).
            Some("23505") => RepoError::Conflict(db.message().to_string()),
            Some("23503") => RepoError::InvalidReference(db.message().to_string()),
            _ => RepoError::Db(e),
        },
        _ => RepoError::Db(e),
    }
}

pub type RepoResult<T> = Result<T, RepoError>;

fn new_id() -> String {
    mes_core::new_id()
}

// ===========================================================================
// Sites
// ===========================================================================

#[derive(sqlx::FromRow)]
struct SiteRow {
    id: String,
    code: String,
    name: String,
    timezone: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<SiteRow> for Site {
    fn from(r: SiteRow) -> Self {
        Site {
            id: r.id,
            code: r.code,
            name: r.name,
            timezone: r.timezone,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

pub async fn create_site(
    pool: &PgPool,
    code: &str,
    name: &str,
    timezone: &str,
) -> RepoResult<Site> {
    let row: SiteRow = sqlx::query_as(
        "INSERT INTO sites (id, code, name, timezone)
         VALUES ($1, $2, $3, $4)
         RETURNING id, code, name, timezone, created_at, updated_at",
    )
    .bind(new_id())
    .bind(code)
    .bind(name)
    .bind(timezone)
    .fetch_one(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(row.into())
}

pub async fn list_sites(pool: &PgPool) -> RepoResult<Vec<Site>> {
    let rows: Vec<SiteRow> = sqlx::query_as(
        "SELECT id, code, name, timezone, created_at, updated_at
         FROM sites ORDER BY code",
    )
    .fetch_all(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(rows.into_iter().map(Into::into).collect())
}

pub async fn get_site(pool: &PgPool, id: &str) -> RepoResult<Site> {
    let row: SiteRow = sqlx::query_as(
        "SELECT id, code, name, timezone, created_at, updated_at
         FROM sites WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(map_sqlx)?
    .ok_or(RepoError::NotFound)?;
    Ok(row.into())
}

pub async fn update_site(
    pool: &PgPool,
    id: &str,
    code: &str,
    name: &str,
    timezone: &str,
) -> RepoResult<Site> {
    let row: SiteRow = sqlx::query_as(
        "UPDATE sites SET code = $2, name = $3, timezone = $4, updated_at = now()
         WHERE id = $1
         RETURNING id, code, name, timezone, created_at, updated_at",
    )
    .bind(id)
    .bind(code)
    .bind(name)
    .bind(timezone)
    .fetch_optional(pool)
    .await
    .map_err(map_sqlx)?
    .ok_or(RepoError::NotFound)?;
    Ok(row.into())
}

pub async fn delete_site(pool: &PgPool, id: &str) -> RepoResult<()> {
    let res = sqlx::query("DELETE FROM sites WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .map_err(map_sqlx)?;
    if res.rows_affected() == 0 {
        return Err(RepoError::NotFound);
    }
    Ok(())
}

// ===========================================================================
// Areas
// ===========================================================================

#[derive(sqlx::FromRow)]
struct AreaRow {
    id: String,
    site_id: String,
    code: String,
    name: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<AreaRow> for Area {
    fn from(r: AreaRow) -> Self {
        Area {
            id: r.id,
            site_id: r.site_id,
            code: r.code,
            name: r.name,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

pub async fn create_area(pool: &PgPool, site_id: &str, code: &str, name: &str) -> RepoResult<Area> {
    let row: AreaRow = sqlx::query_as(
        "INSERT INTO areas (id, site_id, code, name)
         VALUES ($1, $2, $3, $4)
         RETURNING id, site_id, code, name, created_at, updated_at",
    )
    .bind(new_id())
    .bind(site_id)
    .bind(code)
    .bind(name)
    .fetch_one(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(row.into())
}

pub async fn list_areas(pool: &PgPool) -> RepoResult<Vec<Area>> {
    let rows: Vec<AreaRow> = sqlx::query_as(
        "SELECT id, site_id, code, name, created_at, updated_at
         FROM areas ORDER BY code",
    )
    .fetch_all(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(rows.into_iter().map(Into::into).collect())
}

pub async fn get_area(pool: &PgPool, id: &str) -> RepoResult<Area> {
    let row: AreaRow = sqlx::query_as(
        "SELECT id, site_id, code, name, created_at, updated_at
         FROM areas WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(map_sqlx)?
    .ok_or(RepoError::NotFound)?;
    Ok(row.into())
}

pub async fn update_area(pool: &PgPool, id: &str, code: &str, name: &str) -> RepoResult<Area> {
    let row: AreaRow = sqlx::query_as(
        "UPDATE areas SET code = $2, name = $3, updated_at = now()
         WHERE id = $1
         RETURNING id, site_id, code, name, created_at, updated_at",
    )
    .bind(id)
    .bind(code)
    .bind(name)
    .fetch_optional(pool)
    .await
    .map_err(map_sqlx)?
    .ok_or(RepoError::NotFound)?;
    Ok(row.into())
}

pub async fn delete_area(pool: &PgPool, id: &str) -> RepoResult<()> {
    let res = sqlx::query("DELETE FROM areas WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .map_err(map_sqlx)?;
    if res.rows_affected() == 0 {
        return Err(RepoError::NotFound);
    }
    Ok(())
}

// ===========================================================================
// Work centers
// ===========================================================================

#[derive(sqlx::FromRow)]
struct WorkCenterRow {
    id: String,
    area_id: String,
    code: String,
    name: String,
    external_ref: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<WorkCenterRow> for WorkCenter {
    fn from(r: WorkCenterRow) -> Self {
        WorkCenter {
            id: r.id,
            area_id: r.area_id,
            code: r.code,
            name: r.name,
            external_ref: r.external_ref,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

pub async fn create_work_center(
    pool: &PgPool,
    area_id: &str,
    code: &str,
    name: &str,
    external_ref: Option<&str>,
) -> RepoResult<WorkCenter> {
    let row: WorkCenterRow = sqlx::query_as(
        "INSERT INTO work_centers (id, area_id, code, name, external_ref)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING id, area_id, code, name, external_ref, created_at, updated_at",
    )
    .bind(new_id())
    .bind(area_id)
    .bind(code)
    .bind(name)
    .bind(external_ref)
    .fetch_one(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(row.into())
}

pub async fn list_work_centers(pool: &PgPool) -> RepoResult<Vec<WorkCenter>> {
    let rows: Vec<WorkCenterRow> = sqlx::query_as(
        "SELECT id, area_id, code, name, external_ref, created_at, updated_at
         FROM work_centers ORDER BY code",
    )
    .fetch_all(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(rows.into_iter().map(Into::into).collect())
}

pub async fn get_work_center(pool: &PgPool, id: &str) -> RepoResult<WorkCenter> {
    let row: WorkCenterRow = sqlx::query_as(
        "SELECT id, area_id, code, name, external_ref, created_at, updated_at
         FROM work_centers WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(map_sqlx)?
    .ok_or(RepoError::NotFound)?;
    Ok(row.into())
}

pub async fn update_work_center(
    pool: &PgPool,
    id: &str,
    code: &str,
    name: &str,
    external_ref: Option<&str>,
) -> RepoResult<WorkCenter> {
    let row: WorkCenterRow = sqlx::query_as(
        "UPDATE work_centers SET code = $2, name = $3, external_ref = $4, updated_at = now()
         WHERE id = $1
         RETURNING id, area_id, code, name, external_ref, created_at, updated_at",
    )
    .bind(id)
    .bind(code)
    .bind(name)
    .bind(external_ref)
    .fetch_optional(pool)
    .await
    .map_err(map_sqlx)?
    .ok_or(RepoError::NotFound)?;
    Ok(row.into())
}

pub async fn delete_work_center(pool: &PgPool, id: &str) -> RepoResult<()> {
    let res = sqlx::query("DELETE FROM work_centers WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .map_err(map_sqlx)?;
    if res.rows_affected() == 0 {
        return Err(RepoError::NotFound);
    }
    Ok(())
}

// ===========================================================================
// Parts
// ===========================================================================

#[derive(sqlx::FromRow)]
struct PartRow {
    id: String,
    code: String,
    name: String,
    uom: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<PartRow> for Part {
    fn from(r: PartRow) -> Self {
        Part {
            id: r.id,
            code: r.code,
            name: r.name,
            uom: r.uom,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

pub async fn create_part(pool: &PgPool, code: &str, name: &str, uom: &str) -> RepoResult<Part> {
    let row: PartRow = sqlx::query_as(
        "INSERT INTO parts (id, code, name, uom)
         VALUES ($1, $2, $3, $4)
         RETURNING id, code, name, uom, created_at, updated_at",
    )
    .bind(new_id())
    .bind(code)
    .bind(name)
    .bind(uom)
    .fetch_one(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(row.into())
}

pub async fn list_parts(pool: &PgPool) -> RepoResult<Vec<Part>> {
    let rows: Vec<PartRow> = sqlx::query_as(
        "SELECT id, code, name, uom, created_at, updated_at FROM parts ORDER BY code",
    )
    .fetch_all(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(rows.into_iter().map(Into::into).collect())
}

pub async fn get_part(pool: &PgPool, id: &str) -> RepoResult<Part> {
    let row: PartRow = sqlx::query_as(
        "SELECT id, code, name, uom, created_at, updated_at FROM parts WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(map_sqlx)?
    .ok_or(RepoError::NotFound)?;
    Ok(row.into())
}

pub async fn update_part(
    pool: &PgPool,
    id: &str,
    code: &str,
    name: &str,
    uom: &str,
) -> RepoResult<Part> {
    let row: PartRow = sqlx::query_as(
        "UPDATE parts SET code = $2, name = $3, uom = $4, updated_at = now()
         WHERE id = $1
         RETURNING id, code, name, uom, created_at, updated_at",
    )
    .bind(id)
    .bind(code)
    .bind(name)
    .bind(uom)
    .fetch_optional(pool)
    .await
    .map_err(map_sqlx)?
    .ok_or(RepoError::NotFound)?;
    Ok(row.into())
}

pub async fn delete_part(pool: &PgPool, id: &str) -> RepoResult<()> {
    let res = sqlx::query("DELETE FROM parts WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .map_err(map_sqlx)?;
    if res.rows_affected() == 0 {
        return Err(RepoError::NotFound);
    }
    Ok(())
}

// ===========================================================================
// Users + auth
// ===========================================================================

#[derive(sqlx::FromRow)]
struct UserRow {
    id: String,
    username: String,
    display_name: String,
    role_code: String,
    active: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<UserRow> for User {
    fn from(r: UserRow) -> Self {
        User {
            id: r.id,
            username: r.username,
            display_name: r.display_name,
            role_code: r.role_code,
            active: r.active,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

/// Auth-facing view of a user, carrying the stored secret hashes. Stays inside
/// the server — never serialised to a client.
#[derive(sqlx::FromRow, Debug, Clone)]
pub struct UserAuth {
    pub id: String,
    pub username: String,
    pub role_code: String,
    pub password_hash: Option<String>,
    pub pin_hash: Option<String>,
    pub active: bool,
}

#[allow(clippy::too_many_arguments)]
pub async fn create_user(
    pool: &PgPool,
    username: &str,
    display_name: &str,
    role_code: &str,
    password_hash: Option<&str>,
    pin_hash: Option<&str>,
    badge_id: Option<&str>,
) -> RepoResult<User> {
    let row: UserRow = sqlx::query_as(
        "INSERT INTO users (id, username, display_name, role_code, password_hash, pin_hash, badge_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING id, username, display_name, role_code, active, created_at, updated_at",
    )
    .bind(new_id())
    .bind(username)
    .bind(display_name)
    .bind(role_code)
    .bind(password_hash)
    .bind(pin_hash)
    .bind(badge_id)
    .fetch_one(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(row.into())
}

pub async fn list_users(pool: &PgPool) -> RepoResult<Vec<User>> {
    let rows: Vec<UserRow> = sqlx::query_as(
        "SELECT id, username, display_name, role_code, active, created_at, updated_at
         FROM users ORDER BY username",
    )
    .fetch_all(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(rows.into_iter().map(Into::into).collect())
}

pub async fn find_auth_by_username(pool: &PgPool, username: &str) -> RepoResult<UserAuth> {
    sqlx::query_as::<_, UserAuth>(
        "SELECT id, username, role_code, password_hash, pin_hash, active
         FROM users WHERE username = $1",
    )
    .bind(username)
    .fetch_optional(pool)
    .await
    .map_err(map_sqlx)?
    .ok_or(RepoError::NotFound)
}

pub async fn find_auth_by_badge(pool: &PgPool, badge_id: &str) -> RepoResult<UserAuth> {
    sqlx::query_as::<_, UserAuth>(
        "SELECT id, username, role_code, password_hash, pin_hash, active
         FROM users WHERE badge_id = $1",
    )
    .bind(badge_id)
    .fetch_optional(pool)
    .await
    .map_err(map_sqlx)?
    .ok_or(RepoError::NotFound)
}

// ===========================================================================
// Audit log
// ===========================================================================

/// Append an audit entry (§7). Best-effort caller decides; this returns the id.
pub async fn insert_audit(
    pool: &PgPool,
    actor_id: Option<&str>,
    action: &str,
    entity: &str,
    entity_id: Option<&str>,
    detail: Option<serde_json::Value>,
) -> RepoResult<String> {
    let id = new_id();
    sqlx::query(
        "INSERT INTO audit_log (id, actor_id, action, entity, entity_id, detail)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(&id)
    .bind(actor_id)
    .bind(action)
    .bind(entity)
    .bind(entity_id)
    .bind(detail)
    .execute(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(id)
}

/// Count audit rows for a given entity type — used by tests and admin views.
pub async fn count_audit_for_entity(pool: &PgPool, entity: &str) -> RepoResult<i64> {
    let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM audit_log WHERE entity = $1")
        .bind(entity)
        .fetch_one(pool)
        .await
        .map_err(map_sqlx)?;
    Ok(n)
}
