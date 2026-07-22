-- M6 — OEE (§8.2, §12 M6). Additive.
--
-- Nominal (ideal) cycle time per work center, used as the Performance-factor
-- rate. A per-routing-op override can be layered on additively later; this is
-- the work-center default. **Schema freeze:** core production/QMS/CMMS tables
-- are additive-only from here on (§6, §14).

ALTER TABLE work_centers
    ADD COLUMN ideal_cycle_seconds NUMERIC;
