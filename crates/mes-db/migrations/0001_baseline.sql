-- M0 baseline migration.
--
-- Establishes the TimescaleDB extension and a tiny meta table so a freshly
-- provisioned edge/cloud database is in a known state. Domain tables
-- (equipment, people, execution, ...) arrive additively from M1 onward as
-- new, separate migration files — this file is never edited after merge (§14).

CREATE EXTENSION IF NOT EXISTS timescaledb;

-- Records which schema baseline is installed. Not the migration ledger itself
-- (sqlx owns _sqlx_migrations); this is a human-readable marker other tooling
-- can read without depending on sqlx internals.
CREATE TABLE IF NOT EXISTS mes_meta (
    key         TEXT PRIMARY KEY,
    value       TEXT NOT NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO mes_meta (key, value)
VALUES ('schema_baseline', 'M0')
ON CONFLICT (key) DO NOTHING;
