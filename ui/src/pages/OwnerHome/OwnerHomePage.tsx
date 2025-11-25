/**
 * Owner Home Page
 *
 * Unified dashboard for System Owner / Root operator providing:
 * - System overview with health metrics
 * - Tenant and adapter stack summaries
 * - Embedded system chat and CLI console
 * - Model load/unload/download controls
 *
 * This is a composition layer aggregating existing functionality
 * into a single "god view" for system owners.
 *
 * Citations:
 * - docs/PRD_OWNER_HOME_IMPLEMENTATION.md: PRD-OH-01
 * - CLAUDE.md: RBAC section (Admin role = System Owner)
 */

import React, { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import { Crown, RefreshCw, ExternalLink } from 'lucide-react';
import { toast } from 'sonner';

import { useAuth } from '@/providers/CoreProviders';
import apiClient from '@/api/client';

import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Skeleton } from '@/components/ui/skeleton';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';

import SystemHealthStrip from './components/SystemHealthStrip';
import SystemOverviewCard from './components/SystemOverviewCard';
import TenantsCard from './components/TenantsCard';
import { StacksAdaptersCard } from './components/StacksAdaptersCard';
import { ModelControlPanel } from './components/ModelControlPanel';
import ActivityCard from './components/ActivityCard';
import UsageCard from './components/UsageCard';
import { SystemChatWidget } from './components/SystemChatWidget';
import { CliConsole } from './components/CliConsole';
import { OnboardingStrip } from './components/OnboardingStrip';

export default function OwnerHomePage() {
  const navigate = useNavigate();
  const { user } = useAuth();
  const [refreshKey, setRefreshKey] = useState(0);
  const [activeRightTab, setActiveRightTab] = useState<'chat' | 'cli'>('chat');

  // Fetch system overview
  const {
    data: systemOverview,
    isLoading: systemLoading,
    error: systemError,
    refetch: refetchSystem,
  } = useQuery({
    queryKey: ['owner-system-overview', refreshKey],
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
    queryKey: ['owner-tenants', refreshKey],
    queryFn: () => apiClient.listTenants(),
    staleTime: 30000,
  });

  // Fetch adapters
  const {
    data: adapters,
    isLoading: adaptersLoading,
    refetch: refetchAdapters,
  } = useQuery({
    queryKey: ['owner-adapters', refreshKey],
    queryFn: () => apiClient.listAdapters(),
    staleTime: 15000,
  });

  // Fetch adapter stacks
  const {
    data: stacks,
    isLoading: stacksLoading,
    refetch: refetchStacks,
  } = useQuery({
    queryKey: ['owner-stacks', refreshKey],
    queryFn: () => apiClient.listAdapterStacks(),
    staleTime: 30000,
  });

  // Fetch base models
  const {
    data: rawModels,
    isLoading: modelsLoading,
    refetch: refetchModels,
  } = useQuery({
    queryKey: ['owner-models', refreshKey],
    queryFn: () => apiClient.listModels(),
    staleTime: 30000,
  });

  // Map OpenAIModelInfo to BaseModel format
  const models = React.useMemo(() => {
    if (!Array.isArray(rawModels)) return [];
    return rawModels.map(model => ({
      id: model.id,
      name: model.id, // Use id as name since OpenAIModelInfo doesn't have name field
      size_bytes: undefined,
      format: undefined,
      status: undefined,
      path: undefined,
    }));
  }, [rawModels]);

  // Check if first-time user for onboarding
  const isFirstTimeUser = React.useMemo(() => {
    const tenantsArray = Array.isArray(tenants) ? tenants : [];
    const adaptersArray = Array.isArray(adapters) ? adapters : [];
    return tenantsArray.length <= 1 && adaptersArray.length === 0 && models.length === 0;
  }, [tenants, adapters, models]);

  // Refresh all data
  const handleRefresh = async () => {
    setRefreshKey((prev) => prev + 1);
    await Promise.all([
      refetchSystem(),
      refetchTenants(),
      refetchAdapters(),
      refetchStacks(),
      refetchModels(),
    ]);
    toast.success('Dashboard refreshed');
  };

  const isLoading = systemLoading || tenantsLoading || adaptersLoading || stacksLoading || modelsLoading;

  return (
    <div className="min-h-screen bg-slate-50">
      <div className="max-w-[1800px] mx-auto px-4 sm:px-6 lg:px-8 py-6">
        {/* System Health Strip (Top) */}
        <SectionErrorBoundary sectionName="System Health">
          <SystemHealthStrip
            systemOverview={systemOverview}
            isLoading={systemLoading}
            error={systemError}
          />
        </SectionErrorBoundary>

        {/* Header */}
        <div className="mt-6 mb-6 flex items-center justify-between">
          <div className="flex items-center gap-3">
            <Crown className="h-8 w-8 text-amber-500" />
            <div>
              <h1 className="text-2xl font-bold text-slate-900">Owner Home</h1>
              <p className="text-sm text-slate-600">
                Welcome, {user?.display_name || user?.email}. Full system access.
              </p>
            </div>
            <Badge variant="default" className="ml-4 bg-amber-500 hover:bg-amber-600">
              System Owner
            </Badge>
          </div>
          <div className="flex items-center gap-2">
            <Button
              variant="outline"
              size="sm"
              onClick={handleRefresh}
              disabled={isLoading}
            >
              <RefreshCw className={`h-4 w-4 mr-2 ${isLoading ? 'animate-spin' : ''}`} />
              Refresh
            </Button>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => navigate('/dashboard')}
            >
              Standard Dashboard
              <ExternalLink className="h-4 w-4 ml-2" />
            </Button>
          </div>
        </div>

        {/* Onboarding Strip (show for first-time users) */}
        {isFirstTimeUser && !isLoading && (
          <SectionErrorBoundary sectionName="Onboarding">
            <OnboardingStrip />
          </SectionErrorBoundary>
        )}

        {/* Main Content Grid */}
        <div className="grid grid-cols-12 gap-6">
          {/* Left Column - System & Tenant Overview (3 cols) */}
          <div className="col-span-12 lg:col-span-3 space-y-4">
            <SectionErrorBoundary sectionName="System Overview">
              <SystemOverviewCard
                systemOverview={systemOverview}
                isLoading={systemLoading}
              />
            </SectionErrorBoundary>

            <SectionErrorBoundary sectionName="Tenants">
              <TenantsCard
                tenants={Array.isArray(tenants) ? tenants : []}
                isLoading={tenantsLoading}
              />
            </SectionErrorBoundary>

            <SectionErrorBoundary sectionName="Stacks & Adapters">
              <StacksAdaptersCard
                stacks={Array.isArray(stacks) ? stacks : []}
                adapters={Array.isArray(adapters) ? adapters : []}
                isLoading={stacksLoading || adaptersLoading}
              />
            </SectionErrorBoundary>
          </div>

          {/* Center Column - Models & Activity (4 cols) */}
          <div className="col-span-12 lg:col-span-4 space-y-4">
            <SectionErrorBoundary sectionName="Model Control">
              <ModelControlPanel
                models={Array.isArray(models) ? models : []}
                adapters={Array.isArray(adapters) ? adapters : []}
                isLoading={modelsLoading || adaptersLoading}
                onRefresh={refetchModels}
              />
            </SectionErrorBoundary>

            <SectionErrorBoundary sectionName="Activity">
              <ActivityCard refreshKey={refreshKey} />
            </SectionErrorBoundary>

            <SectionErrorBoundary sectionName="Usage">
              <UsageCard refreshKey={refreshKey} />
            </SectionErrorBoundary>
          </div>

          {/* Right Column - Chat & CLI (5 cols) */}
          <div className="col-span-12 lg:col-span-5">
            <div className="bg-white rounded-lg border shadow-sm h-[calc(100vh-220px)] min-h-[600px] flex flex-col">
              <Tabs
                value={activeRightTab}
                onValueChange={(v) => setActiveRightTab(v as 'chat' | 'cli')}
                className="flex flex-col h-full"
              >
                <div className="border-b px-4 py-2">
                  <TabsList className="grid w-full grid-cols-2">
                    <TabsTrigger value="chat">System Chat</TabsTrigger>
                    <TabsTrigger value="cli">CLI Console</TabsTrigger>
                  </TabsList>
                </div>
                <TabsContent value="chat" className="flex-1 m-0 overflow-hidden">
                  <SectionErrorBoundary sectionName="System Chat">
                    <SystemChatWidget
                      systemOverview={systemOverview}
                      adapters={Array.isArray(adapters) ? adapters : []}
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
          </div>
        </div>
      </div>
    </div>
  );
}
