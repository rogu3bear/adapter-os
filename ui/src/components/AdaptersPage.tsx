import React, { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from './ui/table';
import { AdapterStateVisualization } from './AdapterStateVisualization';
import { AdapterMemoryMonitor } from './AdapterMemoryMonitor';
import apiClient from '@/api/client';
import { Adapter } from '@/api/types';
import { toast } from 'sonner';
import { logger } from '@/utils/logger';
import { usePolling } from '@/hooks/usePolling';
import { ErrorRecovery, errorRecoveryTemplates } from './ui/error-recovery';
import { EmptyState } from './ui/empty-state';
import { LoadingState } from './ui/loading-state';
import { Code, MemoryStick, Activity, Clock, Pin, ArrowUp, Trash2, MoreHorizontal, Upload, Power, PowerOff } from 'lucide-react';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  DropdownMenuSeparator,
} from './ui/dropdown-menu';
import { useProgressiveHints } from '@/hooks/useProgressiveHints';
import { getPageHints } from '@/data/page-hints';
import { ProgressiveHint } from './ui/progressive-hint';
import { useAdapterOperations } from '@/hooks/useAdapterOperations';
import { getLifecycleVariant } from '@/utils/lifecycle';
import { useRBAC } from '@/hooks/useRBAC';
import { GlossaryTooltip } from './ui/glossary-tooltip';
import { PageErrorsProvider, PageErrors, usePageErrors } from '@/components/ui/page-error-boundary';
import { LIFECYCLE_STATE_LABELS } from '@/constants/terminology';
import { AdapterLifecycleState } from '@/api/system-state-types';

interface AdaptersData {
  adapters: Adapter[];
  totalMemory: number;
}

function AdaptersPageContent() {
  const { can, userRole } = useRBAC();
  const { errors, addError, clearError } = usePageErrors();
  const navigate = useNavigate();

  const fetchAdaptersData = async (): Promise<AdaptersData> => {
    const adaptersData = await apiClient.listAdapters();
    const metrics = await apiClient.getSystemMetrics();
    const totalMemory = metrics.memory_total_gb * 1024 * 1024 * 1024; // Convert GB to bytes
    return { adapters: adaptersData, totalMemory };
  };

  const { data, isLoading: loading, error, refetch } = usePolling(
    fetchAdaptersData,
    'normal',
    {
      showLoadingIndicator: false,
      onError: (err) => {
        logger.error('Failed to fetch adapters', { component: 'AdaptersPage' }, err);
      }
    }
  );

  const adapters = data?.adapters ?? [];
  const totalMemory = data?.totalMemory ?? 0;

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

  // Adapter operations using shared hook
  const {
    isOperationLoading,
    operationError,
    clearOperationError,
    loadAdapter,
    unloadAdapter,
    evictAdapter,
    pinAdapter,
    promoteAdapter,
    deleteAdapter,
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


  const getCategoryIcon = (category: string) => {
    switch (category) {
      case 'code': return <Code className="h-4 w-4" />;
      default: return <Activity className="h-4 w-4" />;
    }
  };

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
            <Button variant="outline" size="sm" onClick={() => navigate('/adapters/new')} disabled={!canRegister}>
              Register adapter
            </Button>
            <Button size="sm" onClick={() => navigate('/training/jobs')} disabled={!canStartTraining}>
              Start training
            </Button>
            <Button variant="outline" size="sm" onClick={() => navigate('/router-config')}>
              Configure routing
            </Button>
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
        <div className="flex items-center gap-2">
          <h1 className="text-2xl font-bold">Adapters</h1>
          <GlossaryTooltip termId="adapter" variant="icon" />
        </div>
        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            disabled={!canRegister}
            title={!canRegister ? 'Requires adapter:register permission' : 'Register a new adapter'}
            onClick={() => navigate('/adapters/new')}
          >
            <Upload className="h-4 w-4 mr-2" />
            Register
            <GlossaryTooltip brief="Register a new LoRA adapter from weights file" />
          </Button>
          <Button
            disabled={!canStartTraining}
            title={!canStartTraining ? 'Requires training:start permission' : 'Train a new adapter'}
            onClick={() => navigate('/training/jobs')}
          >
            Train New Adapter
            <GlossaryTooltip brief="Start the training wizard to create a new LoRA adapter from documents" />
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
            state: a.current_state,
            pinned: a.pinned,
            memory_bytes: a.memory_bytes,
            category: a.category,
            scope: a.scope,
            last_activated: a.last_activated,
            activation_count: a.activation_count,
          }));
          return <AdapterStateVisualization adapters={stateRecords} totalMemory={totalMemory} />;
        })()}
        <AdapterMemoryMonitor
          adapters={adapters}
          totalMemory={totalMemory}
          onEvictAdapter={canUnload ? evictAdapter : undefined}
          onPinAdapter={canLoad ? pinAdapter : undefined}
          onUpdateMemoryLimit={handleUpdateMemoryLimit}
        />
      </div>

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
            <LoadingState
              title="Loading adapters"
              description="Fetching adapter fleet status and usage metrics."
              skeletonLines={4}
              size="sm"
            />
          ) : adapters.length === 0 ? (
            <EmptyState
              icon={Code}
              title="No adapters deployed"
              description="Train or import an adapter to get started. Your fleet will appear here once deployed."
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
                {adapters.map(adapter => (
                  <TableRow key={adapter.id}>
                    <TableCell className="font-medium">{adapter.name}</TableCell>
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
                      <Badge>{LIFECYCLE_STATE_LABELS[adapter.current_state] || adapter.current_state}</Badge>
                      {adapter.pinned && <Pin className="h-4 w-4 ml-2" />}
                    </TableCell>
                    <TableCell>{(adapter.memory_bytes / 1024 / 1024).toFixed(1)} MB</TableCell>
                    <TableCell>{adapter.activation_count}</TableCell>
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
                            onClick={() => loadAdapter?.(adapter.id)}
                            disabled={!canLoad || adapter.current_state === 'resident'}
                            title={!canLoad ? 'Requires adapter:load permission' : 'Load adapter into memory'}
                          >
                            <Power className="mr-2 h-4 w-4" />
                            Load
                            <GlossaryTooltip brief="Load adapter weights into GPU memory for inference" />
                          </DropdownMenuItem>
                          <DropdownMenuItem
                            onClick={() => unloadAdapter?.(adapter.id)}
                            disabled={!canUnload || adapter.current_state === 'unloaded'}
                            title={!canUnload ? 'Requires adapter:unload permission' : 'Unload adapter from memory'}
                          >
                            <PowerOff className="mr-2 h-4 w-4" />
                            Unload
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
                            onClick={() => deleteAdapter(adapter.id)}
                            disabled={!canDelete}
                            title={!canDelete ? 'Requires adapter:delete permission' : 'Permanently delete adapter'}
                            className="text-destructive focus:text-destructive"
                          >
                            <Trash2 className="mr-2 h-4 w-4" />
                            Delete
                            <GlossaryTooltip brief="Permanently remove adapter and weights from the system" />
                          </DropdownMenuItem>
                          <DropdownMenuSeparator />
                          <DropdownMenuItem onClick={() => navigate(`/training/jobs?adapterId=${adapter.id}`)}>
                            <Upload className="mr-2 h-4 w-4" />
                            View training jobs
                          </DropdownMenuItem>
                          <DropdownMenuItem onClick={() => navigate(`/router-config?adapterId=${adapter.id}`)}>
                            <ArrowUp className="mr-2 h-4 w-4" />
                            Configure routing
                          </DropdownMenuItem>
                        </DropdownMenuContent>
                      </DropdownMenu>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>
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
