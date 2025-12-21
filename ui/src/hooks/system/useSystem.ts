// Re-export system-related hooks from useSystemMetrics
// This file provides a centralized import point for System page hooks

export {
  // System Metrics
  useSystemMetrics,
  useQualityMetrics,
  useAdapterMetrics,
  useMetricsSnapshot,
  useComputedMetrics,
  useSystemHealthStatus,
  getHealthStatus,
  type UseSystemMetricsReturn,
  type HealthStatus,

  // Nodes
  useNodes,
  useNodeDetails,
  useNodeOperations,
  type UseNodesReturn,

  // Workers
  useWorkers,
  useWorkerDetails,
  useWorkerLogs,
  useWorkerCrashes,
  useWorkerOperations,
  type UseWorkersReturn,

  // Memory
  useMemoryUsage,
  useMemoryOperations,
  type MemoryUsage,
} from './useSystemMetrics';
