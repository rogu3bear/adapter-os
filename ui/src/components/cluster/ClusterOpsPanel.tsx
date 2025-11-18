import React, { useState } from 'react';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '../ui/tabs';
import { Button } from '../ui/button';
import { RefreshCw, Server, Activity, Network } from 'lucide-react';
import { ClusterHealthWidget } from './ClusterHealthWidget';
import { NodeTable } from './NodeTable';
import { WorkerTable } from './WorkerTable';
import apiClient from '../../api/client';
import { logger, toError } from '../../utils/logger';
import { usePolling } from '../../hooks/usePolling';
import { LoadingState } from '../ui/loading-state';
import { LastUpdated } from '../ui/last-updated';
import type { Node, WorkerResponse, ClusterStatus } from '@/api/types';

interface ClusterOpsPanelProps {
  tenantId?: string;
  onToolbarChange?: (actions: React.ReactNode) => void;
}

export function ClusterOpsPanel({ tenantId, onToolbarChange }: ClusterOpsPanelProps) {
  const [activeTab, setActiveTab] = useState('cluster-health');

  const fetchData = async () => {
    const [clusterStatus, nodes, workers] = await Promise.all([
      apiClient.getClusterStatus().catch((err) => {
        logger.error('Failed to fetch cluster status', { component: 'ClusterOpsPanel' }, toError(err));
        return null;
      }),
      apiClient.listNodes().catch((err) => {
        logger.error('Failed to fetch nodes', { component: 'ClusterOpsPanel' }, toError(err));
        return [];
      }),
      apiClient.listWorkers().catch((err) => {
        logger.error('Failed to fetch workers', { component: 'ClusterOpsPanel' }, toError(err));
        return [];
      }),
    ]);

    return {
      clusterStatus,
      nodes,
      workers,
    };
  };

  const {
    data,
    isLoading,
    lastUpdated,
    error: pollingError,
    refetch: refreshData,
  } = usePolling(
    fetchData,
    'normal',
    {
      showLoadingIndicator: false,
      onError: (err) => {
        logger.error('Polling error in ClusterOpsPanel', { component: 'ClusterOpsPanel' }, toError(err));
      }
    }
  );

  // Update toolbar with refresh button
  React.useEffect(() => {
    if (onToolbarChange) {
      onToolbarChange(
        <div className="flex items-center gap-2">
          <LastUpdated timestamp={lastUpdated} />
          <Button
            variant="outline"
            size="sm"
            onClick={refreshData}
            disabled={isLoading}
          >
            <RefreshCw className={`h-4 w-4 mr-2 ${isLoading ? 'animate-spin' : ''}`} />
            Refresh
          </Button>
        </div>
      );
    }
  }, [onToolbarChange, lastUpdated, refreshData, isLoading]);

  if (isLoading && !data) {
    return <LoadingState message="Loading cluster data..." />;
  }

  if (pollingError) {
    return (
      <div className="p-8 text-center">
        <p className="text-red-500">Error loading cluster data: {pollingError.message}</p>
        <Button onClick={refreshData} className="mt-4">
          <RefreshCw className="h-4 w-4 mr-2" />
          Retry
        </Button>
      </div>
    );
  }

  const { clusterStatus, nodes, workers } = data || {
    clusterStatus: null,
    nodes: [],
    workers: [],
  };

  return (
    <div className="space-y-6">
      <Tabs value={activeTab} onValueChange={setActiveTab} className="w-full">
        <TabsList className="grid w-full grid-cols-3">
          <TabsTrigger value="cluster-health" className="flex items-center gap-2">
            <Network className="h-4 w-4" />
            Cluster Health
          </TabsTrigger>
          <TabsTrigger value="nodes" className="flex items-center gap-2">
            <Server className="h-4 w-4" />
            Nodes ({nodes.length})
          </TabsTrigger>
          <TabsTrigger value="workers" className="flex items-center gap-2">
            <Activity className="h-4 w-4" />
            Workers ({workers.length})
          </TabsTrigger>
        </TabsList>

        <TabsContent value="cluster-health" className="mt-6">
          <ClusterHealthWidget
            clusterStatus={clusterStatus}
            nodes={nodes}
            workers={workers}
            onRefresh={refreshData}
          />
        </TabsContent>

        <TabsContent value="nodes" className="mt-6">
          <NodeTable
            nodes={nodes}
            onRefresh={refreshData}
          />
        </TabsContent>

        <TabsContent value="workers" className="mt-6">
          <WorkerTable
            workers={workers}
            nodes={nodes}
            onRefresh={refreshData}
          />
        </TabsContent>
      </Tabs>
    </div>
  );
}
