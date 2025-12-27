// @ts-nocheck
import React, { createContext, useContext, useMemo, useState } from 'react';
import { useSSE } from '@/hooks/realtime/useSSE';
import { useModelStatus } from '@/hooks/model-loading/useModelStatus';
import { useSystemState } from '@/hooks/system/useSystemState';
import type { MetricsStreamEvent, MetricsSnapshotEvent, SystemMetricsEvent } from '@/api/streaming-types';

type KernelTelemetryState = {
  backendLabel: 'Metal' | 'CPU' | 'Auto';
  vramUsedMb?: number | null;
  vramTotalMb?: number | null;
  latencyMs?: number | null;
  uptimeSeconds?: number | null;
  baseModelName?: string | null;
  baseModelId?: string | null;
  baseModelStatus?: string | null;
  baseModelLoadedAt?: string | null;
  metricsConnected: boolean;
  metricsError: Error | null;
  metricsStale: boolean;
  lastMetricsAt?: number | null;
};

const KernelTelemetryContext = createContext<KernelTelemetryState | null>(null);

function extractGpuMetrics(event: MetricsStreamEvent): { used?: number | null; total?: number | null } {
  if ((event as SystemMetricsEvent).gpu) {
    const gpu = (event as SystemMetricsEvent).gpu!;
    return {
      used: gpu.memory_used_mb,
      total: gpu.memory_total_mb,
    };
  }

  if ('gpu_memory_used_mb' in (event as Record<string, unknown>)) {
    const maybeUsed = (event as { gpu_memory_used_mb?: number }).gpu_memory_used_mb;
    const maybeTotal = (event as { gpu_memory_total_mb?: number }).gpu_memory_total_mb;
    return {
      used: typeof maybeUsed === 'number' ? maybeUsed : null,
      total: typeof maybeTotal === 'number' ? maybeTotal : null,
    };
  }

  return { used: null, total: null };
}

function extractLatency(event: MetricsStreamEvent): number | null {
  if ((event as MetricsSnapshotEvent).latency) {
    return (event as MetricsSnapshotEvent).latency.p95_ms;
  }
  if ('latency_p95_ms' in (event as Record<string, unknown>)) {
    const maybe = (event as { latency_p95_ms?: number }).latency_p95_ms;
    return typeof maybe === 'number' ? maybe : null;
  }
  return null;
}

interface KernelTelemetryProviderProps {
  tenantId: string;
  children: React.ReactNode;
}

export function KernelTelemetryProvider({ tenantId, children }: KernelTelemetryProviderProps) {
  const [lastMetricsAt, setLastMetricsAt] = useState<number | null>(null);
  const [latestEvent, setLatestEvent] = useState<MetricsStreamEvent | null>(null);

  const { connected: metricsConnected, error: metricsError } = useSSE<MetricsStreamEvent>(
    '/v1/stream/metrics',
    {
      enabled: true,
      onMessage: (event) => {
        setLatestEvent(event);
        setLastMetricsAt(Date.now());
      },
    }
  );

  const { node: systemNode } = useSystemState({
    tenantId,
    pollingInterval: 20000,
  }).data ?? { node: null };

  const modelStatus = useModelStatus(tenantId, 5000);

  const telemetry = useMemo<KernelTelemetryState>(() => {
    const gpuMetrics = latestEvent ? extractGpuMetrics(latestEvent) : { used: null, total: null };
    const latencyMs = latestEvent ? extractLatency(latestEvent) : null;

    const backendLabel: KernelTelemetryState['backendLabel'] =
      systemNode?.gpu_available || gpuMetrics.total
        ? 'Metal'
        : 'CPU';

    const metricsStale = Boolean(
      lastMetricsAt &&
      Date.now() - lastMetricsAt > 5000
    );

    return {
      backendLabel,
      vramUsedMb: gpuMetrics.used,
      vramTotalMb: gpuMetrics.total,
      latencyMs,
      uptimeSeconds: systemNode?.uptime_seconds ?? null,
      baseModelName: modelStatus.modelName,
      baseModelId: modelStatus.modelId,
      baseModelStatus: modelStatus.status,
      baseModelLoadedAt: null,
      metricsConnected,
      metricsError,
      metricsStale,
      lastMetricsAt,
    };
  }, [
    latestEvent,
    metricsConnected,
    metricsError,
    systemNode?.gpu_available,
    systemNode?.uptime_seconds,
    modelStatus.modelName,
    modelStatus.modelId,
    modelStatus.status,
    modelStatus.modelPath,
    lastMetricsAt,
  ]);

  return (
    <KernelTelemetryContext.Provider value={telemetry}>
      {children}
    </KernelTelemetryContext.Provider>
  );
}

export function useKernelTelemetry(): KernelTelemetryState {
  const ctx = useContext(KernelTelemetryContext);
  if (!ctx) {
    return {
      backendLabel: 'CPU',
      vramUsedMb: null,
      vramTotalMb: null,
      latencyMs: null,
      uptimeSeconds: null,
      baseModelId: null,
      baseModelName: null,
      baseModelStatus: null,
      baseModelLoadedAt: null,
      metricsConnected: false,
      metricsError: null,
      metricsStale: true,
      lastMetricsAt: null,
    } as KernelTelemetryState;
  }
  return ctx;
}
