// ERP integration settings (§11) — the no-code page. Paste an endpoint, token,
// and JSON field-mapping; run a sync-now export; read the sync log. The token is
// write-only (the server never returns it), so the field is always blank on edit
// and left untouched when submitted empty.

import { useEffect, useState } from "react";
import { useErpConnections, useErpSyncLog, useErpExport, useSaveErpConnection } from "../api/hooks";
import type { ErpConnection } from "../api/types";
import { Badge, Card, Empty, ErrorNote, statusTone } from "../components/ui";

const BLANK = {
  name: "",
  endpoint_url: "",
  auth_token: "",
  direction: "both",
  field_mapping: `{
  "fields": {
    "code": "sku",
    "stock": "on_hand"
  }
}`,
};

export function ErpSettings() {
  const conns = useErpConnections();
  const log = useErpSyncLog();
  const save = useSaveErpConnection();
  const runExport = useErpExport();

  const [editId, setEditId] = useState<string | undefined>(undefined);
  const [form, setForm] = useState(BLANK);
  const [mappingError, setMappingError] = useState<string | null>(null);

  useEffect(() => {
    if (!editId) return;
    const c = conns.data?.find((x) => x.id === editId);
    if (c) {
      setForm({
        name: c.name,
        endpoint_url: c.endpoint_url,
        auth_token: "",
        direction: c.direction,
        field_mapping: JSON.stringify(c.field_mapping ?? {}, null, 2),
      });
    }
  }, [editId, conns.data]);

  function submit() {
    let mapping: unknown;
    try {
      mapping = JSON.parse(form.field_mapping);
      setMappingError(null);
    } catch (e) {
      setMappingError(`Invalid JSON: ${(e as Error).message}`);
      return;
    }
    const body: Record<string, unknown> = {
      name: form.name,
      endpoint_url: form.endpoint_url,
      direction: form.direction,
      field_mapping: mapping,
    };
    if (form.auth_token) body.auth_token = form.auth_token;
    save.mutate(
      { id: editId, body },
      {
        onSuccess: () => {
          setEditId(undefined);
          setForm(BLANK);
        },
      },
    );
  }

  return (
    <div className="space-y-6">
      <h2 className="text-xl font-bold text-slate-800">ERP integration</h2>

      <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
        <Card title={editId ? "Edit connection" : "New connection"}>
          <div className="space-y-3 text-sm">
            <Field label="Name">
              <input
                className="input"
                value={form.name}
                onChange={(e) => setForm({ ...form, name: e.target.value })}
              />
            </Field>
            <Field label="Endpoint URL">
              <input
                className="input"
                value={form.endpoint_url}
                onChange={(e) => setForm({ ...form, endpoint_url: e.target.value })}
              />
            </Field>
            <Field label={`Auth token ${editId ? "(blank = keep existing)" : ""}`}>
              <input
                className="input"
                type="password"
                value={form.auth_token}
                onChange={(e) => setForm({ ...form, auth_token: e.target.value })}
              />
            </Field>
            <Field label="Direction">
              <select
                className="input"
                value={form.direction}
                onChange={(e) => setForm({ ...form, direction: e.target.value })}
              >
                <option value="both">both</option>
                <option value="import">import</option>
                <option value="export">export</option>
              </select>
            </Field>
            <Field label="Field mapping (JSON)">
              <textarea
                className="input font-mono text-xs"
                rows={8}
                value={form.field_mapping}
                onChange={(e) => setForm({ ...form, field_mapping: e.target.value })}
              />
            </Field>
            {mappingError && <ErrorNote error={mappingError} />}
            {save.error != null && <ErrorNote error={save.error} />}
            <div className="flex gap-2">
              <button
                className="rounded-lg bg-blue-600 px-4 py-2 font-semibold text-white disabled:opacity-50"
                onClick={submit}
                disabled={save.isPending || !form.name || !form.endpoint_url}
              >
                {editId ? "Save changes" : "Create connection"}
              </button>
              {editId && (
                <button
                  className="rounded-lg px-4 py-2 text-slate-600"
                  onClick={() => {
                    setEditId(undefined);
                    setForm(BLANK);
                  }}
                >
                  Cancel
                </button>
              )}
            </div>
          </div>
        </Card>

        <Card title="Connections">
          {conns.error && <ErrorNote error={conns.error} />}
          {conns.data?.length === 0 ? (
            <Empty>No connections yet.</Empty>
          ) : (
            <ul className="space-y-2 text-sm">
              {conns.data?.map((c: ErpConnection) => (
                <li key={c.id} className="rounded-lg border border-slate-200 p-3">
                  <div className="flex items-center justify-between">
                    <div className="font-semibold text-slate-700">{c.name}</div>
                    <Badge tone={c.enabled ? "green" : "slate"}>{c.direction}</Badge>
                  </div>
                  <div className="truncate text-xs text-slate-400">{c.endpoint_url}</div>
                  <div className="mt-1 text-xs text-slate-400">
                    token: {c.has_token ? "set" : "none"}
                  </div>
                  <div className="mt-2 flex gap-2">
                    <button
                      className="rounded border border-slate-300 px-2 py-1 text-xs"
                      onClick={() => setEditId(c.id)}
                    >
                      Edit
                    </button>
                    <button
                      className="rounded border border-slate-300 px-2 py-1 text-xs"
                      onClick={() => runExport.mutate({ connection_id: c.id, entity: "stock_level" })}
                    >
                      Sync stock now
                    </button>
                    <button
                      className="rounded border border-slate-300 px-2 py-1 text-xs"
                      onClick={() =>
                        runExport.mutate({ connection_id: c.id, entity: "procurement_request" })
                      }
                    >
                      Push procurement
                    </button>
                  </div>
                </li>
              ))}
            </ul>
          )}
          {runExport.error != null && (
            <div className="mt-2">
              <ErrorNote error={runExport.error} />
            </div>
          )}
        </Card>
      </div>

      <Card title="Sync log">
        {log.data?.length === 0 ? (
          <Empty>No syncs yet.</Empty>
        ) : (
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-slate-100 text-left text-slate-500">
                <th className="py-2">When</th>
                <th>Direction</th>
                <th>Entity</th>
                <th className="text-right">Records</th>
                <th>Status</th>
              </tr>
            </thead>
            <tbody>
              {log.data?.map((e) => (
                <tr key={e.id} className="border-b border-slate-50">
                  <td className="py-2 text-slate-500">{e.ts.slice(0, 19).replace("T", " ")}</td>
                  <td>{e.direction}</td>
                  <td>{e.entity}</td>
                  <td className="text-right">{e.record_count}</td>
                  <td>
                    <Badge tone={statusTone(e.status)}>{e.status}</Badge>
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

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="block">
      <span className="mb-1 block text-xs font-medium text-slate-500">{label}</span>
      {children}
    </label>
  );
}
