import { useMemo } from 'react';
import { type WorkerResponse } from '@/api/api-types';
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
import { MoreHorizontal, FileText, Bug, StopCircle } from 'lucide-react';
import { useWorkerOperations } from '@/hooks/useSystemMetrics';
import { useToast } from '@/hooks/use-toast';

interface WorkerTableProps {
  workers: WorkerResponse[];
  isLoading: boolean;
  onWorkerSelect: (workerId: string) => void;
  onRefresh: () => void;
}

export default function WorkerTable({ workers, isLoading, onWorkerSelect, onRefresh }: WorkerTableProps) {
  const { stopWorker } = useWorkerOperations();
  const { toast } = useToast();

  const handleStopWorker = async (workerId: string) => {
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
  };

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
        header: 'Tenant',
        cell: ({ row }) => row.tenant_id ?? '--',
      },
      {
        id: 'plan_id',
        accessorKey: 'plan_id',
        header: 'Plan',
        cell: ({ row }) => row.plan_id ?? '--',
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
    [onWorkerSelect, stopWorker.isLoading]
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
