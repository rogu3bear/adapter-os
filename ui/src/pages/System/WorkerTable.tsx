import { useMemo, useCallback } from 'react';
import { type WorkerResponse, type WorkerHealthSummary } from '@/api/api-types';
import { DataTable } from '@/components/shared/DataTable/DataTable';
import { type Column } from '@/components/shared/DataTable/types';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { MoreHorizontal, FileText, Bug, StopCircle, AlertTriangle } from 'lucide-react';
import { useWorkerOperations } from '@/hooks/system/useSystemMetrics';
import { useToast } from '@/hooks/use-toast';

interface WorkerTableProps {
  workers: WorkerResponse[];
  healthSummaries?: WorkerHealthSummary[];
  isLoading: boolean;
  onWorkerSelect: (workerId: string) => void;
  onIncidentsSelect?: (workerId: string) => void;
  onRefresh: () => void;
}

export default function WorkerTable({
  workers,
  healthSummaries = [],
  isLoading,
  onWorkerSelect,
  onIncidentsSelect,
  onRefresh
}: WorkerTableProps) {
  const { stopWorker } = useWorkerOperations();
  const { toast } = useToast();

  // Create a map of worker health data for quick lookup
  const healthMap = useMemo(() => {
    const map = new Map<string, WorkerHealthSummary>();
    healthSummaries.forEach(health => {
      map.set(health.worker_id, health);
    });
    return map;
  }, [healthSummaries]);

  const getHealthBadge = useCallback((status: string) => {
    const variant =
      status === 'healthy'
        ? 'default'
        : status === 'degraded'
        ? 'warning'
        : status === 'crashed'
        ? 'destructive'
        : 'secondary';
    return <Badge variant={variant}>{status.charAt(0).toUpperCase() + status.slice(1)}</Badge>;
  }, []);

  const handleViewIncidents = useCallback((workerId: string) => {
    if (onIncidentsSelect) {
      onIncidentsSelect(workerId);
    }
  }, [onIncidentsSelect]);

  const handleStopWorker = useCallback(async (workerId: string) => {
    if (!confirm(`Are you sure you want to stop worker ${workerId}?`)) {
      return;
    }

    try {
      await stopWorker.execute(workerId, false);
      toast({
        title: 'Worker Stopped',
        description: `Worker ${workerId} has been stopped`,
      });
      onRefresh();
    } catch (error) {
      toast({
        title: 'Operation Failed',
        description: error instanceof Error ? error.message : 'Unknown error',
        variant: 'destructive',
      });
    }
  }, [stopWorker, toast, onRefresh]);

  const columns = useMemo<Column<WorkerResponse>[]>(
    () => [
      {
        id: 'worker_id',
        accessorKey: 'worker_id',
        header: 'Worker ID',
        cell: ({ row }) => (
          <Button
            variant="link"
            className="p-0 h-auto font-mono text-sm"
            onClick={() => onWorkerSelect(row.worker_id)}
          >
            {row.worker_id}
          </Button>
        ),
      },
      {
        id: 'status',
        accessorKey: 'status',
        header: 'Status',
        cell: ({ row }) => {
          const status = row.status;
          const variant =
            status === 'running'
              ? 'success'
              : status === 'stopped'
              ? 'secondary'
              : status === 'error'
              ? 'destructive'
              : 'default';
          return (
            <Badge variant={variant}>
              {status.charAt(0).toUpperCase() + status.slice(1)}
            </Badge>
          );
        },
      },
      {
        id: 'worker_type',
        accessorKey: 'worker_type',
        header: 'Type',
        cell: ({ row }) => row.worker_type ?? '--',
      },
      {
        id: 'node_id',
        accessorKey: 'node_id',
        header: 'Node ID',
        cell: ({ row }) => (
          <span className="font-mono text-sm">{row.node_id}</span>
        ),
      },
      {
        id: 'tenant_id',
        accessorKey: 'tenant_id',
        header: 'Organization',
        cell: ({ row }) => row.tenant_id ?? '--',
      },
      {
        id: 'plan_id',
        accessorKey: 'plan_id',
        header: 'Plan',
        cell: ({ row }) => row.plan_id ?? '--',
      },
      {
        id: 'manifest_hash',
        accessorKey: 'manifest_hash',
        header: 'Manifest Hash',
        cell: ({ row }) => {
          const hash = row.manifest_hash;
          if (!hash) return <span className="text-muted-foreground">--</span>;
          // Show first 12 characters of hash with tooltip
          return (
            <span
              className="font-mono text-xs cursor-help"
              title={hash}
            >
              {hash.slice(0, 12)}...
            </span>
          );
        },
      },
      {
        id: 'memory_mb',
        accessorKey: 'memory_mb',
        header: 'Memory',
        cell: ({ row }) => {
          const memory = row.memory_mb;
          return memory ? `${memory} MB` : '--';
        },
      },
      {
        id: 'cpu_percent',
        accessorKey: 'cpu_percent',
        header: 'CPU',
        cell: ({ row }) => {
          const cpu = row.cpu_percent;
          return cpu ? `${cpu.toFixed(1)}%` : '--';
        },
      },
      {
        id: 'health_status',
        header: 'Health',
        cell: ({ row }) => {
          const health = healthMap.get(row.worker_id);
          if (!health) return <span className="text-muted-foreground">--</span>;
          return getHealthBadge(health.health_status);
        },
      },
      {
        id: 'avg_latency',
        header: 'Avg Latency',
        cell: ({ row }) => {
          const health = healthMap.get(row.worker_id);
          if (!health || health.avg_latency_ms === 0) {
            return <span className="text-muted-foreground">--</span>;
          }
          const latency = health.avg_latency_ms;
          const className = latency > 1000 ? 'text-destructive font-semibold' : latency > 500 ? 'text-warning' : '';
          return <span className={className}>{latency.toFixed(0)}ms</span>;
        },
      },
      {
        id: 'incidents',
        header: 'Incidents',
        cell: ({ row }) => {
          const health = healthMap.get(row.worker_id);
          if (!health || health.total_failures === 0) {
            return <span className="text-muted-foreground">--</span>;
          }
          return (
            <Button
              variant="link"
              size="sm"
              className="p-0 h-auto text-destructive"
              onClick={() => handleViewIncidents(row.worker_id)}
            >
              <AlertTriangle className="h-4 w-4 mr-1" />
              {health.total_failures}
            </Button>
          );
        },
      },
      {
        id: 'created_at',
        accessorKey: 'created_at',
        header: 'Created',
        cell: ({ row }) => {
          const created = row.created_at;
          if (!created) return '--';
          const date = new Date(created);
          return date.toLocaleString();
        },
      },
      {
        id: 'actions',
        header: 'Actions',
        cell: ({ row }) => {
          return (
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button variant="ghost" size="sm">
                  <MoreHorizontal className="h-4 w-4" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuItem onClick={() => onWorkerSelect(row.worker_id)}>
                  <FileText className="mr-2 h-4 w-4" />
                  View Logs
                </DropdownMenuItem>
                <DropdownMenuItem
                  onClick={() => handleStopWorker(row.worker_id)}
                  disabled={stopWorker.isLoading || row.status === 'stopped'}
                >
                  <StopCircle className="mr-2 h-4 w-4" />
                  Stop Worker
                </DropdownMenuItem>
                <DropdownMenuItem onClick={() => onWorkerSelect(row.worker_id)}>
                  <Bug className="mr-2 h-4 w-4" />
                  Debug
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          );
        },
      },
    ],
    [onWorkerSelect, stopWorker.isLoading, handleStopWorker, healthMap, getHealthBadge, handleViewIncidents]
  );

  return (
    <DataTable
      columns={columns}
      data={workers}
      getRowId={(row) => row.worker_id}
      isLoading={isLoading}
      globalFilter=""
    />
  );
}
