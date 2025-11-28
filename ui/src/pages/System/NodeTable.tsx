import { useMemo } from 'react';
import { type Node } from '@/api/api-types';
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
import { MoreHorizontal, Activity, PowerOff, Trash2 } from 'lucide-react';
import { useNodeOperations } from '@/hooks/useSystemMetrics';
import { useToast } from '@/hooks/use-toast';

interface NodeTableProps {
  nodes: Node[];
  isLoading: boolean;
  onNodeSelect: (nodeId: string) => void;
  onRefresh: () => void;
}

export default function NodeTable({ nodes, isLoading, onNodeSelect, onRefresh }: NodeTableProps) {
  const { pingNode, markOffline, evictNode } = useNodeOperations();
  const { toast } = useToast();

  const handlePing = async (nodeId: string) => {
    try {
      const result = await pingNode.execute(nodeId);
      toast({
        title: 'Node Ping',
        description: `Status: ${result.status}, Latency: ${result.latency_ms}ms`,
      });
      onRefresh();
    } catch (error) {
      toast({
        title: 'Ping Failed',
        description: error instanceof Error ? error.message : 'Unknown error',
        variant: 'destructive',
      });
    }
  };

  const handleMarkOffline = async (nodeId: string) => {
    try {
      await markOffline.execute(nodeId);
      toast({
        title: 'Node Marked Offline',
        description: `Node ${nodeId} has been marked offline`,
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

  const handleEvict = async (nodeId: string) => {
    if (!confirm(`Are you sure you want to remove node ${nodeId}? This action cannot be undone.`)) {
      return;
    }

    try {
      await evictNode.execute(nodeId);
      toast({
        title: 'Node Removed',
        description: `Node ${nodeId} has been removed from the cluster`,
      });
      onRefresh();
    } catch (error) {
      toast({
        title: 'Removal Failed',
        description: error instanceof Error ? error.message : 'Unknown error',
        variant: 'destructive',
      });
    }
  };

  const columns = useMemo<Column<Node>[]>(
    () => [
      {
        id: 'id',
        accessorKey: 'id',
        header: 'Node ID',
        cell: ({ row }) => (
          <Button
            variant="link"
            className="p-0 h-auto font-mono text-sm"
            onClick={() => onNodeSelect(row.id)}
          >
            {row.id}
          </Button>
        ),
      },
      {
        id: 'hostname',
        accessorKey: 'hostname',
        header: 'Hostname',
      },
      {
        id: 'status',
        accessorKey: 'status',
        header: 'Status',
        cell: ({ row }) => {
          const status = row.status;
          const variant =
            status === 'healthy'
              ? 'success'
              : status === 'offline'
              ? 'secondary'
              : 'destructive';
          return (
            <Badge variant={variant}>
              {status.charAt(0).toUpperCase() + status.slice(1)}
            </Badge>
          );
        },
      },
      {
        id: 'memory_gb',
        accessorKey: 'memory_gb',
        header: 'Memory',
        cell: ({ row }) => {
          const memory = row.memory_gb;
          return memory ? `${memory} GB` : '--';
        },
      },
      {
        id: 'gpu_count',
        accessorKey: 'gpu_count',
        header: 'GPUs',
        cell: ({ row }) => row.gpu_count ?? '--',
      },
      {
        id: 'metal_family',
        accessorKey: 'metal_family',
        header: 'Metal Family',
        cell: ({ row }) => row.metal_family ?? '--',
      },
      {
        id: 'last_heartbeat',
        accessorKey: 'last_heartbeat',
        header: 'Last Seen',
        cell: ({ row }) => {
          const heartbeat = row.last_heartbeat;
          if (!heartbeat) return '--';
          const date = new Date(heartbeat);
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
                <DropdownMenuItem onClick={() => onNodeSelect(row.id)}>
                  View Details
                </DropdownMenuItem>
                <DropdownMenuItem
                  onClick={() => handlePing(row.id)}
                  disabled={pingNode.isLoading}
                >
                  <Activity className="mr-2 h-4 w-4" />
                  Ping Node
                </DropdownMenuItem>
                <DropdownMenuItem
                  onClick={() => handleMarkOffline(row.id)}
                  disabled={markOffline.isLoading}
                >
                  <PowerOff className="mr-2 h-4 w-4" />
                  Mark Offline
                </DropdownMenuItem>
                <DropdownMenuItem
                  onClick={() => handleEvict(row.id)}
                  disabled={evictNode.isLoading}
                  className="text-destructive"
                >
                  <Trash2 className="mr-2 h-4 w-4" />
                  Remove Node
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          );
        },
      },
    ],
    [onNodeSelect, pingNode.isLoading, markOffline.isLoading, evictNode.isLoading]
  );

  return (
    <DataTable
      columns={columns}
      data={nodes}
      getRowId={(row) => row.id}
      isLoading={isLoading}
      globalFilter=""
    />
  );
}
