/**
 * Status Display Types
 * Types for displaying various status states
 */

export type LoadingStatus = 'idle' | 'loading' | 'loaded' | 'error';

export type OperationStatus = 'pending' | 'in_progress' | 'completed' | 'failed' | 'cancelled';

export type HealthStatus = 'healthy' | 'degraded' | 'unhealthy' | 'unknown';

export type ConnectionStatus = 'connected' | 'connecting' | 'disconnected' | 'error';

export type SyncStatus = 'synced' | 'syncing' | 'out_of_sync' | 'conflict' | 'error';

export interface StatusIndicatorProps {
  status: LoadingStatus | OperationStatus | HealthStatus | ConnectionStatus | SyncStatus;
  label?: string;
  showIcon?: boolean;
  showPulse?: boolean;
  size?: 'sm' | 'md' | 'lg';
  className?: string;
}

export interface StatusBadgeProps extends StatusIndicatorProps {
  variant?: 'default' | 'outline' | 'subtle';
}

export interface ProgressIndicator {
  current: number;
  total: number;
  status: OperationStatus;
  message?: string;
  estimatedTimeRemaining?: number;
}

export interface HealthCheckResult {
  component: string;
  status: HealthStatus;
  message?: string;
  lastChecked: Date;
  metadata?: Record<string, any>;
}

export interface SystemHealth {
  overall: HealthStatus;
  components: HealthCheckResult[];
  lastUpdated: Date;
}

export interface ConnectionInfo {
  status: ConnectionStatus;
  connectedAt?: Date;
  lastPing?: Date;
  latency?: number;
  reconnectAttempts?: number;
}

export interface SyncInfo {
  status: SyncStatus;
  lastSynced?: Date;
  itemsSynced?: number;
  itemsPending?: number;
  conflicts?: number;
  error?: string;
}

export interface WorkflowStepStatus {
  id: string;
  label: string;
  status: OperationStatus;
  startedAt?: Date;
  completedAt?: Date;
  error?: string;
  metadata?: Record<string, any>;
}

export interface WorkflowStatus {
  id: string;
  name: string;
  overallStatus: OperationStatus;
  steps: WorkflowStepStatus[];
  startedAt: Date;
  completedAt?: Date;
  progress?: number;
}
