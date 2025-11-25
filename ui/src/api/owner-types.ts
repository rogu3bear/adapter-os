/**
 * Owner Home API Types
 *
 * Type definitions for owner-level administrative endpoints including
 * CLI execution and AI-powered chat assistance.
 */

// CLI Run types
export interface CliRunRequest {
  command: string;
  session_id?: string;
}

export interface CliRunResponse {
  stdout: string;
  stderr: string;
  exit_code: number;
  duration_ms: number;
}

// Owner Chat types
export interface OwnerChatMessage {
  role: 'user' | 'assistant';
  content: string;
}

export interface OwnerChatContext {
  route?: string;
  metrics_snapshot?: Record<string, unknown>;
  user_role?: string;
}

export interface OwnerChatRequest {
  messages: OwnerChatMessage[];
  context?: OwnerChatContext;
}

export interface OwnerChatResponse {
  response: string;
  suggested_cli?: string;
  relevant_links: string[];
}

// System Overview types (matching Rust backend: crates/adapteros-server-api/src/handlers/system_overview.rs)

/**
 * Service health status enum matching Rust ServiceHealthStatus
 */
export type ServiceHealthStatus = 'healthy' | 'degraded' | 'unhealthy' | 'unknown';

/**
 * Service status matching Rust ServiceStatus struct
 */
export interface ServiceStatus {
  name: string;
  status: ServiceHealthStatus;
  message?: string;
  last_check: number;
}

/**
 * Load average information matching Rust LoadAverageInfo struct
 */
export interface LoadAverageInfo {
  load_1min: number;
  load_5min: number;
  load_15min: number;
}

/**
 * Resource usage information matching Rust ResourceUsageInfo struct
 */
export interface ResourceUsageInfo {
  cpu_usage_percent: number;
  memory_usage_percent: number;
  disk_usage_percent: number;
  network_rx_mbps: number;
  network_tx_mbps: number;
  gpu_utilization_percent?: number;
  gpu_memory_used_gb?: number;
  gpu_memory_total_gb?: number;
}

/**
 * System overview response matching Rust SystemOverviewResponse struct
 * Source: crates/adapteros-server-api/src/handlers/system_overview.rs L19-L33
 */
export interface SystemOverview {
  schema_version: string;
  uptime_seconds: number;
  process_count: number;
  load_average: LoadAverageInfo;
  resource_usage: ResourceUsageInfo;
  services: ServiceStatus[];
  active_sessions: number;
  active_workers: number;
  adapter_count: number;
  timestamp: number;
}
