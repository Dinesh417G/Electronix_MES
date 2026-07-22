-- M8 — QMS (§7, §8, §12 M8). Additive.

CREATE TABLE inspection_plans (
    id          TEXT PRIMARY KEY,
    part_id     TEXT NOT NULL REFERENCES parts(id) ON DELETE CASCADE,
    code        TEXT NOT NULL,
    name        TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (part_id, code)
);

CREATE TABLE characteristics (
    id           TEXT PRIMARY KEY,
    plan_id      TEXT NOT NULL REFERENCES inspection_plans(id) ON DELETE CASCADE,
    name         TEXT NOT NULL,
    uom          TEXT,
    nominal      NUMERIC,
    lower_limit  NUMERIC,
    upper_limit  NUMERIC,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_characteristics_plan ON characteristics(plan_id);

CREATE TABLE inspection_results (
    id                TEXT PRIMARY KEY,
    characteristic_id TEXT NOT NULL REFERENCES characteristics(id),
    lot_id            TEXT REFERENCES lots(id),
    serial_id         TEXT REFERENCES serials(id),
    wo_operation_id   TEXT REFERENCES wo_operations(id),
    measured_value    NUMERIC NOT NULL,
    result            TEXT NOT NULL,          -- 'pass' | 'fail'
    inspected_by      TEXT REFERENCES users(id),
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_inspection_results_char ON inspection_results(characteristic_id);

CREATE TABLE ncrs (
    id                   TEXT PRIMARY KEY,
    ncr_no               TEXT NOT NULL UNIQUE,
    inspection_result_id TEXT REFERENCES inspection_results(id),
    lot_id               TEXT REFERENCES lots(id),
    serial_id            TEXT REFERENCES serials(id),
    part_id              TEXT REFERENCES parts(id),
    status               TEXT NOT NULL DEFAULT 'open',   -- open|dispositioned|closed
    disposition          TEXT,                            -- rework|scrap|use_as_is|return
    reason               TEXT,
    dispositioned_by     TEXT REFERENCES users(id),
    dispositioned_at     TIMESTAMPTZ,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_ncrs_status ON ncrs(status);

-- Link the M7 holds table to the NCR that raised it (additive, per M7 note).
ALTER TABLE holds ADD COLUMN ncr_id TEXT REFERENCES ncrs(id);
CREATE INDEX idx_holds_ncr ON holds(ncr_id);
