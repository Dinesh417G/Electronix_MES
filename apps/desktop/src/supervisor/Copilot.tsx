// Supervisor copilot panel (§11) — LLM-backed, calls the *cloud* `/v1/copilot`
// (M13). It requires connectivity and degrades gracefully: when the cloud is
// unreachable it shows an "unavailable offline" banner while the rest of the
// console keeps working over the LAN. The full tool-use loop lands in M13; this
// panel is the client seam wired against the endpoint.

import { useState } from "react";
import { api, cloudUrl } from "../api/client";
import { Card } from "../components/ui";

interface Msg {
  role: "user" | "assistant";
  content: string;
}

export function CopilotPanel() {
  const [messages, setMessages] = useState<Msg[]>([]);
  const [draft, setDraft] = useState("");
  const [offline, setOffline] = useState(false);
  const [busy, setBusy] = useState(false);

  async function send() {
    const text = draft.trim();
    if (!text) return;
    setDraft("");
    setMessages((m) => [...m, { role: "user", content: text }]);
    setBusy(true);
    try {
      const resp = await api<{ reply: string }>("/v1/copilot", {
        method: "POST",
        base: cloudUrl,
        body: { message: text },
      });
      setMessages((m) => [...m, { role: "assistant", content: resp.reply }]);
      setOffline(false);
    } catch {
      // Cloud unreachable (or M13 not yet deployed) — degrade gracefully.
      setOffline(true);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="space-y-4">
      <h2 className="text-xl font-bold text-slate-800">Copilot</h2>

      {offline && (
        <div className="rounded-lg border border-amber-200 bg-amber-50 px-4 py-3 text-sm text-amber-800">
          Copilot is unavailable offline. The rest of the console keeps working over the LAN.
        </div>
      )}

      <Card>
        <div className="mb-3 min-h-[200px] space-y-2">
          {messages.length === 0 && (
            <p className="text-sm text-slate-400">
              Ask things like “why is OEE down on WC-1 today?” — answered server-side with tool
              calls against your plant data (cloud required).
            </p>
          )}
          {messages.map((m, i) => (
            <div
              key={i}
              className={`rounded-lg p-3 text-sm ${
                m.role === "user" ? "ml-8 bg-blue-50 text-blue-900" : "mr-8 bg-slate-50 text-slate-700"
              }`}
            >
              {m.content}
            </div>
          ))}
        </div>
        <div className="flex gap-2">
          <input
            className="input"
            placeholder="Ask the copilot…"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && send()}
          />
          <button
            className="rounded-lg bg-blue-600 px-4 py-2 font-semibold text-white disabled:opacity-50"
            onClick={send}
            disabled={busy}
          >
            Send
          </button>
        </div>
      </Card>
    </div>
  );
}
