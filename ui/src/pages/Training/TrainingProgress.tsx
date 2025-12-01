import { useState, useCallback, useEffect, useMemo } from 'react';
import { DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { useTraining } from '@/hooks/useTraining';
import { useTrainingStream } from '@/hooks/useStreamingEndpoints';
import { TrainingProgressCard } from './TrainingProgressCard';
import {
  Activity,
  CheckCircle,
  XCircle,
  Clock,
  FileText,
  BarChart3,
  Package,
  X,
  RefreshCw,
  Download,
  Loader2,
  Radio,
} from 'lucide-react';
import { toast } from 'sonner';
import apiClient from '@/api/client';
import type { TrainingJob, TrainingStatus, TrainingArtifact } from '@/api/training-types';
import { isTrainingProgressEvent, TrainingProgressEvent } from '@/api/streaming-types';
import { formatTimestamp } from '@/utils/format';

interface TrainingProgressProps {
  jobId: string;
  onClose: () => void;
}

const STATUS_CONFIG: Record<TrainingStatus, {
  icon: React.ElementType;
  className: string;
  variant: 'default' | 'secondary' | 'destructive' | 'outline';
}> = {
  pending: {
    icon: Clock,
    className: 'text-yellow-500',
    variant: 'secondary',
  },
  running: {
    icon: Activity,
    className: 'text-blue-500 animate-pulse',
    variant: 'default',
  },
  completed: {
    icon: CheckCircle,
    className: 'text-green-500',
    variant: 'outline',
  },
  failed: {
    icon: XCircle,
    className: 'text-red-500',
    variant: 'destructive',
  },
  cancelled: {
    icon: XCircle,
    className: 'text-gray-500',
    variant: 'outline',
  },
  paused: {
    icon: Clock,
    className: 'text-orange-500',
    variant: 'secondary',
  },
};

export function TrainingProgress({ jobId, onClose }: TrainingProgressProps) {
  const [activeTab, setActiveTab] = useState('progress');
  const [downloadingArtifacts, setDownloadingArtifacts] = useState<Set<string>>(new Set());

  // Streaming state for real-time updates
  const [streamingJob, setStreamingJob] = useState<Partial<TrainingJob> | null>(null);

  const {
    data: job,
    isLoading: isJobLoading,
    error: jobError,
    refetch: refetchJob,
  } = useTraining.useTrainingJob(jobId, {
    refetchInterval: (query) => {
      // Reduce polling frequency when streaming is active
      const data = query.state.data;
      const isActiveJob = data?.status === 'running' || data?.status === 'pending';
      if (isActiveJob) {
        // Use longer polling interval (10s) when streaming is connected
        // Fall back to faster polling (2s) if streaming is not available
        return streamConnected ? 10000 : 2000;
      }
      return false;
    },
  });

  // Determine if job is active and should use streaming
  const isActiveJob = job?.status === 'running' || job?.status === 'pending';

  // Memoize onMessage callback to prevent unnecessary reconnections
  const handleStreamMessage = useCallback((event: TrainingProgressEvent | unknown) => {
    if (isTrainingProgressEvent(event) && event.job_id === jobId) {
      // Update local state with streaming data
      setStreamingJob({
        status: event.status,
        progress_pct: event.progress_pct,
        current_epoch: event.current_epoch,
        total_epochs: event.total_epochs,
        current_loss: event.current_loss,
        learning_rate: event.learning_rate,
        tokens_per_second: event.tokens_per_second,
        eta_seconds: event.estimated_time_remaining_sec,
      });
    }
  }, [jobId]);

  // Connect to training stream for real-time updates
  const {
    connected: streamConnected,
    error: streamError,
    lastUpdated: streamLastUpdated,
  } = useTrainingStream({
    enabled: isActiveJob,
    onMessage: handleStreamMessage,
  });

  // Merge streaming data with polled data
  const effectiveJob = useMemo(() => {
    if (!job) return null;
    if (!streamingJob) return job;
    // Streaming data takes precedence for real-time fields
    return {
      ...job,
      ...streamingJob,
    };
  }, [job, streamingJob]);

  // Clear streaming state when job completes
  useEffect(() => {
    if (job?.status === 'completed' || job?.status === 'failed' || job?.status === 'cancelled') {
      setStreamingJob(null);
    }
  }, [job?.status]);

  const {
    data: logs,
    isLoading: isLogsLoading,
    refetch: refetchLogs,
  } = useTraining.useJobLogs(jobId, {
    enabled: activeTab === 'logs',
    refetchInterval: () => {
      // Poll logs for active jobs
      if (job?.status === 'running' || job?.status === 'pending') {
        return 2000;
      }
      return false;
    },
  });

  const {
    data: metrics,
    isLoading: isMetricsLoading,
    refetch: refetchMetrics,
  } = useTraining.useJobMetrics(jobId, {
    enabled: activeTab === 'metrics',
    refetchInterval: () => {
      // Poll metrics for active jobs
      if (job?.status === 'running' || job?.status === 'pending') {
        return 2000;
      }
      return false;
    },
  });

  const {
    data: artifactsData,
    isLoading: isArtifactsLoading,
    refetch: refetchArtifacts,
  } = useTraining.useJobArtifacts(jobId, {
    enabled: activeTab === 'artifacts',
  });

  const statusConfig = effectiveJob ? STATUS_CONFIG[effectiveJob.status] : STATUS_CONFIG.pending;
  const StatusIcon = statusConfig.icon;

  const handleDownloadArtifact = useCallback(async (artifact: TrainingArtifact) => {
    setDownloadingArtifacts((prev) => new Set(prev).add(artifact.id));

    try {
      // Extract filename from path or use artifact type + id
      const filename = artifact.path.split('/').pop() || `${artifact.type}-${artifact.id}`;
      await apiClient.downloadArtifact(jobId, artifact.id, filename);
      toast.success(`Downloaded ${filename}`);
    } catch (error) {
      const message = error instanceof Error ? error.message : 'Failed to download artifact';
      toast.error(message);
    } finally {
      setDownloadingArtifacts((prev) => {
        const next = new Set(prev);
        next.delete(artifact.id);
        return next;
      });
    }
  }, [jobId]);

  if (isJobLoading) {
    return (
      <>
        <DialogHeader>
          <DialogTitle>Loading Job Details...</DialogTitle>
        </DialogHeader>
        <div className="flex items-center justify-center py-8">
          <RefreshCw className="h-6 w-6 animate-spin text-muted-foreground" />
        </div>
      </>
    );
  }

  if (jobError || !effectiveJob) {
    return (
      <>
        <DialogHeader>
          <DialogTitle>Error Loading Job</DialogTitle>
        </DialogHeader>
        <Alert variant="destructive">
          <XCircle className="h-4 w-4" />
          <AlertDescription>
            {jobError?.message || 'Failed to load job details'}
          </AlertDescription>
        </Alert>
        <div className="flex justify-end gap-2 pt-4">
          <Button variant="outline" onClick={() => refetchJob()}>
            Retry
          </Button>
          <Button onClick={onClose}>Close</Button>
        </div>
      </>
    );
  }

  return (
    <>
      <DialogHeader>
        <DialogTitle className="flex items-center justify-between">
          <span className="flex items-center gap-2">
            <StatusIcon className={`h-5 w-5 ${statusConfig.className}`} />
            {effectiveJob.adapter_name || effectiveJob.id}
          </span>
          <div className="flex items-center gap-2">
            {/* Streaming indicator */}
            {isActiveJob && (
              <Badge variant={streamConnected ? 'default' : 'outline'} className="flex items-center gap-1">
                <Radio className={`h-3 w-3 ${streamConnected ? 'text-green-400 animate-pulse' : 'text-muted-foreground'}`} />
                {streamConnected ? 'Live' : 'Polling'}
              </Badge>
            )}
            <Button variant="ghost" size="sm" onClick={onClose}>
              <X className="h-4 w-4" />
            </Button>
          </div>
        </DialogTitle>
      </DialogHeader>

      <div className="space-y-4">
        {/* Job Overview */}
        <Card>
          <CardHeader>
            <CardTitle className="text-sm flex items-center justify-between">
              <span className="flex items-center gap-2">
                Job Information
                {streamLastUpdated && streamConnected && (
                  <span className="text-xs text-muted-foreground font-normal">
                    Updated: {new Date(streamLastUpdated).toLocaleTimeString()}
                  </span>
                )}
              </span>
              <Badge variant={statusConfig.variant}>{effectiveJob.status}</Badge>
            </CardTitle>
          </CardHeader>
          <CardContent className="grid grid-cols-2 gap-4 text-sm">
            <div>
              <span className="text-muted-foreground">Job ID</span>
              <p className="font-mono">{effectiveJob.id.slice(0, 16)}...</p>
            </div>
            {effectiveJob.dataset_id && (
              <div>
                <span className="text-muted-foreground">Dataset</span>
                <p className="font-mono">{effectiveJob.dataset_id.slice(0, 16)}...</p>
              </div>
            )}
            {effectiveJob.created_at && (
              <div>
                <span className="text-muted-foreground">Created</span>
                <p>{formatTimestamp(effectiveJob.created_at, 'long')}</p>
              </div>
            )}
            {effectiveJob.started_at && (
              <div>
                <span className="text-muted-foreground">Started</span>
                <p>{formatTimestamp(effectiveJob.started_at, 'long')}</p>
              </div>
            )}
            {effectiveJob.completed_at && (
              <div>
                <span className="text-muted-foreground">Completed</span>
                <p>{formatTimestamp(effectiveJob.completed_at, 'long')}</p>
              </div>
            )}
          </CardContent>
        </Card>

        {/* Tabs */}
        <Tabs value={activeTab} onValueChange={setActiveTab}>
          <TabsList className="grid w-full grid-cols-4">
            <TabsTrigger value="progress">Progress</TabsTrigger>
            <TabsTrigger value="logs">Logs</TabsTrigger>
            <TabsTrigger value="metrics">Metrics</TabsTrigger>
            <TabsTrigger value="artifacts">Artifacts</TabsTrigger>
          </TabsList>

          <TabsContent value="progress" className="mt-4">
            <TrainingProgressCard jobId={jobId} initialJob={effectiveJob} />
          </TabsContent>

          <TabsContent value="logs" className="mt-4">
            <Card>
              <CardHeader>
                <CardTitle className="text-sm flex items-center justify-between">
                  <span className="flex items-center gap-2">
                    <FileText className="h-4 w-4" />
                    Training Logs
                  </span>
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => refetchLogs()}
                    disabled={isLogsLoading}
                  >
                    <RefreshCw className={`h-4 w-4 ${isLogsLoading ? 'animate-spin' : ''}`} />
                  </Button>
                </CardTitle>
              </CardHeader>
              <CardContent>
                {isLogsLoading ? (
                  <div className="text-center py-4 text-muted-foreground">
                    Loading logs...
                  </div>
                ) : !logs || logs.length === 0 ? (
                  <div className="text-center py-4 text-muted-foreground">
                    No logs available yet
                  </div>
                ) : (
                  <ScrollArea className="h-[400px]">
                    <pre className="text-xs font-mono whitespace-pre-wrap">
                      {logs.join('\n')}
                    </pre>
                  </ScrollArea>
                )}
              </CardContent>
            </Card>
          </TabsContent>

          <TabsContent value="metrics" className="mt-4">
            <Card>
              <CardHeader>
                <CardTitle className="text-sm flex items-center justify-between">
                  <span className="flex items-center gap-2">
                    <BarChart3 className="h-4 w-4" />
                    Training Metrics
                  </span>
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => refetchMetrics()}
                    disabled={isMetricsLoading}
                  >
                    <RefreshCw className={`h-4 w-4 ${isMetricsLoading ? 'animate-spin' : ''}`} />
                  </Button>
                </CardTitle>
              </CardHeader>
              <CardContent>
                {isMetricsLoading ? (
                  <div className="text-center py-4 text-muted-foreground">
                    Loading metrics...
                  </div>
                ) : !metrics ? (
                  <div className="text-center py-4 text-muted-foreground">
                    No metrics available yet
                  </div>
                ) : (
                  <div className="space-y-3">
                    {metrics.loss !== undefined && (
                      <div className="flex justify-between">
                        <span className="text-muted-foreground">Loss</span>
                        <span className="font-mono">{metrics.loss.toFixed(6)}</span>
                      </div>
                    )}
                    {metrics.learning_rate !== undefined && (
                      <div className="flex justify-between">
                        <span className="text-muted-foreground">Learning Rate</span>
                        <span className="font-mono">{metrics.learning_rate.toExponential(2)}</span>
                      </div>
                    )}
                    {metrics.epoch !== undefined && (
                      <div className="flex justify-between">
                        <span className="text-muted-foreground">Epoch</span>
                        <span className="font-mono">{metrics.epoch}</span>
                      </div>
                    )}
                    {metrics.tokens_per_second !== undefined && (
                      <div className="flex justify-between">
                        <span className="text-muted-foreground">Tokens/sec</span>
                        <span className="font-mono">{metrics.tokens_per_second.toFixed(0)}</span>
                      </div>
                    )}
                  </div>
                )}
              </CardContent>
            </Card>
          </TabsContent>

          <TabsContent value="artifacts" className="mt-4">
            <Card>
              <CardHeader>
                <CardTitle className="text-sm flex items-center justify-between">
                  <span className="flex items-center gap-2">
                    <Package className="h-4 w-4" />
                    Artifacts
                  </span>
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => refetchArtifacts()}
                    disabled={isArtifactsLoading}
                  >
                    <RefreshCw className={`h-4 w-4 ${isArtifactsLoading ? 'animate-spin' : ''}`} />
                  </Button>
                </CardTitle>
              </CardHeader>
              <CardContent>
                {isArtifactsLoading ? (
                  <div className="text-center py-4 text-muted-foreground">
                    Loading artifacts...
                  </div>
                ) : !artifactsData?.artifacts || artifactsData.artifacts.length === 0 ? (
                  <div className="text-center py-4 text-muted-foreground">
                    No artifacts available yet
                  </div>
                ) : (
                  <div className="space-y-2">
                    {artifactsData.artifacts.map((artifact) => {
                      const isDownloading = downloadingArtifacts.has(artifact.id);
                      return (
                        <div
                          key={artifact.id}
                          className="flex items-center justify-between p-3 border rounded hover:bg-muted/50 transition-colors"
                        >
                          <div className="flex items-center gap-3 flex-1 min-w-0">
                            <Package className="h-4 w-4 text-muted-foreground shrink-0" />
                            <div className="min-w-0">
                              <p className="text-sm font-medium capitalize">{artifact.type}</p>
                              <p className="text-xs text-muted-foreground truncate" title={artifact.path}>
                                {artifact.path}
                              </p>
                            </div>
                          </div>
                          <div className="flex items-center gap-2 shrink-0">
                            <Badge variant="outline">
                              {(artifact.size_bytes / 1024 / 1024).toFixed(2)} MB
                            </Badge>
                            <Button
                              size="sm"
                              variant="outline"
                              onClick={() => handleDownloadArtifact(artifact)}
                              disabled={isDownloading}
                              title={`Download ${artifact.type}`}
                              aria-label={`Download ${artifact.type} artifact`}
                            >
                              {isDownloading ? (
                                <Loader2 className="h-4 w-4 animate-spin" />
                              ) : (
                                <Download className="h-4 w-4" />
                              )}
                            </Button>
                          </div>
                        </div>
                      );
                    })}
                  </div>
                )}
              </CardContent>
            </Card>
          </TabsContent>
        </Tabs>

        <div className="flex justify-end pt-4">
          <Button onClick={onClose}>Close</Button>
        </div>
      </div>
    </>
  );
}
