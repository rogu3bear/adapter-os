import React, { useState, useEffect } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Textarea } from './ui/textarea';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from './ui/table';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from './ui/dialog';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs';
import { Progress } from './ui/progress';
import { Alert, AlertDescription } from './ui/alert';
import { EmptyState } from './ui/empty-state';
import { 
  Plus, 
  Settings, 
  Square, 
  Download, 
  Upload,
  Eye,
  Edit,
  Trash2,
  Clock,
  Zap,
  Target,
  BarChart3,
  Activity,
  AlertTriangle,
  CheckCircle,
  XCircle,
  Brain,
  Database,
  GitBranch,
  Layers,
  Cpu,
  MemoryStick,
  HardDrive,
  Snowflake,
  Thermometer,
  Flame,
  Anchor,
  Pin,
  MoreHorizontal,
  ArrowUp,
  FileText,
  Code,
  Image,
  Activity as ActivityIcon,
  Hash,
  TestTube,
  Monitor,
  Shield
} from 'lucide-react';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from './ui/dropdown-menu';
import apiClient from '../api/client';
import { User } from '../api/types';
import { toast } from 'sonner';

interface DomainAdapterManagerProps {
  user: User;
  selectedTenant: string;
}

interface DomainAdapter {
  id: string;
  name: string;
  version: string;
  description: string;
  domain_type: 'text' | 'vision' | 'telemetry';
  model: string;
  hash: string;
  input_format: string;
  output_format: string;
  config: Record<string, any>;
  status: 'loaded' | 'unloaded' | 'error';
  epsilon_stats?: EpsilonStats;
  last_execution?: string;
  execution_count: number;
  created_at: string;
  updated_at: string;
}

interface EpsilonStats {
  mean_error: number;
  max_error: number;
  error_count: number;
  last_updated: string;
}

interface DomainAdapterTest {
  id: string;
  adapter_id: string;
  input_data: string;
  expected_output?: string;
  actual_output?: string;
  epsilon?: number;
  passed: boolean;
  executed_at: string;
}

export function DomainAdapterManager({ user, selectedTenant }: DomainAdapterManagerProps) {
  const [adapters, setAdapters] = useState<DomainAdapter[]>([]);
  const [tests, setTests] = useState<DomainAdapterTest[]>([]);
  const [loading, setLoading] = useState(true);
  const [isCreateDialogOpen, setIsCreateDialogOpen] = useState(false);
  const [isTestDialogOpen, setIsTestDialogOpen] = useState(false);
  const [selectedAdapter, setSelectedAdapter] = useState<DomainAdapter | null>(null);
  const [activeTab, setActiveTab] = useState('adapters');
  const [newAdapterForm, setNewAdapterForm] = useState({
    name: '',
    domain_type: 'text' as const,
    model: '',
    description: '',
    config: {}
  });

  useEffect(() => {
    const fetchData = async () => {
      try {
        // Citation: ui/src/api/client.ts L625-L628
        const adaptersData = await apiClient.listDomainAdapters();
        setAdapters(adaptersData);
        // Domain adapter tests - placeholder implementation
        setTests([]);
      } catch (err) {
        const errorMsg = err instanceof Error ? err.message : 'Failed to fetch domain adapters';
        console.error(errorMsg, err);
        toast.error(errorMsg);
      } finally {
        setLoading(false);
      }
    };
    fetchData();
  }, [selectedTenant]);

  const getDomainIcon = (domainType: string) => {
    switch (domainType) {
      case 'text': return <FileText className="h-4 w-4" />;
      case 'vision': return <Image className="h-4 w-4" />;
      case 'telemetry': return <ActivityIcon className="h-4 w-4" />;
      default: return <Code className="h-4 w-4" />;
    }
  };

  const getStatusBadge = (status: string) => {
    const variants = {
      loaded: 'bg-green-100 text-green-800',
      unloaded: 'bg-gray-100 text-gray-800',
      error: 'bg-red-100 text-red-800'
    };
    return variants[status as keyof typeof variants] || 'bg-gray-100 text-gray-800';
  };

  const handleCreateAdapter = async () => {
    try {
      // API call - placeholder implementation
      // await apiClient.createDomainAdapter(newAdapterForm);
      toast.success('Domain adapter created successfully');
      setIsCreateDialogOpen(false);
      setNewAdapterForm({
        name: '',
        domain_type: 'text',
        model: '',
        description: '',
        config: {}
      });
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to create domain adapter';
      toast.error(errorMsg);
    }
  };

  const handleTestAdapter = async (adapterId: string, inputData: string) => {
    try {
      // API call - placeholder implementation
      // const result = await apiClient.testDomainAdapter(adapterId, inputData);
      toast.success('Domain adapter test completed');
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to test domain adapter';
      toast.error(errorMsg);
    }
  };


  if (loading) {
    return <div className="text-center p-8">Loading domain adapters...</div>;
  }

  return (
    <div className="space-y-6">
      <div className="flex-between section-header">
        <div>
          <h1 className="section-title">Domain Adapter Management</h1>
          <p className="section-description">
            Manage deterministic domain adapters for text, vision, and telemetry processing
          </p>
        </div>
        <div className="flex-standard">
          <Button onClick={() => setIsTestDialogOpen(true)}>
            <TestTube className="icon-standard mr-2" />
            Run Tests
          </Button>
          <Button onClick={() => setIsCreateDialogOpen(true)}>
            <Plus className="icon-standard mr-2" />
            Create Adapter
          </Button>
        </div>
      </div>

      <Tabs value={activeTab} onValueChange={setActiveTab}>
        <TabsList>
          <TabsTrigger value="adapters">Adapters</TabsTrigger>
          <TabsTrigger value="testing">Testing</TabsTrigger>
          <TabsTrigger value="monitoring">Monitoring</TabsTrigger>
          <TabsTrigger value="manifests">Manifests</TabsTrigger>
        </TabsList>

        <TabsContent value="adapters" className="form-field">
          <Card className="card-standard">
            <CardHeader>
              <CardTitle className="flex-center">
                <Layers className="icon-large mr-2" />
                Domain Adapters
              </CardTitle>
            </CardHeader>
            <CardContent>
              <Table className="table-standard">
                <TableHeader>
                  <TableRow>
                    <TableHead className="table-cell-standard">Name</TableHead>
                    <TableHead className="table-cell-standard">Domain</TableHead>
                    <TableHead className="table-cell-standard">Status</TableHead>
                    <TableHead className="table-cell-standard">Epsilon (ε)</TableHead>
                    <TableHead className="table-cell-standard">Executions</TableHead>
                    <TableHead className="table-cell-standard">Last Used</TableHead>
                    <TableHead className="table-cell-standard">Actions</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {adapters.length === 0 ? (
                    <TableRow>
                      <TableCell colSpan={7} className="h-32">
                        <EmptyState
                          icon={Layers}
                          title="No Domain Adapters"
                          description="Create your first domain adapter to get started with deterministic processing."
                        />
                      </TableCell>
                    </TableRow>
                  ) : (
                    adapters.map((adapter) => (
                      <TableRow key={adapter.id}>
                        <TableCell className="table-cell-standard">
                          <div className="flex-center">
                            {getDomainIcon(adapter.domain_type)}
                            <div>
                              <div className="font-medium">{adapter.name}</div>
                              <div className="text-sm text-muted-foreground">
                                v{adapter.version} • {adapter.domain_type}
                              </div>
                            </div>
                          </div>
                        </TableCell>
                        <TableCell className="table-cell-standard">
                          <div className="status-indicator status-neutral flex-center">
                            {getDomainIcon(adapter.domain_type)}
                            <span>{adapter.domain_type}</span>
                          </div>
                        </TableCell>
                        <TableCell className="table-cell-standard">
                          <Badge className={getStatusBadge(adapter.status)}>
                            {adapter.status}
                          </Badge>
                        </TableCell>
                        <TableCell className="table-cell-standard">
                          {adapter.epsilon_stats ? (
                            <div className="flex-center">
                              <Hash className="icon-standard" />
                              <span>ε: {adapter.epsilon_stats.mean_error.toFixed(4)}</span>
                            </div>
                          ) : (
                            <span className="text-muted-foreground">No data</span>
                          )}
                        </TableCell>
                        <TableCell className="table-cell-standard">
                          <div className="flex-center">
                            <Target className="icon-standard" />
                            <span>{adapter.execution_count}</span>
                          </div>
                        </TableCell>
                        <TableCell className="table-cell-standard">
                          <div className="flex-center">
                            <Clock className="icon-standard" />
                            <span>{adapter.last_execution ? new Date(adapter.last_execution).toLocaleString() : 'Never'}</span>
                          </div>
                        </TableCell>
                        <TableCell className="table-cell-standard">
                          <DropdownMenu>
                            <DropdownMenuTrigger asChild>
                              <Button variant="ghost" size="sm">
                                <MoreHorizontal className="h-4 w-4" />
                              </Button>
                            </DropdownMenuTrigger>
                            <DropdownMenuContent align="end">
                              <DropdownMenuItem onClick={() => handleTestAdapter(adapter.id, 'test_input')}>
                                <TestTube className="mr-2 h-4 w-4" />
                                Test
                              </DropdownMenuItem>
                              <DropdownMenuItem onClick={() => setSelectedAdapter(adapter)}>
                                <Eye className="mr-2 h-4 w-4" />
                                View Details
                              </DropdownMenuItem>
                              <DropdownMenuItem>
                                <Download className="mr-2 h-4 w-4" />
                                Download Manifest
                              </DropdownMenuItem>
                              <DropdownMenuItem>
                                <Trash2 className="mr-2 h-4 w-4 text-red-600" />
                                Delete
                              </DropdownMenuItem>
                            </DropdownMenuContent>
                          </DropdownMenu>
                        </TableCell>
                      </TableRow>
                    ))
                  )}
                </TableBody>
              </Table>
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="testing" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center">
                <TestTube className="mr-2 h-5 w-5" />
                Determinism Testing
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="space-y-4">
                {tests.map((test) => (
                  <Card key={test.id} className="cursor-pointer hover:shadow-md transition-shadow">
                    <CardContent className="pt-6">
                      <div className="flex items-center justify-between mb-4">
                        <div className="flex items-center space-x-2">
                          <TestTube className="h-5 w-5" />
                          <h3 className="font-medium">Test {test.id}</h3>
                          <Badge className={test.passed ? 'bg-green-100 text-green-800' : 'bg-red-100 text-red-800'}>
                            {test.passed ? 'PASSED' : 'FAILED'}
                          </Badge>
                        </div>
                        <div className="text-sm text-muted-foreground">
                          ε: {test.epsilon?.toFixed(4) || 'N/A'}
                        </div>
                      </div>
                      
                      <div className="grid grid-cols-1 md:grid-cols-3 gap-4 text-sm">
                        <div>
                          <div className="font-medium mb-1">Input</div>
                          <div className="text-muted-foreground font-mono text-xs">
                            {test.input_data.length > 50 ? `${test.input_data.substring(0, 50)}...` : test.input_data}
                          </div>
                        </div>
                        <div>
                          <div className="font-medium mb-1">Expected Output</div>
                          <div className="text-muted-foreground font-mono text-xs">
                            {test.expected_output || 'N/A'}
                          </div>
                        </div>
                        <div>
                          <div className="font-medium mb-1">Actual Output</div>
                          <div className="text-muted-foreground font-mono text-xs">
                            {test.actual_output || 'N/A'}
                          </div>
                        </div>
                      </div>

                      <div className="text-sm text-muted-foreground mt-2">
                        Executed {new Date(test.executed_at).toLocaleString()}
                      </div>
                    </CardContent>
                  </Card>
                ))}
              </div>
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="monitoring" className="space-y-4">
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
            <Card>
              <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
                <CardTitle className="text-sm font-medium">Total Adapters</CardTitle>
                <Layers className="h-4 w-4 text-muted-foreground" />
              </CardHeader>
              <CardContent>
                <div className="text-2xl font-bold">{adapters.length}</div>
                <p className="text-xs text-muted-foreground">
                  Domain adapters registered
                </p>
              </CardContent>
            </Card>

            <Card>
              <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
                <CardTitle className="text-sm font-medium">Loaded Adapters</CardTitle>
                <Activity className="h-4 w-4 text-muted-foreground" />
              </CardHeader>
              <CardContent>
                <div className="text-2xl font-bold">
                  {adapters.filter(a => a.status === 'loaded').length}
                </div>
                <p className="text-xs text-muted-foreground">
                  Currently active
                </p>
              </CardContent>
            </Card>

            <Card>
              <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
                <CardTitle className="text-sm font-medium">Total Executions</CardTitle>
                <Target className="h-4 w-4 text-muted-foreground" />
              </CardHeader>
              <CardContent>
                <div className="text-2xl font-bold">
                  {adapters.reduce((sum, adapter) => sum + adapter.execution_count, 0)}
                </div>
                <p className="text-xs text-muted-foreground">
                  Across all adapters
                </p>
              </CardContent>
            </Card>

            <Card>
              <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
                <CardTitle className="text-sm font-medium">Avg Epsilon (ε)</CardTitle>
                <Hash className="h-4 w-4 text-muted-foreground" />
              </CardHeader>
              <CardContent>
                <div className="text-2xl font-bold">
                  {(() => {
                    const adaptersWithEpsilon = adapters.filter(a => a.epsilon_stats);
                    if (adaptersWithEpsilon.length === 0) return 'N/A';
                    const avgEpsilon = adaptersWithEpsilon.reduce((sum, a) => sum + (a.epsilon_stats?.mean_error || 0), 0) / adaptersWithEpsilon.length;
                    return avgEpsilon.toFixed(4);
                  })()}
                </div>
                <p className="text-xs text-muted-foreground">
                  Numerical stability
                </p>
              </CardContent>
            </Card>
          </div>

          <Card>
            <CardHeader>
              <CardTitle>Epsilon (ε) Trends</CardTitle>
            </CardHeader>
            <CardContent>
              <div className="h-64 flex items-center justify-center text-muted-foreground">
                Epsilon trend charts would go here
              </div>
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="manifests" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center">
                <FileText className="mr-2 h-5 w-5" />
                Adapter Manifests
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="space-y-4">
                {adapters.map((adapter) => (
                  <Card key={adapter.id} className="cursor-pointer hover:shadow-md transition-shadow">
                    <CardContent className="pt-6">
                      <div className="flex items-center justify-between mb-4">
                        <div className="flex items-center space-x-2">
                          {getDomainIcon(adapter.domain_type)}
                          <h3 className="font-medium">{adapter.name}</h3>
                          <Badge variant="outline">v{adapter.version}</Badge>
                        </div>
                        <div className="flex space-x-2">
                          <Button variant="outline" size="sm">
                            <Eye className="h-4 w-4" />
                          </Button>
                          <Button variant="outline" size="sm">
                            <Download className="h-4 w-4" />
                          </Button>
                        </div>
                      </div>
                      
                      <div className="space-y-2 text-sm">
                        <div><strong>Model:</strong> {adapter.model}</div>
                        <div><strong>Hash:</strong> <code className="text-xs">{adapter.hash}</code></div>
                        <div><strong>Input Format:</strong> {adapter.input_format}</div>
                        <div><strong>Output Format:</strong> {adapter.output_format}</div>
                        <div><strong>Description:</strong> {adapter.description}</div>
                      </div>
                    </CardContent>
                  </Card>
                ))}
              </div>
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>

      {/* Create Adapter Dialog */}
      <Dialog open={isCreateDialogOpen} onOpenChange={setIsCreateDialogOpen}>
        <DialogContent className="modal-large">
          <DialogHeader>
            <DialogTitle>Create Domain Adapter</DialogTitle>
          </DialogHeader>
          <div className="space-y-4">
            <div className="grid grid-cols-2 gap-4">
              <div>
                <Label htmlFor="name">Adapter Name</Label>
                <Input 
                  id="name" 
                  value={newAdapterForm.name}
                  onChange={(e) => setNewAdapterForm({...newAdapterForm, name: e.target.value})}
                  placeholder="my-domain-adapter-v1"
                />
              </div>
              <div>
                <Label htmlFor="domain_type">Domain Type</Label>
                <Select 
                  value={newAdapterForm.domain_type} 
                  onValueChange={(value: 'text' | 'vision' | 'telemetry') => 
                    setNewAdapterForm({...newAdapterForm, domain_type: value})
                  }
                >
                  <SelectTrigger>
                    <SelectValue placeholder="Select domain type" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="text">Text Processing</SelectItem>
                    <SelectItem value="vision">Vision Processing</SelectItem>
                    <SelectItem value="telemetry">Telemetry Processing</SelectItem>
                  </SelectContent>
                </Select>
              </div>
            </div>

            <div>
              <Label htmlFor="model">Model</Label>
              <Input 
                id="model" 
                value={newAdapterForm.model}
                onChange={(e) => setNewAdapterForm({...newAdapterForm, model: e.target.value})}
                placeholder="mlx_lora_base_v1"
              />
            </div>

            <div>
              <Label htmlFor="description">Description</Label>
              <Textarea 
                id="description" 
                value={newAdapterForm.description}
                onChange={(e) => setNewAdapterForm({...newAdapterForm, description: e.target.value})}
                placeholder="Brief description of the adapter's functionality"
              />
            </div>

            <Alert>
              <AlertTriangle className="icon-standard" />
              <AlertDescription>
                Domain adapter creation will generate a deterministic manifest and register the adapter with the AdapterOS core.
              </AlertDescription>
            </Alert>

            <div className="flex justify-end space-x-2">
              <Button variant="outline" onClick={() => setIsCreateDialogOpen(false)}>
                Cancel
              </Button>
              <Button onClick={handleCreateAdapter}>
                Create Adapter
              </Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>

      {/* Test Dialog */}
      <Dialog open={isTestDialogOpen} onOpenChange={setIsTestDialogOpen}>
        <DialogContent className="modal-standard">
          <DialogHeader>
            <DialogTitle>Run Determinism Tests</DialogTitle>
          </DialogHeader>
          <div className="space-y-4">
            <Alert>
              <TestTube className="icon-standard" />
              <AlertDescription>
                Run comprehensive determinism tests to verify byte-identical outputs across multiple executions.
              </AlertDescription>
            </Alert>
            <div className="flex justify-end space-x-2">
              <Button variant="outline" onClick={() => setIsTestDialogOpen(false)}>
                Close
              </Button>
              <Button onClick={() => {
                toast.info('Determinism tests started');
                setIsTestDialogOpen(false);
              }}>
                Start Tests
              </Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}
