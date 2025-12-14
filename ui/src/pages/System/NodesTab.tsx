import { useState, useCallback } from 'react';
import FeatureLayout from '@/layout/FeatureLayout';
import { DensityProvider } from '@/contexts/DensityContext';
import { useNodes } from '@/hooks/system/useSystemMetrics';
import { usePolling } from '@/hooks/realtime/usePolling';
import NodeTable from './NodeTable';
import NodeDetailModal from './NodeDetailModal';
import { Card, CardContent } from '@/components/ui/card';
import { PeerSyncStatusCard } from '@/components/federation/PeerSyncStatusCard';
import { derivePeerSyncInfoList } from '@/utils/peerSync';
import { apiClient } from '@/api/client';
import { PeerListResponse } from '@/api/federation-types';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Info } from 'lucide-react';

export default function NodesTab() {
  const { nodes, isLoading, error, refetch } = useNodes('normal');
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);

  // Fetch peer sync status
  const fetchPeers = useCallback(async () => {
    return await apiClient.getFederationPeers();
  }, []);

  const {
    data: peersData,
    isLoading: peersLoading,
    error: peersError,
  } = usePolling<PeerListResponse>(
    fetchPeers,
    'normal', // 10s polling
    {
      enabled: true,
      operationName: 'fetchFederationPeersInNodes',
    }
  );

  if (error) {
    return (
      <DensityProvider pageKey="system-nodes">
        <FeatureLayout
          title="Nodes"
          description="Manage cluster nodes and federation sync status"
          maxWidth="xl"
        >
          <Card className="border-destructive bg-destructive/10">
            <CardContent className="pt-6">
              <p className="text-destructive">Failed to load nodes: {error.message}</p>
            </CardContent>
          </Card>
        </FeatureLayout>
      </DensityProvider>
    );
  }

  return (
    <DensityProvider pageKey="system-nodes">
      <FeatureLayout
        title="Nodes"
        description="Manage cluster nodes and federation sync status"
        maxWidth="xl"
      >
        <div className="space-y-6">
          <Alert>
            <Info className="h-4 w-4" />
            <AlertDescription>
              This page shows local cluster nodes and their federation sync status with peer nodes.
            </AlertDescription>
          </Alert>

          {!peersError && peersData && peersData.peers.length > 0 && (
            <PeerSyncStatusCard
              peers={derivePeerSyncInfoList(peersData.peers)}
              isLoading={peersLoading}
              showTitle={true}
              compact={true}
            />
          )}

          <NodeTable
            nodes={nodes}
            isLoading={isLoading}
            onNodeSelect={setSelectedNodeId}
            onRefresh={refetch}
          />

          {selectedNodeId && (
            <NodeDetailModal
              nodeId={selectedNodeId}
              open={!!selectedNodeId}
              onClose={() => setSelectedNodeId(null)}
            />
          )}
        </div>
      </FeatureLayout>
    </DensityProvider>
  );
}
