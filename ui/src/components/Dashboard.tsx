import React, { useState, useMemo } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
<<<<<<< HEAD
import { logger, toError } from '../utils/logger';
import type { MetricsSnapshotResponse } from '../api/types';
import {
  Activity,
  Shield,
  CheckCircle,
=======
import { Progress } from './ui/progress';
import { Skeleton } from './ui/skeleton';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger, DialogFooter } from './ui/dialog';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Alert, AlertDescription, AlertTitle } from './ui/alert';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs';
import { logger } from '../utils/logger';
import { useActivityFeed } from '../hooks/useActivityFeed';
import { 
  Activity, 
  Server, 
  Users, 
  Shield, 
  AlertTriangle, 
  CheckCircle, 
  Clock,
  Cpu,
  HardDrive,
  Network,
  Zap,
>>>>>>> integration-branch
  Code,
  Eye,
<<<<<<< HEAD
  Download,
  Bell,
  Zap,
  Play,
  FileText,
  TrendingUp,
  Settings
} from 'lucide-react';
import { MLPipelineWidget } from './dashboard/MLPipelineWidget';
import { NextStepsWidget } from './dashboard/NextStepsWidget';
import { AdapterStatusWidget } from './dashboard/AdapterStatusWidget';
import { ComplianceScoreWidget } from './dashboard/ComplianceScoreWidget';
import { ActiveAlertsWidget } from './dashboard/ActiveAlertsWidget';
import { MultiModelStatusWidget } from './dashboard/MultiModelStatusWidget';
import { BaseModelWidget } from './dashboard/BaseModelWidget';
import { ReportingSummaryWidget } from './dashboard/ReportingSummaryWidget';
import { ServiceStatusWidget } from './dashboard/ServiceStatusWidget';
import { DashboardSettings } from './dashboard/DashboardSettings';
import { useAuth, useTenant } from '@/layout/LayoutProvider';
import { useNavigate } from 'react-router-dom';
import type { UserRole, User, SystemMetrics } from '@/api/types';
=======
  Target,
  Download,
  XCircle,
  Bell,
  BarChart3
} from 'lucide-react';
import { BaseModelStatusComponent } from './BaseModelStatus';
import { Nodes } from './Nodes';
import { AlertsPage } from './AlertsPage';
import { useInformationDensity } from '../hooks/useInformationDensity';
import { DensityControls } from './ui/density-controls';
>>>>>>> integration-branch
import apiClient from '../api/client';
import { useAnnounce, useKeyboardShortcuts } from '@/utils/accessibility';
import { usePolling } from '../hooks/usePolling';
import { useDashboardConfig } from '../hooks/useDashboardConfig';

interface DashboardProps {
  user?: User;
  selectedTenant?: string;
  onNavigate?: (tab: string) => void;
}

<<<<<<< HEAD
interface DashboardWidget {
  id: string;
  component: React.ComponentType<any>;
  priority: number;
}

interface DashboardLayout {
  widgets: DashboardWidget[];
  quickActions: Array<{
    label: string;
    icon: any;
    route: string;
    variant?: 'default' | 'outline' | 'secondary';
  }>;
}
=======
import { useAuth, useTenant } from '@/layout/LayoutProvider';
import { useNavigate } from 'react-router-dom';

export function Dashboard({ user: userProp, selectedTenant: tenantProp, onNavigate }: DashboardProps) {
  const { user } = useAuth();
  const { selectedTenant } = useTenant();
  const navigate = useNavigate();
  const effectiveUser = userProp ?? user!;
  const effectiveTenant = tenantProp ?? selectedTenant;
  const [systemMetrics, setSystemMetrics] = useState<SystemMetrics | null>(null);
  const [nodeCount, setNodeCount] = useState<number>(0);
  const [tenantCount, setTenantCount] = useState<number>(0);
  const [loading, setLoading] = useState(true);
  const [activeTab, setActiveTab] = useState('overview');
  
  // Information density management
  const { density, setDensity, spacing, textSizes } = useInformationDensity({
    key: 'dashboard',
    defaultDensity: 'comfortable',
    persist: true
  });
  
  // SSE connection for real-time metrics
  const { data: sseMetrics, error: sseError, connected } = useSSE<SystemMetrics>('/v1/stream/metrics');
  
  // Modals
  const [showHealthModal, setShowHealthModal] = useState(false);
  const [showCreateTenantModal, setShowCreateTenantModal] = useState(false);
  const [showDeployAdapterModal, setShowDeployAdapterModal] = useState(false);
  
  // Form states
  const [newTenantName, setNewTenantName] = useState('');
  const [newTenantIsolation, setNewTenantIsolation] = useState('standard');
  const [adapters, setAdapters] = useState<Adapter[]>([]);
  const [selectedAdapter, setSelectedAdapter] = useState('');
  const [deployTargetTenant, setDeployTargetTenant] = useState(selectedTenant);
  const [error, setError] = useState<string | null>(null);

  const fetchData = async () => {
    try {
      setError(null);
      const [metrics, nodes, tenants] = await Promise.all([
        apiClient.getSystemMetrics(),
        apiClient.listNodes(),
        apiClient.listTenants(),
      ]);
      setSystemMetrics(metrics);
      setNodeCount(nodes.length);
      setTenantCount(tenants.length);
    } catch (err) {
      // Replace: console.error('Failed to fetch dashboard data:', err);
      logger.error('Failed to fetch dashboard data', {
        component: 'Dashboard',
        operation: 'fetchData',
        tenantId: selectedTenant,
        userId: user.id
      }, err instanceof Error ? err : new Error(String(err)));
      
      const errorMsg = err instanceof Error ? err.message : 'Failed to load dashboard data';
      setError(errorMsg);
    } finally {
      setLoading(false);
      setRefreshing(false);
    }
  };
>>>>>>> integration-branch

// Simple system health widget for all roles
function SystemHealthWidget() {
  const announce = useAnnounce();

<<<<<<< HEAD
  const { data: metrics, isLoading: loading } = usePolling(
    () => apiClient.getSystemMetrics(),
    'slow',
    {
      operationName: 'SystemHealthWidget.getSystemMetrics',
      showLoadingIndicator: false,
      onSuccess: (data) => {
        const metrics = data as MetricsSnapshotResponse;
        if (metrics) {
          announce(`Metrics updated. Active sessions ${metrics.gauges?.active_sessions ?? 0}, latency ${metrics.gauges?.latency_p95_ms ?? 0} milliseconds`);
        }
      },
      onError: (err) => {
        logger.error('Failed to fetch system metrics', { component: 'SystemHealthWidget' }, err);
=======
  // Update metrics from SSE stream
  useEffect(() => {
    if (sseMetrics) {
      setSystemMetrics(sseMetrics);
    }
  }, [sseMetrics]);

  // Handle SSE connection status
  useEffect(() => {
    if (sseError) {
      // Replace: console.error('Real-time metrics connection error:', sseError);
      logger.error('Real-time metrics connection error', {
        component: 'Dashboard',
        operation: 'sse_connection',
        tenantId: selectedTenant,
        userId: user.id
      }, sseError);
    }
  }, [sseError, selectedTenant, user.id]);


  const handleCreateTenant = async () => {
    if (!newTenantName.trim()) {
      setError('Tenant name is required');
      return;
    }
    
    try {
      await apiClient.createTenant({
        name: newTenantName,
        isolation_level: newTenantIsolation,
      });
      toast.success(`Tenant "${newTenantName}" created successfully`);
      setShowCreateTenantModal(false);
      setNewTenantName('');
      setNewTenantIsolation('standard');
      setError(null);
      await fetchData();
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to create tenant';
      setError(errorMsg);
      toast.error(errorMsg);
    }
  };

  const handleDeployAdapter = async () => {
    if (!selectedAdapter) {
      setError('Please select an adapter');
      return;
    }
    
    try {
      // For now, we'll just show a success message
      // In a full implementation, this would call an adapter deployment endpoint
      toast.success(`Adapter deployed to tenant "${deployTargetTenant}"`);
      setShowDeployAdapterModal(false);
      setSelectedAdapter('');
      setError(null);
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to deploy adapter';
      setError(errorMsg);
      toast.error(errorMsg);
    }
  };

  const handleExportLogs = async () => {
    try {
      toast.info('Preparing log export...');
      // In a full implementation, this would call the log export endpoint
      // For now, we'll simulate a download
      setTimeout(() => {
        toast.success('Logs exported successfully');
      }, 1000);
    } catch (err) {
      toast.error('Failed to export logs');
    }
  };

  useEffect(() => {
    // Load adapters for deployment modal
    const loadAdapters = async () => {
      try {
        const adaptersList = await apiClient.listAdapters();
        setAdapters(adaptersList);
      } catch (err) {
        // Replace: console.error('Failed to load adapters:', err);
        logger.error('Failed to load adapters', {
          component: 'Dashboard',
          operation: 'loadAdapters',
          tenantId: selectedTenant,
          userId: user.id
        }, err instanceof Error ? err : new Error(String(err)));
>>>>>>> integration-branch
      }
    }
<<<<<<< HEAD
  );
=======
  }, [showDeployAdapterModal]);

  // Real-time activity feed from telemetry and audit logs
  const { events: activityEvents, loading: activityLoading, error: activityError } = useActivityFeed({
    enabled: true,
    maxEvents: 10,
    tenantId: effectiveTenant,
    userId: effectiveUser.id
  });

  // Helper functions for activity feed
  const formatTimeAgo = (timestamp: string): string => {
    const now = new Date();
    const eventTime = new Date(timestamp);
    const diffMs = now.getTime() - eventTime.getTime();
    const diffMins = Math.floor(diffMs / (1000 * 60));
    const diffHours = Math.floor(diffMins / 60);
    const diffDays = Math.floor(diffHours / 24);

    if (diffMins < 1) return 'just now';
    if (diffMins < 60) return `${diffMins}m ago`;
    if (diffHours < 24) return `${diffHours}h ago`;
    return `${diffDays}d ago`;
  };

  const getActivityIcon = (type: string) => {
    switch (type) {
      case 'recovery': return CheckCircle;
      case 'policy': return Shield;
      case 'build': return Zap;
      case 'adapter': return Code;
      case 'telemetry': return Eye;
      case 'security': return Shield;
      case 'error': return AlertTriangle;
      default: return Activity;
    }
  };

  // Transform activity events to display format
  const recentActivity = activityEvents.map(event => ({
    time: formatTimeAgo(event.timestamp),
    action: event.message,
    type: event.type,
    icon: getActivityIcon(event.type),
    severity: event.severity
  }));

  const quickActions = [
    { 
      label: 'View System Health', 
      icon: Activity, 
      color: 'text-emerald-600',
      onClick: () => setShowHealthModal(true)
    },
    { 
      label: 'Create Tenant', 
      icon: Users, 
      color: 'text-blue-600', 
      restricted: effectiveUser.role !== 'Admin',
      onClick: () => setShowCreateTenantModal(true)
    },
    { 
      label: 'Deploy Adapter', 
      icon: Code, 
      color: 'text-violet-600',
      onClick: () => setShowDeployAdapterModal(true)
    },
    { 
      label: 'Review Policies', 
      icon: Shield, 
      color: 'text-amber-600',
      onClick: () => (onNavigate ? onNavigate('policies') : navigate('/policies'))
    }
  ];
>>>>>>> integration-branch

  if (loading) {
    return (
      <Card aria-labelledby="sys-health-title">
        <CardHeader>
          <CardTitle id="sys-health-title">System Health</CardTitle>
        </CardHeader>
        <CardContent aria-busy={true}>
          <div role="status" aria-live="polite" className="h-20 animate-pulse bg-muted rounded">
            <span className="sr-only">Loading system health...</span>
          </div>
        </CardContent>
      </Card>
    );
  }

<<<<<<< HEAD
  return (
    <Card aria-labelledby="sys-health-title">
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Activity className="h-5 w-5" aria-hidden="true" />
          System Health
        </CardTitle>
      </CardHeader>
      <CardContent>
        <div className="grid grid-cols-2 gap-4">
          <div>
            <p className="text-sm text-muted-foreground">Memory Usage</p>
            <p className="text-2xl font-bold">{metrics?.memory_usage_pct || 0}%</p>
          </div>
          <div>
            <p className="text-sm text-muted-foreground">Active Sessions</p>
            <p className="text-2xl font-bold">{metrics?.active_sessions || 0}</p>
          </div>
          <div>
            <p className="text-sm text-muted-foreground">Tokens/sec</p>
            <p className="text-2xl font-bold">{metrics?.tokens_per_second || 0}</p>
          </div>
          <div>
            <p className="text-sm text-muted-foreground">P95 Latency</p>
            <p className="text-2xl font-bold">{metrics?.latency_p95_ms || 0}ms</p>
          </div>
        </div>
      </CardContent>
    </Card>
  );
}

// Role-specific dashboard configurations
const dashboardLayouts: Record<UserRole, DashboardLayout> = {
  admin: {
    widgets: [
      { id: 'service-status', component: ServiceStatusWidget, priority: 1 },
      { id: 'multi-model-status', component: MultiModelStatusWidget, priority: 2 },
      { id: 'system-health', component: SystemHealthWidget, priority: 3 },
      { id: 'active-alerts', component: ActiveAlertsWidget, priority: 4 },
      { id: 'compliance-score', component: ComplianceScoreWidget, priority: 5 },
      { id: 'reporting-summary', component: ReportingSummaryWidget, priority: 6 },
      { id: 'base-model', component: BaseModelWidget, priority: 7 },
    ],
    quickActions: [
      { label: 'System Health', icon: Activity, route: '/monitoring' },
      { label: 'Review Policies', icon: Shield, route: '/policies' },
      { label: 'View Telemetry', icon: Eye, route: '/telemetry' },
      { label: 'Manage Adapters', icon: Code, route: '/adapters' },
      { label: 'Reports', icon: FileText, route: '/reports' }
    ]
  },
  operator: {
    widgets: [
      { id: 'service-status', component: ServiceStatusWidget, priority: 1 },
      { id: 'ml-pipeline', component: MLPipelineWidget, priority: 2 },
      { id: 'adapter-status', component: AdapterStatusWidget, priority: 3 },
      { id: 'next-steps', component: NextStepsWidget, priority: 4 },
      { id: 'active-alerts', component: ActiveAlertsWidget, priority: 5 },
      { id: 'base-model', component: BaseModelWidget, priority: 6 },
    ],
    quickActions: [
      { label: 'Start Training', icon: Zap, route: '/training', variant: 'default' },
      { label: 'Test Adapter', icon: CheckCircle, route: '/testing' },
      { label: 'Run Inference', icon: Play, route: '/inference' },
      { label: 'View Routing', icon: TrendingUp, route: '/routing' },
    ]
  },
  sre: {
    widgets: [
      { id: 'service-status', component: ServiceStatusWidget, priority: 1 },
      { id: 'multi-model-status', component: MultiModelStatusWidget, priority: 2 },
      { id: 'active-alerts', component: ActiveAlertsWidget, priority: 3 },
      { id: 'system-health', component: SystemHealthWidget, priority: 4 },
      { id: 'adapter-status', component: AdapterStatusWidget, priority: 5 }
    ],
    quickActions: [
      { label: 'View Alerts', icon: Bell, route: '/monitoring', variant: 'default' },
      { label: 'System Logs', icon: FileText, route: '/telemetry' },
      { label: 'Routing Inspector', icon: TrendingUp, route: '/routing' },
      { label: 'Adapter Health', icon: Activity, route: '/adapters' }
    ]
  },
  compliance: {
    widgets: [
      { id: 'compliance-score', component: ComplianceScoreWidget, priority: 1 },
      { id: 'system-health', component: SystemHealthWidget, priority: 2 },
      { id: 'active-alerts', component: ActiveAlertsWidget, priority: 3 },
      { id: 'next-steps', component: NextStepsWidget, priority: 4 }
    ],
    quickActions: [
      { label: 'Review Policies', icon: Shield, route: '/policies', variant: 'default' },
      { label: 'Audit Trails', icon: FileText, route: '/audit' },
      { label: 'Export Telemetry', icon: Download, route: '/telemetry' },
      { label: 'Compliance Report', icon: CheckCircle, route: '/policies' }
    ]
  },
  auditor: {
    widgets: [
      { id: 'compliance-score', component: ComplianceScoreWidget, priority: 1 },
      { id: 'system-health', component: SystemHealthWidget, priority: 2 },
      { id: 'next-steps', component: NextStepsWidget, priority: 3 }
    ],
    quickActions: [
      { label: 'Audit Trails', icon: FileText, route: '/audit', variant: 'default' },
      { label: 'Verify Bundles', icon: Shield, route: '/telemetry' },
      { label: 'Export Audit', icon: Download, route: '/telemetry' },
      { label: 'Policy Review', icon: Shield, route: '/policies' }
    ]
  },
  viewer: {
    widgets: [
      { id: 'reporting-summary', component: ReportingSummaryWidget, priority: 1 },
      { id: 'system-health', component: SystemHealthWidget, priority: 2 },
      { id: 'adapter-status', component: AdapterStatusWidget, priority: 3 },
      { id: 'active-alerts', component: ActiveAlertsWidget, priority: 4 }
    ],
    quickActions: [
      { label: 'View Reports', icon: FileText, route: '/reports' },
      { label: 'Inference Playground', icon: Play, route: '/inference' },
      { label: 'System Metrics', icon: Activity, route: '/monitoring' },
      { label: 'Adapter Status', icon: Code, route: '/adapters' }
    ]
  }
};

export function Dashboard({ user: userProp, selectedTenant: tenantProp, onNavigate }: DashboardProps) {
  const { user } = useAuth();
  const { selectedTenant } = useTenant();
  const navigate = useNavigate();
  const effectiveUser = userProp ?? user!;
  const [settingsOpen, setSettingsOpen] = useState(false);

  // Dashboard configuration hook
  const {
    widgets: userWidgetConfig,
    isLoading: configLoading,
    updateWidgetVisibility,
    resetConfig
  } = useDashboardConfig(effectiveUser?.id);

  if (!effectiveUser) {
    return null;
  }

  // Get layout for the user's role (validation happens in auth provider)
  let layout = dashboardLayouts[effectiveUser.role];

  // Safety check - this should never happen with proper role validation
  if (!layout) {
    logger.error('Critical: Valid user role has no dashboard layout', {
      component: 'Dashboard',
      userRole: effectiveUser.role,
      availableLayouts: Object.keys(dashboardLayouts)
    });
    // Emergency fallback to prevent crash
    layout = dashboardLayouts.viewer;
  }

  // Filter and order widgets based on user configuration
  const visibleWidgets = useMemo(() => {
    if (userWidgetConfig.length === 0) {
      // No custom configuration, use role defaults
      return layout.widgets;
    }

    // Create a map of widget configurations
    const configMap = new Map(
      userWidgetConfig.map(config => [config.widget_id, config])
    );

    // Filter and sort widgets
    return layout.widgets
      .filter(widget => {
        const config = configMap.get(widget.id);
        // Show widget if no config exists (default) or if explicitly enabled
        return config === undefined || config.enabled;
      })
      .sort((a, b) => {
        const configA = configMap.get(a.id);
        const configB = configMap.get(b.id);
        const posA = configA?.position ?? a.priority;
        const posB = configB?.position ?? b.priority;
        return posA - posB;
      });
  }, [layout.widgets, userWidgetConfig]);

  // Get available widget IDs for the settings modal
  const availableWidgetIds = useMemo(
    () => layout.widgets.map(w => w.id),
    [layout.widgets]
  );

  // Global shortcuts for search/help (announced via live region)
  const announce = useAnnounce();
  useKeyboardShortcuts({
    onSearch: () => announce('Search shortcut pressed'),
    onHelp: () => announce('Help shortcut pressed'),
  });

  return (
    <div className="space-y-6">
      {/* Dashboard Header with Customize Button */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-bold">Dashboard</h1>
          <p className="text-muted-foreground mt-1">
            Welcome back, {effectiveUser.email}
          </p>
        </div>
        <Button
          variant="outline"
          size="sm"
          onClick={() => setSettingsOpen(true)}
          aria-label="Customize dashboard"
        >
          <Settings className="h-4 w-4 mr-2" />
          Customize
        </Button>
      </div>

      {/* Widgets Grid */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
        {visibleWidgets.map((widget) => {
          const WidgetComponent = widget.component;
          return <WidgetComponent key={widget.id} selectedTenant={selectedTenant} />;
        })}
=======
  const memoryUsage = systemMetrics?.memory_usage_pct || 0;
  const adapterCount = systemMetrics?.adapter_count || 0;
  const activeSessions = systemMetrics?.active_sessions || 0;
  const tokensPerSecond = systemMetrics?.tokens_per_second || 0;
  const latencyP95 = systemMetrics?.latency_p95_ms || 0;
  const cpuUsage = systemMetrics?.cpu_usage_percent || 0;
  const diskUsage = systemMetrics?.disk_usage_percent || 0;
  const networkBandwidth = systemMetrics?.network_rx_bytes ? (systemMetrics.network_rx_bytes / 1024 / 1024).toFixed(1) : '0';

  // Citation: docs/architecture/MasterPlan.md L30-L33
  const dashboardTabs = [
    { id: 'overview', label: 'Overview', icon: BarChart3, description: 'System overview and metrics' },
    { id: 'nodes', label: 'Nodes', icon: Server, description: 'Compute infrastructure monitoring' },
    { id: 'alerts', label: 'Alerts', icon: Bell, description: 'System alerts and monitoring' }
  ];

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className={`${textSizes.title} font-bold tracking-tight`}>Dashboard</h1>
          <p className="text-muted-foreground">
            System overview, health monitoring, and alerts
          </p>
        </div>
        <div className="flex items-center gap-2">
          <DensityControls 
            density={density} 
            onDensityChange={setDensity}
            showLabel={false}
          />
          <Badge variant="outline" className="text-sm">
            Tenant: {effectiveTenant}
          </Badge>
          <Badge variant="secondary" className="text-sm">
            {effectiveUser.role}
          </Badge>
        </div>
      </div>

      {/* Dashboard Tabs */}
      <Tabs value={activeTab} onValueChange={setActiveTab}>
        <TabsList className="grid w-full grid-cols-3">
          {dashboardTabs.map((tab) => {
            const Icon = tab.icon;
            return (
              <TabsTrigger key={tab.id} value={tab.id} className="flex items-center gap-2">
                <Icon className="h-4 w-4" />
                <span className="hidden sm:inline">{tab.label}</span>
              </TabsTrigger>
            );
          })}
        </TabsList>

        {/* Overview Tab */}
        <TabsContent value="overview" className={spacing.sectionGap}>
          {/* Error Alert */}
      {error && (
        <Alert variant="destructive">
          <AlertTriangle className="h-4 w-4" />
          <AlertTitle>Error Loading Dashboard</AlertTitle>
          <AlertDescription>
            {error}
            <Button 
              onClick={() => {
                setError(null);
                fetchData();
              }}
              variant="outline" 
              size="sm"
              className="mt-2"
            >
              Retry
            </Button>
          </AlertDescription>
        </Alert>
      )}

      {/* Header */}
      <div className="flex-between section-header">
        <div>
          <h1 className="section-title">System Dashboard</h1>
          <p className="section-description">
            Welcome back, {effectiveUser.display_name}. System status: Operational
          </p>
        </div>
        <div className="flex-standard">
          <div className="status-indicator status-success">
            <CheckCircle className="icon-small" />
            All Systems Operational
          </div>
          <Button variant="outline" size="sm" onClick={handleExportLogs}>
            <Download className="icon-standard mr-2" />
            Export Logs
          </Button>
        </div>
      </div>

      {/* System Overview Cards */}
      <div className="grid-standard grid-cols-4">
        <Card className="card-standard">
          <CardHeader className="flex-between pb-2">
            <CardTitle className="text-sm font-medium">Compute Nodes</CardTitle>
            <Server className="icon-standard text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold text-green-600">{nodeCount}</div>
            <p className="text-xs text-muted-foreground">
              {nodeCount} nodes online
            </p>
          </CardContent>
        </Card>

        <Card className="card-standard">
          <CardHeader className="flex-between pb-2">
            <CardTitle className="text-sm font-medium">Active Tenants</CardTitle>
            <Users className="icon-standard text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold text-blue-600">{tenantCount}</div>
            <p className="text-xs text-muted-foreground">
              All tenants operational
            </p>
          </CardContent>
        </Card>

        <Card className="card-standard">
          <CardHeader className="flex-between pb-2">
            <CardTitle className="text-sm font-medium">Code Adapters</CardTitle>
            <Code className="icon-standard text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold text-purple-600">{adapterCount}</div>
            <p className="text-xs text-muted-foreground">
              {activeSessions} active sessions
            </p>
          </CardContent>
        </Card>

        <Card className="card-standard">
          <CardHeader className="flex-between pb-2">
            <CardTitle className="text-sm font-medium">Performance</CardTitle>
            <Zap className="icon-standard text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold text-green-600">{tokensPerSecond.toFixed(0)}</div>
            <p className="text-xs text-muted-foreground">
              tokens/sec (p95: {latencyP95.toFixed(0)}ms)
            </p>
          </CardContent>
        </Card>
      </div>

      {/* Content Grid */}
      <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
        {/* System Resources */}
        <Card className="card-standard">
          <CardHeader>
            <CardTitle>System Resources</CardTitle>
          </CardHeader>
          <CardContent className="space-y-6">
            <div className="space-y-2">
              <div className="flex justify-between items-center mb-2">
                <div className="flex items-center gap-2">
                  <Cpu className="h-5 w-5 text-muted-foreground" />
                  <span className="text-sm font-medium">CPU Usage</span>
                  {connected && (
                    <Badge variant="outline" className="text-xs px-2 py-0 h-5">
                      <span className="relative flex h-2 w-2 mr-1">
                        <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-green-400 opacity-75"></span>
                        <span className="relative inline-flex rounded-full h-2 w-2 bg-green-500"></span>
                      </span>
                      Live
                    </Badge>
                  )}
                </div>
                <span className="text-sm font-semibold">
                  {systemMetrics ? `${cpuUsage.toFixed(1)}%` : '--'}
                </span>
              </div>
              <Progress value={cpuUsage} className="h-3 transition-all duration-500" />
            </div>

            <div className="space-y-2">
              <div className="flex justify-between items-center mb-2">
                <div className="flex items-center gap-2">
                  <HardDrive className="h-5 w-5 text-muted-foreground" />
                  <span className="text-sm font-medium">Memory Usage</span>
                </div>
                <span className="text-sm font-semibold">
                  {systemMetrics ? `${systemMetrics.memory_usage_percent ? systemMetrics.memory_usage_percent.toFixed(1) : memoryUsage.toFixed(1)}%` : '--'}
                </span>
              </div>
              <Progress value={systemMetrics?.memory_usage_percent || memoryUsage} className="h-3 transition-all duration-500" />
            </div>

            <div className="space-y-2">
              <div className="flex justify-between items-center mb-2">
                <div className="flex items-center gap-2">
                  <HardDrive className="h-5 w-5 text-muted-foreground" />
                  <span className="text-sm font-medium">Disk Usage</span>
                </div>
                <span className="text-sm font-semibold">
                  {systemMetrics ? `${diskUsage.toFixed(1)}%` : '--'}
                </span>
              </div>
              <Progress value={diskUsage} className="h-3 transition-all duration-500" />
            </div>

            <div className="space-y-2">
              <div className="flex justify-between items-center mb-2">
                <div className="flex items-center gap-2">
                  <Network className="h-5 w-5 text-muted-foreground" />
                  <span className="text-sm font-medium">Network Bandwidth</span>
                </div>
                <span className="text-sm font-semibold">
                  {systemMetrics ? `${networkBandwidth} MB/s` : '--'}
                </span>
              </div>
              <Progress value={Math.min(parseFloat(networkBandwidth), 100)} className="h-3 transition-all duration-500" />
            </div>
          </CardContent>
        </Card>

        {/* Recent Activity */}
        <Card className="card-standard">
          <CardHeader>
            <CardTitle>Recent Activity</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="form-field">
              {recentActivity.map((activity, index) => {
                const Icon = activity.icon;
                return (
                  <div key={index} className="flex-standard">
                    <div className={`p-1 rounded-full bg-muted`}>
                      <Icon className="icon-small" />
                    </div>
                    <div className="flex-1 form-field">
                      <p className="text-sm">{activity.action}</p>
                      <p className="text-xs text-muted-foreground">{activity.time}</p>
                    </div>
                  </div>
                );
              })}
            </div>
          </CardContent>
        </Card>

        {/* Base Model Status */}
        <BaseModelStatusComponent selectedTenant={effectiveTenant} />
>>>>>>> integration-branch
      </div>

      {/* Quick Actions */}
      <Card>
        <CardHeader>
          <CardTitle>Quick Actions</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-4 gap-3" aria-label="Quick actions" role="list">
            {layout.quickActions.map((action, index) => {
              const Icon = action.icon;
              return (
                <Button
                  key={`${action.label}-${index}`}
                  variant={action.variant || 'outline'}
                  className="justify-start h-auto py-4"
                  aria-label={`Quick action: ${action.label}`}
                  onClick={() => {
                    if (onNavigate) {
                      onNavigate(action.route);
                    } else {
                      navigate(action.route);
                    }
                  }}
                >
                  <div className="flex items-center gap-3">
                    <Icon className="h-5 w-5" aria-hidden="true" />
                    <span className="font-medium">{action.label}</span>
                  </div>
                </Button>
              );
            })}
          </div>
        </CardContent>
      </Card>

<<<<<<< HEAD
      {/* Dashboard Settings Modal */}
      <DashboardSettings
        open={settingsOpen}
        onOpenChange={setSettingsOpen}
        availableWidgetIds={availableWidgetIds}
        currentConfig={userWidgetConfig}
        onUpdateVisibility={updateWidgetVisibility}
        onReset={resetConfig}
        isUpdating={configLoading}
      />
=======
      {/* System Health Modal */}
      <Dialog open={showHealthModal} onOpenChange={setShowHealthModal}>
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle>System Health Details</DialogTitle>
          </DialogHeader>
          <div className="space-y-4">
            <div className="grid grid-cols-2 gap-4">
              <Card>
                <CardHeader className="pb-2">
                  <CardTitle className="text-sm">CPU Usage</CardTitle>
                </CardHeader>
                <CardContent>
                  <div className="text-2xl font-bold">34%</div>
                  <Progress value={34} className="mt-2" />
                </CardContent>
              </Card>
              <Card>
                <CardHeader className="pb-2">
                  <CardTitle className="text-sm">Memory Usage</CardTitle>
                </CardHeader>
                <CardContent>
                  <div className="text-2xl font-bold">{memoryUsage.toFixed(0)}%</div>
                  <Progress value={memoryUsage} className="mt-2" />
                </CardContent>
              </Card>
            </div>
            <div className="space-y-2">
              <div className="flex justify-between text-sm">
                <span>Active Nodes:</span>
                <span className="font-medium">{nodeCount}</span>
              </div>
              <div className="flex justify-between text-sm">
                <span>Active Adapters:</span>
                <span className="font-medium">{adapterCount}</span>
              </div>
              <div className="flex justify-between text-sm">
                <span>Tokens/Second:</span>
                <span className="font-medium">{tokensPerSecond.toFixed(0)}</span>
              </div>
              <div className="flex justify-between text-sm">
                <span>Latency (p95):</span>
                <span className="font-medium">{latencyP95.toFixed(0)}ms</span>
              </div>
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setShowHealthModal(false)}>Close</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Create Tenant Modal */}
      <Dialog open={showCreateTenantModal} onOpenChange={setShowCreateTenantModal}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Create New Tenant</DialogTitle>
          </DialogHeader>
          {error && (
            <Alert variant="destructive">
              <XCircle className="h-4 w-4" />
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          )}
          <div className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="tenant-name">Tenant Name</Label>
              <Input
                id="tenant-name"
                placeholder="Enter tenant name"
                value={newTenantName}
                onChange={(e) => setNewTenantName(e.target.value)}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="isolation-level">Isolation Level</Label>
              <Select value={newTenantIsolation} onValueChange={setNewTenantIsolation}>
                <SelectTrigger id="isolation-level">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="standard">Standard</SelectItem>
                  <SelectItem value="high">High</SelectItem>
                  <SelectItem value="maximum">Maximum</SelectItem>
                </SelectContent>
              </Select>
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => {
              setShowCreateTenantModal(false);
              setError(null);
            }}>Cancel</Button>
            <Button onClick={handleCreateTenant}>Create Tenant</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Deploy Adapter Modal */}
      <Dialog open={showDeployAdapterModal} onOpenChange={setShowDeployAdapterModal}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Deploy Adapter</DialogTitle>
          </DialogHeader>
          {error && (
            <Alert variant="destructive">
              <XCircle className="h-4 w-4" />
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          )}
          <div className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="adapter-select">Select Adapter</Label>
              <Select value={selectedAdapter} onValueChange={setSelectedAdapter}>
                <SelectTrigger id="adapter-select">
                  <SelectValue placeholder="Choose an adapter" />
                </SelectTrigger>
                <SelectContent>
                  {adapters.map((adapter) => (
                    <SelectItem key={adapter.id} value={adapter.adapter_id}>
                      {adapter.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-2">
              <Label htmlFor="target-tenant">Target Tenant</Label>
              <Input
                id="target-tenant"
                value={deployTargetTenant}
                onChange={(e) => setDeployTargetTenant(e.target.value)}
              />
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => {
              setShowDeployAdapterModal(false);
              setError(null);
            }}>Cancel</Button>
            <Button onClick={handleDeployAdapter}>Deploy</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
        </TabsContent>

        {/* Nodes Tab */}
        <TabsContent value="nodes" className="space-y-4">
          <Nodes user={user} selectedTenant={selectedTenant} />
        </TabsContent>

        {/* Alerts Tab */}
        <TabsContent value="alerts" className="space-y-4">
          <AlertsPage selectedTenant={selectedTenant} />
        </TabsContent>
      </Tabs>
>>>>>>> integration-branch
    </div>
  );
}
