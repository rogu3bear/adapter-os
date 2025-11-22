import { useState } from 'react';
import { useNodes } from '@/hooks/useSystemMetrics';
import NodeTable from './NodeTable';
import NodeDetailModal from './NodeDetailModal';
import { Card, CardContent } from '@/components/ui/card';

export default function NodesTab() {
  const { nodes, isLoading, error, refetch } = useNodes('normal');
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);

  if (error) {
    return (
      <Card className="border-destructive bg-destructive/10">
        <CardContent className="pt-6">
          <p className="text-destructive">Failed to load nodes: {error.message}</p>
        </CardContent>
      </Card>
    );
  }

  return (
    <div className="space-y-6">
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
  );
}
