# apps/desktop — reserved (built at M11)

Tauri 2 + React 18 + TypeScript + Vite + Tailwind + Recharts + TanStack Query
(§5). Two modes in one app (§11):

- **Operator Kiosk** — fast tap-buttons (Good/Scrap/state) + a scripted,
  deterministic, **offline** chat/notification panel. Never LLM-backed. LAN-only
  to `mes-edge`, zero cloud dependency.
- **Supervisor/Planner console** — dashboards, scheduling board, CMMS view, ERP
  settings page, program-revision review queue, and an LLM-backed copilot panel
  that calls `mes-cloud` `/v1/copilot` (online-only, degrades gracefully).

Directory intentionally empty until M11 — do not scaffold Tauri here before then.
