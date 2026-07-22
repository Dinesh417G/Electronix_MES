-- M3 — Work orders + execution (§7 Execution, §12 M3). Additive.

CREATE TABLE work_orders (
    id            TEXT PRIMARY KEY,
    wo_number     TEXT NOT NULL UNIQUE,
    part_id       TEXT NOT NULL REFERENCES parts(id),
    routing_id    TEXT REFERENCES routings(id),
    qty_ordered   NUMERIC NOT NULL,
    priority      INTEGER NOT NULL DEFAULT 100,
    status        TEXT NOT NULL DEFAULT 'draft',
    planned_start TIMESTAMPTZ,
    planned_end   TIMESTAMPTZ,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_work_orders_status ON work_orders(status);
CREATE INDEX idx_work_orders_part ON work_orders(part_id);

CREATE TABLE wo_operations (
    id             TEXT PRIMARY KEY,
    work_order_id  TEXT NOT NULL REFERENCES work_orders(id) ON DELETE CASCADE,
    routing_op_id  TEXT REFERENCES routing_ops(id),
    op_no          INTEGER NOT NULL,
    work_center_id TEXT REFERENCES work_centers(id),
    status         TEXT NOT NULL DEFAULT 'pending',
    qty_good       INTEGER NOT NULL DEFAULT 0,
    qty_scrap      INTEGER NOT NULL DEFAULT 0,
    started_at     TIMESTAMPTZ,
    completed_at   TIMESTAMPTZ,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (work_order_id, op_no)
);

CREATE INDEX idx_wo_operations_wo ON wo_operations(work_order_id);
CREATE INDEX idx_wo_operations_wc ON wo_operations(work_center_id, status);

-- Tie production counts to the operation that produced them (operator entry).
-- The M2 hypertable stays; this is an additive nullable column.
ALTER TABLE production_counts
    ADD COLUMN wo_operation_id TEXT REFERENCES wo_operations(id);

-- Scrap counts carry a reason (§7). Nullable so machine-auto counts (no reason)
-- still fit; the exec API requires a reason when scrap > 0.
ALTER TABLE production_counts
    ADD COLUMN scrap_reason_id TEXT REFERENCES scrap_reasons(id);

CREATE INDEX idx_production_counts_wo_op ON production_counts(wo_operation_id);
