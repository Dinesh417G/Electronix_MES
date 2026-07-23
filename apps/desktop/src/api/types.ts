// TypeScript mirrors of the `mes-client` wire DTOs (§1 — one contract, both
// ends). Only the fields the desktop app reads are modelled; Decimal/NUMERIC
// values arrive as strings or numbers, so money/qty fields are typed `Numeric`.

export type Numeric = string | number;

export interface LoginResponse {
  token: string;
  user_id: string;
  username: string;
  role_code: string;
  expires_at: number;
}

export interface WorkCenter {
  id: string;
  area_id: string;
  code: string;
  name: string;
  external_ref?: string | null;
}

export interface Part {
  id: string;
  code: string;
  name: string;
  uom: string;
}

export interface WorkOrder {
  id: string;
  wo_number: string;
  part_id: string;
  routing_id?: string | null;
  qty_ordered: Numeric;
  priority: number;
  status: string;
  planned_start?: string | null;
  planned_end?: string | null;
  created_at: string;
  updated_at: string;
}

export interface WoOperation {
  id: string;
  work_order_id: string;
  routing_op_id?: string | null;
  op_no: number;
  work_center_id?: string | null;
  status: string;
  qty_good: number;
  qty_scrap: number;
  started_at?: string | null;
  completed_at?: string | null;
}

export interface WorkOrderDetail extends WorkOrder {
  operations: WoOperation[];
}

export interface ParetoBar {
  reason_id?: string | null;
  reason_label: string;
  total_seconds: number;
  event_count: number;
}

export interface OeeResult {
  work_center_id: string;
  availability: number;
  performance: number;
  quality: number;
  oee: number;
}

export interface DncTransfer {
  id: string;
  wo_operation_id?: string | null;
  program_id: string;
  direction: string;
  status: string;
  triggered_at?: string | null;
  completed_at?: string | null;
}

export interface ProgramRevision {
  id: string;
  program_id: string;
  revision_no: number;
  status: string;
  submitted_by?: string | null;
  submitted_at?: string | null;
}

export interface Ncr {
  id: string;
  ncr_no: string;
  lot_id?: string | null;
  serial_id?: string | null;
  part_id?: string | null;
  status: string;
  disposition?: string | null;
  reason?: string | null;
  created_at: string;
}

export interface PmSchedule {
  id: string;
  work_center_id: string;
  name: string;
  trigger_type: string;
  interval_value: Numeric;
  next_due_at?: string | null;
  next_due_usage_h?: Numeric | null;
  enabled: boolean;
}

export interface PmDue {
  schedule: PmSchedule;
  current_usage_h: Numeric;
}

export interface MaintenanceWo {
  id: string;
  work_center_id: string;
  wo_type: string;
  status: string;
  technician_id?: string | null;
  notes?: string | null;
  opened_at: string;
  closed_at?: string | null;
}

export interface SparePart {
  id: string;
  code: string;
  name: string;
  uom: string;
  reorder_point: Numeric;
  reorder_qty: Numeric;
  stock: Numeric;
}

export interface ProcurementRequest {
  id: string;
  spare_part_id: string;
  qty_requested: Numeric;
  reason: string;
  status: string;
  erp_reference?: string | null;
  pushed_at?: string | null;
}

export interface ErpConnection {
  id: string;
  site_id?: string | null;
  name: string;
  endpoint_url: string;
  has_token: boolean;
  field_mapping: unknown;
  direction: string;
  enabled: boolean;
  created_at: string;
  updated_at: string;
}

export interface ErpSyncLogEntry {
  id: string;
  connection_id?: string | null;
  direction: string;
  entity: string;
  record_count: number;
  status: string;
  detail?: string | null;
  ts: string;
}

// ---- Live WS events (§10 /ws) --------------------------------------------

export type WsEvent =
  | { event: "work_order_status_changed"; work_order_id: string; status: string }
  | { event: "operation_started"; work_order_id: string; wo_operation_id: string }
  | { event: "operation_completed"; work_order_id: string; wo_operation_id: string }
  | { event: "count_recorded"; wo_operation_id: string; good: number; scrap: number }
  | { event: "downtime_classified"; downtime_event_id: string; reason_id: string }
  | {
      event: "dnc_transfer_scheduled";
      transfer_id: string;
      program_id: string;
      program_identifier: string;
      wo_operation_id?: string | null;
    }
  | { event: "dnc_transfer_completed"; transfer_id: string }
  | { event: "program_revision_drafted"; revision_id: string; program_id: string }
  | {
      event: "oee_snapshot";
      work_center_id: string;
      availability: number;
      performance: number;
      quality: number;
      oee: number;
    }
  | { event: "ncr_raised"; ncr_id: string; ncr_no: string; lot_id?: string | null };
