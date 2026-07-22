-- M9 — CMMS (§7, §12 M9). Additive (new tables + one role row; post-freeze OK).
--
-- Preventive maintenance (calendar + usage-hours off the existing machine_states
-- RUNNING intervals — no new raw data), maintenance work orders (which double as
-- maintenance history), a spare-parts ledger (stock derived by summing txns,
-- never a mutable column), and procurement *requests* only — the PO/vendor
-- lifecycle stays in ERP (§3); status caps at 'requested' until M10 wires the
-- ERP push.

-- Maintenance role — a plain INSERT, not a schema change, which is exactly why
-- roles is a lookup table not an enum (§7).
INSERT INTO roles (code, label) VALUES ('Maintenance', 'Maintenance')
    ON CONFLICT (code) DO NOTHING;

-- ---------------------------------------------------------------------------
-- PM schedules (§7)
-- ---------------------------------------------------------------------------

CREATE TABLE pm_schedules (
    id                TEXT PRIMARY KEY,
    work_center_id    TEXT NOT NULL REFERENCES work_centers(id) ON DELETE CASCADE,
    name              TEXT NOT NULL,
    -- 'calendar' | 'usage_hours'
    trigger_type      TEXT NOT NULL,
    -- days for calendar, run-hours for usage_hours
    interval_value    NUMERIC NOT NULL,
    -- calendar bookkeeping
    last_done_at      TIMESTAMPTZ,
    next_due_at       TIMESTAMPTZ,
    -- usage-hours bookkeeping (cumulative RUNNING hours at last service / next due)
    last_done_usage_h NUMERIC,
    next_due_usage_h  NUMERIC,
    checklist_ref     TEXT,
    enabled           BOOLEAN NOT NULL DEFAULT TRUE,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_pm_schedules_wc ON pm_schedules(work_center_id);

-- ---------------------------------------------------------------------------
-- Maintenance work orders (§7) — closed WOs *are* the maintenance history
-- ---------------------------------------------------------------------------

CREATE TABLE maintenance_work_orders (
    id             TEXT PRIMARY KEY,
    work_center_id TEXT NOT NULL REFERENCES work_centers(id) ON DELETE CASCADE,
    pm_schedule_id TEXT REFERENCES pm_schedules(id),
    -- 'PM' | 'Corrective' | 'Breakdown'
    wo_type        TEXT NOT NULL,
    -- 'requested' → 'scheduled' → 'in_progress' → 'completed' → 'verified'
    status         TEXT NOT NULL DEFAULT 'requested',
    technician_id  TEXT REFERENCES users(id),
    failure_code   TEXT,
    notes          TEXT,
    opened_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    scheduled_at   TIMESTAMPTZ,
    started_at     TIMESTAMPTZ,
    closed_at      TIMESTAMPTZ,
    verified_at    TIMESTAMPTZ,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_mwo_wc ON maintenance_work_orders(work_center_id);
CREATE INDEX idx_mwo_status ON maintenance_work_orders(status);

-- ---------------------------------------------------------------------------
-- Spare parts + txn ledger (§7). Current stock is SUM(qty); every txn stores
-- its signed stock effect (receive +, issue -, adjust signed).
-- ---------------------------------------------------------------------------

CREATE TABLE spare_parts (
    id            TEXT PRIMARY KEY,
    code          TEXT NOT NULL UNIQUE,
    name          TEXT NOT NULL,
    uom           TEXT NOT NULL DEFAULT 'ea',
    reorder_point NUMERIC NOT NULL DEFAULT 0,
    reorder_qty   NUMERIC NOT NULL DEFAULT 0,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE spare_txns (
    id                TEXT PRIMARY KEY,
    spare_part_id     TEXT NOT NULL REFERENCES spare_parts(id) ON DELETE CASCADE,
    maintenance_wo_id TEXT REFERENCES maintenance_work_orders(id),
    -- 'issue' | 'receive' | 'adjust'
    txn_type          TEXT NOT NULL,
    -- signed stock effect of this txn (already negated for issues)
    qty               NUMERIC NOT NULL,
    ts                TIMESTAMPTZ NOT NULL DEFAULT now(),
    user_id           TEXT REFERENCES users(id)
);

CREATE INDEX idx_spare_txns_part ON spare_txns(spare_part_id);

-- ---------------------------------------------------------------------------
-- Procurement requests (§7) — request only; PO/vendor lifecycle stays in ERP.
-- ---------------------------------------------------------------------------

CREATE TABLE procurement_requests (
    id            TEXT PRIMARY KEY,
    spare_part_id TEXT NOT NULL REFERENCES spare_parts(id) ON DELETE CASCADE,
    qty_requested NUMERIC NOT NULL,
    -- 'reorder_point' | 'manual'
    reason        TEXT NOT NULL,
    -- 'requested' | 'sent_to_erp' | 'fulfilled' (caps at 'requested' until M10)
    status        TEXT NOT NULL DEFAULT 'requested',
    erp_reference TEXT,
    pushed_at     TIMESTAMPTZ,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_procurement_requests_status ON procurement_requests(status);
-- At most one open (requested) auto-reorder request per spare, so a repeated
-- reorder-point breach does not spam duplicates.
CREATE UNIQUE INDEX uq_procurement_open_reorder
    ON procurement_requests(spare_part_id)
    WHERE status = 'requested' AND reason = 'reorder_point';
