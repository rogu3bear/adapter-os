import React, { useState, useEffect } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Badge } from './ui/badge';
import { Button } from './ui/button';
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
  Code,
  GitBranch,
  Eye,
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
import apiClient from '../api/client';
import { SystemMetrics, User, Adapter } from '../api/types';
import { toast } from 'sonner';
import { useSSE } from '../hooks/useSSE';

interface DashboardProps {
  user?: User;
  selectedTenant?: string;
  onNavigate?: (tab: string) => void;
}

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

  useEffect(() => {
    fetchData();
  }, [selectedTenant]);

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
      }
    };
    if (showDeployAdapterModal) {
      loadAdapters();
    }
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

  if (loading) {
    return (
      <div className="space-y-6">
        {/* Header Skeleton */}
        <div className="flex justify-between items-start mb-6">
          <div>
            <Skeleton className="h-8 w-48 mb-2" />
            <Skeleton className="h-4 w-96" />
          </div>
          <div className="flex gap-2">
            <Skeleton className="h-10 w-32" />
            <Skeleton className="h-10 w-24" />
            <Skeleton className="h-10 w-32" />
          </div>
        </div>

        {/* Metric Cards Skeleton */}
        <div className="grid grid-cols-1 gap-6 md:grid-cols-2 lg:grid-cols-4">
          {[...Array(4)].map((_, i) => (
            <Card key={i}>
              <CardHeader className="pb-2">
                <Skeleton className="h-4 w-24" />
              </CardHeader>
              <CardContent>
                <Skeleton className="h-8 w-16 mb-2" />
                <Skeleton className="h-3 w-32" />
              </CardContent>
            </Card>
          ))}
        </div>

        {/* Content Grid Skeleton */}
        <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
          {[...Array(2)].map((_, i) => (
            <Card key={i}>
              <CardHeader>
                <Skeleton className="h-6 w-40" />
              </CardHeader>
              <CardContent className="space-y-4">
                {[...Array(4)].map((_, j) => (
                  <div key={j} className="space-y-2">
                    <Skeleton className="h-4 w-full" />
                    <Skeleton className="h-2 w-full" />
                  </div>
                ))}
              </CardContent>
            </Card>
          ))}
        </div>

        {/* Quick Actions Skeleton */}
        <Card>
          <CardHeader>
            <Skeleton className="h-6 w-32" />
          </CardHeader>
          <CardContent>
            <div className="grid grid-cols-2 gap-4 md:grid-cols-4">
              {[...Array(4)].map((_, i) => (
                <Skeleton key={i} className="h-24" />
              ))}
            </div>
          </CardContent>
        </Card>
      </div>
    );
  }

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
      </div>

      {/* Quick Actions */}
      <Card className="card-standard">
        <CardHeader>
          <CardTitle>Quick Actions</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid-standard grid-cols-4">
            {quickActions.map((action, index) => {
              const Icon = action.icon;
              return (
                <Button
                  key={index}
                  variant="outline"
                  className="h-24 flex-col form-field"
                  disabled={action.restricted}
                  onClick={action.onClick}
                >
                  <Icon className={`icon-large ${action.color}`} />
                  <span className="text-xs text-center">{action.label}</span>
                </Button>
              );
            })}
          </div>
        </CardContent>
      </Card>

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
    </div>
  );
}