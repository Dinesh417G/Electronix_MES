// CMMS view (§11) — PM-due list, the maintenance-WO board with forward-only
// transitions, spares stock, and the procurement queue.

import {
  useMaintenanceWos,
  useProcurement,
  usePmDue,
  useSpareParts,
  useTransitionMaintenanceWo,
} from "../api/hooks";
import { Badge, Card, Empty, ErrorNote, num, statusTone } from "../components/ui";

const NEXT: Record<string, string | undefined> = {
  requested: "scheduled",
  scheduled: "in_progress",
  in_progress: "completed",
  completed: "verified",
};

export function CmmsView() {
  const due = usePmDue();
  const wos = useMaintenanceWos();
  const spares = useSpareParts();
  const procurement = useProcurement();
  const transition = useTransitionMaintenanceWo();

  return (
    <div className="space-y-6">
      <h2 className="text-xl font-bold text-slate-800">Maintenance</h2>

      <Card title="PM due now">
        {due.error && <ErrorNote error={due.error} />}
        {due.data?.length === 0 ? (
          <Empty>Nothing due.</Empty>
        ) : (
          <ul className="space-y-1 text-sm">
            {due.data?.map((d) => (
              <li key={d.schedule.id} className="flex justify-between">
                <span>
                  {d.schedule.name}{" "}
                  <span className="text-slate-400">({d.schedule.trigger_type})</span>
                </span>
                <span className="text-slate-500">
                  {d.schedule.trigger_type === "usage_hours"
                    ? `${num(d.current_usage_h, 1)}h run`
                    : d.schedule.next_due_at?.slice(0, 10)}
                </span>
              </li>
            ))}
          </ul>
        )}
      </Card>

      <Card title="Maintenance work orders">
        {wos.error && <ErrorNote error={wos.error} />}
        {transition.error && <ErrorNote error={transition.error} />}
        {wos.data?.length === 0 ? (
          <Empty>No maintenance work orders.</Empty>
        ) : (
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-slate-100 text-left text-slate-500">
                <th className="py-2">Type</th>
                <th>Status</th>
                <th>Opened</th>
                <th className="text-right">Advance</th>
              </tr>
            </thead>
            <tbody>
              {wos.data?.map((w) => {
                const next = NEXT[w.status.toLowerCase()];
                return (
                  <tr key={w.id} className="border-b border-slate-50">
                    <td className="py-2">{w.wo_type}</td>
                    <td>
                      <Badge tone={statusTone(w.status)}>{w.status}</Badge>
                    </td>
                    <td className="text-slate-500">{w.opened_at.slice(0, 10)}</td>
                    <td className="text-right">
                      {next ? (
                        <button
                          className="rounded border border-slate-300 px-2 py-1 text-xs hover:bg-slate-50"
                          onClick={() => transition.mutate({ id: w.id, status: next })}
                        >
                          → {next}
                        </button>
                      ) : (
                        <span className="text-xs text-slate-400">done</span>
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </Card>

      <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
        <Card title="Spares stock">
          {spares.error && <ErrorNote error={spares.error} />}
          {spares.data?.length === 0 ? (
            <Empty>No spare parts.</Empty>
          ) : (
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-slate-100 text-left text-slate-500">
                  <th className="py-2">Code</th>
                  <th>Name</th>
                  <th className="text-right">Stock</th>
                  <th className="text-right">Reorder@</th>
                </tr>
              </thead>
              <tbody>
                {spares.data?.map((s) => {
                  const low = parseFloat(String(s.stock)) <= parseFloat(String(s.reorder_point));
                  return (
                    <tr key={s.id} className="border-b border-slate-50">
                      <td className="py-2 font-medium">{s.code}</td>
                      <td>{s.name}</td>
                      <td className={`text-right ${low ? "font-bold text-red-600" : ""}`}>{num(s.stock)}</td>
                      <td className="text-right text-slate-500">{num(s.reorder_point)}</td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          )}
        </Card>

        <Card title="Procurement requests">
          {procurement.error && <ErrorNote error={procurement.error} />}
          {procurement.data?.length === 0 ? (
            <Empty>No procurement requests.</Empty>
          ) : (
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-slate-100 text-left text-slate-500">
                  <th className="py-2">Qty</th>
                  <th>Reason</th>
                  <th>Status</th>
                  <th>ERP ref</th>
                </tr>
              </thead>
              <tbody>
                {procurement.data?.map((p) => (
                  <tr key={p.id} className="border-b border-slate-50">
                    <td className="py-2">{num(p.qty_requested)}</td>
                    <td className="text-slate-500">{p.reason}</td>
                    <td>
                      <Badge tone={statusTone(p.status)}>{p.status}</Badge>
                    </td>
                    <td className="text-slate-500">{p.erp_reference ?? "—"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </Card>
      </div>
    </div>
  );
}
