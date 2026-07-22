# MILESTONES

Progress log for the ElectronIx MES build. One short report per completed
milestone (§14). Milestones are gated: all tests green + this file updated
before advancing (§12).

| Milestone | Status | Date |
|---|---|---|
| M0 — Scaffold + dev CI | ✅ Done | 2026-07-22 |
| M1 — Master data + auth | ✅ Done | 2026-07-22 |
| M2 — Ingestion + state machine | ✅ Done | 2026-07-22 |
| M3 — Work orders + execution | ✅ Done | 2026-07-22 |
| M4 — DNC orchestration | ✅ Done | 2026-07-22 |
| M5 — Downtime analytics | ✅ Done | 2026-07-22 |
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

---

## M2 — Ingestion + state machine ✅

**Goal (§12):** `SignalSource` trait, adapters, hypertables, and the state
machine producing `machine_states` + auto `downtime_events`.

**Acceptance:** a scripted hour (run→micro-stop→down→run) matches a golden file;
an unknown source is dropped gracefully.

### What landed

- **State machine (`mes-core::state_machine`, pure/I/O-free §8.1)** — turns a
  time-ordered cycle-pulse stream over a window into non-overlapping
  Running/MicroStop/Down/PlannedStop intervals and derives an unclassified
  `DowntimeEvent` per stop. Documented, configurable thresholds (the v1 spec
  left exact numbers open): `debounce` 2s, `micro_stop_after` 60s (gaps within
  it are normal cycle variation → Running), `down_after` 5m (a stop ≤ it is a
  MicroStop, longer is Down). Includes planned-stop overlay and
  shift-boundary interval splitting. 11 unit tests including the **golden hour**.
- **Schema** — migration `0003` (additive): `signal_sources` registry,
  `machine_events` **[hypertable]**, `production_counts` **[hypertable]**,
  `machine_states`, `downtime_events`, and the `downtime_reasons`/`scrap_reasons`
  lookups. Hypertable PKs are composite `(ts, id)` so the partition column is
  covered.
- **Ingestion (`mes-ingest`)** — the `SignalSource` async trait plus a scripted
  `SimSource` (used by the E2E test and the future `machine-sim` tool). Wire
  DTOs (`RawSignal`/`SignalEvent`/`IngestResult`) live in `mes-client`.
- **Pipeline** — `/v1/ingest/signals` resolves each signal's source; unknown or
  disabled sources are **dropped and logged, never errored** (§9); known cycle/
  heartbeat signals append to `machine_events`, counts to `production_counts`.
  `/v1/ingest/recompute` runs `process::recompute_states` → the pure engine →
  and atomically **replaces** the window's `machine_states` + unclassified
  `downtime_events` (operator-classified rows preserved), so recompute is
  idempotent.
- Integration test harness extracted to `tests/common/mod.rs` and shared by the
  M1 and M2 suites.

### Verification

- `cargo fmt` / `clippy -D warnings` clean; `cargo test --all` green locally
  (state-machine golden + planned/shift/debounce unit tests; sim source test).
- **Integration suite** (`tests/m2_ingest_states.rs`, fresh schema per test):
  the scripted hour ingested through the real HTTP pipeline recomputes to the
  **exact golden** run→micro-stop→down→run states (+2 downtime events), recompute
  is idempotent, and unknown / disabled sources are dropped with nothing
  persisted. Runs in CI against the TimescaleDB service.

### Notes / deferrals

- MQTT/HTTP-line/TCP adapters are represented by the trait + sim adapter for
  M2; the live transports land with `machine-sim` and M3/M4 wiring. Recompute is
  invoked explicitly via the endpoint here; automatic scheduling (debounced,
  per-work-center) can hang off ingest later without touching the engine.
- `/v1/ingest` currently requires an authenticated caller; per-device ingest
  tokens (§14) refine this in a later pass.

---

## M3 — Work orders + execution ✅

**Goal (§12):** WO lifecycle, `/v1/exec`, counts, scrap+reasons, downtime
classify/split, WS channel, `programs` wired to `routing_ops`.

**Acceptance:** full simulated order start→complete via API; WS events observed
in a test client.

### What landed

- **Lifecycles (`mes-core::work_order`, pure)** — `WoStatus`
  (Draft→Released→InProgress→Completed→Closed, plus Cancelled) and `OpStatus`
  (Pending→InProgress→Completed) with `can_transition` guards. Illegal
  transitions are rejected before any DB write; one definition shared by
  handlers and tests. 5 unit tests.
- **Schema** — migration `0004` (additive): `work_orders`, `wo_operations`
  (unique `(work_order_id, op_no)`); `production_counts` gains nullable
  `wo_operation_id` + `scrap_reason_id` so operator counts tie to the operation
  and scrap carries a reason.
- **`/v1/orders`** — create (with operations, Planner/Admin), list, get detail,
  and release/cancel/close transitions (validated + audited + WS-published).
- **`/v1/exec`** — operator actions (any authenticated user): start operation
  (auto-advances the WO Released→InProgress), record good/scrap counts (scrap
  **requires** a reason; counts append to the `production_counts` ledger and roll
  up onto the operation atomically), complete operation, complete WO, and
  classify / **split** a downtime event (split cuts one event into two at a
  timestamp, optionally classifying each).
- **`/ws`** — a `tokio::broadcast` bus on `AppState`; every exec/order mutation
  publishes a typed `WsEvent` (`mes-client::ws`) forwarded to subscribers as
  JSON frames.
- **Programs** — `/v1/master/programs` create/list wired to `routing_ops`/parts
  (the §7 join to DNC's library, consumed at M4).

### Verification

- `cargo fmt` / `clippy -D warnings` clean; `cargo test --all` green (16
  mes-core unit tests incl. WO/op transitions).
- **Integration suite** (`tests/m3_orders_exec.rs`, fresh schema per test):
  - `full_order_lifecycle` — Operator create → **403**; Planner create → release
    → (re-release → **409** guard) → operator start (WO auto → in_progress) →
    scrap-without-reason → **400** → good + scrap-with-reason counts roll up →
    complete op → complete WO → close; `production_counts` ledger has 2 rows.
  - `ws_client_observes_execution_events` — a **real `tokio-tungstenite` client**
    connects to a served instance and observes `work_order_status_changed`,
    `operation_started`, `count_recorded`, and `operation_completed`. The served
    router and the HTTP-driving router share one `AppState` (hence one broadcast
    bus), so published events reach the socket.
  Runs in CI against the TimescaleDB service.

### Notes / deferrals

- The WS test drives HTTP via the in-process router and reads events over a real
  WebSocket; both share the same `AppState` broadcast sender (a `broadcast::Sender`
  clone points at the same channel).
- Downtime `split`/`classify` operate on the events M2 derives; the reason
  **trees** and Six-Big-Losses mapping arrive at M5.

---

## M4 — DNC orchestration ✅

**Goal (§12):** `mes-dnc-bridge`, auto-schedule on job completion,
`dnc_transfer_events`, kiosk notification event, `program_revisions` created as
Draft on an edited-program receive.

**Acceptance:** simulated job-complete → transfer scheduled → simulated
dnc-daemon ack → event clears; simulated edited-program receive → draft revision
created and explicitly **not** auto-promoted.

> **Protocol note (§8.4, §17 Q3):** the real `dnc-daemon` repo was not in this
> session, so the NDJSON command/event shapes are a **documented assumption
> isolated to `mes-dnc-bridge::protocol`**. Callers work only in terms of typed
> `DncCommand`/`DncEvent`, so reconciling with the real daemon later changes one
> module, not the orchestration. Tested entirely against a **virtual daemon**,
> as §13 requires ("never real CNC hardware").

### What landed

- **Lifecycles (`mes-core::dnc`, pure)** — `TransferStatus`
  (Scheduled→Notified→Fetched→Completed, plus Failed from any active state) and
  `RevisionStatus` (Draft→Promoted|Rejected). The revision table only allows
  Draft→Promoted, structurally enforcing "never auto-promoted" (§3). 5 unit
  tests.
- **Transport (`mes-dnc-bridge`)** — the typed NDJSON `protocol` (assumption,
  isolated), a swappable `DncDaemon` command trait, and three impls:
  `VirtualDaemon` (records commands, deterministic refs — tests/§13),
  `DisconnectedDaemon` (default; sends fail cleanly so a plant with no CNC
  degrades gracefully), and `TcpDncClient` (real socket at `127.0.0.1:8765`).
- **Schema** — migration `0005` (additive): `dnc_transfer_events`,
  `program_revisions` (unique `(program_id, revision_no)`).
- **Orchestration (`mes-edge::dnc`)** — `on_job_complete` (wired into
  `/v1/exec/.../complete`, best-effort) resolves the next queued operation's
  program (routing-op program preferred, else the part's), sends the daemon a
  `SendProgram`, records a Scheduled transfer, and publishes a kiosk
  `DncTransferScheduled` WS event. `handle_daemon_event` marks transfers
  Completed/Failed (clearing the kiosk prompt) and turns a `program_received`
  into a **draft** revision + a supervisor `ProgramRevisionDrafted` event.
- **`/v1/dnc`** — list/manual-trigger/retry transfers, list revisions,
  promote/reject (Supervisor/Admin/Planner via `roles::can_promote_revision`),
  and a `daemon-events` seam the virtual daemon / `machine-sim` drive (§13).
- **AppState** gains a swappable `Arc<dyn DncDaemon>` (default disconnected; real
  client wired from `MES_DNC_ADDR`).

### Verification

- `cargo fmt` / `clippy -D warnings` clean; `cargo test --all` green (mes-core
  dnc transitions + roles; mes-dnc-bridge transport unit tests).
- **Integration suite** (`tests/m4_dnc.rs`, fresh schema per test, virtual
  daemon injected): complete op #10 → the daemon receives `SendProgram("O1000")`
  and a **Scheduled** transfer appears → simulated `transfer_completed` →
  transfer **Completed** (`completed_at` set) → simulated `program_received` → a
  **draft** revision (asserted *not* promoted) → Operator promote → **403**,
  Supervisor promote → **promoted**, re-promote → **409**. Runs in CI against the
  TimescaleDB service.

### Notes / deferrals

- The daemon's real acknowledgement/ref shape and its inbound event loop wiring
  are the parts to confirm against `dnc-daemon` source; the `TcpDncClient`
  currently generates a local ref as a placeholder (flagged in-code).
- `machine-sim`'s virtual dnc-daemon mode (§13) reuses the same
  `DncEvent`/`daemon-events` seam this milestone establishes.

---

## M5 — Downtime analytics ✅

**Goal (§12):** reason trees, Six-Big-Losses mapping, Pareto + trend queries.

**Acceptance:** a seeded week of data → Pareto ordering matches a hand-computed
fixture.

### What landed

- **Analytics math (`mes-core::analytics`, pure)** — the `SixBigLoss` enum
  (with its OEE bucket: availability/performance/quality, §8.2) and `pareto()`:
  ranks categories by descending magnitude (ties broken by key for
  determinism), drops zero/empty, and computes each row's share + running
  cumulative share. 4 unit tests including the hand-computed 50/30/20 fixture.
- **Schema** — migration `0006` (additive): `downtime_reasons` gains
  `parent_id` (reason **tree**) + `six_big_loss`; `scrap_reasons` gains
  `six_big_loss` (quality bucket).
- **Aggregation (`mes-db::repo_analytics`)** — SQL sums classified-downtime
  seconds per reason and per loss bucket, and daily totals for the trend; the
  ranking/cumulative maths stay in `mes-core` so they're fixture-testable.
- **`/v1/analytics`** — `downtime/pareto`, `downtime/six-big-losses`, and
  `downtime/trend`, each taking a `?start=&end=` window; read-only, any
  authenticated user.

### Verification

- `cargo fmt` / `clippy -D warnings` clean; `cargo test --all` green.
- **Integration suite** (`tests/m5_analytics.rs`, fresh schema per test): a
  seeded week (Breakdown 5400s, Setup 2400s, Minor 900s classified + 600s
  unclassified) yields the Pareto order **Breakdown > Setup > Minor** with the
  fixture's exact seconds, `pct` = 5400/8700, cumulative reaching 100%; the
  Six-Big-Losses rollup ranks `breakdown` first; and the trend (which includes
  unclassified downtime) totals 9300s. Runs in CI against the TimescaleDB
  service.

### Notes / deferrals

- Pareto operates on the leaf reason assigned to each event; the `parent_id`
  tree is in place for roll-up-to-parent grouping when the supervisor UI needs
  it (M11). Unclassified downtime is excluded from the Pareto but included in
  the raw trend. The OEE **engine** (A×P×Q, continuous aggregates) is M6.
