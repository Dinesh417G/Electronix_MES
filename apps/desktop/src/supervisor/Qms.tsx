// QMS console (§11) — the NCR queue with disposition actions. Dispositioning is
// quality-gated on the server; the UI simply surfaces the actions and lets the
// server enforce the role.

import { useNcrs, useDispositionNcr } from "../api/hooks";
import { Badge, Card, Empty, ErrorNote, statusTone } from "../components/ui";

const DISPOSITIONS = ["rework", "use_as_is", "scrap", "return"];

export function QmsConsole() {
  const ncrs = useNcrs();
  const disposition = useDispositionNcr();

  return (
    <div className="space-y-6">
      <h2 className="text-xl font-bold text-slate-800">Non-conformances</h2>
      {ncrs.error && <ErrorNote error={ncrs.error} />}
      {disposition.error && <ErrorNote error={disposition.error} />}

      <Card>
        {ncrs.data?.length === 0 ? (
          <Empty>No NCRs. Failing inspections raise them automatically.</Empty>
        ) : (
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-slate-100 text-left text-slate-500">
                <th className="py-2">NCR</th>
                <th>Status</th>
                <th>Disposition</th>
                <th>Reason</th>
                <th className="text-right">Actions</th>
              </tr>
            </thead>
            <tbody>
              {ncrs.data?.map((n) => (
                <tr key={n.id} className="border-b border-slate-50">
                  <td className="py-2 font-medium">{n.ncr_no}</td>
                  <td>
                    <Badge tone={statusTone(n.status)}>{n.status}</Badge>
                  </td>
                  <td>{n.disposition ?? "—"}</td>
                  <td className="text-slate-500">{n.reason ?? "—"}</td>
                  <td className="text-right">
                    {n.status === "open" ? (
                      <div className="inline-flex gap-1">
                        {DISPOSITIONS.map((d) => (
                          <button
                            key={d}
                            className="rounded border border-slate-300 px-2 py-1 text-xs hover:bg-slate-50"
                            onClick={() => disposition.mutate({ id: n.id, disposition: d })}
                          >
                            {d}
                          </button>
                        ))}
                      </div>
                    ) : (
                      <span className="text-xs text-slate-400">closed for action</span>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Card>
    </div>
  );
}
