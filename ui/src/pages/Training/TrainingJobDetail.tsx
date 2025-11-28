import React, { useState, useEffect, useCallback } from 'react';
import { useParams, Link, useNavigate } from 'react-router-dom';
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
} from 'lucide-react';
import apiClient from '@/api/client';
import { useSSE } from '@/hooks/useSSE';
import { logger } from '@/utils/logger';
import { useToast } from '@/hooks/use-toast';
import { StackFormModal } from '@/pages/Admin/StackFormModal';
import type {
  TrainingJob,
  TrainingMetrics,
  TrainingArtifact,
  TrainingConfig,
} from '@/api/training-types';
import type { TrainingProgressEvent } from '@/api/streaming-types';

function TrainingJobDetailContent() {
  const { jobId } = useParams<{ jobId: string }>();
  const navigate = useNavigate();
  const { toast } = useToast();

  const [job, setJob] = useState<TrainingJob | null>(null);
  const [logs, setLogs] = useState<string[]>([]);
  const [metrics, setMetrics] = useState<TrainingMetrics | null>(null);
  const [artifacts, setArtifacts] = useState<TrainingArtifact[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState('overview');
  const [stackModalOpen, setStackModalOpen] = useState(false);

  // SSE for real-time updates when job is active
  const isJobActive = job?.status === 'running' || job?.status === 'pending';

  const { data: streamData } = useSSE<TrainingProgressEvent>(
    '/v1/streams/training',
    {
      enabled: isJobActive && !!jobId,
      onMessage: (event) => {
        if (event.job_id === jobId) {
          setJob(prev => prev ? {
            ...prev,
            status: event.status,
            progress_pct: event.progress_pct,
            current_epoch: event.current_epoch,
            total_epochs: event.total_epochs,
            current_loss: event.current_loss,
            tokens_per_second: event.tokens_per_second,
            eta_seconds: event.estimated_time_remaining_sec,
            error_message: event.error,
          } : prev);

          if (event.current_loss !== undefined) {
            setMetrics(prev => ({
              ...prev,
              loss: event.current_loss,
              learning_rate: event.learning_rate,
              epoch: event.current_epoch,
              progress_pct: event.progress_pct,
              tokens_per_second: event.tokens_per_second,
            }));
          }
        }
      },
    }
  );

  const fetchJobDetails = useCallback(async () => {
    if (!jobId) return;

    setIsLoading(true);
    setError(null);

    try {
      const [jobData, logsData, metricsData, artifactsData] = await Promise.allSettled([
        apiClient.getTrainingJob(jobId),
        apiClient.getTrainingLogs(jobId),
        apiClient.getTrainingMetrics(jobId),
        apiClient.getTrainingArtifacts(jobId),
      ]);

      if (jobData.status === 'fulfilled') {
        setJob(jobData.value);
      } else {
        throw new Error('Failed to fetch job details');
      }

      if (logsData.status === 'fulfilled') {
        setLogs(logsData.value);
      }

      if (metricsData.status === 'fulfilled') {
        setMetrics(metricsData.value);
      }

      if (artifactsData.status === 'fulfilled') {
        setArtifacts(artifactsData.value.artifacts || []);
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to load job details';
      setError(message);
      logger.error('Failed to fetch training job details', { jobId }, err as Error);
    } finally {
      setIsLoading(false);
    }
  }, [jobId]);

  useEffect(() => {
    fetchJobDetails();
  }, [fetchJobDetails]);

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

  // Auto-refresh logs for active jobs
  useEffect(() => {
    if (!isJobActive || !jobId) return;

    const interval = setInterval(async () => {
      try {
        const newLogs = await apiClient.getTrainingLogs(jobId);
        setLogs(newLogs);
      } catch (err) {
        logger.warn('Failed to refresh logs', { jobId });
      }
    }, 5000);

    return () => clearInterval(interval);
  }, [isJobActive, jobId]);

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <RefreshCw className="h-8 w-8 animate-spin text-muted-foreground" />
      </div>
    );
  }

  if (error || !job) {
    return (
      <Card className="border-destructive">
        <CardContent className="pt-6">
          <div className="flex items-center gap-2 text-destructive mb-4">
            <AlertTriangle className="h-5 w-5" />
            <span>{error || 'Job not found'}</span>
          </div>
          <div className="flex gap-2">
            <Button variant="outline" onClick={() => navigate('/training')}>
              <ArrowLeft className="h-4 w-4 mr-2" />
              Back to Jobs
            </Button>
            <Button variant="outline" onClick={fetchJobDetails}>
              <RefreshCw className="h-4 w-4 mr-2" />
              Retry
            </Button>
          </div>
        </CardContent>
      </Card>
    );
  }

  const statusConfig = {
    pending: { icon: Clock, className: 'text-yellow-500', label: 'Pending' },
    running: { icon: Activity, className: 'text-blue-500 animate-pulse', label: 'Running' },
    completed: { icon: CheckCircle, className: 'text-green-500', label: 'Completed' },
    failed: { icon: XCircle, className: 'text-red-500', label: 'Failed' },
    cancelled: { icon: Square, className: 'text-gray-500', label: 'Cancelled' },
    paused: { icon: Pause, className: 'text-orange-500', label: 'Paused' },
  };

  const StatusIcon = statusConfig[job.status]?.icon || Clock;
  const statusClass = statusConfig[job.status]?.className || 'text-gray-500';

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-4">
          <Button variant="ghost" size="sm" onClick={() => navigate('/training')}>
            <ArrowLeft className="h-4 w-4 mr-2" />
            Back
          </Button>
          <div>
            <p className="text-sm text-muted-foreground">Job ID: {job.id}</p>
          </div>
        </div>
        <div className="flex items-center gap-2">
          {job.status === 'completed' && job.adapter_id && (
            <>
              <Button
                onClick={() => navigate(`/adapters/${job.adapter_id}`)}
                variant="default"
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
                <div className="font-medium">Error</div>
                <div className="text-sm">{job.error_message}</div>
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
          <Card>
            <CardHeader>
              <CardTitle>Job Information</CardTitle>
            </CardHeader>
            <CardContent>
              <dl className="grid grid-cols-2 gap-4">
                <div>
                  <dt className="text-sm text-muted-foreground">Dataset ID</dt>
                  <dd className="flex items-center gap-2">
                    <span className="font-mono text-sm">{job.dataset_id || '-'}</span>
                    {job.dataset_id && (
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => navigate(`/training/datasets/${job.dataset_id}`)}
                      >
                        <ExternalLink className="h-3 w-3" />
                      </Button>
                    )}
                  </dd>
                </div>
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
                <div>
                  <dt className="text-sm text-muted-foreground">Template ID</dt>
                  <dd className="font-mono text-sm">{job.template_id || '-'}</dd>
                </div>
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
                onClick={async () => {
                  const newLogs = await apiClient.getTrainingLogs(jobId!);
                  setLogs(newLogs);
                }}
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
                  {artifacts.map((artifact) => (
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
                      <Button variant="outline" size="sm">
                        <Download className="h-4 w-4 mr-2" />
                        Download
                      </Button>
                    </div>
                  ))}
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
                    <dd className="font-mono">{job.config.gradient_clip ?? '-'}</dd>
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
