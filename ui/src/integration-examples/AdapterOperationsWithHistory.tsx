//! Adapter Operations with History Integration
//!
//! Example implementation showing how to integrate action history tracking
//! with adapter management operations.

import React, { useCallback, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { AlertCircle, CheckCircle } from 'lucide-react';
import useEnhancedActionHistory from '@/hooks/useEnhancedActionHistory';
import { ActionHistoryItem } from '@/types/history';

interface Adapter {
  id: string;
  name: string;
  rank: number;
  status: 'loaded' | 'unloaded' | 'error';
}

export function AdapterOperationsWithHistory() {
  const [adapters, setAdapters] = useState<Adapter[]>([]);
  const [loading, setLoading] = useState(false);

  const history = useEnhancedActionHistory({
    maxSize: 500,
    persistToLocalStorage: true,
    autoCleanup: true,
  });

  // Create adapter with history tracking
  const handleCreateAdapter = useCallback(async (name: string, rank: number) => {
    const startTime = Date.now();
    const operationId = `${Date.now()}-${Math.random()}`;

    try {
      setLoading(true);

      // Simulate API call
      const adapterId = `adapter-${Date.now()}`;
      const newAdapter: Adapter = { id: adapterId, name, rank, status: 'unloaded' };

      // Track action before operation
      let actionId = '';
      history.addAction({
        action: 'create',
        resource: 'adapter',
        status: 'pending',
        description: `Creating adapter: ${name} (rank=${rank})`,
        metadata: {
          adapterId,
          name,
          rank,
          operationId,
        },
        undo: async () => {
          // Remove the adapter
          setAdapters((prev) => prev.filter((a) => a.id !== adapterId));
          console.log(`Undo: Adapter ${adapterId} removed`);
        },
        redo: async () => {
          // Re-add the adapter
          setAdapters((prev) => [...prev, newAdapter]);
          console.log(`Redo: Adapter ${adapterId} created`);
        },
        tags: ['adapter-creation', name],
        userId: 'user-123',
        tenantId: 'default',
      });

      // Simulate some processing
      await new Promise((resolve) => setTimeout(resolve, 500));

      // Add adapter to state
      setAdapters((prev) => [...prev, newAdapter]);

      // Update action to success
      const duration = Date.now() - startTime;
      history.addAction({
        action: 'create',
        resource: 'adapter',
        status: 'success',
        description: `Created adapter: ${name} (ID: ${adapterId})`,
        duration,
        metadata: {
          adapterId,
          name,
          rank,
          operationId,
        },
        undo: async () => {
          setAdapters((prev) => prev.filter((a) => a.id !== adapterId));
        },
        redo: async () => {
          setAdapters((prev) => [...prev, newAdapter]);
        },
        tags: ['adapter-creation', name],
        userId: 'user-123',
        tenantId: 'default',
      });

      console.log(`Created adapter: ${name}`);
    } catch (error) {
      const duration = Date.now() - startTime;
      const errorMessage = error instanceof Error ? error.message : 'Unknown error';

      // Track failed action
      history.addAction({
        action: 'create',
        resource: 'adapter',
        status: 'failed',
        description: `Failed to create adapter: ${name}`,
        duration,
        errorMessage,
        undo: async () => {},
        metadata: {
          name,
          rank,
          operationId,
          error: errorMessage,
        },
        tags: ['adapter-creation', 'failed'],
        userId: 'user-123',
        tenantId: 'default',
      });

      console.error('Failed to create adapter:', error);
    } finally {
      setLoading(false);
    }
  }, [history]);

  // Load adapter with history tracking
  const handleLoadAdapter = useCallback(async (adapterId: string) => {
    const startTime = Date.now();

    try {
      setAdapters((prev) =>
        prev.map((a) =>
          a.id === adapterId ? { ...a, status: 'loaded' as const } : a
        )
      );

      const duration = Date.now() - startTime;

      history.addAction({
        action: 'load',
        resource: 'adapter',
        status: 'success',
        description: `Loaded adapter: ${adapterId}`,
        duration,
        metadata: { adapterId },
        undo: async () => {
          setAdapters((prev) =>
            prev.map((a) =>
              a.id === adapterId ? { ...a, status: 'unloaded' as const } : a
            )
          );
        },
        redo: async () => {
          setAdapters((prev) =>
            prev.map((a) =>
              a.id === adapterId ? { ...a, status: 'loaded' as const } : a
            )
          );
        },
        tags: ['adapter-load'],
        userId: 'user-123',
        tenantId: 'default',
      });
    } catch (error) {
      const duration = Date.now() - startTime;

      history.addAction({
        action: 'load',
        resource: 'adapter',
        status: 'failed',
        description: `Failed to load adapter: ${adapterId}`,
        duration,
        errorMessage: error instanceof Error ? error.message : 'Unknown error',
        undo: async () => {},
        metadata: { adapterId },
        tags: ['adapter-load', 'failed'],
        userId: 'user-123',
        tenantId: 'default',
      });
    }
  }, [history]);

  // Delete adapter with history tracking
  const handleDeleteAdapter = useCallback(async (adapterId: string) => {
    const startTime = Date.now();
    const adapterToDelete = adapters.find((a) => a.id === adapterId);

    if (!adapterToDelete) return;

    try {
      setAdapters((prev) => prev.filter((a) => a.id !== adapterId));

      const duration = Date.now() - startTime;

      history.addAction({
        action: 'delete',
        resource: 'adapter',
        status: 'success',
        description: `Deleted adapter: ${adapterToDelete.name}`,
        duration,
        metadata: { adapterId, adapterName: adapterToDelete.name },
        undo: async () => {
          setAdapters((prev) => [...prev, adapterToDelete]);
        },
        redo: async () => {
          setAdapters((prev) => prev.filter((a) => a.id !== adapterId));
        },
        tags: ['adapter-delete'],
        userId: 'user-123',
        tenantId: 'default',
      });
    } catch (error) {
      const duration = Date.now() - startTime;

      history.addAction({
        action: 'delete',
        resource: 'adapter',
        status: 'failed',
        description: `Failed to delete adapter: ${adapterToDelete.name}`,
        duration,
        errorMessage: error instanceof Error ? error.message : 'Unknown error',
        undo: async () => {},
        metadata: { adapterId },
        tags: ['adapter-delete', 'failed'],
        userId: 'user-123',
        tenantId: 'default',
      });
    }
  }, [adapters, history]);

  return (
    <div className="space-y-6 p-4">
      {/* Header with Undo/Redo */}
      <div className="flex items-center justify-between">
        <h2 className="text-2xl font-bold">Adapter Operations</h2>
        <div className="flex gap-2">
          <Button
            variant="outline"
            onClick={history.undo}
            disabled={!history.canUndo}
          >
            Undo
          </Button>
          <Button
            variant="outline"
            onClick={history.redo}
            disabled={!history.canRedo}
          >
            Redo
          </Button>
          <Button
            variant="outline"
            size="sm"
          >
            History ({history.historyCount})
          </Button>
        </div>
      </div>

      {/* Create Adapter Form */}
      <Card className="p-4 space-y-4">
        <h3 className="font-semibold">Create Adapter</h3>
        <div className="space-y-2">
          <Input placeholder="Adapter name" id="adapter-name" />
          <Input
            type="number"
            placeholder="LoRA rank"
            defaultValue="16"
            id="adapter-rank"
          />
          <Button
            onClick={() => {
              const name = (document.getElementById('adapter-name') as HTMLInputElement)?.value;
              const rank = parseInt((document.getElementById('adapter-rank') as HTMLInputElement)?.value || '16');
              if (name) {
                handleCreateAdapter(name, rank);
              }
            }}
            disabled={loading}
          >
            {loading ? 'Creating...' : 'Create'}
          </Button>
        </div>
      </Card>

      {/* Adapters List */}
      <Card className="p-4">
        <h3 className="font-semibold mb-4">Adapters</h3>
        {adapters.length === 0 ? (
          <p className="text-muted-foreground">No adapters yet</p>
        ) : (
          <div className="space-y-2">
            {adapters.map((adapter) => (
              <div
                key={adapter.id}
                className="flex items-center justify-between p-3 border rounded-lg"
              >
                <div className="flex items-center gap-3">
                  {adapter.status === 'loaded' ? (
                    <CheckCircle className="h-4 w-4 text-green-600" />
                  ) : (
                    <AlertCircle className="h-4 w-4 text-gray-400" />
                  )}
                  <div>
                    <p className="font-medium">{adapter.name}</p>
                    <p className="text-sm text-muted-foreground">
                      Rank: {adapter.rank} | Status: {adapter.status}
                    </p>
                  </div>
                </div>
                <div className="flex gap-2">
                  {adapter.status === 'unloaded' && (
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={() => handleLoadAdapter(adapter.id)}
                    >
                      Load
                    </Button>
                  )}
                  <Button
                    size="sm"
                    variant="destructive"
                    onClick={() => handleDeleteAdapter(adapter.id)}
                  >
                    Delete
                  </Button>
                </div>
              </div>
            ))}
          </div>
        )}
      </Card>

      {/* History Stats */}
      <Card className="p-4">
        <h3 className="font-semibold mb-4">History Statistics</h3>
        <div className="grid grid-cols-2 gap-4 md:grid-cols-4">
          <div>
            <p className="text-sm text-muted-foreground">Total Actions</p>
            <p className="text-2xl font-bold">{history.stats.totalActions}</p>
          </div>
          <div>
            <p className="text-sm text-muted-foreground">Success Rate</p>
            <p className="text-2xl font-bold">{history.stats.successRate.toFixed(1)}%</p>
          </div>
          <div>
            <p className="text-sm text-muted-foreground">Creates</p>
            <p className="text-2xl font-bold">{history.stats.actionsByType.create}</p>
          </div>
          <div>
            <p className="text-sm text-muted-foreground">Deletes</p>
            <p className="text-2xl font-bold">{history.stats.actionsByType.delete}</p>
          </div>
        </div>
      </Card>

      {/* Recent Actions */}
      {history.stats.recentActions.length > 0 && (
        <Card className="p-4">
          <h3 className="font-semibold mb-4">Recent Actions</h3>
          <div className="space-y-2">
            {history.stats.recentActions.slice(0, 5).map((action) => (
              <div key={action.id} className="flex items-start justify-between p-2 border-b last:border-b-0">
                <div>
                  <p className="text-sm font-medium">{action.description}</p>
                  <p className="text-xs text-muted-foreground">
                    {new Date(action.timestamp).toLocaleTimeString()}
                  </p>
                </div>
                <span className={`text-xs font-semibold ${
                  action.status === 'success' ? 'text-green-600' : 'text-red-600'
                }`}>
                  {action.status}
                </span>
              </div>
            ))}
          </div>
        </Card>
      )}
    </div>
  );
}

export default AdapterOperationsWithHistory;
