-- M4 — DNC orchestration (§7, §8.4, §12 M4). Additive.
--
-- MES never reimplements program transfer; these tables record MES's view of
-- transfers it asks the existing dnc-daemon to perform, and the draft revisions
-- created when an operator edits a program at the machine and sends it back.

CREATE TABLE dnc_transfer_events (
    id              TEXT PRIMARY KEY,
    wo_operation_id TEXT REFERENCES wo_operations(id) ON DELETE SET NULL,
    program_id      TEXT NOT NULL REFERENCES programs(id),
    direction       TEXT NOT NULL,          -- 'to_machine' | 'from_machine'
    status          TEXT NOT NULL DEFAULT 'scheduled',
    -- Opaque handle the dnc-daemon returns for correlation (§8.4).
    dnc_daemon_ref  TEXT,
    triggered_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at    TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_dnc_transfer_program ON dnc_transfer_events(program_id);
CREATE INDEX idx_dnc_transfer_status ON dnc_transfer_events(status);
CREATE INDEX idx_dnc_transfer_open ON dnc_transfer_events(wo_operation_id)
    WHERE status NOT IN ('completed', 'failed');

CREATE TABLE program_revisions (
    id            TEXT PRIMARY KEY,
    program_id    TEXT NOT NULL REFERENCES programs(id),
    revision_no   INTEGER NOT NULL,
    source        TEXT NOT NULL DEFAULT 'operator_edit',
    -- Pointer to the stored program content (blob store / path); never the
    -- program body inline. Diagnostics redaction never touches this (§8.5).
    content_ref   TEXT,
    status        TEXT NOT NULL DEFAULT 'draft',
    submitted_by  TEXT REFERENCES users(id),
    submitted_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    promoted_by   TEXT REFERENCES users(id),
    promoted_at   TIMESTAMPTZ,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (program_id, revision_no)
);

CREATE INDEX idx_program_revisions_program ON program_revisions(program_id);
CREATE INDEX idx_program_revisions_draft ON program_revisions(program_id)
    WHERE status = 'draft';
