// TanStack Query hooks over the edge API. Keeping them in one place means the
// screens stay declarative and cache invalidation is consistent.

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "./client";
import type {
  DncTransfer,
  ErpConnection,
  ErpSyncLogEntry,
  MaintenanceWo,
  Ncr,
  OeeResult,
  ParetoBar,
  Part,
  PmDue,
  ProcurementRequest,
  ProgramRevision,
  SparePart,
  WorkCenter,
  WorkOrder,
  WorkOrderDetail,
} from "./types";

const q = <T>(key: unknown[], path: string, enabled = true) =>
  useQuery<T>({ queryKey: key, queryFn: () => api<T>(path), enabled });

// ---- Master data ---------------------------------------------------------

export const useWorkCenters = () => q<WorkCenter[]>(["work-centers"], "/v1/master/work-centers");
export const useParts = () => q<Part[]>(["parts"], "/v1/master/parts");

// ---- Orders + execution --------------------------------------------------

export const useWorkOrders = () => q<WorkOrder[]>(["orders"], "/v1/orders");
export const useWorkOrder = (id?: string) =>
  q<WorkOrderDetail>(["orders", id], `/v1/orders/${id}`, !!id);

export function useRecordCount(opId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: { good: number; scrap: number; scrap_reason_id?: string }) =>
      api(`/v1/exec/operations/${opId}/count`, { method: "POST", body }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["orders"] }),
  });
}

export function useStartOperation(opId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => api(`/v1/exec/operations/${opId}/start`, { method: "POST" }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["orders"] }),
  });
}

export function useCompleteOperation(opId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => api(`/v1/exec/operations/${opId}/complete`, { method: "POST" }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["orders"] }),
  });
}

export function useClassifyDowntime(downtimeId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: { reason_id: string }) =>
      api(`/v1/exec/downtime/${downtimeId}/classify`, { method: "POST", body }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["orders"] }),
  });
}

// ---- Analytics -----------------------------------------------------------

export const useOee = (workCenterId?: string, start?: string, end?: string) =>
  useQuery<OeeResult>({
    queryKey: ["oee", workCenterId, start, end],
    queryFn: () =>
      api<OeeResult>(
        `/v1/analytics/oee?work_center_id=${workCenterId}&start=${start}&end=${end}`,
      ),
    enabled: !!workCenterId && !!start && !!end,
  });

export const usePareto = (workCenterId?: string, start?: string, end?: string) =>
  useQuery<ParetoBar[]>({
    queryKey: ["pareto", workCenterId, start, end],
    queryFn: () => {
      const params = new URLSearchParams();
      if (workCenterId) params.set("work_center_id", workCenterId);
      if (start) params.set("start", start);
      if (end) params.set("end", end);
      return api<ParetoBar[]>(`/v1/analytics/downtime/pareto?${params.toString()}`);
    },
  });

// ---- DNC -----------------------------------------------------------------

export const useTransfers = () => q<DncTransfer[]>(["dnc-transfers"], "/v1/dnc/transfers");
export const useRevisions = () => q<ProgramRevision[]>(["dnc-revisions"], "/v1/dnc/revisions");

export function usePromoteRevision() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => api(`/v1/dnc/revisions/${id}/promote`, { method: "POST" }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["dnc-revisions"] }),
  });
}

export function useRejectRevision() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => api(`/v1/dnc/revisions/${id}/reject`, { method: "POST" }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["dnc-revisions"] }),
  });
}

// ---- QMS -----------------------------------------------------------------

export const useNcrs = () => q<Ncr[]>(["ncrs"], "/v1/qms/ncrs");

export function useDispositionNcr() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (args: { id: string; disposition: string; reason?: string }) =>
      api(`/v1/qms/ncrs/${args.id}/disposition`, {
        method: "POST",
        body: { disposition: args.disposition, reason: args.reason },
      }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["ncrs"] }),
  });
}

// ---- CMMS ----------------------------------------------------------------

export const usePmDue = () => q<PmDue[]>(["pm-due"], "/v1/cmms/pm-schedules/due");
export const useMaintenanceWos = () => q<MaintenanceWo[]>(["mwos"], "/v1/cmms/work-orders");
export const useSpareParts = () => q<SparePart[]>(["spares"], "/v1/cmms/spares");
export const useProcurement = () =>
  q<ProcurementRequest[]>(["procurement"], "/v1/cmms/procurement");

export function useTransitionMaintenanceWo() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (args: { id: string; status: string }) =>
      api(`/v1/cmms/work-orders/${args.id}/transition`, {
        method: "POST",
        body: { status: args.status },
      }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["mwos"] }),
  });
}

// ---- ERP -----------------------------------------------------------------

export const useErpConnections = () => q<ErpConnection[]>(["erp-conns"], "/v1/erp/connections");
export const useErpSyncLog = () => q<ErpSyncLogEntry[]>(["erp-log"], "/v1/erp/sync-log");

export function useSaveErpConnection() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (args: { id?: string; body: Record<string, unknown> }) =>
      api(args.id ? `/v1/erp/connections/${args.id}` : "/v1/erp/connections", {
        method: args.id ? "PUT" : "POST",
        body: args.body,
      }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["erp-conns"] });
    },
  });
}

export function useErpExport() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (args: { connection_id: string; entity: string }) =>
      api("/v1/erp/export", { method: "POST", body: args }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["erp-log"] });
      qc.invalidateQueries({ queryKey: ["procurement"] });
    },
  });
}
