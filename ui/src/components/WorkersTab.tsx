
// 【ui/src/components/WorkersTab.tsx§64-69】 - Replace manual polling with standardized hook

>
import React, { useState, useEffect } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from './ui/table';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';

import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from './ui/dialog';

import { Dialog, DialogContent } from './ui/dialog';
>
import {
  Activity,
  Play,
  Square,
  RefreshCw,
  Filter,
  Eye,
  Trash2,
  AlertTriangle,
} from 'lucide-react';
import { toast } from 'sonner';
import apiClient from '../api/client';
import { WorkerResponse, Node, Plan } from '../api/types';
import { SpawnWorkerModal } from './SpawnWorkerModal';
import { ProcessDebugger } from './ProcessDebugger';

import { logger, toError } from '../utils/logger';
import { usePolling } from '../hooks/usePolling';
import { LastUpdated } from './ui/last-updated';
import { ErrorRecovery, ErrorRecoveryTemplates } from './ui/error-recovery';

>

interface WorkersTabProps {
  selectedTenant: string;
}

export function WorkersTab({ selectedTenant }: WorkersTabProps) {
  const [workers, setWorkers] = useState<WorkerResponse[]>([]);
  const [filteredWorkers, setFilteredWorkers] = useState<WorkerResponse[]>([]);

  const [showSpawnModal, setShowSpawnModal] = useState(false);
  const [debugWorkerId, setDebugWorkerId] = useState<string | null>(null);
  const [hotSwapOpen, setHotSwapOpen] = useState(false);
  const [hotSwapAdd, setHotSwapAdd] = useState('');
  const [hotSwapRemove, setHotSwapRemove] = useState('');
  const [error, setError] = useState<Error | null>(null);

  // Filters
  const [filterTenant, setFilterTenant] = useState<string>('');
  const [filterNode, setFilterNode] = useState<string>('');
  const [filterStatus, setFilterStatus] = useState<string>('all');

  // 【ui/src/hooks/usePolling.ts】 - Standardized polling hook for workers
  const fetchWorkersData = async () => {
    const data = await apiClient.listWorkers(selectedTenant || undefined);
    return data;
  };

  const {
    data: workersData,
    isLoading: loading,
    lastUpdated,
    error: pollingError,
    refetch: refreshWorkers
  } = usePolling(
    fetchWorkersData,
    'normal', // Background updates for workers
    {
      showLoadingIndicator: true,
      onError: (err) => {
        const error = err instanceof Error ? err : new Error('Failed to load workers');
        setError(error);
        logger.error('Failed to fetch workers', {
          component: 'WorkersTab',
          operation: 'polling',
          tenantFilter: selectedTenant || 'all',
        }, err);
      }
    }
  );

  const fetchWorkers = async () => {
    await refreshWorkers();
  };

  // Update workers when polling data arrives
  useEffect(() => {
    if (!workersData) return;
    setWorkers(workersData);
    setFilteredWorkers(workersData);
    setError(null);
  }, [workersData]);

  const [loading, setLoading] = useState(true);
  const [showSpawnModal, setShowSpawnModal] = useState(false);
  const [debugWorkerId, setDebugWorkerId] = useState<string | null>(null);
  
  // Filters
  const [filterTenant, setFilterTenant] = useState<string>('');
  const [filterNode, setFilterNode] = useState<string>('');
  const [filterStatus, setFilterStatus] = useState<string>('');

  const fetchWorkers = async () => {
    try {
      setLoading(true);
      const data = await apiClient.listWorkers(selectedTenant || undefined);
      setWorkers(data);
      setFilteredWorkers(data);
    } catch (error) {
      console.error('Failed to fetch workers:', error);
      toast.error('Failed to load workers');
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchWorkers();
    // Poll every 1 second for instant updates
    const interval = setInterval(fetchWorkers, 1000);
    return () => clearInterval(interval);
  }, [selectedTenant]);
>

  useEffect(() => {
    // Apply filters
    let filtered = workers;

    if (filterTenant) {
      filtered = filtered.filter((w) => w.tenant_id.includes(filterTenant));
    }
    if (filterNode) {
      filtered = filtered.filter((w) => w.node_id.includes(filterNode));
    }

    if (filterStatus && filterStatus !== 'all') {

    if (filterStatus) {
>
      filtered = filtered.filter((w) => w.status === filterStatus);
    }

    setFilteredWorkers(filtered);
  }, [workers, filterTenant, filterNode, filterStatus]);

  const handleStopWorker = async (workerId: string, force: boolean = false) => {
    try {
      await apiClient.stopWorker(workerId, force);
      toast.success(`Worker ${workerId} stopped`);

      await refreshWorkers();
    } catch (error) {
      logger.error('Failed to stop worker', {
        component: 'WorkersTab',
        operation: 'stopWorker',
        workerId,
        force,
      }, toError(error));

      await fetchWorkers();
    } catch (error) {
      console.error('Failed to stop worker:', error);
>
      toast.error(error instanceof Error ? error.message : 'Failed to stop worker');
    }
  };

  const getStatusBadgeVariant = (status: string) => {
    switch (status) {
      case 'ready':
        return 'default';
      case 'starting':
        return 'secondary';
      case 'busy':
        return 'default';
      case 'stopping':
        return 'secondary';
      case 'stopped':
      case 'error':
        return 'destructive';
      default:
        return 'secondary';
    }
  };

  const getStatusIcon = (status: string) => {
    switch (status) {
      case 'ready':
      case 'busy':
        return <Activity className="h-3 w-3 animate-pulse" />;
      case 'starting':
        return <Play className="h-3 w-3 animate-spin" />;
      case 'stopping':
      case 'stopped':
        return <Square className="h-3 w-3" />;
      case 'error':
        return <AlertTriangle className="h-3 w-3" />;
      default:
        return null;
    }
  };


  if (error) {
    return (
      <ErrorRecovery
        title="Workers Tab Error"
        message={error.message}
        recoveryActions={[
          { label: 'Retry', action: () => refreshWorkers() },
          { label: 'Clear Filters', action: () => {
            setFilterTenant('');
            setFilterNode('');
            setFilterStatus('');
            setError(null);
          }}
        ]}
      />
    );
  }


>
  if (loading && workers.length === 0) {
    return <div className="text-center p-8">Loading workers...</div>;
  }

  return (
    <div className="space-y-4">
      {/* Header */}
      <div className="flex-between">
        <div>
          <h2 className="text-2xl font-bold">Workers</h2>
          <p className="text-sm text-muted-foreground">
            Manage worker processes across compute nodes
          </p>

          {lastUpdated && <LastUpdated timestamp={lastUpdated} className="mt-1" />}
        </div>
        <div className="flex gap-2">
          <Button variant="outline" size="sm" onClick={() => refreshWorkers()}>

        </div>
        <div className="flex gap-2">
          <Button variant="outline" size="sm" onClick={fetchWorkers}>
>
            <RefreshCw className="h-4 w-4 mr-2" />
            Refresh
          </Button>
          <Button onClick={() => setShowSpawnModal(true)}>
            <Play className="h-4 w-4 mr-2" />
            Spawn Worker
          </Button>

          <Button variant="outline" onClick={() => setHotSwapOpen(true)}>
            Hot-swap
          </Button>

>
        </div>
      </div>

      {/* Filters */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Filter className="h-4 w-4" />
            Filters
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            <div className="space-y-2">
              <Label htmlFor="filterTenant">Tenant</Label>
              <Input
                id="filterTenant"
                placeholder="Filter by tenant..."
                value={filterTenant}
                onChange={(e) => setFilterTenant(e.target.value)}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="filterNode">Node</Label>
              <Input
                id="filterNode"
                placeholder="Filter by node..."
                value={filterNode}
                onChange={(e) => setFilterNode(e.target.value)}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="filterStatus">Status</Label>
              <Select value={filterStatus} onValueChange={setFilterStatus}>
                <SelectTrigger id="filterStatus">
                  <SelectValue placeholder="All statuses" />
                </SelectTrigger>
                <SelectContent>

                  <SelectItem value="all">All</SelectItem>

                  <SelectItem value="">All</SelectItem>
>
                  <SelectItem value="starting">Starting</SelectItem>
                  <SelectItem value="ready">Ready</SelectItem>
                  <SelectItem value="busy">Busy</SelectItem>
                  <SelectItem value="stopping">Stopping</SelectItem>
                  <SelectItem value="stopped">Stopped</SelectItem>
                  <SelectItem value="error">Error</SelectItem>
                </SelectContent>
              </Select>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Workers Table */}
      <Card>
        <CardHeader>
          <CardTitle>
            Active Workers ({filteredWorkers.length})
          </CardTitle>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Worker ID</TableHead>
                <TableHead>Tenant</TableHead>
                <TableHead>Node</TableHead>
                <TableHead>Plan</TableHead>
                <TableHead>Status</TableHead>
                <TableHead>PID</TableHead>
                <TableHead>Started</TableHead>
                <TableHead>Last Seen</TableHead>
                <TableHead className="w-[100px]">Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {filteredWorkers.map((worker) => (
                <TableRow key={worker.id}>
                  <TableCell className="font-mono text-xs">
                    {worker.id.substring(0, 8)}...
                  </TableCell>
                  <TableCell>{worker.tenant_id}</TableCell>
                  <TableCell className="font-mono text-xs">
                    {worker.node_id.substring(0, 8)}...
                  </TableCell>
                  <TableCell className="font-mono text-xs">
                    {worker.plan_id.substring(0, 8)}...
                  </TableCell>
                  <TableCell>
                    <Badge variant={getStatusBadgeVariant(worker.status) as any} className="gap-1">
                      {getStatusIcon(worker.status)}
                      {worker.status}
                    </Badge>
                  </TableCell>
                  <TableCell>{worker.pid || 'N/A'}</TableCell>
                  <TableCell className="text-xs">
                    {new Date(worker.started_at).toLocaleString()}
                  </TableCell>
                  <TableCell className="text-xs">
                    {worker.last_seen_at ? new Date(worker.last_seen_at).toLocaleString() : 'Never'}
                  </TableCell>
                  <TableCell>
                    <div className="flex gap-1">
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => setDebugWorkerId(worker.id)}
                        title="Debug Worker"
                      >
                        <Eye className="h-4 w-4" />
                      </Button>
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => handleStopWorker(worker.id, false)}
                        disabled={worker.status === 'stopped' || worker.status === 'stopping'}
                        title="Stop Worker"
                      >
                        <Square className="h-4 w-4" />
                      </Button>
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => handleStopWorker(worker.id, true)}
                        disabled={worker.status === 'stopped'}
                        title="Force Kill"
                        className="text-destructive hover:text-destructive"
                      >
                        <Trash2 className="h-4 w-4" />
                      </Button>
                    </div>
                  </TableCell>
                </TableRow>
              ))}
              {filteredWorkers.length === 0 && (
                <TableRow>
                  <TableCell colSpan={9} className="text-center text-muted-foreground">
                    {workers.length === 0 ? 'No workers found' : 'No workers match the current filters'}
                  </TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
        </CardContent>
      </Card>

      {/* Spawn Worker Modal */}
      <SpawnWorkerModal
        open={showSpawnModal}
        onOpenChange={setShowSpawnModal}
        selectedTenant={selectedTenant}
        onSuccess={fetchWorkers}
      />


      {/* Hot-swap Dialog */}
      <Dialog open={hotSwapOpen} onOpenChange={setHotSwapOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Hot-swap Adapters</DialogTitle>
          </DialogHeader>
          <div className="space-y-3">
            <div>
              <label className="font-medium text-sm mb-1">Add (comma-separated adapter IDs)</label>
              <Input value={hotSwapAdd} onChange={(e) => setHotSwapAdd(e.target.value)} placeholder="adapter_a,adapter_b" />
            </div>
            <div>
              <label className="font-medium text-sm mb-1">Remove (comma-separated adapter IDs)</label>
              <Input value={hotSwapRemove} onChange={(e) => setHotSwapRemove(e.target.value)} placeholder="adapter_x,adapter_y" />
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setHotSwapOpen(false)}>Cancel</Button>
            <Button onClick={async () => {
              try {
                const add = hotSwapAdd.split(',').map(s => s.trim()).filter(Boolean);
                const remove = hotSwapRemove.split(',').map(s => s.trim()).filter(Boolean);
                await apiClient.swapAdapters(add, remove, true);
                setHotSwapOpen(false);
                setHotSwapAdd('');
                setHotSwapRemove('');
                toast.success('Hot-swap committed');
              } catch (err) {
                toast.error('Hot-swap failed');
              }
            }}>Commit</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>


>
      {/* Process Debugger Modal */}
      {debugWorkerId && (
        <Dialog open={!!debugWorkerId} onOpenChange={() => setDebugWorkerId(null)}>
          <DialogContent className="max-w-6xl max-h-[90vh] overflow-hidden">
            <ProcessDebugger
              workerId={debugWorkerId}
              workerName={`Worker ${debugWorkerId.substring(0, 8)}`}
              onClose={() => setDebugWorkerId(null)}
            />
          </DialogContent>
        </Dialog>
      )}
    </div>
  );
}




>
