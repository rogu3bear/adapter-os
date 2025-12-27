import { useCallback, useEffect, useMemo, useState } from 'react';
import { apiClient } from '@/api/services';
import { logger, toError } from '@/utils/logger';
import { useSSE } from './useSSE';
import type { ReasoningSwapEvent, RouterEventStep, RouterRealtimeState } from '@/types/topology';

interface UseRouterEventsOptions {
  enabled?: boolean;
  startingClusterId?: string | null;
  trailLimit?: number;
}

interface NormalizedRouterEvent {
  step: RouterEventStep;
  path?: string[];
}

const ROUTER_EVENTS_ENDPOINT = '/v1/stream/router-events';
const TRACE_RECEIPTS_ENDPOINT = '/v1/stream/trace-receipts';

const coerceString = (value: unknown): string | null => {
  if (typeof value === 'string' && value.trim()) return value;
  if (typeof value === 'number') return String(value);
  return null;
};

const normalizeRouterEvent = (payload: unknown): NormalizedRouterEvent | null => {
  if (!payload || typeof payload !== 'object') return null;
  const raw = payload as Record<string, unknown>;
  const adapterId = coerceString(
    raw.active_adapter ?? raw.adapter_id ?? raw.adapter ?? raw.activeAdapter ?? raw.node_id
  );
  const clusterId = coerceString(
    raw.current_cluster ?? raw.cluster_id ?? raw.cluster ?? raw.active_cluster ?? raw.clusterId
  );
  const pathField = raw.path ?? raw.sequence ?? raw.adapters_used ?? raw.adapter_path;
  const path = Array.isArray(pathField) ? pathField.map((p) => coerceString(p)).filter(Boolean) as string[] : undefined;
  const driftValue = raw.drift ?? raw.drift_distance ?? raw.driftDistance;
  const scoreValue = raw.reasoning_score ?? raw.score ?? raw.router_score;
  const score = typeof scoreValue === 'number' ? scoreValue : Number.isFinite(Number(scoreValue)) ? Number(scoreValue) : null;
  const reason = coerceString(raw.reason ?? raw.message) ?? null;
  const timestamp = typeof raw.timestamp === 'string' && raw.timestamp.trim()
    ? raw.timestamp
    : new Date().toISOString();
  const id = coerceString(raw.event_id ?? raw.id ?? raw.trace_id) ?? `${timestamp}-${adapterId ?? clusterId ?? 'router'}`;

  return {
    step: {
      id,
      adapterId,
      clusterId,
      score,
      reason,
      timestamp,
      drift: typeof driftValue === 'number' ? driftValue : null,
    },
    path,
  };
};

const normalizeReasoningSwap = (payload: unknown): ReasoningSwapEvent | null => {
  if (!payload || typeof payload !== 'object') return null;
  const raw = payload as Record<string, unknown>;
  const eventType = coerceString(raw.event_type ?? raw.type ?? raw.kind);
  const eventTypeLc = eventType?.toLowerCase() ?? '';
  const isSwapFlag =
    raw.reasoning_swap === true ||
    raw.reasoningSwap === true ||
    raw.reasoning === 'swap';

  const toClusterId = coerceString(
    raw.to_cluster_id ?? raw.to_cluster ?? raw.target_cluster ?? raw.cluster_id ?? raw.cluster
  );
  const fromClusterId = coerceString(
    raw.from_cluster_id ?? raw.from_cluster ?? raw.source_cluster ?? raw.previous_cluster
  );
  const toAdapterId = coerceString(
    raw.to_adapter_id ?? raw.to_adapter ?? raw.target_adapter ?? raw.adapter_id ?? raw.active_adapter
  );
  const fromAdapterId = coerceString(
    raw.from_adapter_id ?? raw.from_adapter ?? raw.source_adapter ?? raw.previous_adapter_id
  );

  const swapType =
    eventTypeLc.includes('swap') ||
    eventTypeLc.includes('reasoning_swap') ||
    eventTypeLc.includes('thought_swap') ||
    eventTypeLc.includes('route_on_reasoning');

  const hasTarget = Boolean(toClusterId || toAdapterId);
  const hasSource = Boolean(fromClusterId || fromAdapterId);
  const looksLikeSwap = swapType || (isSwapFlag && hasTarget) || (hasTarget && hasSource);
  if (!looksLikeSwap) return null;

  const timestamp =
    typeof raw.timestamp === 'string' && raw.timestamp.trim()
      ? raw.timestamp
      : new Date().toISOString();

  const id =
    coerceString(raw.event_id ?? raw.id ?? raw.trace_id ?? raw.swap_id) ??
    `${timestamp}-${toClusterId ?? toAdapterId ?? 'swap'}`;

  const reason =
    coerceString(raw.reason ?? raw.rationale ?? raw.rationale_hash) ?? null;
  const traceId = coerceString(raw.trace_id ?? raw.request_id ?? raw.run_id);

  return {
    id,
    fromClusterId,
    toClusterId,
    fromAdapterId,
    toAdapterId,
    reason,
    traceId,
    timestamp,
  };
};

const mergeTrail = (prevTrail: string[], adapterId?: string | null, explicitPath?: string[], limit: number = 24) => {
  let next = prevTrail;
  if (explicitPath && explicitPath.length > 0) {
    next = explicitPath.filter(Boolean);
  } else if (adapterId) {
    if (prevTrail[prevTrail.length - 1] !== adapterId) {
      next = [...prevTrail, adapterId];
    }
  }
  return next.slice(-limit);
};

export interface UseRouterEventsResult {
  state: RouterRealtimeState;
  steps: RouterEventStep[];
  swaps: ReasoningSwapEvent[];
  connected: boolean;
  circuitOpen: boolean;
  reconnectAttempts: number;
  error: Error | null;
  reconnect: () => void;
  forceClusterLock: (clusterId: string) => Promise<void>;
}

export function useRouterEvents(options: UseRouterEventsOptions = {}): UseRouterEventsResult {
  const { enabled = true, startingClusterId = null, trailLimit = 24 } = options;
  const [state, setState] = useState<RouterRealtimeState>({
    activeAdapterId: null,
    activeClusterId: startingClusterId ?? null,
    reasoningScore: null,
    startingClusterId: startingClusterId ?? null,
    driftDistance: null,
    trail: [],
  });
  const [steps, setSteps] = useState<RouterEventStep[]>([]);
  const [swaps, setSwaps] = useState<ReasoningSwapEvent[]>([]);

  useEffect(() => {
    if (!startingClusterId) return;
    setState((prev) => {
      if (prev.startingClusterId) return prev;
      return { ...prev, startingClusterId };
    });
  }, [startingClusterId]);

  const handleRouterEvent = useCallback((payload: unknown) => {
    const normalized = normalizeRouterEvent(payload);
    if (!normalized) return;

    const { step, path } = normalized;

    setSteps((prev) => {
      const next = [...prev, step];
      if (next.length > trailLimit) {
        next.splice(0, next.length - trailLimit);
      }
      return next;
    });

    setState((prev) => {
      const trail = mergeTrail(prev.trail, step.adapterId, path, trailLimit);
      return {
        activeAdapterId: step.adapterId ?? prev.activeAdapterId,
        activeClusterId: step.clusterId ?? prev.activeClusterId,
        reasoningScore: typeof step.score === 'number' ? step.score : prev.reasoningScore,
        startingClusterId: prev.startingClusterId ?? startingClusterId ?? step.clusterId ?? null,
        driftDistance: step.drift ?? prev.driftDistance,
        lastUpdated: step.timestamp,
        trail,
      };
    });
  }, [startingClusterId, trailLimit]);

  const handleReasoningSwap = useCallback(
    (payload: unknown) => {
      const swap = normalizeReasoningSwap(payload);
      if (!swap) return;

      setSwaps((prev) => {
        const next = [...prev, swap];
        if (next.length > trailLimit) {
          next.splice(0, next.length - trailLimit);
        }
        return next;
      });

      setState((prev) => {
        const trail = mergeTrail(prev.trail, swap.toAdapterId, undefined, trailLimit);
        return {
          ...prev,
          activeAdapterId: swap.toAdapterId ?? prev.activeAdapterId,
          activeClusterId: swap.toClusterId ?? prev.activeClusterId,
          lastUpdated: swap.timestamp,
          trail,
        };
      });
    },
    [trailLimit],
  );

  const forceClusterLock = useCallback(async (clusterId: string) => {
    const payload = { cluster_id: clusterId };
    await apiClient.request('/v1/router/force_cluster', {
      method: 'POST',
      body: JSON.stringify(payload),
    });
    logger.info('Sent force_cluster signal', {
      component: 'useRouterEvents',
      clusterId,
    });
  }, []);

  const { connected, error, reconnect, circuitOpen, reconnectAttempts } = useSSE<unknown>(ROUTER_EVENTS_ENDPOINT, {
    enabled,
    onMessage: handleRouterEvent,
    onError: (event) => {
      logger.error('Router events stream error', { component: 'useRouterEvents' }, toError(event as unknown as Error));
    },
  });

  const {
    connected: swapConnected,
    error: swapError,
    reconnect: swapReconnect,
    circuitOpen: swapCircuitOpen,
    reconnectAttempts: swapReconnectAttempts,
  } = useSSE<unknown>(TRACE_RECEIPTS_ENDPOINT, {
    enabled,
    onMessage: handleReasoningSwap,
    onError: (event) => {
      logger.error('Trace receipt stream error', { component: 'useRouterEvents' }, toError(event as unknown as Error));
    },
  });

  const unifiedConnected = connected && swapConnected;
  const unifiedCircuitOpen = circuitOpen || swapCircuitOpen;
  const unifiedReconnectAttempts = Math.max(reconnectAttempts, swapReconnectAttempts);
  const unifiedError = error ?? swapError;

  const unifiedReconnect = useCallback(() => {
    reconnect();
    swapReconnect();
  }, [reconnect, swapReconnect]);

  return useMemo(() => ({
    state,
    steps,
    swaps,
    connected: unifiedConnected,
    circuitOpen: unifiedCircuitOpen,
    reconnectAttempts: unifiedReconnectAttempts,
    error: unifiedError,
    reconnect: unifiedReconnect,
    forceClusterLock,
  }), [
    state,
    steps,
    swaps,
    unifiedConnected,
    unifiedCircuitOpen,
    unifiedReconnectAttempts,
    unifiedError,
    unifiedReconnect,
    forceClusterLock,
  ]);
}
