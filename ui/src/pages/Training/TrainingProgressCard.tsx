import { useEffect, useState } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Progress } from '@/components/ui/progress';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { useLiveData } from '@/hooks/realtime/useLiveData';
import { Activity, AlertCircle, CheckCircle, XCircle, Clock, Zap } from 'lucide-react';
import type { TrainingJob, TrainingMetrics } from '@/api/training-types';
import { calculateTrainingETA, formatDuration as formatDurationUtil } from '@/utils/trainingEta';

interface TrainingProgressCardProps {
  jobId: string;
  initialJob?: TrainingJob;
}

interface TrainingEvent {
  event_type: 'training.progress' | 'training.completed' | 'training.failed';
  job_id: string;
  payload: {
    progress_pct?: number;
    current_loss?: number;
    current_epoch?: number;
    total_epochs?: number;
    tokens_per_second?: number;
    eta_seconds?: number;
    learning_rate?: number;
    status?: string;
    error_message?: string;
  };
}

// Use utility function for formatting
const formatDuration = formatDurationUtil;

export function TrainingProgressCard({ jobId, initialJob }: TrainingProgressCardProps) {
  const [progress, setProgress] = useState<TrainingMetrics>({
    progress_pct: initialJob?.progress_pct ?? 0,
    loss: initialJob?.current_loss ?? initialJob?.loss,
    current_epoch: initialJob?.current_epoch,
    total_epochs: initialJob?.total_epochs,
    tokens_per_second: initialJob?.tokens_per_second,
    eta_seconds: initialJob?.eta_seconds,
    learning_rate: initialJob?.learning_rate,
  });
  const [status, setStatus] = useState<string>(initialJob?.status ?? 'running');
  const [errorMessage, setErrorMessage] = useState<string | undefined>(initialJob?.error_message ?? undefined);

  const { error: sseError, connectionStatus } = useLiveData({
    sseEndpoint: '/v1/streams/training',
    sseEventType: 'training',
    fetchFn: async () => {
      // No polling fallback for training progress - SSE only
      return null;
    },
    enabled: status === 'running' || status === 'pending',
    pollingSpeed: 'fast',
    onSSEMessage: (event) => {
      const trainingEvent = event as TrainingEvent;
      if (trainingEvent.job_id === jobId) {
        if (trainingEvent.event_type === 'training.progress') {
          setProgress({
            progress_pct: trainingEvent.payload.progress_pct,
            loss: trainingEvent.payload.current_loss,
            current_epoch: trainingEvent.payload.current_epoch,
            total_epochs: trainingEvent.payload.total_epochs,
            tokens_per_second: trainingEvent.payload.tokens_per_second,
            eta_seconds: trainingEvent.payload.eta_seconds,
            learning_rate: trainingEvent.payload.learning_rate,
          });
        } else if (trainingEvent.event_type === 'training.completed') {
          setStatus('completed');
          setProgress((prev) => ({ ...prev, progress_pct: 100 }));
        } else if (trainingEvent.event_type === 'training.failed') {
          setStatus('failed');
          setErrorMessage(trainingEvent.payload.error_message);
        }
      }
    },
  });

  const connected = connectionStatus === 'sse' || connectionStatus === 'polling';

  const getStatusIcon = () => {
    switch (status) {
      case 'running':
        return <Activity className="h-4 w-4 text-blue-500 animate-pulse" />;
      case 'completed':
        return <CheckCircle className="h-4 w-4 text-green-500" />;
      case 'failed':
        return <XCircle className="h-4 w-4 text-red-500" />;
      case 'pending':
        return <Clock className="h-4 w-4 text-yellow-500" />;
      default:
        return <Activity className="h-4 w-4 text-gray-500" />;
    }
  };

  const getStatusColor = () => {
    switch (status) {
      case 'running':
        return 'default';
      case 'completed':
        return 'default';
      case 'failed':
        return 'destructive';
      case 'pending':
        return 'secondary';
      default:
        return 'outline';
    }
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center justify-between">
          <span className="flex items-center gap-2">
            {getStatusIcon()}
            Training Progress
          </span>
          <Badge variant={getStatusColor()}>
            {status}
          </Badge>
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        {/* Connection Status */}
        {!connected && status === 'running' && (
          <Alert>
            <AlertCircle className="h-4 w-4" />
            <AlertDescription>
              Live updates disconnected. Progress may be delayed.
            </AlertDescription>
          </Alert>
        )}

        {sseError && (
          <Alert variant="destructive">
            <AlertCircle className="h-4 w-4" />
            <AlertDescription>
              Failed to connect to live updates: {sseError.message}
            </AlertDescription>
          </Alert>
        )}

        {errorMessage && (
          <Alert variant="destructive">
            <XCircle className="h-4 w-4" />
            <AlertDescription>
              {errorMessage}
            </AlertDescription>
          </Alert>
        )}

        {/* Progress Bar */}
        <div className="space-y-2">
          <div className="flex items-center justify-between text-sm">
            <span className="font-medium">Overall Progress</span>
            <span className="text-muted-foreground">{progress.progress_pct ?? 0}%</span>
          </div>
          <Progress value={progress.progress_pct ?? 0} className="h-2" />
        </div>

        {/* Epoch Progress */}
        {progress.current_epoch !== undefined && progress.total_epochs !== undefined && (
          <div className="flex items-center justify-between text-sm">
            <span className="text-muted-foreground">Epoch</span>
            <span className="font-medium">
              {progress.current_epoch} / {progress.total_epochs}
            </span>
          </div>
        )}

        {/* Loss */}
        {progress.loss !== undefined && (
          <div className="flex items-center justify-between text-sm">
            <span className="text-muted-foreground">Current Loss</span>
            <span className="font-mono font-medium">
              {progress.loss.toFixed(6)}
            </span>
          </div>
        )}

        {/* Learning Rate */}
        {progress.learning_rate !== undefined && (
          <div className="flex items-center justify-between text-sm">
            <span className="text-muted-foreground">Learning Rate</span>
            <span className="font-mono font-medium">
              {progress.learning_rate.toExponential(2)}
            </span>
          </div>
        )}

        {/* Tokens per Second */}
        {progress.tokens_per_second !== undefined && progress.tokens_per_second > 0 && (
          <div className="flex items-center justify-between text-sm">
            <span className="text-muted-foreground flex items-center gap-1">
              <Zap className="h-3 w-3" />
              Tokens/sec
            </span>
            <span className="font-medium">
              {progress.tokens_per_second.toFixed(0)}
            </span>
          </div>
        )}

        {/* ETA */}
        {(status === 'running' || status === 'pending' || status === 'paused') && (
          <div className="flex items-center justify-between text-sm">
            <span className="text-muted-foreground flex items-center gap-1">
              <Clock className="h-3 w-3" />
              Estimated Time Remaining
            </span>
            <span className="font-medium">
              {status === 'paused'
                ? 'Paused'
                : progress.eta_seconds !== undefined && progress.eta_seconds > 0
                ? formatDuration(progress.eta_seconds)
                : initialJob?.started_at && progress.progress_pct
                ? formatDuration(calculateTrainingETA(progress.progress_pct, initialJob.started_at, undefined, status))
                : 'Calculating...'}
            </span>
          </div>
        )}

        {/* Live Update Indicator */}
        {connected && status === 'running' && (
          <div className="flex items-center gap-2 text-xs text-muted-foreground pt-2 border-t">
            <div className="h-2 w-2 bg-green-500 rounded-full animate-pulse" />
            <span>Live updates active</span>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
