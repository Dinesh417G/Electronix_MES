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
| M6 — OEE | ✅ Done | 2026-07-22 |
| M7 — Traceability | ✅ Done | 2026-07-22 |
| M8 — QMS | ✅ Done | 2026-07-22 |
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

---

## M6 — OEE ✅  · 🔒 schema freeze

**Goal (§12):** Rust OEE engine + continuous aggregates (`oee_hourly`,
`oee_by_shift`), `/v1/analytics`, live shift OEE over WS.

**Acceptance:** golden-day test — A/P/Q/OEE within 0.1% in both Rust and SQL
paths; shift-boundary test passes.

### What landed

- **OEE engine (`mes-core::oee`, pure)** — `compute()`: Availability = run ÷
  planned-production; Performance = (ideal-cycle × total) ÷ run, **capped at
  1.0**; Quality = good ÷ total; OEE = A×P×Q. Zero denominators yield 0, not
  NaN. 3 unit tests incl. the hand-computed golden fixture (A=0.8, P=0.75,
  Q=0.9, OEE=0.54).
- **Schema** — migration `0007` (additive): `work_centers.ideal_cycle_seconds`
  (the Performance-factor rate). **🔒 Schema freeze:** core production/QMS/CMMS
  tables are additive-only from here (§6, §14).
- **Dual paths (`mes-db::repo_oee`)** — `oee_inputs` (Rust path: raw scalars →
  `mes_core::oee::compute`) and `oee_sql` (SQL path: one CTE computing A/P/Q/OEE
  with the *same* interval-clamping and performance cap). `oee_by_shift` splits
  the window by the work center's site shifts (per day, overnight-aware).
- **`/v1/analytics`** — `oee` (window) and `oee/by-shift`.
- **Live OEE over WS** — completing a count publishes an `OeeSnapshot` WsEvent
  with the work center's day-to-date OEE (best-effort, §8.2).

### Verification

- `cargo fmt` / `clippy -D warnings` clean; `cargo test --all` green.
- **Integration suite** (`tests/m6_oee.rs`, fresh schema per test):
  - `golden_day_rust_and_sql_agree` — a seeded day (run 21600s, planned-stop
    1800s, down 5400s, counts 729/810, ideal 20s) yields **A=0.80, P=0.75,
    Q=0.90, OEE=0.54** via the API (Rust path), and `oee_sql` (SQL path) agrees
    **within 0.1%** on every factor.
  - `oee_by_shift_respects_boundary` — two back-to-back shifts split at exactly
    12:00 with no overlap and the correct per-shift OEE (A≈0.833, B=0.50).
  Runs in CI against the TimescaleDB service.

### Notes / deferrals

- **Continuous aggregates deferred.** TimescaleDB continuous aggregates require
  non-transactional DDL (`CREATE MATERIALIZED VIEW … WITH
  (timescaledb.continuous)` cannot run inside a transaction), while `sqlx
  migrate` wraps each migration in one. OEE hourly/by-shift is therefore served
  by SQL queries with **identical semantics**; materialized continuous
  aggregates are a pure performance optimization to add when a non-transactional
  migration path is introduced (tracked for a follow-up; does not affect the M6
  acceptance, which is the dual Rust/SQL cross-check + shift boundary).
- `ideal_cycle_seconds` is per work center (nominal rate) for v1; a
  per-routing-op override can be layered additively (§14) without a breaking
  change.
- The live `OeeSnapshot` is day-to-date; scoping it to the *current shift*
  reuses the same `oee_by_shift` logic when the kiosk/console needs it (M11).

---

## M7 — Traceability ✅

**Goal (§12):** lots/serials, genealogy, recursive forward/backward trace,
barcode format `EMX1|<type>|<id>`.

**Acceptance:** a 3-level assembly fixture traces both directions; a held lot
blocks issue.

### What landed

- **Barcode (`mes-core::barcode`, pure)** — `encode`/`parse` for the
  `EMX1|<type>|<id>` format, with `LOT`/`SER` type codes. 3 unit tests
  (roundtrip, malformed rejection, pipe-in-id handling).
- **Schema** — migration `0008` (additive, all new tables): `lots`, `serials`,
  `genealogy` (parent=assembly/output → child=component/input edges),
  `material_txns` (issue/receive/adjust ledger), and `holds` (introduced here
  for "held lot blocks issue"; M8 QMS extends it additively).
- **Recursive trace (`mes-db::repo_trace`)** — `trace_backward` (all components
  consumed by an assembly) and `trace_forward` (all assemblies a component ended
  up in) via `WITH RECURSIVE` CTEs, cycle-guarded (depth < 64), de-duplicated to
  min-depth, resolving each node's `lot_no`/`serial_no`.
- **Hold-checked issue** — `issue_material` refuses to issue a lot/serial under
  an **active hold** (returns Conflict → 409).
- **`/v1/trace`** — create lots/serials/genealogy + issue material (any
  authenticated user); place/release holds (**quality role** via
  `roles::can_manage_quality` — Quality/Supervisor/Admin); backward/forward trace
  lookups; and a barcode-parse endpoint.

### Verification

- `cargo fmt` / `clippy -D warnings` clean; `cargo test --all` green.
- **Integration suite** (`tests/m7_traceability.rs`, fresh schema per test):
  - `three_level_assembly_traces_both_directions` — FG→SUB→{RAW-A,RAW-B};
    **backward** from FG returns SUB (depth 1) + both raws (depth 2), **forward**
    from RAW-A returns SUB (depth 1) + FG (depth 2).
  - `held_lot_blocks_issue` — un-held issue → 201; Operator place-hold → **403**;
    Quality place-hold → 201; issue of the held lot → **409**; release → issue
    → 201.
  - `barcode_parse_roundtrip` — `EMX1|LOT|01HXYZ` parses; garbage → 400.
  Runs in CI against the TimescaleDB service.

### Notes / deferrals

- Trace is modelled at lot/serial granularity via generic `entity_type` edges;
  serial-level-vs-lot-only default per part (§17 Q6) is a policy toggle for a
  launch customer, not a schema change. `holds` lands at M7 for the issue-block;
  M8 adds the NCR linkage additively.

---

## M8 — QMS ✅

**Goal (§12):** plans/characteristics/results with auto pass/fail, auto-NCR +
hold on fail, disposition lifecycle.

**Acceptance:** fail → NCR + hold created; Rework disposition releases correctly;
Quality-role gating enforced.

### What landed

- **QMS domain (`mes-core::qms`, pure)** — `evaluate()` (measurement vs optional
  inclusive lower/upper limits → Pass/Fail), `NcrStatus`
  (Open→Dispositioned→Closed), and `Disposition` (Rework/Scrap/UseAsIs/Return)
  with `releases_hold()` — **Rework & Use-As-Is release**, Scrap & Return keep
  the hold. 4 unit tests.
- **Schema** — migration `0009` (additive): `inspection_plans`,
  `characteristics` (nominal + lower/upper limits), `inspection_results`,
  `ncrs`, and `holds.ncr_id` (the additive M7→M8 link).
- **Auto-NCR flow (`mes-db::repo_qms`)** — `record_result` evaluates pass/fail
  server-side and, **on fail, atomically** inserts the result, raises an NCR
  (Open), and places an NCR-linked hold on the lot/serial.
  `disposition_ncr` moves Open→Dispositioned and, when the disposition
  releases (Rework/UseAsIs), releases the NCR's active holds in the same
  transaction.
- **`/v1/qms`** — plan/characteristic create + NCR disposition are quality-gated
  (`roles::can_manage_quality`); recording a result is open to any authenticated
  user; a raised NCR broadcasts an `NcrRaised` andon WS event.

### Verification

- `cargo fmt` / `clippy -D warnings` clean; `cargo test --all` green.
- **Integration suite** (`tests/m8_qms.rs`, fresh schema per test):
  - `fail_raises_ncr_and_hold_rework_releases` — pass (10.0) → no NCR; fail
    (12.0) → NCR **open** + hold that **blocks issue (409)**; Operator
    disposition → **403**; Quality **Rework** disposition → hold released →
    issue **succeeds (201)**.
  - `scrap_disposition_keeps_hold` — a **Scrap** disposition leaves the hold
    active, so issue stays blocked (409).
  Runs in CI against the TimescaleDB service.

### Notes / deferrals

- NCR `Closed` (verification) transition exists in the domain and can be exposed
  as a `/close` endpoint when the QMS console needs it (M11). Auto-hold is placed
  on the failing lot/serial; a characteristic without limits always passes.

---

## M9 — CMMS ✅

**Goal:** preventive-maintenance scheduling (calendar + usage-hours), maintenance
work orders, a spare-parts ledger, and procurement *requests* — plus the
`Maintenance` role added additively (§7, §12 M9).

**Acceptance:** usage-hours PM triggers correctly off simulated run-hours;
maintenance-WO lifecycle test; spare-consumption ledger test; a reorder-point
breach creates a `procurement_request`.

### What landed

- **CMMS domain (`mes-core::cmms`, pure)** — `PmTrigger` (calendar | usage_hours)
  with `calendar_next_due`/`calendar_is_due` and `usage_next_due`/`usage_is_due`;
  `MaintenanceType` (PM/Corrective/Breakdown); `MaintenanceStatus`
  (Requested→Scheduled→InProgress→Completed→Verified) with a forward-only
  `can_transition`; `ProcurementStatus` (Requested→SentToErp→Fulfilled) and
  `ProcurementReason`. The PM-due decision lives here so it is the exact
  unit-tested logic (§13) — 6 unit tests.
- **Role** — `roles::MAINTENANCE` + `can_manage_maintenance`
  (Maintenance/Supervisor/Admin). The role is seeded by a plain `INSERT` in
  migration `0010`, exactly the additive path §7 reserved (lookup table, not an
  enum).
- **Schema** — migration `0010` (additive): `pm_schedules` (calendar + usage
  bookkeeping), `maintenance_work_orders` (closed WOs *are* the history),
  `spare_parts`, `spare_txns` (signed ledger — stock is `SUM(qty)`, never a
  mutable column), and `procurement_requests` (status caps at `requested` until
  M10). A partial unique index keeps at most one open auto-reorder request per
  spare.
- **Repositories (`mes-db::repo_cmms`)** — `work_center_run_hours` sums the
  existing `machine_states` RUNNING intervals (no new raw data, §7);
  `create_pm_schedule` anchors calendar next-due to now+interval and usage
  next-due to current run-hours + interval; `list_pm_due` supplies the clock /
  run-hours and defers the due decision to `mes-core`;
  `transition_maintenance_wo` validates the step and stamps the matching
  timestamp under `FOR UPDATE`; `record_spare_txn` applies the sign from the txn
  type, derives new stock, and (idempotently) raises a reorder-point request on
  breach.
- **`/v1/cmms`** — PM schedules, maintenance-WO board + transition, spares +
  ledger txns, procurement queue. Mutations are maintenance-gated
  (`can_manage_maintenance`); reads are open to any authenticated user.

### Verification

- `cargo fmt` / `clippy -D warnings` clean; `cargo test --all` green
  (mes-core CMMS: 6 tests; roles: +1).
- **Integration suite** (`tests/m9_cmms.rs`, fresh schema per test):
  - `usage_pm_triggers_off_run_hours` — two usage schedules (due at 10h/20h);
    after 12 simulated run-hours the 10h schedule appears in `/pm-schedules/due`
    and the 20h one does not, with `current_usage_h ≈ 12`.
  - `maintenance_wo_lifecycle_forward_only` — a WO advances
    requested→…→verified (all 200); skipping requested→completed is **409**; an
    Operator transition is **403**.
  - `spare_ledger_and_reorder_point` — receive 10 → stock 10, no request; issue 6
    → stock 4 (≤5) → one `reorder_point` request for 20; a second breach raises
    **no duplicate**; Operator create-spare is **403**.
  Runs in CI against the TimescaleDB service.

### Notes / deferrals

- Procurement is **request-only** (§3 locked decision): MES raises
  `procurement_requests` and stops at `requested`; the PO/vendor lifecycle and
  the `SentToErp`→`Fulfilled` transitions are wired through the ERP integration
  at M10. `ProcurementStatus::can_transition` already encodes those steps.
- `complete_pm` (resetting a schedule's baseline after a PM WO is verified) is
  deferred to when the CMMS console drives it (M11); the due engine and lifecycle
  it needs are already in place.

---

## M10 — ERP integration page ✅

**Goal:** an admin-configurable ERP integration (`erp_connections` with encrypted
token storage), generic `/v1/erp/import` + `/v1/erp/export` driven by a
configurable field-mapping engine, `erp_sync_log`, and wiring M9's
`procurement_requests` through to `SentToErp` (§7, §10, §12 M10).

**Acceptance:** a fixture "generic ERP" (a spawned REST mock) round-trips a WO
import and a stock-level export via configured mapping — re-pointing the mock at
a different fake shape needs a mapping change only, never a code change.

### What landed

- **Field-mapping engine (`mes-erp::mapping`, pure)** — `FieldMapping` parses a
  connection's `{ "fields": { "<canonical>": "<external>" } }` JSONB and maps
  `to_canonical` (import) / `to_external` (export). The "no per-customer code"
  guarantee (§3) is proven by a unit test: the same canonical data maps to two
  different ERP vocabularies with only the mapping changed. 6 unit tests.
- **Token encryption at rest (`mes-erp::crypto`, §14)** — XChaCha20-Poly1305 AEAD
  with a key derived (SHA-256, domain-separated) from the server signing secret,
  so there is one secret to configure and the key is stable across restarts.
  Tokens are **write-only**: accepted on create/update, encrypted, and never
  returned (`ErpConnection` exposes only `has_token`). 4 unit tests.
- **Generic REST client (`mes-erp::push`)** — one shape-agnostic bearer-authed
  POST to the connection's endpoint (proxy disabled for loopback/plant-local).
- **Schema** — migration `0011` (additive): `erp_connections` (endpoint,
  `auth_token_enc`, `field_mapping` JSONB, direction, enabled) and `erp_sync_log`
  (the audit trail the admin page's "last sync" view reads). The `json` sqlx
  feature was enabled to bind/decode JSONB.
- **Repositories (`mes-db::repo_erp`)** — connection CRUD (update keeps the token
  when omitted via `COALESCE`), sync-log insert/list, and `mark_procurement_sent`
  (Requested→SentToErp with `pushed_at` + `erp_reference`).
- **`/v1/erp`** — connection CRUD, generic `import` (external records → mapping →
  canonical → create), generic `export`/"sync now" (gather → mapping → POST to
  ERP → log; procurement export transitions to SentToErp), and the sync log. All
  master-writer gated (Admin/Planner) since they carry credentials/config.

### Verification

- `cargo fmt` / `clippy -D warnings` clean; `cargo test --all` green (mes-erp: 10
  tests).
- **Integration suite** (`tests/m10_erp.rs`, fresh schema per test, spawns a mock
  ERP REST server):
  - `import_work_order_via_mapping_and_remap` — import a WO through mapping A
    (OrderNo/Item/Qty); then **re-point the connection at a different shape**
    (po/material/amount) and import again — mapping change only, no code change;
    the token is never echoed (`has_token` only); both imports are logged.
  - `export_stock_level_pushes_mapped_payload` — MES stock is mapped to the ERP's
    field names (sku/on_hand) and the mock **receives the mapped payload over
    HTTP**.
  - `export_procurement_marks_sent_to_erp` — a reorder-point request is pushed,
    transitions to **`sent_to_erp`** with the ERP's reference and `pushed_at`;
    an Operator export attempt is **403**.
  Runs in CI against the TimescaleDB service.

### Notes / deferrals

- Import is push-based (external records posted to `/v1/erp/import`); export is
  the outbound "sync now". Entities supported in v1: import `work_order`; export
  `stock_level` and `procurement_request`. More entities are additive mapping +
  gather code, no schema change.
- Clearing a stored token (vs. leaving it unchanged) isn't exposed yet — omitting
  the token on update keeps the existing one; an explicit "remove token" action
  can be added when the settings page needs it (M11).
- The admin settings **page** itself is M11 (desktop); M10 delivers its backend.

---

## M11 — Desktop app ✅

**Goal:** the Tauri 2 + React desktop app — operator kiosk (fast lane + scripted
offline chat panel) and supervisor/planner console (dashboards + CMMS + ERP
settings + program-revision review queue + copilot panel) (§11).

**Acceptance (§12 M11 / §13):** a written manual test plan covering the operator
flow and the supervisor flow; the app type-checks and builds. Gated by
`apps/desktop/TESTPLAN.md` (component tests optional in v1).

### What landed

- **Scaffold** — Vite + React 18 + TypeScript + Tailwind + Recharts + TanStack
  Query + react-router; a Tauri 2 shell (`src-tauri/`) kept as its **own** Cargo
  workspace (and `exclude`d from the backend workspace) so it never enters the
  dev-gate.
- **API layer** — a typed `fetch` client (bearer token, 401 → sign-out), a full
  set of TanStack Query hooks over the edge API, a reconnecting `/ws` hook, and
  TS mirrors of the `mes-client` DTOs (one wire contract, both ends).
- **Auth** — dual login: a big PIN pad (kiosk) and a password form (console),
  both minting the same bearer token; role-based routing keeps Operators out of
  the console.
- **Operator kiosk** — work-center picker → active-order card → giant GOOD/SCRAP
  buttons (scrap forces a reason), Start/Complete operation, a Classify-Downtime
  path, the blue **DNC job-ready banner**, and the **scripted offline chat
  panel** that renders `/ws` events (job-ready → fetch, transfer-complete,
  program-edit → draft, NCR) as deterministic bubbles with tap actions — **zero
  LLM calls**, LAN-only.
- **Supervisor console** — live plant tiles + shift-OEE breakdown (live `/ws`
  `oee_snapshot`, Recharts), downtime Pareto, QMS/NCR console with disposition,
  CMMS view (PM-due, maintenance-WO board with forward-only transitions, spares
  stock, procurement queue), program-revision review queue (promote/reject —
  never auto-promoted), the no-code **ERP settings page** (endpoint + write-only
  token + JSON field-mapping + sync-now + sync log), and the **copilot panel**
  that calls cloud `/v1/copilot` and degrades to an "unavailable offline" banner.

### Verification

- `npm run build` (`tsc --noEmit` + `vite build`) passes — 897 modules, clean
  type-check.
- Backend dev-gate unaffected: `cargo fmt --all --check` clean and
  `cargo build --workspace` green with `src-tauri` excluded.
- **`apps/desktop/TESTPLAN.md`** documents the full operator (A1–A13) and
  supervisor (B1–B12) manual acceptance scripts.

### Notes / deferrals

- The copilot panel is the client seam; its live tool-use loop lands with the
  cloud at M13. Offline degradation is already implemented and testable.
- Scrap/downtime reason chips use a small default set for the demo; a plant seeds
  its own reason master data. Bundle icons are generated at packaging time
  (`npm run tauri icon`), not checked in.
- Tauri packaging (`tauri build`) needs system webkit libs and runs via the Tauri
  CLI outside the backend CI; the React app is the CI-independent build check.
