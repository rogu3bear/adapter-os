/**
 * Owner Home Page - Redesigned
 *
 * Clean 2-column layout with collapsible sidebar for system owners.
 * Provides system health at a glance with clear task prioritization.
 *
 * Layout:
 * - Desktop: Main content (8 cols) + Collapsible Sidebar (4 cols)
 * - Mobile: Single column with FAB for Chat/CLI
 *
 * Citations:
 * - docs/PRD_OWNER_HOME_IMPLEMENTATION.md: Owner Home Implementation
 * - CLAUDE.md: RBAC section (Admin role = System Owner)
 */

import React, { useState, useCallback, useMemo } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import { Crown, RefreshCw, ExternalLink, PlusCircle } from 'lucide-react';
import { toast } from 'sonner';

import { useAuth } from '@/providers/CoreProviders';
import PageWrapper from '@/layout/PageWrapper';
import apiClient from '@/api/client';
import type { ModelWithStatsResponse } from '@/api/types';
import type { MetricsSnapshotEvent } from '@/api/streaming-types';

import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';
import { LiveDataBadge } from '@/components/ui/live-data-badge';

// New components
import { StatusBar } from './components/StatusBar';
import { AlertHero } from './components/AlertHero';
import { OnboardingStrip } from './components/OnboardingStrip';
import { ActiveModelCard } from './components/ActiveModelCard';
import { SystemKpiGrid } from './components/SystemKpiGrid';
import { CollapsibleSidebar } from './components/CollapsibleSidebar';

// Existing components (kept)
import ActivityCard from './components/ActivityCard';
import { SystemChatWidget } from './components/SystemChatWidget';
import { CliConsole } from './components/CliConsole';

import { useSystemState } from '@/hooks/useSystemState';
import { useLiveData } from '@/hooks/useLiveData';

const MODEL_STATUS_VALUES = ['loaded', 'available', 'loading', 'error'] as const;
type ModelStatusValue = (typeof MODEL_STATUS_VALUES)[number];

function parseModelStatus(status?: string | null): ModelStatusValue | undefined {
  if (!status) {
    return undefined;
  }
  const normalized = status.trim().toLowerCase();
  return MODEL_STATUS_VALUES.includes(normalized as ModelStatusValue)
    ? (normalized as ModelStatusValue)
    : undefined;
}

export default function OwnerHomePage() {
  const navigate = useNavigate();
  const { user } = useAuth();
  const [activeRightTab, setActiveRightTab] = useState<'chat' | 'cli'>('chat');

  // Fetch system overview
  const {
    data: systemOverview,
    isLoading: systemLoading,
    error: systemError,
    refetch: refetchSystem,
  } = useQuery({
    queryKey: ['owner-system-overview'],
    queryFn: () => apiClient.getSystemOverview(),
    staleTime: 10000,
    refetchInterval: 30000,
  });

  // Fetch tenants
  const {
    data: tenants,
    isLoading: tenantsLoading,
    refetch: refetchTenants,
  } = useQuery({
    queryKey: ['owner-tenants'],
    queryFn: () => apiClient.listTenants(),
    staleTime: 30000,
  });

  // Fetch adapters
  const {
    data: adapters,
    isLoading: adaptersLoading,
    refetch: refetchAdapters,
  } = useQuery({
    queryKey: ['owner-adapters'],
    queryFn: () => apiClient.listAdapters(),
    staleTime: 15000,
  });

  // Fetch adapter stacks
  const {
    data: stacks,
    isLoading: stacksLoading,
    refetch: refetchStacks,
  } = useQuery({
    queryKey: ['owner-stacks'],
    queryFn: () => apiClient.listAdapterStacks(),
    staleTime: 30000,
  });

  // Fetch base models
  const {
    data: rawModels,
    isLoading: modelsLoading,
    refetch: refetchModels,
  } = useQuery<ModelWithStatsResponse[]>({
    queryKey: ['owner-models'],
    queryFn: () => apiClient.listModels(),
    staleTime: 30000,
  });

  // Fetch base model status
  const {
    data: baseModelStatus,
    isLoading: baseModelLoading,
    refetch: refetchBaseModel,
  } = useQuery({
    queryKey: ['owner-base-model-status'],
    queryFn: () => apiClient.getBaseModelStatus(),
    staleTime: 15000,
    refetchInterval: 30000,
  });

  // Fetch ground truth system state (memory pressure, top adapters)
  const {
    data: systemState,
    isLoading: systemStateLoading,
    error: systemStateError,
    isLive: systemStateIsLive,
    lastUpdated: systemStateLastUpdated,
    refetch: refetchSystemState,
  } = useSystemState({
    enabled: true,
    pollingInterval: 10000,
    topAdapters: 5,
  });

  // SSE stream for live metrics (CPU, memory, disk)
  const [sseMetrics, setSseMetrics] = useState<MetricsSnapshotEvent | null>(null);
  const handleMetricsMessage = useCallback((data: unknown) => {
    if (data && typeof data === 'object' && 'system' in data) {
      setSseMetrics(data as MetricsSnapshotEvent);
    }
  }, []);

  const {
    sseConnected: metricsConnected,
    connectionStatus: metricsConnectionStatus,
    lastUpdated: metricsLastUpdated,
    freshnessLevel: metricsFreshness,
    reconnect: reconnectMetrics,
  } = useLiveData<MetricsSnapshotEvent>({
    sseEndpoint: '/v1/stream/metrics',
    sseEventType: 'metrics',
    fetchFn: async () => {
      return sseMetrics || ({} as MetricsSnapshotEvent);
    },
    pollingSpeed: 'normal',
    enabled: true,
    onSSEMessage: handleMetricsMessage,
    operationName: 'owner-metrics-stream',
  });

  // Merge SSE metrics with polled system overview
  const enhancedSystemOverview = useMemo(() => {
    if (!systemOverview) return systemOverview;
    if (!sseMetrics?.system) return systemOverview;
    return {
      ...systemOverview,
      resource_usage: {
        ...systemOverview.resource_usage,
        cpu_usage_percent: sseMetrics.system.cpu_percent,
        memory_usage_percent: sseMetrics.system.memory_percent,
      },
    };
  }, [systemOverview, sseMetrics]);

  // Map OpenAIModelInfo to BaseModel format
  const models = React.useMemo(() => {
    if (!Array.isArray(rawModels)) return [];
    return rawModels.map((model) => ({
      id: model.id,
      name: model.name || model.id,
      size_bytes: model.size_bytes ?? undefined,
      format: model.format ?? undefined,
      status: parseModelStatus(model.import_status),
      path: model.model_path ?? undefined,
    }));
  }, [rawModels]);

  // Derive active stack from stacks list
  const activeStack = React.useMemo(() => {
    if (!Array.isArray(stacks)) return null;
    return stacks.find((s) => s.is_default) || stacks[0] || null;
  }, [stacks]);

  // Adapter count for onboarding
  const adapterCount = Array.isArray(adapters) ? adapters.length : 0;
  const hasModel = !!baseModelStatus?.model_name;

  // Refresh all data
  const handleRefresh = async () => {
    await Promise.all([
      refetchSystem(),
      refetchTenants(),
      refetchAdapters(),
      refetchStacks(),
      refetchModels(),
      refetchBaseModel(),
      refetchSystemState(),
    ]);
    toast.success('Dashboard refreshed');
  };

  const isLoading =
    systemLoading || tenantsLoading || adaptersLoading || stacksLoading || modelsLoading;

  return (
    <div className="min-h-full bg-slate-50">
      <PageWrapper
        pageKey="owner-home"
        title="Owner Home (Legacy)"
        description={`Welcome, ${user?.display_name || user?.email}`}
        maxWidth="xl"
        contentPadding="default"
        customHeader={
          <div className="flex flex-col sm:flex-row items-start sm:items-center justify-between gap-4">
            <div className="flex items-center gap-3">
              <Crown className="h-7 w-7 text-amber-500" />
              <div>
                <h1 className="text-xl font-bold text-slate-900">Owner Home (Legacy)</h1>
                <p className="text-sm text-slate-600">
                  Welcome, {user?.display_name || user?.email}
                </p>
              </div>
              <Badge
                variant="default"
                className="ml-2 bg-amber-500 hover:bg-amber-600 hidden sm:flex"
              >
                Legacy
              </Badge>
            </div>
            <div className="flex flex-wrap items-center gap-2">
              <LiveDataBadge
                isLive={metricsConnected}
                connectionStatus={metricsConnectionStatus}
                freshnessLevel={metricsFreshness}
                lastUpdated={metricsLastUpdated}
                onReconnect={reconnectMetrics}
              />
              <Button
                variant="outline"
                size="sm"
                onClick={() => navigate('/dashboard')}
              >
                Dashboard
              </Button>
              <Button
                variant="outline"
                size="sm"
                onClick={() => navigate('/testing')}
              >
                Testing
              </Button>
              <Button
                variant="outline"
                size="sm"
                onClick={() => navigate('/replay')}
              >
                Replay
              </Button>
              <Button
                variant="outline"
                size="sm"
                onClick={() => navigate('/security/policies')}
              >
                Guardrails
              </Button>
              <Button
                variant="default"
                size="sm"
                onClick={() => navigate('/create-adapter')}
                className="bg-blue-600 hover:bg-blue-700"
              >
                <PlusCircle className="h-4 w-4 mr-1.5" />
                <span className="hidden sm:inline">Create Adapter</span>
                <span className="sm:hidden">Create</span>
              </Button>
              <Button
                variant="outline"
                size="sm"
                onClick={handleRefresh}
                disabled={isLoading}
              >
                <RefreshCw
                  className={`h-4 w-4 ${isLoading ? 'animate-spin' : ''} sm:mr-1.5`}
                />
                <span className="hidden sm:inline">Refresh</span>
              </Button>
            </div>
          </div>
        }
      >
        {/* Main Layout: Content + Sidebar */}
        <div className="flex gap-6">
          {/* Main Content */}
          <div className="flex-1 min-w-0 space-y-6">
            {/* Status Bar */}
            <SectionErrorBoundary sectionName="Status Bar">
              <StatusBar
                systemOverview={enhancedSystemOverview}
                baseModelStatus={baseModelStatus}
                adapters={Array.isArray(adapters) ? adapters : []}
                systemState={systemState}
                isLoading={systemLoading}
                error={systemError}
                isLive={metricsConnected}
              />
            </SectionErrorBoundary>

            {/* Alert Hero (conditional) */}
            <SectionErrorBoundary sectionName="Alerts">
              <AlertHero
                systemOverview={enhancedSystemOverview}
                baseModelStatus={baseModelStatus}
                systemState={systemState}
              />
            </SectionErrorBoundary>

            {/* Onboarding (conditional based on user state) */}
            <SectionErrorBoundary sectionName="Onboarding">
              <OnboardingStrip adapterCount={adapterCount} hasModel={hasModel} />
            </SectionErrorBoundary>

            {/* Command Center: Model + Activity */}
            <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
              <SectionErrorBoundary sectionName="Active Model">
                <ActiveModelCard
                  models={Array.isArray(models) ? models : []}
                  isLoading={modelsLoading}
                  onRefresh={refetchModels}
                />
              </SectionErrorBoundary>

              <SectionErrorBoundary sectionName="Activity">
                <ActivityCard />
              </SectionErrorBoundary>
            </div>

            {/* System KPIs */}
            <SectionErrorBoundary sectionName="System Overview">
              <SystemKpiGrid
                systemOverview={enhancedSystemOverview}
                systemState={systemState}
                adapters={Array.isArray(adapters) ? adapters : []}
                stacks={Array.isArray(stacks) ? stacks : []}
                tenants={Array.isArray(tenants) ? tenants : []}
                isLoading={systemLoading || adaptersLoading || stacksLoading}
              />
            </SectionErrorBoundary>
          </div>

          {/* Collapsible Sidebar */}
          <CollapsibleSidebar defaultExpanded={true}>
            <div className="h-full flex flex-col">
              <Tabs
                value={activeRightTab}
                onValueChange={(v) => setActiveRightTab(v as 'chat' | 'cli')}
                className="flex flex-col h-full"
              >
                <div className="border-b px-3 py-2">
                  <TabsList className="grid w-full grid-cols-2">
                    <TabsTrigger value="chat">Chat</TabsTrigger>
                    <TabsTrigger value="cli">CLI</TabsTrigger>
                  </TabsList>
                </div>
                <TabsContent value="chat" className="flex-1 m-0 overflow-hidden">
                  <SectionErrorBoundary sectionName="System Chat">
                    <SystemChatWidget
                      systemOverview={systemOverview}
                      adapters={Array.isArray(adapters) ? adapters : []}
                      baseModelStatus={baseModelStatus}
                      activeStack={activeStack}
                    />
                  </SectionErrorBoundary>
                </TabsContent>
                <TabsContent value="cli" className="flex-1 m-0 overflow-hidden">
                  <SectionErrorBoundary sectionName="CLI Console">
                    <CliConsole />
                  </SectionErrorBoundary>
                </TabsContent>
              </Tabs>
            </div>
          </CollapsibleSidebar>
        </div>
      </PageWrapper>
    </div>
  );
}
