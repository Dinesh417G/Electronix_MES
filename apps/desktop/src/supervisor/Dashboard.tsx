// Live plant tiles + shift OEE (§11). Work-center tiles subscribe to the live
// `/ws` OEE snapshots so the board updates without polling; a per-work-center
// OEE breakdown chart renders the A/P/Q components.

import { useMemo, useState } from "react";
import { Bar, BarChart, CartesianGrid, Cell, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { useOee, useWorkCenters } from "../api/hooks";
import { useWebSocket } from "../api/useWebSocket";
import type { WsEvent } from "../api/types";
import { Card, Empty, pct } from "../components/ui";

interface Snap {
  availability: number;
  performance: number;
  quality: number;
  oee: number;
}

export function Dashboard() {
  const workCenters = useWorkCenters();
  const [live, setLive] = useState<Record<string, Snap>>({});
  const [selected, setSelected] = useState<string>("");

  const { connected } = useWebSocket((e: WsEvent) => {
    if (e.event === "oee_snapshot") {
      setLive((prev) => ({
        ...prev,
        [e.work_center_id]: {
          availability: e.availability,
          performance: e.performance,
          quality: e.quality,
          oee: e.oee,
        },
      }));
    }
  });

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h2 className="text-xl font-bold text-slate-800">Live plant</h2>
        <span className={`text-sm ${connected ? "text-green-600" : "text-amber-600"}`}>
          {connected ? "● live" : "○ reconnecting"}
        </span>
      </div>

      <div className="grid grid-cols-2 gap-4 md:grid-cols-3 lg:grid-cols-4">
        {workCenters.data?.map((wc) => {
          const snap = live[wc.id];
          return (
            <button
              key={wc.id}
              onClick={() => setSelected(wc.id)}
              className={`rounded-xl border p-4 text-left shadow-sm ${
                selected === wc.id ? "border-blue-500 bg-blue-50" : "border-slate-200 bg-white"
              }`}
            >
              <div className="text-sm font-semibold text-slate-700">{wc.code}</div>
              <div className="truncate text-xs text-slate-400">{wc.name}</div>
              <div className="mt-2 text-3xl font-bold text-slate-800">
                {snap ? pct(snap.oee) : "—"}
              </div>
              <div className="text-xs text-slate-400">live OEE</div>
            </button>
          );
        })}
        {workCenters.data?.length === 0 && <Empty>No work centers configured.</Empty>}
      </div>

      {selected && <OeeBreakdown workCenterId={selected} live={live[selected]} />}
    </div>
  );
}

function OeeBreakdown({ workCenterId, live }: { workCenterId: string; live?: Snap }) {
  // Fall back to a queried shift OEE when no live snapshot has arrived yet.
  const now = new Date();
  const start = new Date(now.getTime() - 8 * 3600 * 1000).toISOString();
  const end = now.toISOString();
  const queried = useOee(workCenterId, start, end);

  const snap = live ?? queried.data;
  const data = useMemo(
    () =>
      snap
        ? [
            { name: "Availability", value: snap.availability },
            { name: "Performance", value: snap.performance },
            { name: "Quality", value: snap.quality },
            { name: "OEE", value: snap.oee },
          ]
        : [],
    [snap],
  );

  const colors = ["#3b82f6", "#8b5cf6", "#14b8a6", "#0f172a"];

  return (
    <Card title="OEE breakdown (last 8h / live)">
      {data.length === 0 ? (
        <Empty>No OEE data yet for this work center.</Empty>
      ) : (
        <div className="h-64">
          <ResponsiveContainer width="100%" height="100%">
            <BarChart data={data} margin={{ top: 8, right: 8, bottom: 8, left: 8 }}>
              <CartesianGrid strokeDasharray="3 3" stroke="#e2e8f0" />
              <XAxis dataKey="name" tick={{ fontSize: 12 }} />
              <YAxis domain={[0, 1]} tickFormatter={(v) => `${Math.round(v * 100)}%`} tick={{ fontSize: 12 }} />
              <Tooltip formatter={(v: number) => pct(v)} />
              <Bar dataKey="value" radius={[4, 4, 0, 0]}>
                {data.map((_, i) => (
                  <Cell key={i} fill={colors[i]} />
                ))}
              </Bar>
            </BarChart>
          </ResponsiveContainer>
        </div>
      )}
    </Card>
  );
}
