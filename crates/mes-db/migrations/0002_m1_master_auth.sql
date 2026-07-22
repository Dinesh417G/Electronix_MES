-- M1 — Master data + auth (§7, §12 M1).
--
-- Equipment & calendar, products & routing, people (roles as a *lookup table*,
-- not an enum, so Maintenance can be added additively at M9), users, and the
-- audit log. All ids are ULID TEXT with created_at/updated_at (§7). Additive
-- migrations only after this point; never edit a shipped file (§14).

-- ---------------------------------------------------------------------------
-- People: roles (lookup) + users
-- ---------------------------------------------------------------------------

CREATE TABLE roles (
    code        TEXT PRIMARY KEY,
    label       TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Seeded at M1 (§7). Maintenance is added by a later migration at M9 — a plain
-- INSERT, not a schema change, which is exactly why this is a table not an enum.
INSERT INTO roles (code, label) VALUES
    ('Admin',      'Administrator'),
    ('Planner',    'Planner'),
    ('Supervisor', 'Supervisor'),
    ('Operator',   'Operator'),
    ('Quality',    'Quality');

CREATE TABLE users (
    id            TEXT PRIMARY KEY,
    username      TEXT NOT NULL UNIQUE,
    display_name  TEXT NOT NULL,
    role_code     TEXT NOT NULL REFERENCES roles(code),
    -- argon2 PHC string for console/password login. Nullable so a kiosk-only
    -- operator can exist with just a PIN/badge (§14 — hashed at rest).
    password_hash TEXT,
    -- argon2 PHC string of the kiosk PIN, and an opaque badge id, both optional.
    pin_hash      TEXT,
    badge_id      TEXT UNIQUE,
    active        BOOLEAN NOT NULL DEFAULT TRUE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_users_role_code ON users(role_code);

-- ---------------------------------------------------------------------------
-- Audit log (§7) — append-only record of mutating actions.
-- ---------------------------------------------------------------------------

CREATE TABLE audit_log (
    id          TEXT PRIMARY KEY,
    actor_id    TEXT REFERENCES users(id),
    action      TEXT NOT NULL,          -- e.g. 'create', 'update', 'delete', 'login'
    entity      TEXT NOT NULL,          -- e.g. 'work_center', 'part'
    entity_id   TEXT,
    detail      JSONB,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_audit_log_entity ON audit_log(entity, entity_id);
CREATE INDEX idx_audit_log_actor ON audit_log(actor_id);

-- ---------------------------------------------------------------------------
-- Equipment & calendar (§7)
-- ---------------------------------------------------------------------------

CREATE TABLE sites (
    id          TEXT PRIMARY KEY,
    code        TEXT NOT NULL UNIQUE,
    name        TEXT NOT NULL,
    -- IANA timezone; shift/PM/OEE math converts via this (§14).
    timezone    TEXT NOT NULL DEFAULT 'Asia/Kolkata',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE areas (
    id          TEXT PRIMARY KEY,
    site_id     TEXT NOT NULL REFERENCES sites(id) ON DELETE CASCADE,
    code        TEXT NOT NULL,
    name        TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (site_id, code)
);

CREATE INDEX idx_areas_site ON areas(site_id);

CREATE TABLE work_centers (
    id           TEXT PRIMARY KEY,
    area_id      TEXT NOT NULL REFERENCES areas(id) ON DELETE CASCADE,
    code         TEXT NOT NULL,
    name         TEXT NOT NULL,
    -- Reserved seam for a future ElectronIx ID machine-passport link (§8.7).
    external_ref TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (area_id, code)
);

CREATE INDEX idx_work_centers_area ON work_centers(area_id);

CREATE TABLE shifts (
    id          TEXT PRIMARY KEY,
    site_id     TEXT NOT NULL REFERENCES sites(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    start_time  TIME NOT NULL,
    end_time    TIME NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (site_id, name)
);

CREATE TABLE planned_stops (
    id            TEXT PRIMARY KEY,
    work_center_id TEXT NOT NULL REFERENCES work_centers(id) ON DELETE CASCADE,
    reason        TEXT NOT NULL,
    starts_at     TIMESTAMPTZ NOT NULL,
    ends_at       TIMESTAMPTZ NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_planned_stops_wc ON planned_stops(work_center_id);

-- ---------------------------------------------------------------------------
-- Products & routing (§7)
-- ---------------------------------------------------------------------------

CREATE TABLE parts (
    id          TEXT PRIMARY KEY,
    code        TEXT NOT NULL UNIQUE,
    name        TEXT NOT NULL,
    uom         TEXT NOT NULL DEFAULT 'ea',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE boms (
    id           TEXT PRIMARY KEY,
    parent_part  TEXT NOT NULL REFERENCES parts(id) ON DELETE CASCADE,
    child_part   TEXT NOT NULL REFERENCES parts(id),
    qty_per      NUMERIC NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (parent_part, child_part)
);

CREATE INDEX idx_boms_parent ON boms(parent_part);

CREATE TABLE routings (
    id          TEXT PRIMARY KEY,
    part_id     TEXT NOT NULL REFERENCES parts(id) ON DELETE CASCADE,
    version     TEXT NOT NULL DEFAULT '1',
    active      BOOLEAN NOT NULL DEFAULT TRUE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (part_id, version)
);

CREATE TABLE routing_ops (
    id            TEXT PRIMARY KEY,
    routing_id    TEXT NOT NULL REFERENCES routings(id) ON DELETE CASCADE,
    op_no         INTEGER NOT NULL,
    name          TEXT NOT NULL,
    work_center_id TEXT REFERENCES work_centers(id),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (routing_id, op_no)
);

CREATE INDEX idx_routing_ops_routing ON routing_ops(routing_id);

-- programs — the join between MES routing and DNC's program library (§7). The
-- program_identifier is what dnc-daemon knows the program by (§8.4).
CREATE TABLE programs (
    id                 TEXT PRIMARY KEY,
    routing_op_id      TEXT REFERENCES routing_ops(id) ON DELETE CASCADE,
    part_id            TEXT REFERENCES parts(id) ON DELETE CASCADE,
    program_identifier TEXT NOT NULL,
    target_machine     TEXT,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_programs_routing_op ON programs(routing_op_id);
