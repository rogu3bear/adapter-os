// AdapterDetailPage - Full adapter detail view with tabs
// Displays comprehensive adapter information including overview, activations, lineage, manifest, and lifecycle controls

import React, { useState, useCallback, useEffect, useMemo } from 'react';
import { useParams, useNavigate } from 'react-router-dom';
import { ArrowLeft, RefreshCw, MoreHorizontal, Power, PowerOff, Pin, Trash2, Radio, Layers, Copy } from 'lucide-react';
import { toast } from 'sonner';

import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Label } from '@/components/ui/label';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
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
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { PageAsyncBoundary, SectionAsyncBoundary } from '@/components/shared/Feedback/AsyncBoundary';

import { useAdapterDetail, useAdapterOperations, useAdapterActions } from '@/hooks/adapters';
import { useRBAC } from '@/hooks/security/useRBAC';
import { PERMISSIONS } from '@/utils/rbac';
import { useAdaptersStream } from '@/hooks/streaming/useStreamingEndpoints';
import { getLifecycleVariant } from '@/utils/lifecycle';
import { logger } from '@/utils/logger';
import { isAdapterStateTransitionEvent, AdapterStreamEvent } from '@/api/streaming-types';
import { useAuth } from '@/providers/CoreProviders';
import { isDemoMvpMode } from '@/config/demo';

import AdapterOverview from './AdapterOverview';
import AdapterActivations from './AdapterActivations';
import AdapterLineage from './AdapterLineage';
import AdapterManifest from './AdapterManifest';
import AdapterLifecycle from './AdapterLifecycle';
import { TrainingSnapshotPanel } from '@/components/adapters/TrainingSnapshotPanel';
import { AddToStackModal } from '@/components/AddToStackModal';
import { PolicyPreflightDialog } from '@/components/PolicyPreflightDialog';
import type { PolicyPreflightResponse } from '@/api/policyTypes';
import { useAdapters } from '@/hooks/adapters';
import { ConfirmationModal } from '@/components/shared/Modal';
import { buildAdapterRecentActivity } from './adapterRecentActivity';
import { useLineage } from '@/hooks/observability/useLineage';
import { LineageViewer } from '@/components/lineage/LineageViewer';
import { buildAdaptersListLink, buildTrainingJobsLink, buildTrainingJobDetailLink, buildAdapterDetailLink, buildDatasetDetailLink } from '@/utils/navLinks';

type TabValue = 'overview' | 'evidence' | 'events' | 'activations' | 'lineage' | 'manifest' | 'lifecycle' | 'provenance';

export default function AdapterDetailPage() {
  return (
    <PageAsyncBoundary pageName="Adapter Detail">
      <AdapterDetailContent />
    </PageAsyncBoundary>
  );
}

function AdapterDetailContent() {
  const { adapterId } = useParams<{ adapterId: string }>();
  const navigate = useNavigate();
  const { can } = useRBAC();
  const { sessionMode } = useAuth();
  const demoMode = isDemoMvpMode(sessionMode);
  const [activeTab, setActiveTab] = useState<TabValue>('overview');
  const [showAddToStackModal, setShowAddToStackModal] = useState(false);
  const [lineageDirection, setLineageDirection] = useState<'both' | 'upstream' | 'downstream'>('both');
  const [includeEvidence, setIncludeEvidence] = useState(true);
  const [lineageCursors, setLineageCursors] = useState<Record<string, string>>({});

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
  const [showDuplicateModal, setShowDuplicateModal] = useState(false);
  const [baseVersion, setBaseVersion] = useState('');
  const [newVersion, setNewVersion] = useState('');
  const [defaultVersion, setDefaultVersion] = useState<string | null>(null);

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
  const { pinAdapter } = useAdapterOperations({
    onDataRefresh: refetch,
  });

  const {
    openAction,
    pendingAction,
    isConfirmOpen,
    setIsConfirmOpen,
    performAction,
    isRunning: isActionRunning,
    inlineStatuses,
    confirmationCopy,
  } = useAdapterActions({
    onRefetch: refetch,
    onDeleteSuccess: () => navigate(buildAdaptersListLink()),
    onShowPreflight: handleShowPreflight,
  });

  const { data: adaptersList = [] } = useAdapters();

  // Permission checks
  const canLoad = can('adapter:load');
  const canUnload = can('adapter:unload');
  const canDelete = can('adapter:delete');
  const canManageStacks = can('adapter:register'); // Use adapter:register as proxy for stack management

  // Handle back navigation
  const handleBack = () => {
    navigate(buildAdaptersListLink());
  };

  // Handle refresh
  const handleRefresh = async () => {
    await refetch();
    toast.success('Adapter data refreshed');
  };

  // Handle load/unload/delete through shared action hook
  const handleLoad = () => {
    if (!adapterId) return;
    openAction('load', {
      id: adapterId,
      name: adapter?.adapter?.name || adapter?.adapter?.adapter_name || adapterId,
      version: adapter?.adapter?.version || null,
      state: adapter?.current_state || adapter?.runtime_state || adapter?.adapter?.current_state || null,
    });
  };

  const handleUnload = () => {
    if (!adapterId) return;
    openAction('unload', {
      id: adapterId,
      name: adapter?.adapter?.name || adapter?.adapter?.adapter_name || adapterId,
      version: adapter?.adapter?.version || null,
      state: adapter?.current_state || adapter?.runtime_state || adapter?.adapter?.current_state || null,
    });
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
  const handleDelete = () => {
    if (!adapterId) return;
    openAction('delete', {
      id: adapterId,
      name: adapter?.adapter?.name || adapter?.adapter?.adapter_name || adapterId,
      version: adapter?.adapter?.version || null,
      state: adapter?.current_state || adapter?.adapter?.current_state || null,
    });
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
    const errorMessage = (error && typeof error === 'object' && 'message' in error)
      ? String((error as { message: unknown }).message)
      : 'Failed to load adapter';
    return (
      <FeatureLayout
        title="Adapter Detail"
        description="Error loading adapter"
        maxWidth="xl"
        contentPadding="default"
      >
        <ErrorRecovery
          error={errorMessage}
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
  const adapterVersion = adapterData?.version;
  const adapterHash = adapterData?.hash_b3 || adapter?.hash_b3 || adapter?.content_hash_b3;
  const adapterPrimaryId = adapterData?.adapter_id || adapterId || '';
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
  const kvConsistent = adapterData?.kv_consistent ?? false;
  const kvMessage = adapterData?.kv_message;
  const inlineStatus = adapterId ? inlineStatuses[adapterId] : undefined;
  const actionBusy = isActionRunning;

  const siblingVersions = useMemo(() => {
    if (!adapterName) return [];
    return (adaptersList || [])
      .filter((a: { name?: string; adapter_name?: string; adapter_id: string }) => (a.name || a.adapter_name || a.adapter_id) === adapterName)
      .sort((a: { created_at?: string }, b: { created_at?: string }) => {
        const aTs = a.created_at ? new Date(a.created_at).getTime() : 0;
        const bTs = b.created_at ? new Date(b.created_at).getTime() : 0;
        return bTs - aTs;
      });
  }, [adapterName, adaptersList]);

  const newestSiblingId = siblingVersions[0]?.adapter_id;
  const isNewestAdapter = newestSiblingId === adapterPrimaryId;

  const recentActivity = useMemo(
    () =>
      buildAdapterRecentActivity({
        adapterId: adapterPrimaryId,
        lineageHistory: lineage?.history,
        activations,
      }),
    [adapterPrimaryId, activations, lineage?.history],
  );

  const {
    data: lineageGraph,
    isLoading: isLoadingLineage,
    refetch: refetchLineage,
  } = useLineage('adapter_version', adapterPrimaryId, {
    params: {
      direction: lineageDirection,
      include_evidence: includeEvidence,
      limit_per_level: 6,
      cursors: lineageCursors,
    },
    enabled: Boolean(adapterPrimaryId),
  });

  useEffect(() => {
    if (siblingVersions.length > 0) {
      setBaseVersion(prev => prev || siblingVersions[0].version || adapterVersion || '');
    } else if (adapterVersion) {
      setBaseVersion(prev => prev || adapterVersion);
    }
  }, [adapterVersion, siblingVersions]);

  useEffect(() => {
    if (!adapterName || !adapterVersion) return;
    const storageKey = `adapter-default-version:${adapterName}`;
    const stored = localStorage.getItem(storageKey);
    if (stored) {
      setDefaultVersion(stored);
    }
  }, [adapterName, adapterVersion]);

  const handleSetDefaultVersion = () => {
    if (!adapterName || !adapterVersion) {
      toast.error('No version information available to set default');
      return;
    }
    const storageKey = `adapter-default-version:${adapterName}`;
    localStorage.setItem(storageKey, adapterVersion);
    setDefaultVersion(adapterVersion);
    toast.success(`Default version for new jobs set to v${adapterVersion}`);
  };

  const handleDuplicateSubmit = () => {
    if (!newVersion.trim()) {
      toast.error('Enter a new version to duplicate');
      return;
    }
    toast.info('Prepared duplicate request', {
      description: `Base version ${baseVersion || 'current'} → new version ${newVersion}`,
    });
    setShowDuplicateModal(false);
    setNewVersion('');
  };

  const handleNavigateLineageNode = useCallback(
    (node: { type?: string; id: string; href?: string }) => {
      if (node.href) {
        navigate(node.href);
        return;
      }
      switch (node.type) {
        case 'adapter_version':
          navigate(buildAdapterDetailLink(node.id));
          return;
        case 'training_job':
          navigate(buildTrainingJobDetailLink(node.id));
          return;
        case 'dataset_version':
        case 'dataset':
          navigate(buildDatasetDetailLink(node.id));
          return;
        case 'document':
          navigate(`/documents/${node.id}`);
          return;
        default:
          return;
      }
    },
    [navigate],
  );

  const handleLineageLoadMore = useCallback((level: { type: string; next_cursor?: string }) => {
    if (!level.next_cursor) return;
    const cursor = level.next_cursor;
    setLineageCursors((prev) => ({
      ...prev,
      [level.type]: cursor,
    }));
  }, []);

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
              {isNewestAdapter && (
                <Badge variant="default">
                  Newest
                </Badge>
              )}
                {adapterVersion && (
                  <Badge variant="outline">
                    v{adapterVersion}
                  </Badge>
                )}
                {adapterHash && (
                  <Badge variant="secondary">
                    b3 {adapterHash.slice(0, 8)}…
                  </Badge>
                )}
              {isPinned && (
                <Badge variant="secondary">
                  <Pin className="h-3 w-3 mr-1" />
                  Protected
                </Badge>
              )}
              <Badge variant={kvConsistent ? 'outline' : 'destructive'}>
                {kvConsistent ? 'KV Ready' : 'KV Stale'}
                {!kvConsistent && kvMessage ? `: ${kvMessage}` : ''}
              </Badge>
              {/* Streaming indicator */}
              <Badge variant={streamConnected ? 'default' : 'outline'} className="flex items-center gap-1">
                <Radio className={`h-3 w-3 ${streamConnected ? 'text-green-400 animate-pulse' : 'text-muted-foreground'}`} />
                {streamConnected ? 'Live' : 'Polling'}
              </Badge>
              {inlineStatus && (
                <Badge variant={inlineStatus.type === 'conflict' ? 'secondary' : 'destructive'}>
                  {inlineStatus.type === 'conflict' ? 'Changed elsewhere' : 'Action failed'}
                </Badge>
              )}
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
            <Button
              variant="outline"
              size="sm"
              onClick={() => setShowDuplicateModal(true)}
              disabled={!adapterVersion}
            >
              Duplicate from latest
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={handleSetDefaultVersion}
              disabled={!adapterVersion}
            >
              {defaultVersion === adapterVersion ? 'Default for new jobs (current)' : 'Set as default for new jobs'}
            </Button>

            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button variant="outline" size="sm" disabled={actionBusy}>
                  <MoreHorizontal className="h-4 w-4" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuItem
                  onClick={handleLoad}
                  disabled={!canLoad || currentState === 'resident' || actionBusy}
                >
                  <Power className="h-4 w-4 mr-2" />
                  Activate Adapter
                  <GlossaryTooltip brief="Activate adapter - load weights into GPU memory" />
                </DropdownMenuItem>
                <DropdownMenuItem
                  onClick={handleUnload}
                  disabled={!canUnload || currentState === 'unloaded' || actionBusy}
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
                  disabled={!canDelete || actionBusy}
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

        <Card>
          <CardHeader className="pb-3">
            <CardTitle>Next steps</CardTitle>
          </CardHeader>
            <CardContent className="flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
              <div className="text-sm text-muted-foreground">
              Train jobs for this adapter{demoMode ? '.' : ' or configure routing to activate it in the stack.'}
              </div>
              <div className="flex flex-wrap gap-2">
                {adapter?.lineage?.training_job_id && (
                  <Button variant="outline" size="sm" onClick={() => navigate(buildTrainingJobDetailLink(adapter.lineage!.training_job_id!))}>
                    Origin job
                  </Button>
                )}
                <Button size="sm" onClick={() => navigate(buildTrainingJobsLink({ adapterId }))}>
                  View training jobs
                </Button>
              {!demoMode && (
                <Button variant="outline" size="sm" onClick={() => navigate(`/router-config?adapterId=${adapterId}`)}>
                  Configure routing
                </Button>
              )}
              </div>
            </CardContent>
          </Card>

        {/* Tabs */}
        <Tabs value={activeTab} onValueChange={(v) => setActiveTab(v as TabValue)}>
          <TabsList className="grid w-full grid-cols-8">
            <TabsTrigger value="overview">Overview</TabsTrigger>
            <TabsTrigger value="evidence">Evidence</TabsTrigger>
            <TabsTrigger value="events">Events</TabsTrigger>
            <TabsTrigger value="activations">Activations</TabsTrigger>
            <TabsTrigger value="lineage">Lineage</TabsTrigger>
            <TabsTrigger value="manifest">Manifest</TabsTrigger>
            <TabsTrigger value="lifecycle">Lifecycle</TabsTrigger>
            <TabsTrigger value="provenance">Provenance</TabsTrigger>
          </TabsList>

          <TabsContent value="overview" className="mt-6">
            <SectionAsyncBoundary section="adapter-overview">
              <div className="space-y-4">
                <Card>
                  <CardHeader>
                    <CardTitle>Recent activity</CardTitle>
                  </CardHeader>
                  <CardContent>
                    {recentActivity.length ? (
                      <div className="space-y-3">
                        {recentActivity.map(event => (
                          <div key={`${event.label}-${event.timestamp}`} className="flex items-center justify-between text-sm">
                            <div className="flex items-center gap-2">
                              <Badge variant="outline">{event.label}</Badge>
                              <span className="text-foreground">{event.detail || 'Event recorded'}</span>
                            </div>
                            <span className="text-xs text-muted-foreground">
                              {new Date(event.timestamp).toLocaleString()}
                            </span>
                          </div>
                        ))}
                      </div>
                    ) : (
                      <div className="text-sm text-muted-foreground">No recent events.</div>
                    )}
                  </CardContent>
                </Card>
                <AdapterOverview
                  adapter={adapter}
                  health={health}
                  isLoading={isLoadingDetail}
                />
              </div>
            </SectionAsyncBoundary>
          </TabsContent>

          <TabsContent value="evidence" className="mt-6 space-y-4">
            <SectionAsyncBoundary section="adapter-evidence">
              <Card>
                <CardHeader>
                  <CardTitle>Policy pack and manifest</CardTitle>
                </CardHeader>
                <CardContent className="space-y-3 text-sm text-muted-foreground">
                  <EvidenceRow
                    label="Policy pack version"
                    value="unknown"
                    tooltip="Policy pack applied during admission and routing."
                  />
                  <EvidenceRow
                    label="Manifest hash (B3)"
                    value={manifest?.hash || adapter?.content_hash_b3 || 'unavailable'}
                    copyValue={manifest?.hash || adapter?.content_hash_b3 || undefined}
                    tooltip="Adapter manifest integrity hash (BLAKE3)."
                  />
                  <EvidenceRow
                    label="Signature"
                    value={adapter?.adapter?.signature_valid ? 'Valid' : 'not provided'}
                    tooltip="Signature accompanying the manifest payload."
                  />
                </CardContent>
              </Card>
              <AdapterManifest
                adapterId={adapterId}
                manifest={manifest}
                isLoading={isLoading}
              />
            </SectionAsyncBoundary>
          </TabsContent>

          <TabsContent value="events" className="mt-6 space-y-4">
            <SectionAsyncBoundary section="adapter-events">
              <AdapterLineage
                adapterId={adapterId}
                lineage={lineage}
                isLoading={isLoading}
              />
              <AdapterActivations
                adapterId={adapterId}
                activations={activations}
                isLoading={isLoading}
                onRefresh={refetch}
              />
            </SectionAsyncBoundary>
          </TabsContent>

          <TabsContent value="activations" className="mt-6">
            <SectionAsyncBoundary section="adapter-activations">
              <AdapterActivations
                adapterId={adapterId}
                activations={activations}
                isLoading={isLoading}
                onRefresh={refetch}
              />
            </SectionAsyncBoundary>
          </TabsContent>

          <TabsContent value="lineage" className="mt-6">
            <SectionAsyncBoundary section="adapter-lineage">
              <LineageViewer
                title="Adapter Lineage"
                data={lineageGraph ?? null}
                isLoading={isLoadingLineage}
                onRefresh={() => {
                  setLineageCursors({});
                  refetchLineage();
                }}
                direction={lineageDirection}
                includeEvidence={includeEvidence}
                onChangeDirection={setLineageDirection}
                onToggleEvidence={() => setIncludeEvidence((v) => !v)}
                onNavigateNode={handleNavigateLineageNode}
                onLoadMore={(level) => handleLineageLoadMore(level)}
              />
            </SectionAsyncBoundary>
          </TabsContent>

          <TabsContent value="manifest" className="mt-6">
            <SectionAsyncBoundary section="adapter-manifest">
              <AdapterManifest
                adapterId={adapterId}
                manifest={manifest}
                isLoading={isLoading}
              />
            </SectionAsyncBoundary>
          </TabsContent>

          <TabsContent value="lifecycle" className="mt-6">
            <SectionAsyncBoundary section="adapter-lifecycle">
              <AdapterLifecycle
                adapterId={adapterId}
                adapter={adapter}
                onPromote={promoteLifecycle}
                onDemote={demoteLifecycle}
                isPromoting={isPromoting}
                isDemoting={isDemoting}
              />
            </SectionAsyncBoundary>
          </TabsContent>

          <TabsContent value="provenance" className="mt-6">
            <SectionAsyncBoundary section="adapter-provenance">
              <TrainingSnapshotPanel adapterId={adapterId} />
            </SectionAsyncBoundary>
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

      {/* Duplicate from latest Modal */}
      <Dialog open={showDuplicateModal} onOpenChange={setShowDuplicateModal}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Duplicate adapter from latest</DialogTitle>
            <DialogDescription>
              Choose a base version and specify the new version to create.
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-4">
            <div className="space-y-2">
              <Label>Base version</Label>
              <Select value={baseVersion} onValueChange={setBaseVersion}>
                <SelectTrigger>
                  <SelectValue placeholder="Select base version" />
                </SelectTrigger>
                <SelectContent>
                  {siblingVersions.map((sibling: { adapter_id: string; version?: string }) => (
                    <SelectItem key={sibling.adapter_id} value={sibling.version || sibling.adapter_id}>
                      {sibling.version ? `v${sibling.version}` : 'unnamed'} • {sibling.adapter_id}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-2">
              <Label htmlFor="new-version">New version</Label>
              <Input
                id="new-version"
                value={newVersion}
                onChange={(e) => setNewVersion(e.target.value)}
                placeholder="e.g., 1.3.0"
              />
            </div>
            <div className="flex justify-end gap-2">
              <Button variant="outline" onClick={() => setShowDuplicateModal(false)}>
                Cancel
              </Button>
              <Button onClick={handleDuplicateSubmit}>
                Duplicate (prepare)
              </Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>

      {pendingAction && confirmationCopy && (
        <ConfirmationModal
          open={isConfirmOpen}
          onOpenChange={setIsConfirmOpen}
          title={confirmationCopy.title}
          description={confirmationCopy.description}
          confirmText={confirmationCopy.confirmText}
          confirmVariant={confirmationCopy.variant}
          onConfirm={performAction}
          isLoading={isActionRunning}
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
          isAdmin={can(PERMISSIONS.POLICY_OVERRIDE)}
          isLoading={isActionRunning}
        />
      )}
    </FeatureLayout>
  );
}

interface EvidenceRowProps {
  label: string;
  value: React.ReactNode;
  copyValue?: string;
  tooltip?: string;
}

function EvidenceRow({ label, value, copyValue, tooltip }: EvidenceRowProps) {
  const handleCopy = async () => {
    if (!copyValue) return;
    try {
      await navigator.clipboard.writeText(copyValue);
      toast.success(`${label} copied`);
    } catch {
      toast.error('Failed to copy');
    }
  };

  return (
    <div className="flex items-center justify-between gap-3">
      <div className="flex items-center gap-2 text-foreground">
        <Badge variant="secondary">{label}</Badge>
        {tooltip && <GlossaryTooltip brief={tooltip} />}
      </div>
      <div className="flex items-center gap-2 text-foreground">
        <span className="text-sm font-medium break-all">{value}</span>
        {copyValue && (
          <Button variant="ghost" size="icon" onClick={handleCopy} aria-label={`Copy ${label}`}>
            <Copy className="h-4 w-4" />
          </Button>
        )}
      </div>
    </div>
  );
}
