// AdapterDetailPage - Full adapter detail view with tabs
// Displays comprehensive adapter information including overview, activations, lineage, manifest, and lifecycle controls

import React, { useState, useCallback, useEffect, useMemo } from 'react';
import { useParams, useNavigate } from 'react-router-dom';
import { ArrowLeft, RefreshCw, MoreHorizontal, Power, PowerOff, Pin, Trash2, Radio, Layers } from 'lucide-react';
import { toast } from 'sonner';

import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { LoadingState } from '@/components/ui/loading-state';
import { ErrorRecovery } from '@/components/ui/error-recovery';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';
import FeatureLayout from '@/layout/FeatureLayout';

import { useAdapterDetail } from '@/hooks/useAdapterDetail';
import { useAdapterOperations } from '@/hooks/useAdapterOperations';
import { useRBAC } from '@/hooks/useRBAC';
import { useAdaptersStream } from '@/hooks/useStreamingEndpoints';
import { getLifecycleVariant } from '@/utils/lifecycle';
import { logger } from '@/utils/logger';
import { isAdapterStateTransitionEvent, AdapterStreamEvent } from '@/api/streaming-types';

import AdapterOverview from './AdapterOverview';
import AdapterActivations from './AdapterActivations';
import AdapterLineage from './AdapterLineage';
import AdapterManifest from './AdapterManifest';
import AdapterLifecycle from './AdapterLifecycle';
import { TrainingSnapshotPanel } from '@/components/adapters/TrainingSnapshotPanel';
import { AddToStackModal } from '@/components/AddToStackModal';
import { PolicyPreflightDialog } from '@/components/PolicyPreflightDialog';
import type { PolicyPreflightResponse } from '@/api/policyTypes';

type TabValue = 'overview' | 'activations' | 'lineage' | 'manifest' | 'lifecycle' | 'provenance';

export default function AdapterDetailPage() {
  const { adapterId } = useParams<{ adapterId: string }>();
  const navigate = useNavigate();
  const { can } = useRBAC();
  const [activeTab, setActiveTab] = useState<TabValue>('overview');
  const [showAddToStackModal, setShowAddToStackModal] = useState(false);

  // Preflight dialog state
  const [showPreflightDialog, setShowPreflightDialog] = useState(false);
  const [preflightResult, setPreflightResult] = useState<PolicyPreflightResponse | null>(null);
  const [preflightOperation, setPreflightOperation] = useState<'load' | 'unload'>('load');
  const [preflightResolve, setPreflightResolve] = useState<((value: boolean) => void) | null>(null);

  // Streaming state for real-time adapter updates
  const [streamingState, setStreamingState] = useState<{
    currentState?: string;
    lifecycleState?: string;
    isPinned?: boolean;
  } | null>(null);

  // Ref to store refetch function for use in streaming callback
  const refetchRef = React.useRef<(() => void) | null>(null);

  // Fetch adapter data using React Query hook
  const {
    adapter,
    lineage,
    activations,
    manifest,
    health,
    isLoading,
    isLoadingDetail,
    error,
    refetch,
    promoteLifecycle,
    demoteLifecycle,
    isPromoting,
    isDemoting,
  } = useAdapterDetail(adapterId || '', {
    enabled: !!adapterId,
    // Reduce polling frequency when streaming is connected (will be updated after streamConnected is available)
    refetchInterval: 30000,
    onError: (err) => {
      logger.error('Failed to fetch adapter details', { component: 'AdapterDetailPage', adapterId }, err);
    },
  });

  if (error) {
    return (
      <FeatureLayout
        title="Adapter Detail"
        description={`Adapter ID: ${adapterId}`}
        maxWidth="xl"
        contentPadding="default"
      >
        <ErrorRecovery
          error={`Unable to load adapter: ${error.message}`}
          onRetry={refetch}
        />
      </FeatureLayout>
    );
  }

  if (!adapter && isLoadingDetail) {
    return (
      <FeatureLayout
        title="Adapter Detail"
        description={`Adapter ID: ${adapterId}`}
        maxWidth="xl"
        contentPadding="default"
      >
        <LoadingState />
      </FeatureLayout>
    );
  }

  if (!adapter && !isLoadingDetail) {
    return (
      <FeatureLayout
        title="Adapter Detail"
        description={`Adapter ID: ${adapterId}`}
        maxWidth="xl"
        contentPadding="default"
      >
        <ErrorRecovery
          error="Adapter not found. This adapter could not be located. It may have been deleted."
          onRetry={refetch}
        />
      </FeatureLayout>
    );
  }

  // Keep refetch ref updated
  useEffect(() => {
    refetchRef.current = refetch;
  }, [refetch]);

  // Memoize onMessage callback to prevent unnecessary reconnections
  const handleStreamMessage = useCallback((event: AdapterStreamEvent) => {
    // Handle state transitions for this adapter
    if (isAdapterStateTransitionEvent(event) && event.adapter_id === adapterId) {
      setStreamingState(prev => ({
        ...prev,
        currentState: event.current_state,
      }));
      // Show toast for state transitions
      toast.info(`Adapter state: ${event.previous_state || 'unknown'} -> ${event.current_state}`);
    }
    // Handle pin events
    if ('action' in event && (event.action === 'pinned' || event.action === 'unpinned') && event.adapter_id === adapterId) {
      setStreamingState(prev => ({
        ...prev,
        isPinned: event.action === 'pinned',
      }));
    }
    // Handle health events
    if ('status' in event && 'issue' in event && event.adapter_id === adapterId) {
      // Health event - trigger a refetch to get full details
      refetchRef.current?.();
    }
  }, [adapterId]);

  // Connect to adapter stream for real-time updates
  const {
    connected: streamConnected,
    error: streamError,
    lastUpdated: streamLastUpdated,
  } = useAdaptersStream({
    enabled: !!adapterId,
    onMessage: handleStreamMessage,
  });

  // Clear streaming state when adapter changes
  useEffect(() => {
    setStreamingState(null);
  }, [adapterId]);

  // Preflight dialog handler
  const handleShowPreflight = async (
    id: string,
    operation: 'load' | 'unload',
    result: PolicyPreflightResponse
  ): Promise<boolean> => {
    return new Promise((resolve) => {
      setPreflightResult(result);
      setPreflightOperation(operation);
      setPreflightResolve(() => resolve);
      setShowPreflightDialog(true);
    });
  };

  // Handle preflight dialog proceed
  const handlePreflightProceed = () => {
    if (preflightResolve) {
      preflightResolve(true);
      setPreflightResolve(null);
    }
    setShowPreflightDialog(false);
  };

  // Handle preflight dialog cancel
  const handlePreflightCancel = () => {
    if (preflightResolve) {
      preflightResolve(false);
      setPreflightResolve(null);
    }
    setShowPreflightDialog(false);
  };

  // Adapter operations
  const {
    loadAdapter,
    unloadAdapter,
    pinAdapter,
    deleteAdapter,
    isOperationLoading,
  } = useAdapterOperations({
    onDataRefresh: refetch,
    onShowPreflight: handleShowPreflight,
  });

  // Permission checks
  const canLoad = can('adapter:load');
  const canUnload = can('adapter:unload');
  const canDelete = can('adapter:delete');
  const canManageStacks = can('adapter:register'); // Use adapter:register as proxy for stack management

  // Handle back navigation
  const handleBack = () => {
    navigate('/adapters');
  };

  // Handle refresh
  const handleRefresh = async () => {
    await refetch();
    toast.success('Adapter data refreshed');
  };

  // Handle load adapter
  const handleLoad = async () => {
    if (!adapterId || !loadAdapter) return;
    try {
      await loadAdapter(adapterId);
      toast.success('Adapter loaded successfully');
    } catch (err) {
      logger.error('Failed to load adapter', { component: 'AdapterDetailPage', adapterId }, err as Error);
    }
  };

  // Handle unload adapter
  const handleUnload = async () => {
    if (!adapterId || !unloadAdapter) return;
    try {
      await unloadAdapter(adapterId);
      toast.success('Adapter unloaded successfully');
    } catch (err) {
      logger.error('Failed to unload adapter', { component: 'AdapterDetailPage', adapterId }, err as Error);
    }
  };

  // Handle pin/unpin adapter
  const handleTogglePin = async () => {
    if (!adapterId || !adapter) return;
    const isPinned = adapter.adapter?.pinned ?? false;
    try {
      await pinAdapter(adapterId, !isPinned);
      toast.success(isPinned ? 'Adapter can now be removed when memory is needed' : 'Adapter is now protected and will stay in memory');
    } catch (err) {
      logger.error('Failed to toggle pin', { component: 'AdapterDetailPage', adapterId }, err as Error);
    }
  };

  // Handle delete adapter
  const handleDelete = async () => {
    if (!adapterId) return;
    if (!window.confirm('Are you sure you want to delete this adapter? This action cannot be undone.')) {
      return;
    }
    try {
      await deleteAdapter(adapterId);
      toast.success('Adapter deleted successfully');
      navigate('/adapters');
    } catch (err) {
      logger.error('Failed to delete adapter', { component: 'AdapterDetailPage', adapterId }, err as Error);
    }
  };

  // Loading state
  if (isLoading && !adapter) {
    return (
      <FeatureLayout
        title="Loading Adapter..."
        description="Fetching adapter details"
        maxWidth="xl"
        contentPadding="default"
      >
        <LoadingState
          title="Loading adapter details"
          description="Fetching metadata, lineage, and activation history"
          skeletonLines={8}
        />
      </FeatureLayout>
    );
  }

  // Error state
  if (error && !adapter) {
    return (
      <FeatureLayout
        title="Adapter Detail"
        description="Error loading adapter"
        maxWidth="xl"
        contentPadding="default"
      >
        <ErrorRecovery
          error={error.message}
          onRetry={refetch}
        />
      </FeatureLayout>
    );
  }

  // Not found state
  if (!adapterId) {
    return (
      <FeatureLayout
        title="Adapter Not Found"
        description="The requested adapter could not be found"
        maxWidth="xl"
        contentPadding="default"
      >
        <div className="text-center py-12">
          <p className="text-muted-foreground">No adapter ID provided</p>
          <Button onClick={handleBack} className="mt-4">
            <ArrowLeft className="h-4 w-4 mr-2" />
            Back to Adapters
          </Button>
        </div>
      </FeatureLayout>
    );
  }

  // Extract adapter info - merge with streaming state for real-time updates
  const adapterData = adapter?.adapter;
  const adapterName = adapterData?.name || adapterData?.adapter_name || adapterId;
  // Streaming state takes precedence for real-time fields
  const currentState =
    streamingState?.currentState ||
    adapter?.runtime_state ||
    adapter?.current_state ||
    adapterData?.runtime_state ||
    adapterData?.current_state ||
    'unknown';
  const lifecycleState =
    streamingState?.lifecycleState ||
    adapter?.lifecycle_state ||
    adapterData?.lifecycle_state ||
    'active';
  const isPinned = streamingState?.isPinned ?? adapterData?.pinned ?? false;

  return (
    <FeatureLayout
      title={adapterName}
      description={`Adapter ID: ${adapterId}`}
      maxWidth="xl"
      contentPadding="default"
    >
      <div className="space-y-6">
        {/* Header */}
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-4">
            <Button variant="ghost" size="sm" onClick={handleBack}>
              <ArrowLeft className="h-4 w-4 mr-2" />
              Back
            </Button>
            <div className="flex items-center gap-2">
              <GlossaryTooltip termId="adapter" variant="icon" />
              <Badge variant={getLifecycleVariant(lifecycleState)}>
                {lifecycleState}
              </Badge>
              <Badge variant="outline">{currentState}</Badge>
              {isPinned && (
                <Badge variant="secondary">
                  <Pin className="h-3 w-3 mr-1" />
                  Protected
                </Badge>
              )}
              {/* Streaming indicator */}
              <Badge variant={streamConnected ? 'default' : 'outline'} className="flex items-center gap-1">
                <Radio className={`h-3 w-3 ${streamConnected ? 'text-green-400 animate-pulse' : 'text-muted-foreground'}`} />
                {streamConnected ? 'Live' : 'Polling'}
              </Badge>
            </div>
          </div>

          <div className="flex items-center gap-2">
            {/* Last updated indicator when streaming */}
            {streamLastUpdated && streamConnected && (
              <span className="text-xs text-muted-foreground">
                Updated: {new Date(streamLastUpdated).toLocaleTimeString()}
              </span>
            )}
            <Button
              variant="outline"
              size="sm"
              onClick={handleRefresh}
              disabled={isLoadingDetail}
            >
              <RefreshCw className={`h-4 w-4 mr-2 ${isLoadingDetail ? 'animate-spin' : ''}`} />
              Refresh
            </Button>

            <Button
              variant="outline"
              size="sm"
              onClick={() => setShowAddToStackModal(true)}
              disabled={!canManageStacks}
            >
              <Layers className="h-4 w-4 mr-2" />
              Add to Stack
            </Button>

            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button variant="outline" size="sm" disabled={isOperationLoading}>
                  <MoreHorizontal className="h-4 w-4" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuItem
                  onClick={handleLoad}
                  disabled={!canLoad || currentState === 'resident'}
                >
                  <Power className="h-4 w-4 mr-2" />
                  Activate Adapter
                  <GlossaryTooltip brief="Activate adapter - load weights into GPU memory" />
                </DropdownMenuItem>
                <DropdownMenuItem
                  onClick={handleUnload}
                  disabled={!canUnload || currentState === 'unloaded'}
                >
                  <PowerOff className="h-4 w-4 mr-2" />
                  Deactivate Adapter
                  <GlossaryTooltip brief="Deactivate adapter - remove from GPU memory" />
                </DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem onClick={handleTogglePin} disabled={!canLoad}>
                  <Pin className="h-4 w-4 mr-2" />
                  {isPinned ? 'Allow Removal' : 'Protect Adapter'}
                  <GlossaryTooltip brief={isPinned ? 'Allow removal when memory is needed' : 'Keep in memory always'} />
                </DropdownMenuItem>
                <DropdownMenuItem
                  onClick={() => setShowAddToStackModal(true)}
                  disabled={!canManageStacks}
                >
                  <Layers className="h-4 w-4 mr-2" />
                  Add to Stack
                  <GlossaryTooltip brief="Add this adapter to an existing or new stack" />
                </DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem
                  onClick={handleDelete}
                  disabled={!canDelete}
                  className="text-destructive focus:text-destructive"
                >
                  <Trash2 className="h-4 w-4 mr-2" />
                  Delete Adapter
                  <GlossaryTooltip brief="Permanently remove adapter" />
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        </div>

        {/* Tabs */}
        <Tabs value={activeTab} onValueChange={(v) => setActiveTab(v as TabValue)}>
          <TabsList className="grid w-full grid-cols-6">
            <TabsTrigger value="overview">Overview</TabsTrigger>
            <TabsTrigger value="activations">Activations</TabsTrigger>
            <TabsTrigger value="lineage">Lineage</TabsTrigger>
            <TabsTrigger value="manifest">Manifest</TabsTrigger>
            <TabsTrigger value="lifecycle">Lifecycle</TabsTrigger>
            <TabsTrigger value="provenance">Provenance</TabsTrigger>
          </TabsList>

          <TabsContent value="overview" className="mt-6">
            <AdapterOverview
              adapter={adapter}
              health={health}
              isLoading={isLoadingDetail}
            />
          </TabsContent>

          <TabsContent value="activations" className="mt-6">
            <AdapterActivations
              adapterId={adapterId}
              activations={activations}
              isLoading={isLoading}
              onRefresh={refetch}
            />
          </TabsContent>

          <TabsContent value="lineage" className="mt-6">
            <AdapterLineage
              adapterId={adapterId}
              lineage={lineage}
              isLoading={isLoading}
            />
          </TabsContent>

          <TabsContent value="manifest" className="mt-6">
            <AdapterManifest
              adapterId={adapterId}
              manifest={manifest}
              isLoading={isLoading}
            />
          </TabsContent>

          <TabsContent value="lifecycle" className="mt-6">
            <AdapterLifecycle
              adapterId={adapterId}
              adapter={adapter}
              onPromote={promoteLifecycle}
              onDemote={demoteLifecycle}
              isPromoting={isPromoting}
              isDemoting={isDemoting}
            />
          </TabsContent>

          <TabsContent value="provenance" className="mt-6">
            <TrainingSnapshotPanel adapterId={adapterId} />
          </TabsContent>
        </Tabs>
      </div>

      {/* Add to Stack Modal */}
      {adapterId && (
        <AddToStackModal
          open={showAddToStackModal}
          onOpenChange={setShowAddToStackModal}
          adapterId={adapterId}
        />
      )}

      {/* Policy Preflight Dialog */}
      {preflightResult && (
        <PolicyPreflightDialog
          open={showPreflightDialog}
          onOpenChange={setShowPreflightDialog}
          title={`Policy Validation - ${preflightOperation === 'load' ? 'Activate' : 'Deactivate'} Adapter`}
          description={`The following policies will be enforced when ${preflightOperation === 'load' ? 'activating' : 'deactivating'} this adapter`}
          checks={preflightResult.checks}
          canProceed={preflightResult.canProceed}
          onProceed={handlePreflightProceed}
          onCancel={handlePreflightCancel}
          isAdmin={can('policy:override')}
          isLoading={isOperationLoading}
        />
      )}
    </FeatureLayout>
  );
}
