import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { apiClient } from '@/api/services';
import type { SystemStatusResponse, StatusIndicator, AneMemoryStatus } from '@/api/system-status-types';
import type { SystemStateResponse } from '@/api/system-state-types';
import type {
  BaseModelStatus,
  DeterminismStatusResponse,
  ReadyzResponse,
  SystemReadyResponse,
} from '@/api/api-types';
import { MODEL_STATUS_EVENT } from '@/hooks/model-loading';

type StatusSource = 'native' | 'fallback';

export interface UseSystemStatusOptions {
  enabled?: boolean;
  tenantId?: string | null;
  autoRefreshOnModelEvents?: boolean;
}

export interface UseSystemStatusReturn {
  data: SystemStatusResponse | null;
  loading: boolean;
  error: Error | null;
  source: StatusSource | null;
  /** True when data is from fallback endpoints, not native /v1/system/status */
  isFallback: boolean;
  /** Schema version: 'v1' for native, 'fallback' for constructed */
  schemaVersion: string | null;
  lastUpdated: Date | null;
  /** True when returning cached data due to fetch failure */
  stale: boolean;
  refetch: () => Promise<void>;
}

function coerceStatus(value: StatusIndicator): StatusIndicator {
  if (value === undefined) return null;
  return value;
}

function summarizeAdapters(state: SystemStateResponse | null) {
  if (!state) {
    return {
      activePlan: null as string | null,
      totalAdapters: null as number | null,
      hotAdapters: null as number | null,
      umaPressure: null as string | null,
      aneMemory: null as AneMemoryStatus | null,
    };
  }

  let activePlan: string | null = null;
  let totalAdapters = 0;
  let hotAdapters = 0;
  let aneMemory: AneMemoryStatus | null = null;

  for (const tenant of state.tenants ?? []) {
    for (const stack of tenant.stacks ?? []) {
      totalAdapters += stack.adapter_count ?? 0;
      if (stack.is_active && !activePlan) {
        activePlan = stack.name ?? stack.stack_id ?? null;
      }
      if (Array.isArray(stack.adapters)) {
        hotAdapters += stack.adapters.filter((adapter) => adapter.state === 'hot').length;
      }
    }
  }

  if (state.memory?.ane) {
    aneMemory = {
      usedMb: state.memory.ane.used_mb ?? null,
      totalMb: state.memory.ane.allocated_mb ?? state.memory.ane.available_mb ?? null,
      pressure: state.memory.ane.usage_percent ?? null,
    };
  }

  return {
    activePlan,
    totalAdapters,
    hotAdapters,
    umaPressure: state.memory?.pressure_level ?? null,
    aneMemory,
  };
}

/**
 * Boot phases that indicate the system is still booting (not ready for inference).
 * These match the backend BootState::is_booting() check.
 */
const BOOTING_PHASES = new Set([
  'stopped',
  'starting',
  'db-connecting',
  'migrating',
  'seeding',
  'loading-policies',
  'starting-backend',
  'loading-base-models',
  'loading-adapters',
  'worker-discovery',
  // Legacy aliases
  'booting',
  'initializing-db',
]);

function isBootingPhase(phase: string | null | undefined): boolean {
  if (!phase) return false;
  return BOOTING_PHASES.has(phase.toLowerCase());
}

function isFailedPhase(phase: string | null | undefined): boolean {
  if (!phase) return false;
  const normalized = phase.toLowerCase();
  return normalized === 'failed' || normalized.includes('fail') || normalized.includes('panic');
}

function buildInferenceStatus(
  readyz: ReadyzResponse | null,
  phase: string | null,
  degraded: string[],
): { inferenceReady: string | null; inferenceBlockers: string[] | null } {
  const blockers: string[] = [];

  // Check boot state first - block ALL boot phases until Ready
  if (isBootingPhase(phase)) {
    return {
      inferenceReady: 'false',
      inferenceBlockers: ['system_booting'],
    };
  }

  if (isFailedPhase(phase)) {
    return {
      inferenceReady: 'false',
      inferenceBlockers: ['boot_failed'],
    };
  }

  // Check for degraded state - any degradation triggers TelemetryDegraded
  if (degraded.length > 0) {
    blockers.push('telemetry_degraded');
  }

  // Reconstruct blockers from legacy endpoints
  if (readyz?.checks?.db?.ok === false) {
    blockers.push('database_unavailable');
  }
  if (readyz?.checks?.worker?.ok === false) {
    blockers.push('worker_missing');
  }
  if (readyz?.checks?.models_seeded?.ok === false) {
    blockers.push('no_model_loaded');
  }

  const inferenceReady = blockers.length === 0 ? 'true' : 'false';

  return {
    inferenceReady,
    inferenceBlockers: blockers.length > 0 ? blockers : null,
  };
}

function buildFallbackStatus(
  readyz: ReadyzResponse | null,
  systemReady: SystemReadyResponse | null,
  systemState: SystemStateResponse | null,
  baseModel: BaseModelStatus | null,
  determinism: DeterminismStatusResponse | null,
  settings: unknown,
  tenantId?: string | null,
): SystemStatusResponse {
  const readyzExtras = (readyz as (ReadyzResponse & {
    phases?: Array<{ state?: string | null }>;
    boot_trace_id?: string | null;
    last_error_code?: string | null;
  }) | null) || null;
  const degraded = [
    ...(systemReady?.critical_degraded ?? []),
    ...(systemReady?.non_critical_degraded ?? []),
  ];

  const phaseFromReadyz = readyzExtras?.phases?.length
    ? readyzExtras.phases[readyzExtras.phases.length - 1]?.state
    : null;
  const phase =
    systemReady?.state ||
    phaseFromReadyz ||
    (readyz?.ready === false ? 'Starting' : null);

  const componentStatus = systemReady?.components || [];
  const migrationsComponent = componentStatus.find((c) =>
    (c.component || '').toLowerCase().includes('migration')
  );

  const adapterSummary = summarizeAdapters(systemState);

  const securitySettings =
    settings && typeof settings === 'object' && 'security' in (settings as Record<string, unknown>)
      ? (settings as { security?: { egress_enabled?: boolean; require_pf_deny?: boolean } }).security
      : undefined;

  return {
    schemaVersion: 'fallback',
    timestamp: new Date().toISOString(),
    integrity: {
      localSecureMode:
        typeof securitySettings?.egress_enabled === 'boolean'
          ? !securitySettings.egress_enabled
          : null,
      strictMode: null,
      pfDeny: coerceStatus(securitySettings?.require_pf_deny),
      drift: determinism
        ? {
            status: determinism.result,
            detail: determinism.divergences
              ? `${determinism.divergences} divergences`
              : undefined,
            lastRun: determinism.last_run,
          }
        : null,
    },
    readiness: {
      db: coerceStatus(readyz?.checks?.db?.ok),
      migrations: coerceStatus(migrationsComponent?.status),
      workers: coerceStatus(readyz?.checks?.worker?.ok),
      modelsSeeded: coerceStatus(readyz?.checks?.models_seeded?.ok),
      phase,
      bootTraceId: readyzExtras?.boot_trace_id ?? null,
      degraded,
    },
    ...buildInferenceStatus(readyz, phase, degraded),
    kernel: {
      activeModel: baseModel?.model_name ?? baseModel?.model_id ?? null,
      activePlan: adapterSummary.activePlan,
      activeAdapters: adapterSummary.totalAdapters,
      hotAdapters: adapterSummary.hotAdapters,
      aneMemory: adapterSummary.aneMemory,
      umaPressure: adapterSummary.umaPressure,
    },
    boot: {
      phase,
      degradedReasons: degraded.length ? degraded : null,
      bootTraceId: readyzExtras?.boot_trace_id ?? null,
      lastError: systemReady?.reason ?? readyzExtras?.last_error_code ?? null,
    },
    components: componentStatus.map((c) => ({
      name: c.component,
      status: c.status,
      message: c.message,
    })),
  };
}

export function useSystemStatus(options: UseSystemStatusOptions = {}): UseSystemStatusReturn {
  const { enabled = true, tenantId, autoRefreshOnModelEvents = true } = options;
  const [data, setData] = useState<SystemStatusResponse | null>(null);
  const [source, setSource] = useState<StatusSource | null>(null);
  const [error, setError] = useState<Error | null>(null);
  const [loading, setLoading] = useState(false);
  const [lastUpdated, setLastUpdated] = useState<Date | null>(null);
  const lastGoodRef = useRef<SystemStatusResponse | null>(null);

  const fetchStatus = useCallback(async () => {
    if (!enabled) return;
    setLoading(true);
    try {
      const native = await apiClient.getSystemStatus();
      setData(native);
      setSource('native');
      setError(null);
      lastGoodRef.current = native;
      setLastUpdated(new Date());
      setLoading(false);
      return;
    } catch (nativeErr) {
      setSource((prev) => prev ?? null);
      setError(nativeErr as Error);
    }

    try {
      const [readyz, systemReady, systemState, baseModel, determinism, settings] = await Promise.all([
        apiClient.getReadyz().catch(() => null),
        apiClient.getSystemReady().catch(() => null),
        apiClient.getSystemState({
          include_adapters: true,
          top_adapters: 5,
          tenant_id: tenantId ?? undefined,
        }).catch(() => null),
        apiClient.getBaseModelStatus(tenantId ?? undefined).catch(() => null),
        apiClient.getDeterminismStatus().catch(() => null),
        apiClient.getSettings().catch(() => null),
      ]);

      const fallback = buildFallbackStatus(
        readyz,
        systemReady,
        systemState,
        baseModel,
        determinism,
        settings,
        tenantId,
      );
      setData(fallback);
      setSource('fallback');
      setError(null);
      lastGoodRef.current = fallback;
      setLastUpdated(new Date());
    } catch (fallbackErr) {
      setError(fallbackErr as Error);
      setData(lastGoodRef.current);
    } finally {
      setLoading(false);
    }
  }, [enabled, tenantId]);

  useEffect(() => {
    if (!enabled) return;
    void fetchStatus();
  }, [enabled, fetchStatus]);

  useEffect(() => {
    if (!enabled || !autoRefreshOnModelEvents) return;
    const handler = () => void fetchStatus();
    window.addEventListener(MODEL_STATUS_EVENT, handler);
    return () => window.removeEventListener(MODEL_STATUS_EVENT, handler);
  }, [autoRefreshOnModelEvents, enabled, fetchStatus]);

  const stale = useMemo(() => Boolean(error && lastGoodRef.current), [error]);
  const isFallback = source === 'fallback';
  const schemaVersion = data?.schemaVersion ?? null;

  return {
    data,
    loading,
    error,
    source,
    isFallback,
    schemaVersion,
    lastUpdated,
    stale,
    refetch: fetchStatus,
  };
}

export default useSystemStatus;
