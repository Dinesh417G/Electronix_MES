//! `mes-db` — database access layer.
//!
//! Owns the sqlx `PgPool`, the embedded migration set, and (from M1 onward)
//! repository modules. Migrations are embedded at compile time via
//! `sqlx::migrate!` so a service binary can apply them on startup with no
//! external `sqlx-cli` present (§12 M0, §14).
//!
//! Migrations are **append-only** after merge (§14): never edit a shipped
//! migration file — add a new one.

pub mod repo;

use std::time::Duration;

use sqlx::migrate::Migrator;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use thiserror::Error;

/// Embedded migration set, compiled from `crates/mes-db/migrations`.
pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

/// Errors from establishing a pool or running migrations.
#[derive(Debug, Error)]
pub enum DbError {
    #[error("failed to connect to database: {0}")]
    Connect(#[source] sqlx::Error),

    #[error("failed to run migrations: {0}")]
    Migrate(#[source] sqlx::migrate::MigrateError),
}

/// Connect to Postgres/TimescaleDB and return a pooled handle.
///
/// The pool is bounded so a single edge box with a modest Postgres cannot be
/// exhausted by burst ingest (§17 perf assumptions).
pub async fn connect(database_url: &str, max_connections: u32) -> Result<PgPool, DbError> {
    let pool = PgPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(Duration::from_secs(10))
        .connect(database_url)
        .await
        .map_err(DbError::Connect)?;
    Ok(pool)
}

/// Apply all pending migrations against `pool`.
pub async fn run_migrations(pool: &PgPool) -> Result<(), DbError> {
    MIGRATOR.run(pool).await.map_err(DbError::Migrate)?;
    tracing::info!("database migrations applied");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrator_is_non_empty() {
        // The embedded set must contain at least the M0 baseline migration.
        assert!(MIGRATOR.iter().next().is_some());
    }
}
