-- M13 — MCP + Copilot (§7, §8.6, §12 M13). Additive.
--
-- `copilot_messages` is an audit log only — the copilot itself is stateless
-- request/response with tool-use, not a stored session (§7). `work_orders`
-- gains a `plant_id` tag so cloud-aggregated data is tenant-attributable: the
-- agent tools scope every query by org → plants → plant_id, so tenant isolation
-- is enforced at the query layer in exactly one place (§14, §8.6).

-- Tag applied/aggregated work orders with the plant they belong to. Nullable and
-- un-constrained (edges have no plants table populated); on the cloud the sync
-- apply sets it from the pushing plant.
ALTER TABLE work_orders ADD COLUMN plant_id TEXT;

CREATE INDEX idx_work_orders_plant ON work_orders(plant_id);

CREATE TABLE copilot_messages (
    id         TEXT PRIMARY KEY,
    org_id     TEXT REFERENCES orgs(id) ON DELETE CASCADE,
    user_id    TEXT,
    role       TEXT NOT NULL,          -- 'user' | 'assistant'
    content    TEXT NOT NULL,
    tool_calls JSONB,                  -- tool_use requests/results for this turn
    ts         TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_copilot_messages_org ON copilot_messages(org_id, ts DESC);
