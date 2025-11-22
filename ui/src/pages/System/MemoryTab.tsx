import { useMemo } from 'react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Progress } from '@/components/ui/progress';
import { Skeleton } from '@/components/ui/skeleton';
import { Button } from '@/components/ui/button';
import { DataTable } from '@/components/shared/DataTable/DataTable';
import { type Column } from '@/components/shared/DataTable/types';
import { useMemoryUsage, useMemoryOperations } from '@/hooks/useSystemMetrics';
import { useToast } from '@/hooks/use-toast';
import { Trash2 } from 'lucide-react';

interface AdapterMemoryInfo {
  id: string;
  name: string;
  memory_usage_mb: number;
  state: string;
  pinned: boolean;
  category: string;
}

export default function MemoryTab() {
  const { data: memoryData, isLoading, refetch } = useMemoryUsage('normal');
  const { evictAdapter } = useMemoryOperations();
  const { toast } = useToast();

  const handleEvict = async (adapterId: string) => {
    if (!confirm(`Are you sure you want to evict adapter ${adapterId} from memory?`)) {
      return;
    }

    try {
      await evictAdapter.execute(adapterId);
      toast({
        title: 'Adapter Evicted',
        description: `Adapter ${adapterId} has been evicted from memory`,
      });
      refetch();
    } catch (error) {
      toast({
        title: 'Eviction Failed',
        description: error instanceof Error ? error.message : 'Unknown error',
        variant: 'destructive',
      });
    }
  };

  const memoryUsagePercent = useMemo(() => {
    if (!memoryData) return 0;
    const used = memoryData.total_memory_mb - memoryData.available_memory_mb;
    return (used / memoryData.total_memory_mb) * 100;
  }, [memoryData]);

  const pressureVariant = useMemo(() => {
    if (!memoryData) return 'secondary';
    switch (memoryData.memory_pressure_level) {
      case 'low':
        return 'success';
      case 'medium':
        return 'warning';
      case 'high':
      case 'critical':
        return 'destructive';
      default:
        return 'secondary';
    }
  }, [memoryData]);

  const columns = useMemo<Column<AdapterMemoryInfo>[]>(
    () => [
      {
        id: 'id',
        accessorKey: 'id',
        header: 'Adapter ID',
        cell: ({ row }) => <span className="font-mono text-sm">{row.id}</span>,
      },
      {
        id: 'name',
        accessorKey: 'name',
        header: 'Name',
        cell: ({ row }) => <span className="text-sm">{row.name}</span>,
      },
      {
        id: 'memory_usage_mb',
        accessorKey: 'memory_usage_mb',
        header: 'Memory Usage',
        cell: ({ row }) => `${row.memory_usage_mb.toFixed(2)} MB`,
      },
      {
        id: 'state',
        accessorKey: 'state',
        header: 'State',
        cell: ({ row }) => (
          <Badge variant="outline" className="capitalize">
            {row.state}
          </Badge>
        ),
      },
      {
        id: 'pinned',
        accessorKey: 'pinned',
        header: 'Pinned',
        cell: ({ row }) => (
          <Badge variant={row.pinned ? 'default' : 'secondary'}>
            {row.pinned ? 'Yes' : 'No'}
          </Badge>
        ),
      },
      {
        id: 'actions',
        header: 'Actions',
        cell: ({ row }) => {
          return (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => handleEvict(row.id)}
              disabled={evictAdapter.isLoading || row.pinned}
              title={row.pinned ? 'Cannot evict pinned adapter' : 'Evict adapter from memory'}
            >
              <Trash2 className="h-4 w-4 mr-2" />
              Evict
            </Button>
          );
        },
      },
    ],
    [evictAdapter.isLoading]
  );

  if (isLoading) {
    return (
      <div className="space-y-6">
        <Skeleton className="h-48 w-full" />
        <Skeleton className="h-64 w-full" />
      </div>
    );
  }

  if (!memoryData) {
    return (
      <Card className="border-destructive bg-destructive/10">
        <CardContent className="pt-6">
          <p className="text-destructive">Failed to load memory information</p>
        </CardContent>
      </Card>
    );
  }

  return (
    <div className="space-y-6">
      {/* Memory Overview */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        <Card>
          <CardHeader>
            <CardDescription>Total Memory</CardDescription>
          </CardHeader>
          <CardContent>
            <div className="text-3xl font-bold">
              {(memoryData.total_memory_mb / 1024).toFixed(2)} GB
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardDescription>Available Memory</CardDescription>
          </CardHeader>
          <CardContent>
            <div className="text-3xl font-bold">
              {(memoryData.available_memory_mb / 1024).toFixed(2)} GB
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardDescription>Pressure Level</CardDescription>
          </CardHeader>
          <CardContent>
            <Badge variant={pressureVariant} className="text-lg px-4 py-2">
              {memoryData.memory_pressure_level.toUpperCase()}
            </Badge>
          </CardContent>
        </Card>
      </div>

      {/* Memory Usage Chart */}
      <Card>
        <CardHeader>
          <CardTitle>Memory Usage</CardTitle>
          <CardDescription>
            {memoryUsagePercent.toFixed(1)}% of total memory in use
          </CardDescription>
        </CardHeader>
        <CardContent>
          <Progress value={memoryUsagePercent} className="h-4" />
          <div className="mt-4 grid grid-cols-2 gap-4 text-sm">
            <div>
              <span className="text-muted-foreground">Used:</span>{' '}
              <span className="font-semibold">
                {((memoryData.total_memory_mb - memoryData.available_memory_mb) / 1024).toFixed(2)} GB
              </span>
            </div>
            <div>
              <span className="text-muted-foreground">Free:</span>{' '}
              <span className="font-semibold">
                {(memoryData.available_memory_mb / 1024).toFixed(2)} GB
              </span>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Adapter Memory Table */}
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between">
            <div>
              <CardTitle>Adapters in Memory</CardTitle>
              <CardDescription>
                {memoryData.adapters.length} adapter(s) currently loaded
              </CardDescription>
            </div>
            <Button variant="outline" onClick={() => refetch()}>
              Refresh
            </Button>
          </div>
        </CardHeader>
        <CardContent>
          <DataTable
            columns={columns}
            data={memoryData.adapters}
            getRowId={(row) => row.id}
            isLoading={isLoading}
            globalFilter=""
          />
        </CardContent>
      </Card>
    </div>
  );
}
