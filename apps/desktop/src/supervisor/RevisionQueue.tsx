// Program-revision review queue (§8.4/§11). Operator-edited programs arrive as
// Draft revisions and are **never** auto-promoted — a supervisor promotes or
// rejects them here.

import { useRevisions, usePromoteRevision, useRejectRevision } from "../api/hooks";
import { Badge, Card, Empty, ErrorNote, statusTone } from "../components/ui";

export function RevisionQueue() {
  const revisions = useRevisions();
  const promote = usePromoteRevision();
  const reject = useRejectRevision();

  const drafts = revisions.data?.filter((r) => r.status.toLowerCase() === "draft") ?? [];

  return (
    <div className="space-y-6">
      <h2 className="text-xl font-bold text-slate-800">Program reviews</h2>
      {revisions.error && <ErrorNote error={revisions.error} />}
      {(promote.error || reject.error) && <ErrorNote error={promote.error || reject.error} />}

      <Card title={`Draft revisions (${drafts.length})`}>
        {drafts.length === 0 ? (
          <Empty>No drafts awaiting review.</Empty>
        ) : (
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-slate-100 text-left text-slate-500">
                <th className="py-2">Program</th>
                <th>Rev</th>
                <th>Submitted</th>
                <th className="text-right">Decision</th>
              </tr>
            </thead>
            <tbody>
              {drafts.map((r) => (
                <tr key={r.id} className="border-b border-slate-50">
                  <td className="py-2 font-mono text-xs">{r.program_id}</td>
                  <td>#{r.revision_no}</td>
                  <td className="text-slate-500">{r.submitted_at?.slice(0, 16).replace("T", " ")}</td>
                  <td className="text-right">
                    <div className="inline-flex gap-1">
                      <button
                        className="rounded bg-good px-3 py-1 text-xs font-semibold text-white"
                        onClick={() => promote.mutate(r.id)}
                      >
                        Promote
                      </button>
                      <button
                        className="rounded border border-slate-300 px-3 py-1 text-xs"
                        onClick={() => reject.mutate(r.id)}
                      >
                        Reject
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Card>

      <Card title="All revisions">
        {revisions.data?.length === 0 ? (
          <Empty>No revisions.</Empty>
        ) : (
          <ul className="space-y-1 text-sm">
            {revisions.data?.map((r) => (
              <li key={r.id} className="flex items-center justify-between">
                <span className="font-mono text-xs">
                  {r.program_id} #{r.revision_no}
                </span>
                <Badge tone={statusTone(r.status)}>{r.status}</Badge>
              </li>
            ))}
          </ul>
        )}
      </Card>
    </div>
  );
}
