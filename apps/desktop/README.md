# apps/desktop — ElectronIx MES desktop app (M11)

Tauri 2 + React 18 + TypeScript + Vite + Tailwind + Recharts + TanStack Query
(§5). Two modes in one app (§11):

- **Operator Kiosk** — fast tap-buttons (Good/Scrap/state) + a scripted,
  deterministic, **offline** chat/notification panel. Never LLM-backed. LAN-only
  to `mes-edge`, zero cloud dependency.
- **Supervisor/Planner console** — live tiles + OEE, downtime Pareto, QMS/NCR
  console, CMMS view, program-revision review queue, the no-code ERP settings
  page, and an LLM-backed copilot panel that calls `mes-cloud` `/v1/copilot`
  (online-only, degrades gracefully to an offline banner).

## Layout

```
apps/desktop/
├── index.html                 # Vite entry
├── src/
│   ├── api/                    # typed client, TanStack Query hooks, /ws hook, wire types
│   ├── auth/                   # AuthContext + dual (PIN / password) login
│   ├── components/             # shared UI primitives
│   ├── kiosk/                  # operator fast lane + scripted offline chat panel
│   ├── supervisor/             # console screens (dashboard, pareto, qms, cmms, erp, reviews, copilot)
│   ├── App.tsx                 # role-based routing
│   └── main.tsx                # providers (Query, Router, Auth)
└── src-tauri/                  # Tauri 2 shell (own Cargo workspace; not in the backend dev-gate)
```

## Develop

```bash
cp .env.example .env            # point VITE_MES_EDGE_URL at your edge
npm install
npm run dev                     # web dev server (Vite) on :1420
npm run build                   # tsc --noEmit + vite build  (the CI-independent build check)
npm run tauri dev               # run inside the Tauri desktop shell (needs system webkit + Tauri CLI)
```

The React app is validated with `npm run build`. The Tauri shell (`src-tauri/`)
is its **own** Cargo workspace so it never joins the backend dev-gate; it needs
system webkit libs and is packaged with the Tauri CLI. Generate bundle icons
(`npm run tauri icon <logo.png>`) before `npm run tauri build`.

## Acceptance

See [`TESTPLAN.md`](./TESTPLAN.md) — the manual operator + supervisor scripts
that gate M11 (§13).
