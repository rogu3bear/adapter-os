import { useCallback, useMemo } from 'react';
import { usePolling, type PollingSpeed } from '@/hooks/realtime/usePolling';
import { useAsyncOperation } from '@/hooks/async/useAsyncOperation';
import { apiClient } from '@/api/services';
import { useDemoMetrics } from '@/hooks/demo/useDemoMetrics';
import type {
  SystemMetrics,
  MetricsSnapshotResponse,
  Node,
  NodeDetailsResponse,
  NodePingResponse,
  WorkerResponse,
  WorkerDetailsResponse,
  ProcessLog,
  ProcessCrash,
  DebugSession,
  TroubleshootingResult,
  RegisterNodeRequest,
  SpawnWorkerRequest,
  ProcessLogFilters,
  DebugSessionConfig,
  TroubleshootingStep,
} from '@/api/api-types';

// System Metrics Hook
/** Return system metrics with polling and circuit breaker support. */
export interface UseSystemMetricsReturn {
  metrics: SystemMetrics | null;
  isLoading: boolean;
  error: Error | null;
  lastUpdated: Date | null;
  refetch: () => Promise<void>;
}

export function useSystemMetrics(
  speed: PollingSpeed = 'normal',
  enabled = true
): UseSystemMetricsReturn {
  const { data, isLoading, error, lastUpdated, refetch } = usePolling<SystemMetrics>(
    () => apiClient.getSystemMetrics(),
    speed,
    {
      enabled,
      operationName: 'getSystemMetrics',
      enableCircuitBreaker: true,
      showLoadingIndicator: true,
    }
  );

  const { metrics: demoMetrics, lastUpdated: demoLastUpdated } = useDemoMetrics(data, lastUpdated);

  return {
    metrics: demoMetrics,
    isLoading,
    error,
    lastUpdated: demoLastUpdated,
    refetch,
  };
}

// Quality Metrics Hook
export function useQualityMetrics(speed: PollingSpeed = 'slow', enabled = true) {
  return usePolling(
    () => apiClient.getQualityMetrics(),
    speed,
    {
      enabled,
      operationName: 'getQualityMetrics',
    }
  );
}

// Adapter Metrics Hook
export function useAdapterMetrics(speed: PollingSpeed = 'normal', enabled = true) {
  return usePolling(
    () => apiClient.getAdapterMetrics(),
    speed,
    {
      enabled,
      operationName: 'getAdapterMetrics',
    }
  );
}

// Metrics Snapshot Hook
export interface UseMetricsSnapshotReturn {
  data: MetricsSnapshotResponse | null;
  isLoading: boolean;
  error: Error | null;
  lastUpdated: Date | null;
  refetch: () => Promise<void>;
}

/** Return aggregated metrics snapshot with safe loading/error surface. */
export function useMetricsSnapshot(enabled = true): UseMetricsSnapshotReturn {
  const { data, isLoading, error, lastUpdated, refetch } = usePolling<MetricsSnapshotResponse>(
    () => apiClient.getMetricsSnapshot(),
    'slow',
    {
      enabled,
      operationName: 'getMetricsSnapshot',
      showLoadingIndicator: true,
    }
  );

  return {
    data,
    isLoading,
    error,
    lastUpdated,
    refetch,
  };
}

// Nodes Hook
export interface UseNodesReturn {
  nodes: Node[];
  isLoading: boolean;
  error: Error | null;
  lastUpdated: Date | null;
  refetch: () => Promise<void>;
}

export function useNodes(speed: PollingSpeed = 'normal', enabled = true): UseNodesReturn {
  const { data, isLoading, error, lastUpdated, refetch } = usePolling<Node[]>(
    () => apiClient.listNodes(),
    speed,
    {
      enabled,
      operationName: 'listNodes',
      enableCircuitBreaker: true,
    }
  );

  return {
    nodes: data ?? [],
    isLoading,
    error,
    lastUpdated,
    refetch,
  };
}

// Node Details Hook
export function useNodeDetails(nodeId: string | null, enabled = true) {
  const fetchNodeDetails = useCallback(async (): Promise<NodeDetailsResponse | null> => {
    if (!nodeId) return null;
    return apiClient.getNodeDetails(nodeId);
  }, [nodeId]);

  return usePolling<NodeDetailsResponse | null>(
    fetchNodeDetails,
    'normal',
    {
      enabled: enabled && !!nodeId,
      operationName: 'getNodeDetails',
    }
  );
}

// Node Operations Hook
export function useNodeOperations() {
  const registerNode = useAsyncOperation<Node>(
    async (data: unknown) => apiClient.registerNode(data as RegisterNodeRequest),
    { operationName: 'registerNode' }
  );

  const pingNode = useAsyncOperation<NodePingResponse>(
    async (nodeId: unknown) => apiClient.testNodeConnection(nodeId as string),
    { operationName: 'pingNode' }
  );

  const markOffline = useAsyncOperation<void>(
    async (nodeId: unknown) => apiClient.markNodeOffline(nodeId as string),
    { operationName: 'markNodeOffline' }
  );

  const evictNode = useAsyncOperation<void>(
    async (nodeId: unknown) => apiClient.evictNode(nodeId as string),
    { operationName: 'evictNode' }
  );

  return {
    registerNode,
    pingNode,
    markOffline,
    evictNode,
  };
}

// Workers Hook
export interface UseWorkersReturn {
  workers: WorkerResponse[];
  isLoading: boolean;
  error: Error | null;
  lastUpdated: Date | null;
  refetch: () => Promise<void>;
}

export function useWorkers(
  tenantId?: string,
  nodeId?: string,
  speed: PollingSpeed = 'normal',
  enabled = true
): UseWorkersReturn {
  const fetchWorkers = useCallback(async (): Promise<WorkerResponse[]> => {
    return apiClient.listWorkers(tenantId, nodeId);
  }, [tenantId, nodeId]);

  const { data, isLoading, error, lastUpdated, refetch } = usePolling<WorkerResponse[]>(
    fetchWorkers,
    speed,
    {
      enabled,
      operationName: 'listWorkers',
      enableCircuitBreaker: true,
    }
  );

  return {
    workers: data ?? [],
    isLoading,
    error,
    lastUpdated,
    refetch,
  };
}

// Worker Details Hook
export function useWorkerDetails(workerId: string | null, enabled = true) {
  const fetchWorkerDetails = useCallback(async (): Promise<WorkerDetailsResponse | null> => {
    if (!workerId) return null;
    return apiClient.getWorkerDetails(workerId);
  }, [workerId]);

  return usePolling<WorkerDetailsResponse | null>(
    fetchWorkerDetails,
    'normal',
    {
      enabled: enabled && !!workerId,
      operationName: 'getWorkerDetails',
    }
  );
}

// Worker Logs Hook
export function useWorkerLogs(workerId: string | null, filters?: ProcessLogFilters, enabled = true) {
  const fetchLogs = useCallback(async (): Promise<ProcessLog[]> => {
    if (!workerId) return [];
    return apiClient.getProcessLogs(workerId, filters);
  }, [workerId, filters]);

  return usePolling<ProcessLog[]>(
    fetchLogs,
    'fast',
    {
      enabled: enabled && !!workerId,
      operationName: 'getWorkerLogs',
    }
  );
}

// Worker Crashes Hook
export function useWorkerCrashes(workerId: string | null, enabled = true) {
  const fetchCrashes = useCallback(async (): Promise<ProcessCrash[]> => {
    if (!workerId) return [];
    return apiClient.getProcessCrashes(workerId);
  }, [workerId]);

  return usePolling<ProcessCrash[]>(
    fetchCrashes,
    'slow',
    {
      enabled: enabled && !!workerId,
      operationName: 'getWorkerCrashes',
    }
  );
}

export function useWorkerIncidents(workerId: string | null, enabled = true, limit?: number) {
  const fetchIncidents = useCallback(async () => {
    if (!workerId) return [];
    return apiClient.getWorkerIncidents(workerId, limit);
  }, [workerId, limit]);

  return usePolling(
    fetchIncidents,
    'slow',
    {
      enabled: enabled && !!workerId,
      operationName: 'getWorkerIncidents',
    }
  );
}

export function useWorkersHealthSummary(speed: PollingSpeed = 'normal', enabled = true) {
  const fetchHealthSummary = useCallback(async () => {
    return apiClient.getWorkersHealthSummary();
  }, []);

  return usePolling(
    fetchHealthSummary,
    speed,
    {
      enabled,
      operationName: 'getWorkersHealthSummary',
    }
  );
}

// Worker Operations Hook
export function useWorkerOperations() {
  const spawnWorker = useAsyncOperation<WorkerResponse>(
    async (data: unknown) => apiClient.spawnWorker(data as SpawnWorkerRequest),
    { operationName: 'spawnWorker' }
  );

  const stopWorker = useAsyncOperation<void>(
    async (workerId: unknown, force: unknown) => apiClient.stopWorker(workerId as string, force as boolean),
    { operationName: 'stopWorker' }
  );

  const startDebugSession = useAsyncOperation<DebugSession>(
    async (workerId: unknown, config: unknown) =>
      apiClient.startDebugSession(workerId as string, config as DebugSessionConfig),
    { operationName: 'startDebugSession' }
  );

  const runTroubleshooting = useAsyncOperation<TroubleshootingResult>(
    async (workerId: unknown, step: unknown) =>
      apiClient.runTroubleshootingStep(workerId as string, step as TroubleshootingStep),
    { operationName: 'runTroubleshooting' }
  );

  return {
    spawnWorker,
    stopWorker,
    startDebugSession,
    runTroubleshooting,
  };
}

// Memory Usage Hook
export interface MemoryUsage {
  adapters: Array<{
    id: string;
    name: string;
    memory_usage_mb: number;
    state: string;
    pinned: boolean;
    category: string;
  }>;
  total_memory_mb: number;
  available_memory_mb: number;
  memory_pressure_level: 'low' | 'medium' | 'high' | 'critical';
}

export function useMemoryUsage(speed: PollingSpeed = 'normal', enabled = true) {
  return usePolling<MemoryUsage>(
    () => apiClient.getMemoryUsage() as Promise<MemoryUsage>,
    speed,
    {
      enabled,
      operationName: 'getMemoryUsage',
      enableCircuitBreaker: true,
    }
  );
}

// Memory Operations Hook
export function useMemoryOperations() {
  const evictAdapter = useAsyncOperation<{ success: boolean; message: string }>(
    async (adapterId: unknown) => apiClient.evictAdapter(adapterId as string),
    { operationName: 'evictAdapter' }
  );

  return {
    evictAdapter,
  };
}

// Computed metrics helpers
//
// IMPORTANT: Resource usage percentages (CPU, memory, disk, GPU) return null
// when unavailable instead of 0 to prevent operators from interpreting
// missing data as "0% usage" which could lead to bad capacity decisions.
// Counts (node, worker, adapter) still default to 0 as this is safe.
export function useComputedMetrics(metrics: SystemMetrics | null) {
  return useMemo(() => {
    if (!metrics) return null;

    // Resource usage percentages: null means unavailable, not 0%
    const cpuUsage = metrics.cpu_usage_percent ?? metrics.cpu_usage ?? null;
    const memoryUsage = metrics.memory_usage_percent ?? metrics.memory_usage_pct ?? metrics.memory_usage ?? null;
    const diskUsage = metrics.disk_usage_percent ?? metrics.disk_usage ?? null;
    const gpuUsage = metrics.gpu_utilization_percent ?? null;

    return {
      cpuUsage,
      memoryUsage,
      diskUsage,
      gpuUsage,
      // Counts are safe to default to 0
      nodeCount: metrics.node_count ?? 0,
      workerCount: metrics.worker_count ?? 0,
      // Memory amounts: null means unavailable
      memoryUsedGb: metrics.memory_used_gb ?? null,
      memoryTotalGb: metrics.memory_total_gb ?? null,
      gpuMemoryUsedMb: metrics.gpu_memory_used_mb ?? null,
      gpuMemoryTotalMb: metrics.gpu_memory_total_mb ?? null,
      // Network stats: null means unavailable
      networkRx: metrics.network_rx_bytes ?? metrics.network_rx ?? null,
      networkTx: metrics.network_tx_bytes ?? metrics.network_tx ?? null,
      // Counts are safe to default to 0
      adapterCount: metrics.adapter_count ?? 0,
      activeSessions: metrics.active_sessions ?? 0,
      // Performance metrics: null means unavailable
      tokensPerSecond: metrics.tokens_per_second ?? null,
      latencyP95Ms: metrics.latency_p95_ms ?? null,
      // Temperature/power: null means unavailable
      cpuTemp: metrics.cpu_temp_celsius ?? null,
      gpuTemp: metrics.gpu_temp_celsius ?? null,
      gpuPower: metrics.gpu_power_watts ?? null,
      // Disk I/O: null means unavailable
      diskReadMbps: metrics.disk_read_mbps ?? null,
      diskWriteMbps: metrics.disk_write_mbps ?? null,
      // Rates: null means unavailable
      cacheHitRate: metrics.cache_hit_rate ?? null,
      errorRate: metrics.error_rate ?? null,
    };
  }, [metrics]);
}

// Health status helper
export type HealthStatus = 'healthy' | 'warning' | 'critical' | 'unknown';

export function getHealthStatus(
  value: number | null,
  warningThreshold: number,
  criticalThreshold: number
): HealthStatus {
  // null means unavailable data - return unknown, not healthy
  if (value === null) return 'unknown';
  if (value >= criticalThreshold) return 'critical';
  if (value >= warningThreshold) return 'warning';
  return 'healthy';
}

export function useSystemHealthStatus(metrics: SystemMetrics | null): HealthStatus {
  const computed = useComputedMetrics(metrics);

  return useMemo(() => {
    if (!computed) return 'unknown';

    const cpuStatus = getHealthStatus(computed.cpuUsage, 70, 90);
    const memStatus = getHealthStatus(computed.memoryUsage, 75, 90);
    const diskStatus = getHealthStatus(computed.diskUsage, 80, 95);
    const gpuStatus = getHealthStatus(computed.gpuUsage, 80, 95);

    const statuses = [cpuStatus, memStatus, diskStatus, gpuStatus];

    // If any critical, overall is critical
    if (statuses.some(s => s === 'critical')) return 'critical';
    // If any warning, overall is warning
    if (statuses.some(s => s === 'warning')) return 'warning';
    // If all unknown, overall is unknown (don't pretend we're healthy)
    if (statuses.every(s => s === 'unknown')) return 'unknown';
    return 'healthy';
  }, [computed]);
}
