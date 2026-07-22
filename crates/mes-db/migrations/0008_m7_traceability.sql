-- M7 — Traceability (§7, §12 M7). Additive (new tables only; post-freeze OK).

CREATE TABLE lots (
    id          TEXT PRIMARY KEY,
    lot_no      TEXT NOT NULL UNIQUE,
    part_id     TEXT NOT NULL REFERENCES parts(id),
    qty         NUMERIC NOT NULL DEFAULT 0,
    uom         TEXT NOT NULL DEFAULT 'ea',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_lots_part ON lots(part_id);

CREATE TABLE serials (
    id          TEXT PRIMARY KEY,
    serial_no   TEXT NOT NULL UNIQUE,
    part_id     TEXT NOT NULL REFERENCES parts(id),
    lot_id      TEXT REFERENCES lots(id),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_serials_part ON serials(part_id);
CREATE INDEX idx_serials_lot ON serials(lot_id);

-- Genealogy edges: a parent (assembly/output) consumed a child (component/input).
-- Backward trace walks parent→child; forward trace walks child→parent (§7).
CREATE TABLE genealogy (
    id           TEXT PRIMARY KEY,
    parent_type  TEXT NOT NULL,     -- 'lot' | 'serial'
    parent_id    TEXT NOT NULL,
    child_type   TEXT NOT NULL,
    child_id     TEXT NOT NULL,
    qty          NUMERIC,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (parent_type, parent_id, child_type, child_id)
);

CREATE INDEX idx_genealogy_parent ON genealogy(parent_type, parent_id);
CREATE INDEX idx_genealogy_child ON genealogy(child_type, child_id);

-- Material movement ledger (§7). Issuing a held lot is blocked at the API.
CREATE TABLE material_txns (
    id              TEXT PRIMARY KEY,
    lot_id          TEXT REFERENCES lots(id),
    serial_id       TEXT REFERENCES serials(id),
    txn_type        TEXT NOT NULL,     -- 'issue' | 'receive' | 'adjust'
    qty             NUMERIC NOT NULL,
    wo_operation_id TEXT REFERENCES wo_operations(id),
    ts              TIMESTAMPTZ NOT NULL DEFAULT now(),
    user_id         TEXT REFERENCES users(id)
);

CREATE INDEX idx_material_txns_lot ON material_txns(lot_id);

-- Holds (§7 QMS; introduced here for "held lot blocks issue"). Additive columns
-- (e.g. an NCR link) can be added at M8.
CREATE TABLE holds (
    id           TEXT PRIMARY KEY,
    entity_type  TEXT NOT NULL,        -- 'lot' | 'serial'
    entity_id    TEXT NOT NULL,
    reason       TEXT,
    status       TEXT NOT NULL DEFAULT 'active',  -- 'active' | 'released'
    created_by   TEXT REFERENCES users(id),
    released_by  TEXT REFERENCES users(id),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    released_at  TIMESTAMPTZ
);

-- Fast "is this entity currently held?" lookup.
CREATE INDEX idx_holds_active ON holds(entity_type, entity_id) WHERE status = 'active';
