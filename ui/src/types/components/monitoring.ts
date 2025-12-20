/**
 * Component prop types for monitoring and dashboard components
 *
 * These types define the interfaces for components that display system metrics,
 * dashboards, alerts, and resource monitoring.
 */

import type {
  User,
  TrainingJob,
  DatasetValidationStatus,
  AdapterStack,
  Adapter,
} from '@/api/types';

/**
 * Metric data point for charts
 */
export interface MetricData {
  /** Timestamp */
  time: string;
  /** Metric value */
  value: number;
  /** Optional label */
  label?: string;
}

/**
 * Props for MetricsChart component
 * Line chart for displaying metrics over time
 */
export interface MetricsChartProps {
  /** Metric data points */
  data: MetricData[];
  /** Chart title */
  title?: string;
  /** Y-axis label */
  yAxisLabel?: string;
  /** Line color */
  color?: string;
  /** Chart height in pixels */
  height?: number;
}

/**
 * Resource metrics structure
 */
export interface ResourceMetrics {
  /** Timestamp of the metrics */
  timestamp: string;
  /** CPU metrics */
  cpu: {
    /** CPU usage percentage (0-100) */
    usage: number;
    /** Number of CPU cores */
    cores: number;
    /** CPU temperature in Celsius (optional) */
    temperature?: number;
  };
  /** Memory metrics */
  memory: {
    /** Used memory in bytes */
    used: number;
    /** Total memory in bytes */
    total: number;
    /** Memory usage percentage (0-100) */
    usage_percent: number;
  };
  /** GPU metrics */
  gpu: {
    /** GPU utilization percentage (0-100) */
    utilization: number;
    /** GPU memory used in bytes */
    memory_used: number;
    /** GPU memory total in bytes */
    memory_total: number;
    /** GPU temperature in Celsius (optional) */
    temperature?: number;
    /** GPU power draw in watts (optional) */
    power_draw?: number;
  };
  /** Disk metrics */
  disk: {
    /** Disk space used in bytes */
    used: number;
    /** Total disk space in bytes */
    total: number;
    /** Disk usage percentage (0-100) */
    usage_percent: number;
    /** Disk I/O read bytes per second */
    io_read: number;
    /** Disk I/O write bytes per second */
    io_write: number;
  };
  /** Network metrics */
  network: {
    /** Bytes received */
    bytes_in: number;
    /** Bytes sent */
    bytes_out: number;
    /** Packets received */
    packets_in: number;
    /** Packets sent */
    packets_out: number;
  };
  /** Training-specific metrics (optional) */
  training?: {
    /** Tokens processed per second */
    tokens_per_second: number;
    /** Current loss value */
    loss: number;
    /** Current learning rate */
    learning_rate: number;
    /** Current epoch number */
    current_epoch: number;
    /** Total number of epochs */
    total_epochs: number;
  };
}

/**
 * Node information
 */
export interface NodeInfo {
  /** Node ID */
  id: string;
  /** Node name */
  name: string;
  /** Node status */
  status: 'online' | 'offline' | 'degraded';
  /** Node type */
  type: 'worker' | 'control' | 'storage';
  /** Node capabilities */
  capabilities?: string[];
  /** Last heartbeat timestamp */
  lastHeartbeat?: string;
}

/**
 * Props for ResourceMonitor component
 * Real-time resource monitoring for jobs and nodes
 */
export interface ResourceMonitorProps {
  /** Training job ID to monitor (optional) */
  jobId?: string;
  /** Node ID to monitor (optional) */
  nodeId?: string;
}

/**
 * Props for Dashboard component
 * Main dashboard interface
 */
export interface DashboardProps {
  /** Current user */
  user?: User;
  /** Selected tenant ID */
  selectedTenant?: string;
  /** Callback when navigating to a different tab */
  onNavigate?: (tab: string) => void;
}

/**
 * Props for DashboardLayout component
 * Layout wrapper for dashboard pages
 */
export interface DashboardLayoutProps {
  /** Child components */
  children: React.ReactNode;
  /** Dashboard title */
  title: string;
  /** Quick action buttons */
  quickActions?: React.ReactNode;
}

/**
 * Metric card data structure
 */
export interface MetricCardData {
  /** Metric label */
  label: string;
  /** Current value */
  value: number | string;
  /** Unit of measurement */
  unit?: string;
  /** Change from previous period */
  change?: number;
  /** Trend direction */
  trend?: 'up' | 'down' | 'stable';
  /** Severity level */
  severity?: 'normal' | 'warning' | 'critical';
  /** Optional icon */
  icon?: React.ComponentType<{ className?: string }>;
}

/**
 * Props for MetricsCard component
 * Card displaying a single metric with trend
 */
export interface MetricsCardProps {
  /** Metric data */
  metric: MetricCardData;
  /** Whether to show trend indicator */
  showTrend?: boolean;
  /** Optional click handler */
  onClick?: () => void;
  /** Optional CSS class name */
  className?: string;
}

/**
 * Alert data structure
 */
export interface AlertData {
  /** Alert ID */
  id: string;
  /** Alert title */
  title: string;
  /** Alert message */
  message: string;
  /** Alert severity */
  severity: 'info' | 'warning' | 'error' | 'critical';
  /** Alert timestamp */
  timestamp: string;
  /** Whether the alert has been acknowledged */
  acknowledged?: boolean;
  /** Source of the alert */
  source?: string;
  /** Related resource ID */
  resourceId?: string;
}

/**
 * Props for AlertList component
 * List of system alerts
 */
export interface AlertListProps {
  /** Alerts to display */
  alerts: AlertData[];
  /** Callback when alert is acknowledged */
  onAcknowledge?: (alertId: string) => void;
  /** Callback when alert is dismissed */
  onDismiss?: (alertId: string) => void;
  /** Maximum number of alerts to show */
  maxAlerts?: number;
  /** Optional CSS class name */
  className?: string;
}

/**
 * Props for DashboardWidgetFrame component
 * Wrapper for dashboard widgets
 */
export interface DashboardWidgetFrameProps {
  /** Widget title */
  title: string;
  /** Widget description */
  description?: string;
  /** Widget content */
  children: React.ReactNode;
  /** Widget actions */
  actions?: React.ReactNode;
  /** Whether the widget is loading */
  isLoading?: boolean;
  /** Error message */
  error?: string;
  /** Optional CSS class name */
  className?: string;
}

/**
 * Props for DashboardSettings component
 * Dashboard configuration interface
 */
export interface DashboardSettingsProps {
  /** Current dashboard configuration */
  config: {
    /** Refresh interval in seconds */
    refreshInterval?: number;
    /** Visible widgets */
    visibleWidgets?: string[];
    /** Widget layout */
    layout?: Record<string, unknown>;
  };
  /** Callback when configuration changes */
  onChange: (config: DashboardSettingsProps['config']) => void;
}

/**
 * Props for role-specific dashboards
 */
export interface RoleDashboardProps {
  /** Current user */
  user: User;
  /** Selected tenant ID */
  tenantId?: string;
}

/**
 * Props for SREDashboard component
 * SRE-focused dashboard
 */
export interface SREDashboardProps extends RoleDashboardProps {
  /** Whether to show advanced metrics */
  showAdvancedMetrics?: boolean;
}

/**
 * Props for OperatorDashboard component
 * Operator-focused dashboard
 */
export interface OperatorDashboardProps extends RoleDashboardProps {
  /** Whether to show training metrics */
  showTrainingMetrics?: boolean;
}

/**
 * Props for ComplianceDashboard component
 * Compliance-focused dashboard
 */
export interface ComplianceDashboardProps extends RoleDashboardProps {
  /** Whether to show policy violations */
  showPolicyViolations?: boolean;
}

/**
 * Props for ITAdminDashboard component
 * IT admin dashboard
 */
export interface ITAdminDashboardProps {
  /** Current user */
  user: User;
  /** Selected tenant */
  selectedTenant: string;
}

/**
 * System health status
 */
export interface SystemHealthStatus {
  /** Overall health status */
  status: 'healthy' | 'degraded' | 'critical';
  /** Component statuses */
  components: {
    /** Component name */
    name: string;
    /** Component status */
    status: 'healthy' | 'degraded' | 'critical';
    /** Status message */
    message?: string;
  }[];
  /** Last update timestamp */
  lastUpdate: string;
}

/**
 * Props for HealthStatus component
 * System health status display
 */
export interface HealthStatusProps {
  /** Health status data */
  status: SystemHealthStatus;
  /** Whether to show detailed view */
  detailed?: boolean;
  /** Optional CSS class name */
  className?: string;
}

/**
 * Props for RealtimeMetrics component
 * Real-time metrics streaming display
 */
export interface RealtimeMetricsProps {
  /** Metrics source */
  source: 'system' | 'training' | 'inference';
  /** Update interval in milliseconds */
  updateInterval?: number;
  /** Maximum data points to keep */
  maxDataPoints?: number;
  /** Optional CSS class name */
  className?: string;
}

/**
 * Props for TelemetryViewer component
 * Telemetry data visualization
 */
export interface TelemetryViewerProps {
  /** Telemetry session ID */
  sessionId?: string;
  /** Time range for telemetry */
  timeRange?: {
    start: string;
    end: string;
  };
  /** Optional CSS class name */
  className?: string;
}
