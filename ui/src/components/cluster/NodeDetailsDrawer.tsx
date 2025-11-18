import React from 'react';
import {
  Drawer,
  DrawerContent,
  DrawerDescription,
  DrawerHeader,
  DrawerTitle,
} from '../ui/drawer';
import { Badge } from '../ui/badge';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '../ui/card';
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from '../ui/accordion';
import {
  Server,
  Activity,
  Cpu,
  MemoryStick,
  HardDrive,
  Network,
  Clock,
  FileText,
  CheckCircle,
  XCircle,
  AlertTriangle,
} from 'lucide-react';
import type { NodeDetailsResponse } from '@/api/types';

interface NodeDetailsDrawerProps {
  node: NodeDetailsResponse;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function NodeDetailsDrawer({ node, open, onOpenChange }: NodeDetailsDrawerProps) {
  const getStatusIcon = (status: string) => {
    switch (status) {
      case 'active':
        return <CheckCircle className="h-4 w-4 text-green-600" />;
      case 'offline':
        return <XCircle className="h-4 w-4 text-red-600" />;
      case 'maintenance':
        return <AlertTriangle className="h-4 w-4 text-yellow-600" />;
      default:
        return <Clock className="h-4 w-4 text-gray-600" />;
    }
  };

  const getStatusBadge = (status: string) => {
    const variant =
      status === 'active' || status === 'serving' ? 'default' :
      status === 'offline' || status === 'stopped' || status === 'crashed' ? 'destructive' :
      'secondary';

    return (
      <Badge variant={variant} className="flex items-center gap-1 w-fit">
        {getStatusIcon(status)}
        {status}
      </Badge>
    );
  };

  const formatTimestamp = (timestamp: string | null | undefined) => {
    if (!timestamp) return 'Never';
    return new Date(timestamp).toLocaleString();
  };

  return (
    <Drawer open={open} onOpenChange={onOpenChange}>
      <DrawerContent className="h-[85vh]">
        <DrawerHeader>
          <DrawerTitle className="flex items-center gap-2">
            <Server className="h-5 w-5" />
            Node Details: {node.hostname}
          </DrawerTitle>
          <DrawerDescription>
            Node ID: {node.id}
          </DrawerDescription>
        </DrawerHeader>

        <div className="overflow-y-auto px-6 pb-6">
          <Accordion type="multiple" defaultValue={['status', 'workers', 'resources']}>
            {/* Status & Metadata */}
            <AccordionItem value="status">
              <AccordionTrigger>
                <div className="flex items-center gap-2">
                  <Activity className="h-4 w-4" />
                  Status & Metadata
                </div>
              </AccordionTrigger>
              <AccordionContent>
                <Card>
                  <CardContent className="pt-6">
                    <div className="grid grid-cols-2 gap-4">
                      <div>
                        <div className="text-sm font-medium text-muted-foreground mb-1">
                          Status
                        </div>
                        {getStatusBadge(node.status)}
                      </div>
                      <div>
                        <div className="text-sm font-medium text-muted-foreground mb-1">
                          Endpoint
                        </div>
                        <div className="text-sm font-mono bg-muted px-2 py-1 rounded">
                          {node.agent_endpoint}
                        </div>
                      </div>
                      <div>
                        <div className="text-sm font-medium text-muted-foreground mb-1">
                          Last Seen
                        </div>
                        <div className="text-sm">{formatTimestamp(node.last_seen_at)}</div>
                      </div>
                      <div>
                        <div className="text-sm font-medium text-muted-foreground mb-1">
                          Last Heartbeat
                        </div>
                        <div className="text-sm">{formatTimestamp(node.last_heartbeat)}</div>
                      </div>
                    </div>
                  </CardContent>
                </Card>
              </AccordionContent>
            </AccordionItem>

            {/* Resource Information */}
            <AccordionItem value="resources">
              <AccordionTrigger>
                <div className="flex items-center gap-2">
                  <HardDrive className="h-4 w-4" />
                  Resource Information
                </div>
              </AccordionTrigger>
              <AccordionContent>
                <Card>
                  <CardContent className="pt-6">
                    <div className="grid grid-cols-2 md:grid-cols-3 gap-6">
                      <div className="flex items-start gap-3">
                        <div className="p-2 bg-purple-100 dark:bg-purple-900/20 rounded">
                          <Cpu className="h-5 w-5 text-purple-600" />
                        </div>
                        <div>
                          <div className="text-sm font-medium text-muted-foreground">
                            Hardware Family
                          </div>
                          <div className="text-lg font-semibold">
                            {node.metal_family || 'Unknown'}
                          </div>
                        </div>
                      </div>

                      <div className="flex items-start gap-3">
                        <div className="p-2 bg-blue-100 dark:bg-blue-900/20 rounded">
                          <MemoryStick className="h-5 w-5 text-blue-600" />
                        </div>
                        <div>
                          <div className="text-sm font-medium text-muted-foreground">
                            Memory
                          </div>
                          <div className="text-lg font-semibold">
                            {node.memory_gb || 0} GB
                          </div>
                        </div>
                      </div>

                      {node.gpu_count !== undefined && (
                        <div className="flex items-start gap-3">
                          <div className="p-2 bg-orange-100 dark:bg-orange-900/20 rounded">
                            <Cpu className="h-5 w-5 text-orange-600" />
                          </div>
                          <div>
                            <div className="text-sm font-medium text-muted-foreground">
                              GPU Count
                            </div>
                            <div className="text-lg font-semibold">
                              {node.gpu_count}
                            </div>
                            {node.gpu_type && (
                              <div className="text-xs text-muted-foreground">
                                {node.gpu_type}
                              </div>
                            )}
                          </div>
                        </div>
                      )}
                    </div>
                  </CardContent>
                </Card>
              </AccordionContent>
            </AccordionItem>

            {/* Workers */}
            <AccordionItem value="workers">
              <AccordionTrigger>
                <div className="flex items-center gap-2">
                  <Activity className="h-4 w-4" />
                  Workers ({node.workers.length})
                </div>
              </AccordionTrigger>
              <AccordionContent>
                <Card>
                  <CardHeader>
                    <CardTitle>Active Workers</CardTitle>
                    <CardDescription>
                      Worker processes running on this node
                    </CardDescription>
                  </CardHeader>
                  <CardContent>
                    {node.workers.length === 0 ? (
                      <div className="text-center py-8 text-muted-foreground">
                        <Activity className="h-8 w-8 mx-auto mb-2 opacity-50" />
                        No workers running on this node
                      </div>
                    ) : (
                      <div className="space-y-3">
                        {node.workers.map((worker) => (
                          <div
                            key={worker.id}
                            className="flex items-center justify-between p-3 rounded-lg border hover:bg-accent/50 transition-colors"
                          >
                            <div className="flex items-center gap-3">
                              <Activity className="h-4 w-4 text-muted-foreground" />
                              <div>
                                <div className="font-medium text-sm">
                                  Worker {worker.id.substring(0, 8)}
                                </div>
                                <div className="text-xs text-muted-foreground">
                                  Tenant: {worker.tenant_id} • Plan: {worker.plan_id}
                                </div>
                              </div>
                            </div>
                            {getStatusBadge(worker.status)}
                          </div>
                        ))}
                      </div>
                    )}
                  </CardContent>
                </Card>
              </AccordionContent>
            </AccordionItem>

            {/* Recent Logs */}
            <AccordionItem value="logs">
              <AccordionTrigger>
                <div className="flex items-center gap-2">
                  <FileText className="h-4 w-4" />
                  Recent Logs
                </div>
              </AccordionTrigger>
              <AccordionContent>
                <Card>
                  <CardHeader>
                    <CardTitle>Recent Activity</CardTitle>
                    <CardDescription>
                      Latest log entries from this node
                    </CardDescription>
                  </CardHeader>
                  <CardContent>
                    {node.recent_logs.length === 0 ? (
                      <div className="text-center py-8 text-muted-foreground">
                        <FileText className="h-8 w-8 mx-auto mb-2 opacity-50" />
                        No recent logs available
                      </div>
                    ) : (
                      <div className="space-y-2">
                        {node.recent_logs.map((log, index) => (
                          <div
                            key={index}
                            className="p-3 rounded bg-muted font-mono text-xs"
                          >
                            {log}
                          </div>
                        ))}
                      </div>
                    )}
                  </CardContent>
                </Card>
              </AccordionContent>
            </AccordionItem>

            {/* Network Information */}
            <AccordionItem value="network">
              <AccordionTrigger>
                <div className="flex items-center gap-2">
                  <Network className="h-4 w-4" />
                  Network Information
                </div>
              </AccordionTrigger>
              <AccordionContent>
                <Card>
                  <CardContent className="pt-6">
                    <div className="space-y-3">
                      <div>
                        <div className="text-sm font-medium text-muted-foreground mb-1">
                          Agent Endpoint
                        </div>
                        <div className="text-sm font-mono bg-muted px-3 py-2 rounded">
                          {node.agent_endpoint}
                        </div>
                      </div>
                      <div>
                        <div className="text-sm font-medium text-muted-foreground mb-1">
                          Hostname
                        </div>
                        <div className="text-sm font-mono bg-muted px-3 py-2 rounded">
                          {node.hostname}
                        </div>
                      </div>
                    </div>
                  </CardContent>
                </Card>
              </AccordionContent>
            </AccordionItem>
          </Accordion>
        </div>
      </DrawerContent>
    </Drawer>
  );
}
