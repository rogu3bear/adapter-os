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
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';
import { PageTable } from '@/components/ui/PageTable';
import { LoadingState } from '@/components/ui/loading-state';
import { EmptyState } from '@/components/ui/empty-state';
import {
  Activity,
  CheckCircle,
  XCircle,
  Clock,
  Square,
  Eye,
  RefreshCw,
} from 'lucide-react';
import type { TrainingJob, TrainingStatus } from '@/api/training-types';
import { formatDurationSeconds, formatTimestamp } from '@/lib/formatters';

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
    icon: Clock,
    className: 'text-orange-500',
    description: 'Training is paused',
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
    return <LoadingState variant="minimal" message="Loading training jobs..." />;
  }

  if (jobs.length === 0) {
    return (
      <EmptyState
        variant="minimal"
        icon={Activity}
        title="No training jobs found"
        description="Start a new training job to see it here"
      />
    );
  }

  return (
    <PageTable minWidth="md">
      <div className="max-h-[calc(var(--base-unit)*150)] overflow-auto" data-virtual-container>
      <Table role="table" aria-label="Training jobs">
        <TableHeader>
          <TableRow role="row">
            <TableHead role="columnheader" scope="col">
              <GlossaryTooltip termId="training-job-id">Job</GlossaryTooltip>
            </TableHead>
            <TableHead role="columnheader" scope="col">
              Repository
            </TableHead>
            <TableHead role="columnheader" scope="col">
              Branch
            </TableHead>
            <TableHead role="columnheader" scope="col">
              <GlossaryTooltip termId="training-status">Status</GlossaryTooltip>
            </TableHead>
            <TableHead role="columnheader" scope="col">
              Requested policy
            </TableHead>
            <TableHead role="columnheader" scope="col">
              Backend used
            </TableHead>
            <TableHead role="columnheader" scope="col">
              CoreML
            </TableHead>
            <TableHead role="columnheader" scope="col">
              Start time
            </TableHead>
            <TableHead role="columnheader" scope="col">
              Duration
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
              const isActive = typedJob.status === 'running';
              const isTerminal = typedJob.status === 'completed' || typedJob.status === 'failed' || typedJob.status === 'cancelled';
              const backendLabel = typedJob.backend ?? 'n/a';
              const backendDevice = typedJob.backend_device ?? '';
              const usingGpu = typedJob.backend_device?.toLowerCase().includes('gpu')
                || typedJob.backend?.toLowerCase().includes('metal')
                || typedJob.backend?.toLowerCase().includes('coreml')
                || typedJob.require_gpu === true;
              const coremlUsed = Boolean(
                typedJob.coreml_export_requested ||
                typedJob.coreml_training_fallback === 'used' ||
                (typedJob.backend ?? '').toLowerCase().includes('coreml') ||
                (typedJob.backend_device ?? '').toLowerCase().includes('ane')
              );
              const startedAt = typedJob.started_at
                ? new Date(typedJob.started_at).getTime()
                : typedJob.created_at
                  ? new Date(typedJob.created_at).getTime()
                  : undefined;

              return (
                <TableRow key={typedJob.id} role="row">
                  <TableCell className="font-medium">
                    <div className="flex flex-col">
                      <span className="truncate max-w-[calc(var(--base-unit)*50)]" title={typedJob.adapter_name || typedJob.id}>
                        {typedJob.adapter_name || typedJob.id.slice(0, 8)}
                      </span>
                      {typedJob.adapter_name && (
                        <span className="text-xs text-muted-foreground truncate max-w-[calc(var(--base-unit)*50)]">
                          {typedJob.id.slice(0, 8)}...
                        </span>
                      )}
                    </div>
                  </TableCell>
                  <TableCell className="text-muted-foreground">
                    {typedJob.repo_id ?? typedJob.config?.repo_id ?? '—'}
                  </TableCell>
                  <TableCell className="text-muted-foreground">
                    {typedJob.target_branch ?? typedJob.branch ?? (typedJob as any).branch_classification ?? typedJob.config?.commit_sha ?? '—'}
                  </TableCell>
                  <TableCell>
                    <StatusBadge status={typedJob.status as TrainingStatus} />
                  </TableCell>
                  <TableCell className="text-muted-foreground text-sm">
                    {normalizeBackendPolicy(typedJob.backend_policy ?? undefined)}
                  </TableCell>
                  <TableCell className="text-muted-foreground text-sm">
                    {backendLabel}{' '}
                    <Badge variant={usingGpu ? 'default' : 'secondary'} className="text-[10px] ml-2">
                      {usingGpu ? 'GPU/Accelerator' : 'CPU'}
                    </Badge>
                  </TableCell>
                  <TableCell className="text-muted-foreground text-sm">
                    {coremlUsed ? 'Yes' : 'No'}
                  </TableCell>
                  <TableCell className="text-muted-foreground text-sm">
                    {formatTimestamp(typedJob.started_at ?? typedJob.created_at ?? '', 'long')}
                  </TableCell>
                  <TableCell className="text-muted-foreground text-sm">
                    {startedAt && typedJob.completed_at
                      ? formatDurationSeconds((new Date(typedJob.completed_at).getTime() - startedAt) / 1000)
                      : startedAt && (typedJob.status === 'running' || typedJob.status === 'pending')
                        ? formatDurationSeconds((Date.now() - startedAt) / 1000)
                        : '—'}
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
    </PageTable>
  );
}
