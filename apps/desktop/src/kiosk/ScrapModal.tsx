// Scrap forces a reason before the count is recorded (§11). Reason codes are
// master data; the operator picks one (or enters a code when the list is empty).

import { useState } from "react";

// A small default set; a plant seeds its own scrap_reasons in master data.
const COMMON_REASONS: { id: string; label: string }[] = [
  { id: "dimensional", label: "Dimensional" },
  { id: "surface", label: "Surface defect" },
  { id: "material", label: "Material flaw" },
  { id: "setup", label: "Setup / first-off" },
];

export function ScrapModal({
  onClose,
  onConfirm,
}: {
  onClose: () => void;
  onConfirm: (reasonId?: string) => void;
}) {
  const [reason, setReason] = useState<string>("");

  return (
    <div className="fixed inset-0 z-10 flex items-center justify-center bg-black/40">
      <div className="w-full max-w-md rounded-2xl bg-white p-6 shadow-xl">
        <h3 className="mb-4 text-lg font-bold text-slate-800">Why is this a scrap?</h3>
        <div className="mb-4 grid grid-cols-2 gap-2">
          {COMMON_REASONS.map((r) => (
            <button
              key={r.id}
              className={`rounded-lg border px-3 py-4 text-sm font-semibold ${
                reason === r.id ? "border-scrap bg-red-50 text-scrap" : "border-slate-300"
              }`}
              onClick={() => setReason(r.id)}
            >
              {r.label}
            </button>
          ))}
        </div>
        <div className="flex justify-end gap-2">
          <button className="rounded-lg px-4 py-2 text-slate-600" onClick={onClose}>
            Cancel
          </button>
          <button
            className="rounded-lg bg-scrap px-4 py-2 font-semibold text-white disabled:opacity-50"
            disabled={!reason}
            onClick={() => onConfirm(reason || undefined)}
          >
            Record scrap
          </button>
        </div>
      </div>
    </div>
  );
}
