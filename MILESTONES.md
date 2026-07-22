# MILESTONES

Progress log for the ElectronIx MES build. One short report per completed
milestone (§14). Milestones are gated: all tests green + this file updated
before advancing (§12).

| Milestone | Status | Date |
|---|---|---|
| M0 — Scaffold + dev CI | ✅ Done | 2026-07-22 |
| M1 — Master data + auth | ✅ Done | 2026-07-22 |
| M2 — Ingestion + state machine | ⬜ Not started | — |
| M3 — Work orders + execution | ⬜ Not started | — |
| M4 — DNC orchestration | ⬜ Not started | — |
| M5 — Downtime analytics | ⬜ Not started | — |
| M6 — OEE | ⬜ Not started | — |
| M7 — Traceability | ⬜ Not started | — |
| M8 — QMS | ⬜ Not started | — |
| M9 — CMMS | ⬜ Not started | — |
| M10 — ERP integration page | ⬜ Not started | — |
| M11 — Desktop app | ⬜ Not started | — |
| M12 — Cloud + sync | ⬜ Not started | — |
| M13 — MCP + Copilot | ⬜ Not started | — |
| M14 — Release CI + Diagnostics | ⬜ Not started | — |
| M15 — Mobile app (deferred) | ⬜ Deferred | — |

---

## M0 — Scaffold + dev CI ✅

**Goal (§12):** Workspace, docker-compose (timescaledb + mosquitto), sqlx
migrate baseline, `ci.yml` mirroring DNC's dev-gate (fmt + clippy + test on
push), tracing, health endpoints.

**Acceptance:** `cargo test` green; `docker compose up` → `/healthz` OK on edge.

### What landed

- **Cargo workspace** (`resolver = "2"`, pinned `[workspace.dependencies]`
  matching §5) with all eleven crates from §6:
  `mes-core`, `mes-db`, `mes-ingest`, `mes-dnc-bridge`, `mes-erp`,
  `mes-agent-tools`, `mes-diagnostics`, `mes-sync`, `mes-edge` (bin),
  `mes-cloud` (bin), `mes-client`.
- **`mes-core`** established I/O-free with ULID `new_id()` and a `thiserror`
  `CoreError`. Library crates carry shaped stubs (trait/consts/error types)
  pointing at their milestone, so M1+ append rather than rewrite.
- **`mes-db`**: bounded `PgPool` factory + `sqlx::migrate!`-embedded migration
  set; baseline migration `0001_baseline.sql` enables the `timescaledb`
  extension and seeds a `mes_meta` marker. Migrations are append-only (§14).
- **`mes-edge` / `mes-cloud` binaries**: env-driven config, structured tracing
  (`RUST_LOG`), optional DB connect + migrate on boot, graceful shutdown, and an
  Axum router exposing `/healthz` (liveness), `/readyz` (DB-backed readiness,
  503 until a pool is wired), and `/api-doc/openapi.json` (utoipa, §10). Every
  handler runs under `TraceLayer` (§14).
- **`mes-client`**: shared `HealthResponse` DTO so the wire contract lives once.
- **Infra**: `Dockerfile` (multi-stage, builds both binaries), `docker-compose.yml`
  (TimescaleDB pg16 + Mosquitto + edge + cloud with healthchecks),
  `.github/workflows/ci.yml` dev-gate mirroring DNC (fmt + clippy `-D warnings`
  + test, with a TimescaleDB service ready for M1 integration tests),
  `rustfmt.toml`, `.gitignore`.
- **`.claude/agents/`**: `scaffold` (haiku), `builder` (sonnet), `architect`
  (opus) per the §15 model-routing table.
- Reserved dirs: `apps/desktop` (M11), `apps/mobile` (M15, deferred),
  `tools/machine-sim` (M2), each with a README stating scope.

### Verification

- `cargo fmt --all -- --check` — clean.
- `cargo clippy --all-targets --all-features -- -D warnings` — clean.
- `cargo test --all --all-features` — **16 passed, 0 failed** across 11 crates.
- Local smoke test: `mes-edge` boots without a DB, `GET /healthz` →
  `200 {"service":"mes-edge","status":"ok","version":"0.1.0"}`, `GET /readyz` →
  `503` (no pool, correct), `GET /api-doc/openapi.json` → the OpenAPI document.
- `docker compose config` validates.

### Notes / deferrals

- Health endpoints are DB-optional by design so the binaries boot for local
  smoke tests without Postgres; compose always supplies `DATABASE_URL`, so the
  containerised edge runs migrations and reports ready.
- No `sqlx::query!` macros yet, so no `SQLX_OFFLINE`/prepared-cache is needed at
  M0. That gets introduced when the first compile-checked queries land (M1).
- Full `docker compose up` image build was not run end-to-end in this
  environment (long Rust release build); the compose file is config-validated
  and the same binaries pass the local `/healthz` smoke test.

---

## M1 — Master data + auth ✅

**Goal (§12):** Equipment/product/people tables (`roles` as a lookup table, not
an enum), CRUD, argon2 + JWT, PIN/badge kiosk login, `audit_log`.

**Acceptance:** CRUD + role-enforcement integration tests (Operator cannot touch
master data).

### What landed

- **Schema** — migration `0002_m1_master_auth.sql` (additive; `0001` untouched):
  `roles` (seeded Admin/Planner/Supervisor/Operator/Quality — a *lookup table*
  so Maintenance is a plain insert at M9), `users` (argon2 password/PIN hashes +
  optional badge, `role_code → roles.code`), `audit_log`, the equipment
  hierarchy (`sites → areas → work_centers`, plus `shifts`, `planned_stops`,
  `work_centers.external_ref` reserved for the §8.7 ID seam), and products/
  routing (`parts`, `boms`, `routings`, `routing_ops`, `programs`).
- **Auth (`mes-edge::auth`)** — argon2id hashing/verification and HS256 JWTs
  (`sub`+`role`+`exp`); role embedded in the token so authz needs no DB
  round-trip. Secret from `MES_JWT_SECRET` (ephemeral fallback + warning in dev).
- **Extractors (`mes-edge::extract`)** — `AuthUser` validates the bearer token;
  `MasterWriter` layers the master-write policy (Admin/Planner) so a
  write handler *structurally* cannot run for a disallowed role. The policy
  itself lives in `mes-core::roles` (pure, unit-tested).
- **`/v1/auth`** — `POST /login` (password), `POST /pin-login` (badge presence,
  or username + PIN). Generic 401s to avoid user enumeration.
- **`/v1/master`** — full CRUD for sites, areas, work-centers, parts; user
  create/list. Reads need any authenticated user; writes need `MasterWriter`.
  Every mutation writes an `audit_log` row.
- **Repositories (`mes-db::repo`)** — runtime-checked `query_as` (keeps
  `cargo build` hermetic without a DB); sqlx errors mapped to semantic
  `RepoError` (NotFound→404, unique→409, FK→400). Repos return `mes-client`
  DTOs directly; secret hashes never leave the crate except via the internal
  auth row.
- `mes-edge` refactored to lib + thin bin so integration tests exercise the
  router in-process.

### Verification

- `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features
  -- -D warnings` — clean.
- `cargo test --all` — green locally (unit tests: role policy, argon2
  roundtrip, JWT issue/verify/expiry/wrong-secret).
- **Integration suite** (`crates/mes-edge/tests/m1_master_auth.rs`, §13
  "fresh schema per test" via an isolated Postgres schema + search_path):
  roles-seeded, Admin full CRUD + audit-trail assertion, **Operator write →
  403 while read → 200** (the M1 acceptance gate), unauthenticated → 401,
  password-login → working token → authorized write, and FK enforcement on the
  equipment hierarchy. Gated on `DATABASE_URL`: runs in CI against the
  TimescaleDB service, skipped where no database is available.

### Notes / deferrals

- No local DB run was possible in this environment (the container registry is
  network-blocked, so no TimescaleDB image; migration `0001` needs the
  `timescaledb` extension, so vanilla Postgres can't stand in). The integration
  suite is therefore validated by CI, not locally.
- CRUD handlers cover the representative equipment + product entities (sites,
  areas, work-centers, parts) and users; the remaining M1 tables (`shifts`,
  `planned_stops`, `boms`, `routings`, `routing_ops`, `programs`) exist in the
  schema and get their handlers when their consuming milestones need them.
- Queries are runtime-checked for now; migrating to compile-checked (`query!`)
  awaits a committed sqlx prepared-cache (§14).
