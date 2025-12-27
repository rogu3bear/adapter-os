/**
 * Role-based dashboard configuration
 *
 * Defines widget layouts and quick actions for each RBAC role.
 * Based on permission matrix from AGENTS.md and routes from config/routes.ts.
 *
 * @module roleConfigs
 */

export interface WidgetConfig {
  id: string;
  title: string;
  description: string;
  component: string;
  defaultSize?: 'small' | 'medium' | 'large';
  position?: { row: number; col: number };
  permissions?: string[];
  ariaLabel?: string;
}

export interface QuickAction {
  id: string;
  label: string;
  variant: 'primary' | 'secondary' | 'danger';
  action: string;
  icon?: string;
  permissions?: string[];
  description?: string;
  ariaLabel?: string;
}

export interface RoleDashboardConfig {
  role: string;
  title: string;
  displayName: string;
  description: string;
  defaultRoute: string;
  widgets: WidgetConfig[];
  quickActions: QuickAction[];
}

/**
 * Admin Dashboard Configuration
 * Full system management capabilities with all administrative privileges
 */
export const adminConfig: RoleDashboardConfig = {
  role: 'admin',
  title: 'Admin Dashboard',
  displayName: 'Administrator',
  description: 'Full system access with all administrative privileges',
  defaultRoute: '/dashboard',
  widgets: [
    {
      id: 'system-overview',
      title: 'System Overview',
      description: 'System metrics and health status',
      component: 'SystemOverviewWidget',
      defaultSize: 'large',
      position: { row: 0, col: 0 },
      ariaLabel: 'System overview widget showing metrics and health status',
    },
    {
      id: 'tenant-summary',
      title: 'Tenant Summary',
      description: 'Multi-tenant operations and policies',
      component: 'TenantSummaryWidget',
      defaultSize: 'medium',
      position: { row: 0, col: 2 },
      permissions: ['TenantManage'],
      ariaLabel: 'Tenant summary widget showing multi-tenant operations and policies',
    },
    {
      id: 'adapter-stats',
      title: 'Adapter Statistics',
      description: 'Adapter registry and lifecycle distribution',
      component: 'AdapterStatsWidget',
      defaultSize: 'medium',
      position: { row: 1, col: 0 },
      permissions: ['AdapterList'],
      ariaLabel: 'Adapter statistics widget showing registry and lifecycle distribution',
    },
    {
      id: 'training-jobs',
      title: 'Active Training Jobs',
      description: 'Training pipeline status and progress',
      component: 'TrainingJobsWidget',
      defaultSize: 'medium',
      position: { row: 1, col: 1 },
      permissions: ['TrainingView'],
    },
    {
      id: 'policy-compliance',
      title: 'Policy Compliance',
      description: '23 canonical policy enforcement status',
      component: 'PolicyComplianceWidget',
      defaultSize: 'medium',
      position: { row: 1, col: 2 },
      permissions: ['PolicyView'],
    },
    {
      id: 'recent-activity',
      title: 'Recent Activity',
      description: 'System-wide activity feed',
      component: 'RecentActivityWidget',
      defaultSize: 'large',
      position: { row: 2, col: 0 },
      permissions: ['ActivityView'],
    },
    {
      id: 'audit-alerts',
      title: 'Audit Alerts',
      description: 'Critical audit events requiring attention',
      component: 'AuditAlertsWidget',
      defaultSize: 'small',
      position: { row: 2, col: 2 },
      permissions: ['AuditView'],
    },
    {
      id: 'node-health',
      title: 'Node Health',
      description: 'Cluster node status and federation',
      component: 'NodeHealthWidget',
      defaultSize: 'medium',
      position: { row: 3, col: 0 },
      permissions: ['NodeView'],
    },
    {
      id: 'resource-usage',
      title: 'Resource Usage',
      description: 'Memory, GPU, and compute utilization',
      component: 'ResourceUsageWidget',
      defaultSize: 'medium',
      position: { row: 3, col: 1 },
      permissions: ['MetricsView'],
    },
  ],
  quickActions: [
    {
      id: 'create-tenant',
      label: 'Create Tenant',
      icon: 'Building',
      variant: 'secondary',
      action: 'navigate:/admin/tenants',
      permissions: ['TenantManage'],
      description: 'Create a new tenant with isolation policies',
    },
    {
      id: 'register-adapter',
      label: 'Register Adapter',
      icon: 'Box',
      variant: 'primary',
      action: 'navigate:/adapters/new',
      permissions: ['AdapterRegister'],
      description: 'Register a new adapter to the system',
    },
    {
      id: 'start-training',
      label: 'Start Training',
      icon: 'Zap',
      variant: 'secondary',
      action: 'navigate:/training',
      permissions: ['TrainingStart'],
      description: 'Launch a new training job',
    },
    {
      id: 'manage-policies',
      label: 'Manage Policies',
      icon: 'Shield',
      variant: 'secondary',
      action: 'navigate:/security/policies',
      permissions: ['PolicyApply'],
      description: 'Apply and sign policy packs',
    },
    {
      id: 'view-audit-logs',
      label: 'Audit Logs',
      icon: 'FileText',
      variant: 'secondary',
      action: 'navigate:/security/audit',
      permissions: ['AuditView'],
      description: 'View comprehensive audit trail',
    },
    {
      id: 'manage-stacks',
      label: 'Adapter Stacks',
      icon: 'Layers',
      variant: 'secondary',
      action: 'navigate:/admin/stacks',
      permissions: ['AdapterStackManage'],
      description: 'Create and configure adapter stacks',
    },
    {
      id: 'system-settings',
      label: 'System Settings',
      icon: 'Settings',
      variant: 'secondary',
      action: 'navigate:/admin/settings',
      permissions: ['TenantManage'],
      description: 'Configure global system settings',
    },
    {
      id: 'manage-nodes',
      label: 'Manage Nodes',
      icon: 'Server',
      variant: 'secondary',
      action: 'navigate:/system/nodes',
      permissions: ['NodeManage'],
      description: 'Register and manage cluster nodes',
    },
  ],
};

/**
 * Operator Dashboard Configuration
 * Runtime operations and workflow management
 */
export const operatorConfig: RoleDashboardConfig = {
  role: 'operator',
  title: 'Operator Dashboard',
  displayName: 'Operator',
  description: 'Runtime operations and workflow management',
  defaultRoute: '/management',
  widgets: [
    {
      id: 'active-adapters',
      title: 'Active AI Modules',
      description: 'Currently loaded AI models and their states',
      component: 'ActiveAdaptersWidget',
      defaultSize: 'large',
      position: { row: 0, col: 0 },
      permissions: ['AdapterList'],
    },
    {
      id: 'training-queue',
      title: 'Learning Tasks',
      description: 'Active and queued learning jobs',
      component: 'TrainingQueueWidget',
      defaultSize: 'medium',
      position: { row: 0, col: 2 },
      permissions: ['TrainingView'],
    },
    {
      id: 'inference-stats',
      title: 'Inference Statistics',
      description: 'Throughput, latency, and request volume',
      component: 'InferenceStatsWidget',
      defaultSize: 'medium',
      position: { row: 1, col: 0 },
      permissions: ['InferenceExecute'],
    },
    {
      id: 'adapter-stacks',
      title: 'AI Module Stacks',
      description: 'Active and configured module stacks',
      component: 'AdapterStacksWidget',
      defaultSize: 'medium',
      position: { row: 1, col: 1 },
      permissions: ['AdapterStackView'],
    },
    {
      id: 'worker-status',
      title: 'Worker Status',
      description: 'Worker processes and health metrics',
      component: 'WorkerStatusWidget',
      defaultSize: 'medium',
      position: { row: 1, col: 2 },
      permissions: ['WorkerView'],
    },
    {
      id: 'recent-operations',
      title: 'Recent Operations',
      description: 'Recent adapter and training operations',
      component: 'RecentOperationsWidget',
      defaultSize: 'large',
      position: { row: 2, col: 0 },
      permissions: ['ActivityView'],
    },
    {
      id: 'dataset-library',
      title: 'Dataset Library',
      description: 'Available training datasets',
      component: 'DatasetLibraryWidget',
      defaultSize: 'medium',
      position: { row: 3, col: 0 },
      permissions: ['DatasetView'],
    },
    {
      id: 'code-repositories',
      title: 'Code Repositories',
      description: 'Registered code intelligence repositories',
      component: 'CodeRepositoriesWidget',
      defaultSize: 'medium',
      position: { row: 3, col: 1 },
      permissions: ['CodeView'],
    },
  ],
  quickActions: [
    {
      id: 'load-adapter',
      label: 'Load AI Module',
      icon: 'Upload',
      variant: 'secondary',
      action: 'modal:load-adapter',
      permissions: ['AdapterLoad'],
      description: 'Load an AI model into memory',
    },
    {
      id: 'start-inference',
      label: 'Run Action',
      icon: 'Play',
      variant: 'primary',
      action: 'navigate:/inference',
      permissions: ['InferenceExecute'],
      description: 'Execute a quick action or test prompt',
    },
    {
      id: 'start-training',
      label: 'Start Training',
      icon: 'Zap',
      variant: 'secondary',
      action: 'navigate:/training',
      permissions: ['TrainingStart'],
      description: 'Launch a new training job',
    },
    {
      id: 'upload-dataset',
      label: 'Upload Dataset',
      icon: 'Upload',
      variant: 'secondary',
      action: 'navigate:/training/datasets',
      permissions: ['DatasetUpload'],
      description: 'Upload a dataset for learning tasks',
    },
    {
      id: 'manage-stack',
      label: 'Manage Stacks',
      icon: 'Layers',
      variant: 'secondary',
      action: 'navigate:/admin/stacks',
      permissions: ['AdapterStackManage'],
      description: 'Create and modify AI module stacks',
    },
    {
      id: 'spawn-worker',
      label: 'Spawn Worker',
      icon: 'Cpu',
      variant: 'secondary',
      action: 'modal:spawn-worker',
      permissions: ['WorkerSpawn'],
      description: 'Spawn a new worker process',
    },
    {
      id: 'scan-code',
      label: 'Open Telemetry Viewer',
      icon: 'Eye',
      variant: 'secondary',
      action: 'navigate:/telemetry/viewer',
      permissions: ['MetricsView'],
      description: 'Inspect per-session routing and tokens',
    },
    {
      id: 'chat-interface',
      label: 'Chat Interface',
      icon: 'MessageSquare',
      variant: 'secondary',
      action: 'navigate:/chat',
      permissions: ['InferenceExecute'],
      description: 'Interactive chat with adapters',
    },
  ],
};

/**
 * SRE Dashboard Configuration
 * Infrastructure monitoring and troubleshooting
 */
export const sreConfig: RoleDashboardConfig = {
  role: 'sre',
  title: 'SRE Dashboard',
  displayName: 'Site Reliability Engineer',
  description: 'Infrastructure monitoring and troubleshooting',
  defaultRoute: '/metrics',
  widgets: [
    {
      id: 'system-health',
      title: 'System Health',
      description: 'Overall system status, alerts, and availability',
      component: 'SystemHealthWidget',
      defaultSize: 'large',
      position: { row: 0, col: 0 },
      permissions: ['MetricsView'],
    },
    {
      id: 'node-metrics',
      title: 'Node Metrics',
      description: 'Cluster node health and resource utilization',
      component: 'NodeMetricsWidget',
      defaultSize: 'medium',
      position: { row: 0, col: 2 },
      permissions: ['NodeView'],
    },
    {
      id: 'memory-usage',
      title: 'Memory Usage',
      description: 'UMA memory breakdown and adapter memory',
      component: 'MemoryUsageWidget',
      defaultSize: 'medium',
      position: { row: 1, col: 0 },
      permissions: ['MetricsView'],
    },
    {
      id: 'performance-metrics',
      title: 'Performance Metrics',
      description: 'Latency, throughput, and inference performance',
      component: 'PerformanceMetricsWidget',
      defaultSize: 'medium',
      position: { row: 1, col: 1 },
      permissions: ['MetricsView'],
    },
    {
      id: 'alert-summary',
      title: 'Alert Summary',
      description: 'Active monitoring alerts and incidents',
      component: 'AlertSummaryWidget',
      defaultSize: 'medium',
      position: { row: 1, col: 2 },
      permissions: ['MonitoringManage'],
    },
    {
      id: 'adapter-health',
      title: 'Adapter Health',
      description: 'Adapter load status and error rates',
      component: 'AdapterHealthWidget',
      defaultSize: 'large',
      position: { row: 2, col: 0 },
      permissions: ['AdapterView'],
    },
    {
      id: 'worker-diagnostics',
      title: 'Worker Diagnostics',
      description: 'Worker process health and crash reports',
      component: 'WorkerDiagnosticsWidget',
      defaultSize: 'medium',
      position: { row: 3, col: 0 },
      permissions: ['WorkerView'],
    },
    {
      id: 'telemetry-events',
      title: 'Telemetry Events',
      description: 'Recent system telemetry and events',
      component: 'TelemetryEventsWidget',
      defaultSize: 'medium',
      position: { row: 3, col: 1 },
      permissions: ['TelemetryView'],
    },
  ],
  quickActions: [
    {
      id: 'view-metrics',
      label: 'System Metrics',
      icon: 'BarChart3',
      variant: 'primary',
      action: 'navigate:/metrics',
      permissions: ['MetricsView'],
      description: 'View detailed system metrics',
    },
    {
      id: 'node-diagnostics',
      label: 'Node Diagnostics',
      icon: 'Server',
      variant: 'secondary',
      action: 'navigate:/system/nodes',
      permissions: ['NodeView'],
      description: 'Diagnose node issues',
    },
    {
      id: 'memory-analysis',
      label: 'Memory Analysis',
      icon: 'MemoryStick',
      variant: 'secondary',
      action: 'navigate:/system/memory',
      permissions: ['MetricsView'],
      description: 'Analyze memory usage and pressure',
    },
    {
      id: 'test-inference',
      label: 'Test Inference',
      icon: 'Play',
      variant: 'secondary',
      action: 'navigate:/inference',
      permissions: ['InferenceExecute'],
      description: 'Test inference for troubleshooting',
    },
    {
      id: 'view-telemetry',
      label: 'Event History',
      icon: 'Eye',
      variant: 'secondary',
      action: 'navigate:/telemetry',
      permissions: ['TelemetryView'],
      description: 'View telemetry events',
    },
    {
      id: 'replay-session',
      label: 'Replay Session',
      icon: 'RotateCcw',
      variant: 'secondary',
      action: 'navigate:/replay',
      permissions: ['ReplayManage'],
      description: 'Create replay session for debugging',
    },
    {
      id: 'monitoring-rules',
      label: 'Monitoring Rules',
      icon: 'Activity',
      variant: 'secondary',
      action: 'navigate:/metrics',
      permissions: ['MonitoringManage'],
      description: 'Manage monitoring rules and alerts',
    },
    {
      id: 'view-logs',
      label: 'Worker Logs',
      icon: 'FileText',
      variant: 'secondary',
      action: 'modal:worker-logs',
      permissions: ['WorkerView'],
      description: 'View worker logs',
    },
  ],
};

/**
 * Compliance Dashboard Configuration
 * Audit and compliance verification
 */
export const complianceConfig: RoleDashboardConfig = {
  role: 'compliance',
  title: 'Compliance Dashboard',
  displayName: 'Compliance Officer',
  description: 'Audit and compliance verification',
  defaultRoute: '/security/compliance',
  widgets: [
    {
      id: 'audit-summary',
      title: 'Audit Summary',
      description: 'Comprehensive audit event summary',
      component: 'AuditSummaryWidget',
      defaultSize: 'large',
      position: { row: 0, col: 0 },
      permissions: ['AuditView'],
    },
    {
      id: 'policy-status',
      title: 'Policy Status',
      description: '23 canonical policy enforcement status',
      component: 'PolicyStatusWidget',
      defaultSize: 'medium',
      position: { row: 0, col: 2 },
      permissions: ['PolicyView'],
    },
    {
      id: 'compliance-score',
      title: 'Compliance Score',
      description: 'Overall system compliance metrics',
      component: 'ComplianceScoreWidget',
      defaultSize: 'medium',
      position: { row: 1, col: 0 },
      permissions: ['PolicyValidate'],
    },
    {
      id: 'policy-violations',
      title: 'Policy Violations',
      description: 'Recent policy violations and resolutions',
      component: 'PolicyViolationsWidget',
      defaultSize: 'medium',
      position: { row: 1, col: 1 },
      permissions: ['AuditView'],
    },
    {
      id: 'dataset-validation',
      title: 'Dataset Validation',
      description: 'Dataset compliance and validation status',
      component: 'DatasetValidationWidget',
      defaultSize: 'medium',
      position: { row: 1, col: 2 },
      permissions: ['DatasetValidate'],
    },
    {
      id: 'audit-trail',
      title: 'Recent Audit Events',
      description: 'Immutable audit trail with signatures',
      component: 'AuditTrailWidget',
      defaultSize: 'large',
      position: { row: 2, col: 0 },
      permissions: ['AuditView'],
    },
    {
      id: 'adapter-compliance',
      title: 'Adapter Compliance',
      description: 'Adapter policy adherence and ACL status',
      component: 'AdapterComplianceWidget',
      defaultSize: 'medium',
      position: { row: 3, col: 0 },
      permissions: ['AdapterView'],
    },
    {
      id: 'training-compliance',
      title: 'Training Compliance',
      description: 'Training job policy compliance',
      component: 'TrainingComplianceWidget',
      defaultSize: 'medium',
      position: { row: 3, col: 1 },
      permissions: ['TrainingView'],
    },
  ],
  quickActions: [
    {
      id: 'view-audit-logs',
      label: 'Audit Logs',
      icon: 'FileText',
      variant: 'primary',
      action: 'navigate:/security/audit',
      permissions: ['AuditView'],
      description: 'View comprehensive audit logs',
    },
    {
      id: 'policy-compliance',
      label: 'Policy Compliance',
      icon: 'Shield',
      variant: 'secondary',
      action: 'navigate:/security/policies',
      permissions: ['PolicyView'],
      description: 'Review policy compliance status',
    },
    {
      id: 'validate-policy',
      label: 'Validate Policy',
      icon: 'CheckCircle',
      variant: 'secondary',
      action: 'modal:validate-policy',
      permissions: ['PolicyValidate'],
      description: 'Validate policy compliance',
    },
    {
      id: 'compliance-report',
      label: 'Compliance Report',
      icon: 'BarChart3',
      variant: 'secondary',
      action: 'navigate:/security/compliance',
      permissions: ['AuditView'],
      description: 'Generate compliance report',
    },
    {
      id: 'validate-dataset',
      label: 'Validate Dataset',
      icon: 'Database',
      variant: 'secondary',
      action: 'modal:validate-dataset',
      permissions: ['DatasetValidate'],
      description: 'Validate dataset compliance',
    },
    {
      id: 'verify-replay',
      label: 'Verify Replay',
      icon: 'RotateCcw',
      variant: 'secondary',
      action: 'navigate:/replay',
      permissions: ['ReplayManage'],
      description: 'Verify replay session for compliance',
    },
    {
      id: 'federation-audit',
      label: 'Federation Audit',
      icon: 'Network',
      variant: 'secondary',
      action: 'modal:federation-audit',
      permissions: ['FederationView'],
      description: 'Audit federation operations',
    },
    {
      id: 'telemetry-review',
      label: 'Telemetry Review',
      icon: 'Eye',
      variant: 'secondary',
      action: 'navigate:/telemetry',
      permissions: ['TelemetryView'],
      description: 'Review telemetry events',
    },
  ],
};

/**
 * Viewer Dashboard Configuration
 * Read-only access to system information
 */
export const viewerConfig: RoleDashboardConfig = {
  role: 'viewer',
  title: 'Dashboard',
  displayName: 'Viewer',
  description: 'Read-only access to system information',
  defaultRoute: '/dashboard',
  widgets: [
    {
      id: 'system-status',
      title: 'System Status',
      description: 'Current system health and operational status',
      component: 'SystemStatusWidget',
      defaultSize: 'large',
      position: { row: 0, col: 0 },
      permissions: ['MetricsView'],
    },
    {
      id: 'adapter-list',
      title: 'Adapters',
      description: 'Registered adapters and their status',
      component: 'AdapterListWidget',
      defaultSize: 'medium',
      position: { row: 0, col: 2 },
      permissions: ['AdapterList'],
    },
    {
      id: 'training-status',
      title: 'Training Status',
      description: 'Training job progress and history',
      component: 'TrainingStatusWidget',
      defaultSize: 'medium',
      position: { row: 1, col: 0 },
      permissions: ['TrainingView'],
    },
    {
      id: 'policy-list',
      title: 'Policies',
      description: 'Active policy enforcement status',
      component: 'PolicyListWidget',
      defaultSize: 'medium',
      position: { row: 1, col: 1 },
      permissions: ['PolicyView'],
    },
    {
      id: 'node-list',
      title: 'Nodes',
      description: 'Cluster node availability',
      component: 'NodeListWidget',
      defaultSize: 'medium',
      position: { row: 1, col: 2 },
      permissions: ['NodeView'],
    },
    {
      id: 'activity-feed',
      title: 'Activity Feed',
      description: 'Recent system activity and events',
      component: 'ActivityFeedWidget',
      defaultSize: 'large',
      position: { row: 2, col: 0 },
      permissions: ['ActivityView'],
    },
  ],
  quickActions: [
    {
      id: 'view-dashboard',
      label: 'Dashboard',
      icon: 'LayoutDashboard',
      variant: 'secondary',
      action: 'navigate:/dashboard',
      description: 'View system dashboard',
    },
    {
      id: 'view-adapters',
      label: 'View Adapters',
      icon: 'Box',
      variant: 'secondary',
      action: 'navigate:/adapters',
      permissions: ['AdapterView'],
      description: 'Browse adapter registry',
    },
    {
      id: 'view-training',
      label: 'View Training',
      icon: 'Zap',
      variant: 'secondary',
      action: 'navigate:/training',
      permissions: ['TrainingView'],
      description: 'View training jobs',
    },
    {
      id: 'view-metrics',
      label: 'View Metrics',
      icon: 'BarChart3',
      variant: 'secondary',
      action: 'navigate:/metrics',
      permissions: ['MetricsView'],
      description: 'View system metrics',
    },
    {
      id: 'view-policies',
      label: 'View Policies',
      icon: 'Shield',
      variant: 'secondary',
      action: 'navigate:/security/policies',
      permissions: ['PolicyView'],
      description: 'View policy configurations',
    },
    {
      id: 'view-nodes',
      label: 'View Nodes',
      icon: 'Server',
      variant: 'secondary',
      action: 'navigate:/system/nodes',
      permissions: ['NodeView'],
      description: 'View cluster nodes',
    },
  ],
};

/**
 * Role configuration registry
 */
export const roleConfigs: Record<string, RoleDashboardConfig> = {
  admin: adminConfig,
  operator: operatorConfig,
  sre: sreConfig,
  compliance: complianceConfig,
  viewer: viewerConfig,
};

/**
 * Get role configuration by role name
 * Falls back to viewer config if role not found
 */
export function getRoleConfig(role: string): RoleDashboardConfig {
  return roleConfigs[role.toLowerCase()] || roleConfigs.viewer;
}

/**
 * Get widgets for a specific role, filtered by user permissions
 */
export function getWidgetsForRole(role: string, userPermissions: string[] = []): WidgetConfig[] {
  const config = getRoleConfig(role);

  // Filter widgets based on permissions
  return config.widgets.filter(widget => {
    if (!widget.permissions || widget.permissions.length === 0) {
      return true; // No permission required
    }

    // Check if user has at least one of the required permissions
    return widget.permissions.some(perm =>
      userPermissions.includes(perm) || userPermissions.includes('*')
    );
  });
}

/**
 * Get quick actions for a specific role, filtered by user permissions
 */
export function getQuickActionsForRole(role: string, userPermissions: string[] = []): QuickAction[] {
  const config = getRoleConfig(role);

  // Filter quick actions based on permissions
  return config.quickActions.filter(action => {
    if (!action.permissions || action.permissions.length === 0) {
      return true; // No permission required
    }

    // Check if user has at least one of the required permissions
    return action.permissions.some(perm =>
      userPermissions.includes(perm) || userPermissions.includes('*')
    );
  });
}

/**
 * Get default route for a specific role
 */
export function getDefaultRouteForRole(role: string): string {
  const config = getRoleConfig(role);
  return config.defaultRoute;
}

/**
 * Check if a user has permission for a specific action
 */
export function hasPermission(userPermissions: string[], requiredPermissions?: string[]): boolean {
  if (!requiredPermissions || requiredPermissions.length === 0) {
    return true;
  }
  return requiredPermissions.some(permission =>
    userPermissions.includes(permission) || userPermissions.includes('*')
  );
}

/**
 * Check if a role has access to a specific widget
 */
export function hasWidgetAccess(
  role: string,
  widgetId: string,
  userPermissions: string[] = []
): boolean {
  const widgets = getWidgetsForRole(role, userPermissions);
  return widgets.some(w => w.id === widgetId);
}

/**
 * Check if a role has access to a specific quick action
 */
export function hasQuickActionAccess(
  role: string,
  actionId: string,
  userPermissions: string[] = []
): boolean {
  const actions = getQuickActionsForRole(role, userPermissions);
  return actions.some(a => a.id === actionId);
}

/**
 * Get all available roles
 */
export function getAvailableRoles(): string[] {
  return Object.keys(roleConfigs);
}

/**
 * Get role display name
 */
export function getRoleDisplayName(role: string): string {
  const config = getRoleConfig(role);
  return config.displayName;
}

/**
 * Get role description
 */
export function getRoleDescription(role: string): string {
  const config = getRoleConfig(role);
  return config.description;
}
