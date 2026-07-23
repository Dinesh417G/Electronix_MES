// The scripted, offline "virtual supervisor" chat panel (§11). It renders
// backend WS events as chat bubbles with tap-to-reply quick actions. It makes
// **no LLM calls** and needs no cloud — everything here is deterministic and
// driven by the local event stream. Free text is logged, never sent to a model.

import { useState } from "react";
import { api } from "../api/client";
import { Badge } from "../components/ui";

export interface Bubble {
  id: string;
  kind: "job_ready" | "job_cleared" | "revision" | "ncr" | "info";
  text: string;
  action?: { label: string; transferId?: string };
}

export function ChatPanel({
  bubbles,
  connected,
}: {
  bubbles: Bubble[];
  connected: boolean;
}) {
  const [draft, setDraft] = useState("");
  const [log, setLog] = useState<string[]>([]);

  return (
    <div className="flex h-full flex-col rounded-xl border border-slate-200 bg-white">
      <div className="flex items-center justify-between border-b border-slate-100 px-4 py-3">
        <h3 className="text-sm font-semibold text-slate-700">Shift assistant</h3>
        <Badge tone={connected ? "green" : "amber"}>{connected ? "live" : "reconnecting"}</Badge>
      </div>

      <div className="flex-1 space-y-2 overflow-y-auto p-4">
        {bubbles.length === 0 && (
          <p className="text-sm text-slate-400">No messages. Prompts appear here as jobs progress.</p>
        )}
        {bubbles.map((b) => (
          <div key={b.id} className="rounded-lg bg-slate-50 p-3">
            <p className="text-sm text-slate-700">{b.text}</p>
            {b.action && (
              <button
                className="mt-2 rounded-md bg-blue-600 px-3 py-1 text-xs font-semibold text-white"
                onClick={() => {
                  if (b.action?.transferId) {
                    void api(`/v1/dnc/transfers/${b.action.transferId}/retry`, { method: "POST" }).catch(
                      () => undefined,
                    );
                  }
                }}
              >
                {b.action.label}
              </button>
            )}
          </div>
        ))}
        {log.map((l, i) => (
          <div key={`log-${i}`} className="ml-8 rounded-lg bg-blue-50 p-2 text-right text-sm text-blue-800">
            {l}
          </div>
        ))}
      </div>

      <div className="border-t border-slate-100 p-2">
        {/* Free text is logged locally only — never sent to any model (§11). */}
        <div className="flex gap-2">
          <input
            className="flex-1 rounded-lg border border-slate-300 px-3 py-2 text-sm"
            placeholder="Note (logged locally)…"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && draft.trim()) {
                setLog((l) => [...l, draft.trim()]);
                setDraft("");
              }
            }}
          />
        </div>
      </div>
    </div>
  );
}
