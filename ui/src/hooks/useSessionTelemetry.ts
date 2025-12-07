import { useCallback, useEffect, useMemo, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import apiClient from '@/api/client';
import type {
  SessionRouterViewResponse,
  SessionStep,
  TransformedRoutingDecision,
  MetricsSeriesResponse,
} from '@/api/types';
import { logger, toError } from '@/utils/logger';
import type { AdapterStack } from '@/api/adapter-types';

export interface SessionSummary {
  requestId: string;
  timestamp: string;
  adapters: string[];
  entropy?: number;
}

export interface MetricPoint {
  timestamp: string;
  value: number;
}

interface UseSessionTelemetryOptions {
  initialRequestId?: string;
  tenantId?: string;
  pageSize?: number;
  initialFilter?: string;
  sourceType?: string;
}

interface UseSessionTelemetryResult {
  sessions: SessionSummary[];
  sessionsLoading: boolean;
  sessionsError: Error | null;
  refetchSessions: () => Promise<unknown>;
  page: number;
  setPage: (page: number) => void;
  filterRequestId: string;
  setFilterRequestId: (value: string) => void;
  selectedRequestId: string | null;
  selectRequestId: (requestId: string) => void;
  steps: SessionStep[];
  stepsLoading: boolean;
  stepsError: Error | null;
  refetchSteps: () => Promise<unknown>;
  tokensPerSecond: MetricPoint[];
  latencyP50: MetricPoint[];
  metricsLoading: boolean;
  metricsError: Error | null;
  refetchMetrics: () => Promise<unknown>;
  totalTokens: number;
  adapterMap: Map<number, { id: string; name: string }>;
}

function uniqueSessions(decisions: TransformedRoutingDecision[] | undefined): SessionSummary[] {
  if (!decisions) return [];

  const byRequest = new Map<string, TransformedRoutingDecision>();

  for (const decision of decisions) {
    if (!decision.request_id) continue;
    if (!byRequest.has(decision.request_id)) {
      byRequest.set(decision.request_id, decision);
    }
  }

  return Array.from(byRequest.values()).map((decision) => ({
    requestId: decision.request_id,
    timestamp: decision.timestamp,
    adapters: decision.selected_adapters ?? [],
    entropy: decision.entropy,
  }));
}

function flattenMetrics(series: MetricsSeriesResponse[] | undefined): MetricPoint[] {
  if (!series) return [];
  return series.flatMap((s) =>
    s.data_points.map((p) => ({
      timestamp: p.timestamp,
      value: p.value,
    }))
  );
}

export function useSessionTelemetry(options: UseSessionTelemetryOptions = {}): UseSessionTelemetryResult {
  const { initialRequestId, tenantId, pageSize = 25, initialFilter = '', sourceType } = options;

  const [selectedRequestId, setSelectedRequestId] = useState<string | null>(initialRequestId ?? null);
  const [page, setPage] = useState<number>(1);
  const [filterRequestId, setFilterRequestId] = useState<string>(initialFilter);

  // Recent sessions list
  const {
    data: decisions,
    isLoading: sessionsLoading,
    error: sessionsErrorRaw,
    refetch: refetchSessions,
  } = useQuery({
    queryKey: ['session-telemetry', 'sessions', tenantId, page, pageSize, filterRequestId, sourceType],
    queryFn: async () =>
      apiClient.getRoutingDecisions({
        limit: pageSize,
        offset: (page - 1) * pageSize,
        tenant_id: tenantId,
        request_id: filterRequestId || undefined,
        source_type: sourceType || undefined,
      }),
    staleTime: 30_000,
  });

  const sessions = useMemo(() => uniqueSessions(decisions), [decisions]);

  // Auto-select first session if none selected
  useEffect(() => {
    if (!selectedRequestId && sessions.length > 0) {
      setSelectedRequestId(sessions[0].requestId);
    }
  }, [sessions, selectedRequestId]);

  // Steps for the selected session
  const {
    data: sessionRouterView,
    isLoading: stepsLoading,
    error: stepsErrorRaw,
    refetch: refetchSteps,
  } = useQuery<SessionRouterViewResponse>({
    queryKey: ['session-telemetry', 'steps', selectedRequestId],
    queryFn: async () => {
      if (!selectedRequestId) {
        throw new Error('No request ID selected');
      }
      return apiClient.getSessionRouterView(selectedRequestId);
    },
    enabled: !!selectedRequestId,
    staleTime: 15_000,
  });

  const steps: SessionStep[] = useMemo(
    () => sessionRouterView?.steps ?? [],
    [sessionRouterView?.steps]
  );

  // Adapter map (index -> {id,name}) derived from stack_id if available
  const {
    data: adapterMap,
  } = useQuery<Map<number, { id: string; name: string }>>({
    queryKey: ['session-telemetry', 'adapter-map', sessionRouterView?.stack_id],
    queryFn: async () => {
      const map = new Map<number, { id: string; name: string }>();
      const stackId = sessionRouterView?.stack_id;
      if (!stackId) return map;

      let stack: AdapterStack | null = null;
      try {
        stack = await apiClient.getAdapterStack(stackId);
      } catch (err) {
        logger.warn('Failed to fetch adapter stack for telemetry session', {
          component: 'useSessionTelemetry',
          stackId,
          details: toError(err).message,
        });
        return map;
      }

      if (stack?.adapter_ids && stack.adapter_ids.length > 0) {
        await Promise.all(
          stack.adapter_ids.map(async (adapterId, idx) => {
            try {
              const adapter = await apiClient.getAdapter(adapterId);
              map.set(idx, { id: adapterId, name: adapter.name || adapterId });
            } catch (err) {
              map.set(idx, { id: adapterId, name: adapterId });
              logger.warn('Failed to resolve adapter name', {
                component: 'useSessionTelemetry',
                adapterId,
                details: toError(err).message,
              });
            }
          })
        );
      }

      return map;
    },
    enabled: !!sessionRouterView?.stack_id,
    staleTime: 60_000,
  });

  const totalTokens = useMemo(() => {
    if (!steps.length) return 0;
    // Prefer input_token_id when present; otherwise fall back to step count
    const maxTokenId = steps
      .map((s) => s.input_token_id)
      .filter((id): id is number => typeof id === 'number')
      .reduce((max, id) => Math.max(max, id), 0);
    return maxTokenId > 0 ? maxTokenId + 1 : steps.length;
  }, [steps]);

  // Metrics window derived from steps
  const [timeWindow, setTimeWindow] = useState<{ startMs: number; endMs: number } | null>(null);
  useEffect(() => {
    if (!steps.length) {
      setTimeWindow(null);
      return;
    }
    const firstTs = Date.parse(steps[0].timestamp);
    const lastTs = Date.parse(steps[steps.length - 1].timestamp);
    if (Number.isNaN(firstTs) || Number.isNaN(lastTs)) {
      setTimeWindow(null);
      return;
    }
    // Pad end time slightly to include trailing metrics
    const paddedEnd = Math.max(lastTs, firstTs + 1_000);
    setTimeWindow({ startMs: firstTs, endMs: paddedEnd });
  }, [steps]);

  const {
    data: tokensPerSecondSeries,
    isLoading: metricsLoading,
    error: metricsErrorRaw,
    refetch: refetchMetrics,
  } = useQuery<MetricsSeriesResponse[]>({
    queryKey: ['session-telemetry', 'metrics-tps', selectedRequestId, timeWindow?.startMs, timeWindow?.endMs],
    queryFn: async () => {
      if (!timeWindow) return [];
      return apiClient.getMetricsSeries({
        series_name: 'tokens_per_second',
        start_ms: timeWindow.startMs,
        end_ms: timeWindow.endMs,
      });
    },
    enabled: !!selectedRequestId && !!timeWindow,
    staleTime: 15_000,
  });

  const { data: latencySeries } = useQuery<MetricsSeriesResponse[]>({
    queryKey: ['session-telemetry', 'metrics-latency', selectedRequestId, timeWindow?.startMs, timeWindow?.endMs],
    queryFn: async () => {
      if (!timeWindow) return [];
      return apiClient.getMetricsSeries({
        series_name: 'latency_p50_ms',
        start_ms: timeWindow.startMs,
        end_ms: timeWindow.endMs,
      });
    },
    enabled: !!selectedRequestId && !!timeWindow,
    staleTime: 15_000,
  });

  const tokensPerSecond = useMemo(() => flattenMetrics(tokensPerSecondSeries), [tokensPerSecondSeries]);
  const latencyP50 = useMemo(() => flattenMetrics(latencySeries), [latencySeries]);

  const selectRequestId = useCallback((requestId: string) => {
    setSelectedRequestId(requestId);
    logger.info('Telemetry session selected', {
      component: 'useSessionTelemetry',
      requestId,
    });
  }, []);

  return {
    sessions,
    sessionsLoading,
    sessionsError: sessionsErrorRaw as Error | null,
    refetchSessions,
    page,
    setPage,
    filterRequestId,
    setFilterRequestId,
    selectedRequestId,
    selectRequestId,
    steps,
    stepsLoading,
    stepsError: stepsErrorRaw as Error | null,
    refetchSteps,
    tokensPerSecond,
    latencyP50,
    metricsLoading,
    metricsError: metricsErrorRaw as Error | null,
    refetchMetrics,
    totalTokens,
    adapterMap: adapterMap ?? new Map(),
  };
}

