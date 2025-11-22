import { useState } from 'react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Badge } from '@/components/ui/badge';
import { Card, CardContent } from '@/components/ui/card';
import { Skeleton } from '@/components/ui/skeleton';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { useWorkerDetails, useWorkerLogs, useWorkerCrashes } from '@/hooks/useSystemMetrics';

interface WorkerLogsModalProps {
  workerId: string;
  open: boolean;
  onClose: () => void;
}

export default function WorkerLogsModal({ workerId, open, onClose }: WorkerLogsModalProps) {
  const { data: workerDetails, isLoading: detailsLoading } = useWorkerDetails(workerId, open);
  const { data: logs, isLoading: logsLoading } = useWorkerLogs(workerId, undefined, open);
  const { data: crashes, isLoading: crashesLoading } = useWorkerCrashes(workerId, open);
  const [activeTab, setActiveTab] = useState('details');

  const getLevelBadge = (level: string) => {
    const variant =
      level === 'error'
        ? 'destructive'
        : level === 'warn'
        ? 'warning'
        : level === 'info'
        ? 'default'
        : 'secondary';
    return <Badge variant={variant}>{level.toUpperCase()}</Badge>;
  };

  return (
    <Dialog open={open} onOpenChange={onClose}>
      <DialogContent className="max-w-5xl max-h-[90vh] overflow-hidden flex flex-col">
        <DialogHeader>
          <DialogTitle>Worker Details</DialogTitle>
          <DialogDescription>Logs and information for worker {workerId}</DialogDescription>
        </DialogHeader>

        <Tabs value={activeTab} onValueChange={setActiveTab} className="flex-1 flex flex-col min-h-0">
          <TabsList className="grid w-full grid-cols-3">
            <TabsTrigger value="details">Details</TabsTrigger>
            <TabsTrigger value="logs">
              Logs {logs && logs.length > 0 && <Badge className="ml-2">{logs.length}</Badge>}
            </TabsTrigger>
            <TabsTrigger value="crashes">
              Crashes {crashes && crashes.length > 0 && <Badge className="ml-2">{crashes.length}</Badge>}
            </TabsTrigger>
          </TabsList>

          <TabsContent value="details" className="flex-1 overflow-auto">
            {detailsLoading ? (
              <div className="space-y-4">
                <Skeleton className="h-32 w-full" />
                <Skeleton className="h-24 w-full" />
              </div>
            ) : workerDetails ? (
              <div className="space-y-4">
                <Card>
                  <CardContent className="pt-6 grid grid-cols-2 gap-4">
                    <div>
                      <div className="text-sm font-medium text-muted-foreground">Worker ID</div>
                      <div className="font-mono">{workerDetails.worker_id}</div>
                    </div>
                    <div>
                      <div className="text-sm font-medium text-muted-foreground">Status</div>
                      <div>
                        <Badge
                          variant={
                            workerDetails.status === 'running'
                              ? 'success'
                              : workerDetails.status === 'stopped'
                              ? 'secondary'
                              : 'destructive'
                          }
                        >
                          {workerDetails.status}
                        </Badge>
                      </div>
                    </div>
                    <div>
                      <div className="text-sm font-medium text-muted-foreground">Type</div>
                      <div>{workerDetails.worker_type}</div>
                    </div>
                    <div>
                      <div className="text-sm font-medium text-muted-foreground">Node ID</div>
                      <div className="font-mono">{workerDetails.node_id}</div>
                    </div>
                    <div>
                      <div className="text-sm font-medium text-muted-foreground">Tenant ID</div>
                      <div>{workerDetails.tenant_id ?? '--'}</div>
                    </div>
                    <div>
                      <div className="text-sm font-medium text-muted-foreground">Plan ID</div>
                      <div>{workerDetails.plan_id ?? '--'}</div>
                    </div>
                    <div>
                      <div className="text-sm font-medium text-muted-foreground">PID</div>
                      <div>{workerDetails.pid ?? '--'}</div>
                    </div>
                    <div>
                      <div className="text-sm font-medium text-muted-foreground">Memory</div>
                      <div>{workerDetails.memory_mb ? `${workerDetails.memory_mb} MB` : '--'}</div>
                    </div>
                    <div>
                      <div className="text-sm font-medium text-muted-foreground">CPU</div>
                      <div>{workerDetails.cpu_percent ? `${workerDetails.cpu_percent.toFixed(1)}%` : '--'}</div>
                    </div>
                    <div>
                      <div className="text-sm font-medium text-muted-foreground">Uptime</div>
                      <div>
                        {workerDetails.uptime_seconds
                          ? `${Math.floor(workerDetails.uptime_seconds / 60)} min`
                          : '--'}
                      </div>
                    </div>
                    <div>
                      <div className="text-sm font-medium text-muted-foreground">Created</div>
                      <div>{new Date(workerDetails.created_at).toLocaleString()}</div>
                    </div>
                    <div>
                      <div className="text-sm font-medium text-muted-foreground">Last Heartbeat</div>
                      <div>
                        {workerDetails.last_heartbeat
                          ? new Date(workerDetails.last_heartbeat).toLocaleString()
                          : '--'}
                      </div>
                    </div>
                  </CardContent>
                </Card>

                {workerDetails.error && (
                  <Card className="border-destructive bg-destructive/10">
                    <CardContent className="pt-6">
                      <div className="text-sm font-medium text-destructive mb-2">Error</div>
                      <pre className="text-sm text-destructive whitespace-pre-wrap">{workerDetails.error}</pre>
                    </CardContent>
                  </Card>
                )}
              </div>
            ) : (
              <div className="text-center text-muted-foreground py-8">
                No details available for this worker
              </div>
            )}
          </TabsContent>

          <TabsContent value="logs" className="flex-1 min-h-0">
            <ScrollArea className="h-[500px] border rounded-lg">
              {logsLoading ? (
                <div className="p-4 space-y-2">
                  {[...Array(10)].map((_, i) => (
                    <Skeleton key={i} className="h-12 w-full" />
                  ))}
                </div>
              ) : logs && logs.length > 0 ? (
                <div className="p-4 space-y-2 font-mono text-sm">
                  {logs.map((log) => (
                    <div key={log.id} className="flex items-start gap-3 p-2 border-b">
                      <div className="text-muted-foreground whitespace-nowrap text-xs">
                        {new Date(log.timestamp).toLocaleTimeString()}
                      </div>
                      {getLevelBadge(log.level)}
                      <div className="flex-1 break-words">{log.message}</div>
                    </div>
                  ))}
                </div>
              ) : (
                <div className="flex items-center justify-center h-full text-muted-foreground">
                  No logs available
                </div>
              )}
            </ScrollArea>
          </TabsContent>

          <TabsContent value="crashes" className="flex-1 min-h-0">
            <ScrollArea className="h-[500px] border rounded-lg">
              {crashesLoading ? (
                <div className="p-4 space-y-2">
                  {[...Array(5)].map((_, i) => (
                    <Skeleton key={i} className="h-24 w-full" />
                  ))}
                </div>
              ) : crashes && crashes.length > 0 ? (
                <div className="p-4 space-y-4">
                  {crashes.map((crash) => (
                    <Card key={crash.id} className="border-destructive">
                      <CardContent className="pt-6">
                        <div className="space-y-2">
                          <div className="flex items-center justify-between">
                            <Badge variant="destructive">{crash.crash_type}</Badge>
                            <span className="text-sm text-muted-foreground">
                              {new Date(crash.crash_timestamp).toLocaleString()}
                            </span>
                          </div>
                          {crash.exit_code !== undefined && (
                            <div className="text-sm">
                              <span className="font-medium">Exit Code:</span> {crash.exit_code}
                            </div>
                          )}
                          {crash.signal && (
                            <div className="text-sm">
                              <span className="font-medium">Signal:</span> {crash.signal}
                            </div>
                          )}
                          {crash.stack_trace && (
                            <div className="mt-2">
                              <div className="text-sm font-medium mb-1">Stack Trace:</div>
                              <pre className="text-xs bg-muted p-2 rounded overflow-x-auto">
                                {crash.stack_trace}
                              </pre>
                            </div>
                          )}
                          {crash.recovery_action && (
                            <div className="text-sm">
                              <span className="font-medium">Recovery Action:</span> {crash.recovery_action}
                            </div>
                          )}
                          {crash.recovered_at && (
                            <div className="text-sm">
                              <span className="font-medium">Recovered At:</span>{' '}
                              {new Date(crash.recovered_at).toLocaleString()}
                            </div>
                          )}
                        </div>
                      </CardContent>
                    </Card>
                  ))}
                </div>
              ) : (
                <div className="flex items-center justify-center h-full text-muted-foreground">
                  No crashes recorded
                </div>
              )}
            </ScrollArea>
          </TabsContent>
        </Tabs>
      </DialogContent>
    </Dialog>
  );
}
