import React, { useState, useEffect, useCallback } from 'react';
import { useParams, Link, useNavigate } from 'react-router-dom';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { buildTrainingOverviewLink, buildTrainingJobsLink, buildTrainingJobDetailLink, buildTrainingJobChatLink, buildDatasetDetailLink } from '@/utils/navLinks';
import FeatureLayout from '@/layout/FeatureLayout';
import { DensityProvider } from '@/contexts/DensityContext';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Progress } from '@/components/ui/progress';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  ArrowLeft,
  Activity,
  CheckCircle,
  XCircle,
  Clock,
  Pause,
  Square,
  RefreshCw,
  Download,
  FileText,
  BarChart3,
  Package,
  Settings,
  AlertTriangle,
  ExternalLink,
  Box,
  Layers,
  MessageSquare,
  Send,
  Power,
} from 'lucide-react';
import { apiClient } from '@/api/services';
import { useLiveData } from '@/hooks/realtime/useLiveData';
import { logger } from '@/utils/logger';
import { useToast } from '@/hooks/use-toast';
import { useTenant } from '@/providers/FeatureProviders';
import { StackFormModal } from '@/pages/Admin/StackFormModal';
import { PublishAdapterDialog } from '@/components/training/PublishAdapterDialog';
import type {
  TrainingJob,
  TrainingMetrics,
  TrainingArtifact,
  TrainingConfig,
  BackendAttempt,
  TrainingErrorCategory,
} from '@/api/training-types';
import type { TrainingProgressEvent } from '@/api/streaming-types';

type BackendTimelineEntry = {
  backend: string;
  result: 'selected' | 'failed' | 'skipped';
  reason?: string;
  errorCategory?: TrainingErrorCategory;
  coremlDevice?: string;
};

const normalizeBackendPolicy = (policy?: string): string => {
  if (!policy) return 'unknown';
  const normalized = policy.toLowerCase().replace(/-/g, '_');
  switch (normalized) {
    case 'auto':
      return 'auto';
    case 'coreml_only':
    case 'coremlonly':
      return 'coreml_only';
    case 'coreml_else_fallback':
    case 'coreml_else':
    case 'coreml_fallback':
    case 'coreml_elsefallback':
      return 'coreml_else_fallback';
    default:
      return policy;
  }
};

const formatDuration = (seconds?: number) => {
  if (seconds === undefined || Number.isNaN(seconds)) return 'N/A';
  const mins = Math.floor(seconds / 60);
  const secs = Math.floor(seconds % 60);
  const hours = Math.floor(mins / 60);
  const remainingMins = mins % 60;
  if (hours > 0) return `${hours}h ${remainingMins}m`;
  if (mins > 0) return `${mins}m ${secs}s`;
  return `${secs}s`;
};

export function buildBackendTimeline(job: TrainingJob, backendLabel: string): BackendTimelineEntry[] {
  const backendAttempts = job.backend_attempts;
  if (backendAttempts && backendAttempts.length > 0) {
    return backendAttempts.map((attempt: BackendAttempt) => ({
      backend: attempt.backend,
      result: attempt.result ?? 'selected',
      reason: attempt.reason,
      errorCategory: attempt.error_category,
      coremlDevice: attempt.coreml?.device_type,
    }));
  }

  const requested = job.requested_backend;
  const backendReason = job.backend_reason ?? undefined;

  if (requested && backendLabel && requested !== backendLabel) {
    return [
      {
        backend: requested,
        result: 'failed',
        reason: backendReason || 'Fallback from requested backend',
        errorCategory: (requested.toLowerCase().includes('coreml') ? 'coreml_compile' : undefined) as TrainingErrorCategory | undefined,
      },
      {
        backend: backendLabel,
        result: 'selected',
        reason: 'Used for this run',
      },
    ];
  }

  if (backendLabel && backendLabel !== 'unknown') {
    return [{
      backend: backendLabel,
      result: 'selected',
      reason: backendReason,
    }];
  }

  return [];
}

export function classifyErrorCategory(job: TrainingJob): TrainingErrorCategory | undefined {
  const errorCategory = job.error_category;
  if (errorCategory) return errorCategory;

  const datasetTrustState = job.dataset_trust_state;
  if (datasetTrustState === 'blocked' || datasetTrustState === 'needs_approval') {
    return 'dataset_trust';
  }

  const errorDetail = job.error_detail;
  const message = (job.error_message ?? errorDetail ?? '').toLowerCase();
  if (!message) return undefined;

  if (message.includes('coreml')) return 'coreml_compile';
  if (message.includes('storage') || message.includes('filesystem') || message.includes('fs ')) return 'storage';
  if (message.includes('backend') || message.includes('device')) return 'backend';

  return 'other';
}

function TrainingJobDetailContent() {
  const { jobId } = useParams<{ jobId: string }>();
  const navigate = useNavigate();
  const { toast } = useToast();
  const queryClient = useQueryClient();
  const { selectedTenant } = useTenant();

  const [activeTab, setActiveTab] = useState('overview');
  const [stackModalOpen, setStackModalOpen] = useState(false);
  const [publishDialogOpen, setPublishDialogOpen] = useState(false);
  const [downloadingArtifacts, setDownloadingArtifacts] = useState<Set<string>>(() => new Set());
  const [isActivatingAdapter, setIsActivatingAdapter] = useState(false);
  const missingJobId = !jobId;

  // Fetch job data using React Query
  const isJobActive = (job: TrainingJob | null) =>
    job?.status === 'running' || job?.status === 'pending';

  const {
    data: job = null,
    isLoading: isJobLoading,
    error: jobError,
    refetch: refetchJob
  } = useQuery({
    queryKey: ['training-job', jobId],
    queryFn: () => apiClient.getTrainingJob(jobId!),
    enabled: !!jobId,
    refetchInterval: (query) => isJobActive(query.state.data ?? null) ? 5000 : false,
  });

  // Fetch metrics using React Query
  const { data: metrics, isLoading: isMetricsLoading } = useQuery({
    queryKey: ['training-job-metrics', jobId],
    queryFn: async () => {
      if (!jobId) return null;
      return await apiClient.getTrainingMetrics(jobId);
    },
    enabled: !!jobId,
    refetchInterval: () => isJobActive(job) ? 5000 : false,
  });

  // Fetch artifacts using React Query
  const { data: artifacts = [], isLoading: isArtifactsLoading } = useQuery({
    queryKey: ['training-job-artifacts', jobId],
    queryFn: async () => {
      if (!jobId) return [];
      const result = await apiClient.getTrainingArtifacts(jobId);
      return result.artifacts || [];
    },
    enabled: !!jobId,
  });

  // Fetch logs using React Query with automatic polling when job is active
  const { data: logs = [], isLoading: isLogsLoading, refetch: refetchLogs } = useQuery({
    queryKey: ['training-job-logs', jobId],
    queryFn: () => apiClient.getTrainingLogs(jobId!),
    enabled: !!jobId,
    refetchInterval: isJobActive(job) ? 5000 : false,
  });

  // SSE for real-time updates when job is active
  const isJobCurrentlyActive = isJobActive(job);

  useLiveData({
    sseEndpoint: '/v1/streams/training',
    sseEventType: 'training',
    fetchFn: async () => {
      // Polling fallback - fetch job details
      if (!jobId) return null;
      const jobData = await apiClient.getTrainingJob(jobId);
      return jobData;
    },
    enabled: isJobCurrentlyActive && !!jobId,
    pollingSpeed: 'fast',
    onSSEMessage: (event) => {
      const progressEvent = event as TrainingProgressEvent;
      if (progressEvent.job_id === jobId) {
        // Update job data in React Query cache
        queryClient.setQueryData(['training-job', jobId], (prev: TrainingJob | undefined) =>
          prev ? {
            ...prev,
            status: progressEvent.status,
            progress_pct: progressEvent.progress_pct,
            current_epoch: progressEvent.current_epoch,
            total_epochs: progressEvent.total_epochs,
            current_loss: progressEvent.current_loss,
            tokens_per_second: progressEvent.tokens_per_second,
            tokens_processed: progressEvent.tokens_processed ?? prev.tokens_processed,
            eta_seconds: progressEvent.estimated_time_remaining_sec,
            error_message: progressEvent.error,
          } : prev
        );

        if (progressEvent.current_loss !== undefined) {
          queryClient.setQueryData(['training-job-metrics', jobId], (prev: TrainingMetrics | undefined) => ({
            ...prev,
            loss: progressEvent.current_loss,
            learning_rate: progressEvent.learning_rate,
            epoch: progressEvent.current_epoch,
            progress_pct: progressEvent.progress_pct,
            tokens_per_second: progressEvent.tokens_per_second,
            tokens_processed: progressEvent.tokens_processed ?? prev?.tokens_processed,
          }));
        }
      }
    },
  });

  const handleDownloadArtifact = useCallback(async (artifact: TrainingArtifact) => {
    if (!jobId) return;
    setDownloadingArtifacts((prev) => {
      const next = new Set(prev);
      next.add(artifact.id);
      return next;
    });

    try {
      const filename = artifact.path?.split('/').pop() || `${artifact.type}-${artifact.id}`;
      await apiClient.downloadArtifact(jobId, artifact.id, filename);
      toast({ title: 'Download started', description: filename });
    } catch (error) {
      toast({
        title: 'Download failed',
        description: error instanceof Error ? error.message : 'Failed to download artifact',
        variant: 'destructive',
      });
    } finally {
      setDownloadingArtifacts((prev) => {
        const next = new Set(prev);
        next.delete(artifact.id);
        return next;
      });
    }
  }, [jobId, toast]);

  const handleStackCreated = useCallback((stackId: string) => {
    setStackModalOpen(false);
    toast({
      title: 'Stack created',
      description: 'Your adapter has been added to the stack.',
    });
    // Show a follow-up action to use the stack in chat
    setTimeout(() => {
      toast({
        title: 'Ready to use',
        description: 'Navigate to Chat to use your new stack.',
      });
    }, 2000);
  }, [toast]);

  const handleActivateAdapter = useCallback(async () => {
    if (!job?.adapter_id || isActivatingAdapter) return;
    setIsActivatingAdapter(true);
    try {
      await apiClient.activateAdapter(job.adapter_id, {
        workspace_id: selectedTenant ?? undefined,
      });
      toast({
        title: 'Adapter activated',
        description: selectedTenant
          ? `Activated for workspace ${selectedTenant}.`
          : 'Activated for your workspace.',
      });
    } catch (error) {
      const message = error instanceof Error ? error.message : 'Failed to activate adapter';
      toast({
        title: 'Activation failed',
        description: message,
        variant: 'destructive',
      });
      logger.error('Failed to activate adapter', { adapterId: job.adapter_id }, error as Error);
    } finally {
      setIsActivatingAdapter(false);
    }
  }, [job?.adapter_id, isActivatingAdapter, selectedTenant, toast]);

  if (missingJobId) {
    return (
      <Card className="border-destructive">
        <CardContent className="pt-6">
          <p className="text-destructive">Missing training job ID.</p>
          <div className="mt-3">
            <Button variant="outline" onClick={() => navigate(buildTrainingJobsLink())}>
              <ArrowLeft className="h-4 w-4 mr-2" />
              Back to jobs
            </Button>
          </div>
        </CardContent>
      </Card>
    );
  }

  if (isJobLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <RefreshCw className="h-8 w-8 animate-spin text-muted-foreground" />
      </div>
    );
  }

  if (jobError || !job) {
    const errorMessage = jobError instanceof Error ? jobError.message : 'Job not found';
    return (
      <Card className="border-destructive">
        <CardContent className="pt-6">
          <div className="flex items-center gap-2 text-destructive mb-4">
            <AlertTriangle className="h-5 w-5" />
            <span>{errorMessage}</span>
          </div>
          <div className="flex gap-2">
            <Button variant="outline" onClick={() => navigate(buildTrainingOverviewLink())}>
              <ArrowLeft className="h-4 w-4 mr-2" />
              Back to Jobs
            </Button>
            <Button variant="outline" onClick={() => refetchJob()}>
              <RefreshCw className="h-4 w-4 mr-2" />
              Retry
            </Button>
          </div>
        </CardContent>
      </Card>
    );
  }

  const statusConfig: Record<string, { icon: React.ElementType; className: string; label: string }> = {
    pending: { icon: Clock, className: 'text-yellow-500', label: 'Pending' },
    running: { icon: Activity, className: 'text-blue-500 animate-pulse', label: 'Running' },
    completed: { icon: CheckCircle, className: 'text-green-500', label: 'Completed' },
    failed: { icon: XCircle, className: 'text-red-500', label: 'Failed' },
    cancelled: { icon: Square, className: 'text-gray-500', label: 'Cancelled' },
    paused: { icon: Pause, className: 'text-orange-500', label: 'Paused' },
  };

  const StatusIcon = statusConfig[job.status]?.icon || Clock;
  const statusClass = statusConfig[job.status]?.className || 'text-gray-500';
  const backendLabel = job.backend ?? metrics?.backend ?? 'unknown';
  const requestedBackend = job.requested_backend ?? undefined;
  const coremlFallback = job.coreml_training_fallback ?? undefined;
  const backendDevice = job.backend_device ?? metrics?.backend_device ?? undefined;
  const backendDeviceLower = (backendDevice ?? '').toLowerCase();
  const backendLower = (job.backend ?? '').toLowerCase();
  const determinismLabel = job.determinism_mode ?? 'n/a';
  const seedLabel = job.training_seed !== undefined ? `seed ${job.training_seed}` : null;
  const backendPolicyMode = normalizeBackendPolicy(
    job.backend_policy ?? undefined
  );
  const backendTimeline = buildBackendTimeline(job, backendLabel);
  const errorCategory = classifyErrorCategory(job);
  const usingGpu = (metrics?.using_gpu ?? job.require_gpu ?? false)
    || backendDeviceLower.includes('gpu')
    || backendDeviceLower.includes('ane')
    || backendLower.includes('metal')
    || backendLower.includes('coreml');
  const latestLoss = metrics?.loss ?? job.current_loss;
  const tokensProcessed = job.tokens_processed ?? metrics?.tokens_processed ?? undefined;
  const examplesProcessed = job.examples_processed ?? metrics?.examples_processed ?? undefined;
  const tokensPerSecond = job.tokens_per_second ?? metrics?.tokens_per_second;
  const examplesPerSecond = metrics?.throughput_examples_per_sec ?? job.throughput_examples_per_sec ?? undefined;
  // Note: batch_size, loss_curve, drift_metrics not in generated types yet
  const batchSize = job.config?.batch_size;
  const lossCurveSnippet = job.loss_curve?.slice(-5);
  const driftMetrics = job.drift_metrics;
  const durationSeconds = job.training_time_ms
    ? job.training_time_ms / 1000
    : (job.started_at && job.completed_at
      ? (new Date(job.completed_at).getTime() - new Date(job.started_at).getTime()) / 1000
      : undefined);
  const repoLink = job.repo_id ? `/repos/${job.repo_id}` : undefined;
  const coremlDeviceType = job.coreml_device_type
    || backendTimeline.find(entry => entry.coremlDevice)?.coremlDevice
    || backendDevice;
  const coremlAttempted = job.coreml_attempted
    ?? backendTimeline.some(entry => entry.backend.toLowerCase().includes('coreml'))
    ?? (requestedBackend?.toLowerCase().includes('coreml'));
  const coremlUsed = job.coreml_used ?? backendLower.includes('coreml');
  const datasetVersionTrust = job.dataset_version_trust || [];
  // Use produced_version_id (newer) or fall back to adapter_version_id (both are defined in TrainingJob)
  const adapterVersionId = job.produced_version_id || job.adapter_version_id;
  const hasPerformanceMetrics = latestLoss !== undefined
    || tokensProcessed !== undefined
    || examplesProcessed !== undefined
    || tokensPerSecond !== undefined
    || examplesPerSecond !== undefined;
  const hasMetricsData = hasPerformanceMetrics
    || durationSeconds !== undefined
    || batchSize !== undefined
    || (lossCurveSnippet && lossCurveSnippet.length > 0)
    || driftMetrics !== undefined;
  const datasetTrustState = job.dataset_trust_state;
  const datasetTrustReason = job.dataset_trust_reason;
  const dataSpecHash = job.data_spec_hash;
  const formatCount = (value?: number) => value !== undefined ? value.toLocaleString() : 'N/A';
  const formatRate = (value?: number, fractionDigits = 1) =>
    value !== undefined
      ? value.toLocaleString(undefined, { minimumFractionDigits: fractionDigits, maximumFractionDigits: fractionDigits })
      : 'N/A';

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-4">
          <Button variant="ghost" size="sm" onClick={() => navigate(buildTrainingOverviewLink())}>
            <ArrowLeft className="h-4 w-4 mr-2" />
            Back
          </Button>
          <div>
            <p className="text-sm text-muted-foreground">Job ID: {job.id}</p>
          </div>
        </div>
        <div className="flex items-center gap-2">
          {job.status === 'completed' && job.stack_id && (
            <Button
              onClick={() => navigate(buildTrainingJobChatLink(job.id))}
              variant="default"
              data-testid="open-result-chat"
            >
              <MessageSquare className="h-4 w-4 mr-2" />
              Open Result Chat
            </Button>
          )}
          {job.status === 'completed' && job.adapter_id && (
            <>
              <Button
                onClick={handleActivateAdapter}
                variant="default"
                disabled={isActivatingAdapter}
              >
                <Power className="h-4 w-4 mr-2" />
                {isActivatingAdapter ? 'Activating...' : 'Activate Adapter'}
              </Button>
              {/* Publish button - show for completed jobs with produced version */}
              {job.repo_id && job.produced_version_id && (
                <Button
                  onClick={() => setPublishDialogOpen(true)}
                  variant="default"
                  data-testid="publish-adapter-button"
                >
                  <Send className="h-4 w-4 mr-2" />
                  Publish Adapter
                </Button>
              )}
              <Button
                onClick={() => navigate(`/adapters/${job.adapter_id}`)}
                variant={job.stack_id ? 'outline' : 'default'}
              >
                <Box className="h-4 w-4 mr-2" />
                View Adapter
              </Button>
              <Button
                onClick={() => setStackModalOpen(true)}
                variant="outline"
              >
                <Layers className="h-4 w-4 mr-2" />
                Add to Stack
              </Button>
            </>
          )}
          <Badge variant="outline" className={`gap-1 ${statusClass}`}>
            <StatusIcon className="h-4 w-4" />
            {statusConfig[job.status]?.label || job.status}
          </Badge>
        </div>
      </div>

      {/* Progress Card */}
      <Card>
        <CardHeader>
          <CardTitle>Training Progress</CardTitle>
          <CardDescription>
            {job.current_epoch !== undefined && job.total_epochs !== undefined
              ? `Epoch ${job.current_epoch} of ${job.total_epochs}`
              : 'Progress overview'}
          </CardDescription>
        </CardHeader>
        <CardContent>
          <div className="space-y-4">
            <div className="flex items-center gap-4">
              <Progress value={job.progress_pct ?? job.progress ?? 0} className="flex-1" />
              <span className="text-lg font-medium min-w-[60px] text-right">
                {job.progress_pct ?? job.progress ?? 0}%
              </span>
            </div>

            <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
              <div className="p-3 bg-muted rounded-lg">
                <div className="text-sm text-muted-foreground">Loss</div>
                <div className="text-lg font-mono">
                  {job.current_loss?.toFixed(4) ?? job.loss?.toFixed(4) ?? '-'}
                </div>
              </div>
              <div className="p-3 bg-muted rounded-lg">
                <div className="text-sm text-muted-foreground">Learning Rate</div>
                <div className="text-lg font-mono">
                  {job.learning_rate?.toExponential(2) ?? metrics?.learning_rate?.toExponential(2) ?? '-'}
                </div>
              </div>
              <div className="p-3 bg-muted rounded-lg">
                <div className="text-sm text-muted-foreground">Tokens/sec</div>
                <div className="text-lg font-mono">
                  {job.tokens_per_second?.toFixed(1) ?? '-'}
                </div>
              </div>
              <div className="p-3 bg-muted rounded-lg">
                <div className="text-sm text-muted-foreground">ETA</div>
                <div className="text-lg font-mono">
                  {job.eta_seconds
                    ? `${Math.floor(job.eta_seconds / 60)}m ${job.eta_seconds % 60}s`
                    : '-'}
                </div>
              </div>
            </div>

            {job.error_message && (
              <div className="p-3 bg-destructive/10 text-destructive rounded-lg">
                <div className="font-medium flex items-center gap-2">
                  <span>Error</span>
                  {errorCategory && (
                    <Badge variant="outline" className="text-destructive border-destructive">
                      {errorCategory}
                    </Badge>
                  )}
                </div>
                <div className="text-sm">{job.error_message}</div>
                <div className="mt-2">
                  <Button size="sm" variant="outline" onClick={() => setActiveTab('logs')}>
                    View logs
                  </Button>
                </div>
              </div>
            )}
          </div>
        </CardContent>
      </Card>

      {/* Tabs for Logs, Metrics, Artifacts, Config */}
      <Tabs value={activeTab} onValueChange={setActiveTab}>
        <TabsList>
          <TabsTrigger value="overview" className="gap-2">
            <BarChart3 className="h-4 w-4" />
            Overview
          </TabsTrigger>
          <TabsTrigger value="logs" className="gap-2">
            <FileText className="h-4 w-4" />
            Logs ({logs.length})
          </TabsTrigger>
          <TabsTrigger value="artifacts" className="gap-2">
            <Package className="h-4 w-4" />
            Artifacts ({artifacts.length})
          </TabsTrigger>
          <TabsTrigger value="config" className="gap-2">
            <Settings className="h-4 w-4" />
            Config
          </TabsTrigger>
        </TabsList>

        <TabsContent value="overview" className="mt-4">
          <Card className="mb-4">
            <CardHeader>
              <CardTitle>Backend & Determinism</CardTitle>
              <CardDescription>Placement and reproducibility for this run</CardDescription>
            </CardHeader>
            <CardContent>
            <dl className="grid grid-cols-2 md:grid-cols-4 gap-4">
                <div>
                  <dt className="text-sm text-muted-foreground">Backend</dt>
                  <dd className="flex items-center gap-2 text-sm">
                    <span>{backendLabel}</span>
                    <Badge variant={usingGpu ? 'default' : 'secondary'} className="text-[10px]">
                      {usingGpu ? 'GPU/Accelerator' : 'CPU'}
                    </Badge>
                  </dd>
                  {job.backend_reason && (
                    <p className="text-xs text-muted-foreground mt-1">{job.backend_reason}</p>
                  )}
                  {requestedBackend && requestedBackend !== backendLabel && (
                    <p className="text-xs text-muted-foreground mt-1">
                      Requested: {requestedBackend}
                    </p>
                  )}
                </div>
                <div>
                  <dt className="text-sm text-muted-foreground">Backend Policy</dt>
                  <dd className="text-sm">
                    {backendPolicyMode}
                  </dd>
                  {coremlFallback && (
                    <p className="text-xs text-muted-foreground mt-1">
                      Fallback: {coremlFallback}
                    </p>
                  )}
                </div>
                <div>
                  <dt className="text-sm text-muted-foreground">Backend Device</dt>
                  <dd className="text-sm">
                    {backendDevice || 'device n/a'}
                  </dd>
                </div>
                <div>
                  <dt className="text-sm text-muted-foreground">Determinism</dt>
                  <dd className="text-sm">
                    {determinismLabel}
                    {seedLabel && (
                      <span className="font-mono text-xs text-muted-foreground ml-2">{seedLabel}</span>
                    )}
                  </dd>
                </div>
                <div>
                  <dt className="text-sm text-muted-foreground">CoreML</dt>
                  <dd className="text-sm space-y-1">
                    <div>Attempted: {coremlAttempted ? 'yes' : 'no'}</div>
                    <div>Used: {coremlUsed ? 'yes' : 'no'}</div>
                    <div>Device: {coremlDeviceType || 'n/a'}</div>
                  </dd>
                </div>
              </dl>

            {backendTimeline.length > 0 && (
              <div className="mt-4 space-y-2">
                <div className="text-sm font-medium">Fallback path</div>
                <div className="space-y-2">
                  {backendTimeline.map((attempt, idx) => (
                    <div
                      key={`${attempt.backend}-${idx}`}
                      className="flex flex-col md:flex-row md:items-center md:justify-between rounded-lg border p-3 gap-2"
                    >
                      <div className="flex items-center gap-2">
                        <Badge variant="outline">{attempt.backend}</Badge>
                        <Badge variant={attempt.result === 'selected' ? 'default' : attempt.result === 'failed' ? 'destructive' : 'secondary'}>
                          {attempt.result}
                        </Badge>
                        {attempt.coremlDevice && (
                          <Badge variant="secondary" className="text-[10px]">
                            {attempt.coremlDevice}
                          </Badge>
                        )}
                      </div>
                      <div className="text-xs text-muted-foreground flex-1">
                        {attempt.reason || 'No reason provided'}
                        {attempt.errorCategory && (
                          <span className="text-destructive ml-2">[{attempt.errorCategory}]</span>
                        )}
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            )}
            </CardContent>
          </Card>

          <Card className="mb-4">
            <CardHeader>
              <CardTitle>Performance</CardTitle>
              <CardDescription>Latest training throughput metrics</CardDescription>
            </CardHeader>
            <CardContent>
              {hasMetricsData ? (
                <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-4">
                  <div className="p-3 bg-muted rounded-lg">
                    <div className="text-sm text-muted-foreground">Latest Loss</div>
                    <div className="text-lg font-mono">
                      {latestLoss !== undefined ? latestLoss.toFixed(4) : 'N/A'}
                    </div>
                  </div>
                  {lossCurveSnippet && lossCurveSnippet.length > 0 && (
                    <div className="p-3 bg-muted rounded-lg">
                      <div className="text-sm text-muted-foreground">Loss Curve (last)</div>
                      <div className="text-xs font-mono">
                        {lossCurveSnippet.map((val: number) => val.toFixed(3)).join(', ')}
                      </div>
                    </div>
                  )}
                  <div className="p-3 bg-muted rounded-lg">
                    <div className="text-sm text-muted-foreground">Duration</div>
                    <div className="text-lg font-mono">{formatDuration(durationSeconds)}</div>
                  </div>
                  <div className="p-3 bg-muted rounded-lg">
                    <div className="text-sm text-muted-foreground">Batch Size</div>
                    <div className="text-lg font-mono">{batchSize ?? 'N/A'}</div>
                  </div>
                  <div className="p-3 bg-muted rounded-lg">
                    <div className="text-sm text-muted-foreground">Tokens Processed</div>
                    <div className="text-lg font-mono">{formatCount(tokensProcessed)}</div>
                    {examplesProcessed !== undefined && (
                      <div className="text-xs text-muted-foreground">
                        Examples: {formatCount(examplesProcessed)}
                      </div>
                    )}
                  </div>
                  <div className="p-3 bg-muted rounded-lg">
                    <div className="text-sm text-muted-foreground">Tokens/sec</div>
                    <div className="text-lg font-mono">{formatRate(tokensPerSecond, 1)}</div>
                  </div>
                  <div className="p-3 bg-muted rounded-lg">
                    <div className="text-sm text-muted-foreground">Examples/sec</div>
                    <div className="text-lg font-mono">{formatRate(examplesPerSecond, 2)}</div>
                  </div>
                  {driftMetrics && (
                    <div className="p-3 bg-muted rounded-lg">
                      <div className="text-sm text-muted-foreground">Drift</div>
                      <div className="text-sm">
                        Score: <span className="font-mono">{String(driftMetrics.drift_score ?? 'n/a')}</span>
                      </div>
                      {typeof driftMetrics.drift_tokens === 'number' && (
                        <div className="text-xs text-muted-foreground">
                          Tokens: {formatCount(driftMetrics.drift_tokens)}
                        </div>
                      )}
                    </div>
                  )}
                </div>
              ) : (
                <div className="text-sm text-muted-foreground">
                  Performance metrics not available for this job
                </div>
              )}
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle>Job Information</CardTitle>
            </CardHeader>
            <CardContent>
              <dl className="grid grid-cols-2 gap-4">
                <div>
                  <dt className="text-sm text-muted-foreground">Repository</dt>
                  <dd className="flex items-center gap-2 text-sm">
                    {job.repo_id ? (
                      <>
                        <span className="font-mono">{job.repo_id}</span>
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => repoLink && navigate(repoLink)}
                        >
                          <ExternalLink className="h-3 w-3" />
                        </Button>
                      </>
                    ) : (
                      'n/a'
                    )}
                  </dd>
                  {job.branch && (
                    <p className="text-xs text-muted-foreground">Branch: {job.branch}</p>
                  )}
                </div>
                <div>
                  <dt className="text-sm text-muted-foreground">Dataset ID</dt>
                  <dd className="flex items-center gap-2">
                    <span className="font-mono text-sm">{job.dataset_id || '-'}</span>
                    {job.dataset_id && (
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => navigate(buildDatasetDetailLink(job.dataset_id!))}
                      >
                        <ExternalLink className="h-3 w-3" />
                      </Button>
                    )}
                  </dd>
                </div>
                {datasetTrustState && (
                  <div>
                    <dt className="text-sm text-muted-foreground">Dataset Trust</dt>
                    <dd className="text-sm">
                      <Badge variant={datasetTrustState === 'blocked' || datasetTrustState === 'needs_approval' ? 'destructive' : 'outline'}>
                        {datasetTrustState}
                      </Badge>
                      {datasetTrustReason && (
                        <span className="text-xs text-muted-foreground ml-2">{datasetTrustReason}</span>
                      )}
                    </dd>
                  </div>
                )}
                {job.dataset_version_ids && job.dataset_version_ids.length > 0 && (
                  <div className="col-span-2">
                    <dt className="text-sm text-muted-foreground">Dataset Versions</dt>
                    <dd className="space-y-2">
                      {job.dataset_version_ids.map((v) => {
                        const trust = datasetVersionTrust.find(t => t.dataset_version_id === v.dataset_version_id);
                        return (
                          <div key={v.dataset_version_id} className="flex items-center justify-between rounded border p-2">
                            <div className="flex items-center gap-2">
                              <Badge variant="outline">{v.dataset_version_id}</Badge>
                              {v.weight !== undefined && (
                                <span className="text-xs text-muted-foreground">wt {v.weight}</span>
                              )}
                            </div>
                            <div className="flex items-center gap-2">
                              {trust?.trust_at_training_time && (
                                <Badge variant={trust.trust_at_training_time === 'blocked' ? 'destructive' : 'secondary'}>
                                  {trust.trust_at_training_time}
                                </Badge>
                              )}
                              {job.dataset_id && (
                                <Button
                                  variant="ghost"
                                  size="sm"
                                  onClick={() => navigate(buildDatasetDetailLink(job.dataset_id!, { datasetVersionId: v.dataset_version_id }))}
                                >
                                  <ExternalLink className="h-3 w-3" />
                                </Button>
                              )}
                            </div>
                          </div>
                        );
                      })}
                    </dd>
                  </div>
                )}
                {job.adapter_id && (
                  <div>
                    <dt className="text-sm text-muted-foreground">Result Adapter ID</dt>
                    <dd className="flex items-center gap-2">
                      <span className="font-mono text-sm">{job.adapter_id}</span>
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => navigate(`/adapters/${job.adapter_id}`)}
                      >
                        <ExternalLink className="h-3 w-3" />
                      </Button>
                    </dd>
                  </div>
                )}
                {adapterVersionId && (
                  <div>
                    <dt className="text-sm text-muted-foreground">Adapter Version</dt>
                    <dd className="flex items-center gap-2">
                      <span className="font-mono text-sm">{adapterVersionId}</span>
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => navigate(`/adapters/${adapterVersionId}`)}
                      >
                        <ExternalLink className="h-3 w-3" />
                      </Button>
                    </dd>
                  </div>
                )}
                <div>
                  <dt className="text-sm text-muted-foreground">Template ID</dt>
                  <dd className="font-mono text-sm">{job.template_id || '-'}</dd>
                </div>
                <div>
                  <dt className="text-sm text-muted-foreground">Base Model</dt>
                  <dd className="font-mono text-sm">{job.base_model_id || 'n/a'}</dd>
                </div>
                {dataSpecHash && (
                  <div>
                    <dt className="text-sm text-muted-foreground">Data Spec Hash</dt>
                    <dd className="font-mono text-sm break-all">{dataSpecHash}</dd>
                  </div>
                )}
                <div>
                  <dt className="text-sm text-muted-foreground">GPU Requirements</dt>
                  <dd className="text-sm">
                    {job.require_gpu ? 'GPU required' : 'GPU optional'}
                    {job.max_gpu_memory_mb !== undefined && (
                      <span className="text-muted-foreground ml-2">{job.max_gpu_memory_mb} MB max</span>
                    )}
                  </dd>
                </div>
                {job.initiated_by && (
                  <div>
                    <dt className="text-sm text-muted-foreground">Submitted By</dt>
                    <dd className="text-sm">{job.initiated_by}</dd>
                  </div>
                )}
                <div>
                  <dt className="text-sm text-muted-foreground">Created</dt>
                  <dd className="text-sm">
                    {job.created_at ? new Date(job.created_at).toLocaleString() : '-'}
                  </dd>
                </div>
                <div>
                  <dt className="text-sm text-muted-foreground">Started</dt>
                  <dd className="text-sm">
                    {job.started_at ? new Date(job.started_at).toLocaleString() : '-'}
                  </dd>
                </div>
                <div>
                  <dt className="text-sm text-muted-foreground">Completed</dt>
                  <dd className="text-sm">
                    {job.completed_at ? new Date(job.completed_at).toLocaleString() : '-'}
                  </dd>
                </div>
                <div>
                  <dt className="text-sm text-muted-foreground">Output Path</dt>
                  <dd className="font-mono text-sm truncate" title={job.output_path || '-'}>
                    {job.output_path || '-'}
                  </dd>
                </div>
                <div>
                  <dt className="text-sm text-muted-foreground">Package (.aos)</dt>
                  <dd className="font-mono text-sm truncate" title={job.aos_path || '-'}>
                    {job.aos_path || '-'}
                  </dd>
                </div>
                <div>
                  <dt className="text-sm text-muted-foreground">Package Hash</dt>
                  <dd className="font-mono text-sm truncate" title={job.package_hash_b3 || job.weights_hash_b3 || '-'}>
                    {job.package_hash_b3 || job.weights_hash_b3 || '-'}
                  </dd>
                </div>
                <div>
                  <dt className="text-sm text-muted-foreground">Manifest</dt>
                  <dd className="text-sm">
                    {job.manifest_rank !== undefined ? `rank ${job.manifest_rank}` : 'rank n/a'}
                    {job.manifest_base_model && (
                      <span className="text-muted-foreground ml-2">{job.manifest_base_model}</span>
                    )}
                    {job.manifest_per_layer_hashes !== undefined && (
                      <span className="text-muted-foreground ml-2">
                        {job.manifest_per_layer_hashes ? 'per-layer hashes' : 'no per-layer hashes'}
                      </span>
                    )}
                    {job.signature_status && (
                      <span className="text-muted-foreground ml-2">{job.signature_status}</span>
                    )}
                  </dd>
                </div>
              </dl>
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="logs" className="mt-4">
          <Card>
            <CardHeader className="flex flex-row items-center justify-between">
              <CardTitle>Training Logs</CardTitle>
              <Button
                variant="outline"
                size="sm"
                onClick={() => refetchLogs()}
              >
                <RefreshCw className="h-4 w-4 mr-2" />
                Refresh
              </Button>
            </CardHeader>
            <CardContent>
              <ScrollArea className="h-[400px] rounded-lg border bg-muted/50 p-4">
                {logs.length === 0 ? (
                  <div className="text-center text-muted-foreground py-8">
                    No logs available
                  </div>
                ) : (
                  <pre className="text-xs font-mono whitespace-pre-wrap">
                    {logs.join('\n')}
                  </pre>
                )}
              </ScrollArea>
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="artifacts" className="mt-4">
          <Card>
            <CardHeader>
              <CardTitle>Training Artifacts</CardTitle>
              <CardDescription>
                Generated files from the training process
              </CardDescription>
            </CardHeader>
            <CardContent>
              {artifacts.length === 0 ? (
                <div className="text-center text-muted-foreground py-8">
                  No artifacts available yet
                </div>
              ) : (
                <div className="space-y-2">
                  {artifacts.map((artifact) => {
                    const isDownloading = downloadingArtifacts.has(artifact.id);
                    return (
                      <div
                        key={artifact.id}
                        className="flex items-center justify-between p-3 rounded-lg border"
                      >
                        <div className="flex items-center gap-3">
                          <Package className="h-4 w-4 text-muted-foreground" />
                          <div>
                            <div className="font-medium">{artifact.type}</div>
                            <div className="text-sm text-muted-foreground">
                              {(artifact.size_bytes / 1024 / 1024).toFixed(2)} MB
                            </div>
                          </div>
                        </div>
                        <Button
                          variant="outline"
                          size="sm"
                          onClick={() => handleDownloadArtifact(artifact)}
                          disabled={isDownloading}
                        >
                          <Download className="h-4 w-4 mr-2" />
                          {isDownloading ? 'Downloading…' : 'Download'}
                        </Button>
                      </div>
                    );
                  })}
                </div>
              )}
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="config" className="mt-4">
          <Card>
            <CardHeader>
              <CardTitle>Training Configuration</CardTitle>
            </CardHeader>
            <CardContent>
              {job.config ? (
                <dl className="grid grid-cols-2 md:grid-cols-3 gap-4">
                  <div>
                    <dt className="text-sm text-muted-foreground">Learning Rate</dt>
                    <dd className="font-mono">{job.config.learning_rate}</dd>
                  </div>
                  <div>
                    <dt className="text-sm text-muted-foreground">Epochs</dt>
                    <dd className="font-mono">{job.config.epochs}</dd>
                  </div>
                  <div>
                    <dt className="text-sm text-muted-foreground">Batch Size</dt>
                    <dd className="font-mono">{job.config.batch_size}</dd>
                  </div>
                  <div>
                    <dt className="text-sm text-muted-foreground">LoRA Rank</dt>
                    <dd className="font-mono">{job.config.rank}</dd>
                  </div>
                  <div>
                    <dt className="text-sm text-muted-foreground">LoRA Alpha</dt>
                    <dd className="font-mono">{job.config.alpha}</dd>
                  </div>
                  <div>
                    <dt className="text-sm text-muted-foreground">Warmup Steps</dt>
                    <dd className="font-mono">{job.config.warmup_steps ?? '-'}</dd>
                  </div>
                  <div>
                    <dt className="text-sm text-muted-foreground">Weight Decay</dt>
                    <dd className="font-mono">{job.config.weight_decay ?? '-'}</dd>
                  </div>
                  <div>
                    <dt className="text-sm text-muted-foreground">Gradient Clip</dt>
                    <dd className="font-mono">{String(job.config?.gradient_clip ?? '-')}</dd>
                  </div>
                  <div>
                    <dt className="text-sm text-muted-foreground">Max Seq Length</dt>
                    <dd className="font-mono">{job.config.max_seq_length ?? '-'}</dd>
                  </div>
                </dl>
              ) : (
                <div className="text-center text-muted-foreground py-8">
                  No configuration available
                </div>
              )}
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>

      {/* Stack Form Modal */}
      {job.status === 'completed' && job.adapter_id && (
        <StackFormModal
          open={stackModalOpen}
          onOpenChange={setStackModalOpen}
          initialAdapterId={job.adapter_id}
          onStackCreated={handleStackCreated}
        />
      )}

      {/* Publish Adapter Dialog */}
      {job.status === 'completed' && job.repo_id && job.produced_version_id && (
        <PublishAdapterDialog
          open={publishDialogOpen}
          onOpenChange={setPublishDialogOpen}
          trainingJob={job}
          onPublished={() => {
            // Refetch job to update published_at status
            refetchJob();
          }}
        />
      )}
    </div>
  );
}

export default function TrainingJobDetail() {
  return (
    <DensityProvider pageKey="training-job-detail">
      <FeatureLayout title="Training Job Details" description="View training job details">
        <TrainingJobDetailContent />
      </FeatureLayout>
    </DensityProvider>
  );
}
