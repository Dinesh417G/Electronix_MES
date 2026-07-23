// Downtime Pareto (§5/§11) — the classic ranked bar of loss by reason, over a
// selectable work center and window.

import { useState } from "react";
import { Bar, BarChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { usePareto, useWorkCenters } from "../api/hooks";
import { Card, Empty, ErrorNote } from "../components/ui";

export function ParetoView() {
  const workCenters = useWorkCenters();
  const [wcId, setWcId] = useState<string>("");
  const now = new Date();
  const start = new Date(now.getTime() - 7 * 24 * 3600 * 1000).toISOString();
  const end = now.toISOString();
  const pareto = usePareto(wcId || undefined, start, end);

  const data =
    pareto.data?.map((b) => ({
      reason: b.reason_label,
      minutes: Math.round(b.total_seconds / 60),
      events: b.event_count,
    })) ?? [];

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h2 className="text-xl font-bold text-slate-800">Downtime Pareto (7 days)</h2>
        <select
          className="rounded-md border border-slate-300 px-2 py-1 text-sm"
          value={wcId}
          onChange={(e) => setWcId(e.target.value)}
        >
          <option value="">All work centers</option>
          {workCenters.data?.map((wc) => (
            <option key={wc.id} value={wc.id}>
              {wc.code}
            </option>
          ))}
        </select>
      </div>

      <Card>
        {pareto.error ? (
          <ErrorNote error={pareto.error} />
        ) : data.length === 0 ? (
          <Empty>No downtime in this window.</Empty>
        ) : (
          <div className="h-80">
            <ResponsiveContainer width="100%" height="100%">
              <BarChart data={data} margin={{ top: 8, right: 8, bottom: 40, left: 8 }}>
                <CartesianGrid strokeDasharray="3 3" stroke="#e2e8f0" />
                <XAxis dataKey="reason" angle={-30} textAnchor="end" interval={0} tick={{ fontSize: 11 }} />
                <YAxis tick={{ fontSize: 12 }} label={{ value: "minutes", angle: -90, position: "insideLeft" }} />
                <Tooltip />
                <Bar dataKey="minutes" fill="#ea580c" radius={[4, 4, 0, 0]} />
              </BarChart>
            </ResponsiveContainer>
          </div>
        )}
      </Card>
    </div>
  );
}
