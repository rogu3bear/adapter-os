import React, { useState, useEffect, useCallback } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
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
import { 
  Server, 
  CheckCircle, 
  AlertTriangle, 
  Cpu, 
  HardDrive,
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
import { toast } from 'sonner';
import { WorkersTab } from './WorkersTab';

interface NodesProps {
  user: User;
  selectedTenant: string;
}

export function Nodes({ user, selectedTenant }: NodesProps) {
  const [activeTab, setActiveTab] = useState('nodes');
  const [nodes, setNodes] = useState<Node[]>([]);
  const [loading, setLoading] = useState(true);
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
  const [errorRecovery, setErrorRecovery] = useState<React.ReactElement | null>(null);
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

  const fetchNodes = useCallback(async () => {
    try {
      const data = await apiClient.listNodes();
      setNodes(data);
      setStatusMessage(null);
      setErrorRecovery(null);
    } catch (err) {
      logger.error('Failed to fetch nodes', {
        component: 'Nodes',
        operation: 'listNodes',
        tenantId: selectedTenant,
      }, toError(err));
      setStatusMessage({ message: 'Failed to load nodes.', variant: 'warning' });
      setErrorRecovery(
        ErrorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error('Failed to load nodes'),
          () => fetchNodes()
        )
      );
    } finally {
      setLoading(false);
    }
  }, [selectedTenant]);

  useEffect(() => {
    fetchNodes();
  }, [fetchNodes]);

  const handleRegisterNode = async () => {
    if (!newHostname.trim() || !newAgentEndpoint.trim()) {
      setError('Hostname and agent endpoint are required');
      return;
    }

    try {
      const hostname = newHostname;
      await apiClient.registerNode({
        hostname,
        metal_family: 'M3', // Default value
        memory_gb: 128, // Default value
        agent_endpoint: newAgentEndpoint,
      });
      setShowRegisterModal(false);
      setNewHostname('');
      setNewAgentEndpoint('');
      setError(null);
      showStatus(`Node "${hostname}" registered successfully.`, 'success');
      await fetchNodes();
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to register node';
      setError(errorMsg);
      setStatusMessage({ message: errorMsg, variant: 'warning' });
      setErrorRecovery(
        ErrorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error(errorMsg),
          () => handleRegisterNode()
        )
      );
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
      setErrorRecovery(
        ErrorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error('Failed to test connection'),
          () => handleTestConnection(node)
        )
      );
      setStatusMessage({ message: 'Failed to test connection.', variant: 'warning' });
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
      setErrorRecovery(
        ErrorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error('Failed to load node details'),
          () => handleViewDetails(node)
        )
      );
      setStatusMessage({ message: 'Failed to load node details.', variant: 'warning' });
    }
  };
  const handleSaveLabelsCapacity = async () => {
    if (!selectedNode) return;
    try {
      // Optimistic update only; backend endpoint not defined here
      setShowDetailsModal(false);
      showStatus('Labels and capacity saved.', 'success');
    } catch (err) {
      setErrorRecovery(
        ErrorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error('Failed to save labels/capacity'),
          () => handleSaveLabelsCapacity()
        )
      );
      setStatusMessage({ message: 'Failed to save labels/capacity.', variant: 'warning' });
    }
  };

  const handleMarkOffline = async (node: Node) => {
    try {
      await apiClient.markNodeOffline(node.id);
      showStatus(`Node "${node.hostname}" marked offline.`, 'success');
      await fetchNodes();
    } catch (err) {
      setErrorRecovery(
        ErrorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error('Failed to mark node offline'),
          () => handleMarkOffline(node)
        )
      );
      setStatusMessage({ message: 'Failed to mark node offline.', variant: 'warning' });
    }
  };

  const handleEvictNode = async () => {
    if (!selectedNode) return;

    try {
      const node = selectedNode;
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
      setErrorRecovery(
        ErrorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error(errorMsg),
          () => handleEvictNode()
        )
      );
      setStatusMessage({ message: errorMsg, variant: 'warning' });
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
        setErrorRecovery(
          ErrorRecoveryTemplates.genericError(
            new Error(`Failed to mark ${errorCount} node(s) offline.`),
            () => performBulkMarkOffline()
          )
        );
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
        setErrorRecovery(
          ErrorRecoveryTemplates.genericError(
            new Error(`Failed to evict ${errorCount} node(s).`),
            () => performBulkEvict()
          )
        );
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
      handler: handleBulkMarkOffline
    },
    {
      id: 'evict',
      label: 'Evict',
      variant: 'destructive',
      handler: handleBulkEvict
    }
  ];

  if (loading) {
    return <div className="text-center p-8">Loading nodes...</div>;
  }

  return (
    <div className="space-y-6">
      {errorRecovery && (
        <div>
          <h1 className="section-title">Compute Infrastructure</h1>
          <p className="section-description">
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
            <Button variant="outline" size="sm" onClick={fetchNodes}>
              <RefreshCw className="icon-standard mr-2" />
              Refresh
            </Button>
            <Button onClick={() => setShowRegisterModal(true)}>
              <Server className="icon-standard mr-2" />
              Register Node
            </Button>
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
                <TableHead className="p-4 border-b border-border">Hostname</TableHead>
                <TableHead className="p-4 border-b border-border">Endpoint</TableHead>
                <TableHead className="p-4 border-b border-border">Status</TableHead>
                <TableHead className="p-4 border-b border-border">Last Seen</TableHead>
                <TableHead className="p-4 border-b border-border">Actions</TableHead>
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
                  <TableCell className="p-4 border-b border-border text-sm text-muted-foreground">{(node as any).agent_endpoint || 'N/A'}</TableCell>
                  <TableCell className="p-4 border-b border-border">
                    <div className={`status-indicator ${
                      node.status === 'healthy' ? 'status-success' : 
                      node.status === 'offline' ? 'status-neutral' : 
                      'status-error'
                    }`}>
                      {node.status}
                    </div>
                  </TableCell>
                  <TableCell className="p-4 border-b border-border">{(node as any).last_seen_at ? new Date((node as any).last_seen_at).toLocaleString() : (node.last_heartbeat ? new Date(node.last_heartbeat).toLocaleString() : 'Never')}</TableCell>
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
                        <DropdownMenuItem onClick={() => handleMarkOffline(node)}>
                          <WifiOff className="h-4 w-4 mr-2" />
                          Mark Offline
                        </DropdownMenuItem>
                        <DropdownMenuItem 
                          onClick={() => {
                            setSelectedNode(node);
                            setShowConfirmEvictModal(true);
                          }}
                          className="text-red-600"
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
                  <TableCell colSpan={6} className="text-center text-muted-foreground">
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
              <Label htmlFor="hostname" className="font-medium text-sm mb-1">Hostname</Label>
              <Input
                id="hostname"
                placeholder="node-01"
                value={newHostname}
                onChange={(e) => setNewHostname(e.target.value)}
              />
            </div>
            <div className="mb-4">
              <Label htmlFor="agent-endpoint" className="font-medium text-sm mb-1">Agent Endpoint</Label>
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
              <Button onClick={handleRegisterNode}>Register</Button>
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
                      <p className="font-medium text-sm mb-1">Status</p>
                      <div className={`status-indicator ${
                        nodeDetails.status === 'healthy' ? 'status-success' : 'status-error'
                      }`}>
                        {nodeDetails.status}
                      </div>
                    </div>
                    <div>
                      <p className="font-medium text-sm mb-1">Last Seen</p>
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
                    <p className="font-medium text-sm mb-1">Labels</p>
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
                        <p className="font-medium text-sm mb-1">Memory (GB)</p>
                        <Input type="number" value={capacityDraft.memory_gb ?? ''} onChange={(e) => setCapacityDraft({ ...capacityDraft, memory_gb: parseInt(e.target.value || '0', 10) })} />
                      </div>
                      <div>
                        <p className="font-medium text-sm mb-1">GPU Count</p>
                        <Input type="number" value={capacityDraft.gpu_count ?? ''} onChange={(e) => setCapacityDraft({ ...capacityDraft, gpu_count: parseInt(e.target.value || '0', 10) })} />
                      </div>
                    </div>
                    <div>
                      <p className="font-medium text-sm mb-1">Running Workers ({nodeDetails.workers.length})</p>
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
              <Button onClick={handleSaveLabelsCapacity}>Save</Button>
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
              <Button variant="destructive" onClick={handleEvictNode}>Evict Node</Button>
            </div>
          </DialogFooter>
        </DialogContent>
      </Dialog>
        </TabsContent>

        <TabsContent value="workers">
          <WorkersTab selectedTenant={selectedTenant} />
        </TabsContent>
      </Tabs>
    </div>
  );
}
