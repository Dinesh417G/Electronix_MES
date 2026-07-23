// Supervisor / planner console shell: a sidebar + routed screens (§11). All
// screens work over the LAN against the edge; only the copilot needs the cloud
// and degrades to an "unavailable offline" banner.

import { NavLink, Navigate, Route, Routes, useNavigate } from "react-router-dom";
import { useAuth } from "../auth/AuthContext";
import { Dashboard } from "./Dashboard";
import { ParetoView } from "./Pareto";
import { CmmsView } from "./Cmms";
import { ErpSettings } from "./ErpSettings";
import { RevisionQueue } from "./RevisionQueue";
import { QmsConsole } from "./Qms";
import { CopilotPanel } from "./Copilot";

const NAV = [
  { to: "/supervisor", label: "Live & OEE", end: true },
  { to: "/supervisor/pareto", label: "Downtime Pareto" },
  { to: "/supervisor/qms", label: "Quality (NCRs)" },
  { to: "/supervisor/cmms", label: "Maintenance" },
  { to: "/supervisor/revisions", label: "Program reviews" },
  { to: "/supervisor/erp", label: "ERP integration" },
  { to: "/supervisor/copilot", label: "Copilot" },
];

export function Supervisor() {
  const { session, logout } = useAuth();
  const navigate = useNavigate();

  return (
    <div className="flex h-screen bg-slate-100">
      <aside className="flex w-56 flex-col bg-slate-900 text-slate-200">
        <div className="px-4 py-4 text-lg font-bold text-white">ElectronIx MES</div>
        <nav className="flex-1 space-y-1 px-2">
          {NAV.map((n) => (
            <NavLink
              key={n.to}
              to={n.to}
              end={n.end}
              className={({ isActive }) =>
                `block rounded-lg px-3 py-2 text-sm ${
                  isActive ? "bg-slate-700 font-semibold text-white" : "hover:bg-slate-800"
                }`
              }
            >
              {n.label}
            </NavLink>
          ))}
        </nav>
        <div className="space-y-2 border-t border-slate-800 px-4 py-4 text-sm">
          <div className="text-slate-400">
            {session?.username} · {session?.role}
          </div>
          <button className="underline" onClick={() => navigate("/kiosk")}>
            Open kiosk
          </button>
          <button className="block underline" onClick={logout}>
            Sign out
          </button>
        </div>
      </aside>

      <main className="flex-1 overflow-y-auto p-6">
        <Routes>
          <Route index element={<Dashboard />} />
          <Route path="pareto" element={<ParetoView />} />
          <Route path="qms" element={<QmsConsole />} />
          <Route path="cmms" element={<CmmsView />} />
          <Route path="revisions" element={<RevisionQueue />} />
          <Route path="erp" element={<ErpSettings />} />
          <Route path="copilot" element={<CopilotPanel />} />
          <Route path="*" element={<Navigate to="/supervisor" replace />} />
        </Routes>
      </main>
    </div>
  );
}
