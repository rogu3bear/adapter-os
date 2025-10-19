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
import { BaseModelLoader } from './BaseModelLoader';
import { CursorSetupWizard } from './CursorSetupWizard';
import { Nodes } from './Nodes';
import { AlertsPage } from './AlertsPage';
import { useInformationDensity } from '../hooks/useInformationDensity';
import { DensityControls } from './ui/density-controls';
import { HelpTooltip } from './ui/help-tooltip';
import apiClient from '../api/client';
import { SystemMetrics, User, Adapter } from '../api/types';
import { toast } from 'sonner';
import { useSSE } from '../hooks/useSSE';
import { useTimestamp } from '../hooks/useTimestamp';

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
  const [showCursorWizard, setShowCursorWizard] = useState(false);
  
  // Model status state
  const [modelStatus, setModelStatus] = useState<any>(null);
  
  // Form states
  const [newTenantName, setNewTenantName] = useState('');
  const [newTenantIsolation, setNewTenantIsolation] = useState('standard');
  const [adapters, setAdapters] = useState<Adapter[]>([]);
  const [selectedAdapter, setSelectedAdapter] = useState('');
  const [deployTargetTenant, setDeployTargetTenant] = useState(selectedTenant);
  const [error, setError] = useState<string | null>(null);
  const [lastUpdatedAt, setLastUpdatedAt] = useState<string | null>(null);

  const fetchData = async () => {
    try {
      setError(null);
      const [metrics, nodes, tenants, baseModelStatus] = await Promise.all([
        apiClient.getSystemMetrics(),
        apiClient.listNodes(),
        apiClient.listTenants(),
        apiClient.getBaseModelStatus(effectiveTenant).catch(() => null),
      ]);
      setSystemMetrics(metrics);
      setNodeCount(nodes.length);
      setTenantCount(tenants.length);
      setModelStatus(baseModelStatus);
      setLastUpdatedAt(new Date().toISOString());
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
      setLastUpdatedAt(new Date().toISOString());
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
      const result = await apiClient.loadAdapter(selectedAdapter);
      toast.success(`Adapter "${result.name}" loaded (ID: ${result.adapter_id})`);
      setShowDeployAdapterModal(false);
      setSelectedAdapter('');
      setError(null);
      // Optionally refresh dashboard metrics
      await fetchData();
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to deploy adapter';
      setError(errorMsg);
      toast.error(errorMsg);
    }
  };

  const handleExportLogs = async () => {
    try {
      toast.info('Preparing telemetry bundle export...');
      const bundles = await apiClient.listTelemetryBundles();
      if (!bundles || bundles.length === 0) {
        toast.info('No telemetry bundles available to export');
        return;
      }
      // Export the most recent bundle by created_at
      const sorted = [...bundles].sort((a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime());
      const latest = sorted[0];
      const res = await apiClient.exportTelemetryBundle(latest.id);
      toast.success(`Bundle ${res.bundle_id} ready: ${res.events_count} events, ${(res.size_bytes/1024/1024).toFixed(1)} MB`);
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to export telemetry bundle';
      toast.error(errorMsg);
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
      label: 'Generate Telemetry Bundle', 
      icon: Download, 
      color: 'text-emerald-600',
      onClick: async () => {
        try {
          toast.info('Generating telemetry bundle...');
          const res = await apiClient.generateTelemetryBundle();
          toast.success(`Bundle ${res.id} created with ${res.event_count} events`);
        } catch (err) {
          const errorMsg = err instanceof Error ? err.message : 'Failed to generate bundle';
          toast.error(errorMsg);
        }
      }
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
    <div className="p-[var(--space-4)] bg-[var(--surface-1)] rounded-[var(--radius-card)] shadow-[var(--shadow-md)]">
      {/* Header */}
      <h1 className="text-[var(--font-h1)] font-[var(--font-weight-bold)] text-gray-900 mb-[var(--space-6)]">
        Dashboard
      </h1>
      
      {/* Metrics cards */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-[var(--space-4)]">
        <Card className="border-[var(--gray-300)] hover:border-[var(--accent-500)]">
          <CardHeader className="pb-[var(--space-3)]">
            <CardTitle className="text-[var(--font-h3)] text-gray-700">
              Nodes
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-[var(--font-h2)] font-[var(--font-weight-semibold)] text-green-600">
              42
            </div>
          </CardContent>
        </Card>
        {/* Similar for other cards: use --error for alerts, --info for telemetry */}
        {/* ... existing content ... */}
      </div>
      
      {/* Existing rest ... */}
    </div>
  );
}
