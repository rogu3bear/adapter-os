import { useNavigate, useParams } from 'react-router-dom';
import { Modal } from '@/components/shared/Modal';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Skeleton } from '@/components/ui/skeleton';
import { LoadingState } from '@/components/ui/loading-state';
import { ErrorRecovery } from '@/components/ui/error-recovery';
import { useNodeDetails, useNodes } from '@/hooks/system/useSystemMetrics';

interface NodeDetailModalProps {
  nodeId: string;
  open: boolean;
  onClose: () => void;
}

export function NodeDetailModal({ nodeId, open, onClose }: NodeDetailModalProps) {
  const { data: nodeDetails, isLoading } = useNodeDetails(nodeId, open);

  return (
    <Modal
      open={open}
      onOpenChange={onClose}
      title="Node Details"
      description={`Detailed information for node ${nodeId}`}
      size="xl"
      className="max-w-3xl max-h-[80vh] overflow-y-auto"
    >
      {isLoading ? (
          <div className="space-y-4">
            <Skeleton className="h-24 w-full" />
            <Skeleton className="h-32 w-full" />
            <Skeleton className="h-32 w-full" />
          </div>
        ) : nodeDetails ? (
          <div className="space-y-6">
            {/* Basic Information */}
            <Card>
              <CardHeader>
                <CardTitle>Basic Information</CardTitle>
              </CardHeader>
              <CardContent className="grid grid-cols-2 gap-4">
                <div>
                  <div className="text-sm font-medium text-muted-foreground">Node ID</div>
                  <div className="font-mono">{nodeDetails.id}</div>
                </div>
                <div>
                  <div className="text-sm font-medium text-muted-foreground">Hostname</div>
                  <div>{nodeDetails.hostname}</div>
                </div>
                <div>
                  <div className="text-sm font-medium text-muted-foreground">Status</div>
                  <div>
                    <Badge
                      variant={
                        nodeDetails.status === 'healthy'
                          ? 'success'
                          : nodeDetails.status === 'offline'
                          ? 'secondary'
                          : 'destructive'
                      }
                    >
                      {nodeDetails.status}
                    </Badge>
                  </div>
                </div>
                <div>
                  <div className="text-sm font-medium text-muted-foreground">Last Seen</div>
                  <div>
                    {nodeDetails.last_seen_at
                      ? new Date(nodeDetails.last_seen_at).toLocaleString()
                      : 'Never'}
                  </div>
                </div>
              </CardContent>
            </Card>

            {/* Hardware Information */}
            <Card>
              <CardHeader>
                <CardTitle>Hardware</CardTitle>
              </CardHeader>
              <CardContent className="grid grid-cols-2 gap-4">
                <div>
                  <div className="text-sm font-medium text-muted-foreground">Memory</div>
                  <div>{nodeDetails.memory_gb ? `${nodeDetails.memory_gb} GB` : '--'}</div>
                </div>
                <div>
                  <div className="text-sm font-medium text-muted-foreground">GPU Count</div>
                  <div>{nodeDetails.gpu_count ?? '--'}</div>
                </div>
                <div>
                  <div className="text-sm font-medium text-muted-foreground">GPU Type</div>
                  <div>{nodeDetails.gpu_type ?? '--'}</div>
                </div>
                <div>
                  <div className="text-sm font-medium text-muted-foreground">Metal Family</div>
                  <div>{nodeDetails.metal_family ?? '--'}</div>
                </div>
              </CardContent>
            </Card>

            {/* Workers */}
            <Card>
              <CardHeader>
                <div className="flex items-center justify-between">
                  <CardTitle>Workers</CardTitle>
                  <Badge variant="secondary">{nodeDetails.workers?.length ?? 0}</Badge>
                </div>
                <CardDescription>Worker processes running on this node</CardDescription>
              </CardHeader>
              <CardContent>
                {nodeDetails.workers && nodeDetails.workers.length > 0 ? (
                  <div className="space-y-2">
                    {nodeDetails.workers.map((worker) => (
                      <div
                        key={worker.id}
                        className="flex items-center justify-between p-3 border rounded-lg"
                      >
                        <div className="space-y-1">
                          <div className="font-mono text-sm">{worker.id}</div>
                          <div className="text-xs text-muted-foreground">
                            Tenant: {worker.tenant_id} | Plan: {worker.plan_id}
                          </div>
                        </div>
                        <Badge
                          variant={
                            worker.status === 'running'
                              ? 'success'
                              : worker.status === 'stopped'
                              ? 'secondary'
                              : 'destructive'
                          }
                        >
                          {worker.status}
                        </Badge>
                      </div>
                    ))}
                  </div>
                ) : (
                  <div className="text-center text-muted-foreground py-4">
                    No workers running on this node
                  </div>
                )}
              </CardContent>
            </Card>
          </div>
        ) : (
          <div className="text-center text-muted-foreground py-8">
            No details available for this node
          </div>
        )}
    </Modal>
  );
}

/**
 * Route-safe wrapper for NodeDetailModal.
 * This component reads nodeId from URL params and fetches the node data,
 * rendering the modal NodeDetailModal with proper props.
 *
 * NOTE: Modal components with required props should NEVER be used directly as route components.
 * Always use a *RoutePage wrapper that reads params and fetches data.
 */
export default function NodeDetailRoutePage() {
  const navigate = useNavigate();
  const { nodeId } = useParams<{ nodeId: string }>();
  const { nodes, isLoading, error, refetch } = useNodes('normal');

  const handleClose = () => {
    navigate('/system/nodes');
  };

  // Loading state
  if (isLoading) {
    return (
      <div className="flex items-center justify-center min-h-[400px]">
        <LoadingState message="Loading node details..." />
      </div>
    );
  }

  // Error state
  if (error) {
    return (
      <div className="flex items-center justify-center min-h-[400px]">
        <ErrorRecovery
          error={error instanceof Error ? error.message : String(error)}
          onRetry={refetch}
        />
      </div>
    );
  }

  // Find the node from the list
  const node = nodes?.find((n) => n.id === nodeId);

  // Not found state
  if (!node && nodeId) {
    return (
      <div className="flex flex-col items-center justify-center min-h-[400px] gap-4">
        <Card className="max-w-md">
          <CardHeader>
            <CardTitle>Node Not Found</CardTitle>
            <CardDescription>
              The node with ID <span className="font-mono">{nodeId}</span> could not be found.
            </CardDescription>
          </CardHeader>
          <CardContent>
            <Button onClick={handleClose} variant="outline">
              Back to Nodes
            </Button>
          </CardContent>
        </Card>
      </div>
    );
  }

  // Render the modal with node data
  if (!nodeId) {
    return null;
  }

  return <NodeDetailModal nodeId={nodeId} open={true} onClose={handleClose} />;
}
