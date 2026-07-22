-- M5 — Downtime analytics (§8.2, §12 M5). Additive.
--
-- Give downtime reasons a tree (parent_id) and a Six-Big-Losses mapping so
-- Pareto/rollup analytics can group by cause and by OEE loss bucket. Scrap
-- reasons carry a loss too (quality bucket).

ALTER TABLE downtime_reasons
    ADD COLUMN parent_id TEXT REFERENCES downtime_reasons(id),
    ADD COLUMN six_big_loss TEXT;   -- 'breakdown' | 'setup_adjustment' | 'minor_stop' | 'reduced_speed'

CREATE INDEX idx_downtime_reasons_parent ON downtime_reasons(parent_id);

ALTER TABLE scrap_reasons
    ADD COLUMN six_big_loss TEXT;   -- 'startup_reject' | 'production_reject'
