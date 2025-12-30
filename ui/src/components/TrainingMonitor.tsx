// 【ui/src/components/TrainingMonitor.tsx§45-88】 - Replace manual polling with standardized hook
import React, { useState, useEffect, useRef, useCallback } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Progress } from './ui/progress';
import { Alert, AlertDescription } from './ui/alert';
import { ScrollArea } from './ui/scroll-area';
import {
  Activity,
  Square,
  Zap,
  Target,
  Cpu,
  MemoryStick,
  Clock,
  CheckCircle,
  XCircle,
  AlertTriangle,
  TrendingUp,
  TrendingDown,
  BarChart3
} from 'lucide-react';
import { apiClient } from '@/api/services';

import { TrainingJob, TrainingSession, TrainingMetrics, TrainingMetricsListResponse, TrainingArtifactsResponse } from '@/api/types';
import { logger, toError } from '@/utils/logger';
import { toast } from 'sonner';
import { usePolling } from '@/hooks/realtime/usePolling';
import { useSSE } from '@/hooks/realtime/useSSE';
import { LastUpdated } from './ui/last-updated';
import { ErrorRecovery, errorRecoveryTemplates } from './ui/error-recovery';
import { SectionErrorBoundary } from './ui/section-error-boundary';
import { Link } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import { calculateTrainingETA, formatDuration as formatDurationUtil } from '@/utils/trainingEta';
import { formatDurationSeconds, formatRelativeTime } from '@/lib/formatters';
import { buildInferenceLink } from '@/utils/navLinks';

/**
 * Transforms backend TrainingMetricsListResponse to UI-friendly TrainingMetrics.
 * Extracts the latest metric entry from the time-series array.
 */
const transformMetricsResponse = (response: TrainingMetricsListResponse): TrainingMetrics => {
  const entries = response.metrics;
  if (!entries || entries.length === 0) {
    return {};
  }
  // Get the latest metric entry (last in the array, highest step)
  const latest = entries[entries.length - 1];
  return {
    step: latest.step,
    loss: latest.loss,
    learning_rate: latest.learning_rate,
    epoch: latest.epoch,
    tokens_processed: latest.tokens_processed,
  };
};

interface TrainingMonitorProps {
  sessionId?: string;
  jobId?: string;
  onClose?: () => void;
}

export function TrainingMonitor({ sessionId, jobId, onClose }: TrainingMonitorProps) {
  // Validate props: at least one ID must be provided
  if (!sessionId && !jobId) {
    return (
      <div className="text-center p-8 text-red-600">
        <p>Error: TrainingMonitor requires either sessionId or jobId</p>
        {onClose && (
          <button onClick={onClose} className="mt-4 text-sm underline">
            Close
          </button>
        )}
      </div>
    );
  }

  const isSessionMode = !!sessionId;
  const effectiveId = sessionId || jobId || '';
  
  const [session, setSession] = useState<TrainingSession | null>(null);
  const [job, setJob] = useState<TrainingJob | null>(null);
  const [metrics, setMetrics] = useState<TrainingMetrics | null>(null);
  const [logs, setLogs] = useState<string[]>([]);
  const [artifacts, setArtifacts] = useState<TrainingArtifactsResponse | null>(null);
  const [error, setError] = useState<Error | null>(null);
  const [stopError, setStopError] = useState<Error | null>(null);
  const [isPolling, setIsPolling] = useState(true);
  const logScrollRef = useRef<HTMLDivElement>(null);

  // Track component mount state to prevent state updates after unmount
  const isMountedRef = useRef(true);

  useEffect(() => {
    isMountedRef.current = true;
    return () => {
      isMountedRef.current = false;
    };
  }, []);

  // Track SSE connection state for polling coordination
  const sseConnectedRef = useRef(false);
  const lastSseUpdateRef = useRef<number>(0);

  // Memoize SSE onMessage callback to prevent reconnections
  const handleSSEMessage = useCallback((data: {
    job_id?: string;
    progress?: number;
    status?: string;
    metrics?: TrainingMetrics;
    logs?: string[];
  }) => {
    lastSseUpdateRef.current = Date.now();

    // Update metrics in real-time from SSE
    if (data.metrics) {
      setMetrics(data.metrics);
    }
    if (data.logs && data.logs.length > 0) {
      // Limit logs to last 1000 entries to prevent unbounded growth
      setLogs(prev => {
        const newLogs = [...prev, ...(data.logs ?? [])];
        return newLogs.length > 1000 ? newLogs.slice(-1000) : newLogs;
      });
    }
    if (data.status) {
      setJob(prev => prev ? { ...prev, status: data.status as TrainingJob['status'] } : null);
    }
  }, []);

  // SSE connection for real-time training updates
  const {
    connected: sseConnected,
    error: sseError,
    reconnect: sseReconnect
  } = useSSE<{
    job_id?: string;
    progress?: number;
    status?: string;
    metrics?: TrainingMetrics;
    logs?: string[];
  }>(`/v1/stream/training/${effectiveId}`, {
    enabled: !!effectiveId && isPolling,
    onMessage: handleSSEMessage
  });

  // Update SSE connection ref
  useEffect(() => {
    sseConnectedRef.current = sseConnected;
  }, [sseConnected]);

  // 【ui/src/hooks/usePolling.ts】 - Standardized polling hook for training monitor
  const fetchTrainingData = async () => {
    if (isSessionMode && sessionId) {
      // Session mode: fetch session data
      const sessionData = await apiClient.getTrainingSession(sessionId);
      return {
        session: sessionData,
        job: null,
        metrics: null,
        logs: [],
        artifacts: null
      };
    } else if (jobId) {
      // Job mode: fetch job data with metrics/logs
      const [jobData, metricsResponse, logsData] = await Promise.all([
        apiClient.getTrainingJob(jobId),
        apiClient.getTrainingMetrics(jobId),
        apiClient.getTrainingLogs(jobId)
      ]);

      // Fetch artifacts separately; ignore errors so UI still updates
      let artifactsData: TrainingArtifactsResponse | null = null;
      try {
        artifactsData = await apiClient.getTrainingArtifacts(jobId);
      } catch (e) {
        // Not all jobs produce artifacts; keep null
      }


      return {
        session: null,
        job: jobData,
        metrics: transformMetricsResponse(metricsResponse),
        logs: logsData,
        artifacts: artifactsData
      };
    }

    throw new Error('Either sessionId or jobId must be provided');
  };

  const {
    data: trainingData,
    isLoading: loading,
    lastUpdated,
    error: pollingError,
    refetch: refreshTraining
  } = usePolling(
    fetchTrainingData,
    'normal', // Background updates for training monitoring
    {
      showLoadingIndicator: false,
      enabled: isPolling && !!effectiveId,
      onError: (err) => {
        const error = err instanceof Error ? err : new Error('Failed to fetch training data');
        setError(error);
        logger.error('Failed to fetch training data', {
          component: 'TrainingMonitor',
          operation: 'polling',
          sessionId,
          jobId
        }, err);
      }
    }
  );

  // Update state when polling data arrives
  // Only update if SSE is not connected or data is newer than last SSE update
  useEffect(() => {
    if (!trainingData) return;

    // If SSE is connected and we received SSE data recently (within 5s), skip polling updates
    // to avoid race conditions. Exception: always update session/job/artifacts from polling.
    const sseRecentlyUpdated = sseConnectedRef.current &&
      (Date.now() - lastSseUpdateRef.current) < 5000;

    if (trainingData.session) {
      setSession(trainingData.session);
      setJob(null);
    } else {
      setSession(null);
      setJob(trainingData.job);
    }

    // Only update metrics/logs from polling if SSE hasn't provided recent updates
    if (!sseRecentlyUpdated) {
      setMetrics(trainingData.metrics);
      // Limit logs from polling as well
      const limitedLogs = trainingData.logs.length > 1000
        ? trainingData.logs.slice(-1000)
        : trainingData.logs;
      setLogs(limitedLogs);
    }

    setArtifacts(trainingData.artifacts);
    setError(null);

    // Auto-scroll logs to bottom
    if (logScrollRef.current) {
      logScrollRef.current.scrollTop = logScrollRef.current.scrollHeight;
    }
  }, [trainingData]);

  // Adapter registration polling: if job completed but adapter_id missing, poll adapters list
  const shouldPollAdapters = job?.status === 'completed' && !job?.adapter_id;
  const { data: adapters = [] } = useQuery({
    queryKey: ['adapters', 'registration-check'],
    queryFn: async () => {
      try {
        return await apiClient.listAdapters();
      } catch (e) {
        logger.error('Failed to fetch adapters for registration check', {
          component: 'TrainingMonitor',
          operation: 'adapter-polling',
        }, toError(e));
        return [];
      }
    },
    enabled: shouldPollAdapters,
    refetchInterval: shouldPollAdapters ? 3000 : false, // Poll every 3 seconds if waiting for adapter
    staleTime: 0, // Always fetch fresh data when polling
  });

  // Check if adapter appeared in the list
  useEffect(() => {
    if (!shouldPollAdapters || !job || !adapters || adapters.length === 0) return;

    // Try to find adapter by name (adapter_name might match registered adapter name)
    const foundAdapter = adapters.find(a =>
      a.name === job.adapter_name ||
      a.id === job.adapter_id ||
      (job.adapter_name && a.name?.includes(job.adapter_name))
    );

    if (foundAdapter && !job.adapter_id) {
      // Update job state with found adapter_id
      setJob(prev => prev ? { ...prev, adapter_id: foundAdapter.id } : null);
      toast.success(`Adapter "${foundAdapter.name}" registered successfully`);
      logger.info('Adapter registration detected', {
        component: 'TrainingMonitor',
        operation: 'adapter-polling',
        adapterId: foundAdapter.id,
        adapterName: foundAdapter.name,
      });
    }
  }, [adapters, shouldPollAdapters, job]);

  const handleStop = async () => {
    if (!jobId) {
      toast.error('Stop is only available for training jobs');
      return;
    }

    setStopError(null);
    try {
      logger.info('Cancelling training job', {
        component: 'TrainingMonitor',
        operation: 'handleStop',
        jobId
      });

      await apiClient.cancelTraining(jobId);

      // Only update state if component is still mounted
      if (isMountedRef.current) {
        setIsPolling(false);
        toast.success('Training job cancelled successfully');

        logger.info('Training job cancelled', {
          component: 'TrainingMonitor',
          operation: 'handleStop',
          jobId
        });
      }
    } catch (err) {
      // Only update state if component is still mounted
      if (isMountedRef.current) {
        const error = err instanceof Error ? err : new Error('Failed to cancel training');
        logger.error('Failed to cancel training', {
          component: 'TrainingMonitor',
          operation: 'handleStop',
          jobId,
          error: error.message
        });
        setStopError(error);
        toast.error(`Failed to cancel training: ${error.message}`);
      }
    }
  };

  const getStatusIcon = (status: string) => {
    switch (status) {
      case 'running': return <Activity className="h-4 w-4 text-blue-600 animate-pulse" />;
      case 'completed': return <CheckCircle className="h-4 w-4 text-green-600" />;
      case 'failed': return <XCircle className="h-4 w-4 text-red-600" />;
      case 'cancelled': return <AlertTriangle className="h-4 w-4 text-yellow-600" />;
      case 'queued':
      case 'pending': return <Clock className="h-4 w-4 text-gray-600" />;
      default: return <AlertTriangle className="h-4 w-4 text-gray-600" />;
    }
  };

  const getStatusBadge = (status: string) => {
    const variants = {
      running: 'bg-blue-100 text-blue-800',
      completed: 'bg-green-100 text-green-800',
      failed: 'bg-red-100 text-red-800',
      cancelled: 'bg-yellow-100 text-yellow-800',
      queued: 'bg-gray-100 text-gray-800',
      pending: 'bg-gray-100 text-gray-800'
    };
    return variants[status as keyof typeof variants] || 'bg-gray-100 text-gray-800';
  };

  const formatDuration = (startTime: string) => {
    const start = new Date(startTime);
    const now = new Date();
    const diffMs = now.getTime() - start.getTime();
    const diffSeconds = Math.floor(diffMs / 1000);
    return formatDurationSeconds(diffSeconds);
  };

  const formatETA = (startTime: string, progress: number, jobStatus?: string) => {
    if (progress === 0) return 'Calculating...';

    const etaSeconds = calculateTrainingETA(progress, startTime, undefined, jobStatus);
    if (etaSeconds === null) return 'Calculating...';

    return formatDurationUtil(etaSeconds);
  };

  if (error) {
    return errorRecoveryTemplates.genericError(
      error.message,
      () => refreshTraining()
    );
  }

  // Determine display data: prefer session if available, fall back to job
  const displayData = session || job;
  if (!displayData) {
    return <div className="text-center p-8">Loading training {isSessionMode ? 'session' : 'job'}...</div>;
  }

  const status = displayData.status;
  const adapterName = session?.adapter_name || job?.adapter_name || 'Unknown';
  const progress = session?.progress ?? job?.progress ?? job?.progress_pct ?? 0;
  const startTime = session?.created_at || job?.started_at || job?.created_at || new Date().toISOString();

  return (
    <div className="space-y-6">
      {/* Error Recovery for stop operations */}
      {stopError && errorRecoveryTemplates.genericError(
        `Failed to stop training: ${stopError.message}`,
        handleStop
      )}

      {/* SSE Connection Error */}
      {sseError && (
        <Alert variant="destructive">
          <AlertTriangle className="h-4 w-4" />
          <AlertDescription className="flex items-center justify-between">
            <span>Real-time updates unavailable: {typeof sseError === 'string' ? sseError : sseError.message}. Falling back to polling.</span>
            {(typeof sseError === 'string' ? sseError : sseError.message).includes('failed after') && (
              <Button variant="outline" size="sm" onClick={sseReconnect} className="ml-4">
                Reconnect
              </Button>
            )}
          </AlertDescription>
        </Alert>
      )}

      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center space-x-3">
          {getStatusIcon(status)}
          <div>
            <h2 className="text-lg font-semibold">{adapterName}</h2>
            <div className="flex items-center space-x-2 text-sm text-muted-foreground">
              <Badge className={getStatusBadge(status)}>
                {status}
              </Badge>
              <span>•</span>
              <span>Running for {formatDuration(startTime)}</span>
                {status === 'running' && progress > 0 && (
                <>
                  <span>•</span>
                  <span>ETA: {formatETA(startTime, progress, status)}</span>
                </>
              )}
            </div>
            {lastUpdated && <LastUpdated timestamp={lastUpdated} className="mt-1" />}
          </div>
        </div>
        
        <div className="flex space-x-2">
          {!isSessionMode && status === 'running' && (
            <Button variant="outline" size="sm" onClick={handleStop}>
              <Square className="h-4 w-4 mr-1" />
              Stop
            </Button>
          )}
          {onClose && (
            <Button variant="outline" size="sm" onClick={onClose}>
              Close
            </Button>
          )}
        </div>
      </div>

      {/* Progress */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center">
            <BarChart3 className="mr-2 h-5 w-5" />
            Training Progress
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="space-y-4">
            <div className="flex items-center justify-between">
              <span className="text-sm font-medium">Overall Progress</span>
              <span className="text-sm text-muted-foreground">{progress}%</span>
            </div>
            <Progress value={progress} className="h-2" />
            
            {metrics && job && (
              <div className="grid grid-cols-2 md:grid-cols-4 gap-4 pt-4">
                <div className="text-center">
                  <div className="text-2xl font-bold">{metrics.current_epoch}</div>
                  <div className="text-xs text-muted-foreground">Epoch</div>
                </div>
                <div className="text-center">
                  <div className="text-2xl font-bold">{metrics.total_epochs}</div>
                  <div className="text-xs text-muted-foreground">Total Epochs</div>
                </div>
                <div className="text-center">
                  <div className="text-2xl font-bold">{metrics.loss?.toFixed(4) ?? 'N/A'}</div>
                  <div className="text-xs text-muted-foreground">Loss</div>
                </div>
                <div className="text-center">
                  <div className="text-2xl font-bold">{metrics.validation_loss?.toFixed(4) ?? 'N/A'}</div>
                  <div className="text-xs text-muted-foreground">Val Loss</div>
                </div>
              </div>
            )}
          </div>
        </CardContent>
      </Card>

      {/* Resource Usage */}
      {metrics && (
        <SectionErrorBoundary sectionName="Training Metrics">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center">
                <Cpu className="mr-2 h-5 w-5" />
                Resource Usage
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                <div className="space-y-2">
                  <div className="flex items-center justify-between">
                    <div className="flex items-center space-x-2">
                      <Cpu className="h-4 w-4" />
                      <span className="text-sm font-medium">GPU Utilization</span>
                    </div>
                    <span className="text-sm text-muted-foreground">{metrics.gpu_utilization}%</span>
                  </div>
                  <Progress value={metrics.gpu_utilization} className="h-2" />
                </div>

                <div className="space-y-2">
                  <div className="flex items-center justify-between">
                    <div className="flex items-center space-x-2">
                      <MemoryStick className="h-4 w-4" />
                      <span className="text-sm font-medium">Memory Usage</span>
                    </div>
                    <span className="text-sm text-muted-foreground">{metrics.memory_usage ?? 0}GB</span>
                  </div>
                  <Progress value={((metrics.memory_usage ?? 0) / 24) * 100} className="h-2" />
                </div>

                <div className="space-y-2">
                  <div className="flex items-center justify-between">
                    <div className="flex items-center space-x-2">
                      <Zap className="h-4 w-4" />
                      <span className="text-sm font-medium">Tokens/sec</span>
                    </div>
                    <span className="text-sm text-muted-foreground">{metrics.tokens_per_second ?? 0}</span>
                  </div>
                  <div className="h-2 bg-gray-200 rounded-full">
                    <div
                      className="h-2 bg-green-500 rounded-full transition-all duration-300"
                      style={{ width: `${Math.min(((metrics.tokens_per_second ?? 0) / 2000) * 100, 100)}%` }}
                    />
                  </div>
                </div>
              </div>
            </CardContent>
          </Card>
        </SectionErrorBoundary>
      )}

      {/* Training Configuration */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center">
            <Target className="mr-2 h-5 w-5" />
            Training Configuration
          </CardTitle>
        </CardHeader>
        <CardContent>
          {job?.config ? (
            <div className="grid grid-cols-2 md:grid-cols-4 gap-4 text-sm">
              <div>
                <div className="text-muted-foreground">Rank</div>
                <div className="font-medium">{job.config.rank}</div>
              </div>
              <div>
                <div className="text-muted-foreground">Alpha</div>
                <div className="font-medium">{job.config.alpha}</div>
              </div>
              <div>
                <div className="text-muted-foreground">Epochs</div>
                <div className="font-medium">{job.config.epochs}</div>
              </div>
              <div>
                <div className="text-muted-foreground">Learning Rate</div>
                <div className="font-medium">{job.config.learning_rate}</div>
              </div>
              <div>
                <div className="text-muted-foreground">Batch Size</div>
                <div className="font-medium">{job.config.batch_size}</div>
              </div>
              <div>
                <div className="text-muted-foreground">Category</div>
                <div className="font-medium">{job.config.category ?? 'N/A'}</div>
              </div>
              <div>
                <div className="text-muted-foreground">Scope</div>
                <div className="font-medium">{job.config.scope ?? 'N/A'}</div>
              </div>
              {job.config.repo_id && (
                <div>
                  <div className="text-muted-foreground">Repository</div>
                  <div className="font-medium">{String(job.config.repo_id)}</div>
                </div>
              )}
            </div>
          ) : session ? (
            <div className="text-sm text-muted-foreground">
              <div>Repository: {session.repository_path}</div>
              <div className="mt-2">Configuration details available after training completes</div>
            </div>
          ) : null}
        </CardContent>
      </Card>

      {/* Adapter Registration Pending Warning */}
      {shouldPollAdapters && (
        <Alert variant="default" className="border-warning-border bg-warning-surface text-warning">
          <AlertTriangle className="h-4 w-4" />
          <AlertDescription>
            Training completed, but adapter registration is pending. Checking adapter registry...
            {adapters.length > 0 && (
              <span className="ml-2 text-sm text-muted-foreground">
                ({adapters.length} adapters found, still searching...)
              </span>
            )}
          </AlertDescription>
        </Alert>
      )}

      {/* Artifacts & Verification */}
      {artifacts && (
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center">
              <CheckCircle className="mr-2 h-5 w-5" />
              Packaged Artifacts
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4 text-sm">
              <div>
                <div className="text-muted-foreground">Artifact Path</div>
                <div className="font-mono break-all">{artifacts.artifact_path || 'n/a'}</div>
              </div>
              <div>
                <div className="text-muted-foreground">Adapter ID</div>
                <div className="font-mono">{artifacts.adapter_id || adapterName}</div>
              </div>
              <div>
                <div className="text-muted-foreground">Weights Hash (B3)</div>
                <div className="font-mono break-all">{artifacts.weights_hash_b3 || 'n/a'}</div>
              </div>
              <div>
                <div className="text-muted-foreground">Manifest Hash (B3)</div>
                <div className="font-mono break-all">{artifacts.manifest_hash_b3 || 'n/a'}</div>
              </div>
              <div>
                <div className="text-muted-foreground">Hash Matches</div>
                <Badge variant={artifacts.manifest_hash_matches ? 'default' : 'destructive'}>
                  {artifacts.manifest_hash_matches ? 'Yes' : 'No'}
                </Badge>
              </div>
              <div>
                <div className="text-muted-foreground">Signature Valid</div>
                <Badge variant={artifacts.signature_valid ? 'default' : 'destructive'}>
                  {artifacts.signature_valid ? 'Yes' : 'No'}
                </Badge>
              </div>
              <div>
                <div className="text-muted-foreground">Ready</div>
                <Badge variant={artifacts.ready ? 'default' : 'destructive'}>
                  {artifacts.ready ? 'Ready' : 'Not Ready'}
                </Badge>
              </div>
            </div>
            {job?.status === 'completed' && artifacts?.adapter_id && (
              <div className="mt-4 pt-4 border-t space-y-2">
                <div className="flex items-center gap-2">
                  <Link to={`/adapters/${artifacts.adapter_id}`}>
                    <Button variant="outline" size="sm">
                      View Adapter
                    </Button>
                  </Link>
                  <Link to={buildInferenceLink()}>
                    <Button variant="default" size="sm">
                      Test in Chat
                    </Button>
                  </Link>
                </div>
                <p className="text-xs text-muted-foreground">
                  Adapter registered successfully. A default stack was created automatically.
                </p>
              </div>
            )}
          </CardContent>
        </Card>
      )}

      {/* Training Logs - only show in job mode */}
      {job && (
        <SectionErrorBoundary sectionName="Training Logs">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center">
                <Activity className="mr-2 h-5 w-5" />
                Training Logs
              </CardTitle>
            </CardHeader>
            <CardContent>
              <ScrollArea className="h-64 w-full" ref={logScrollRef}>
                <div className="space-y-1 font-mono text-sm">
                  {logs.map((log, idx) => (
                    <div key={idx} className="text-muted-foreground">
                      {log}
                    </div>
                  ))}
                  {logs.length === 0 && (
                    <div className="text-center text-muted-foreground py-8">
                      No logs available yet
                    </div>
                  )}
                </div>
              </ScrollArea>
            </CardContent>
          </Card>
        </SectionErrorBoundary>
      )}

      {/* Loss Chart Placeholder */}
      <SectionErrorBoundary sectionName="Progress Chart">
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center">
              <TrendingDown className="mr-2 h-5 w-5" />
              Loss Trend
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="h-48 flex items-center justify-center text-muted-foreground">
              Loss visualization chart would go here
              <br />
              <small className="text-xs">Real-time loss tracking and validation curves</small>
            </div>
          </CardContent>
        </Card>
      </SectionErrorBoundary>
    </div>
  );
}
