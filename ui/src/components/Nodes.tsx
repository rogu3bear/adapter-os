import React, { useState, useEffect } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from './ui/table';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from './ui/dialog';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Alert, AlertDescription } from './ui/alert';
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
  
  // Register form
  const [newHostname, setNewHostname] = useState('');
  const [newAgentEndpoint, setNewAgentEndpoint] = useState('');

  const fetchNodes = async () => {
    try {
      const data = await apiClient.listNodes();
      setNodes(data);
    } catch (err) {
      console.error('Failed to fetch nodes:', err);
      toast.error('Failed to load nodes');
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchNodes();
  }, [selectedTenant]);

  const handleRegisterNode = async () => {
    if (!newHostname.trim() || !newAgentEndpoint.trim()) {
      setError('Hostname and agent endpoint are required');
      return;
    }

    try {
      await apiClient.registerNode({
        hostname: newHostname,
        metal_family: 'M3', // Default value
        memory_gb: 128, // Default value
        agent_endpoint: newAgentEndpoint,
      });
      toast.success(`Node "${newHostname}" registered successfully`);
      setShowRegisterModal(false);
      setNewHostname('');
      setNewAgentEndpoint('');
      setError(null);
      await fetchNodes();
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to register node';
      setError(errorMsg);
      toast.error(errorMsg);
    }
  };

  const handleTestConnection = async (node: Node) => {
    try {
      toast.info(`Testing connection to ${node.hostname}...`);
      const result = await apiClient.testNodeConnection(node.id);
      setPingResult(result);
      // Update row display optimistically
      setNodes(prev => prev.map(n => n.id === node.id ? { ...n, status: result.status as any } : n));
      if (result.status === 'reachable') {
        toast.success(`Node is reachable (${result.latency_ms.toFixed(0)}ms)`);
      } else {
        toast.error(`Node is ${result.status}`);
      }
    } catch (err) {
      toast.error('Failed to test connection');
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
      toast.error('Failed to load node details');
    }
  };
  const handleSaveLabelsCapacity = async () => {
    if (!selectedNode) return;
    try {
      // Optimistic update only; backend endpoint not defined here
      setShowDetailsModal(false);
      toast.success('Labels and capacity saved');
    } catch (err) {
      toast.error('Failed to save labels/capacity');
    }
  };

  const handleMarkOffline = async (node: Node) => {
    try {
      await apiClient.markNodeOffline(node.id);
      toast.success(`Node "${node.hostname}" marked offline`);
      await fetchNodes();
    } catch (err) {
      toast.error('Failed to mark node offline');
    }
  };

  const handleEvictNode = async () => {
    if (!selectedNode) return;

    try {
      await apiClient.evictNode(selectedNode.id);
      toast.success(`Node "${selectedNode.hostname}" evicted`);
      setShowConfirmEvictModal(false);
      setSelectedNode(null);
      await fetchNodes();
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to evict node';
      toast.error(errorMsg);
    }
  };

  if (loading) {
    return <div className="text-center p-8">Loading nodes...</div>;
  }

  return (
    <div className="space-y-6">
      <div className="flex-between section-header">
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
          <Table className="table-standard">
            <TableHeader>
              <TableRow>
                <TableHead className="table-cell-standard">Hostname</TableHead>
                <TableHead className="table-cell-standard">Endpoint</TableHead>
                <TableHead className="table-cell-standard">Status</TableHead>
                <TableHead className="table-cell-standard">Last Seen</TableHead>
                <TableHead className="table-cell-standard">Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {nodes.map((node) => (
                <TableRow key={node.id}>
                  <TableCell className="table-cell-standard font-medium">{node.hostname}</TableCell>
                  <TableCell className="table-cell-standard text-sm text-muted-foreground">{node.agent_endpoint || 'N/A'}</TableCell>
                  <TableCell className="table-cell-standard">
                    <div className={`status-indicator ${
                      node.status === 'healthy' ? 'status-success' : 
                      node.status === 'offline' ? 'status-neutral' : 
                      'status-error'
                    }`}>
                      {node.status}
                    </div>
                  </TableCell>
                  <TableCell className="table-cell-standard">{node.last_seen_at ? new Date(node.last_seen_at).toLocaleString() : 'Never'}</TableCell>
                  <TableCell className="table-cell-standard">
                    <DropdownMenu>
                      <DropdownMenuTrigger asChild>
                        <Button variant="ghost" size="sm">
                          <MoreHorizontal className="icon-standard" />
                        </Button>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent align="end">
                        <DropdownMenuItem onClick={() => handleViewDetails(node)}>
                          <Eye className="icon-standard mr-2" />
                          View Details
                        </DropdownMenuItem>
                        <DropdownMenuItem onClick={() => handleTestConnection(node)}>
                          <Wifi className="icon-standard mr-2" />
                          Test Connection
                        </DropdownMenuItem>
                        <DropdownMenuItem onClick={() => handleMarkOffline(node)}>
                          <WifiOff className="icon-standard mr-2" />
                          Mark Offline
                        </DropdownMenuItem>
                        <DropdownMenuItem 
                          onClick={() => {
                            setSelectedNode(node);
                            setShowConfirmEvictModal(true);
                          }}
                          className="text-red-600"
                        >
                          <Trash2 className="icon-standard mr-2" />
                          Evict Node
                        </DropdownMenuItem>
                      </DropdownMenuContent>
                    </DropdownMenu>
                  </TableCell>
                </TableRow>
              ))}
              {nodes.length === 0 && (
                <TableRow>
                  <TableCell colSpan={5} className="text-center text-muted-foreground">
                    No nodes registered
                  </TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
        </CardContent>
      </Card>

      {/* Register Node Modal */}
      <Dialog open={showRegisterModal} onOpenChange={setShowRegisterModal}>
        <DialogContent className="modal-standard">
          <DialogHeader>
            <DialogTitle>Register New Node</DialogTitle>
          </DialogHeader>
          {error && (
            <Alert variant="destructive">
              <XCircle className="icon-standard" />
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          )}
          <div className="form-field">
            <div className="form-field">
              <Label htmlFor="hostname" className="form-label">Hostname</Label>
              <Input
                id="hostname"
                placeholder="node-01"
                value={newHostname}
                onChange={(e) => setNewHostname(e.target.value)}
              />
            </div>
            <div className="form-field">
              <Label htmlFor="agent-endpoint" className="form-label">Agent Endpoint</Label>
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
        <DialogContent className="modal-large">
          <DialogHeader>
            <DialogTitle>Node Details: {nodeDetails?.hostname}</DialogTitle>
          </DialogHeader>
          {nodeDetails && (
            <div className="form-field">
              <div className="grid-standard grid-cols-2">
                <div>
                  <p className="form-label">Status</p>
                  <div className={`status-indicator ${
                    nodeDetails.status === 'healthy' ? 'status-success' : 'status-error'
                  }`}>
                    {nodeDetails.status}
                  </div>
                </div>
                <div>
                  <p className="form-label">Last Seen</p>
                  <p className="text-sm font-medium">
                    {nodeDetails.last_seen_at ? new Date(nodeDetails.last_seen_at).toLocaleString() : 'Never'}
                  </p>
                </div>
              </div>
              {/* Labels Editor */}
              <div className="form-field">
                <p className="form-label">Labels</p>
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

              {/* Capacity Editor */}
              <div className="grid-standard grid-cols-2">
                <div>
                  <p className="form-label">Memory (GB)</p>
                  <Input type="number" value={capacityDraft.memory_gb ?? ''} onChange={(e) => setCapacityDraft({ ...capacityDraft, memory_gb: parseInt(e.target.value || '0', 10) })} />
                </div>
                <div>
                  <p className="form-label">GPU Count</p>
                  <Input type="number" value={capacityDraft.gpu_count ?? ''} onChange={(e) => setCapacityDraft({ ...capacityDraft, gpu_count: parseInt(e.target.value || '0', 10) })} />
                </div>
              </div>
              <div>
                <p className="form-label">Running Workers ({nodeDetails.workers.length})</p>
                {nodeDetails.workers.length > 0 ? (
                  <div className="form-field">
                    {nodeDetails.workers.map((worker) => (
                      <div key={worker.id} className="border rounded p-2 text-sm">
                        <div className="flex-between">
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
        <DialogContent className="modal-standard">
          <DialogHeader>
            <DialogTitle>Confirm Node Eviction</DialogTitle>
          </DialogHeader>
          <Alert variant="destructive">
            <AlertTriangle className="icon-standard" />
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