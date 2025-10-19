import React, { useState, useEffect } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Textarea } from './ui/textarea';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from './ui/table';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger, DialogFooter } from './ui/dialog';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Switch } from './ui/switch';
import { Alert, AlertDescription } from './ui/alert';
import { Progress } from './ui/progress';
import { 
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from './ui/dropdown-menu';
import { 
  Plus, 
  Users, 
  Shield, 
  AlertTriangle, 
  CheckCircle, 
  Settings,
  Lock,
  Eye,
  UserCheck,
  Database,
  Network,
  MoreHorizontal,
  Edit,
  Archive,
  Layers,
  BarChart3
} from 'lucide-react';
import apiClient from '../api/client';
import { Tenant as ApiTenant, User, Policy, Adapter, TenantUsageResponse } from '../api/types';
import { toast } from 'sonner';
import { logger } from '../utils/logger';

interface TenantsProps {
  user: User;
  selectedTenant: string;
}

export function Tenants({ user, selectedTenant }: TenantsProps) {
  const [tenants, setTenants] = useState<ApiTenant[]>([]);
  const [loading, setLoading] = useState(true);
  const [showEditModal, setShowEditModal] = useState(false);
  const [showAssignPoliciesModal, setShowAssignPoliciesModal] = useState(false);
  const [showAssignAdaptersModal, setShowAssignAdaptersModal] = useState(false);
  const [showUsageModal, setShowUsageModal] = useState(false);
  const [showArchiveModal, setShowArchiveModal] = useState(false);
  const [selectedTenantForAction, setSelectedTenantForAction] = useState<ApiTenant | null>(null);
  const [editName, setEditName] = useState('');
  const [usageData, setUsageData] = useState<TenantUsageResponse | null>(null);
  const [selectedPolicies, setSelectedPolicies] = useState<string[]>([]);
  const [selectedAdapters, setSelectedAdapters] = useState<string[]>([]);
  const [policies, setPolicies] = useState<Policy[]>([]);
  const [adapters, setAdapters] = useState<Adapter[]>([]);

  const fetchTenants = async () => {
    setLoading(true);
    try {
      const data = await apiClient.listTenants();
      setTenants(data);
    } catch (err) {
      // Replace: console.error('Failed to fetch tenants:', err);
      logger.error('Failed to fetch tenants', {
        component: 'Tenants',
        operation: 'fetchTenants',
        userId: user.id
      }, err instanceof Error ? err : new Error(String(err)));
      toast.error('Failed to load tenants');
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchTenants();
  }, []);

  useEffect(() => {
    const fetchData = async () => {
      try {
        const [policiesData, adaptersData] = await Promise.all([
          apiClient.listPolicies(),
          apiClient.listAdapters(),
        ]);
        setPolicies(policiesData);
        setAdapters(adaptersData);
      } catch (err) {
        // Replace: console.error('Failed to fetch policies/adapters:', err);
        logger.error('Failed to fetch policies/adapters', {
          component: 'Tenants',
          operation: 'fetchPoliciesAdapters',
          userId: user.id
        }, err instanceof Error ? err : new Error(String(err)));
      }
    };
    fetchData();
  }, []);

  const handleEdit = async () => {
    if (!selectedTenantForAction) return;
    try {
      await apiClient.updateTenant(selectedTenantForAction.id, editName);
      toast.success('Tenant updated');
      setShowEditModal(false);
      fetchTenants();
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to update tenant';
      toast.error(errorMsg);
    }
  };


  const handleArchive = async () => {
    if (!selectedTenantForAction) return;
    try {
      await apiClient.archiveTenant(selectedTenantForAction.id);
      toast.success('Tenant archived');
      setShowArchiveModal(false);
      setSelectedTenantForAction(null);
      fetchTenants();
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to archive tenant';
      toast.error(errorMsg);
    }
  };

  const handleAssignPolicies = async () => {
    if (!selectedTenantForAction) return;
    try {
      await apiClient.assignTenantPolicies(selectedTenantForAction.id, selectedPolicies);
      toast.success(`Assigned ${selectedPolicies.length} policies`);
      setShowAssignPoliciesModal(false);
      setSelectedPolicies([]);
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to assign policies';
      toast.error(errorMsg);
    }
  };

  const handleAssignAdapters = async () => {
    if (!selectedTenantForAction) return;
    try {
      await apiClient.assignTenantAdapters(selectedTenantForAction.id, selectedAdapters);
      toast.success(`Assigned ${selectedAdapters.length} adapters`);
      setShowAssignAdaptersModal(false);
      setSelectedAdapters([]);
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to assign adapters';
      toast.error(errorMsg);
    }
  };

  const handleViewUsage = async (tenant: ApiTenant) => {
    try {
      const usage = await apiClient.getTenantUsage(tenant.id);
      setUsageData(usage);
      setSelectedTenantForAction(tenant);
      setShowUsageModal(true);
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to fetch tenant usage';
      toast.error(errorMsg);
    }
  };

  const handlePause = async (tenant: ApiTenant) => {
    try {
      await apiClient.pauseTenant(tenant.id);
      toast.success(`Tenant "${tenant.name}" paused`);
      await fetchTenants();
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to pause tenant';
      toast.error(errorMsg);
    }
  };

  // Mock data removed - using real API data from state

  const [isCreateDialogOpen, setIsCreateDialogOpen] = useState(false);
  const [newTenant, setNewTenant] = useState({
    name: '',
    description: '',
    dataClassification: 'internal' as const,
    itarCompliant: false
  });

  const getStatusBadge = (status?: ApiTenant['status']) => {
    const currentStatus = status || 'active';
    switch (currentStatus) {
      case 'active':
        return <div className="status-indicator status-success"><CheckCircle className="icon-small" />Active</div>;
      case 'suspended':
        return <div className="status-indicator status-error"><AlertTriangle className="icon-small" />Suspended</div>;
      case 'maintenance':
        return <div className="status-indicator status-warning"><Settings className="icon-small" />Maintenance</div>;
      case 'paused':
        return <div className="status-indicator status-neutral"><Lock className="icon-small" />Inactive</div>;
      case 'archived':
        return <div className="status-indicator status-neutral"><Database className="icon-small" />Archived</div>;
      default:
        return <div className="status-indicator status-neutral">Unknown</div>;
    }
  };

  const getClassificationBadge = (classification?: ApiTenant['data_classification']) => {
    const current = classification || 'internal';
    const colors = {
      public: 'status-info',
      internal: 'status-neutral',
      confidential: 'status-warning',
      restricted: 'status-error'
    };
    
    return (
      <div className={`status-indicator ${colors[current]}`}>
        <Lock className="icon-small" />
        {current.toUpperCase()}
      </div>
    );
  };

  const handleCreateTenant = async () => {
    if (!newTenant.name.trim()) return;
    try {
      await apiClient.createTenant({ name: newTenant.name, isolation_level: 'standard' });
      toast.success('Tenant created');
      setNewTenant({ name: '', description: '', dataClassification: 'internal', itarCompliant: false });
      setIsCreateDialogOpen(false);
      await fetchTenants();
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to create tenant';
      toast.error(errorMsg);
    }
  };

  if (user.role !== 'Admin') {
    return (
      <div className="flex items-center justify-center h-96">
        <div className="text-center space-y-2">
          <Shield className="h-12 w-12 text-muted-foreground mx-auto" />
          <h3>Access Restricted</h3>
          <p className="text-muted-foreground">
            Tenant management requires Administrator privileges.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex-between section-header">
        <div>
          <h1 className="section-title">Tenant Management</h1>
          <p className="section-description">
            Manage tenant isolation, data classification, and access controls
          </p>
        </div>
        <Dialog open={isCreateDialogOpen} onOpenChange={setIsCreateDialogOpen}>
          <DialogTrigger asChild>
            <Button>
              <Plus className="icon-standard mr-2" />
              Create Tenant
            </Button>
          </DialogTrigger>
          <DialogContent className="modal-standard">
            <DialogHeader>
              <DialogTitle>Create New Tenant</DialogTitle>
            </DialogHeader>
            <div className="form-field">
              <div className="form-field">
                <Label htmlFor="name" className="form-label">Tenant Name</Label>
                <Input
                  id="name"
                  placeholder="Enter tenant name"
                  value={newTenant.name}
                  onChange={(e) => setNewTenant({ ...newTenant, name: e.target.value })}
                />
              </div>
              
              <div className="form-field">
                <Label htmlFor="description" className="form-label">Description</Label>
                <Textarea
                  id="description"
                  placeholder="Describe the tenant's purpose"
                  value={newTenant.description}
                  onChange={(e) => setNewTenant({ ...newTenant, description: e.target.value })}
                />
              </div>
              
              <div className="form-field">
                <Label htmlFor="classification" className="form-label">Data Classification</Label>
                <Select 
                  value={newTenant.dataClassification} 
                  onValueChange={(value: any) => setNewTenant({ ...newTenant, dataClassification: value })}
                >
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="public">Public</SelectItem>
                    <SelectItem value="internal">Internal</SelectItem>
                    <SelectItem value="confidential">Confidential</SelectItem>
                    <SelectItem value="restricted">Restricted</SelectItem>
                  </SelectContent>
                </Select>
              </div>
              
              <div className="flex-between">
                <Label htmlFor="itar" className="form-label">ITAR Compliance Required</Label>
                <Switch
                  id="itar"
                  checked={newTenant.itarCompliant}
                  onCheckedChange={(checked) => setNewTenant({ ...newTenant, itarCompliant: checked })}
                />
              </div>
              
              <div className="flex-standard justify-end">
                <Button variant="outline" onClick={() => setIsCreateDialogOpen(false)}>
                  Cancel
                </Button>
                <Button onClick={handleCreateTenant} disabled={!newTenant.name.trim()}>
                  Create Tenant
                </Button>
              </div>
            </div>
          </DialogContent>
        </Dialog>
      </div>

      {/* Tenant Statistics */}
      <div className="grid-standard grid-cols-4">
        <Card className="card-standard">
          <CardContent className="pt-6">
            <div className="flex-center">
              <Users className="icon-standard text-blue-600" />
              <div>
                <p className="text-2xl font-bold">{tenants.length}</p>
                <p className="text-xs text-muted-foreground">Total Tenants</p>
              </div>
            </div>
          </CardContent>
        </Card>
        
        <Card className="card-standard">
          <CardContent className="pt-6">
            <div className="flex-center">
              <CheckCircle className="icon-standard text-green-600" />
              <div>
                <p className="text-2xl font-bold">{tenants.filter(t => t.status === 'active').length}</p>
                <p className="text-xs text-muted-foreground">Active</p>
              </div>
            </div>
          </CardContent>
        </Card>
        
        <Card className="card-standard">
          <CardContent className="pt-6">
            <div className="flex-center">
              <Shield className="icon-standard text-orange-600" />
              <div>
                <p className="text-2xl font-bold">{tenants.filter(t => t.itarCompliant).length}</p>
                <p className="text-xs text-muted-foreground">ITAR Compliant</p>
              </div>
            </div>
          </CardContent>
        </Card>
        
        <Card className="card-standard">
          <CardContent className="pt-6">
            <div className="flex-center">
              <Database className="icon-standard text-purple-600" />
              <div>
                <p className="text-2xl font-bold">{tenants.reduce((sum, t) => sum + t.adapters, 0)}</p>
                <p className="text-xs text-muted-foreground">Total Adapters</p>
              </div>
            </div>
          </CardContent>
        </Card>
      </div>

      {/* Tenants Table */}
      <Card className="card-standard">
        <CardHeader>
          <CardTitle>Active Tenants</CardTitle>
        </CardHeader>
        <CardContent>
          <Table className="table-standard">
            <TableHeader>
              <TableRow>
                <TableHead className="table-cell-standard">Tenant</TableHead>
                <TableHead className="table-cell-standard">Status</TableHead>
                <TableHead className="table-cell-standard">Classification</TableHead>
                <TableHead className="table-cell-standard">Users</TableHead>
                <TableHead className="table-cell-standard">Adapters</TableHead>
                <TableHead className="table-cell-standard">Policies</TableHead>
                <TableHead className="table-cell-standard">ITAR</TableHead>
                <TableHead className="table-cell-standard">Last Activity</TableHead>
                <TableHead className="table-cell-standard">Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {tenants.map((tenant) => (
                <TableRow key={tenant.id}>
                  <TableCell className="table-cell-standard">
                    <div>
                      <p className="font-medium">{tenant.name}</p>
                      <p className="text-sm text-muted-foreground">{tenant.description || 'No description'}</p>
                    </div>
                  </TableCell>
                  <TableCell className="table-cell-standard">{getStatusBadge(tenant.status)}</TableCell>
                  <TableCell className="table-cell-standard">{getClassificationBadge(tenant.data_classification)}</TableCell>
                  <TableCell className="table-cell-standard">
                    <div className="flex-center">
                      <UserCheck className="icon-small text-muted-foreground" />
                      <span>{tenant.users || 0}</span>
                    </div>
                  </TableCell>
                  <TableCell className="table-cell-standard">
                    <div className="flex-center">
                      <Network className="icon-small text-muted-foreground" />
                      <span>{tenant.adapters || 0}</span>
                    </div>
                  </TableCell>
                  <TableCell className="table-cell-standard">
                    <div className="flex-center">
                      <Shield className="icon-small text-muted-foreground" />
                      <span>{tenant.policies || 0}</span>
                    </div>
                  </TableCell>
                  <TableCell className="table-cell-standard">
                    {tenant.itar_compliant ? (
                      <div className="status-indicator status-success">Yes</div>
                    ) : (
                      <div className="status-indicator status-neutral">No</div>
                    )}
                  </TableCell>
                  <TableCell className="table-cell-standard text-sm text-muted-foreground">
                    {tenant.last_activity || 'Unknown'}
                  </TableCell>
                  <TableCell className="table-cell-standard">
                    <DropdownMenu>
                      <DropdownMenuTrigger asChild>
                        <Button variant="ghost" size="sm">
                          <MoreHorizontal className="h-4 w-4" />
                        </Button>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent align="end">
                        {tenant.status !== 'paused' && tenant.status !== 'archived' && (
                          <DropdownMenuItem onClick={() => handlePause(tenant)}>
                            <Lock className="mr-2 h-4 w-4" />
                            Pause
                          </DropdownMenuItem>
                        )}
                        <DropdownMenuItem onClick={() => {
                          setSelectedTenantForAction(tenant);
                          setEditName(tenant.name);
                          setShowEditModal(true);
                        }}>
                          <Edit className="mr-2 h-4 w-4" />
                          Edit
                        </DropdownMenuItem>
                        <DropdownMenuItem onClick={() => {
                          setSelectedTenantForAction(tenant);
                          setShowAssignPoliciesModal(true);
                        }}>
                          <Shield className="mr-2 h-4 w-4" />
                          Assign Policies
                        </DropdownMenuItem>
                        <DropdownMenuItem onClick={() => {
                          setSelectedTenantForAction(tenant);
                          setShowAssignAdaptersModal(true);
                        }}>
                          <Layers className="mr-2 h-4 w-4" />
                          Assign Adapters
                        </DropdownMenuItem>
                        <DropdownMenuItem onClick={() => handleViewUsage(tenant)}>
                          <BarChart3 className="mr-2 h-4 w-4" />
                          View Usage
                        </DropdownMenuItem>
                        <DropdownMenuItem onClick={() => {
                          setSelectedTenantForAction(tenant);
                          setShowArchiveModal(true);
                        }}>
                          <Archive className="mr-2 h-4 w-4 text-red-600" />
                          Archive
                        </DropdownMenuItem>
                      </DropdownMenuContent>
                    </DropdownMenu>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </CardContent>
      </Card>

      {/* Edit Tenant Modal */}
      <Dialog open={showEditModal} onOpenChange={setShowEditModal}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Edit Tenant</DialogTitle>
          </DialogHeader>
          <div className="space-y-4">
            <div>
              <Label>Tenant Name</Label>
              <Input 
                value={editName} 
                onChange={(e) => setEditName(e.target.value)}
                placeholder="Enter tenant name"
              />
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setShowEditModal(false)}>
              Cancel
            </Button>
            <Button onClick={handleEdit}>Save Changes</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Assign Policies Modal */}
      <Dialog open={showAssignPoliciesModal} onOpenChange={setShowAssignPoliciesModal}>
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle>Assign Policies to {selectedTenantForAction?.name}</DialogTitle>
          </DialogHeader>
          <div className="space-y-4 max-h-96 overflow-y-auto">
            {policies.map((policy) => (
              <div key={policy.cpid} className="flex items-center space-x-2 p-2 border rounded">
                <input 
                  type="checkbox" 
                  id={`policy-${policy.cpid}`}
                  checked={selectedPolicies.includes(policy.cpid)}
                  onChange={(e) => {
                    if (e.target.checked) {
                      setSelectedPolicies([...selectedPolicies, policy.cpid]);
                    } else {
                      setSelectedPolicies(selectedPolicies.filter(id => id !== policy.cpid));
                    }
                  }}
                  className="h-4 w-4"
                />
                <label htmlFor={`policy-${policy.cpid}`} className="flex-1 cursor-pointer">
                  <p className="font-medium">{policy.cpid}</p>
                  <p className="text-xs text-muted-foreground">Hash: {policy.schema_hash.substring(0, 16)}</p>
                </label>
              </div>
            ))}
            {policies.length === 0 && (
              <p className="text-center text-muted-foreground">No policies available</p>
            )}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => {
              setShowAssignPoliciesModal(false);
              setSelectedPolicies([]);
            }}>
              Cancel
            </Button>
            <Button onClick={handleAssignPolicies} disabled={selectedPolicies.length === 0}>
              Assign {selectedPolicies.length} Policies
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Assign Adapters Modal */}
      <Dialog open={showAssignAdaptersModal} onOpenChange={setShowAssignAdaptersModal}>
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle>Assign Adapters to {selectedTenantForAction?.name}</DialogTitle>
          </DialogHeader>
          <div className="space-y-4 max-h-96 overflow-y-auto">
            {adapters.map((adapter) => (
              <div key={adapter.id} className="flex items-center space-x-2 p-2 border rounded">
                <input 
                  type="checkbox" 
                  id={`adapter-${adapter.id}`}
                  checked={selectedAdapters.includes(adapter.id)}
                  onChange={(e) => {
                    if (e.target.checked) {
                      setSelectedAdapters([...selectedAdapters, adapter.id]);
                    } else {
                      setSelectedAdapters(selectedAdapters.filter(id => id !== adapter.id));
                    }
                  }}
                  className="h-4 w-4"
                />
                <label htmlFor={`adapter-${adapter.id}`} className="flex-1 cursor-pointer">
                  <p className="font-medium">{adapter.name}</p>
                  <p className="text-xs text-muted-foreground">Rank: {adapter.rank}</p>
                </label>
              </div>
            ))}
            {adapters.length === 0 && (
              <p className="text-center text-muted-foreground">No adapters available</p>
            )}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => {
              setShowAssignAdaptersModal(false);
              setSelectedAdapters([]);
            }}>
              Cancel
            </Button>
            <Button onClick={handleAssignAdapters} disabled={selectedAdapters.length === 0}>
              Assign {selectedAdapters.length} Adapters
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* View Usage Modal */}
      <Dialog open={showUsageModal} onOpenChange={setShowUsageModal}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Tenant Usage - {selectedTenantForAction?.name}</DialogTitle>
          </DialogHeader>
          {usageData && (
            <div className="space-y-4">
              <div>
                <Label>CPU Usage</Label>
                <Progress value={usageData.cpu_usage_pct} className="mt-2" />
                <p className="text-sm text-muted-foreground mt-1">{usageData.cpu_usage_pct.toFixed(1)}%</p>
              </div>
              <div>
                <Label>GPU Usage</Label>
                <Progress value={usageData.gpu_usage_pct} className="mt-2" />
                <p className="text-sm text-muted-foreground mt-1">{usageData.gpu_usage_pct.toFixed(1)}%</p>
              </div>
              <div>
                <Label>Memory Usage</Label>
                <p className="text-sm">{usageData.memory_used_gb.toFixed(2)} GB / {usageData.memory_total_gb.toFixed(2)} GB</p>
                <Progress value={(usageData.memory_used_gb / usageData.memory_total_gb) * 100} className="mt-2" />
              </div>
              <div>
                <Label>Inference Count (24h)</Label>
                <p className="text-lg font-medium">{usageData.inference_count_24h.toLocaleString()}</p>
              </div>
              <div>
                <Label>Active Adapters</Label>
                <p>{usageData.active_adapters_count}</p>
              </div>
            </div>
          )}
          <DialogFooter>
            {usageData && (
              <Button
                variant="outline"
                onClick={() => {
                  const rows = [
                    ['cpu_usage_pct', usageData.cpu_usage_pct.toFixed(1)],
                    ['gpu_usage_pct', usageData.gpu_usage_pct.toFixed(1)],
                    ['memory_used_gb', usageData.memory_used_gb.toFixed(2)],
                    ['memory_total_gb', usageData.memory_total_gb.toFixed(2)],
                    ['inference_count_24h', usageData.inference_count_24h.toString()],
                    ['active_adapters_count', usageData.active_adapters_count.toString()],
                  ];
                  const csv = 'key,value\n' + rows.map(r => r.join(',')).join('\n');
                  const blob = new Blob([csv], { type: 'text/csv' });
                  const url = URL.createObjectURL(blob);
                  const a = document.createElement('a');
                  a.href = url;
                  a.download = `tenant-usage-${selectedTenantForAction?.id}.csv`;
                  a.click();
                  URL.revokeObjectURL(url);
                }}
              >
                Export CSV
              </Button>
            )}
            <Button onClick={() => setShowUsageModal(false)}>Close</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Archive Tenant Modal */}
      <Dialog open={showArchiveModal} onOpenChange={setShowArchiveModal}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Archive Tenant</DialogTitle>
          </DialogHeader>
          <Alert variant="destructive">
            <AlertTriangle className="h-4 w-4" />
            <AlertDescription>
              This will archive tenant <strong>{selectedTenantForAction?.name}</strong>. 
              All associated resources will be suspended. This action can be reversed by an administrator.
            </AlertDescription>
          </Alert>
          <DialogFooter>
            <Button variant="outline" onClick={() => {
              setShowArchiveModal(false);
              setSelectedTenantForAction(null);
            }}>
              Cancel
            </Button>
            <Button variant="destructive" onClick={handleArchive}>
              Archive Tenant
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}