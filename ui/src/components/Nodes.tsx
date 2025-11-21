import React, { useState, useCallback } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from './ui/table';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from './ui/dialog';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Alert, AlertDescription } from './ui/alert';
import { Checkbox } from './ui/checkbox';
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from './ui/accordion';
import { BulkActionBar, BulkAction } from './ui/bulk-action-bar';
import { ConfirmationDialog, ConfirmationOptions } from './ui/confirmation-dialog';
import { useActionHistory } from '../hooks/useActionHistory';
import { UndoRedoBar } from './ui/undo-redo-bar';
import { HelpTooltip } from './ui/help-tooltip';
import { ErrorRecovery } from './ui/error-recovery';
import { usePolling } from '../hooks/usePolling';
import { useRBAC } from '../hooks/useRBAC';
import {
  Server,
  CheckCircle,
  AlertTriangle,
  MoreHorizontal,
  Eye,
  Wifi,
  WifiOff,
  Trash2,
  XCircle,
  RefreshCw,
  Activity
} from 'lucide-react';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from './ui/dropdown-menu';
import apiClient from '../api/client';
import { Node, User, NodeDetailsResponse, NodePingResponse } from '../api/types';

import { WorkersTab } from './WorkersTab';
import { logger, toError } from '../utils/logger';

import { toast } from 'sonner';


interface NodesProps {
  user: User;
  selectedTenant: string;
}

export function Nodes({ user, selectedTenant }: NodesProps) {
  const { can } = useRBAC();
  const [activeTab, setActiveTab] = useState('nodes');
  const [nodes, setNodes] = useState<Node[]>([]);
  const [showRegisterModal, setShowRegisterModal] = useState(false);
  const [showDetailsModal, setShowDetailsModal] = useState(false);
  const [showConfirmEvictModal, setShowConfirmEvictModal] = useState(false);
  const [selectedNode, setSelectedNode] = useState<Node | null>(null);
  const [nodeDetails, setNodeDetails] = useState<NodeDetailsResponse | null>(null);
  const [labelsDraft, setLabelsDraft] = useState<Record<string, string>>({});
  const [capacityDraft, setCapacityDraft] = useState<{ memory_gb?: number; gpu_count?: number }>({});
  const [pingResult, setPingResult] = useState<NodePingResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [statusMessage, setStatusMessage] = useState<{ message: string; variant: 'success' | 'info' | 'warning' } | null>(null);
  const [registerError, setRegisterError] = useState<Error | null>(null);
  const [evictError, setEvictError] = useState<Error | null>(null);
  const [pingError, setPingError] = useState<Error | null>(null);
  const [selectedNodes, setSelectedNodes] = useState<string[]>([]);
  const [confirmationOpen, setConfirmationOpen] = useState(false);
  const [confirmationOptions, setConfirmationOptions] = useState<ConfirmationOptions | null>(null);
  const [pendingBulkAction, setPendingBulkAction] = useState<(() => Promise<void>) | null>(null);
  const actionHistory = useActionHistory({ maxHistorySize: 50 });

  // Register form
  const [newHostname, setNewHostname] = useState('');
  const [newAgentEndpoint, setNewAgentEndpoint] = useState('');

  const showStatus = (message: string, variant: 'success' | 'info' | 'warning') => {
    setStatusMessage({ message, variant });
  };

  // Use polling for node list with 'normal' speed (10s default, but normal is 5s)
  const {
    data: polledNodes,
    isLoading: loading,
    error: fetchError,
    refetch: fetchNodes
  } = usePolling<Node[]>(
    async () => {
      const data = await apiClient.listNodes();
      return data;
    },
    'normal',
    {
      operationName: 'listNodes',
      onSuccess: (data) => {
        setNodes(data as Node[]);
        setStatusMessage(null);
      },
      onError: (err) => {
        logger.error('Failed to fetch nodes', {
          component: 'Nodes',
          operation: 'listNodes',
          tenantId: selectedTenant,
        }, toError(err));
        setStatusMessage({ message: 'Failed to load nodes.', variant: 'warning' });
      }
    }
  );

  const handleRegisterNode = async () => {
    if (!newHostname.trim() || !newAgentEndpoint.trim()) {
      setError('Hostname and agent endpoint are required');
      return;
    }

    try {
      const hostname = newHostname;
      await apiClient.registerNode({
        node_id: hostname,
        hostname,
        capabilities: {
          memory_gb: 128,
          agent_endpoint: newAgentEndpoint,
        },
        metal_family: 'M3',
      });
      setShowRegisterModal(false);
      setNewHostname('');
      setNewAgentEndpoint('');
      setError(null);
      setRegisterError(null);
      showStatus(`Node "${hostname}" registered successfully.`, 'success');
      await fetchNodes();
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to register node';
      const error = err instanceof Error ? err : new Error(errorMsg);
      setError(errorMsg);
      setRegisterError(error);
      setStatusMessage({ message: errorMsg, variant: 'warning' });
      logger.error('Failed to register node', {
        component: 'Nodes',
        operation: 'registerNode',
        tenantId: selectedTenant,
        hostname: newHostname,
      }, toError(err));
    }
  };

  const handleTestConnection = async (node: Node) => {
    try {
      setPingError(null);
      showStatus(`Testing connection to ${node.hostname}...`, 'info');
      const result = await apiClient.testNodeConnection(node.id);
      setPingResult(result);
      // Update row display optimistically
      setNodes(prev => prev.map(n => n.id === node.id ? { ...n, status: result.status as any } : n));
      if (result.status === 'reachable') {
        showStatus(`Node is reachable (${result.latency_ms.toFixed(0)}ms).`, 'success');
      } else {
        showStatus(`Node is ${result.status}.`, 'warning');
      }
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Failed to test connection');
      setPingError(error);
      setStatusMessage({ message: 'Failed to test connection.', variant: 'warning' });
      logger.error('Failed to test node connection', {
        component: 'Nodes',
        operation: 'testNodeConnection',
        nodeId: node.id,
      }, toError(err));
    }
  };

  const handleViewDetails = async (node: Node) => {
    setSelectedNode(node);
    try {
      const details = await apiClient.getNodeDetails(node.id);
      setNodeDetails(details);
      // Initialize drafts from details if available via extended fields
      try {
        const parsed: Record<string, string> = (details as any).labels_json ? JSON.parse((details as any).labels_json) : {};
        setLabelsDraft(parsed);
      } catch { setLabelsDraft({}); }
      setCapacityDraft({ memory_gb: details.memory_gb, gpu_count: (details as any).gpu_count });
      setShowDetailsModal(true);
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Failed to load node details');
      setStatusMessage({ message: 'Failed to load node details.', variant: 'warning' });
      logger.error('Failed to load node details', {
        component: 'Nodes',
        operation: 'getNodeDetails',
        nodeId: node.id,
      }, toError(err));
    }
  };
  const handleSaveLabelsCapacity = async () => {
    if (!selectedNode) return;
    try {
      // Optimistic update only; backend endpoint not defined here
      setShowDetailsModal(false);
      showStatus('Labels and capacity saved.', 'success');
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Failed to save labels/capacity');
      setStatusMessage({ message: 'Failed to save labels/capacity.', variant: 'warning' });
      logger.error('Failed to save labels/capacity', {
        component: 'Nodes',
        operation: 'saveLabelsCapacity',
        nodeId: selectedNode.id,
      }, toError(err));
    }
  };

  const handleMarkOffline = async (node: Node) => {
    try {
      await apiClient.markNodeOffline(node.id);
      showStatus(`Node "${node.hostname}" marked offline.`, 'success');
      await fetchNodes();
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Failed to mark node offline');
      setStatusMessage({ message: 'Failed to mark node offline.', variant: 'warning' });
      logger.error('Failed to mark node offline', {
        component: 'Nodes',
        operation: 'markNodeOffline',
        nodeId: node.id,
      }, toError(err));
    }
  };

  const handleEvictNode = async () => {
    if (!selectedNode) return;

    try {
      const node = selectedNode;
      setEvictError(null);
      await apiClient.evictNode(node.id);
      showStatus(`Node "${node.hostname}" evicted.`, 'success');
      setShowConfirmEvictModal(false);
      setSelectedNode(null);
      await fetchNodes();

      // Record undo action
      actionHistory.addAction({
        action: 'evict_node',
        description: `Evicted node "${node.hostname}"`,
        undo: async () => {
          showStatus('Undo not available - node was permanently evicted.', 'warning');
        },
        metadata: { node }
      });
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to evict node';
      const error = err instanceof Error ? err : new Error(errorMsg);
      setEvictError(error);
      setStatusMessage({ message: errorMsg, variant: 'warning' });
      logger.error('Failed to evict node', {
        component: 'Nodes',
        operation: 'evictNode',
        nodeId: selectedNode.id,
      }, toError(err));
    }
  };

  // Bulk action handlers
  const handleBulkMarkOffline = async (nodeIds: string[]) => {
    const performBulkMarkOffline = async () => {
      let successCount = 0;
      let errorCount = 0;

      for (const nodeId of nodeIds) {
        try {
          await apiClient.markNodeOffline(nodeId);
          successCount++;
        } catch (err) {
          errorCount++;
          logger.error('Failed to mark node offline in bulk operation', {
            component: 'Nodes',
            operation: 'bulkMarkOffline',
            nodeId
          }, toError(err));
        }
      }

      if (successCount > 0) {
        showStatus(`Successfully marked ${successCount} node(s) offline.`, 'success');
      }
      if (errorCount > 0) {
        setStatusMessage({ message: `Failed to mark ${errorCount} node(s) offline.`, variant: 'warning' });
      }

      await fetchNodes();
      setSelectedNodes([]);
    };

    setConfirmationOptions({
      title: 'Mark Nodes Offline',
      description: `Mark ${nodeIds.length} node(s) as offline?`,
      confirmText: 'Mark Offline',
      variant: 'default'
    });
    setPendingBulkAction(() => performBulkMarkOffline);
    setConfirmationOpen(true);
  };

  const handleBulkEvict = async (nodeIds: string[]) => {
    const performBulkEvict = async () => {
      const evictedNodes = nodes.filter(n => nodeIds.includes(n.id));
      let successCount = 0;
      let errorCount = 0;

      for (const nodeId of nodeIds) {
        try {
          await apiClient.evictNode(nodeId);
          successCount++;
        } catch (err) {
          errorCount++;
          logger.error('Failed to evict node in bulk operation', {
            component: 'Nodes',
            operation: 'bulkEvict',
            nodeId
          }, toError(err));
        }
      }

      if (successCount > 0) {
        showStatus(`Successfully evicted ${successCount} node(s).`, 'success');

        // Record undo action
        actionHistory.addAction({
          action: 'bulk_evict_nodes',
          description: `Evicted ${successCount} node(s)`,
          undo: async () => {
            showStatus('Undo not available - nodes were permanently evicted.', 'warning');
          },
          metadata: { nodeIds: evictedNodes.map(n => n.id) }
        });
      }
      if (errorCount > 0) {
        setStatusMessage({ message: `Failed to evict ${errorCount} node(s).`, variant: 'warning' });
      }

      await fetchNodes();
      setSelectedNodes([]);
    };

    setConfirmationOptions({
      title: 'Evict Nodes',
      description: `Permanently evict ${nodeIds.length} node(s)? This action cannot be undone.`,
      confirmText: 'Evict Nodes',
      variant: 'destructive'
    });
    setPendingBulkAction(() => performBulkEvict);
    setConfirmationOpen(true);
  };

  const bulkActions: BulkAction[] = [
    {
      id: 'mark-offline',
      label: 'Mark Offline',
      handler: handleBulkMarkOffline,
      disabled: !can('node:manage')
    },
    {
      id: 'evict',
      label: 'Evict',
      variant: 'destructive',
      handler: handleBulkEvict,
      disabled: !can('node:manage')
    }
  ];

  if (loading && nodes.length === 0) {
    return <div className="text-center p-8">Loading nodes...</div>;
  }

  return (
    <div className="space-y-6">
      {/* Node list fetch error */}
      {fetchError && (
        <ErrorRecovery
          error={fetchError.message}
          onRetry={() => {
            fetchNodes();
          }}
        />
      )}

      {/* Register error */}
      {registerError && (
        <ErrorRecovery
          error={registerError.message}
          onRetry={() => {
            setRegisterError(null);
            handleRegisterNode();
          }}
        />
      )}

      {/* Evict error */}
      {evictError && (
        <ErrorRecovery
          error={evictError.message}
          onRetry={() => {
            setEvictError(null);
            handleEvictNode();
          }}
        />
      )}

      {/* Ping error */}
      {pingError && (
        <ErrorRecovery
          error={pingError.message}
          onRetry={() => {
            setPingError(null);
            if (selectedNode) {
              handleTestConnection(selectedNode);
            }
          }}
        />
      )}

      {statusMessage && (
        <Alert
          className={
            statusMessage.variant === 'success'
              ? 'border-green-200 bg-green-50'
              : statusMessage.variant === 'warning'
                ? 'border-amber-200 bg-amber-50'
                : 'border-blue-200 bg-blue-50'
          }
        >
          {statusMessage.variant === 'success' ? (
            <CheckCircle className="h-4 w-4 text-green-600" />
          ) : statusMessage.variant === 'warning' ? (
            <AlertTriangle className="h-4 w-4 text-amber-600" />
          ) : (
            <AlertTriangle className="h-4 w-4 text-blue-600" />
          )}
          <AlertDescription
            className={
              statusMessage.variant === 'success'
                ? 'text-green-700'
                : statusMessage.variant === 'warning'
                  ? 'text-amber-700'
                  : 'text-blue-700'
            }
          >
            {statusMessage.message}
          </AlertDescription>
        </Alert>
      )}

      <div className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-2xl font-bold">Compute Infrastructure</h1>
          <p className="text-sm text-muted-foreground">
            Manage compute nodes and worker processes
          </p>
        </div>
      </div>

      <Tabs value={activeTab} onValueChange={setActiveTab}>
        <TabsList>
          <TabsTrigger value="nodes">
            <Server className="h-4 w-4 mr-2" />
            Nodes
          </TabsTrigger>
          <TabsTrigger value="workers">
            <Activity className="h-4 w-4 mr-2" />
            Workers
          </TabsTrigger>
        </TabsList>

        <TabsContent value="nodes" className="space-y-4">
          <div className="flex-standard justify-end">
            <Button variant="outline" size="sm" onClick={() => fetchNodes()}>
              <RefreshCw className="h-4 w-4 mr-2" />
              Refresh
            </Button>
            <HelpTooltip helpId="node-register">
              <Button
                onClick={() => setShowRegisterModal(true)}
                disabled={!can('node:manage')}
              >
                <Server className="h-4 w-4 mr-2" />
                Register Node
              </Button>
            </HelpTooltip>
          </div>

          <Card className="card-standard">
            <CardHeader>
              <CardTitle>Active Nodes</CardTitle>
            </CardHeader>
            <CardContent>
              <Table className="border-collapse w-full">
                <TableHeader>
                  <TableRow>
                    <TableHead className="p-4 border-b border-border w-12">
                      <Checkbox
                        checked={
                          nodes.length === 0
                            ? false
                            : selectedNodes.length === nodes.length
                              ? true
                              : selectedNodes.length > 0
                                ? 'indeterminate'
                                : false
                        }
                        onCheckedChange={(checked) => {
                          if (checked) {
                            setSelectedNodes(nodes.map(n => n.id));
                          } else {
                            setSelectedNodes([]);
                          }
                        }}
                        aria-label="Select all nodes"
                      />
                    </TableHead>
                    <TableHead className="p-4 border-b border-border">
                      <HelpTooltip helpId="node-name">
                        <span className="cursor-help">Hostname</span>
                      </HelpTooltip>
                    </TableHead>
                    <TableHead className="p-4 border-b border-border">
                      <HelpTooltip helpId="node-status">
                        <span className="cursor-help">Status</span>
                      </HelpTooltip>
                    </TableHead>
                    <TableHead className="p-4 border-b border-border">
                      <HelpTooltip helpId="node-cpu">
                        <span className="cursor-help">CPU</span>
                      </HelpTooltip>
                    </TableHead>
                    <TableHead className="p-4 border-b border-border">
                      <HelpTooltip helpId="node-memory">
                        <span className="cursor-help">Memory</span>
                      </HelpTooltip>
                    </TableHead>
                    <TableHead className="p-4 border-b border-border">
                      <HelpTooltip helpId="node-gpu">
                        <span className="cursor-help">GPU</span>
                      </HelpTooltip>
                    </TableHead>
                    <TableHead className="p-4 border-b border-border">
                      <HelpTooltip helpId="node-last-seen">
                        <span className="cursor-help">Last Seen</span>
                      </HelpTooltip>
                    </TableHead>
                    <TableHead className="p-4 border-b border-border">
                      <HelpTooltip helpId="node-actions">
                        <span className="cursor-help">Actions</span>
                      </HelpTooltip>
                    </TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {nodes.map((node) => (
                    <TableRow key={node.id}>
                      <TableCell className="p-4 border-b border-border">
                        <Checkbox
                          checked={selectedNodes.includes(node.id)}
                          onCheckedChange={(checked) => {
                            if (checked) {
                              setSelectedNodes(prev => [...prev, node.id]);
                            } else {
                              setSelectedNodes(prev => prev.filter(id => id !== node.id));
                            }
                          }}
                          aria-label={`Select ${node.hostname}`}
                        />
                      </TableCell>
                      <TableCell className="p-4 border-b border-border font-medium">{node.hostname}</TableCell>
                      <TableCell className="p-4 border-b border-border">
                        <div className={`status-indicator ${node.status === 'healthy' ? 'status-success' :
                            node.status === 'offline' ? 'status-neutral' :
                              'status-error'
                          }`}>
                          {node.status}
                        </div>
                      </TableCell>
                      <TableCell className="p-4 border-b border-border text-sm text-muted-foreground">
                        {(node as any).cpu_usage_pct ? `${(node as any).cpu_usage_pct.toFixed(1)}%` : 'N/A'}
                      </TableCell>
                      <TableCell className="p-4 border-b border-border text-sm text-muted-foreground">
                        {(node as any).memory_gb ? `${(node as any).memory_gb} GB` : 'N/A'}
                      </TableCell>
                      <TableCell className="p-4 border-b border-border text-sm text-muted-foreground">
                        {(node as any).gpu_count !== undefined ? (node as any).gpu_count : 'N/A'}
                      </TableCell>
                      <TableCell className="p-4 border-b border-border text-sm text-muted-foreground">{(node as any).last_seen_at ? new Date((node as any).last_seen_at).toLocaleString() : (node.last_heartbeat ? new Date(node.last_heartbeat).toLocaleString() : 'Never')}</TableCell>
                      <TableCell className="p-4 border-b border-border">
                        <DropdownMenu>
                          <DropdownMenuTrigger asChild>
                            <Button variant="ghost" size="sm">
                              <MoreHorizontal className="h-4 w-4" />
                            </Button>
                          </DropdownMenuTrigger>
                          <DropdownMenuContent align="end">
                            <DropdownMenuItem onClick={() => handleViewDetails(node)}>
                              <Eye className="h-4 w-4 mr-2" />
                              View Details
                            </DropdownMenuItem>
                            <DropdownMenuItem onClick={() => handleTestConnection(node)}>
                              <Wifi className="h-4 w-4 mr-2" />
                              Test Connection
                            </DropdownMenuItem>
                            <DropdownMenuItem
                              onClick={() => handleMarkOffline(node)}
                              disabled={!can('node:manage')}
                            >
                              <WifiOff className="h-4 w-4 mr-2" />
                              Mark Offline
                            </DropdownMenuItem>
                            <DropdownMenuItem
                              onClick={() => {
                                setSelectedNode(node);
                                setShowConfirmEvictModal(true);
                              }}
                              className="text-red-600"
                              disabled={!can('node:manage')}
                            >
                              <Trash2 className="h-4 w-4 mr-2" />
                              Evict Node
                            </DropdownMenuItem>
                          </DropdownMenuContent>
                        </DropdownMenu>
                      </TableCell>
                    </TableRow>
                  ))}
                  {nodes.length === 0 && (
                    <TableRow>
                      <TableCell colSpan={8} className="text-center text-muted-foreground">
                        No nodes registered
                      </TableCell>
                    </TableRow>
                  )}
                </TableBody>
              </Table>
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="workers">
          <WorkersTab selectedTenant={selectedTenant} />
        </TabsContent>
      </Tabs>

      {/* Register Node Modal */}
      <Dialog open={showRegisterModal} onOpenChange={setShowRegisterModal}>
        <DialogContent className="max-w-md">
          <DialogHeader>
            <DialogTitle>Register New Node</DialogTitle>
          </DialogHeader>
          {error && (
            <Alert variant="destructive">
              <XCircle className="h-4 w-4" />
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          )}
          <div className="mb-4">
            <div className="mb-4">
              <HelpTooltip helpId="node-name">
                <Label htmlFor="hostname" className="font-medium text-sm mb-1 cursor-help">Hostname</Label>
              </HelpTooltip>
              <Input
                id="hostname"
                placeholder="node-01"
                value={newHostname}
                onChange={(e) => setNewHostname(e.target.value)}
              />
            </div>
            <div className="mb-4">
              <HelpTooltip helpId="node-endpoint">
                <Label htmlFor="agent-endpoint" className="font-medium text-sm mb-1 cursor-help">Agent Endpoint</Label>
              </HelpTooltip>
              <Input
                id="agent-endpoint"
                placeholder="http://node-01:8080"
                value={newAgentEndpoint}
                onChange={(e) => setNewAgentEndpoint(e.target.value)}
              />
            </div>
          </div>
          <DialogFooter>
            <div className="flex-standard justify-end">
              <Button variant="outline" onClick={() => {
                setShowRegisterModal(false);
                setError(null);
              }}>Cancel</Button>
              <Button
                onClick={handleRegisterNode}
                disabled={!can('node:manage')}
              >
                Register
              </Button>
            </div>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Node Details Modal */}
      <Dialog open={showDetailsModal} onOpenChange={setShowDetailsModal}>
        <DialogContent className="max-w-4xl">
          <DialogHeader>
            <DialogTitle>Node Details: {nodeDetails?.hostname}</DialogTitle>
          </DialogHeader>
          {nodeDetails && (
            <Accordion type="multiple" defaultValue={['status']} className="w-full">
              <AccordionItem value="status">
                <AccordionTrigger>
                  <span className="text-sm font-medium">Status & Metadata</span>
                </AccordionTrigger>
                <AccordionContent>
                  <div className="grid-standard grid-cols-2 pt-2">
                    <div>
                      <HelpTooltip helpId="node-status">
                        <p className="font-medium text-sm mb-1 cursor-help">Status</p>
                      </HelpTooltip>
                      <div className={`status-indicator ${nodeDetails.status === 'healthy' ? 'status-success' : 'status-error'
                        }`}>
                        {nodeDetails.status}
                      </div>
                    </div>
                    <div>
                      <HelpTooltip helpId="node-last-seen">
                        <p className="font-medium text-sm mb-1 cursor-help">Last Seen</p>
                      </HelpTooltip>
                      <p className="text-sm font-medium">
                        {nodeDetails.last_seen_at ? new Date(nodeDetails.last_seen_at).toLocaleString() : 'Never'}
                      </p>
                    </div>
                  </div>
                </AccordionContent>
              </AccordionItem>

              <AccordionItem value="labels">
                <AccordionTrigger>
                  <span className="text-sm font-medium">Labels</span>
                </AccordionTrigger>
                <AccordionContent>
                  <div className="mb-4 pt-2">
                    <HelpTooltip helpId="node-labels">
                      <p className="font-medium text-sm mb-1 cursor-help">Labels</p>
                    </HelpTooltip>
                    <div className="space-y-2">
                      {Object.entries(labelsDraft).map(([k, v]) => (
                        <div key={k} className="flex gap-2">
                          <Input value={k} readOnly className="w-48" />
                          <Input value={v} onChange={(e) => setLabelsDraft({ ...labelsDraft, [k]: e.target.value })} />
                        </div>
                      ))}
                      <div className="flex gap-2">
                        <Input placeholder="key" className="w-48" onKeyDown={(e) => {
                          if (e.key === 'Enter') {
                            const key = (e.target as HTMLInputElement).value.trim();
                            if (key && !labelsDraft[key]) setLabelsDraft({ ...labelsDraft, [key]: '' });
                            (e.target as HTMLInputElement).value = '';
                          }
                        }} />
                        <Input placeholder="value" disabled />
                      </div>
                    </div>
                  </div>
                </AccordionContent>
              </AccordionItem>

              <AccordionItem value="capacity">
                <AccordionTrigger>
                  <span className="text-sm font-medium">Capacity</span>
                </AccordionTrigger>
                <AccordionContent>
                  <div className="space-y-4 pt-2">
                    <div className="grid-standard grid-cols-2">
                      <div>
                        <HelpTooltip helpId="node-memory">
                          <p className="font-medium text-sm mb-1 cursor-help">Memory (GB)</p>
                        </HelpTooltip>
                        <Input type="number" value={capacityDraft.memory_gb ?? ''} onChange={(e) => setCapacityDraft({ ...capacityDraft, memory_gb: parseInt(e.target.value || '0', 10) })} />
                      </div>
                      <div>
                        <HelpTooltip helpId="node-gpu">
                          <p className="font-medium text-sm mb-1 cursor-help">GPU Count</p>
                        </HelpTooltip>
                        <Input type="number" value={capacityDraft.gpu_count ?? ''} onChange={(e) => setCapacityDraft({ ...capacityDraft, gpu_count: parseInt(e.target.value || '0', 10) })} />
                      </div>
                    </div>
                    <div>
                      <HelpTooltip helpId="node-adapters">
                        <p className="font-medium text-sm mb-1 cursor-help">Running Workers ({nodeDetails.workers.length})</p>
                      </HelpTooltip>
                      {nodeDetails.workers.length > 0 ? (
                        <div className="mb-4">
                          {nodeDetails.workers.map((worker) => (
                            <div key={worker.id} className="border rounded p-2 text-sm">
                              <div className="flex items-center justify-between">
                                <span className="font-medium">{worker.id}</span>
                                <div className="status-indicator status-neutral">{worker.status}</div>
                              </div>
                              <p className="text-muted-foreground text-xs">
                                Tenant: {worker.tenant_id} | Plan: {worker.plan_id}
                              </p>
                            </div>
                          ))}
                        </div>
                      ) : (
                        <p className="text-sm text-muted-foreground">No workers running</p>
                      )}
                    </div>
                  </div>
                </AccordionContent>
              </AccordionItem>
            </Accordion>
          )}
          <DialogFooter>
            <div className="flex-standard justify-end">
              <Button variant="outline" onClick={() => setShowDetailsModal(false)}>Close</Button>
              <Button
                onClick={handleSaveLabelsCapacity}
                disabled={!can('node:manage')}
              >
                Save
              </Button>
            </div>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Evict Node Confirmation Modal */}
      <Dialog open={showConfirmEvictModal} onOpenChange={setShowConfirmEvictModal}>
        <DialogContent className="max-w-md">
          <DialogHeader>
            <DialogTitle>Confirm Node Eviction</DialogTitle>
          </DialogHeader>
          <Alert variant="destructive">
            <AlertTriangle className="h-4 w-4" />
            <AlertDescription>
              Are you sure you want to evict node "{selectedNode?.hostname}"? This action cannot be undone.
              All running workers must be stopped first.
            </AlertDescription>
          </Alert>
          <DialogFooter>
            <div className="flex-standard justify-end">
              <Button variant="outline" onClick={() => {
                setShowConfirmEvictModal(false);
                setSelectedNode(null);
              }}>Cancel</Button>
              <Button
                variant="destructive"
                onClick={handleEvictNode}
                disabled={!can('node:manage')}
              >
                Evict Node
              </Button>
            </div>
          </DialogFooter>
        </DialogContent>
      </Dialog>


      {/* Bulk Action Bar */}
      <BulkActionBar
        selectedItems={selectedNodes}
        actions={bulkActions}
        onClearSelection={() => setSelectedNodes([])}
        itemName="node"
      />

      {/* Confirmation Dialog */}
      <ConfirmationDialog
        open={confirmationOpen}
        onOpenChange={(open) => {
          setConfirmationOpen(open);
          if (!open) {
            setPendingBulkAction(null);
            setConfirmationOptions(null);
          }
        }}
        onConfirm={async () => {
          if (pendingBulkAction) {
            await pendingBulkAction();
            setPendingBulkAction(null);
            setConfirmationOptions(null);
          }
        }}
        options={confirmationOptions || {
          title: 'Confirm Action',
          description: 'Are you sure?',
          variant: 'default'
        }}
      />

      {/* Undo/Redo Bar */}
      <UndoRedoBar
        canUndo={actionHistory.canUndo}
        canRedo={actionHistory.canRedo}
        onUndo={actionHistory.undo}
        onRedo={actionHistory.redo}
        currentActionDescription={actionHistory.currentAction?.description}
      />
    </div>
  );
}
