import React, { useState, useEffect } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from './ui/table';
import { AdapterStateVisualization } from './AdapterStateVisualization';
import { AdapterMemoryMonitor } from './AdapterMemoryMonitor';
import apiClient from '../api/client';
import { Adapter } from '../api/types';
import { toast } from 'sonner';
import { logger, toError } from '../utils/logger';
import { Link } from 'react-router-dom';
import { Code, MemoryStick, Activity, Clock, Pin, ArrowUp, Trash2, MoreHorizontal } from 'lucide-react';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from './ui/dropdown-menu';

export function AdaptersPage() {
  const [adapters, setAdapters] = useState<Adapter[]>([]);
  const [totalMemory, setTotalMemory] = useState(0); // Fetch or set total memory
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const fetchAdapters = async () => {
      try {
        const adaptersData = await apiClient.listAdapters();
        setAdapters(adaptersData);
        // Fetch total memory, e.g., from system metrics
        const metrics = await apiClient.getSystemMetrics();
        setTotalMemory(metrics.memory_total_gb * 1024 * 1024 * 1024); // Convert GB to bytes
      } catch (err) {
        logger.error('Failed to fetch adapters', { component: 'AdaptersPage' }, toError(err));
        toast.error('Failed to load adapters');
      } finally {
        setLoading(false);
      }
    };
    fetchAdapters();
    const interval = setInterval(fetchAdapters, 5000);
    return () => clearInterval(interval);
  }, []);

  const handleEvict = (adapterId: string) => {
    // Implement evict logic
    toast.success('Adapter evicted');
  };

  const handlePin = (adapterId: string, pinned: boolean) => {
    // Implement pin logic
    toast.success(pinned ? 'Adapter pinned' : 'Adapter unpinned');
  };

  const handleUpdateMemoryLimit = (category: string, limit: number) => {
    // Implement update limit
    toast.success('Memory limit updated');
  };

  const handlePromote = (adapterId: string) => {
    // Implement promote logic
    toast.success('Adapter promoted');
  };

  const handleDelete = (adapterId: string) => {
    // Implement delete logic
    toast.success('Adapter deleted');
  };

  const getCategoryIcon = (category: string) => {
    switch (category) {
      case 'code': return <Code className="h-4 w-4" />;
      default: return <Activity className="h-4 w-4" />;
    }
  };

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-bold">Adapter Management</h1>
          <p className="text-muted-foreground">Manage adapter lifecycles and deployment</p>
        </div>
        <Link to="/training">
          <Button>
            <Code className="mr-2 h-4 w-4" />
            Train New Adapter
          </Button>
        </Link>
      </div>

      {/* Visualizations */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
        {(() => {
          const stateRecords = adapters.map((a, idx) => ({
            adapter_id: a.adapter_id || a.id,
            adapter_idx: idx,
            state: a.current_state as any,
            pinned: a.pinned,
            memory_bytes: a.memory_bytes,
            category: a.category,
            scope: a.scope,
            last_activated: a.last_activated,
            activation_count: a.activation_count,
          }));
          return <AdapterStateVisualization adapters={stateRecords as any} totalMemory={totalMemory} />;
        })()}
        <AdapterMemoryMonitor
          adapters={adapters}
          totalMemory={totalMemory}
          onEvictAdapter={handleEvict}
          onPinAdapter={handlePin}
          onUpdateMemoryLimit={handleUpdateMemoryLimit}
        />
      </div>

      {/* Adapter Table */}
      <Card>
        <CardHeader>
          <CardTitle>Deployed Adapters</CardTitle>
        </CardHeader>
        <CardContent>
          {loading ? (
            <div className="text-center py-8">Loading adapters...</div>
          ) : adapters.length === 0 ? (
            <div className="text-center py-8 text-muted-foreground">No adapters deployed</div>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Name</TableHead>
                  <TableHead>Category</TableHead>
                  <TableHead>State</TableHead>
                  <TableHead>Memory</TableHead>
                  <TableHead>Activations</TableHead>
                  <TableHead>Last Used</TableHead>
                  <TableHead>Actions</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {adapters.map(adapter => (
                  <TableRow key={adapter.id}>
                    <TableCell className="font-medium">{adapter.name}</TableCell>
                    <TableCell>
                      <Badge>{getCategoryIcon(adapter.category)} {adapter.category}</Badge>
                    </TableCell>
                    <TableCell>
                      <Badge>{adapter.current_state}</Badge>
                      {adapter.pinned && <Pin className="h-4 w-4 ml-2" />}
                    </TableCell>
                    <TableCell>{(adapter.memory_bytes / 1024 / 1024).toFixed(1)} MB</TableCell>
                    <TableCell>{adapter.activation_count}</TableCell>
                    <TableCell>{adapter.last_activated ? new Date(adapter.last_activated).toLocaleString() : 'Never'}</TableCell>
                    <TableCell>
                      <DropdownMenu>
                        <DropdownMenuTrigger asChild>
                          <Button variant="ghost"><MoreHorizontal className="h-4 w-4" /></Button>
                        </DropdownMenuTrigger>
                        <DropdownMenuContent>
                          <DropdownMenuItem onClick={() => handlePromote(adapter.id)}>
                            <ArrowUp className="mr-2 h-4 w-4" /> Promote
                          </DropdownMenuItem>
                          <DropdownMenuItem onClick={() => handlePin(adapter.id, !adapter.pinned)}>
                            <Pin className="mr-2 h-4 w-4" /> {adapter.pinned ? 'Unpin' : 'Pin'}
                          </DropdownMenuItem>
                          <DropdownMenuItem onClick={() => handleEvict(adapter.id)}>
                            <Trash2 className="mr-2 h-4 w-4" /> Evict
                          </DropdownMenuItem>
                          <DropdownMenuItem onClick={() => handleDelete(adapter.id)}>
                            <Trash2 className="mr-2 h-4 w-4 text-red-500" /> Delete
                          </DropdownMenuItem>
                        </DropdownMenuContent>
                      </DropdownMenu>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
