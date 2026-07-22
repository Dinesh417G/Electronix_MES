# CLAUDE.md — ElectronIx MES

> Build spec for Claude Code. Read this file fully before writing any code.
> Work milestone-by-milestone (see §12). Never advance past a milestone with failing tests.
> This spec grows over time — new milestones get appended, old ones don't get rewritten.

---

## 1. What this is

**ElectronIx MES** is a full Manufacturing Execution System + CMMS for Indian MSME manufacturers:
real-time OEE, downtime Pareto, work-order execution, scheduling, lot/serial traceability,
quality management (QMS), preventive maintenance, and CNC program orchestration through the
existing **ElectronIx DNC** daemon — running **offline-first on a plant edge box**, syncing to a
**multi-plant cloud**, operated through a **Tauri 2 desktop app** (operator kiosk + supervisor
console, both with a conversational layer) and a **Tauri 2 Mobile app** (deferred — see §3).

Three deliverables, one Cargo workspace:

1. **Backend** — `mes-edge` (per-plant server) and `mes-cloud` (multi-tenant aggregator, MCP
   server, copilot), both Rust/Axum. This is where almost all new scope in this revision lands.
2. **Desktop app** — Tauri 2 + React. Operator Kiosk (hybrid: fast buttons + scripted chat panel)
   and Supervisor/Planner console (dashboards + LLM copilot panel).
3. **Mobile app** — deferred. Foundation only: shared `mes-client` crate stays mobile-ready, no
   mobile UI built yet.

## 2. Design inspirations & reference repos

Patterns studied, **no code copied** from third parties:

- **Libre** (Grafana/InfluxDB/Postgres): machines push metrics into buckets; schedulers/operators
  run orders against dashboards. Collapsed into one TimescaleDB engine here (see §7).
- **OpenMES** (AGPL-3.0) / **Carbon**: feature-scope references for OEE, Pareto, traceability, QMS.

**ElectronIx DNC** (`electronix_dnc` repo — same owner, not third-party) is a *direct* reference,
not just an inspiration — its proven, shipped infrastructure gets reused pattern-for-pattern:

- **CI/CD**: `.github/workflows/ci.yml` + `release.yml` — dev-gate on every push, tag-triggered
  signed draft release. MES copies this shape, adapted for a multi-target build (§5, §14).
- **Diagnostics**: `crates/dnc-daemon/src/diagnostics/` (`heartbeat.rs`, `manual.rs`,
  `error_trigger.rs`, `redact.rs`, `github.rs`, `buffer.rs`, `crash.rs`) — same module shape,
  new `mes-diagnostics` crate (§8.5, M14).
- **DNC integration**: MES does **not** reimplement program transfer. `dnc-daemon` already owns
  serial/FTP comms with the CNC and exposes a local command/event surface on `127.0.0.1:8765`
  (NDJSON) plus an operator-triggered auto-fetch convention (trigger program, DC1-armed send).
  MES calls into that daemon and listens to its events (§8.4, M4).
- **MCP**: reuse `rmcp` (official Rust MCP SDK) and the OAuth-gated-tiers pattern already used for
  other ElectronIx connectors (Bids/RFQ, Quotation/BOM) (§8.6, M13).

**Hard rule unchanged:** clean-room, proprietary codebase. Never copy/port/paraphrase source from
AGPL/GPL third-party projects (OpenMES, Carbon, Libre). Reuse from ElectronIx's own repos (DNC, ID)
is fine — same owner, same license.

## 3. Locked decisions (do not re-litigate without discussion)

| Decision | Choice | Why |
|---|---|---|
| v1 scope | Full MES + CMMS: OEE, downtime Pareto, WOs, scheduling, traceability, QMS, preventive maintenance, DNC orchestration, ERP integration, MCP + copilot | Product decision, expanded this revision |
| Ingestion | Hybrid: auto machine signals + operator manual entry | Machines give counts/run-stop; humans give reasons/causes/inspections |
| Deployment | Hybrid: edge box per plant (offline-first, source of truth) + cloud aggregator | Indian plant internet is unreliable |
| DB | PostgreSQL 16 + TimescaleDB, edge *and* cloud | One engine everywhere |
| Backend | Rust, Axum, sqlx, tokio | House stack |
| Desktop | Tauri 2 + React 18 + TS + Vite + Tailwind + Recharts + TanStack Query | House stack |
| Mobile | Tauri 2 Mobile — **deferred**, foundation only (§1) | Explicit scope cut this revision |
| DNC integration | `mes-edge` orchestrates the *existing* `dnc-daemon` over its local NDJSON socket; never reimplements transfer | Don't rebuild what's already shipped and proven |
| Edited-program handling | Operator edit → saved as a **draft revision**; supervisor reviews and **promotes** it to master | Data-integrity gate on CNC programs |
| CI/CD | Mirrors DNC's `ci.yml`/`release.yml` shape, adapted for multi-target (services + Tauri desktop) | Proven pattern, don't reinvent |
| Diagnostics | Mirrors DNC's `diagnostics/` module 1:1 in a new `mes-diagnostics` crate | Proven pattern; redaction gets *stricter* here (§8.5) |
| ERP | Admin-configurable integration page (endpoint/token/field-mapping) + generic REST import/export endpoints. **No per-customer code** — you configure it in the UI | Every MSME customer's ERP differs; nobody knows the exact APIs up front |
| CMMS | Full: PM scheduling (calendar + usage-based) + maintenance work orders + history + spares/inventory **with procurement requests** | Explicit scope decision this revision |
| Procurement depth | MES tracks stock + raises procurement *requests*; the actual PO/vendor lifecycle stays in ERP via the same integration | Avoid rebuilding a second procurement system — flagged in §17, confirm before M9 |
| Operator interaction | **Hybrid kiosk**: fast tap-buttons for Good/Scrap/state (high-frequency, must stay instant) + a scripted, deterministic, **offline** chat/notification panel (job-ready, downtime-classify, DNC fetch/edit prompts). **Never LLM-backed** — cannot depend on internet or model latency | Shop floor must work with zero connectivity |
| Supervisor interaction | Existing dashboard + an **LLM-backed copilot panel** (Anthropic API via `mes-cloud`, tool-use), online-only, degrades gracefully offline | This is where open-ended "why is OEE down / what should I fix" lives |
| MCP | `mes-cloud` hosts an `rmcp`-based MCP server; OAuth-gated, tenant-scoped; customers add it as a custom connector in their own Claude account | Matches ElectronIx's existing connector pattern |
| Claude Code model routing | Subagents in `.claude/agents/`, `model:` field per task weight (§15) | Manage build cost across many sessions |
| ElectronIx ID | Standalone v1, integration seam reserved (§8.7) — **unchanged** | Sequencing |
| License | Proprietary (ElectronIx) | Unchanged |

## 4. Architecture

```
                         PLANT (LAN, works fully offline)
 ┌───────────────────────────────────────────────────────────────────────┐
 │  Machines ──MQTT/HTTP/TCP──┐            CNC controllers ──serial/FTP──┐│
 │                            ▼                                         ▼│
 │            ┌────────── mes-edge ───────────┐        ┌── dnc-daemon ──┐│
 │            │ Axum API+WS · ingest adapters │◄──────►│ (existing,     ││
 │            │ state machine · OEE · CMMS    │ NDJSON │  unmodified)   ││
 │            │ DNC-bridge · ERP adapter      │127.0.0.1:8765           ││
 │            │ Postgres+TimescaleDB · outbox │        └────────────────┘│
 │            └───────┬───────────────┬───────┘                         │
 │              LAN HTTP/WS       LAN HTTP/WS                           │
 │        Tauri Desktop (Kiosk)  Tauri Desktop (Supervisor)             │
 │        buttons + scripted     dashboards + LLM copilot panel ────────┼──┐
 │        chat panel (offline)   (needs cloud, degrades gracefully)     │  │
 └───────────────────────────┬───────────────────────────────────────────┘  │
                             │ HTTPS sync: push outbox / pull commands    │
                             ▼                                            │
                ┌──────────  mes-cloud  ──────────┐                       │
                │ Multi-tenant (org → plants)      │◄──────────────────────┘
                │ Postgres + TimescaleDB           │
                │ ERP export · alerts               │
                │ /v1/copilot  (Anthropic API, tool-use) │
                │ MCP server (rmcp, OAuth, tenant-scoped)│◄── customer's own Claude
                └──────────────┬────────────────────┘      (custom connector)
                        Tauri Mobile (deferred)
```

**Source-of-truth rule unchanged:** edge owns all operational data; cloud is read-mostly
aggregator + command queue. `dnc-daemon` is untouched and stays the sole owner of physical
CNC comms — `mes-edge` is a client of it, never a replacement.

## 5. Tech stack (pinned)

- Rust stable, edition 2021. `cargo clippy -- -D warnings` + `rustfmt` clean at every milestone.
- Axum 0.7+, tokio, tower-http, sqlx 0.8 (Postgres), serde, ulid, thiserror + anyhow, tracing,
  utoipa (OpenAPI), argon2, jsonwebtoken.
- MQTT: `rumqttc`. DB: `timescale/timescaledb:latest-pg16`. Migrations via `sqlx migrate`.
- Frontend: React 18, TypeScript, Vite, Tailwind, Recharts, TanStack Query, react-router.
- Desktop/Mobile: Tauri 2.
- **New this revision:**
  - `rmcp` (official Rust MCP SDK) — MCP server on `mes-cloud`, same crate ElectronIx already uses for other connectors.
  - Anthropic Messages API (server-side key, `mes-cloud` only — never shipped in the desktop binary) — copilot, tool-use enabled, streaming.
  - `octocrab` or plain `reqwest` — diagnostics shipping to a private GitHub repo (mirror DNC).
  - Thin NDJSON client over TCP/local-socket for the `dnc-daemon` bridge — exact protocol confirmed from `dnc-daemon` source at build time, not guessed here (§8.4).
- Cloud hosting: DigitalOcean droplet via Coolify, TLS at proxy. Edge: 4-core/8GB mini PC, Docker.

## 6. Repository layout

```
electronix-mes/
├── CLAUDE.md
├── MILESTONES.md
├── docker-compose.yml
├── Cargo.toml                    # workspace
├── .github/workflows/
│   ├── ci.yml                    # mirrors DNC's dev-gate pattern (M0)
│   └── release.yml               # mirrors DNC's tag→signed→draft pattern (M14)
├── .claude/agents/                # subagent model routing (§15)
├── crates/
│   ├── mes-core/                 # pure domain: types, state machine, OEE math, shift/PM-due calc. NO I/O.
│   ├── mes-db/                   # sqlx pool, migrations, repositories
│   ├── mes-ingest/                # SignalSource adapters (mqtt/http/tcp/sim)
│   ├── mes-dnc-bridge/            # NEW — NDJSON client to dnc-daemon, transfer orchestration
│   ├── mes-erp/                   # NEW — generic import/export adapter + field-mapping engine
│   ├── mes-agent-tools/           # NEW — shared query/action tools, used by MCP *and* copilot
│   ├── mes-diagnostics/           # NEW — mirrors DNC's diagnostics/ module shape
│   ├── mes-sync/                  # outbox writer, push/pull protocol
│   ├── mes-edge/                  # binary: plant server (API+WS+ingest+DNC-bridge+sync client)
│   ├── mes-cloud/                 # binary: multi-tenant server (API+sync+MCP server+copilot)
│   └── mes-client/                # shared API types/client used by Tauri apps
├── apps/
│   ├── desktop/                   # Tauri 2 + React (kiosk mode + supervisor console)
│   └── mobile/                    # NOT built yet — directory reserved, empty scaffold only
└── tools/
    └── machine-sim/                # scripted virtual machines (MQTT/HTTP signals)
```

`mes-core` stays I/O-free and 100% unit-testable. `mes-agent-tools` is deliberately its own crate
so the MCP server and the copilot never diverge — one tool implementation, two front doors.

## 7. Domain model (ISA-95-lite)

All tables `id TEXT` (ULID), `created_at`, `updated_at`. Hypertables marked **[HT]**.
Tables carried over unchanged from the original spec are named but not re-described.

**Equipment & calendar** — unchanged: `sites`, `areas`, `work_centers`, `shifts`, `planned_stops`.

**People** — `roles` (code, label), seeded Admin/Planner/Supervisor/Operator/Quality at M1,
**Maintenance added additively at M9**. `roles` is a lookup table rather than a hardcoded
enum specifically so adding Maintenance later stays a plain insert, not a schema change —
keeps the "additive-only after M5" rule in §14 honest. `users.role_code → roles.code`.
`audit_log` unchanged.

**Products & routing** — unchanged: `parts`, `boms`, `routings`, `routing_ops`. New: `programs`
(part/routing_op → the program identifier `dnc-daemon` knows it by + target machine) — the join
between MES routing and DNC's program library.

**Execution** — unchanged: `work_orders`, `wo_operations`, `machine_events` [HT], `machine_states`,
`production_counts` [HT], `downtime_events`, `downtime_reasons`/`scrap_reasons`.

**DNC orchestration (new)**:
- `dnc_transfer_events` (wo_operation, program_id, direction: to_machine|from_machine, status:
  Scheduled→Notified→Fetched→Completed|Failed, triggered_at, completed_at, dnc_daemon_ref)
- `program_revisions` (program_id, revision_no, source: operator_edit, content_ref, status:
  Draft→Promoted|Rejected, submitted_by, submitted_at, promoted_by, promoted_at)

**Traceability** — unchanged: `lots`, `serials`, `genealogy`, `material_txns`.

**QMS** — unchanged: `inspection_plans`, `characteristics`, `inspection_results`, `ncrs`, `holds`.

**CMMS (new)**:
- `pm_schedules` (work_center, trigger_type: calendar|usage_hours, interval_value, last_done_at /
  last_done_usage_h, next_due_at / next_due_usage_h, checklist_ref?) — usage-hours trigger reuses
  the existing `machine_states` RUNNING intervals for cumulative run-hours; no new raw data needed.
- `maintenance_work_orders` (work_center, type: PM|Corrective|Breakdown, status:
  Requested→Scheduled→InProgress→Completed→Verified, technician_id, failure_code?, opened_at,
  closed_at, notes) — **doubles as maintenance history**; closed WOs *are* the history, no
  separate history table.
- `spare_parts` (code, name, uom, reorder_point, reorder_qty)
- `spare_txns` (spare_part, maintenance_wo?, qty, type: issue|receive|adjust, ts, user) — ledger,
  same pattern as `material_txns`; current stock is derived by summing txns, never a mutable
  stock column.
- `procurement_requests` (spare_part, qty_requested, reason: reorder_point|manual, status:
  Requested→SentToErp→Fulfilled, pushed_at, erp_reference?) — **request only**. The PO/vendor
  lifecycle stays in ERP via the integration below. Confirm this split before M9 — see §17.

**ERP integration (new)**:
- `erp_connections` (site/org, endpoint_url, auth_token — encrypted at rest, field_mapping JSONB,
  direction: import|export|both, enabled) — this is what the admin integration page edits.
- `erp_sync_log` (connection, direction, entity, payload_ref, status, ts) — audit trail; the
  admin page's "last sync" view reads from here.

**MCP / Copilot (new)**:
- `copilot_messages` (org, user, role: user|assistant, content, tool_calls JSONB, ts) — audit log
  only. The copilot itself is stateless request/response with tool-use, not a stored session.
- MCP auth reuses `orgs`/`plants` (cloud multi-tenancy already in place): one OAuth client per
  org, tenant-scoping enforced inside `mes-agent-tools` at the query layer, not only at the API
  edge — a bug in the MCP transport must never leak across tenants.

**Sync plumbing** — unchanged: `outbox`, `applied_entries`. Cloud adds `orgs`, `plants`.

**Timescale specifics unchanged**: `machine_events` + `production_counts` as hypertables,
compress after 7d, retain raw 90d / counts 2y, continuous aggregates `oee_hourly`, `oee_by_shift`.

## 8. Core engines

### 8.1 Machine state machine — unchanged from v1 (in `mes-core`, see original rules: cycle
pulses, micro-stop/down thresholds, operator classification/split, planned stops, shift-boundary
interval closing, 2s debounce).

### 8.2 OEE engine — unchanged from v1 (A × P × Q, Six Big Losses waterfall, dual Rust+SQL
implementation cross-checked by a golden-file test within 0.1%).

### 8.3 Sync protocol — unchanged from v1 (outbox in the same transaction as every syncable
write, push in batches ≤500, idempotent apply via `applied_entries`, resumable after 24h+ offline).

### 8.4 DNC orchestration (new, in `mes-dnc-bridge`)

Trigger: a `wo_operations` row completes while its work center has a next queued operation
(by scheduling priority) whose `programs` row resolves to something `dnc-daemon` knows about.

1. `mes-edge` detects completion → resolves the next op's `program_id`.
2. Sends an NDJSON command to `dnc-daemon` (`127.0.0.1:8765`) to stage/send the transfer.
   **Exact command name and payload shape get confirmed from the real `dnc-daemon` source at
   the start of this milestone — not assumed here.**
3. Creates a `dnc_transfer_events` row (Scheduled), pushes a WS event → kiosk chat panel renders
   "Job X ready — fetch program Y" (scripted, offline, no LLM call).
4. Operator fetches — or `dnc-daemon`'s own trigger-program auto-fetch convention fires —
   `dnc-daemon` emits a completion event, `mes-edge` marks the transfer Completed and clears the
   kiosk prompt.
5. If the operator edited the program at the machine and sends it back (existing `dnc-daemon`
   receive path), `mes-edge` creates a `program_revisions` row (Draft) and a WS event to the
   supervisor console. **Never auto-promoted** — supervisor reviews and promotes (§3).

### 8.5 Diagnostics (new, mirrors DNC's module shape 1:1)

`mes-diagnostics` on both `mes-edge` and `mes-cloud`: `heartbeat` (scheduled), `manual` (Send
Diagnostics button in the supervisor console), `error_trigger` (panics/critical errors), `redact`,
`buffer`, `crash` — same shape as `dnc-daemon/src/diagnostics/`. **Redaction is stricter here**:
MES diagnostics can carry production counts, scrap reasons, and business data DNC never touched,
so `redact` needs its own allowlist — structural/error data only, never customer part numbers,
customer names, pricing, or raw inspection values. Shipping is **opt-in per customer**, not
on-by-default like DNC's (§17).

### 8.6 MCP + Copilot shared tools (new, `mes-agent-tools`)

One tool implementation, two front doors:
- `get_oee`, `get_downtime_pareto`, `get_wo_status`, `get_ncr_queue`, `get_trace`,
  `get_maintenance_due` — **read-only in v1**. No destructive/write actions exposed to either
  front door yet (§16).
- **MCP server** (`mes-cloud`, `rmcp`): customer adds it as a custom connector in their own
  Claude account, OAuth-gated, tenant-scoped to their org only.
- **Copilot** (`mes-cloud` `/v1/copilot`, called from the supervisor desktop panel): same tools,
  called server-side via the Anthropic Messages API with tool-use, streamed back to the panel.
  Runs server-side only — no API key ever ships in the desktop binary.

### 8.7 Integration seam — ElectronIx ID (unchanged, still reserved)

`work_centers.external_ref` stays reserved for a future ElectronIx ID machine-passport link.
Not built this revision — no ID-specific code yet.

## 9. Ingestion (`mes-ingest`) — unchanged from v1

`SignalSource` trait, MQTT/HTTP/TCP-line/sim adapters, `signal_sources` table, unknown sources
logged and dropped never crash ingest. **Note:** DNC transfer events do *not* flow through this
path — they're a separate concern owned by `mes-dnc-bridge` (§8.4), keeping "production signals"
and "DNC orchestration events" cleanly separated.

## 10. API surface (edge unless noted; utoipa-documented)

Unchanged from v1: `/v1/auth`, `/v1/master`, `/v1/orders`, `/v1/exec`, `/v1/ingest`,
`/v1/analytics`, `/v1/trace`, `/v1/qms`, `/v1/sync` (cloud), `/ws`.

**New this revision:**
- `/v1/dnc` — transfer status, manual trigger/retry, program-revision list/promote (edge)
- `/v1/cmms` — PM schedules, maintenance WOs, spares, procurement requests (edge)
- `/v1/erp` — connection config CRUD (the admin integration page's backend), manual "sync now",
  sync log (edge)
- `/v1/copilot` — chat endpoint, tool-use loop (cloud only)
- MCP is not a REST route — served over `rmcp`'s own transport (cloud only, §8.6)

## 11. Apps

**Desktop — Operator Kiosk (hybrid).** Fast lane unchanged from v1: My Work Center → active order
card → giant Good/Scrap buttons (scrap forces reason pick) → red *Classify Downtime* banner →
inspection prompts → lot scan/print. **New:** a chat/notification panel — scripted, event-driven,
deterministic, zero LLM calls, works fully offline. Renders backend events as "virtual supervisor"
chat bubbles with tap-to-reply quick actions: job-ready/fetch-program, downtime-classify,
"you edited this program — send it back?" Free-text input is logged but not sent to any model in
v1. Kiosk stays LAN-only to `mes-edge`, zero cloud dependency, chat panel included.

**Desktop — Supervisor/Planner.** Unchanged: live plant tiles, scheduling board (manual drag),
OEE dashboards, Pareto, traceability search, QMS console, master data, sync status. **New:**
CMMS view (PM-due list, maintenance WO board, spares stock, procurement requests), an ERP
integration settings page (paste endpoint/token/field-mapping, view sync log — this is the
no-code-needed page), a program-revision review queue (promote/reject drafts from §8.4), and a
**copilot chat panel** — LLM-backed, calls `mes-cloud` `/v1/copilot`, same tools as the MCP
server. Requires connectivity; degrades to a plain "copilot unavailable offline" banner while the
rest of the console keeps working over LAN as before.

**Mobile — deferred.** `mes-client` stays mobile-ready; no mobile UI built this revision.

## 12. Milestones (gate: all tests green + MILESTONES.md updated before advancing)

**M0 — Scaffold + dev CI.** Workspace, docker-compose (timescaledb+mosquitto), sqlx migrate
baseline, `ci.yml` mirroring DNC's dev-gate (fmt+clippy+test on push), tracing, health endpoints.
*Accept:* `cargo test` green, `docker compose up` → `/healthz` OK on edge.

**M1 — Master data + auth.** Equipment/product/people tables (`roles` as a lookup table, not an
enum — see §7), CRUD, argon2+JWT, PIN/badge kiosk login, audit_log. *Accept:* CRUD + role-
enforcement integration tests (Operator cannot touch master data).

**M2 — Ingestion + state machine.** `SignalSource` trait, adapters, hypertables, state machine
producing `machine_states` + auto `downtime_events`. *Accept:* scripted hour (run→micro-stop→
down→run) matches golden file; unknown source dropped gracefully.

**M3 — Work orders + execution.** WO lifecycle, `/v1/exec`, counts, scrap+reasons, downtime
classify/split, WS channel, `programs` table wired to `routing_ops`. *Accept:* full simulated
order start→complete via API; WS events observed in a test client.

**M4 — DNC orchestration.** *Start by reading the real `dnc-daemon` source for its actual NDJSON
command/event surface — do not assume the shape in §8.4.* `mes-dnc-bridge`: auto-schedule on
job completion, `dnc_transfer_events`, kiosk notification event, `program_revisions` created as
Draft on an edited-program receive. *Accept:* simulated job-complete → transfer scheduled →
simulated dnc-daemon ack → event clears; simulated edited-program receive → draft revision
created and explicitly **not** auto-promoted.

**M5 — Downtime analytics.** Reason trees, Six-Big-Losses mapping, Pareto+trend queries.
*Accept:* seeded week of data → Pareto ordering matches hand-computed fixture.

**M6 — OEE.** Rust OEE engine + continuous aggregates (`oee_hourly`, `oee_by_shift`),
`/v1/analytics`, live shift OEE over WS. *Accept:* golden-day test — A/P/Q/OEE within 0.1% in
both Rust and SQL paths; shift-boundary test passes. **Schema freeze: core production/QMS/CMMS
tables additive-only from here on (§14).**

**M7 — Traceability.** Lots/serials, genealogy, recursive forward/backward trace, barcode format
`EMX1|<type>|<id>`. *Accept:* 3-level assembly fixture traces both directions; held lot blocks issue.

**M8 — QMS.** Plans/characteristics/results with auto pass/fail, auto-NCR+hold on fail,
disposition lifecycle. *Accept:* fail → NCR+hold created; Rework disposition releases correctly;
Quality-role gating enforced.

**M9 — CMMS.** `pm_schedules` (calendar + usage-hours off existing `machine_states` RUNNING
intervals — no new raw data), `maintenance_work_orders`, `spare_parts`+`spare_txns` ledger,
`procurement_requests` (status caps at `Requested` until M10's ERP push exists), `Maintenance`
role added as a plain insert into `roles`. *Accept:* usage-hours PM triggers correctly off
simulated run-hours; maintenance WO lifecycle test; spare-consumption ledger test; a reorder-point
breach creates a `procurement_request`.

**M10 — ERP integration page.** `erp_connections` CRUD with encrypted token storage, generic
`/v1/erp/import` + `/v1/erp/export` with a configurable field-mapping engine, the admin settings
page, `erp_sync_log`. Wire M9's `procurement_requests` through to `SentToErp`. *Accept:* a fixture
"generic ERP" (simple REST mock in tests) round-trips a WO import and a stock-level export via
configured mapping — re-pointing the mock at a *different* fake shape needs a mapping change only,
never a code change.

**M11 — Desktop app.** Kiosk fast lane + scripted offline chat panel (§11); supervisor dashboard +
CMMS view + ERP settings page + program-revision review queue. *Accept:* manual script — operator
flow (PIN login, run order, classify downtime, scrap w/ reason, DNC job-ready prompt→fetch,
program-edit→draft, inspection, complete+lot) and supervisor flow (live tiles, Pareto, OEE, drag
schedule, CMMS board, ERP config save+test-sync, promote a draft revision).

**M12 — Cloud + sync.** `mes-cloud`, orgs/plants/enrollment, outbox push/pull, idempotent apply,
remote WO creation, multi-plant dashboards. *Accept:* kill network 24h in test, replay outbox,
cloud converges; duplicate batch is a no-op; remote WO appears on edge.

**M13 — MCP + Copilot.** `mes-agent-tools` (read-only query set, §8.6), `rmcp` MCP server on
`mes-cloud` (OAuth, tenant-scoped), `/v1/copilot` (Anthropic API, tool-use, streaming), desktop
copilot panel wired in. *Accept:* MCP server answers a tenant-scoped query correctly and *refuses*
a cross-tenant query in tests; copilot round-trips a "why is OEE down today" question using real
tool calls against seeded data; desktop panel shows the offline-degradation banner when cloud is
unreachable, rest of the console keeps working.

**M14 — Release CI + Diagnostics.** Extend M0's `ci.yml` into `release.yml` (tag-triggered,
signed, draft-then-publish) for Tauri desktop artifacts + versioned edge/cloud images, mirroring
DNC's proven shape. `mes-diagnostics` (heartbeat/manual/error_trigger/redact/buffer/crash),
shipping **opt-in per customer** to a private GitHub repo. *Accept:* a throwaway tag smoke-tests
the full release pipeline exactly like DNC's v0.2.99 dry run; manual Send-Diagnostics round-trips
to a test repo with redaction verified against a fixture payload containing fake sensitive fields.

**M15 — Mobile app. Deferred — do not start until explicitly instructed.** When resumed: Tauri 2
Mobile Android build reusing `mes-client` + React components, barcode plugin, live tiles + andon
feed, NCR approval, trace lookup.

## 13. Testing bar

- `mes-core`: state-machine golden scenarios, OEE property tests (unchanged) + PM-due calc unit
  tests (calendar and usage-hours) + program-revision state-transition tests.
- `mes-db`/APIs: sqlx integration tests against dockerized TimescaleDB, fresh schema per test.
- End-to-end: machine-sim scripted days; `machine-sim` gains a **virtual dnc-daemon mode** so
  M4's transfer/edit-back flow is tested against a simulated daemon, never real CNC hardware.
- Sync: offline/replay/idempotency suite (unchanged).
- MCP/Copilot: **tenant-isolation tests are mandatory, not optional** — a cross-tenant data leak
  here is a security bug, not a feature bug.
- Diagnostics: redaction test suite — fixture payloads with fake sensitive fields, assert none
  of them survive `redact`.
- Frontend: component tests optional in v1; M11 gated by a written `apps/*/TESTPLAN.md`.

## 14. Conventions & guardrails for Claude Code

- Plan first inside each milestone; implement; make tests green; append a short report to
  MILESTONES.md.
- No `unwrap()`/`expect()` outside tests. `thiserror` per crate, `anyhow` only in binaries. Every
  handler in a tracing span.
- sqlx compile-checked queries wherever possible; migrations never edited after merge — new
  migration files only.
- All timestamps `timestamptz` UTC; shift/PM/OEE math converts via site timezone in `mes-core`.
- Money/qty: `NUMERIC` in DB, `rust_decimal` in code. Never float for quantities.
- Ask before deviating from §3 locked decisions or changing schema post-M6.
- Secrets via env only; per-device ingest tokens and ERP auth tokens hashed/encrypted at rest;
  CORS locked to app origins.
- `mes-agent-tools` functions are the **only** way MCP or the copilot touch the database — no raw
  queries inside the MCP/copilot handlers themselves, so tenant-scoping lives in exactly one place.
- `dnc-daemon` is a **read+command client only** from `mes-dnc-bridge`. Never modify `dnc-daemon`'s
  own source as part of this project — if a real change there is genuinely needed, that's a
  separate, explicit ask, not a silent edit made while building MES.

## 15. Claude Code model routing

This build spans many sessions — route model choice by task, not default-to-strongest every time.
Define subagents in `.claude/agents/` (checked into the repo), each with a real Claude Code
`model:` frontmatter field:

| Subagent | Model | Use for |
|---|---|---|
| `scaffold` | `haiku` | boilerplate: CRUD handlers, migration files, repetitive DTOs/fixtures, test scaffolding |
| `builder` | `sonnet` (default) | most milestone implementation work |
| `architect` | `opus` | schema design, state-machine/OEE-math correctness, DNC protocol/sync idempotency, anything touching §3 |

Main conversation stays on Sonnet by default; invoke `architect` explicitly for schema-affecting
or correctness-critical steps, delegate repetitive scaffolding to `scaffold`. Anything not listed
uses `model: inherit` (Claude Code's actual default). Capability tiers above Opus exist but aren't
standard subagent model aliases — no need to reach for them on this build.

## 16. Non-goals (v1 — do not build)

Auto finite-capacity scheduling (manual drag board only) · OPC UA / Modbus / Fanuc FOCAS direct
(DNC already owns CNC comms, §8.4) · ElectronIx ID integration (seam only, §8.7) · attendance/
payroll · multi-language UI (English only, i18n map ready for Tamil later) · ZPL label printing
(PDF lot labels in v1) · web frontend · iOS build · a native PO/vendor-management system inside
MES (procurement stays in ERP, §3 — confirm via §17) · destructive/write actions exposed through
MCP or the copilot (read-only tools only, §8.6) · voice interface for the chat panel · a
payments/licensing server (DNC's licensing pattern is a future reference, not built here).

## 17. Open questions (answer before the affected milestone)

1. Target plant size for perf assumptions — default **50 machines, 5k events/min burst** (M2).
2. Procurement split — confirm MES only *requests*, never issues POs, before M9/M10 get built
   around that assumption (locked as a design call in §3, worth one explicit yes first).
3. `dnc-daemon`'s exact NDJSON command/event names — read from source at the start of M4, not
   assumed here.
4. Diagnostics opt-in default: off with a settings toggle, or on-by-default like DNC with an
   opt-out? Leaning opt-in given the more sensitive data MES carries (M14).
5. First native ERP adapter in v2: Tally vs ERPNext (post-M15; generic REST covers v1).
6. Serial-level trace default per part, or lot-only for launch customers (M7).
7. Cloud alerting channel for andon/NCR/PM-overdue escalation — WhatsApp templates via MSG91,
   needs template approval lead time (M12+).
8. Edge hardware standard to certify — any Docker-capable 4-core/8GB mini PC assumed for now.
