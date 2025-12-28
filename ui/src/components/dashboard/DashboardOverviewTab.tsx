/**
 * Dashboard Overview Tab Component
 *
 * Main content for the overview tab including workflow section,
 * KPI cards, system resources, activity feed, and quick actions.
 */

import React, { memo, useMemo } from 'react';
import { useNavigate } from 'react-router-dom';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { ContentGrid } from '@/components/ui/grid';
import { ActionGrid } from '@/components/ui/action-grid';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';
import { errorRecoveryTemplates } from '@/components/ui/error-recovery';
import { BaseModelStatusComponent } from '@/components/BaseModelStatus';
import { PluginStatusWidget } from './PluginStatusWidget';
import { DashboardSettings } from './DashboardSettings';
import { DashboardWorkflowSection } from './DashboardWorkflowSection';
import { DashboardKpiCards } from './DashboardKpiCards';
import { DashboardSystemResources } from './DashboardSystemResources';
import { DashboardHealthDialog } from './DashboardHealthDialog';
import { useActivityFeed } from '@/hooks/realtime/useActivityFeed';
import { formatRelativeTime } from '@/lib/formatters';
import { buildSecurityPoliciesLink } from '@/utils/navLinks';
import {
  Activity,
  Users,
  Shield,
  AlertTriangle,
  CheckCircle,
  Zap,
  Code,
  Eye,
} from 'lucide-react';
import type { User, TrainingJob, AdapterStack, DashboardWidgetConfig } from '@/api/types';
import type { DatasetStats } from '@/hooks/dashboard';

/**
 * Dialog state manager interface
 */
interface DialogManager {
  isOpen: (id: 'health' | 'createTenant' | 'deployAdapter') => boolean;
  openDialog: (id: 'health' | 'createTenant' | 'deployAdapter') => void;
  closeDialog: (id: 'health' | 'createTenant' | 'deployAdapter') => void;
}

/**
 * Props for the DashboardOverviewTab component
 */
export interface DashboardOverviewTabProps {
  // User and tenant context
  effectiveUser: User;
  effectiveTenant: string;
  selectedTenant?: string;

  // Dashboard config
  settingsOpen: boolean;
  onSettingsOpenChange: (open: boolean) => void;
  availableWidgetIds: string[];
  userWidgetConfig: DashboardWidgetConfig[];
  onUpdateWidgetVisibility: (widgetId: string, enabled: boolean) => Promise<void>;
  onResetConfig: () => Promise<void>;
  configLoading: boolean;

  // Dialog management
  dialogs: DialogManager;

  // RBAC
  canManageTenant: boolean;
  canRegisterAdapter: boolean;

  // Navigation
  onNavigate?: (route: string) => void;

  // Metrics
  cpuUsage: number;
  memoryUsage: number;
  diskUsage: number;
  networkBandwidth: string;
  adapterCount: number;
  activeSessions: number;
  tokensPerSecond: number;
  latencyP95: number;
  connected: boolean;
  sseError: Error | null;
  systemMetrics: unknown;
  onReconnect: () => void;

  // Dashboard data
  loading: boolean;
  error: string | null;
  nodeCount: number;
  tenantCount: number;
  onFetchData: () => void;
  onClearError: () => void;

  // Dataset stats
  datasetStats: DatasetStats;
  datasetsLoading: boolean;
  datasetsError: Error | null;
  onRefetchDatasets: () => void;

  // Training stats
  runningJobs: number;
  completedLast7Days: number;
  recentTrainingJob: TrainingJob | null;
  recentCompletedJobWithStack: TrainingJob | null;
  trainingJobsLoading: boolean;
  trainingJobsError: Error | null;
  onRefetchTrainingJobs: () => void;

  // Adapter/Stack stats
  adapterTotal: number;
  stackTotal: number;
  stackNameLookup: Map<string, string>;
  defaultStack: AdapterStack | null;
  defaultStackLabel: string;
  adaptersLoading: boolean;
  stacksLoading: boolean;
  defaultStackLoading: boolean;
  adapterStackError: Error | null;
  onRefetchAdapters: () => void;
  onRefetchStacks: () => void;
  onRefetchDefaultStack: () => void;

  // Spacing from density controls
  spacing: { sectionGap: string };
}

/**
 * Get activity icon based on event type.
 */
function getActivityIcon(type: string) {
  switch (type) {
    case 'recovery':
      return CheckCircle;
    case 'policy':
      return Shield;
    case 'build':
      return Zap;
    case 'adapter':
      return Code;
    case 'telemetry':
      return Eye;
    case 'security':
      return Shield;
    case 'error':
      return AlertTriangle;
    default:
      return Activity;
  }
}

/**
 * Overview tab content for the dashboard.
 *
 * Combines workflow section, KPI cards, system resources,
 * activity feed, and quick actions.
 */
export const DashboardOverviewTab = memo(function DashboardOverviewTab({
  effectiveUser,
  effectiveTenant,
  selectedTenant,
  settingsOpen,
  onSettingsOpenChange,
  availableWidgetIds,
  userWidgetConfig,
  onUpdateWidgetVisibility,
  onResetConfig,
  configLoading,
  dialogs,
  canManageTenant,
  canRegisterAdapter,
  onNavigate,
  cpuUsage,
  memoryUsage,
  diskUsage,
  networkBandwidth,
  adapterCount,
  activeSessions,
  tokensPerSecond,
  latencyP95,
  connected,
  sseError,
  systemMetrics,
  onReconnect,
  loading,
  error,
  nodeCount,
  tenantCount,
  onFetchData,
  onClearError,
  datasetStats,
  datasetsLoading,
  datasetsError,
  onRefetchDatasets,
  runningJobs,
  completedLast7Days,
  recentTrainingJob,
  recentCompletedJobWithStack,
  trainingJobsLoading,
  trainingJobsError,
  onRefetchTrainingJobs,
  adapterTotal,
  stackTotal,
  stackNameLookup,
  defaultStack,
  defaultStackLabel,
  adaptersLoading,
  stacksLoading,
  defaultStackLoading,
  adapterStackError,
  onRefetchAdapters,
  onRefetchStacks,
  onRefetchDefaultStack,
  spacing,
}: DashboardOverviewTabProps) {
  const navigate = useNavigate();

  // Activity feed
  const {
    events: activityEvents,
    isLoading: activityLoading,
    error: activityError,
  } = useActivityFeed({
    enabled: true,
    maxEvents: 10,
    tenantId: effectiveTenant,
  });

  // Transform activity events to display format
  const recentActivity = useMemo(
    () =>
      activityEvents.map((event) => ({
        time: formatRelativeTime(event.timestamp),
        action: event.message,
        type: event.type,
        icon: getActivityIcon(event.type),
        severity: event.severity,
      })),
    [activityEvents]
  );

  // Quick actions
  const quickActions = useMemo(
    () => [
      {
        label: 'View System Health',
        icon: Activity,
        color: 'text-emerald-600',
        helpId: 'quick-action-health',
        onClick: () => dialogs.openDialog('health'),
      },
      {
        label: 'Create Workspace',
        icon: Users,
        color: 'text-blue-600',
        helpId: 'quick-action-create-tenant',
        disabled: !canManageTenant,
        disabledTitle: 'Requires workspace management permission',
        onClick: () => dialogs.openDialog('createTenant'),
      },
      {
        label: 'Deploy Adapter',
        icon: Code,
        color: 'text-violet-600',
        helpId: 'quick-action-deploy-adapter',
        disabled: !canRegisterAdapter,
        disabledTitle: 'Requires adapter:register permission',
        onClick: () => dialogs.openDialog('deployAdapter'),
      },
      {
        label: 'Review Policies',
        icon: Shield,
        color: 'text-amber-600',
        helpId: 'quick-action-policies',
        onClick: () =>
          onNavigate ? onNavigate('policies') : navigate(buildSecurityPoliciesLink()),
      },
    ],
    [canManageTenant, canRegisterAdapter, dialogs, onNavigate, navigate]
  );

  return (
    <div className={spacing.sectionGap}>
      {/* SSE Connection Error Alert */}
      {sseError && (
        <Alert variant="destructive">
          <AlertTriangle className="h-4 w-4" />
          <AlertTitle>Real-time Connection Error</AlertTitle>
          <AlertDescription className="flex items-center justify-between">
            <span>
              {sseError.message}. Falling back to polling for metrics updates.
            </span>
            {sseError.message.includes('failed after') && (
              <Button variant="outline" size="sm" onClick={onReconnect} className="ml-4">
                Reconnect
              </Button>
            )}
          </AlertDescription>
        </Alert>
      )}

      {/* SSE Disconnected Warning */}
      {!connected && !sseError && (
        <Alert
          variant="default"
          className="border-yellow-500 bg-yellow-50 dark:bg-yellow-950"
        >
          <AlertTriangle className="h-4 w-4 text-yellow-600" />
          <AlertTitle className="text-yellow-800 dark:text-yellow-200">
            Real-time Updates Disconnected
          </AlertTitle>
          <AlertDescription className="text-yellow-700 dark:text-yellow-300">
            Live metrics streaming is disconnected. Using polling for updates.
          </AlertDescription>
        </Alert>
      )}

      {/* Error Recovery */}
      {error &&
        errorRecoveryTemplates.genericError(error, () => {
          onClearError();
          onFetchData();
        })}

      {/* Using AdapterOS - Workflow Section */}
      <DashboardWorkflowSection
        effectiveTenant={effectiveTenant}
        defaultStackLabel={defaultStackLabel}
        datasetStats={datasetStats}
        datasetsLoading={datasetsLoading}
        datasetsError={datasetsError}
        onRefetchDatasets={onRefetchDatasets}
        runningJobs={runningJobs}
        completedLast7Days={completedLast7Days}
        recentTrainingJob={recentTrainingJob}
        trainingJobsLoading={trainingJobsLoading}
        trainingJobsError={trainingJobsError}
        onRefetchTrainingJobs={onRefetchTrainingJobs}
        adapterTotal={adapterTotal}
        stackTotal={stackTotal}
        stackNameLookup={stackNameLookup}
        defaultStack={defaultStack}
        adaptersStacksLoading={adaptersLoading || stacksLoading}
        adapterStackError={adapterStackError}
        onRefetchAdaptersStacks={() => {
          onRefetchAdapters();
          onRefetchStacks();
          onRefetchDefaultStack();
        }}
        recentCompletedJobWithStack={recentCompletedJobWithStack}
        defaultStackLoading={defaultStackLoading}
        defaultStackError={adapterStackError}
        onRefetchDefaultStack={onRefetchDefaultStack}
      />

      {/* System Overview Cards - KPI Grid */}
      <DashboardKpiCards
        nodeCount={nodeCount}
        tenantCount={tenantCount}
        adapterCount={adapterCount}
        activeSessions={activeSessions}
        tokensPerSecond={tokensPerSecond}
        latencyP95={latencyP95}
        loading={loading}
      />

      {/* Content Grid */}
      <ContentGrid>
        {/* System Resources */}
        <DashboardSystemResources
          cpuUsage={cpuUsage}
          memoryUsage={memoryUsage}
          diskUsage={diskUsage}
          networkBandwidth={networkBandwidth}
          connected={connected}
          hasMetrics={!!systemMetrics}
        />

        {/* Recent Activity */}
        <SectionErrorBoundary sectionName="Recent Activity">
          <Card className="card-standard">
            <CardHeader>
              <GlossaryTooltip termId="recent-activity">
                <CardTitle className="cursor-help">Recent Activity</CardTitle>
              </GlossaryTooltip>
            </CardHeader>
            <CardContent>
              {activityError ? (
                errorRecoveryTemplates.genericError(
                  activityError || 'Failed to load activity feed',
                  () => window.location.reload()
                )
              ) : (
                <div className="form-field">
                  {recentActivity.map((activity, index) => {
                    const Icon = activity.icon;
                    return (
                      <div key={index} className="flex-standard">
                        <div className="p-1 rounded-full bg-muted">
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
              )}
            </CardContent>
          </Card>
        </SectionErrorBoundary>

        {/* Base Model Status */}
        <SectionErrorBoundary sectionName="Base Model Status">
          <BaseModelStatusComponent selectedTenant={effectiveTenant} />
        </SectionErrorBoundary>

        {/* Plugin Status */}
        <SectionErrorBoundary sectionName="Plugin Status">
          <PluginStatusWidget />
        </SectionErrorBoundary>
      </ContentGrid>

      {/* Quick Actions */}
      <Card>
        <CardHeader>
          <GlossaryTooltip termId="quick-actions">
            <CardTitle className="cursor-help">Quick Actions</CardTitle>
          </GlossaryTooltip>
        </CardHeader>
        <CardContent>
          <ActionGrid actions={quickActions} columns={4} />
        </CardContent>
      </Card>

      {/* Dashboard Settings Modal */}
      <DashboardSettings
        open={settingsOpen}
        onOpenChange={onSettingsOpenChange}
        availableWidgetIds={availableWidgetIds}
        currentConfig={userWidgetConfig}
        onUpdateVisibility={onUpdateWidgetVisibility}
        onReset={onResetConfig}
        isUpdating={configLoading}
      />

      {/* System Health Dialog */}
      <DashboardHealthDialog
        open={dialogs.isOpen('health')}
        onClose={() => dialogs.closeDialog('health')}
        cpuUsage={cpuUsage}
        memoryUsage={memoryUsage}
        nodeCount={nodeCount}
        adapterCount={adapterCount}
        tokensPerSecond={tokensPerSecond}
        latencyP95={latencyP95}
      />
    </div>
  );
});
