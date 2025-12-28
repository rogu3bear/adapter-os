/**
 * System Status Types
 *
 * Minimal client-side representation of the aggregated system status
 * endpoint. All fields are optional to allow graceful fallback when the
 * backend doesn't expose them yet.
 */

export type StatusIndicator = boolean | string | number | null | undefined;

/**
 * Data availability indicator for telemetry metrics.
 *
 * Used to distinguish between real measured data, unavailable data, and stale data.
 * This prevents the UI from displaying zeros as if they were real when data is missing.
 */
export type DataAvailability = 'available' | 'unavailable' | 'stale';

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

/**
 * ANE (Apple Neural Engine) memory status.
 *
 * When `availability` is 'unavailable', all numeric fields will be null/undefined.
 * The UI must display "Unavailable" rather than zeros in this case.
 */
export interface AneMemoryStatus {
  /** Whether ANE metrics are actually available */
  availability?: DataAvailability;
  /** Allocated ANE memory in MB (null if unavailable) */
  allocatedMb?: number | null;
  /** Used ANE memory in MB (null if unavailable) */
  usedMb?: number | null;
  /** Available ANE memory in MB (null if unavailable) */
  availableMb?: number | null;
  /** ANE usage percentage (null if unavailable) */
  usagePct?: number | null;
  /** @deprecated Use usagePct instead */
  totalMb?: number | null;
  /** @deprecated Use availability instead */
  pressure?: StatusIndicator;
}

/**
 * UMA (Unified Memory Architecture) memory status.
 *
 * When `availability` is 'unavailable', all numeric fields will be null/undefined.
 * The UI must display "Unavailable" rather than zeros in this case.
 */
export interface UmaMemoryStatus {
  /** Whether UMA metrics are actually available */
  availability?: DataAvailability;
  /** Total UMA memory in MB (null if unavailable) */
  totalMb?: number | null;
  /** Used UMA memory in MB (null if unavailable) */
  usedMb?: number | null;
  /** Available UMA memory in MB (null if unavailable) */
  availableMb?: number | null;
  /** UMA headroom percentage (null if unavailable) */
  headroomPct?: number | null;
}

/**
 * Kernel memory summary containing ANE and UMA metrics.
 */
export interface KernelMemorySummary {
  ane?: AneMemoryStatus | null;
  uma?: UmaMemoryStatus | null;
  pressure?: string | null;
}

export interface KernelStatus {
  activeModel?: string | null;
  activePlan?: string | null;
  activeAdapters?: number | null;
  hotAdapters?: number | null;
  /** @deprecated Use memory.ane instead */
  aneMemory?: AneMemoryStatus | null;
  /** @deprecated Use memory.uma.headroomPct or memory.pressure instead */
  umaPressure?: string | null;
  /** Complete memory summary with availability indicators */
  memory?: KernelMemorySummary | null;
}

export interface BootStatus {
  phase?: string | null;
  degradedReasons?: string[] | null;
  bootTraceId?: string | null;
  lastError?: string | null;
}

/**
 * Known inference blockers that prevent inference from running.
 * These match the backend InferenceBlocker enum.
 */
export type InferenceBlocker =
  | 'database_unavailable'
  | 'worker_missing'
  | 'no_model_loaded'
  | 'active_model_mismatch'
  | 'telemetry_degraded'
  | 'system_booting'
  | 'boot_failed';

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
