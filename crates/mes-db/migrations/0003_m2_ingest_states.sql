-- M2 — Ingestion + state machine (§7, §9, §12 M2). Additive.
--
-- Raw machine signals land in hypertables (machine_events, production_counts);
-- the pure state machine (mes-core) derives machine_states + downtime_events
-- from the cycle-pulse stream. signal_sources is the registry — signals from an
-- unregistered source are logged and dropped, never persisted (§9).

-- ---------------------------------------------------------------------------
-- Signal source registry (§9)
-- ---------------------------------------------------------------------------

CREATE TABLE signal_sources (
    id             TEXT PRIMARY KEY,
    work_center_id TEXT NOT NULL REFERENCES work_centers(id) ON DELETE CASCADE,
    -- The opaque key a device stamps on its signals (topic, device id, ...).
    source_key     TEXT NOT NULL UNIQUE,
    -- What the source emits: 'cycle' pulses drive the state machine; 'count'
    -- carries good/scrap production counts.
    kind           TEXT NOT NULL,
    enabled        BOOLEAN NOT NULL DEFAULT TRUE,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_signal_sources_wc ON signal_sources(work_center_id);

-- ---------------------------------------------------------------------------
-- Raw machine events [HT] — cycle pulses, heartbeats, run/stop edges (§7)
-- ---------------------------------------------------------------------------

CREATE TABLE machine_events (
    id             TEXT NOT NULL,
    ts             TIMESTAMPTZ NOT NULL,
    work_center_id TEXT NOT NULL REFERENCES work_centers(id) ON DELETE CASCADE,
    source_id      TEXT REFERENCES signal_sources(id),
    event_type     TEXT NOT NULL,      -- 'cycle' | 'heartbeat' | ...
    payload        JSONB,
    -- The partitioning column must be part of any unique constraint, so the PK
    -- is composite (§7 Timescale specifics).
    PRIMARY KEY (ts, id)
);

SELECT create_hypertable('machine_events', 'ts');
CREATE INDEX idx_machine_events_wc_ts ON machine_events(work_center_id, ts DESC);

-- ---------------------------------------------------------------------------
-- Production counts [HT] (§7) — good/scrap tallies over time
-- ---------------------------------------------------------------------------

CREATE TABLE production_counts (
    id               TEXT NOT NULL,
    ts               TIMESTAMPTZ NOT NULL,
    work_center_id   TEXT NOT NULL REFERENCES work_centers(id) ON DELETE CASCADE,
    source_id        TEXT REFERENCES signal_sources(id),
    good             INTEGER NOT NULL DEFAULT 0,
    scrap            INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (ts, id)
);

SELECT create_hypertable('production_counts', 'ts');
CREATE INDEX idx_production_counts_wc_ts ON production_counts(work_center_id, ts DESC);

-- ---------------------------------------------------------------------------
-- Derived machine states (§7) — non-overlapping intervals per work center
-- ---------------------------------------------------------------------------

CREATE TABLE machine_states (
    id             TEXT PRIMARY KEY,
    work_center_id TEXT NOT NULL REFERENCES work_centers(id) ON DELETE CASCADE,
    state          TEXT NOT NULL,      -- 'running' | 'micro_stop' | 'down' | 'planned_stop'
    start_ts       TIMESTAMPTZ NOT NULL,
    end_ts         TIMESTAMPTZ NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_machine_states_wc ON machine_states(work_center_id, start_ts);

-- ---------------------------------------------------------------------------
-- Reason lookups (§7). Trees / six-big-loss mapping arrive at M5 additively.
-- ---------------------------------------------------------------------------

CREATE TABLE downtime_reasons (
    id          TEXT PRIMARY KEY,
    code        TEXT NOT NULL UNIQUE,
    label       TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE scrap_reasons (
    id          TEXT PRIMARY KEY,
    code        TEXT NOT NULL UNIQUE,
    label       TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ---------------------------------------------------------------------------
-- Downtime events (§7) — auto-detected stops, classified by an operator later
-- ---------------------------------------------------------------------------

CREATE TABLE downtime_events (
    id             TEXT PRIMARY KEY,
    work_center_id TEXT NOT NULL REFERENCES work_centers(id) ON DELETE CASCADE,
    state          TEXT NOT NULL,      -- 'micro_stop' | 'down'
    start_ts       TIMESTAMPTZ NOT NULL,
    end_ts         TIMESTAMPTZ NOT NULL,
    reason_id      TEXT REFERENCES downtime_reasons(id),
    classified_by  TEXT REFERENCES users(id),
    classified_at  TIMESTAMPTZ,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_downtime_events_wc ON downtime_events(work_center_id, start_ts);
CREATE INDEX idx_downtime_events_unclassified ON downtime_events(work_center_id)
    WHERE reason_id IS NULL;
