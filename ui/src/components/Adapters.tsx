import React, { useCallback, useEffect, useState } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Textarea } from './ui/textarea';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from './ui/table';
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from './ui/dialog';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Switch } from './ui/switch';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs';
import { Progress } from './ui/progress';
import { Alert, AlertDescription } from './ui/alert';
import { EmptyState } from './ui/empty-state';
import { Checkbox } from './ui/checkbox';
import { BulkActionBar, BulkAction } from './ui/bulk-action-bar';
import { ConfirmationDialog, ConfirmationOptions } from './ui/confirmation-dialog';
import { SuccessFeedback, SuccessTemplates } from './ui/success-feedback';
import { ErrorRecovery, ErrorRecoveryTemplates } from './ui/error-recovery';
import { TrainingWizard } from './TrainingWizard';
import LanguageBaseAdapterDialog from './LanguageBaseAdapterDialog';
import { useViewTransition } from '../hooks/useViewTransition';
import { 
  Plus, 
  Code, 
  Settings, 
  Play, 
  Pause, 
  Square, 
  Download, 
  Upload,
  Eye,
  Edit,
  Trash2,
  Clock,
  Zap,
  Target,
  BarChart3,
  Activity,
  AlertTriangle,
  CheckCircle,
  XCircle,
  Brain,
  Database,
  GitBranch,
  Layers,
  Cpu,
  MemoryStick,
  HardDrive,
  Snowflake,
  Thermometer,
  Flame,
  Anchor,
  Pin,
  MoreHorizontal,
  ArrowUp,
  FileText,
  AlertCircle
} from 'lucide-react';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from './ui/dropdown-menu';
import apiClient from '../api/client';
import { User } from '../api/types';
import { useSSE } from '../hooks/useSSE';
import { TrainingMonitor } from './TrainingMonitor';
import { useNavigate } from 'react-router-dom';
import { CodeIntelligenceTraining } from './CodeIntelligenceTraining';
import { TrainingTemplates } from './TrainingTemplates';
import { ResourceMonitor } from './ResourceMonitor';
import { AdapterStateVisualization } from './AdapterStateVisualization';
import { AdapterLifecycleManager } from './AdapterLifecycleManager';
import { AdapterMemoryMonitor } from './AdapterMemoryMonitor';
import { ContentSection, ContentGrid, ContentList } from './ui/content-section';
import { getVisualHierarchyClasses } from '../utils/visual-hierarchy';
import { CodeIntelligence } from './CodeIntelligence';
import { RouterConfigPage } from './RouterConfigPage';
import { TrainingStreamPage } from './TrainingStreamPage';
import { DomainAdapterManager } from './DomainAdapterManager';
import { logger, toError } from '../utils/logger';

interface AdaptersProps {
  user: User;
  selectedTenant: string;
}

interface Adapter {
  id: string;
  adapter_id: string;
  name: string;
  hash_b3: string;
  rank: number;
  tier: number;
  languages_json?: string;
  framework?: string;
  
  // Code intelligence fields
  category: 'code' | 'framework' | 'codebase' | 'ephemeral';
  scope: 'global' | 'tenant' | 'repo' | 'commit';
  framework_id?: string;
  framework_version?: string;
  repo_id?: string;
  commit_sha?: string;
  intent?: string;
  
  // Lifecycle state management
  current_state: 'unloaded' | 'cold' | 'warm' | 'hot' | 'resident';
  pinned: boolean;
  memory_bytes: number;
  last_activated?: string;
  activation_count: number;
  
  created_at: string;
  updated_at: string;
  active: boolean;
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

interface AdaptersProps {
  user: User;
  selectedTenant: string;
}

export function Adapters({ user, selectedTenant }: AdaptersProps) {
  const navigate = useNavigate();
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
  const [selectedAdapter, setSelectedAdapter] = useState<Adapter | null>(null);
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

  // SSE connection for real-time adapter state updates
  const { data: sseAdapters } = useSSE<Adapter[]>('/v1/stream/adapters');

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
    if (sseAdapters) {
      setAdapters(sseAdapters);
    }
  }, [sseAdapters]);

  const handleDeleteAdapter = async (adapterId: string) => {
    try {
      await apiClient.deleteAdapter(adapterId);
      setAdapters(adapters.filter(a => a.adapter_id !== adapterId));
      setDeleteConfirmId(null);
      showStatus('Adapter deleted successfully.', 'success');
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Failed to delete adapter');
      setErrorRecovery(
        ErrorRecoveryTemplates.genericError(
          error,
          () => handleDeleteAdapter(adapterId)
        )
      );
    }
  };

  const handleLoadAdapter = async (adapterId: string) => {
    try {
      showStatus('Loading adapter...', 'info');
      await apiClient.loadAdapter(adapterId);
      setSuccessFeedback(
        SuccessTemplates.adapterLoaded(
          adapters.find(a => a.adapter_id === adapterId)?.name || 'Adapter',
          () => transitionTo('/inference?adapter=' + adapterId)
        )
      );
      await loadAdapters();
      setStatusMessage(null);
    } catch (err) {
      const adapterName = adapters.find(a => a.adapter_id === adapterId)?.name || 'Adapter';
      setErrorRecovery(
        ErrorRecoveryTemplates.adapterLoadError(
          adapterName,
          () => handleLoadAdapter(adapterId)
        )
      );
      setStatusMessage(null);
    }
  };

  const handleUnloadAdapter = async (adapterId: string) => {
    try {
      showStatus('Unloading adapter...', 'info');
      await apiClient.unloadAdapter(adapterId);
      showStatus('Adapter unloaded successfully.', 'success');
      await loadAdapters();
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Failed to unload adapter');
      setErrorRecovery(
        ErrorRecoveryTemplates.genericError(
          error,
          () => handleUnloadAdapter(adapterId)
        )
      );
      setStatusMessage(null);
    }
  };

  const handlePinToggle = async (adapter: Adapter) => {
    try {
      if (adapter.pinned) {
        await apiClient.unpinAdapter(adapter.adapter_id);
        showStatus('Adapter unpinned.', 'success');
      } else {
        await apiClient.pinAdapter(adapter.adapter_id, true);
        showStatus('Adapter pinned.', 'success');
      }
      await loadAdapters();
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Failed to toggle pin');
      setErrorRecovery(
        ErrorRecoveryTemplates.genericError(
          error,
          () => handlePinToggle(adapter)
        )
      );
    }
  };

  const handlePromoteState = async (adapterId: string) => {
    try {
      const result = await apiClient.promoteAdapterState(adapterId);
      showStatus(`State promoted: ${result.old_state} → ${result.new_state}`, 'success');
      // Refresh adapters list
      const adaptersData = await apiClient.listAdapters();
      setAdapters(adaptersData);
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Failed to promote adapter state');
      setErrorRecovery(
        ErrorRecoveryTemplates.genericError(
          error,
          () => handlePromoteState(adapterId)
        )
      );
    }
  };

  // Bulk action handlers
  const handleBulkLoad = async (adapterIds: string[]) => {
    const performBulkLoad = async () => {
      let successCount = 0;
      let errorCount = 0;

      for (const adapterId of adapterIds) {
        try {
          await apiClient.loadAdapter(adapterId);
          successCount++;
        } catch (err) {
          errorCount++;
          logger.error('Failed to load adapter in bulk operation', {
            component: 'Adapters',
            operation: 'bulkLoad',
            adapterId
          }, toError(err));
        }
      }

      if (successCount > 0) {
        showStatus(`Successfully loaded ${successCount} adapter(s).`, 'success');
      }
      if (errorCount > 0) {
        setErrorRecovery(
          ErrorRecoveryTemplates.genericError(
            new Error(`Failed to load ${errorCount} adapter(s).`),
            () => performBulkLoad()
          )
        );
      }

      await loadAdapters();
      setSelectedAdapters([]);
    };

    setConfirmationOptions({
      title: 'Load Adapters',
      description: `Load ${adapterIds.length} adapter(s) into memory? This may take some time.`,
      confirmText: 'Load Adapters',
      variant: 'default'
    });
    setPendingBulkAction(() => performBulkLoad);
    setConfirmationOpen(true);
  };

  const handleBulkUnload = async (adapterIds: string[]) => {
    const performBulkUnload = async () => {
      let successCount = 0;
      let errorCount = 0;

      for (const adapterId of adapterIds) {
        try {
          await apiClient.unloadAdapter(adapterId);
          successCount++;
        } catch (err) {
          errorCount++;
          logger.error('Failed to unload adapter in bulk operation', {
            component: 'Adapters',
            operation: 'bulkUnload',
            adapterId
          }, toError(err));
        }
      }

      if (successCount > 0) {
        showStatus(`Successfully unloaded ${successCount} adapter(s).`, 'success');
      }
      if (errorCount > 0) {
        setErrorRecovery(
          ErrorRecoveryTemplates.genericError(
            new Error(`Failed to unload ${errorCount} adapter(s).`),
            () => performBulkUnload()
          )
        );
      }

      await loadAdapters();
      setSelectedAdapters([]);
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
      let successCount = 0;
      let errorCount = 0;

      for (const adapterId of adapterIds) {
        try {
          await apiClient.deleteAdapter(adapterId);
          successCount++;
        } catch (err) {
          errorCount++;
          logger.error('Failed to delete adapter in bulk operation', {
            component: 'Adapters',
            operation: 'bulkDelete',
            adapterId
          }, toError(err));
        }
      }

      if (successCount > 0) {
        showStatus(`Successfully deleted ${successCount} adapter(s).`, 'success');
      }
      if (errorCount > 0) {
        setErrorRecovery(
          ErrorRecoveryTemplates.genericError(
            new Error(`Failed to delete ${errorCount} adapter(s).`),
            () => performBulkDelete()
          )
        );
      }

      await loadAdapters();
      setSelectedAdapters([]);
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

  const bulkActions: BulkAction[] = [
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
  ];

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
        ErrorRecoveryTemplates.genericError(
          error,
          () => handleDownloadManifest(adapterId)
        )
      );
    }
  };

  const [showHealthModal, setShowHealthModal] = useState(false);
  const [healthData, setHealthData] = useState<any | null>(null);

  const handleViewHealth = async (adapterId: string) => {
    try {
      const health = await apiClient.getAdapterHealth(adapterId);
      setHealthData(health);
      setShowHealthModal(true);
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Failed to fetch adapter health');
      setErrorRecovery(
        ErrorRecoveryTemplates.genericError(
          error,
          () => handleViewHealth(adapterId)
        )
      );
    }
  };

  const getStateIcon = (state: string) => {
    switch (state) {
      case 'unloaded': return <Square className="h-4 w-4 text-gray-500" />;
      case 'cold': return <Snowflake className="h-4 w-4 text-blue-500" />;
      case 'warm': return <Thermometer className="h-4 w-4 text-orange-500" />;
      case 'hot': return <Flame className="h-4 w-4 text-red-500" />;
      case 'resident': return <Anchor className="h-4 w-4 text-purple-500" />;
      default: return <Activity className="h-4 w-4 text-gray-500" />;
    }
  };

  const getStateBadge = (state: string) => {
    const variants = {
      unloaded: 'bg-gray-100 text-gray-800',
      cold: 'bg-blue-100 text-blue-800',
      warm: 'bg-orange-100 text-orange-800',
      hot: 'bg-red-100 text-red-800',
      resident: 'bg-purple-100 text-purple-800'
    };
    return variants[state as keyof typeof variants] || 'bg-gray-100 text-gray-800';
  };

  const getCategoryIcon = (category: string) => {
    switch (category) {
      case 'code': return <Code className="h-4 w-4" />;
      case 'framework': return <Layers className="h-4 w-4" />;
      case 'codebase': return <GitBranch className="h-4 w-4" />;
      case 'ephemeral': return <Clock className="h-4 w-4" />;
      default: return <Code className="h-4 w-4" />;
    }
  };

  if (loading) {
    return <div className="text-center p-8">Loading adapters...</div>;
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
              ? 'border-green-200 bg-green-50'
              : statusMessage.variant === 'warning'
                ? 'border-amber-200 bg-amber-50'
                : 'border-blue-200 bg-blue-50'
          }
        >
          {statusMessage.variant === 'success' ? (
            <CheckCircle className="w-4 h-4 text-green-600" />
          ) : statusMessage.variant === 'warning' ? (
            <AlertCircle className="w-4 h-4 text-amber-600" />
          ) : (
            <AlertCircle className="w-4 h-4 text-blue-600" />
          )}
          <AlertDescription
            className={
              statusMessage.variant === 'success'
                ? 'text-green-700'
                : statusMessage.variant === 'warning'
                  ? 'text-amber-700'
                  : 'text-blue-700'
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
        <TabsContent value="registry" className="form-field">
          <Card className="card-standard">
            <CardHeader>
              <CardTitle className="flex-center">
                <Code className="icon-large mr-2" />
                Registered Adapters
              </CardTitle>
            </CardHeader>
            <CardContent>
              <Table className="table-standard">
                <TableHeader>
                  <TableRow>
                    <TableHead className="table-cell-standard w-12">
                      <Checkbox
                        checked={
                          adapters.length === 0
                            ? false
                            : selectedAdapters.length === adapters.length
                              ? true
                              : selectedAdapters.length > 0
                                ? 'indeterminate'
                                : false
                        }
                        onCheckedChange={(checked) => {
                          if (checked) {
                            setSelectedAdapters(adapters.map(a => a.adapter_id));
                          } else {
                            setSelectedAdapters([]);
                          }
                        }}
                        aria-label="Select all adapters"
                      />
                    </TableHead>
                    <TableHead className="table-cell-standard">Name</TableHead>
                    <TableHead className="table-cell-standard">Category</TableHead>
                    <TableHead className="table-cell-standard">State</TableHead>
                    <TableHead className="table-cell-standard">Memory</TableHead>
                    <TableHead className="table-cell-standard">Activations</TableHead>
                    <TableHead className="table-cell-standard">Last Used</TableHead>
                    <TableHead className="table-cell-standard">Actions</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {adapters.length === 0 ? (
                    <TableRow>
                      <TableCell colSpan={7} className="h-32">
                        <EmptyState
                          icon={Code}
                          title="No Adapters Registered"
                          description="Get started by registering your first adapter or training a new one from your codebase. Use the Register or Train buttons above to begin."
                        />
                      </TableCell>
                    </TableRow>
                  ) : (
                    adapters.map((adapter) => (
                      <TableRow key={adapter.id}>
                      <TableCell className="table-cell-standard">
                        <Checkbox
                          checked={selectedAdapters.includes(adapter.adapter_id)}
                          onCheckedChange={(checked) => {
                            if (checked) {
                              setSelectedAdapters(prev => [...prev, adapter.adapter_id]);
                            } else {
                              setSelectedAdapters(prev => prev.filter(id => id !== adapter.adapter_id));
                            }
                          }}
                          aria-label={`Select ${adapter.name}`}
                        />
                      </TableCell>
                      <TableCell className="table-cell-standard">
                        <div className="flex-center">
                          {getCategoryIcon(adapter.category)}
                          <div>
                            <div className="font-medium">{adapter.name}</div>
                            <div className="text-sm text-muted-foreground">
                              Tier {adapter.tier} • Rank {adapter.rank}
                            </div>
                          </div>
                        </div>
                      </TableCell>
                      <TableCell className="table-cell-standard">
                        <div className="status-indicator status-neutral flex-center">
                          {getCategoryIcon(adapter.category)}
                          <span>{adapter.category}</span>
                        </div>
                      </TableCell>
                      <TableCell className="table-cell-standard">
                        <div className="flex-center">
                          {getStateIcon(adapter.current_state)}
                          <div className={`status-indicator ${
                            adapter.current_state === 'hot' ? 'status-error' :
                            adapter.current_state === 'warm' ? 'status-warning' :
                            adapter.current_state === 'cold' ? 'status-info' :
                            adapter.current_state === 'resident' ? 'status-success' :
                            'status-neutral'
                          }`}>
                            {adapter.current_state}
                          </div>
                          {adapter.pinned && (
                            <Pin className="icon-standard text-purple-500" />
                          )}
                        </div>
                      </TableCell>
                      <TableCell className="table-cell-standard">
                        <div className="flex-center">
                          <MemoryStick className="icon-standard" />
                          <span>{Math.round(adapter.memory_bytes / 1024 / 1024)} MB</span>
                        </div>
                      </TableCell>
                      <TableCell className="table-cell-standard">
                        <div className="flex-center">
                          <Target className="icon-standard" />
                          <span>{adapter.activation_count}</span>
                        </div>
                      </TableCell>
                      <TableCell className="table-cell-standard">
                        <div className="flex-center">
                          <Clock className="icon-standard" />
                          <span>{adapter.last_activated ? new Date(adapter.last_activated).toLocaleString() : 'Never'}</span>
                        </div>
                      </TableCell>
                      <TableCell className="table-cell-standard">
                        <DropdownMenu>
                          <DropdownMenuTrigger asChild>
                            <Button variant="ghost" size="sm">
                              <MoreHorizontal className="h-4 w-4" />
                            </Button>
                          </DropdownMenuTrigger>
                          <DropdownMenuContent align="end">
                            {adapter.current_state === 'warm' || adapter.current_state === 'hot' || adapter.current_state === 'resident' ? (
                              <DropdownMenuItem onClick={() => handleUnloadAdapter(adapter.adapter_id)}>
                                <Pause className="mr-2 h-4 w-4" />
                                Unload
                              </DropdownMenuItem>
                            ) : (
                              <DropdownMenuItem onClick={() => handleLoadAdapter(adapter.adapter_id)}>
                                <Play className="mr-2 h-4 w-4" />
                                Load
                              </DropdownMenuItem>
                            )}
                            <DropdownMenuItem onClick={() => handlePinToggle(adapter)}>
                              <Pin className="mr-2 h-4 w-4" />
                              {adapter.pinned ? 'Unpin' : 'Pin'}
                            </DropdownMenuItem>
                            <DropdownMenuItem onClick={() => handlePromoteState(adapter.adapter_id)}>
                              <ArrowUp className="mr-2 h-4 w-4" />
                              Promote State
                            </DropdownMenuItem>
                            <DropdownMenuItem onClick={() => handleViewHealth(adapter.adapter_id)}>
                              <Activity className="mr-2 h-4 w-4" />
                              View Health
                            </DropdownMenuItem>
                            <DropdownMenuItem onClick={() => handleDownloadManifest(adapter.adapter_id)}>
                              <Download className="mr-2 h-4 w-4" />
                              Download Manifest
                            </DropdownMenuItem>
                            <DropdownMenuItem onClick={() => setDeleteConfirmId(adapter.adapter_id)}>
                              <Trash2 className="mr-2 h-4 w-4 text-red-600" />
                              Delete
                            </DropdownMenuItem>
                          </DropdownMenuContent>
                        </DropdownMenu>
                      </TableCell>
                    </TableRow>
                    ))
                  )}
                </TableBody>
              </Table>
            </CardContent>
          </Card>
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


        <TabsContent value="training" className="space-y-4">
          {selectedTrainingJob ? (
            <TrainingMonitor 
              jobId={selectedTrainingJob} 
              onClose={() => setSelectedTrainingJob(null)} 
            />
          ) : (
            <Card>
              <CardHeader>
                <CardTitle className="flex items-center">
                  <Brain className="mr-2 h-5 w-5" />
                  Training Jobs
                </CardTitle>
              </CardHeader>
              <CardContent>
                <div className="space-y-4">
                  {trainingJobs.map((job) => (
                    <Card key={job.id} className="cursor-pointer hover:shadow-md transition-shadow">
                      <CardContent className="pt-6">
                        <div className="flex items-center justify-between mb-4">
                          <div className="flex items-center space-x-2">
                            <Brain className="h-5 w-5" />
                            <h3 className="font-medium">{job.adapter_name}</h3>
                            <Badge className={getStateBadge(job.status)}>
                              {job.status}
                            </Badge>
                          </div>
                          <div className="flex space-x-2">
                            <Button 
                              variant="outline" 
                              size="sm"
                              onClick={() => setSelectedTrainingJob(job.id)}
                            >
                              <Eye className="h-4 w-4" />
                            </Button>
                            <Button variant="outline" size="sm">
                              <Pause className="h-4 w-4" />
                            </Button>
                            <Button variant="outline" size="sm">
                              <Square className="h-4 w-4" />
                            </Button>
                          </div>
                        </div>
                        
                        <div className="space-y-3">
                          <div className="flex items-center space-x-4">
                            <div className="flex-1">
                              <div className="flex items-center justify-between text-sm mb-1">
                                <span>Progress</span>
                                <span>{job.progress}%</span>
                              </div>
                              <Progress value={job.progress} />
                            </div>
                          </div>

                          <div className="grid grid-cols-2 md:grid-cols-4 gap-4 text-sm">
                            <div className="flex items-center space-x-2">
                              <Cpu className="h-4 w-4" />
                              <span>GPU: {job.metrics.gpu_utilization}%</span>
                            </div>
                            <div className="flex items-center space-x-2">
                              <MemoryStick className="h-4 w-4" />
                              <span>Memory: {job.metrics.memory_usage}GB</span>
                            </div>
                            <div className="flex items-center space-x-2">
                              <Zap className="h-4 w-4" />
                              <span>{job.metrics.tokens_per_second} tok/s</span>
                            </div>
                            <div className="flex items-center space-x-2">
                              <Target className="h-4 w-4" />
                              <span>Loss: {job.metrics.loss.toFixed(4)}</span>
                            </div>
                          </div>

                          <div className="text-sm text-muted-foreground">
                            Epoch {job.metrics.current_epoch}/{job.metrics.total_epochs} • 
                            Started {new Date(job.started_at).toLocaleString()}
                            {job.estimated_completion && (
                              <> • ETA {new Date(job.estimated_completion).toLocaleString()}</>
                            )}
                          </div>

                          <div className="bg-gray-50 p-3 rounded-md">
                            <div className="text-sm font-medium mb-2">Recent Logs</div>
                            <div className="space-y-1 text-xs font-mono">
                              {job.logs.slice(-3).map((log, idx) => (
                                <div key={idx} className="text-muted-foreground">{log}</div>
                              ))}
                            </div>
                          </div>
                        </div>
                      </CardContent>
                    </Card>
                  ))}
                </div>
              </CardContent>
            </Card>
          )}
        </TabsContent>

      </Tabs>

      </ContentSection>

      {/* Training Dialog */}
      <Dialog open={isTrainingDialogOpen} onOpenChange={setIsTrainingDialogOpen}>
        <DialogContent className="max-w-6xl max-h-[90vh] overflow-y-auto">
          <TrainingWizard
            onComplete={(jobId) => {
              showStatus(`Training job ${jobId} started.`, 'success');
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
        <DialogContent className="modal-large">
          <DialogHeader>
            <DialogTitle>Register Adapter</DialogTitle>
          </DialogHeader>
          <Tabs value={registerTab} onValueChange={(value) => setRegisterTab(value as 'upload' | 'path')}>
            <TabsList className="grid w-full grid-cols-2">
              <TabsTrigger value="upload">Upload .aos File</TabsTrigger>
              <TabsTrigger value="path">From Server Path</TabsTrigger>
            </TabsList>
            <TabsContent value="upload" className="space-y-4">
              <div className="space-y-4">
                <div
                  className="border-2 border-dashed rounded-lg p-12 text-center cursor-pointer hover:border-blue-500 transition-colors"
                  onClick={() => document.getElementById('adapter-file-input')?.click()}
                >
                  <input
                    id="adapter-file-input"
                    type="file"
                    accept=".aos,.safetensors"
                    onChange={(e) => setUploadFile(e.target.files?.[0] || null)}
                    className="hidden"
                  />
                  <FileText className="w-16 h-16 text-muted-foreground mx-auto mb-4" />
                  <p className="text-lg font-medium mb-2">
                    {uploadFile ? uploadFile.name : 'Click to select adapter file'}
                  </p>
                  <p className="text-sm text-muted-foreground">
                    Supports .aos and .safetensors files (max 100MB)
                  </p>
                </div>

                {uploadFile && (
                  <div className="bg-accent p-4 rounded-lg">
                    <div className="flex items-center justify-between mb-2">
                      <span className="font-medium">File Selected</span>
                      <Badge>{(uploadFile.size / 1024 / 1024).toFixed(2)} MB</Badge>
                    </div>
                    <p className="text-sm text-muted-foreground">{uploadFile.name}</p>
                  </div>
                )}

                <div className="flex justify-end gap-2">
              <Button variant="outline" onClick={() => setIsCreateDialogOpen(false)}>
                    Cancel
                  </Button>
                  <Button
                    onClick={async () => {
                      if (!uploadFile) {
                        showStatus('Please select a file before uploading.', 'warning');
                        return;
                      }

                      setIsUploading(true);
                      try {
                        const adapter = await apiClient.importAdapter(uploadFile, true);
                        setSuccessFeedback(
                          SuccessTemplates.adapterCreated(
                            adapter.name,
                            () => transitionTo('/inference?adapter=' + adapter.adapter_id),
                            () => setActiveTab('registry')
                          )
                        );
                        setIsCreateDialogOpen(false);
                        setUploadFile(null);
                        await loadAdapters();
                      } catch (err) {
                        setErrorRecovery(
                          ErrorRecoveryTemplates.genericError(
                            err instanceof Error ? err : new Error(String(err)),
                            () => {
                              // Retry logic could be added here
                              setErrorRecovery(null);
                            }
                          )
                        );
                      } finally {
                        setIsUploading(false);
                      }
                    }}
                    disabled={!uploadFile || isUploading}
                  >
                    {isUploading ? 'Uploading...' : 'Upload & Load'}
              </Button>
            </div>
          </div>
            </TabsContent>
            <TabsContent value="path" className="space-y-4">
              <div className="space-y-3">
                <div>
                  <Label>Tenant</Label>
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
                      ErrorRecoveryTemplates.genericError(
                        err instanceof Error ? err : new Error('Upsert failed'),
                        () => {
                          setErrorRecovery(null);
                        }
                      )
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
              <label className="form-label">Tenant</label>
              <Input value={selectedTenant} readOnly />
            </div>
            <div>
              <label className="form-label">Root (absolute)</label>
              <Input value={upsertRoot} onChange={(e) => setUpsertRoot(e.target.value)} placeholder="/abs/root" />
            </div>
            <div>
              <label className="form-label">Path (relative)</label>
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
                  ErrorRecoveryTemplates.genericError(
                    err instanceof Error ? err : new Error('Upsert failed'),
                    () => {
                      setErrorRecovery(null);
                    }
                  )
                );
              }
            }}>Submit</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Delete Confirmation Dialog */}
      <DeleteConfirmDialog
        open={deleteConfirmId !== null}
        adapterId={deleteConfirmId}
        onConfirm={handleDeleteAdapter}
        onCancel={() => setDeleteConfirmId(null)}
      />

      {/* Health Modal */}
      <Dialog open={showHealthModal} onOpenChange={setShowHealthModal}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Adapter Health</DialogTitle>
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

    </div>
  );
}

// Delete Confirmation Dialog
function DeleteConfirmDialog({ 
  open, 
  adapterId, 
  onConfirm, 
  onCancel 
}: { 
  open: boolean; 
  adapterId: string | null; 
  onConfirm: (id: string) => void; 
  onCancel: () => void;
}) {
  if (!open || !adapterId) return null;

  return (
    <Dialog open={open} onOpenChange={(open) => !open && onCancel()}>
      <DialogContent className="modal-standard">
        <DialogHeader>
          <DialogTitle>Confirm Delete</DialogTitle>
        </DialogHeader>
        <Alert variant="destructive">
          <AlertTriangle className="icon-standard" />
          <AlertDescription>
            Are you sure you want to delete adapter <code className="font-mono">{adapterId}</code>? This action cannot be undone.
          </AlertDescription>
        </Alert>
        <div className="flex-standard justify-end">
          <Button variant="outline" onClick={onCancel}>
            Cancel
          </Button>
          <Button variant="destructive" onClick={() => onConfirm(adapterId)}>
            Delete Adapter
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
