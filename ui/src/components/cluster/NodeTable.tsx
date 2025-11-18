import React, { useState } from 'react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '../ui/card';
import { Button } from '../ui/button';
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
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '../ui/dropdown-menu';
import {
  Server,
  MoreVertical,
  Eye,
  Wifi,
  Trash2,
  WifiOff,
  CheckCircle,
  XCircle,
  AlertTriangle,
  Clock,
} from 'lucide-react';
import { NodeDetailsDrawer } from './NodeDetailsDrawer';
import { ConfirmationDialog } from '../ui/confirmation-dialog';
import apiClient from '../../api/client';
import { logger, toError } from '../../utils/logger';
import { useToast } from '../../hooks/use-toast';
import type { Node, NodeDetailsResponse, NodePingResponse } from '@/api/types';

interface NodeTableProps {
  nodes: Node[];
  onRefresh: () => void;
}

export function NodeTable({ nodes, onRefresh }: NodeTableProps) {
  const { toast } = useToast();
  const [selectedNode, setSelectedNode] = useState<NodeDetailsResponse | null>(null);
  const [showDetailsDrawer, setShowDetailsDrawer] = useState(false);
  const [showEvictConfirm, setShowEvictConfirm] = useState(false);
  const [nodeToEvict, setNodeToEvict] = useState<Node | null>(null);
  const [testingNodes, setTestingNodes] = useState<Set<string>>(new Set());
  const [evictingNodes, setEvictingNodes] = useState<Set<string>>(new Set());
  const [pingResults, setPingResults] = useState<Map<string, NodePingResponse>>(new Map());

  const handleViewDetails = async (node: Node) => {
    try {
      const details = await apiClient.getNodeDetails(node.id);
      setSelectedNode(details);
      setShowDetailsDrawer(true);
    } catch (err) {
      logger.error('Failed to fetch node details', { nodeId: node.id }, toError(err));
      toast({
        title: 'Error',
        description: 'Failed to load node details',
        variant: 'destructive',
      });
    }
  };

  const handleTestConnection = async (node: Node) => {
    setTestingNodes(prev => new Set(prev).add(node.id));

    try {
      const result = await apiClient.testNodeConnection(node.id);
      setPingResults(prev => new Map(prev).set(node.id, result));

      toast({
        title: result.status === 'reachable' ? 'Connection Successful' : 'Connection Failed',
        description: result.status === 'reachable'
          ? `Node ${node.hostname} is reachable (${result.latency_ms.toFixed(2)}ms)`
          : `Node ${node.hostname} is unreachable`,
        variant: result.status === 'reachable' ? 'default' : 'destructive',
      });

      onRefresh();
    } catch (err) {
      logger.error('Failed to test node connection', { nodeId: node.id }, toError(err));
      toast({
        title: 'Test Failed',
        description: 'Failed to test node connection',
        variant: 'destructive',
      });
    } finally {
      setTestingNodes(prev => {
        const next = new Set(prev);
        next.delete(node.id);
        return next;
      });
    }
  };

  const handleMarkOffline = async (node: Node) => {
    try {
      await apiClient.markNodeOffline(node.id);
      toast({
        title: 'Node Marked Offline',
        description: `${node.hostname} has been marked as offline`,
      });
      onRefresh();
    } catch (err) {
      logger.error('Failed to mark node offline', { nodeId: node.id }, toError(err));
      toast({
        title: 'Error',
        description: 'Failed to mark node offline',
        variant: 'destructive',
      });
    }
  };

  const handleEvictNode = async () => {
    if (!nodeToEvict) return;

    setEvictingNodes(prev => new Set(prev).add(nodeToEvict.id));

    try {
      await apiClient.evictNode(nodeToEvict.id);
      toast({
        title: 'Node Evicted',
        description: `${nodeToEvict.hostname} has been removed from the cluster`,
      });
      setShowEvictConfirm(false);
      setNodeToEvict(null);
      onRefresh();
    } catch (err) {
      logger.error('Failed to evict node', { nodeId: nodeToEvict.id }, toError(err));
      toast({
        title: 'Eviction Failed',
        description: err instanceof Error ? err.message : 'Failed to evict node',
        variant: 'destructive',
      });
    } finally {
      setEvictingNodes(prev => {
        const next = new Set(prev);
        next.delete(nodeToEvict.id);
        return next;
      });
    }
  };

  const confirmEvict = (node: Node) => {
    setNodeToEvict(node);
    setShowEvictConfirm(true);
  };

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
      status === 'active' ? 'default' :
      status === 'offline' ? 'destructive' :
      status === 'maintenance' ? 'secondary' :
      'outline';

    return (
      <Badge variant={variant} className="flex items-center gap-1">
        {getStatusIcon(status)}
        {status}
      </Badge>
    );
  };

  if (nodes.length === 0) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Nodes</CardTitle>
          <CardDescription>No nodes registered in the cluster</CardDescription>
        </CardHeader>
        <CardContent>
          <div className="flex flex-col items-center justify-center p-8 text-center">
            <Server className="h-12 w-12 text-muted-foreground mb-4" />
            <p className="text-muted-foreground">
              Register your first node to start managing cluster operations
            </p>
          </div>
        </CardContent>
      </Card>
    );
  }

  return (
    <>
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Server className="h-5 w-5" />
            Nodes ({nodes.length})
          </CardTitle>
          <CardDescription>
            Manage compute nodes and monitor their health
          </CardDescription>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Hostname</TableHead>
                <TableHead>Status</TableHead>
                <TableHead>Hardware</TableHead>
                <TableHead>Memory</TableHead>
                <TableHead>Last Heartbeat</TableHead>
                <TableHead>Connection</TableHead>
                <TableHead className="text-right">Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {nodes.map((node) => {
                const pingResult = pingResults.get(node.id);
                const isTesting = testingNodes.has(node.id);
                const isEvicting = evictingNodes.has(node.id);

                return (
                  <TableRow key={node.id}>
                    <TableCell className="font-medium">
                      <div className="flex items-center gap-2">
                        <Server className="h-4 w-4 text-muted-foreground" />
                        {node.hostname}
                      </div>
                    </TableCell>
                    <TableCell>{getStatusBadge(node.status)}</TableCell>
                    <TableCell>
                      <div className="text-sm">{node.metal_family || 'Unknown'}</div>
                    </TableCell>
                    <TableCell>{node.memory_gb} GB</TableCell>
                    <TableCell>
                      <div className="text-sm text-muted-foreground">
                        {node.last_heartbeat
                          ? new Date(node.last_heartbeat).toLocaleString()
                          : 'Never'}
                      </div>
                    </TableCell>
                    <TableCell>
                      {isTesting ? (
                        <Badge variant="outline" className="animate-pulse">
                          Testing...
                        </Badge>
                      ) : pingResult ? (
                        <Badge
                          variant={pingResult.status === 'reachable' ? 'default' : 'destructive'}
                        >
                          {pingResult.status === 'reachable'
                            ? `${pingResult.latency_ms.toFixed(0)}ms`
                            : 'Unreachable'}
                        </Badge>
                      ) : (
                        <Badge variant="outline">Not tested</Badge>
                      )}
                    </TableCell>
                    <TableCell className="text-right">
                      <DropdownMenu>
                        <DropdownMenuTrigger asChild>
                          <Button variant="ghost" size="sm" disabled={isEvicting}>
                            <MoreVertical className="h-4 w-4" />
                          </Button>
                        </DropdownMenuTrigger>
                        <DropdownMenuContent align="end">
                          <DropdownMenuLabel>Actions</DropdownMenuLabel>
                          <DropdownMenuSeparator />
                          <DropdownMenuItem onClick={() => handleViewDetails(node)}>
                            <Eye className="h-4 w-4 mr-2" />
                            View Details
                          </DropdownMenuItem>
                          <DropdownMenuItem
                            onClick={() => handleTestConnection(node)}
                            disabled={isTesting}
                          >
                            <Wifi className="h-4 w-4 mr-2" />
                            Test Connection
                          </DropdownMenuItem>
                          {node.status === 'active' && (
                            <DropdownMenuItem onClick={() => handleMarkOffline(node)}>
                              <WifiOff className="h-4 w-4 mr-2" />
                              Mark Offline
                            </DropdownMenuItem>
                          )}
                          <DropdownMenuSeparator />
                          <DropdownMenuItem
                            onClick={() => confirmEvict(node)}
                            className="text-destructive"
                          >
                            <Trash2 className="h-4 w-4 mr-2" />
                            Evict Node
                          </DropdownMenuItem>
                        </DropdownMenuContent>
                      </DropdownMenu>
                    </TableCell>
                  </TableRow>
                );
              })}
            </TableBody>
          </Table>
        </CardContent>
      </Card>

      {/* Node Details Drawer */}
      {selectedNode && (
        <NodeDetailsDrawer
          node={selectedNode}
          open={showDetailsDrawer}
          onOpenChange={setShowDetailsDrawer}
        />
      )}

      {/* Evict Confirmation Dialog */}
      <ConfirmationDialog
        open={showEvictConfirm}
        onOpenChange={setShowEvictConfirm}
        title="Evict Node"
        description={
          nodeToEvict
            ? `Are you sure you want to evict ${nodeToEvict.hostname}? This will remove it from the cluster permanently. Any running workers on this node will be terminated.`
            : ''
        }
        confirmText="Evict Node"
        cancelText="Cancel"
        onConfirm={handleEvictNode}
        variant="destructive"
      />
    </>
  );
}
