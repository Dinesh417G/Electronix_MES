# ElectronIx MES Desktop — Manual Test Plan (M11)

Per §13, M11 is gated by this written test plan rather than automated UI tests
(component tests are optional in v1). It exercises the operator kiosk flow and
the supervisor/planner console flow (§12 M11 acceptance).

## Prerequisites

1. Backend up: `docker compose up` (TimescaleDB + mosquitto) and `mes-edge`
   running on `http://localhost:8080` (set `MES_JWT_SECRET`, `DATABASE_URL`).
2. Seed master data + users via `/v1/master` (Admin token): a site → area → work
   center, a part, an Operator (with PIN), and a Supervisor (with password).
3. Frontend: from `apps/desktop`, copy `.env.example` → `.env` (point
   `VITE_MES_EDGE_URL` at the edge), then `npm install` and `npm run dev`
   (or `npm run tauri dev` for the desktop shell).

Build check (CI-independent): `npm run build` must pass (`tsc --noEmit` +
`vite build`).

## A. Operator kiosk flow

| # | Step | Expected |
|---|------|----------|
| A1 | Launch, choose **Use PIN**, enter operator username + PIN, Sign in | Lands on the kiosk (role Operator cannot reach the console) |
| A2 | Pick a work center in the header | Released orders for the plant appear |
| A3 | Select a released order | The active operation card shows, with Good/Scrap counters |
| A4 | Tap **Start operation** | Op moves to `in_progress`; GOOD/SCRAP buttons appear |
| A5 | Tap **GOOD +1** a few times | `qty_good` increments each tap (instant, no reload) |
| A6 | Tap **SCRAP** | Reason modal appears; recording is blocked until a reason is picked |
| A7 | Pick a reason, **Record scrap** | `qty_scrap` increments |
| A8 | Trigger a DNC transfer on the edge (job complete → next op) | Blue **“Job ready — fetch program Y”** banner + a chat bubble with a **Fetch program** action appear (scripted, offline, no LLM) |
| A9 | Tap **Fetch program** (or let the daemon auto-fetch); edge emits completion | Banner clears; a “Program transferred ✓” bubble appears |
| A10 | Simulate an operator program edit sent back (draft revision) | A chat bubble: “You edited a program — saved as a draft for supervisor review” |
| A11 | Tap **Classify downtime**, enter a downtime event id + reason, Classify | Request succeeds (200); modal closes |
| A12 | Tap **Complete operation** | Op moves to `completed` |
| A13 | Pull the LAN cable / stop the edge briefly | Chat panel badge flips to **reconnecting**; buttons still render; reconnects when the edge returns |

## B. Supervisor / planner console flow

| # | Step | Expected |
|---|------|----------|
| B1 | Sign in with **Use password**, supervisor credentials | Lands on **Live & OEE** |
| B2 | Observe live tiles | One tile per work center; OEE % updates from `/ws` `oee_snapshot` events; header shows ● live |
| B3 | Click a tile | OEE breakdown chart (A/P/Q/OEE) renders for that work center |
| B4 | Open **Downtime Pareto**, pick a work center | Ranked bar chart of loss minutes by reason over 7 days |
| B5 | Open **Quality (NCRs)** | Open NCRs listed; use a disposition button (e.g. Rework) → status becomes `dispositioned` (Quality-gated on the server) |
| B6 | Open **Maintenance** | PM-due list, maintenance-WO board (advance a WO one step; skipping is rejected by the server), spares stock (low stock in red), procurement queue |
| B7 | Open **Program reviews** | Draft revisions listed; **Promote** a draft → moves to `promoted`; **Reject** another → `rejected` |
| B8 | Open **ERP integration**, fill name/endpoint/token + a JSON field-mapping, **Create connection** | Connection appears; token shows as **set** but is never displayed |
| B9 | Click **Sync stock now** on the connection | A `success` row appears in the sync log with the exported record count |
| B10 | Click **Push procurement** | Requested procurement rows move to `sent_to_erp` (visible under Maintenance) |
| B11 | Edit the connection, leave the token blank, Save | Other fields update; the stored token is preserved (still “set”) |
| B12 | Open **Copilot** with the cloud unreachable | “Copilot unavailable offline” banner shows; the rest of the console keeps working (M13 wires the live tool-use loop) |

## Notes

- The kiosk is LAN-only to `mes-edge`; it never calls the cloud. The chat panel
  is scripted/deterministic and makes zero LLM calls (§11). Free-text notes are
  logged locally only.
- Role routing: Operators are kept out of `/supervisor`; console roles
  (Admin/Planner/Supervisor/Quality/Maintenance) land on the console and can open
  the kiosk.
- Scrap and downtime reason chips use a small default set for the demo; a plant
  seeds its own `scrap_reasons` / `downtime_reasons` in master data.
