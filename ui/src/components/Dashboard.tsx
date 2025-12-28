/**
 * Dashboard Component
 *
 * Main dashboard view with tabs for Overview, Nodes, and Alerts.
 * Extracted hooks and components are in:
 * - ui/src/hooks/dashboard/ for data fetching hooks
 * - ui/src/components/dashboard/ for sub-components
 */

import React, { useState, useEffect, useCallback, memo } from 'react';
import { useNavigate } from 'react-router-dom';
import { toast } from 'sonner';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from './ui/dialog';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Button } from './ui/button';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Server, Bell, BarChart3 } from 'lucide-react';
import { Nodes } from './Nodes';
import { AlertsPage } from '@/pages/Alerts/AlertsPage';
import { useInformationDensity } from '@/hooks/ui/useInformationDensity';
import { DensityControls } from './ui/density-controls';
import { PageHeader } from './ui/page-header';
import { GlossaryTooltip } from './ui/glossary-tooltip';
import { errorRecoveryTemplates } from './ui/error-recovery';
import { useDashboardMetrics, useDashboardStats } from '@/hooks/dashboard';
import { useDashboardConfig } from '@/hooks/config/useDashboardConfig';
import { useSystemStatus } from '@/hooks/system/useSystemStatus';
import { useRBAC } from '@/hooks/security/useRBAC';
import { createDialogManager } from '@/hooks/ui/useDialogManager';
import { apiClient } from '@/api/services';
import { logger } from '@/utils/logger';
import { DashboardOverviewTab } from './dashboard/DashboardOverviewTab';
import type { User } from '@/api/types';
import type { DashboardProps } from '@/types/components';

// Dashboard dialog manager - local state-based dialog management
const useDashboardDialogs = createDialogManager<
  'health' | 'createTenant' | 'deployAdapter',
  {
    health: undefined;
    createTenant: undefined;
    deployAdapter: undefined;
  }
>(['health', 'createTenant', 'deployAdapter'] as const);

// Static dashboard tabs - moved outside component to prevent recreation
const DASHBOARD_TABS = [
  { id: 'overview', label: 'Overview', icon: BarChart3, description: 'System overview and metrics' },
  { id: 'nodes', label: 'Nodes', icon: Server, description: 'Compute infrastructure monitoring' },
  { id: 'alerts', label: 'Alerts', icon: Bell, description: 'System alerts and monitoring' },
] as const;

// Main Dashboard component
export const Dashboard = memo(function Dashboard({
  user,
  selectedTenant,
  onNavigate,
}: DashboardProps) {
  const navigate = useNavigate();
  const { can } = useRBAC();
  const dialogs = useDashboardDialogs();

  // State declarations
  const [activeTab, setActiveTab] = useState('overview');
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [newTenantName, setNewTenantName] = useState('');
  const [newTenantIsolation, setNewTenantIsolation] = useState('standard');
  const [adapters, setAdapters] = useState<{ id: string; name: string }[]>([]);
  const [selectedAdapter, setSelectedAdapter] = useState('');
  const [deployTargetTenant, setDeployTargetTenant] = useState(selectedTenant || '');
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [nodeCount, setNodeCount] = useState(0);
  const [tenantCount, setTenantCount] = useState(0);
  const [createTenantError, setCreateTenantError] = useState<string | null>(null);
  const [deployAdapterError, setDeployAdapterError] = useState<string | null>(null);

  // Information density
  const { density, setDensity, spacing } = useInformationDensity({
    key: 'dashboard-density',
    defaultDensity: 'comfortable',
    persist: true,
  });

  // Dashboard configuration
  const { widgets, isLoading: configLoading, updateWidgetVisibility, resetConfig } = useDashboardConfig(user?.user_id);

  // Derive available widget IDs from widgets config
  const availableWidgetIds = widgets.map((w) => w.widget_id);
  const userWidgetConfig = widgets;

  // Effective user and tenant
  const effectiveUser: User = user || {
    user_id: 'guest',
    email: 'guest@adapteros.local',
    display_name: 'Guest',
    role: 'viewer' as const,
    created_at: new Date().toISOString(),
  };

  // Display-only tenant label (for UI strings), API hooks use selectedTenant directly
  const effectiveTenant = selectedTenant || 'default';

  // Use extracted hooks for metrics and stats
  const metrics = useDashboardMetrics({
    selectedTenant,
    userId: user?.user_id,
    enabled: true,
  });

  const stats = useDashboardStats({
    selectedTenant,
  });

  // System status for accurate health display
  const systemStatus = useSystemStatus({
    enabled: true,
    tenantId: selectedTenant,
  });

  // Derive system status label from actual backend state
  const getSystemStatusLabel = (): string => {
    if (systemStatus.loading && !systemStatus.data) {
      return 'Checking...';
    }
    if (systemStatus.error && !systemStatus.data) {
      return 'Unknown';
    }
    if (systemStatus.stale) {
      return 'Stale';
    }

    const data = systemStatus.data;
    if (!data) {
      return 'Unknown';
    }

    // Check boot phase first
    const phase = data.boot?.phase ?? data.readiness?.phase;
    if (phase) {
      const normalized = phase.toLowerCase();
      if (normalized === 'ready' || normalized === 'running') {
        // Check for degraded state
        if (data.boot?.degradedReasons?.length || data.readiness?.degraded?.length) {
          return 'Degraded';
        }
        return 'Operational';
      }
      if (normalized.includes('fail') || normalized.includes('panic')) {
        return 'Failed';
      }
      if (normalized === 'starting' || normalized === 'booting' || normalized.includes('loading')) {
        return 'Starting';
      }
      // Other boot phases
      return 'Starting';
    }

    // No phase info - check inference readiness
    if (data.inferenceReady === 'true') {
      return 'Operational';
    }
    if (data.inferenceBlockers?.length) {
      return 'Blocked';
    }

    return 'Unknown';
  };

  const systemStatusLabel = getSystemStatusLabel();
  const systemStatusSuffix = systemStatus.isFallback ? ' (fallback)' : '';

  // Compute header description
  const headerDescription = stats.defaultStackLoading
    ? `Workspace ${effectiveTenant} - Resolving default stack...`
    : stats.adapterStackError
      ? `Workspace ${effectiveTenant} - Default stack unavailable - System status: ${systemStatusLabel}${systemStatusSuffix}`
      : stats.defaultStack
        ? `Workspace ${effectiveTenant} - Default stack ${stats.defaultStack.name} - System status: ${systemStatusLabel}${systemStatusSuffix}`
        : `Workspace ${effectiveTenant} - No default stack configured - System status: ${systemStatusLabel}${systemStatusSuffix}`;

  const deployModalOpen = dialogs.isOpen('deployAdapter');

  // Fetch dashboard data
  const fetchData = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);

      // Fetch nodes and tenants in parallel for efficiency
      const [nodes, tenants] = await Promise.all([
        apiClient.listNodes(),
        apiClient.listTenants(),
      ]);

      setNodeCount(nodes.length);
      setTenantCount(tenants.length);
      setLoading(false);
    } catch (err) {
      logger.error(
        'Failed to fetch dashboard data',
        {
          component: 'Dashboard',
          operation: 'fetchData',
          tenantId: selectedTenant,
          userId: user?.user_id,
        },
        err instanceof Error ? err : new Error(String(err))
      );
      setError(err instanceof Error ? err.message : 'Failed to load dashboard');
      setLoading(false);
    }
  }, [selectedTenant, user?.user_id]);

  useEffect(() => {
    fetchData();
  }, [fetchData]);

  const handleCreateTenant = async () => {
    if (!newTenantName.trim()) {
      setCreateTenantError('Workspace name is required');
      return;
    }

    try {
      await apiClient.createTenant({
        name: newTenantName,
        uid: 1000,
        gid: 1000,
        isolation_level: newTenantIsolation,
      });
      toast.success(`Workspace "${newTenantName}" created successfully`);
      dialogs.closeDialog('createTenant');
      setNewTenantName('');
      setNewTenantIsolation('standard');
      setCreateTenantError(null);
      await fetchData();
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to create workspace';
      setCreateTenantError(errorMsg);
      toast.error(errorMsg);
    }
  };

  const handleDeployAdapter = async () => {
    if (!selectedAdapter) {
      setDeployAdapterError('Please select an adapter');
      return;
    }

    try {
      toast.success(`Adapter deployed to workspace "${deployTargetTenant}"`);
      dialogs.closeDialog('deployAdapter');
      setSelectedAdapter('');
      setDeployAdapterError(null);
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to deploy adapter';
      setDeployAdapterError(errorMsg);
      toast.error(errorMsg);
    }
  };

  useEffect(() => {
    const loadAdapters = async () => {
      try {
        const adaptersList = await apiClient.listAdapters();
        setAdapters(adaptersList);
      } catch (err) {
        logger.error(
          'Failed to load adapters',
          {
            component: 'Dashboard',
            operation: 'loadAdapters',
            tenantId: selectedTenant,
            userId: user?.user_id,
          },
          err instanceof Error ? err : new Error(String(err))
        );
      }
    };
    if (deployModalOpen) {
      loadAdapters();
    }
  }, [deployModalOpen, selectedTenant, user?.user_id]);

  return (
    <div className="space-y-6">
      {/* Header */}
      <PageHeader
        title="Dashboard"
        description={headerDescription}
        badges={[
          { label: `Workspace: ${effectiveTenant}`, variant: 'outline' },
          { label: stats.defaultStackLabel, variant: 'secondary' },
          { label: effectiveUser.role, variant: 'secondary' },
        ]}
      >
        <DensityControls density={density} onDensityChange={setDensity} showLabel={false} />
      </PageHeader>

      {/* Dashboard Tabs */}
      <Tabs value={activeTab} onValueChange={setActiveTab}>
        <TabsList className="grid w-full grid-cols-3">
          {DASHBOARD_TABS.map((tab) => {
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
          <DashboardOverviewTab
            effectiveUser={effectiveUser}
            effectiveTenant={effectiveTenant}
            selectedTenant={selectedTenant}
            settingsOpen={settingsOpen}
            onSettingsOpenChange={setSettingsOpen}
            availableWidgetIds={availableWidgetIds}
            userWidgetConfig={userWidgetConfig}
            onUpdateWidgetVisibility={updateWidgetVisibility}
            onResetConfig={resetConfig}
            configLoading={configLoading}
            dialogs={dialogs}
            canManageTenant={can('tenant:manage')}
            canRegisterAdapter={can('adapter:register')}
            onNavigate={onNavigate}
            cpuUsage={metrics.cpuUsage}
            memoryUsage={metrics.memoryUsage}
            diskUsage={metrics.diskUsage}
            networkBandwidth={metrics.networkBandwidth}
            adapterCount={metrics.adapterCount}
            activeSessions={metrics.activeSessions}
            tokensPerSecond={metrics.tokensPerSecond}
            latencyP95={metrics.latencyP95}
            connected={metrics.connected}
            sseError={metrics.sseError}
            systemMetrics={metrics.systemMetrics}
            onReconnect={metrics.reconnect}
            loading={loading}
            error={error}
            nodeCount={nodeCount}
            tenantCount={tenantCount}
            onFetchData={fetchData}
            onClearError={() => setError(null)}
            datasetStats={stats.datasetStats}
            datasetsLoading={stats.datasetsLoading}
            datasetsError={stats.datasetsError}
            onRefetchDatasets={stats.refetchDatasets}
            runningJobs={stats.runningJobs}
            completedLast7Days={stats.completedLast7Days}
            recentTrainingJob={stats.recentTrainingJob}
            recentCompletedJobWithStack={stats.recentCompletedJobWithStack}
            trainingJobsLoading={stats.trainingJobsLoading}
            trainingJobsError={stats.trainingJobsError}
            onRefetchTrainingJobs={stats.refetchTrainingJobs}
            adapterTotal={stats.adapterTotal}
            stackTotal={stats.stackTotal}
            stackNameLookup={stats.stackNameLookup}
            defaultStack={stats.defaultStack}
            defaultStackLabel={stats.defaultStackLabel}
            adaptersLoading={stats.adaptersLoading}
            stacksLoading={stats.stacksLoading}
            defaultStackLoading={stats.defaultStackLoading}
            adapterStackError={stats.adapterStackError}
            onRefetchAdapters={stats.refetchAdapters}
            onRefetchStacks={stats.refetchStacks}
            onRefetchDefaultStack={stats.refetchDefaultStack}
            spacing={spacing}
          />
        </TabsContent>

        {/* Nodes Tab */}
        <TabsContent value="nodes" className="space-y-4">
          <Nodes user={user ?? effectiveUser} selectedTenant={selectedTenant ?? 'default'} />
        </TabsContent>

        {/* Alerts Tab */}
        <TabsContent value="alerts" className="space-y-4">
          <AlertsPage selectedTenant={selectedTenant} />
        </TabsContent>
      </Tabs>

      {/* Create Workspace Modal */}
      <Dialog
        open={dialogs.isOpen('createTenant')}
        onOpenChange={(open) => {
          if (!open) {
            dialogs.closeDialog('createTenant');
            setCreateTenantError(null);
          }
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Create New Workspace</DialogTitle>
          </DialogHeader>
          {createTenantError &&
            errorRecoveryTemplates.genericError(createTenantError, () => {
              setCreateTenantError(null);
            })}
          <div className="space-y-4">
            <div className="space-y-2">
              <GlossaryTooltip termId="tenant-name-field">
                <Label htmlFor="tenant-name" className="cursor-help">
                  Workspace Name
                </Label>
              </GlossaryTooltip>
              <Input
                id="tenant-name"
                placeholder="Enter workspace name"
                value={newTenantName}
                onChange={(e) => setNewTenantName(e.target.value)}
              />
            </div>
            <div className="space-y-2">
              <GlossaryTooltip termId="isolation-level-field">
                <Label htmlFor="isolation-level" className="cursor-help">
                  Isolation Level
                </Label>
              </GlossaryTooltip>
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
            <Button
              variant="outline"
              onClick={() => {
                dialogs.closeDialog('createTenant');
                setCreateTenantError(null);
              }}
            >
              Cancel
            </Button>
            <Button onClick={handleCreateTenant}>Create Workspace</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Deploy Adapter Modal */}
      <Dialog
        open={deployModalOpen}
        onOpenChange={(open) => {
          if (!open) {
            dialogs.closeDialog('deployAdapter');
            setDeployAdapterError(null);
          }
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Deploy Adapter</DialogTitle>
          </DialogHeader>
          {deployAdapterError &&
            errorRecoveryTemplates.genericError(deployAdapterError, () => {
              setDeployAdapterError(null);
            })}
          <div className="space-y-4">
            <div className="space-y-2">
              <GlossaryTooltip termId="adapter-select-field">
                <Label htmlFor="adapter-select" className="cursor-help">
                  Select Adapter
                </Label>
              </GlossaryTooltip>
              <Select value={selectedAdapter} onValueChange={setSelectedAdapter}>
                <SelectTrigger id="adapter-select">
                  <SelectValue placeholder="Choose an adapter" />
                </SelectTrigger>
                <SelectContent>
                  {adapters.map((adapter) => (
                    <SelectItem key={adapter.id} value={adapter.id}>
                      {adapter.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-2">
              <GlossaryTooltip termId="target-tenant-field">
                <Label htmlFor="target-tenant" className="cursor-help">
                  Target Workspace
                </Label>
              </GlossaryTooltip>
              <Input
                id="target-tenant"
                value={deployTargetTenant}
                onChange={(e) => setDeployTargetTenant(e.target.value)}
              />
            </div>
          </div>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => {
                dialogs.closeDialog('deployAdapter');
                setDeployAdapterError(null);
              }}
            >
              Cancel
            </Button>
            <Button onClick={handleDeployAdapter}>Deploy</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
});
