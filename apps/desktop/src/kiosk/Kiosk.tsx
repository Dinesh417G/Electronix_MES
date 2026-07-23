// Operator kiosk — the fast lane (§11). Giant Good/Scrap buttons that stay
// instant, a Classify-Downtime path, and the DNC job-ready banner, alongside the
// scripted offline chat panel. LAN-only to the edge; zero cloud dependency.

import { useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useAuth } from "../auth/AuthContext";
import {
  useCompleteOperation,
  useRecordCount,
  useStartOperation,
  useWorkCenters,
  useWorkOrder,
  useWorkOrders,
} from "../api/hooks";
import { useWebSocket } from "../api/useWebSocket";
import type { WoOperation, WsEvent } from "../api/types";
import { Badge, ErrorNote, statusTone } from "../components/ui";
import { ChatPanel, type Bubble } from "./ChatPanel";
import { ScrapModal } from "./ScrapModal";
import { ClassifyModal } from "./ClassifyModal";

export function Kiosk() {
  const { session, logout } = useAuth();
  const navigate = useNavigate();
  const workCenters = useWorkCenters();
  const orders = useWorkOrders();

  const [wcId, setWcId] = useState<string>("");
  const [orderId, setOrderId] = useState<string>("");
  const [bubbles, setBubbles] = useState<Bubble[]>([]);
  const [banner, setBanner] = useState<{ text: string; transferId: string } | null>(null);

  const push = (b: Bubble) => setBubbles((prev) => [...prev.slice(-30), b]);

  const { connected } = useWebSocket((e: WsEvent) => {
    switch (e.event) {
      case "dnc_transfer_scheduled": {
        setBanner({
          text: `Job ready — fetch program ${e.program_identifier}`,
          transferId: e.transfer_id,
        });
        push({
          id: e.transfer_id,
          kind: "job_ready",
          text: `Program ${e.program_identifier} is staged. Tap to fetch it to the machine.`,
          action: { label: "Fetch program", transferId: e.transfer_id },
        });
        break;
      }
      case "dnc_transfer_completed":
        setBanner((b) => (b && b.transferId === e.transfer_id ? null : b));
        push({ id: `c-${e.transfer_id}`, kind: "job_cleared", text: "Program transferred to the machine. ✓" });
        break;
      case "program_revision_drafted":
        push({
          id: e.revision_id,
          kind: "revision",
          text: "You edited a program — saved as a draft for supervisor review.",
        });
        break;
      case "ncr_raised":
        push({ id: e.ncr_id, kind: "ncr", text: `Quality hold raised: ${e.ncr_no}.` });
        break;
      default:
        break;
    }
  });

  return (
    <div className="flex h-screen flex-col bg-slate-100">
      <header className="flex items-center justify-between bg-slate-800 px-4 py-3 text-white">
        <div className="flex items-center gap-3">
          <span className="text-lg font-bold">Kiosk</span>
          <select
            className="rounded-md bg-slate-700 px-2 py-1 text-sm"
            value={wcId}
            onChange={(e) => {
              setWcId(e.target.value);
              setOrderId("");
            }}
          >
            <option value="">Select work center…</option>
            {workCenters.data?.map((wc) => (
              <option key={wc.id} value={wc.id}>
                {wc.code} — {wc.name}
              </option>
            ))}
          </select>
        </div>
        <div className="flex items-center gap-3 text-sm">
          <span>{session?.username}</span>
          <button className="underline" onClick={() => navigate("/supervisor")}>
            Console
          </button>
          <button className="underline" onClick={logout}>
            Sign out
          </button>
        </div>
      </header>

      {banner && (
        <div className="flex items-center justify-between bg-blue-600 px-4 py-3 text-white">
          <span className="font-semibold">{banner.text}</span>
        </div>
      )}

      <div className="grid flex-1 grid-cols-3 gap-4 overflow-hidden p-4">
        <div className="col-span-2 overflow-y-auto">
          {orders.error && <ErrorNote error={orders.error} />}
          {!wcId ? (
            <p className="text-slate-500">Pick a work center to see its active work.</p>
          ) : (
            <OrderPicker
              wcId={wcId}
              orderId={orderId}
              setOrderId={setOrderId}
              orders={orders.data ?? []}
            />
          )}
        </div>
        <div className="overflow-hidden">
          <ChatPanel bubbles={bubbles} connected={connected} />
        </div>
      </div>
    </div>
  );
}

function OrderPicker({
  wcId,
  orderId,
  setOrderId,
  orders,
}: {
  wcId: string;
  orderId: string;
  setOrderId: (id: string) => void;
  orders: { id: string; wo_number: string; status: string }[];
}) {
  const active = orders.filter((o) => ["released", "in_progress"].includes(o.status.toLowerCase()));
  return (
    <div className="space-y-4">
      <div className="flex flex-wrap gap-2">
        {active.length === 0 && <p className="text-slate-500">No released orders.</p>}
        {active.map((o) => (
          <button
            key={o.id}
            className={`rounded-lg border px-4 py-2 text-sm font-semibold ${
              orderId === o.id ? "border-blue-600 bg-blue-50 text-blue-700" : "border-slate-300 bg-white"
            }`}
            onClick={() => setOrderId(o.id)}
          >
            {o.wo_number}
            <span className="ml-2">
              <Badge tone={statusTone(o.status)}>{o.status}</Badge>
            </span>
          </button>
        ))}
      </div>
      {orderId && <ActiveOrderCard orderId={orderId} wcId={wcId} />}
    </div>
  );
}

function ActiveOrderCard({ orderId, wcId }: { orderId: string; wcId: string }) {
  const detail = useWorkOrder(orderId);
  const op = useMemo(
    () => detail.data?.operations.find((o) => o.work_center_id === wcId) ?? detail.data?.operations[0],
    [detail.data, wcId],
  );

  if (detail.isLoading) return <p className="text-slate-500">Loading…</p>;
  if (detail.error) return <ErrorNote error={detail.error} />;
  if (!detail.data || !op) return <p className="text-slate-500">No operation for this work center.</p>;

  return <OperationCard op={op} woNumber={detail.data.wo_number} />;
}

function OperationCard({ op, woNumber }: { op: WoOperation; woNumber: string }) {
  const count = useRecordCount(op.id);
  const start = useStartOperation(op.id);
  const complete = useCompleteOperation(op.id);
  const [scrapOpen, setScrapOpen] = useState(false);
  const [classifyOpen, setClassifyOpen] = useState(false);

  const running = op.status.toLowerCase() === "in_progress";

  return (
    <div className="rounded-xl border border-slate-200 bg-white p-5 shadow-sm">
      <div className="mb-4 flex items-center justify-between">
        <div>
          <div className="text-lg font-bold text-slate-800">
            {woNumber} · Op {op.op_no}
          </div>
          <Badge tone={statusTone(op.status)}>{op.status}</Badge>
        </div>
        <div className="text-right text-sm text-slate-600">
          <div>
            Good <span className="text-2xl font-bold text-good">{op.qty_good}</span>
          </div>
          <div>
            Scrap <span className="text-2xl font-bold text-scrap">{op.qty_scrap}</span>
          </div>
        </div>
      </div>

      {!running ? (
        <button
          className="w-full rounded-xl bg-blue-600 py-6 text-2xl font-bold text-white"
          onClick={() => start.mutate()}
        >
          Start operation
        </button>
      ) : (
        <div className="grid grid-cols-2 gap-3">
          <button
            className="rounded-xl bg-good py-10 text-3xl font-extrabold text-white active:brightness-95"
            onClick={() => count.mutate({ good: 1, scrap: 0 })}
          >
            GOOD +1
          </button>
          <button
            className="rounded-xl bg-scrap py-10 text-3xl font-extrabold text-white active:brightness-95"
            onClick={() => setScrapOpen(true)}
          >
            SCRAP
          </button>
        </div>
      )}

      <div className="mt-3 grid grid-cols-2 gap-3">
        <button
          className="rounded-lg border border-down py-3 font-semibold text-down"
          onClick={() => setClassifyOpen(true)}
        >
          Classify downtime
        </button>
        <button
          className="rounded-lg border border-slate-300 py-3 font-semibold text-slate-700"
          onClick={() => complete.mutate()}
          disabled={!running}
        >
          Complete operation
        </button>
      </div>

      {(count.error || start.error || complete.error) && (
        <div className="mt-3">
          <ErrorNote error={count.error || start.error || complete.error} />
        </div>
      )}

      {scrapOpen && (
        <ScrapModal
          onClose={() => setScrapOpen(false)}
          onConfirm={(reasonId) => {
            count.mutate({ good: 0, scrap: 1, scrap_reason_id: reasonId });
            setScrapOpen(false);
          }}
        />
      )}
      {classifyOpen && <ClassifyModal onClose={() => setClassifyOpen(false)} />}
    </div>
  );
}
