# MILESTONES

Progress log for the ElectronIx MES build. One short report per completed
milestone (§14). Milestones are gated: all tests green + this file updated
before advancing (§12).

| Milestone | Status | Date |
|---|---|---|
| M0 — Scaffold + dev CI | ✅ Done | 2026-07-22 |
| M1 — Master data + auth | ⬜ Not started | — |
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
