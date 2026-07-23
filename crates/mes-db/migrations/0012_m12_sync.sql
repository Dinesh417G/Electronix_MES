-- M12 — Cloud + sync (§7, §8.3, §12 M12). Additive.
--
-- The edge writes an `outbox` row in the same transaction as every syncable
-- write, pushes batches (≤500) to the cloud, which applies them idempotently via
-- `applied_entries`. The cloud also enqueues destination-tagged outbox rows
-- (remote commands, e.g. a remotely-created work order) that an edge pulls. Both
-- tables live in the shared schema (one engine everywhere, §3); edge stays the
-- source of truth. Cloud adds `orgs`/`plants` for multi-tenancy + enrollment.

-- Append-only change feed. `destination` NULL = bound for the cloud (edge→cloud);
-- a plant id = a command bound for that edge (cloud→edge). `id` is the
-- idempotency key carried end to end.
CREATE TABLE outbox (
    id          TEXT PRIMARY KEY,
    aggregate   TEXT NOT NULL,          -- e.g. 'work_order'
    entity_id   TEXT NOT NULL,
    op          TEXT NOT NULL,          -- 'upsert' | 'delete'
    payload     JSONB NOT NULL,
    destination TEXT,                   -- NULL = to cloud; else target plant id
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    synced_at   TIMESTAMPTZ             -- set once the peer acks the entry
);

CREATE INDEX idx_outbox_to_cloud ON outbox(created_at)
    WHERE destination IS NULL AND synced_at IS NULL;
CREATE INDEX idx_outbox_to_plant ON outbox(destination, created_at)
    WHERE destination IS NOT NULL AND synced_at IS NULL;

-- Idempotent-apply ledger: an entry id present here has already been applied, so
-- a replayed batch is a no-op (§8.3 — "duplicate batch is a no-op").
CREATE TABLE applied_entries (
    id         TEXT PRIMARY KEY,
    applied_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ---------------------------------------------------------------------------
-- Cloud multi-tenancy (§7) — org → plants, with plant enrollment.
-- ---------------------------------------------------------------------------

CREATE TABLE orgs (
    id         TEXT PRIMARY KEY,
    code       TEXT NOT NULL UNIQUE,
    name       TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE plants (
    id                    TEXT PRIMARY KEY,
    org_id                TEXT NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
    code                  TEXT NOT NULL,
    name                  TEXT NOT NULL,
    -- SHA-256 of the enrollment token (§14 — hashed at rest). The plaintext is
    -- shown once at enrollment and never stored.
    enrollment_token_hash TEXT,
    enrolled_at           TIMESTAMPTZ,
    last_sync_at          TIMESTAMPTZ,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (org_id, code)
);

CREATE INDEX idx_plants_org ON plants(org_id);
