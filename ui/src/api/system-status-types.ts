/**
 * System Status Types
 *
 * Minimal client-side representation of the aggregated system status
 * endpoint. All fields are optional to allow graceful fallback when the
 * backend doesn't expose them yet.
 */

export type StatusIndicator = boolean | string | number | null | undefined;

export interface DriftStatus {
  status?: string | null;
  detail?: string | null;
  lastRun?: string | null;
}

export interface IntegrityStatus {
  localSecureMode?: StatusIndicator;
  strictMode?: StatusIndicator;
  pfDeny?: StatusIndicator;
  drift?: DriftStatus | string | null;
}

export interface ReadinessStatus {
  db?: StatusIndicator;
  migrations?: StatusIndicator;
  workers?: StatusIndicator;
  modelsSeeded?: StatusIndicator;
  phase?: string | null;
  bootTraceId?: string | null;
  degraded?: string[] | null;
}

export interface AneMemoryStatus {
  usedMb?: number | null;
  totalMb?: number | null;
  pressure?: StatusIndicator;
}

export interface KernelStatus {
  activeModel?: string | null;
  activePlan?: string | null;
  activeAdapters?: number | null;
  hotAdapters?: number | null;
  aneMemory?: AneMemoryStatus | null;
  umaPressure?: string | null;
}

export interface BootStatus {
  phase?: string | null;
  degradedReasons?: string[] | null;
  bootTraceId?: string | null;
  lastError?: string | null;
}

export interface SystemStatusResponse {
  schemaVersion?: string;
  timestamp?: string;
  integrity?: IntegrityStatus;
  readiness?: ReadinessStatus;
  inferenceReady?: StatusIndicator;
  inferenceBlockers?: string[] | null;
  kernel?: KernelStatus;
  boot?: BootStatus;
  components?: Array<{
    name?: string;
    status?: string;
    message?: string;
  }>;
}
