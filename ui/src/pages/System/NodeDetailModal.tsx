import { Modal } from '@/components/shared/Modal';
import { Badge } from '@/components/ui/badge';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Skeleton } from '@/components/ui/skeleton';
import { useNodeDetails } from '@/hooks/useSystemMetrics';

interface NodeDetailModalProps {
  nodeId: string;
  open: boolean;
  onClose: () => void;
}

export default function NodeDetailModal({ nodeId, open, onClose }: NodeDetailModalProps) {
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
