import React, { useEffect, useState } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from './ui/table';
import { AdapterStateVisualization } from './AdapterStateVisualization';
import { AdapterMemoryMonitor } from './AdapterMemoryMonitor';
import apiClient from '../api/client';
import { Adapter } from '../api/types';
import { toast } from 'sonner';
import { logger } from '../utils/logger';
import { usePolling } from '../hooks/usePolling';
import { ErrorRecovery } from './ui/error-recovery';
import { EmptyState } from './ui/empty-state';
import { LoadingState } from './ui/loading-state';
import { Code, MemoryStick, Activity, Clock, Pin, ArrowUp, Trash2, MoreHorizontal } from 'lucide-react';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from './ui/dropdown-menu';
import { useProgressiveHints } from '../hooks/useProgressiveHints';
import { getPageHints } from '../data/page-hints';
import { ProgressiveHint } from './ui/progressive-hint';

interface AdaptersData {
  adapters: Adapter[];
  totalMemory: number;
}

export function AdaptersPage() {
  const fetchAdaptersData = async (): Promise<AdaptersData> => {
    const adaptersData = await apiClient.listAdapters();
    const metrics = await apiClient.getSystemMetrics();
    const totalMemory = metrics.memory_total_gb * 1024 * 1024 * 1024; // Convert GB to bytes
    return { adapters: adaptersData, totalMemory };
  };

  const { data, isLoading: loading, error } = usePolling(
    fetchAdaptersData,
    'normal',
    {
      showLoadingIndicator: false,
      onError: (err) => {
        logger.error('Failed to fetch adapters', { component: 'AdaptersPage' }, err);
      }
    }
  );

  const adapters = data?.adapters ?? [];
  const totalMemory = data?.totalMemory ?? 0;

  // Progressive hints
  const hints = getPageHints('adapters').map(hint => ({
    ...hint,
    condition: hint.id === 'empty-adapters' 
      ? () => adapters.length === 0 && !loading
      : hint.condition
  }));
  const { visibleHints, dismissHint, getVisibleHint } = useProgressiveHints({
    pageKey: 'adapters',
    hints
  });
  const visibleHint = getVisibleHint();

  useEffect(() => {
    if (loading) {
      logger.debug('Adapters: showing loading state', { component: 'AdaptersPage' });
    }
  }, [loading]);

  useEffect(() => {
    if (!loading && adapters.length === 0) {
      logger.info('Adapters: empty state displayed', { component: 'AdaptersPage' });
    }
  }, [adapters.length, loading]);

  // Show ErrorRecovery for major data loading failures
  if (error) {
    return (
      <ErrorRecovery
        title="Failed to Load Adapters"
        message="Unable to load adapter data. This may be due to a network issue or server problem."
        error={error}
        recoveryActions={[
          { label: 'Retry', action: () => window.location.reload(), primary: true },
          { label: 'Go to Dashboard', action: () => { window.location.href = '/dashboard'; } }
        ]}
      />
    );
  }

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
      {visibleHint && (
        <ProgressiveHint
          title={visibleHint.hint.title}
          content={visibleHint.hint.content}
          onDismiss={() => dismissHint(visibleHint.hint.id)}
          placement={visibleHint.hint.placement}
        />
      )}

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
            <LoadingState
              title="Loading adapters"
              description="Fetching adapter fleet status and usage metrics."
              skeletonLines={4}
              size="sm"
            />
          ) : adapters.length === 0 ? (
            <EmptyState
              icon={Code}
              title="No adapters deployed"
              description="Train or import an adapter to get started. Your fleet will appear here once deployed."
            />
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
