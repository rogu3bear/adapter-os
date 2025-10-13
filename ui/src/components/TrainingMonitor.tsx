import React, { useState, useEffect, useRef } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Progress } from './ui/progress';
import { Alert, AlertDescription } from './ui/alert';
import { ScrollArea } from './ui/scroll-area';
import { 
  Activity, 
  Pause, 
  Square, 
  Play, 
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
import apiClient from '../api/client';
import { TrainingJob, TrainingMetrics } from '../api/types';

interface TrainingMonitorProps {
  jobId: string;
  onClose?: () => void;
}

export function TrainingMonitor({ jobId, onClose }: TrainingMonitorProps) {
  const [job, setJob] = useState<TrainingJob | null>(null);
  const [metrics, setMetrics] = useState<TrainingMetrics | null>(null);
  const [logs, setLogs] = useState<string[]>([]);
  const [isPolling, setIsPolling] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const intervalRef = useRef<number | null>(null);
  const logScrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const fetchJobData = async () => {
      try {
        const [jobData, metricsData, logsData] = await Promise.all([
          apiClient.getTrainingJob(jobId),
          apiClient.getTrainingMetrics(jobId),
          apiClient.getTrainingLogs(jobId)
        ]);
        
        setJob(jobData);
        setMetrics(metricsData);
        setLogs(logsData);
        setError(null);

        // Auto-scroll logs to bottom
        if (logScrollRef.current) {
          logScrollRef.current.scrollTop = logScrollRef.current.scrollHeight;
        }
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Failed to fetch training data');
      }
    };

    fetchJobData();

    if (isPolling && job?.status === 'running') {
      intervalRef.current = window.setInterval(fetchJobData, 2000); // Poll every 2 seconds
    }

    return () => {
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
      }
    };
  }, [jobId, isPolling, job?.status]);

  const handlePause = async () => {
    try {
      // TODO: Implement pause functionality
      console.log('Pausing training job:', jobId);
    } catch (err) {
      console.error('Failed to pause training:', err);
    }
  };

  const handleStop = async () => {
    try {
      await apiClient.cancelTraining(jobId);
      setIsPolling(false);
    } catch (err) {
      console.error('Failed to cancel training:', err);
    }
  };

  const getStatusIcon = (status: string) => {
    switch (status) {
      case 'running': return <Activity className="h-4 w-4 text-blue-600 animate-pulse" />;
      case 'completed': return <CheckCircle className="h-4 w-4 text-green-600" />;
      case 'failed': return <XCircle className="h-4 w-4 text-red-600" />;
      case 'cancelled': return <AlertTriangle className="h-4 w-4 text-yellow-600" />;
      case 'queued': return <Clock className="h-4 w-4 text-gray-600" />;
      default: return <AlertTriangle className="h-4 w-4 text-gray-600" />;
    }
  };

  const getStatusBadge = (status: string) => {
    const variants = {
      running: 'bg-blue-100 text-blue-800',
      completed: 'bg-green-100 text-green-800',
      failed: 'bg-red-100 text-red-800',
      cancelled: 'bg-yellow-100 text-yellow-800',
      queued: 'bg-gray-100 text-gray-800'
    };
    return variants[status as keyof typeof variants] || 'bg-gray-100 text-gray-800';
  };

  const formatDuration = (startTime: string) => {
    const start = new Date(startTime);
    const now = new Date();
    const diffMs = now.getTime() - start.getTime();
    const diffMins = Math.floor(diffMs / 60000);
    const diffHours = Math.floor(diffMins / 60);
    const diffDays = Math.floor(diffHours / 24);

    if (diffDays > 0) return `${diffDays}d ${diffHours % 24}h ${diffMins % 60}m`;
    if (diffHours > 0) return `${diffHours}h ${diffMins % 60}m`;
    return `${diffMins}m`;
  };

  const formatETA = (startTime: string, progress: number) => {
    if (progress === 0) return 'Calculating...';
    
    const start = new Date(startTime);
    const now = new Date();
    const elapsedMs = now.getTime() - start.getTime();
    const totalEstimatedMs = (elapsedMs / progress) * 100;
    const remainingMs = totalEstimatedMs - elapsedMs;
    
    const remainingMins = Math.floor(remainingMs / 60000);
    const remainingHours = Math.floor(remainingMins / 60);
    
    if (remainingHours > 0) return `${remainingHours}h ${remainingMins % 60}m`;
    return `${remainingMins}m`;
  };

  if (error) {
    return (
      <Alert variant="destructive">
        <XCircle className="h-4 w-4" />
        <AlertDescription>{error}</AlertDescription>
      </Alert>
    );
  }

  if (!job) {
    return <div className="text-center p-8">Loading training job...</div>;
  }

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center space-x-3">
          {getStatusIcon(job.status)}
          <div>
            <h2 className="text-lg font-semibold">{job.adapter_name}</h2>
            <div className="flex items-center space-x-2 text-sm text-muted-foreground">
              <Badge className={getStatusBadge(job.status)}>
                {job.status}
              </Badge>
              <span>•</span>
              <span>Running for {formatDuration(job.started_at)}</span>
              {job.status === 'running' && (
                <>
                  <span>•</span>
                  <span>ETA: {formatETA(job.started_at, job.progress)}</span>
                </>
              )}
            </div>
          </div>
        </div>
        
        <div className="flex space-x-2">
          {job.status === 'running' && (
            <>
              <Button variant="outline" size="sm" onClick={handlePause}>
                <Pause className="h-4 w-4 mr-1" />
                Pause
              </Button>
              <Button variant="outline" size="sm" onClick={handleStop}>
                <Square className="h-4 w-4 mr-1" />
                Stop
              </Button>
            </>
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
              <span className="text-sm text-muted-foreground">{job.progress}%</span>
            </div>
            <Progress value={job.progress} className="h-2" />
            
            {metrics && (
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
                  <div className="text-2xl font-bold">{metrics.loss.toFixed(4)}</div>
                  <div className="text-xs text-muted-foreground">Loss</div>
                </div>
                <div className="text-center">
                  <div className="text-2xl font-bold">{metrics.validation_loss.toFixed(4)}</div>
                  <div className="text-xs text-muted-foreground">Val Loss</div>
                </div>
              </div>
            )}
          </div>
        </CardContent>
      </Card>

      {/* Resource Usage */}
      {metrics && (
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
                  <span className="text-sm text-muted-foreground">{metrics.memory_usage}GB</span>
                </div>
                <Progress value={(metrics.memory_usage / 24) * 100} className="h-2" />
              </div>
              
              <div className="space-y-2">
                <div className="flex items-center justify-between">
                  <div className="flex items-center space-x-2">
                    <Zap className="h-4 w-4" />
                    <span className="text-sm font-medium">Tokens/sec</span>
                  </div>
                  <span className="text-sm text-muted-foreground">{metrics.tokens_per_second}</span>
                </div>
                <div className="h-2 bg-gray-200 rounded-full">
                  <div 
                    className="h-2 bg-green-500 rounded-full transition-all duration-300"
                    style={{ width: `${Math.min((metrics.tokens_per_second / 2000) * 100, 100)}%` }}
                  />
                </div>
              </div>
            </div>
          </CardContent>
        </Card>
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
              <div className="font-medium">{job.config.category}</div>
            </div>
            <div>
              <div className="text-muted-foreground">Scope</div>
              <div className="font-medium">{job.config.scope}</div>
            </div>
            {job.config.framework_id && (
              <div>
                <div className="text-muted-foreground">Framework</div>
                <div className="font-medium">{job.config.framework_id} {job.config.framework_version}</div>
              </div>
            )}
          </div>
        </CardContent>
      </Card>

      {/* Training Logs */}
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

      {/* Loss Chart Placeholder */}
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
    </div>
  );
}
