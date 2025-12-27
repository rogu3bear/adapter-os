import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from './ui/table';
import { Input } from './ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from './ui/select';
import { AdapterStateVisualization } from './AdapterStateVisualization';
import { AdapterMemoryMonitor } from './AdapterMemoryMonitor';
import { apiClient } from '@/api/services';
import { Adapter } from '@/api/types';
import { toast } from 'sonner';
import { logger } from '@/utils/logger';
import { formatCount, formatMB } from '@/utils';
import { usePolling } from '@/hooks/realtime/usePolling';
import { ErrorRecovery, errorRecoveryTemplates } from './ui/error-recovery';
import { EmptyState } from './ui/empty-state';
import { LoadingState } from './ui/loading-state';
import { AdapterListSkeleton } from '@/components/skeletons/AdapterListSkeleton';
import { TableLoadingState, RefreshingIndicator } from './ui/loading-patterns';
import { Code, Pin, ArrowUp, Trash2, MoreHorizontal, Upload, Power, PowerOff, ArrowUpDown, Loader2 } from 'lucide-react';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  DropdownMenuSeparator,
} from './ui/dropdown-menu';
import { useProgressiveHints } from '@/hooks/tutorial/useProgressiveHints';
import { getPageHints } from '@/data/page-hints';
import { ProgressiveHint } from './ui/progressive-hint';
import { useAdapterOperations, useAdapterActions, type AdapterSortColumn, type AdapterActionType } from '@/hooks/adapters';
import { getLifecycleVariant } from '@/utils/lifecycle';
import { useRBAC } from '@/hooks/security/useRBAC';
import { GlossaryTooltip } from './ui/glossary-tooltip';
import { PageErrorsProvider, PageErrors, usePageErrors } from '@/components/ui/page-error-boundary';
import { LIFECYCLE_STATE_LABELS } from '@/constants/terminology';
import { useAdapterFilterState } from '@/hooks/adapters/useAdapterFilterState';
import { ConfirmationModal } from '@/components/shared/Modal';
import { AdapterLifecycleState } from '@/api/system-state-types';
import { buildAdaptersRegisterLink, buildAdapterDetailLink, buildTrainingJobsLink, buildRouterConfigLink } from '@/utils/navLinks';
import { useAuth } from '@/providers/CoreProviders';
import { isDemoMvpMode } from '@/config/demo';
import { useUiMode } from '@/hooks/ui/useUiMode';
import { UiMode } from '@/config/ui-mode';

interface AdaptersData {
  adapters: Adapter[];
  totalMemory: number;
}

function AdaptersPageContent() {
  const { can, userRole, getUser } = useRBAC();
  const user = getUser?.();
  const { sessionMode } = useAuth();
  const { uiMode } = useUiMode();
  const isKernelMode = uiMode === UiMode.Kernel && user?.role?.toLowerCase() === 'developer';
  const demoMode = isDemoMvpMode(sessionMode);
  const { errors, addError, clearError } = usePageErrors();
  const navigate = useNavigate();

  const fetchAdaptersData = async (): Promise<AdaptersData> => {
    const adaptersData = await apiClient.listAdapters();
    const metrics = await apiClient.getSystemMetrics();
    const totalMemory = (metrics.memory_total_gb ?? 0) * 1024 * 1024 * 1024; // Convert GB to bytes
    return { adapters: adaptersData, totalMemory };
  };

  const { data, isLoading: loading, isFetching, error, refetch } = usePolling(
    fetchAdaptersData,
    'normal',
    {
      showLoadingIndicator: true,
      onError: (err) => {
        logger.error('Failed to fetch adapters', { component: 'AdaptersPage' }, err);
      }
    }
  );

  const isRefreshing = !loading && isFetching;
  const [forceMountingId, setForceMountingId] = useState<string | null>(null);

  const adapters = data?.adapters ?? [];
  const totalMemory = data?.totalMemory ?? 0;

  const {
    search,
    filters,
    sort,
    setSearch,
    updateFilters,
    setSort,
    resetFilters,
    applyFiltersAndSort,
  } = useAdapterFilterState({
    tenantId: user?.tenant_id,
    userId: user?.id || user?.user_id,
  });

  const displayedAdapters = useMemo(() => applyFiltersAndSort(adapters), [adapters, applyFiltersAndSort]);

  const newestAdapterIds = useMemo(() => {
    const latestByName = new Map<string, { id: string; ts: number }>();
    adapters.forEach((adapter) => {
      const key = adapter.name || adapter.adapter_name || adapter.adapter_id || adapter.id;
      const ts = adapter.created_at ? new Date(adapter.created_at).getTime() : 0;
      const current = latestByName.get(key);
      if (!current || ts > current.ts) {
        latestByName.set(key, { id: adapter.adapter_id || adapter.id, ts });
      }
    });
    return new Set(Array.from(latestByName.values()).map(v => v.id));
  }, [adapters]);

  const categoryOptions = useMemo(
    () => Array.from(new Set(adapters.map(adapter => adapter.category).filter(Boolean))) as string[],
    [adapters],
  );

  // Progressive hints
  const hints = getPageHints('adapters').map(hint => ({
    ...hint,
    condition: hint.id === 'empty-adapters'
      ? () => adapters.length === 0 && !loading
      : hint.condition
  }));
  const { visibleHints, dismissHint, getVisibleHint } = useProgressiveHints({
    pageKey: 'adapters',
    hints
  });
  const visibleHint = getVisibleHint();

  const handleSortChange = useCallback(
    (column: AdapterSortColumn) => {
      setSort(prev => ({
        column,
        direction: prev.column === column ? (prev.direction === 'asc' ? 'desc' : 'asc') : 'asc',
      }));
    },
    [setSort],
  );

  const {
    openAction,
    pendingAction,
    isConfirmOpen,
    setIsConfirmOpen,
    performAction,
    isRunning: isActionRunning,
    inlineStatuses,
    highlightedId,
    confirmationCopy,
  } = useAdapterActions({
    onRefetch: refetch,
  });

  const openAdapterAction = useCallback(
    (action: AdapterActionType, adapter: Adapter) => {
      const adapterId = adapter.id || adapter.adapter_id || adapter.adapter_name || '';
      if (!adapterId) return;
      openAction(action, {
        id: adapterId,
        name: adapter.name || adapter.adapter_name || adapterId,
        version: adapter.version,
        state: adapter.current_state || adapter.runtime_state,
      });
    },
    [openAction],
  );

  const handleForceMount = useCallback(async (adapterId: string) => {
    if (!adapterId) return;
    setForceMountingId(adapterId);
    try {
      await apiClient.loadAdapter(adapterId);
      toast.success('Force mounted adapter into VRAM');
      await refetch();
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Unable to mount adapter';
      toast.error(`Force mount failed: ${message}`);
    } finally {
      setForceMountingId(null);
    }
  }, [refetch]);

  // Adapter operations using shared hook
  const {
    operationError,
    clearOperationError,
    evictAdapter,
    pinAdapter,
    promoteAdapter,
  } = useAdapterOperations({
    onDataRefresh: refetch,
  });

  // Category memory limit updates (separate from adapter operations)
  const handleUpdateMemoryLimit = (category: string, limit: number) => {
    toast.info(`Memory limit updates for ${category} category will be available in a future release. Current limits are managed via category policies.`, {
      duration: 5000,
    });
    logger.info('Category memory limit update requested', {
      component: 'AdaptersPage',
      category,
      limit,
      note: 'Memory limits are currently managed through category policies, not direct updates'
    });
  };

  useEffect(() => {
    if (loading) {
      logger.debug('Adapters: showing loading state', { component: 'AdaptersPage' });
    }
  }, [loading]);

  useEffect(() => {
    if (error) {
      toast.error('Unable to load adapter data');
    }
  }, [error]);

  useEffect(() => {
    if (!loading && adapters.length === 0) {
      logger.info('Adapters: empty state displayed', { component: 'AdaptersPage' });
    }
  }, [adapters.length, loading]);

  // Show ErrorRecovery for major data loading failures
  if (error) {
    return (
      <ErrorRecovery
        error={error instanceof Error ? error.message : (error || "Unable to load adapter data. This may be due to a network issue or server problem.")}
        onRetry={refetch}
      />
    );
  }


  // Permission check helpers with canonical strings
  const canRegister = can('adapter:register');
  const canStartTraining = can('training:start');
  const canLoad = can('adapter:load');
  const canUnload = can('adapter:unload');
  const canDelete = can('adapter:delete');

  return (
    <div className="space-y-6">
      {/* Consolidated Error Display */}
      <PageErrors errors={errors} />

      <Card>
        <CardHeader>
          <CardTitle>Next steps</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          {adapters.length === 0 ? (
            <div className="text-sm text-muted-foreground">
              No adapters yet. Register an adapter or start training to create one.
            </div>
          ) : (
            <div className="text-sm text-muted-foreground">
              Keep going: train adapters from datasets and configure routing to activate them.
            </div>
          )}
          <div className="flex flex-wrap gap-2">
            <Button variant="outline" size="sm" onClick={() => navigate(buildAdaptersRegisterLink())} disabled={!canRegister}>
              Register adapter
            </Button>
            <Button size="sm" onClick={() => navigate(buildTrainingJobsLink())} disabled={!canStartTraining}>
              Start training
            </Button>
            {!demoMode && (
              <Button variant="outline" size="sm" onClick={() => navigate(buildRouterConfigLink())}>
                Configure routing
              </Button>
            )}
          </div>
        </CardContent>
      </Card>

      {visibleHint && (
        <ProgressiveHint
          title={visibleHint.hint.title}
          content={visibleHint.hint.content}
          onDismiss={() => dismissHint(visibleHint.hint.id)}
          placement={visibleHint.hint.placement}
        />
      )}

      {operationError && (
        <ErrorRecovery
          error={typeof operationError === 'string' ? operationError : String(operationError)}
          onRetry={clearOperationError || (() => {})}
        />
      )}

      {/* Page Header with Actions */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <h1 className="text-2xl font-bold">Adapters</h1>
          <GlossaryTooltip termId="adapter" variant="icon" />
          {isRefreshing && <RefreshingIndicator />}
        </div>
        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            disabled={!canRegister}
            title={!canRegister ? 'Requires adapter:register permission' : 'Register a new adapter'}
            onClick={() => navigate(buildAdaptersRegisterLink())}
          >
            <Upload className="h-4 w-4 mr-2" />
            Register
            <GlossaryTooltip brief="Register a new LoRA adapter from weights file" />
          </Button>
          <Button
            disabled={!canStartTraining}
            title={!canStartTraining ? 'Requires training:start permission' : 'Train a new adapter'}
            onClick={() => navigate(buildTrainingJobsLink())}
          >
            Train New Adapter
            <GlossaryTooltip brief="Start the training wizard to create a new LoRA adapter from documents" />
          </Button>
        </div>
      </div>

      <div className="mt-4 flex flex-wrap items-center justify-between gap-3 rounded-md border bg-card/50 p-3">
        <div className="flex flex-wrap items-center gap-2">
          <Input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Search adapters by name or ID"
            className="w-64"
          />
          <Select
            value={filters.state || 'any'}
            onValueChange={(value) => updateFilters({ state: value === 'any' ? undefined : value })}
          >
            <SelectTrigger className="w-[150px]">
              <SelectValue placeholder="State" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="any">Any state</SelectItem>
              <SelectItem value="resident">Resident</SelectItem>
              <SelectItem value="active">Active</SelectItem>
              <SelectItem value="loading">Loading</SelectItem>
              <SelectItem value="unloaded">Unloaded</SelectItem>
            </SelectContent>
          </Select>
          <Select
            value={filters.category || 'any'}
            onValueChange={(value) => updateFilters({ category: value === 'any' ? undefined : value })}
          >
            <SelectTrigger className="w-[160px]">
              <SelectValue placeholder="Category" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="any">All categories</SelectItem>
              {categoryOptions.map(option => (
                <SelectItem key={option} value={option}>
                  {option}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Button
            variant={filters.pinnedOnly ? 'default' : 'outline'}
            size="sm"
            onClick={() => updateFilters({ pinnedOnly: !filters.pinnedOnly })}
          >
            Protected only
          </Button>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Select
            value={sort.column}
            onValueChange={(value) => handleSortChange(value as AdapterSortColumn)}
          >
            <SelectTrigger className="w-[160px]">
              <SelectValue placeholder="Sort by" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="name">Name</SelectItem>
              <SelectItem value="state">State</SelectItem>
              <SelectItem value="memory">Memory</SelectItem>
              <SelectItem value="activations">Activations</SelectItem>
              <SelectItem value="created_at">Created</SelectItem>
            </SelectContent>
          </Select>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setSort(prev => ({ ...prev, direction: prev.direction === 'asc' ? 'desc' : 'asc' }))}
          >
            <ArrowUpDown className="mr-2 h-4 w-4" />
            {sort.direction === 'asc' ? 'Ascending' : 'Descending'}
          </Button>
          <Button variant="outline" size="sm" onClick={() => { resetFilters(); setSort({ column: 'name', direction: 'asc' }); setSearch(''); }}>
            Reset filters
          </Button>
        </div>
      </div>

      {/* Visualizations */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
        {(() => {
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          const stateRecords: any[] = adapters.map((a, idx) => ({
            adapter_id: a.adapter_id || a.id,
            adapter_idx: idx,
            state: a.current_state ?? 'unloaded',
            pinned: a.pinned ?? false,
            memory_bytes: a.memory_bytes ?? 0,
            category: a.category ?? 'code',
            scope: a.scope ?? 'global',
            last_activated: a.last_activated,
            activation_count: a.activation_count ?? 0,
          }));
          return <AdapterStateVisualization adapters={stateRecords} totalMemory={totalMemory} />;
        })()}
        <AdapterMemoryMonitor
          adapters={adapters}
          totalMemory={totalMemory}
          onEvictAdapter={evictAdapter}
          onPinAdapter={pinAdapter}
          onUpdateMemoryLimit={handleUpdateMemoryLimit}
        />
      </div>

      {/* Adapter Cards with quick actions */}
      {displayedAdapters.length > 0 && (
        <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
          {displayedAdapters.map((adapter) => {
            const adapterKey = adapter.id || adapter.adapter_id || adapter.adapter_name || '';
            const rowStatus = adapterKey ? inlineStatuses[adapterKey] : undefined;
            const isHighlighted = adapterKey ? highlightedId === adapterKey : false;
            const status = adapter.current_state || adapter.lifecycle_state || 'pending';
            const statusLabel = (() => {
              if (status === 'resident' || status === 'active') return 'Active';
              if (status === 'loading' || status === 'training') return 'Training';
              return 'Pending';
            })();
            const statusVariant =
              statusLabel === 'Active'
                ? 'default'
                : statusLabel === 'Training'
                  ? 'secondary'
                  : 'outline';
            const memoryMb = adapter.memory_bytes
              ? (adapter.memory_bytes / 1024 / 1024).toFixed(1)
              : '—';

            return (
              <Card
                key={`adapter-card-${adapter.id}`}
                className={`border-border/70 transition-shadow duration-200 hover:shadow-lg ${isHighlighted ? 'ring-1 ring-amber-400/60' : ''}`}
              >
                <CardHeader className="space-y-2">
                  <div className="flex items-start justify-between gap-2">
                    <div className="space-y-1">
                      <CardTitle className="text-base leading-tight">{adapter.name}</CardTitle>
                      <div className="flex flex-wrap gap-2 text-[11px] text-muted-foreground">
                        {adapter.version && <Badge variant="outline">v{adapter.version}</Badge>}
                        {adapter.hash_b3 && <Badge variant="secondary">b3 {adapter.hash_b3.slice(0, 8)}…</Badge>}
                        <span className="truncate max-w-[200px]">{adapter.id}</span>
                      </div>
                    </div>
                    <div className="flex items-center gap-2">
                      <Badge variant={statusVariant}>{statusLabel}</Badge>
                      {newestAdapterIds.has(adapter.adapter_id || adapter.id) && (
                        <Badge variant="default">Newest</Badge>
                      )}
                      {adapter.pinned && <Pin className="h-4 w-4 text-muted-foreground" />}
                    </div>
                  </div>
                  {rowStatus && (
                    <div className="flex items-center gap-2 text-xs text-muted-foreground">
                      <Badge variant={rowStatus.type === 'conflict' ? 'secondary' : 'destructive'}>
                        {rowStatus.type === 'conflict' ? 'Changed elsewhere' : 'Action failed'}
                      </Badge>
                      <span className="truncate">{rowStatus.message}</span>
                    </div>
                  )}
                  <div className="flex flex-wrap gap-2 text-xs">
                    <Badge variant="outline">{adapter.tier || 'tier_1'}</Badge>
                    <Badge variant="secondary">{adapter.category || 'general'}</Badge>
                    <Badge variant="outline">Rank {adapter.rank ?? 'n/a'}</Badge>
                  </div>
                </CardHeader>
                <CardContent className="space-y-3">
                  <div className="flex items-center justify-between text-xs text-muted-foreground">
                    <span>Memory</span>
                    <span>{memoryMb} MB</span>
                  </div>
                  <div className="flex items-center justify-between text-xs text-muted-foreground">
                    <span>Activations</span>
                    <span>{formatCount(adapter.activation_count)}</span>
                  </div>
                  <div className="flex flex-wrap gap-2">
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={() => navigate(buildAdapterDetailLink(adapter.id))}
                    >
                      View
                    </Button>
                    {!demoMode && (
                      <Button
                        size="sm"
                        variant="outline"
                        onClick={() => navigate(`${buildRouterConfigLink()}?adapterId=${adapter.id}`)}
                      >
                        Configure
                      </Button>
                    )}
                    {isKernelMode && (
                      <Button
                        size="sm"
                        variant="secondary"
                        onClick={() => handleForceMount(adapterKey)}
                        disabled={forceMountingId === adapterKey}
                        title="Hot-Swap: force mount into VRAM"
                        data-cy="force-mount"
                      >
                        {forceMountingId === adapterKey ? 'Mounting…' : 'Force Mount'}
                      </Button>
                    )}
                    <Button
                      size="sm"
                      variant="ghost"
                      disabled={!canLoad && !canUnload}
                      onClick={() => {
                        if (adapter.current_state === 'resident') {
                          openAdapterAction('unload', adapter);
                        } else {
                          openAdapterAction('load', adapter);
                        }
                      }}
                    >
                      {adapter.current_state === 'resident'
                        ? `Deactivate${adapter.version ? ` v${adapter.version}` : ''}`
                        : `Activate${adapter.version ? ` v${adapter.version}` : ''}`}
                    </Button>
                  </div>
                </CardContent>
              </Card>
            );
          })}
        </div>
      )}

      {/* Adapter Table */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            Deployed Adapters
            <GlossaryTooltip brief="List of all registered LoRA adapters with their current state and metrics" />
          </CardTitle>
        </CardHeader>
        <CardContent>
          {loading ? (
            <TableLoadingState rows={10} />
          ) : displayedAdapters.length === 0 ? (
            <EmptyState
              icon={Code}
              title="No adapters deployed"
              description={adapters.length === 0 ? 'Train or import an adapter to get started. Your fleet will appear here once deployed.' : 'No adapters match your current filters.'}
              actionLabel={adapters.length === 0 && canStartTraining ? 'Start Training' : undefined}
              onAction={adapters.length === 0 && canStartTraining ? () => navigate(buildTrainingJobsLink()) : undefined}
            />
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>
                    Name
                    <GlossaryTooltip termId="adapter-name" />
                  </TableHead>
                  <TableHead>
                    Tier
                    <GlossaryTooltip termId="adapter-tier" />
                  </TableHead>
                  <TableHead>
                    Rank
                    <GlossaryTooltip termId="adapter-rank" />
                  </TableHead>
                  <TableHead>
                    Lifecycle
                    <GlossaryTooltip termId="adapter-lifecycle" />
                  </TableHead>
                  <TableHead>
                    State
                    <GlossaryTooltip termId="adapter-state" />
                  </TableHead>
                  <TableHead>
                    Memory
                    <GlossaryTooltip termId="adapter-memory" />
                  </TableHead>
                  <TableHead>
                    Activation
                    <GlossaryTooltip termId="adapter-activation" />
                  </TableHead>
                  <TableHead>
                    Actions
                    <GlossaryTooltip termId="adapter-actions" />
                  </TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {displayedAdapters.map(adapter => {
                  const adapterKey = adapter.id || adapter.adapter_id || adapter.adapter_name || '';
                  const rowStatus = adapterKey ? inlineStatuses[adapterKey] : undefined;
                  const isHighlighted = adapterKey ? highlightedId === adapterKey : false;

                  return (
                    <TableRow key={adapter.id} className={isHighlighted ? 'bg-amber-50/70' : undefined}>
                      <TableCell className="font-medium">
                        <div className="flex flex-col gap-1">
                          <span>{adapter.name}</span>
                          <div className="flex flex-wrap gap-2 text-[11px] text-muted-foreground">
                            {adapter.version && (
                              <Badge variant="outline">v{adapter.version}</Badge>
                            )}
                            {adapter.hash_b3 && (
                              <Badge variant="secondary">b3 {adapter.hash_b3.slice(0, 8)}…</Badge>
                            )}
                            <span className="truncate max-w-[200px]">{adapter.id}</span>
                          </div>
                          {rowStatus && (
                            <div className="flex items-center gap-2 text-xs text-muted-foreground">
                              <Badge variant={rowStatus.type === 'conflict' ? 'secondary' : 'destructive'}>
                                {rowStatus.type === 'conflict' ? 'Changed elsewhere' : 'Action failed'}
                              </Badge>
                              <span className="truncate">{rowStatus.message}</span>
                            </div>
                          )}
                        </div>
                      </TableCell>
                      <TableCell>
                        <Badge variant="outline">{adapter.tier || 'tier_1'}</Badge>
                      </TableCell>
                      <TableCell className="text-sm text-muted-foreground">
                        {adapter.rank || 16}
                      </TableCell>
                      <TableCell>
                        <Badge variant={getLifecycleVariant(adapter.lifecycle_state)}>
                          {adapter.lifecycle_state || 'active'}
                        </Badge>
                      </TableCell>
                      <TableCell>
                        {adapter.current_state && (
                          <Badge>{LIFECYCLE_STATE_LABELS[adapter.current_state] || adapter.current_state}</Badge>
                        )}
                        {adapter.pinned && <Pin className="h-4 w-4 ml-2" />}
                      </TableCell>
                      <TableCell>{formatMB(adapter.memory_bytes, 1)}</TableCell>
                      <TableCell>{formatCount(adapter.activation_count)}</TableCell>
                      <TableCell>
                        <DropdownMenu>
                          <DropdownMenuTrigger asChild>
                            <Button variant="ghost" size="sm">
                              <MoreHorizontal className="h-4 w-4" />
                            </Button>
                          </DropdownMenuTrigger>
                          <DropdownMenuContent align="end">
                            {/* Load/Unload actions */}
                            <DropdownMenuItem
                              onClick={() => openAdapterAction('load', adapter)}
                              disabled={!canLoad || adapter.current_state === 'resident' || isActionRunning}
                              title={!canLoad ? 'Requires adapter:load permission' : 'Load adapter into memory'}
                            >
                              <Power className="mr-2 h-4 w-4" />
                              {`Load${adapter.version ? ` v${adapter.version}` : ''}`}
                              <GlossaryTooltip brief="Load adapter weights into GPU memory for inference" />
                            </DropdownMenuItem>
                            <DropdownMenuItem
                              onClick={() => openAdapterAction('unload', adapter)}
                              disabled={!canUnload || adapter.current_state === 'unloaded' || isActionRunning}
                              title={!canUnload ? 'Requires adapter:unload permission' : 'Unload adapter from memory'}
                            >
                              <PowerOff className="mr-2 h-4 w-4" />
                              {`Unload${adapter.version ? ` v${adapter.version}` : ''}`}
                              <GlossaryTooltip brief="Remove adapter from GPU memory (can be reloaded)" />
                            </DropdownMenuItem>

                            <DropdownMenuSeparator />

                            {/* Promote action */}
                            <DropdownMenuItem
                              onClick={() => promoteAdapter(adapter.id)}
                              disabled={!canLoad}
                              title={!canLoad ? 'Requires adapter:load permission' : 'Promote adapter to higher tier'}
                            >
                              <ArrowUp className="mr-2 h-4 w-4" />
                              Promote
                              <GlossaryTooltip brief="Increase adapter tier for higher routing priority" />
                            </DropdownMenuItem>

                            {/* Pin/Unpin action */}
                            <DropdownMenuItem
                              onClick={() => pinAdapter(adapter.id, !adapter.pinned)}
                              disabled={!canLoad}
                              title={!canLoad ? 'Requires adapter:load permission' : adapter.pinned ? 'Allow adapter removal' : 'Protect adapter'}
                            >
                              <Pin className="mr-2 h-4 w-4" />
                              {adapter.pinned ? 'Allow Removal' : 'Protect Adapter'}
                              <GlossaryTooltip brief={adapter.pinned ? 'Allow adapter to be removed when memory is needed' : 'Prevent adapter from being removed during memory pressure'} />
                            </DropdownMenuItem>

                            {/* Evict action */}
                            <DropdownMenuItem
                              onClick={() => evictAdapter(adapter.id)}
                              disabled={!canUnload || adapter.pinned}
                              title={!canUnload ? 'Requires adapter:unload permission' : adapter.pinned ? 'Cannot remove protected adapter' : 'Remove adapter'}
                            >
                              <Trash2 className="mr-2 h-4 w-4" />
                              Evict
                              <GlossaryTooltip brief="Force remove adapter from memory to free resources" />
                            </DropdownMenuItem>

                            <DropdownMenuSeparator />

                            {/* Delete action (destructive) */}
                            <DropdownMenuItem
                              onClick={() => openAdapterAction('delete', adapter)}
                              disabled={!canDelete || isActionRunning}
                              title={!canDelete ? 'Requires adapter:delete permission' : 'Permanently delete adapter'}
                              className="text-destructive focus:text-destructive"
                            >
                              <Trash2 className="mr-2 h-4 w-4" />
                              Delete
                              <GlossaryTooltip brief="Permanently remove adapter and weights from the system" />
                            </DropdownMenuItem>
                            <DropdownMenuSeparator />
                            <DropdownMenuItem onClick={() => navigate(buildTrainingJobsLink({ adapterId: adapter.id }))}>
                              <Upload className="mr-2 h-4 w-4" />
                              View training jobs
                            </DropdownMenuItem>
                            {!demoMode && (
                              <DropdownMenuItem onClick={() => navigate(`${buildRouterConfigLink()}?adapterId=${adapter.id}`)}>
                                <ArrowUp className="mr-2 h-4 w-4" />
                                Configure routing
                              </DropdownMenuItem>
                            )}
                          </DropdownMenuContent>
                        </DropdownMenu>
                      </TableCell>
                    </TableRow>
                  );
                })}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>

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
    </div>
  );
}

// Wrap with PageErrorsProvider
export function AdaptersPage() {
  return (
    <PageErrorsProvider>
      <AdaptersPageContent />
    </PageErrorsProvider>
  );
}
