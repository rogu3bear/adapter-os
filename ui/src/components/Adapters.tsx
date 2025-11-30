import React, { useCallback, useEffect, useState, useMemo, memo } from 'react';
import { toast } from 'sonner';
import { Button } from './ui/button';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs';
import { Alert, AlertDescription } from './ui/alert';
import { LoadingState } from './ui/loading-state';
import { BulkActionBar, BulkAction } from './ui/bulk-action-bar';
import { ConfirmationDialog, ConfirmationOptions } from './ui/confirmation-dialog';
import { ExportDialog, ExportOptions, ExportScope } from './ui/export-dialog';
import { successTemplates } from './ui/success-feedback';
import { ErrorRecovery } from './ui/error-recovery';
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from './ui/dialog';
import { TrainingWizard } from './TrainingWizard';
import { AdapterImportWizard } from './AdapterImportWizard';
import LanguageBaseAdapterDialog from './LanguageBaseAdapterDialog';
import { AdapterRegistryTab } from './components/AdapterRegistryTab';
import { DeleteConfirmDialog } from './components/DeleteConfirmDialog';
import { useViewTransition } from '../hooks/useViewTransition';
import { useUndoRedoContext } from '../contexts/UndoRedoContext';
import { useProgressOperation } from '../hooks/useProgressOperation';
import { Plus, Upload, Brain, Database, Target, GitBranch, CheckCircle, AlertCircle } from 'lucide-react';
import apiClient from '../api/client';
import { User, Adapter } from '../api/types';
import { useSSE } from '../hooks/useSSE';
import { useNavigate } from 'react-router-dom';
import { logger, toError } from '../utils/logger';
import { getVisualHierarchyClasses } from '../utils/visual-hierarchy';
import { ContentSection } from './ui/content-section';
import { CodeIntelligence } from './CodeIntelligence';
import { RouterConfigPage } from './RouterConfigPage';
import { TrainingStreamPage } from './TrainingStreamPage';
import { SectionErrorBoundary } from './ui/section-error-boundary';
import { useAdapterFilters } from './hooks/useAdapterFilters';

interface AdaptersProps {
  user: User;
  selectedTenant: string;
}


interface TrainingJob {
  id: string;
  adapter_name: string;
  status: 'queued' | 'running' | 'completed' | 'failed' | 'cancelled';
  progress: number;
  started_at: string;
  estimated_completion?: string;
  config: TrainingConfig;
  logs: string[];
  metrics: TrainingMetrics;
}

interface TrainingConfig {
  tenant_id: string;
  repo_id?: string;
  commit_sha?: string;
  rank: number;
  alpha: number;
  targets: string[];
  epochs: number;
  learning_rate: number;
  batch_size: number;
  category: 'code' | 'framework' | 'codebase' | 'ephemeral';
  scope: 'global' | 'tenant' | 'repo' | 'commit';
  framework_id?: string;
  framework_version?: string;
  intent?: string;
}

interface TrainingMetrics {
  current_epoch: number;
  total_epochs: number;
  loss: number;
  validation_loss: number;
  learning_rate: number;
  gpu_utilization: number;
  memory_usage: number;
  tokens_per_second: number;
}

export const Adapters = memo(function Adapters({ user, selectedTenant }: AdaptersProps) {
  const navigate = useNavigate();
  const { addAction } = useUndoRedoContext();
  const [adapters, setAdapters] = useState<Adapter[]>([]);
  const [trainingJobs, setTrainingJobs] = useState<TrainingJob[]>([]);
  const [loading, setLoading] = useState(true);
  const [isCreateDialogOpen, setIsCreateDialogOpen] = useState(false);
  const [registerTab, setRegisterTab] = useState<'upload' | 'path'>('upload');
  const [uploadFile, setUploadFile] = useState<File | null>(null);
  const [isUploading, setIsUploading] = useState(false);
  const [upsertOpen, setUpsertOpen] = useState(false);
  const [upsertRoot, setUpsertRoot] = useState('');
  const [upsertPath, setUpsertPath] = useState('');

  // Bulk selection state
  const [selectedAdapters, setSelectedAdapters] = useState<string[]>([]);
  const [confirmationOpen, setConfirmationOpen] = useState(false);
  const [confirmationOptions, setConfirmationOptions] = useState<ConfirmationOptions | null>(null);
  const [pendingBulkAction, setPendingBulkAction] = useState<(() => Promise<void>) | null>(null);
  const [successFeedback, setSuccessFeedback] = useState<React.ReactElement | null>(null);
  const [errorRecovery, setErrorRecovery] = useState<React.ReactElement | null>(null);
  const [statusMessage, setStatusMessage] = useState<{ message: string; variant: 'success' | 'info' | 'warning' } | null>(null);
  const [upsertActivate, setUpsertActivate] = useState(true);
  const [isTrainingDialogOpen, setIsTrainingDialogOpen] = useState(false);
  const [isLanguageDialogOpen, setIsLanguageDialogOpen] = useState(false);

  // Progress tracking for long operations
  const { operation: activeProgressOperation, start: startProgressOperation, cancel: cancelProgressOperation } = useProgressOperation();
  const [selectedAdapter, setSelectedAdapter] = useState<Adapter | null>(null);

  const [selectedAdapterForHealth, setSelectedAdapterForHealth] = useState<Adapter | null>(null);
  const [activeTab, setActiveTab] = useState('registry');
  const transitionTo = useViewTransition();

  // Clear feedback states
  const clearFeedback = () => {
    setSuccessFeedback(null);
    setErrorRecovery(null);
    setStatusMessage(null);
  };
  const showStatus = (message: string, variant: 'success' | 'info' | 'warning') => {
    setStatusMessage({ message, variant });
  };
  const [selectedTrainingJob, setSelectedTrainingJob] = useState<string | null>(null);
  const [trainingConfig, setTrainingConfig] = useState<Partial<TrainingConfig>>({});
  const [deleteConfirmId, setDeleteConfirmId] = useState<string | null>(null);
  const [showExportDialog, setShowExportDialog] = useState(false);
  const [exportDialogScope, setExportDialogScope] = useState<ExportScope>('all');
  const [showImportDialog, setShowImportDialog] = useState(false);

  // SSE connection for real-time adapter state updates
  const { data: sseAdapters } = useSSE<Adapter[]>('/v1/stream/adapters');

  useEffect(() => {
    const handleOpenExport = (event: Event) => {
      const detail = (event as CustomEvent<{ scope?: ExportScope }>).detail;
      let scope = detail?.scope ?? (selectedAdapters.length > 0 ? 'selected' : 'all');

      if (detail?.scope === 'selected' && selectedAdapters.length === 0) {
        setStatusMessage({
          message: 'Select at least one adapter before exporting from the command palette.',
          variant: 'info',
        });
        scope = 'all';
      }

      setExportDialogScope(scope);
      setShowExportDialog(true);
    };

    window.addEventListener('aos:open-adapter-export', handleOpenExport as EventListener);
    return () => window.removeEventListener('aos:open-adapter-export', handleOpenExport as EventListener);
  }, [selectedAdapters]);

  // Remove mock data - using real API now
  /* Mock data removed
  const mockAdapters: Adapter[] = [
    {
      id: '1',
      adapter_id: 'python-general-v1',
      name: 'python-general-v1',
      hash_b3: 'b3:abc123...',
      rank: 16,
      tier: 1,
      languages_json: '["python"]',
      framework: 'python',
      category: 'code',
      scope: 'global',
      current_state: 'hot',
      pinned: false,
      memory_bytes: 16 * 1024 * 1024,
      last_activated: '2024-02-15T10:30:00Z',
      activation_count: 1247,
      created_at: '2024-01-15T10:30:00Z',
      updated_at: '2024-02-15T10:30:00Z',
      active: true
    },
    {
      id: '2',
      adapter_id: 'django-specific-v2',
      name: 'django-specific-v2',
      hash_b3: 'b3:def456...',
      rank: 12,
      tier: 2,
      languages_json: '["python"]',
      framework: 'django',
      framework_id: 'django',
      framework_version: '4.2',
      category: 'framework',
      scope: 'global',
      current_state: 'warm',
      pinned: false,
      memory_bytes: 16 * 1024 * 1024,
      last_activated: '2024-02-15T09:45:00Z',
      activation_count: 89,
      created_at: '2024-01-20T14:15:00Z',
      updated_at: '2024-02-15T09:45:00Z',
      active: true
    },
    {
      id: '3',
      adapter_id: 'acme-payments-v1',
      name: 'acme-payments-v1',
      hash_b3: 'b3:ghi789...',
      rank: 24,
      tier: 3,
      languages_json: '["python", "javascript"]',
      framework: 'python',
      repo_id: 'acme/payments',
      commit_sha: 'abc123def456',
      intent: 'payments',
      category: 'codebase',
      scope: 'tenant',
      current_state: 'resident',
      pinned: true,
      memory_bytes: 24 * 1024 * 1024,
      last_activated: '2024-02-15T10:25:00Z',
      activation_count: 2341,
      created_at: '2024-02-01T09:45:00Z',
      updated_at: '2024-02-15T10:25:00Z',
      active: true
    },
    {
      id: '4',
      adapter_id: 'temp-debug-v1',
      name: 'temp-debug-v1',
      hash_b3: 'b3:jkl012...',
      rank: 8,
      tier: 4,
      languages_json: '["python"]',
      framework: 'python',
      intent: 'debugging',
      category: 'ephemeral',
      scope: 'global',
      current_state: 'cold',
      pinned: false,
      memory_bytes: 8 * 1024 * 1024,
      last_activated: '2024-02-15T08:30:00Z',
      activation_count: 12,
      created_at: '2024-02-15T08:00:00Z',
      updated_at: '2024-02-15T08:30:00Z',
      active: true
    }
  ]; */

  const loadAdapters = useCallback(async () => {
    try {
      const adaptersData = await apiClient.listAdapters();
      setAdapters(adaptersData);
      // Training jobs API not yet implemented
      setTrainingJobs([]);
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to fetch adapters';
      logger.error('Failed to fetch adapters', {
        component: 'Adapters',
        operation: 'fetchAdapters',
        tenantId: selectedTenant,
        errorMessage: errorMsg,
      }, toError(err));
    }
  }, [selectedTenant]);

  useEffect(() => {
    let isMounted = true;

    const initialise = async () => {
      setLoading(true);
      await loadAdapters();
      if (isMounted) {
        setLoading(false);
      }
    };

    initialise();

    return () => {
      isMounted = false;
    };
  }, [loadAdapters]);

  // Update adapters from SSE stream
  useEffect(() => {
    if (!sseAdapters) return;
    setAdapters(sseAdapters);
  }, [sseAdapters]);

  useEffect(() => {
    setSelectedAdapters(prev => {
      if (prev.length === 0) return prev;
      const valid = new Set(adapters.map(adapter => adapter.adapter_id));
      const next = prev.filter(id => valid.has(id));
      return next.length === prev.length ? prev : next;
    });
  }, [adapters]);

  const handleDeleteAdapter = useCallback(async (adapterId: string) => {
    try {
      const adapter = adapters.find(a => a.adapter_id === adapterId);
      if (!adapter) return;

      const previousAdapter = { ...adapter };

      await apiClient.deleteAdapter(adapterId);
      const updatedAdapters = adapters.filter(a => a.adapter_id !== adapterId);
      setAdapters(updatedAdapters);
      setDeleteConfirmId(null);
      showStatus('Adapter deleted successfully.', 'success');

      // Record undo action
      addAction({
        type: 'delete_adapter',
        description: `Delete adapter "${adapter.name}"`,
        previousState: previousAdapter,
        reverse: async () => {
          // Re-register the adapter (undo delete)
          try {
            await apiClient.registerAdapter({
              adapter_id: previousAdapter.adapter_id,
              name: previousAdapter.name,
              hash_b3: previousAdapter.hash_b3,
              rank: previousAdapter.rank,
              tier: previousAdapter.tier,
              category: previousAdapter.category,
              framework: previousAdapter.framework,
              scope: previousAdapter.scope,
              languages: previousAdapter.languages,
            });
            await loadAdapters();
            showStatus(`Adapter "${adapter.name}" restored.`, 'success');
          } catch (err) {
            logger.error('Failed to undo adapter delete', {
              component: 'Adapters',
              operation: 'undoDelete',
              adapterId,
            }, toError(err));
            showStatus('Failed to restore adapter.', 'warning');
          }
        },
      });
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Failed to delete adapter');
      setErrorRecovery(
        <ErrorRecovery
          error={error.message}
          onRetry={() => handleDeleteAdapter(adapterId)}
        />
      );
    }
  }, [adapters, addAction, loadAdapters]);

  const handleLoadAdapter = useCallback(async (adapterId: string) => {
    try {
      const adapter = adapters.find(a => a.adapter_id === adapterId);
      const previousState = adapter?.current_state;

      // Start progress tracking
      const operationId = startProgressOperation('adapter_load', adapterId, selectedTenant);

      showStatus('Loading adapter...', 'info');
      await apiClient.loadAdapter(adapterId);

      // Record undo action
      if (adapter && previousState) {
        addAction({
          type: 'load_adapter',
          description: `Load adapter "${adapter.name}"`,
          previousState: { adapterId, previousState },
          reverse: async () => {
            try {
              await apiClient.unloadAdapter(adapterId);
              await loadAdapters();
            } catch (err) {
              logger.error('Failed to undo adapter load', {
                component: 'Adapters',
                operation: 'undoLoad',
                adapterId,
              }, toError(err));
            }
          },
        });
      }

      setSuccessFeedback(
        successTemplates.adapterLoaded(
          adapter?.name || 'Adapter',
          () => transitionTo('/inference?adapter=' + adapterId)
        )
      );
      await loadAdapters();
      setStatusMessage(null);
    } catch (err) {
      const adapterName = adapters.find(a => a.adapter_id === adapterId)?.name || 'Adapter';
      setErrorRecovery(
        <ErrorRecovery
          error={`Failed to load adapter ${adapterName}`}
          onRetry={() => handleLoadAdapter(adapterId)}
        />
      );
      setStatusMessage(null);
    }
  }, [adapters, addAction, loadAdapters, selectedTenant, startProgressOperation, transitionTo]);

  const handleUnloadAdapter = useCallback(async (adapterId: string) => {
    try {
      const adapter = adapters.find(a => a.adapter_id === adapterId);
      const previousState = adapter?.current_state;

      showStatus('Unloading adapter...', 'info');
      await apiClient.unloadAdapter(adapterId);

      // Record undo action
      if (adapter && previousState) {
        addAction({
          type: 'unload_adapter',
          description: `Unload adapter "${adapter.name}"`,
          previousState: { adapterId, previousState },
          reverse: async () => {
            try {
              await apiClient.loadAdapter(adapterId);
              await loadAdapters();
            } catch (err) {
              logger.error('Failed to undo adapter unload', {
                component: 'Adapters',
                operation: 'undoUnload',
                adapterId,
              }, toError(err));
            }
          },
        });
      }

      showStatus('Adapter unloaded successfully.', 'success');
      await loadAdapters();
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Failed to unload adapter');
      setErrorRecovery(
        <ErrorRecovery
          error={error.message}
          onRetry={() => handleUnloadAdapter(adapterId)}
        />
      );
      setStatusMessage(null);
    }
  }, [adapters, addAction, loadAdapters]);

  const handlePinToggle = useCallback(async (adapter: Adapter) => {
    try {
      const previousPinned = adapter.pinned;
      const isPinning = !adapter.pinned;

      if (adapter.pinned) {
        await apiClient.unpinAdapter(adapter.adapter_id);
        showStatus('Adapter unpinned.', 'success');
      } else {
        await apiClient.pinAdapter(adapter.adapter_id, true);
        showStatus('Adapter pinned.', 'success');
      }

      // Record undo action
      addAction({
        type: isPinning ? 'pin_adapter' : 'unpin_adapter',
        description: `${isPinning ? 'Pin' : 'Unpin'} adapter "${adapter.name}"`,
        previousState: { adapterId: adapter.adapter_id, pinned: previousPinned },
        reverse: async () => {
          try {
            if (isPinning) {
              await apiClient.unpinAdapter(adapter.adapter_id);
            } else {
              await apiClient.pinAdapter(adapter.adapter_id, true);
            }
            await loadAdapters();
          } catch (err) {
            logger.error('Failed to undo pin toggle', {
              component: 'Adapters',
              operation: 'undoPinToggle',
              adapterId: adapter.adapter_id,
            }, toError(err));
          }
        },
      });

      await loadAdapters();
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Failed to toggle pin');
      setErrorRecovery(
        <ErrorRecovery
          error={error.message}
          onRetry={() => handlePinToggle(adapter)}
        />
      );
    }
  }, [addAction, loadAdapters]);

  const handlePromoteState = useCallback(async (adapterId: string) => {
    try {
      const result = await apiClient.promoteAdapterState(adapterId);
      showStatus(`State promoted: ${result.old_state} → ${result.new_state}`, 'success');
      // Refresh adapters list
      const adaptersData = await apiClient.listAdapters();
      setAdapters(adaptersData);
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Failed to promote adapter state');
      setErrorRecovery(
        <ErrorRecovery
          error={error.message}
          onRetry={() => handlePromoteState(adapterId)}
        />
      );
    }
  }, []);

  // Bulk action handlers
  const handleBulkLoad = async (adapterIds: string[]) => {
    const performBulkLoad = async () => {
      const snapshots = adapters
        .filter(adapter => adapterIds.includes(adapter.adapter_id))
        .map(adapter => ({ ...adapter }));

      if (snapshots.length === 0) {
        showStatus('No adapters selected for load.', 'warning');
        return;
      }

      // Optimistic update
      setAdapters(prev =>
        prev.map(adapter =>
          adapterIds.includes(adapter.adapter_id)
            ? { ...adapter, current_state: 'hot', active: true }
            : adapter
        )
      );

      const failedIds: string[] = [];

      for (const adapterId of adapterIds) {
        try {
          await apiClient.loadAdapter(adapterId);
        } catch (err) {
          failedIds.push(adapterId);
          logger.error('Failed to load adapter in bulk operation', {
            component: 'Adapters',
            operation: 'bulkLoad',
            adapterId,
          }, toError(err));
        }
      }

      if (failedIds.length > 0) {
        // Revert failures to previous snapshot
        setAdapters(prev =>
          prev.map(adapter => {
            if (!failedIds.includes(adapter.adapter_id)) return adapter;
            const fallback = snapshots.find(snapshot => snapshot.adapter_id === adapter.adapter_id);
            return fallback ? fallback : adapter;
          })
        );

        setErrorRecovery(
          <ErrorRecovery
            error={`Failed to load ${failedIds.length} adapter(s).`}
            onRetry={() => handleBulkLoad(failedIds)}
          />
        );
      }

      const successfulIds = adapterIds.filter(id => !failedIds.includes(id));

      if (successfulIds.length > 0) {
        showStatus(`Successfully loaded ${successfulIds.length} adapter(s).`, 'success');
        addAction({
          type: 'bulk_load_adapters',
          description: `Load ${successfulIds.length} adapter(s)`,
          previousState: snapshots.filter(snapshot => successfulIds.includes(snapshot.adapter_id)),
          reverse: async () => {
            try {
              for (const snapshot of snapshots.filter(s => successfulIds.includes(s.adapter_id))) {
                if (!snapshot.active) {
                  await apiClient.unloadAdapter(snapshot.adapter_id);
                } else {
                  await apiClient.loadAdapter(snapshot.adapter_id);
                }
              }
              await loadAdapters();
              showStatus('Reverted adapter load.', 'success');
            } catch (err) {
              logger.error('Failed to undo adapter load', {
                component: 'Adapters',
                operation: 'undoBulkLoad',
              }, toError(err));
              showStatus('Failed to undo load operation.', 'warning');
            }
          },
        });
      }

      await loadAdapters();
      setSelectedAdapters(prev => prev.filter(id => failedIds.includes(id)));
    };

    setConfirmationOptions({
      title: 'Activate Adapters',
      description: `Activate ${adapterIds.length} adapter(s) into memory? This may take some time.`,
      confirmText: 'Activate Adapters',
      variant: 'default'
    });
    setPendingBulkAction(() => performBulkLoad);
    setConfirmationOpen(true);
  };

  const handleBulkUnload = async (adapterIds: string[]) => {
    const performBulkUnload = async () => {
      const snapshots = adapters
        .filter(adapter => adapterIds.includes(adapter.adapter_id))
        .map(adapter => ({ ...adapter }));

      if (snapshots.length === 0) {
        showStatus('No adapters selected for unload.', 'warning');
        return;
      }

      setAdapters(prev =>
        prev.map(adapter =>
          adapterIds.includes(adapter.adapter_id)
            ? { ...adapter, current_state: 'cold', active: false }
            : adapter
        )
      );

      const failedIds: string[] = [];

      for (const adapterId of adapterIds) {
        try {
          await apiClient.unloadAdapter(adapterId);
        } catch (err) {
          failedIds.push(adapterId);
          logger.error('Failed to unload adapter in bulk operation', {
            component: 'Adapters',
            operation: 'bulkUnload',
            adapterId,
          }, toError(err));
        }
      }

      if (failedIds.length > 0) {
        setAdapters(prev =>
          prev.map(adapter => {
            if (!failedIds.includes(adapter.adapter_id)) return adapter;
            const fallback = snapshots.find(snapshot => snapshot.adapter_id === adapter.adapter_id);
            return fallback ? fallback : adapter;
          })
        );

        setErrorRecovery(
          <ErrorRecovery
            error={`Failed to unload ${failedIds.length} adapter(s).`}
            onRetry={() => handleBulkUnload(failedIds)}
          />
        );
      }

      const successfulIds = adapterIds.filter(id => !failedIds.includes(id));

      if (successfulIds.length > 0) {
        showStatus(`Successfully unloaded ${successfulIds.length} adapter(s).`, 'success');
        addAction({
          type: 'bulk_unload_adapters',
          description: `Unload ${successfulIds.length} adapter(s)`,
          previousState: snapshots.filter(snapshot => successfulIds.includes(snapshot.adapter_id)),
          reverse: async () => {
            try {
              for (const snapshot of snapshots.filter(s => successfulIds.includes(s.adapter_id))) {
                if (snapshot.active) {
                  await apiClient.loadAdapter(snapshot.adapter_id);
                } else {
                  await apiClient.unloadAdapter(snapshot.adapter_id);
                }
              }
              await loadAdapters();
              showStatus('Reverted adapter unload.', 'success');
            } catch (err) {
              logger.error('Failed to undo adapter unload', {
                component: 'Adapters',
                operation: 'undoBulkUnload',
              }, toError(err));
              showStatus('Failed to undo unload operation.', 'warning');
            }
          },
        });
      }

      await loadAdapters();
      setSelectedAdapters(prev => prev.filter(id => failedIds.includes(id)));
    };

    setConfirmationOptions({
      title: 'Unload Adapters',
      description: `Unload ${adapterIds.length} adapter(s) from memory?`,
      confirmText: 'Unload Adapters',
      variant: 'default'
    });
    setPendingBulkAction(() => performBulkUnload);
    setConfirmationOpen(true);
  };

  const handleBulkDelete = async (adapterIds: string[]) => {
    const performBulkDelete = async () => {
      const snapshots = adapters
        .filter(adapter => adapterIds.includes(adapter.adapter_id))
        .map(adapter => ({ ...adapter }));

      if (snapshots.length === 0) {
        showStatus('No adapters selected for deletion.', 'warning');
        return;
      }

      setAdapters(prev => prev.filter(adapter => !adapterIds.includes(adapter.adapter_id)));

      const failedAdapters: Adapter[] = [];

      for (const adapterId of adapterIds) {
        try {
          await apiClient.deleteAdapter(adapterId);
        } catch (err) {
          const original = snapshots.find(adapter => adapter.adapter_id === adapterId);
          if (original) {
            failedAdapters.push(original);
          }
          logger.error('Failed to delete adapter in bulk operation', {
            component: 'Adapters',
            operation: 'bulkDelete',
            adapterId,
          }, toError(err));
        }
      }

      if (failedAdapters.length > 0) {
        setAdapters(prev => [...prev, ...failedAdapters]);
        setErrorRecovery(
          <ErrorRecovery
            error={`Failed to delete ${failedAdapters.length} adapter(s).`}
            onRetry={() => handleBulkDelete(failedAdapters.map(adapter => adapter.adapter_id))}
          />
        );
      }

      const successfulAdapters = snapshots.filter(snapshot => !failedAdapters.some(failed => failed.adapter_id === snapshot.adapter_id));

      if (successfulAdapters.length > 0) {
        showStatus(`Successfully deleted ${successfulAdapters.length} adapter(s).`, 'success');

        addAction({
          type: 'bulk_delete_adapters',
          description: `Delete ${successfulAdapters.length} adapter(s)`,
          previousState: successfulAdapters,
          reverse: async () => {
            try {
              for (const adapter of successfulAdapters) {
                await apiClient.registerAdapter({
                  adapter_id: adapter.adapter_id,
                  name: adapter.name,
                  hash_b3: adapter.hash_b3,
                  rank: adapter.rank,
                  tier: adapter.tier,
                  category: adapter.category,
                  framework: adapter.framework,
                  scope: adapter.scope,
                  languages: adapter.languages,
                });
              }
              await loadAdapters();
              showStatus(`Restored ${successfulAdapters.length} adapter(s).`, 'success');
            } catch (err) {
              logger.error('Failed to undo bulk adapter delete', {
                component: 'Adapters',
                operation: 'undoBulkDelete',
                adapterIds: successfulAdapters.map(adapter => adapter.adapter_id),
              }, toError(err));
              showStatus('Failed to restore adapters.', 'warning');
            }
          },
        });
      }

      await loadAdapters();
      setSelectedAdapters(prev => prev.filter(id => failedAdapters.some(adapter => adapter.adapter_id === id)));
    };

    setConfirmationOptions({
      title: 'Delete Adapters',
      description: `Permanently delete ${adapterIds.length} adapter(s)? This action cannot be undone.`,
      confirmText: 'Delete Adapters',
      variant: 'destructive'
    });
    setPendingBulkAction(() => performBulkDelete);
    setConfirmationOpen(true);
  };

  const bulkActions: BulkAction[] = useMemo(() => [
    {
      id: 'load',
      label: 'Load',
      handler: handleBulkLoad
    },
    {
      id: 'unload',
      label: 'Unload',
      handler: handleBulkUnload
    },
    {
      id: 'delete',
      label: 'Delete',
      variant: 'destructive',
      handler: handleBulkDelete
    }
  ], [handleBulkLoad, handleBulkUnload, handleBulkDelete]);

  const handleDownloadManifest = async (adapterId: string) => {
    try {
      const manifest = await apiClient.downloadAdapterManifest(adapterId);
      const blob = new Blob([JSON.stringify(manifest, null, 2)], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `${adapterId}-manifest.json`;
      a.click();
      URL.revokeObjectURL(url);
      showStatus('Manifest downloaded.', 'success');
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Failed to download manifest');
      setErrorRecovery(
        <ErrorRecovery
          error={error.message}
          onRetry={() => handleDownloadManifest(adapterId)}
        />
      );
    }
  };

  const handleExportDialogOpenChange = useCallback((open: boolean) => {
    setShowExportDialog(open);
    if (!open) {
      setExportDialogScope(selectedAdapters.length > 0 ? 'selected' : 'all');
    }
  }, [selectedAdapters]);

  const handleExport = async (options: ExportOptions) => {
    try {
      let adapterIdsToExport: string[] = [];

      if (options.scope === 'selected') {
        adapterIdsToExport = selectedAdapters;
      } else if (options.scope === 'all') {
        adapterIdsToExport = adapters.map(a => a.adapter_id);
      } else {
        // filtered - for now, same as all
        adapterIdsToExport = adapters.map(a => a.adapter_id);
      }

      if (adapterIdsToExport.length === 0) {
        showStatus('No adapters to export.', 'warning');
        handleExportDialogOpenChange(false);
        return;
      }

      // Download all manifests
      const manifests = [];
      for (const adapterId of adapterIdsToExport) {
        try {
          const manifest = await apiClient.downloadAdapterManifest(adapterId);
          manifests.push(manifest);
        } catch (err) {
          logger.error('Failed to download manifest for export', {
            component: 'Adapters',
            operation: 'export',
            adapterId
          }, toError(err));
        }
      }

      // Create export file
      const timestamp = new Date().toISOString().slice(0, 19).replace(/:/g, '-');
      const filename = `adapters-export-${timestamp}`;

      if (options.format === 'json') {
        const blob = new Blob([JSON.stringify(manifests, null, 2)], { type: 'application/json' });
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = `${filename}.json`;
        a.click();
        URL.revokeObjectURL(url);
      } else {
        // CSV export
        if (manifests.length === 0) return;
        
        const headers = [
          // Primary identifiers
          'adapter_id', 'name',

          // Content classification
          'category', 'scope', 'intent', 'languages',

          // Technical details
          'framework', 'framework_id', 'framework_version', 'blake3_hash',

          // Quality metrics
          'tier', 'rank',

          // Provenance tracking
          'repository_id', 'commit_sha',

          // Metadata
          'created_at', 'updated_at'
        ];
        const csvRows = manifests.map(m =>
          headers.map(header => {
            // Map user-friendly header names to API field names
            const fieldName = header === 'languages' ? 'languages_json' :
                             header === 'blake3_hash' ? 'hash_b3' :
                             header === 'repository_id' ? 'repo_id' :
                             header;
            const value = (m as any)[fieldName] || '';
            if (typeof value === 'string' && (value.includes(',') || value.includes('"'))) {
              return `"${value.replace(/"/g, '""')}"`;
            }
            return value;
          }).join(',')
        );
        const csv = [headers.join(','), ...csvRows].join('\n');
        const blob = new Blob([csv], { type: 'text/csv' });
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = `${filename}.csv`;
        a.click();
        URL.revokeObjectURL(url);
      }

      showStatus(`Exported ${manifests.length} adapter manifest(s).`, 'success');
      handleExportDialogOpenChange(false);
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Failed to export adapters');
      setErrorRecovery(
        <ErrorRecovery
          error={error.message}
          onRetry={() => handleExport(options)}
        />
      );
    }
  };

  const [showHealthModal, setShowHealthModal] = useState(false);
  const [healthData, setHealthData] = useState<any | null>(null);

  const { adapterFilterConfigs, filteredAdapters, filterValues, setFilterValues } = useAdapterFilters(adapters);

  const handleViewHealth = useCallback(async (adapterId: string) => {
    try {
      const health = await apiClient.getAdapterHealth(adapterId);
      setHealthData(health);
      const adapter = adapters.find(a => a.adapter_id === adapterId);
      if (adapter) {
        setSelectedAdapterForHealth(adapter);
      }
      setShowHealthModal(true);
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Failed to fetch adapter health');
      setErrorRecovery(
        <ErrorRecovery
          error={error.message}
          onRetry={() => handleViewHealth(adapterId)}
        />
      );
    }
  }, [adapters]);



  if (loading) {
    return (
      <LoadingState
        title="Loading adapters"
        description="Fetching the latest adapter registry and training jobs"
        skeletonLines={3}
        size="md"
        className="my-12"
      />
    );
  }

  const hierarchyClasses = getVisualHierarchyClasses({ level: 'primary', emphasis: 'high' });

  return (
    <div className={hierarchyClasses.container}>

      {successFeedback && (
        <div className="mb-6">
          {successFeedback}
        </div>
      )}

      {errorRecovery && (
        <div className="mb-6">
          {errorRecovery}
        </div>
      )}

      {statusMessage && (
        <Alert
          className={
            statusMessage.variant === 'success'
              ? 'border-gray-300 bg-gray-50'
              : statusMessage.variant === 'warning'
                ? 'border-gray-300 bg-gray-50'
                : 'border-gray-300 bg-gray-50'
          }
        >
          {statusMessage.variant === 'success' ? (
            <CheckCircle className="w-4 h-4 text-gray-600" />
          ) : statusMessage.variant === 'warning' ? (
            <AlertCircle className="w-4 h-4 text-gray-500" />
          ) : (
            <AlertCircle className="w-4 h-4 text-gray-400" />
          )}
          <AlertDescription
            className={
              statusMessage.variant === 'success'
                ? 'text-gray-700'
                : statusMessage.variant === 'warning'
                  ? 'text-gray-600'
                  : 'text-gray-600'
            }
          >
            {statusMessage.message}
          </AlertDescription>
        </Alert>
      )}

      <ContentSection
        title="Adapter Management"
        subtitle="Train, manage, and monitor LoRA adapters for your models"
        level="primary"
        variant="default"
        actions={
          <div className="flex items-center gap-2">
            <Button onClick={() => setIsTrainingDialogOpen(true)}>
              <Brain className="h-4 w-4 mr-2" />
              Train Adapter
            </Button>

            <Button onClick={() => setIsLanguageDialogOpen(true)}>
              <Brain className="h-4 w-4 mr-2" />
              Train Language Base Adapter
            </Button>

            <Button onClick={() => setIsCreateDialogOpen(true)}>
              <Plus className="h-4 w-4 mr-2" />
              Register Adapter
            </Button>

            <Button variant="outline" onClick={() => setShowImportDialog(true)}>
              <Upload className="h-4 w-4 mr-2" />
              Import Adapter
            </Button>
            <Button variant="outline" onClick={() => setUpsertOpen(true)}>
              <Plus className="h-4 w-4 mr-2" />
              Directory Upsert
            </Button>

          </div>
        }
      >

      {/* Citation: docs/architecture/MasterPlan.md L16-L17, L46-L71 */}
      <Tabs value={activeTab} onValueChange={setActiveTab}>
        <TabsList className="grid w-full grid-cols-4">
          <TabsTrigger value="registry" className="flex items-center gap-2">
            <Database className="h-4 w-4" />
            <span className="hidden sm:inline">Registry</span>
          </TabsTrigger>
          <TabsTrigger value="training" className="flex items-center gap-2">
            <Brain className="h-4 w-4" />
            <span className="hidden sm:inline">Training</span>
          </TabsTrigger>
          <TabsTrigger value="router" className="flex items-center gap-2">
            <Target className="h-4 w-4" />
            <span className="hidden sm:inline">Router Config</span>
          </TabsTrigger>
          <TabsTrigger value="code-intel" className="flex items-center gap-2">
            <GitBranch className="h-4 w-4" />
            <span className="hidden sm:inline">Code Intelligence</span>
          </TabsTrigger>
        </TabsList>

        {/* Registry Tab */}

        <TabsContent value="registry" className="mb-4">
          <AdapterRegistryTab
            adapters={adapters}
            filteredAdapters={filteredAdapters}
            selectedAdapters={selectedAdapters}
            setSelectedAdapters={setSelectedAdapters}
            adapterFilterConfigs={adapterFilterConfigs}
            filterValues={filterValues}
            setFilterValues={setFilterValues}
            setExportDialogScope={setExportDialogScope}
            setShowExportDialog={setShowExportDialog}
            handleLoadAdapter={handleLoadAdapter}
            handleUnloadAdapter={handleUnloadAdapter}
            handlePinToggle={handlePinToggle}
            handlePromoteState={handlePromoteState}
            handleViewHealth={handleViewHealth}
            handleDownloadManifest={handleDownloadManifest}
            setDeleteConfirmId={setDeleteConfirmId}
          />
        </TabsContent>

        {/* Training Tab */}


        <TabsContent value="training" className="space-y-4">
          <TrainingStreamPage selectedTenant={selectedTenant} />
        </TabsContent>

        {/* Router Config Tab */}
        <TabsContent value="router" className="space-y-4">
          <RouterConfigPage selectedTenant={selectedTenant} />
        </TabsContent>

        {/* Code Intelligence Tab */}
        <TabsContent value="code-intel" className="space-y-4">
          <CodeIntelligence user={user} selectedTenant={selectedTenant} />
        </TabsContent>

      </Tabs>

      {/* Training Dialog */}
      <Dialog open={isTrainingDialogOpen} onOpenChange={setIsTrainingDialogOpen}>
        <DialogContent className="max-w-6xl max-h-[90vh] overflow-y-auto">
          <TrainingWizard
            onComplete={(jobId) => {

              showStatus(`Training job ${jobId} started.`, 'success');

              toast.success(`Training job ${jobId} started`);
              setIsTrainingDialogOpen(false);
              // Optionally refresh adapters or navigate to training monitor
            }}
            onCancel={() => setIsTrainingDialogOpen(false)}
          />
        </DialogContent>
      </Dialog>

      {/* Language Base Adapter Dialog */}
      <LanguageBaseAdapterDialog
        open={isLanguageDialogOpen}
        onOpenChange={setIsLanguageDialogOpen}
        selectedTenant={selectedTenant}
        onSuccess={(jobId) => {
          showStatus(`Training job ${jobId} started.`, 'success');
          setIsLanguageDialogOpen(false);
          setSelectedTrainingJob(jobId);
          setActiveTab('training');
        }}
      />

      {/* Register Adapter Dialog */}
      <Dialog open={isCreateDialogOpen} onOpenChange={setIsCreateDialogOpen}>
        <DialogContent className="max-w-4xl">
          <DialogHeader>
            <DialogTitle>Register Adapter</DialogTitle>
          </DialogHeader>
          <Tabs value={registerTab} onValueChange={(value) => setRegisterTab(value as 'upload' | 'path')}>
            <TabsList className="grid w-full grid-cols-2">
              <TabsTrigger value="upload">Upload .aos File</TabsTrigger>
              <TabsTrigger value="path">From Server Path</TabsTrigger>
            </TabsList>
            <TabsContent value="upload" className="space-y-4">
              <SectionErrorBoundary sectionName="Import Wizard">
              <AdapterImportWizard
                onComplete={async (adapter) => {
                  setSuccessFeedback(
                    successTemplates.adapterCreated(
                      adapter.name,
                      () => transitionTo('/inference?adapter=' + adapter.adapter_id),
                      () => setActiveTab('registry')
                    )
                  );
                  setIsCreateDialogOpen(false);
                  setUploadFile(null);
                  await loadAdapters();
                }}
                onCancel={() => {
                  setIsCreateDialogOpen(false);
                  setUploadFile(null);
                }}
              />
              </SectionErrorBoundary>
            </TabsContent>
            <TabsContent value="path" className="space-y-4">
              <div className="space-y-3">
                <div>
                  <Label>Organization</Label>
                  <Input value={selectedTenant} readOnly />
                </div>
                <div>
                  <Label>Root (absolute)</Label>
                  <Input
                    value={upsertRoot}
                    onChange={(e) => setUpsertRoot(e.target.value)}
                    placeholder="/abs/root"
                  />
                </div>
                <div>
                  <Label>Path (relative)</Label>
                  <Input
                    value={upsertPath}
                    onChange={(e) => setUpsertPath(e.target.value)}
                    placeholder="src/"
                  />
                </div>
                <div className="flex items-center gap-2">
                  <input
                    id="activate"
                    type="checkbox"
                    checked={upsertActivate}
                    onChange={(e) => setUpsertActivate(e.target.checked)}
                  />
                  <Label htmlFor="activate">Activate after create</Label>
                </div>
              </div>
              <DialogFooter>
                <Button variant="outline" onClick={() => setIsCreateDialogOpen(false)}>Cancel</Button>
                <Button onClick={async () => {
                  try {
                    const data = await apiClient.upsertAdapterDirectory({
                      tenant_id: selectedTenant,
                      root: upsertRoot,
                      path: upsertPath,
                      activate: upsertActivate,
                    });
                    showStatus(`Upserted adapter ${data.adapter_id}.`, 'success');
                    setIsCreateDialogOpen(false);
                    setUpsertRoot('');
                    setUpsertPath('');
                    setUpsertActivate(true);
                    await loadAdapters();
                  } catch (err) {
                    setErrorRecovery(
                      <ErrorRecovery
                        error={err instanceof Error ? err.message : 'Upsert failed'}
                        onRetry={() => {
                          setErrorRecovery(null);
                        }}
                      />
                    );
                  }
                }}>Create</Button>
              </DialogFooter>
            </TabsContent>
          </Tabs>
        </DialogContent>
      </Dialog>

      {/* Directory Upsert Dialog */}
      <Dialog open={upsertOpen} onOpenChange={setUpsertOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Directory Upsert</DialogTitle>
          </DialogHeader>
          <div className="space-y-3">
            <div>
              <label className="font-medium text-sm mb-1">Organization</label>
              <Input value={selectedTenant} readOnly />
            </div>
            <div>
              <label className="font-medium text-sm mb-1">Root (absolute)</label>
              <Input value={upsertRoot} onChange={(e) => setUpsertRoot(e.target.value)} placeholder="/abs/root" />
            </div>
            <div>
              <label className="font-medium text-sm mb-1">Path (relative)</label>
              <Input value={upsertPath} onChange={(e) => setUpsertPath(e.target.value)} placeholder="src/" />
            </div>
            <div className="flex items-center gap-2">
              <input id="activate" type="checkbox" checked={upsertActivate} onChange={(e) => setUpsertActivate(e.target.checked)} />
              <label htmlFor="activate">Activate after create</label>
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setUpsertOpen(false)}>Cancel</Button>
            <Button onClick={async () => {
              try {
                const data = await apiClient.upsertAdapterDirectory({
                  tenant_id: selectedTenant,
                  root: upsertRoot,
                  path: upsertPath,
                  activate: upsertActivate,
                });
                showStatus(`Upserted adapter ${data.adapter_id}.`, 'success');
                setUpsertOpen(false);
                setUpsertRoot('');
                setUpsertPath('');
                setUpsertActivate(true);
                const adaptersData = await apiClient.listAdapters();
                setAdapters(adaptersData);
              } catch (err) {
                setErrorRecovery(
                  <ErrorRecovery
                    error={err instanceof Error ? err.message : 'Upsert failed'}
                    onRetry={() => {
                      setErrorRecovery(null);
                    }}
                  />
                );
              }
            }}>Submit</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Health Modal */}
      <Dialog open={showHealthModal} onOpenChange={setShowHealthModal}>
        <DialogContent>
          <DialogHeader>
            <div className="flex items-center justify-between">
              <DialogTitle>Adapter Health</DialogTitle>
              {healthData && selectedAdapterForHealth && (
                <BookmarkButton
                  type="adapter"
                  title={selectedAdapterForHealth.name}
                  url={`/adapters?adapter=${encodeURIComponent(selectedAdapterForHealth.adapter_id)}`}
                  entityId={selectedAdapterForHealth.adapter_id}
                  description={`${selectedAdapterForHealth.framework || 'Unknown'} • Health View`}
                  variant="ghost"
                  size="icon"
                />
              )}
            </div>
          </DialogHeader>
          {healthData && (
            <div className="space-y-4">
              <div>
                <Label>Status</Label>
                <Badge variant={healthData.is_healthy ? 'default' : 'destructive'}>
                  {healthData.is_healthy ? 'Healthy' : 'Unhealthy'}
                </Badge>
              </div>
              <div>
                <Label>Load Time</Label>
                <p>{healthData.load_time_ms}ms</p>
              </div>
              <div>
                <Label>Memory Usage</Label>
                <p>{Math.round(healthData.memory_usage_bytes / 1024 / 1024)} MB</p>
              </div>
              {healthData.error_message && (
                <div>
                  <Label>Error</Label>
                  <Alert variant="destructive">
                    <AlertDescription>{healthData.error_message}</AlertDescription>
                  </Alert>
                </div>
              )}
            </div>
          )}
        </DialogContent>
      </Dialog>

      {/* Bulk Action Bar */}
      <BulkActionBar
        selectedItems={selectedAdapters}
        actions={bulkActions}
        onClearSelection={() => setSelectedAdapters([])}
        itemName="adapter"
      />

      {/* Confirmation Dialog */}
      <ConfirmationDialog
        open={confirmationOpen}
        onOpenChange={(open) => {
          setConfirmationOpen(open);
          if (!open) {
            setPendingBulkAction(null);
            setConfirmationOptions(null);
          }
        }}
        onConfirm={async () => {
          if (pendingBulkAction) {
            await pendingBulkAction();
            setPendingBulkAction(null);
            setConfirmationOptions(null);
          }
        }}
        options={confirmationOptions || {
          title: 'Confirm Action',
          description: 'Are you sure?',
          variant: 'default'
        }}
      />

      {/* Export Dialog */}
      <ExportDialog
        key={`adapter-export-${exportDialogScope}-${selectedAdapters.length}`}
        open={showExportDialog}
        onOpenChange={handleExportDialogOpenChange}
        onExport={handleExport}
        itemName="adapters"
        hasSelected={selectedAdapters.length > 0}
        hasFilters={false}
        defaultFormat="json"
        defaultScope={exportDialogScope}
      />

      {/* Undo/Redo Bar */}


      {/* Import Dialog */}
      <Dialog open={showImportDialog} onOpenChange={setShowImportDialog}>
        <DialogContent className="max-w-4xl max-h-[90vh] overflow-y-auto">
          <DialogHeader>
            <DialogTitle>Import Adapter</DialogTitle>
          </DialogHeader>
          <SectionErrorBoundary sectionName="Import Wizard">
          <AdapterImportWizard
            onComplete={(adapter) => {
              setShowImportDialog(false);
              loadAdapters();
              showStatus(`Adapter "${adapter.name}" imported successfully.`, 'success');
            }}
            onCancel={() => setShowImportDialog(false)}
          />
          </SectionErrorBoundary>
        </DialogContent>
      </Dialog>

      {/* Delete Confirmation Dialog */}
      <DeleteConfirmDialog
        open={deleteConfirmId !== null}
        adapterId={deleteConfirmId}
        onConfirm={handleDeleteAdapter}
        onCancel={() => setDeleteConfirmId(null)}
      />

      </ContentSection>
    </div>
  );
});
