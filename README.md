# ElectronIx MES

Full Manufacturing Execution System + CMMS for Indian MSME manufacturers:
real-time OEE, downtime Pareto, work-order execution, scheduling, lot/serial
traceability, QMS, preventive maintenance, and CNC program orchestration through
the existing **ElectronIx DNC** daemon — offline-first on a plant edge box,
syncing to a multi-plant cloud, operated through Tauri 2 desktop apps.

> **Read [`CLAUDE.md`](./CLAUDE.md) fully before writing any code.** It is the
> authoritative build spec. Work milestone-by-milestone ([`MILESTONES.md`](./MILESTONES.md));
> never advance past a milestone with failing tests.

## Workspace layout

| Crate | Role |
|---|---|
| `mes-core` | Pure domain: types, state machine, OEE math, PM-due calc. **No I/O.** |
| `mes-db` | sqlx pool, embedded migrations, repositories |
| `mes-ingest` | `SignalSource` adapters (mqtt/http/tcp/sim) |
| `mes-dnc-bridge` | NDJSON client to `dnc-daemon`, transfer orchestration |
| `mes-erp` | Generic ERP import/export + field-mapping engine |
| `mes-agent-tools` | Shared read-only query tools for MCP *and* copilot |
| `mes-diagnostics` | Mirrors DNC's diagnostics module shape |
| `mes-sync` | Outbox writer, push/pull protocol |
| `mes-edge` | **Binary** — per-plant server (API + WS + ingest + DNC bridge + sync) |
| `mes-cloud` | **Binary** — multi-tenant server (API + sync + MCP + copilot) |
| `mes-client` | Shared API types/client used by the Tauri apps |

`apps/desktop` (M11), `apps/mobile` (M15, deferred), and `tools/machine-sim`
(M2) are reserved directories.

## Quick start (M0)

```bash
# Native dev-gate: format, lint, test
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all

# Full local stack: TimescaleDB + Mosquitto + edge + cloud
docker compose up --build
curl -fsS http://localhost:8080/healthz   # mes-edge liveness
curl -fsS http://localhost:8090/healthz   # mes-cloud liveness
```

### Configuration (env only, §14)

| Variable | Default | Used by |
|---|---|---|
| `MES_EDGE_BIND` | `0.0.0.0:8080` | edge |
| `MES_CLOUD_BIND` | `0.0.0.0:8090` | cloud |
| `DATABASE_URL` | *(unset → liveness-only boot)* | both |
| `MES_DB_MAX_CONN` | `10` (edge) / `20` (cloud) | both |
| `RUST_LOG` | `info` | both |

## License

Proprietary — ElectronIx.
