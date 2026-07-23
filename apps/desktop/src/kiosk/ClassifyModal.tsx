// Classify a downtime event with a reason (§8.1/§11). The operator enters the
// downtime event id (from the andon prompt) and picks a reason code; this posts
// to /v1/exec/downtime/:id/classify.

import { useState } from "react";
import { api } from "../api/client";
import { ErrorNote } from "../components/ui";

const REASONS: { id: string; label: string }[] = [
  { id: "tooling", label: "Tooling change" },
  { id: "material", label: "Material wait" },
  { id: "breakdown", label: "Breakdown" },
  { id: "setup", label: "Setup" },
  { id: "quality", label: "Quality check" },
];

export function ClassifyModal({ onClose }: { onClose: () => void }) {
  const [eventId, setEventId] = useState("");
  const [reason, setReason] = useState("");
  const [error, setError] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);

  async function submit() {
    setBusy(true);
    setError(null);
    try {
      await api(`/v1/exec/downtime/${eventId}/classify`, {
        method: "POST",
        body: { reason_id: reason },
      });
      onClose();
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="fixed inset-0 z-10 flex items-center justify-center bg-black/40">
      <div className="w-full max-w-md rounded-2xl bg-white p-6 shadow-xl">
        <h3 className="mb-3 text-lg font-bold text-down">Classify downtime</h3>
        <input
          className="mb-3 w-full rounded-lg border border-slate-300 px-3 py-2 text-sm"
          placeholder="Downtime event id"
          value={eventId}
          onChange={(e) => setEventId(e.target.value)}
        />
        <div className="mb-4 grid grid-cols-2 gap-2">
          {REASONS.map((r) => (
            <button
              key={r.id}
              className={`rounded-lg border px-3 py-3 text-sm font-semibold ${
                reason === r.id ? "border-down bg-orange-50 text-down" : "border-slate-300"
              }`}
              onClick={() => setReason(r.id)}
            >
              {r.label}
            </button>
          ))}
        </div>
        {error != null && (
          <div className="mb-3">
            <ErrorNote error={error} />
          </div>
        )}
        <div className="flex justify-end gap-2">
          <button className="rounded-lg px-4 py-2 text-slate-600" onClick={onClose}>
            Cancel
          </button>
          <button
            className="rounded-lg bg-down px-4 py-2 font-semibold text-white disabled:opacity-50"
            disabled={busy || !eventId || !reason}
            onClick={submit}
          >
            Classify
          </button>
        </div>
      </div>
    </div>
  );
}
