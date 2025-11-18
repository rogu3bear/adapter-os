import React from 'react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '../ui/card';
import { Badge } from '../ui/badge';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '../ui/table';
import {
  Activity,
  Server,
  CheckCircle,
  XCircle,
  AlertTriangle,
  Clock,
  Cpu,
  MemoryStick,
} from 'lucide-react';
import type { WorkerResponse, Node } from '@/api/types';

interface WorkerTableProps {
  workers: WorkerResponse[];
  nodes: Node[];
  onRefresh: () => void;
}

export function WorkerTable({ workers, nodes }: WorkerTableProps) {
  const getNodeHostname = (nodeId: string) => {
    const node = nodes.find(n => n.id === nodeId);
    return node?.hostname || nodeId;
  };

  const getStatusIcon = (status: string) => {
    switch (status) {
      case 'serving':
      case 'starting':
        return <CheckCircle className="h-4 w-4 text-green-600" />;
      case 'stopped':
      case 'crashed':
        return <XCircle className="h-4 w-4 text-red-600" />;
      case 'draining':
        return <AlertTriangle className="h-4 w-4 text-yellow-600" />;
      default:
        return <Clock className="h-4 w-4 text-gray-600" />;
    }
  };

  const getStatusBadge = (status: string) => {
    const variant =
      status === 'serving' ? 'default' :
      status === 'starting' ? 'secondary' :
      status === 'stopped' || status === 'crashed' ? 'destructive' :
      'outline';

    return (
      <Badge variant={variant} className="flex items-center gap-1 w-fit">
        {getStatusIcon(status)}
        {status}
      </Badge>
    );
  };

  const formatTimestamp = (timestamp: string | undefined) => {
    if (!timestamp) return 'N/A';
    return new Date(timestamp).toLocaleString();
  };

  if (workers.length === 0) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Workers</CardTitle>
          <CardDescription>No workers running in the cluster</CardDescription>
        </CardHeader>
        <CardContent>
          <div className="flex flex-col items-center justify-center p-8 text-center">
            <Activity className="h-12 w-12 text-muted-foreground mb-4" />
            <p className="text-muted-foreground">
              No active worker processes found
            </p>
          </div>
        </CardContent>
      </Card>
    );
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Activity className="h-5 w-5" />
          Workers ({workers.length})
        </CardTitle>
        <CardDescription>
          View all worker processes across the cluster
        </CardDescription>
      </CardHeader>
      <CardContent>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Worker ID</TableHead>
              <TableHead>Node</TableHead>
              <TableHead>Tenant</TableHead>
              <TableHead>Status</TableHead>
              <TableHead>PID</TableHead>
              <TableHead>Memory Headroom</TableHead>
              <TableHead>K Current</TableHead>
              <TableHead>Created</TableHead>
              <TableHead>Last Heartbeat</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {workers.map((worker) => (
              <TableRow key={worker.id}>
                <TableCell className="font-mono text-xs">
                  <div className="flex items-center gap-2">
                    <Activity className="h-3 w-3 text-muted-foreground" />
                    {worker.id.substring(0, 12)}...
                  </div>
                </TableCell>
                <TableCell>
                  <div className="flex items-center gap-2">
                    <Server className="h-3 w-3 text-muted-foreground" />
                    <span className="text-sm">{getNodeHostname(worker.node_id)}</span>
                  </div>
                </TableCell>
                <TableCell>
                  <Badge variant="outline" className="text-xs">
                    {worker.tenant_id}
                  </Badge>
                </TableCell>
                <TableCell>{getStatusBadge(worker.status)}</TableCell>
                <TableCell>
                  <span className="font-mono text-xs">
                    {worker.pid || 'N/A'}
                  </span>
                </TableCell>
                <TableCell>
                  {worker.memory_headroom_pct !== undefined ? (
                    <div className="flex items-center gap-2">
                      <MemoryStick className="h-3 w-3 text-muted-foreground" />
                      <span className="text-sm">
                        {worker.memory_headroom_pct.toFixed(1)}%
                      </span>
                    </div>
                  ) : (
                    <span className="text-sm text-muted-foreground">N/A</span>
                  )}
                </TableCell>
                <TableCell>
                  {worker.k_current !== undefined ? (
                    <div className="flex items-center gap-2">
                      <Cpu className="h-3 w-3 text-muted-foreground" />
                      <span className="text-sm">{worker.k_current}</span>
                    </div>
                  ) : (
                    <span className="text-sm text-muted-foreground">N/A</span>
                  )}
                </TableCell>
                <TableCell>
                  <span className="text-xs text-muted-foreground">
                    {formatTimestamp(worker.created_at)}
                  </span>
                </TableCell>
                <TableCell>
                  <span className="text-xs text-muted-foreground">
                    {formatTimestamp(worker.last_heartbeat_at)}
                  </span>
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </CardContent>
    </Card>
  );
}
