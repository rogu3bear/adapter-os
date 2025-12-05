import React from 'react';
import { Link } from 'react-router-dom';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import { VirtualizedTableRows } from '@/components/ui/virtualized-table';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Progress } from '@/components/ui/progress';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';
import {
  Activity,
  CheckCircle,
  XCircle,
  Clock,
  AlertTriangle,
  Pause,
  Square,
  Play,
  Eye,
  RefreshCw,
  Trash2,
} from 'lucide-react';
import type { TrainingJob, TrainingStatus } from '@/api/training-types';
import { formatDurationSeconds, formatTimestamp } from '@/utils/format';

interface TrainingJobTableProps {
  jobs: TrainingJob[];
  isLoading: boolean;
  onViewJob: (job: TrainingJob) => void;
  onCancelJob: (jobId: string) => void;
  isCancelling: Set<string>;
  canCancel: boolean;
}

const STATUS_CONFIG: Record<TrainingStatus, {
  icon: React.ElementType;
  className: string;
  description: string;
}> = {
  pending: {
    icon: Clock,
    className: 'text-yellow-500',
    description: 'Job is queued and waiting to start',
  },
  running: {
    icon: Activity,
    className: 'text-blue-500 animate-pulse',
    description: 'Training is actively in progress',
  },
  completed: {
    icon: CheckCircle,
    className: 'text-green-500',
    description: 'Training finished successfully',
  },
  failed: {
    icon: XCircle,
    className: 'text-red-500',
    description: 'Training encountered an error',
  },
  cancelled: {
    icon: Square,
    className: 'text-gray-500',
    description: 'Training was cancelled by user',
  },
  paused: {
    icon: Pause,
    className: 'text-orange-500',
    description: 'Training is temporarily paused',
  },
};

function StatusBadge({ status }: { status: TrainingStatus }) {
  const config = STATUS_CONFIG[status] || STATUS_CONFIG.pending;
  const Icon = config.icon;

  return (
    <GlossaryTooltip termId={`status-${status}`} side="right">
      <Badge variant="outline" className="gap-1" title={config.description}>
        <Icon className={`h-3 w-3 ${config.className}`} />
        <span className="capitalize">{status}</span>
      </Badge>
    </GlossaryTooltip>
  );
}

// formatDuration replaced by shared formatDurationSeconds from @/utils/format

export function TrainingJobTable({
  jobs,
  isLoading,
  onViewJob,
  onCancelJob,
  isCancelling,
  canCancel,
}: TrainingJobTableProps) {
  if (isLoading && jobs.length === 0) {
    return (
      <div className="text-center py-8 text-muted-foreground">
        <RefreshCw className="h-6 w-6 animate-spin mx-auto mb-2" />
        Loading training jobs...
      </div>
    );
  }

  if (jobs.length === 0) {
    return (
      <div className="text-center py-8 text-muted-foreground">
        <Activity className="h-8 w-8 mx-auto mb-2 opacity-50" />
        <p>No training jobs found</p>
        <p className="text-sm mt-1">Start a new training job to see it here</p>
      </div>
    );
  }

  return (
    <div className="max-h-[600px] overflow-auto" data-virtual-container>
      <Table role="table" aria-label="Training jobs">
        <TableHeader>
          <TableRow role="row">
            <TableHead role="columnheader" scope="col">
              <GlossaryTooltip termId="training-job-id">Job ID / Name</GlossaryTooltip>
            </TableHead>
            <TableHead role="columnheader" scope="col">
              <GlossaryTooltip termId="training-dataset">Dataset</GlossaryTooltip>
            </TableHead>
            <TableHead role="columnheader" scope="col">
              <GlossaryTooltip termId="training-status">Status</GlossaryTooltip>
            </TableHead>
            <TableHead role="columnheader" scope="col">
              <GlossaryTooltip termId="training-progress">Progress</GlossaryTooltip>
            </TableHead>
            <TableHead role="columnheader" scope="col">
              <GlossaryTooltip termId="training-loss">Loss</GlossaryTooltip>
            </TableHead>
            <TableHead role="columnheader" scope="col">
              <GlossaryTooltip termId="training-epoch">Epoch</GlossaryTooltip>
            </TableHead>
            <TableHead role="columnheader" scope="col">
              <GlossaryTooltip termId="training-created">Created</GlossaryTooltip>
            </TableHead>
            <TableHead role="columnheader" scope="col">
              <GlossaryTooltip termId="training-actions">Actions</GlossaryTooltip>
            </TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          <VirtualizedTableRows items={jobs} estimateSize={60}>
            {(job) => {
              const typedJob = job as TrainingJob;
              const isJobCancelling = isCancelling.has(typedJob.id);
              const isActive = typedJob.status === 'running' || typedJob.status === 'paused';
              const isTerminal = typedJob.status === 'completed' || typedJob.status === 'failed' || typedJob.status === 'cancelled';

              return (
                <TableRow key={typedJob.id} role="row">
                  <TableCell className="font-medium">
                    <div className="flex flex-col">
                      <span className="truncate max-w-[200px]" title={typedJob.adapter_name || typedJob.id}>
                        {typedJob.adapter_name || typedJob.id.slice(0, 8)}
                      </span>
                      {typedJob.adapter_name && (
                        <span className="text-xs text-muted-foreground truncate max-w-[200px]">
                          {typedJob.id.slice(0, 8)}...
                        </span>
                      )}
                    </div>
                  </TableCell>
                  <TableCell className="text-muted-foreground">
                    <span className="truncate max-w-[150px] block" title={typedJob.dataset_id || '-'}>
                      {typedJob.dataset_id ? typedJob.dataset_id.slice(0, 12) + '...' : '-'}
                    </span>
                  </TableCell>
                  <TableCell>
                    <StatusBadge status={typedJob.status} />
                  </TableCell>
                  <TableCell>
                    <div className="flex items-center gap-2 min-w-[120px]">
                      <Progress
                        value={typedJob.progress_pct ?? typedJob.progress ?? 0}
                        className="w-20 h-2"
                      />
                      <span className="text-sm text-muted-foreground min-w-[36px]">
                        {typedJob.progress_pct ?? typedJob.progress ?? 0}%
                      </span>
                    </div>
                  </TableCell>
                  <TableCell className="text-muted-foreground font-mono">
                    {typedJob.current_loss?.toFixed(4) ?? typedJob.loss?.toFixed(4) ?? '-'}
                  </TableCell>
                  <TableCell className="text-muted-foreground">
                    {typedJob.current_epoch !== undefined && typedJob.total_epochs !== undefined
                      ? `${typedJob.current_epoch}/${typedJob.total_epochs}`
                      : '-'}
                  </TableCell>
                  <TableCell className="text-muted-foreground text-sm">
                    {formatTimestamp(typedJob.created_at || typedJob.started_at || '', 'long')}
                  </TableCell>
                  <TableCell>
                    <div className="flex items-center gap-1">
                      <Button
                        size="sm"
                        variant="outline"
                        onClick={() => onViewJob(typedJob)}
                        title="View job details"
                        aria-label={`View ${typedJob.adapter_name || typedJob.id}`}
                      >
                        <Eye className="h-4 w-4" />
                      </Button>

                      {isActive && canCancel && (
                        <Button
                          size="sm"
                          variant={isJobCancelling ? 'secondary' : 'destructive'}
                          onClick={() => onCancelJob(typedJob.id)}
                          disabled={isJobCancelling}
                          title="Cancel training job"
                          aria-label={`Cancel ${typedJob.adapter_name || typedJob.id}`}
                        >
                          {isJobCancelling ? (
                            <RefreshCw className="h-4 w-4 animate-spin" />
                          ) : (
                            <Square className="h-4 w-4" />
                          )}
                        </Button>
                      )}

                      {isTerminal && (
                        <Link to={`/training/jobs/${typedJob.id}`}>
                          <Button
                            size="sm"
                            variant="outline"
                            title="View full details"
                          >
                            Details
                          </Button>
                        </Link>
                      )}

                      {typedJob.adapter_id && (
                        <Link to={`/adapters/${typedJob.adapter_id}#overview`}>
                          <Button
                            size="sm"
                            variant="ghost"
                            title="View adapter"
                          >
                            View adapter
                          </Button>
                        </Link>
                      )}

                      {typedJob.status === 'completed' && (
                        <Link to={`/testing?adapter=${typedJob.adapter_id}`}>
                          <Button size="sm" variant="default">
                            Test
                          </Button>
                        </Link>
                      )}
                    </div>
                  </TableCell>
                </TableRow>
              );
            }}
          </VirtualizedTableRows>
        </TableBody>
      </Table>
    </div>
  );
}
