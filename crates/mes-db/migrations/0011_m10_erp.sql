-- M10 — ERP integration (§7, §12 M10). Additive (new tables only; post-freeze OK).
--
-- The admin integration page (§11) edits erp_connections: an endpoint, a token
-- (encrypted at rest, §14 — never stored or returned in plaintext), a JSONB
-- field-mapping, and a direction. erp_sync_log is the audit trail the page's
-- "last sync" view reads. No per-customer code (§3): re-pointing at a different
-- ERP shape is a field_mapping change only.

CREATE TABLE erp_connections (
    id             TEXT PRIMARY KEY,
    -- Scope: a specific site, or NULL for an org-wide connection.
    site_id        TEXT REFERENCES sites(id) ON DELETE CASCADE,
    name           TEXT NOT NULL,
    endpoint_url   TEXT NOT NULL,
    -- Encrypted (base64 nonce||ciphertext); NULL when the ERP needs no token.
    auth_token_enc TEXT,
    -- { "fields": { "<canonical>": "<external>" } } — the mapping engine's input.
    field_mapping  JSONB NOT NULL DEFAULT '{}'::jsonb,
    -- 'import' | 'export' | 'both'
    direction      TEXT NOT NULL DEFAULT 'both',
    enabled        BOOLEAN NOT NULL DEFAULT TRUE,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_erp_connections_site ON erp_connections(site_id);

CREATE TABLE erp_sync_log (
    id            TEXT PRIMARY KEY,
    connection_id TEXT REFERENCES erp_connections(id) ON DELETE CASCADE,
    -- 'import' | 'export'
    direction     TEXT NOT NULL,
    entity        TEXT NOT NULL,
    record_count  INTEGER NOT NULL DEFAULT 0,
    -- 'success' | 'error'
    status        TEXT NOT NULL,
    detail        TEXT,
    ts            TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_erp_sync_log_conn ON erp_sync_log(connection_id, ts DESC);
