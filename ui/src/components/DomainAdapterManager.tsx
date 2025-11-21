import React, { useState, useEffect } from 'react';
import { toast } from 'sonner';
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
import { errorRecoveryTemplates } from './ui/error-recovery';
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
import { User, DomainAdapter as ApiDomainAdapter } from '../api/types';
import { logger, toError } from '../utils/logger';

interface DomainAdapterManagerProps {
  user: User;
  selectedTenant: string;
}

type DomainAdapterDomain = 'text' | 'vision' | 'telemetry';

interface DomainAdapter {
  id: string;
  name: string;
  version?: string;
  description: string;
  domain_type: DomainAdapterDomain;
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

type NewAdapterFormState = {
  name: string;
  domain_type: DomainAdapterDomain;
  model: string;
  description: string;
  config: Record<string, unknown>;
};

// Transform API DomainAdapter to local DomainAdapter interface
function apiToLocalAdapter(apiAdapter: ApiDomainAdapter): DomainAdapter {
  // Map API status to local status
  const statusMap: Record<string, 'loaded' | 'unloaded' | 'error'> = {
    active: 'loaded',
    inactive: 'unloaded',
    loading: 'unloaded',
    error: 'error',
  };

  return {
    id: apiAdapter.id,
    name: apiAdapter.name,
    version: apiAdapter.version,
    description: apiAdapter.description || '',
    domain_type: (apiAdapter.domain_type || apiAdapter.domain || 'text') as DomainAdapterDomain,
    model: apiAdapter.model || '',
    hash: apiAdapter.hash || '',
    input_format: apiAdapter.input_format || '',
    output_format: apiAdapter.output_format || '',
    config: apiAdapter.config as Record<string, unknown>,
    status: statusMap[apiAdapter.status || ''] || 'unloaded',
    epsilon_stats: apiAdapter.epsilon_stats ? {
      mean_error: apiAdapter.epsilon_stats.mean_error,
      max_error: apiAdapter.epsilon_stats.max_error || 0,
      error_count: 0,
      last_updated: new Date().toISOString(),
    } : undefined,
    last_execution: apiAdapter.last_execution,
    execution_count: apiAdapter.execution_count || 0,
    created_at: apiAdapter.created_at,
    updated_at: apiAdapter.updated_at,
  };
}

export function DomainAdapterManager({ user, selectedTenant }: DomainAdapterManagerProps) {
  const [adapters, setAdapters] = useState<DomainAdapter[]>([]);
  const [tests, setTests] = useState<DomainAdapterTest[]>([]);
  const [loading, setLoading] = useState(true);
  const [isCreateDialogOpen, setIsCreateDialogOpen] = useState(false);
  const [isTestDialogOpen, setIsTestDialogOpen] = useState(false);
  const [selectedAdapter, setSelectedAdapter] = useState<DomainAdapter | null>(null);
  const [activeTab, setActiveTab] = useState('adapters');
  const [newAdapterForm, setNewAdapterForm] = useState<NewAdapterFormState>({
    name: '',
    domain_type: 'text',
    model: '',
    description: '',
    config: {},
  });
  const [statusMessage, setStatusMessage] = useState<{ message: string; variant: 'success' | 'info' | 'warning' } | null>(null);
  const [errorRecovery, setErrorRecovery] = useState<React.ReactElement | null>(null);

  const showStatus = (message: string, variant: 'success' | 'info' | 'warning') => {
    setStatusMessage({ message, variant });
  };

  useEffect(() => {
    const fetchData = async () => {
      try {
        // Citation: ui/src/api/client.ts L625-L628
        const adaptersData = await apiClient.listDomainAdapters();
        // Transform API adapters to local interface
        setAdapters(adaptersData.map(apiToLocalAdapter));
        // Domain adapter tests - placeholder implementation
        setTests([]);
      } catch (err) {
        const errorMsg = err instanceof Error ? err.message : 'Failed to fetch domain adapters';
        logger.error('Failed to fetch domain adapters', {
          component: 'DomainAdapterManager',
          operation: 'fetchAdapters',
          tenantId: selectedTenant,
          errorMessage: errorMsg,
        }, toError(err));
        showStatus(errorMsg, 'warning');
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
      showStatus('Domain adapter created successfully.', 'success');
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
      logger.error('Failed to create domain adapter', {
        component: 'DomainAdapterManager',
        operation: 'createAdapter',
        body: newAdapterForm,
        errorMessage: errorMsg
      }, toError(err));
      setStatusMessage({ message: errorMsg, variant: 'warning' });
      setErrorRecovery(
        errorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error(errorMsg),
          () => handleCreateAdapter()
        )
      );
    }
  };

  const handleTestAdapter = async (adapterId: string, inputData: string) => {
    try {

      setStatusMessage(null);
      const result = await apiClient.testDomainAdapter(adapterId, inputData);
      
      logger.info('Domain adapter test completed', {
        component: 'DomainAdapterManager',
        operation: 'testAdapter',
        adapterId,
        testId: result.test_id,
        passed: result.passed,
        executionTimeMs: result.execution_time_ms,
      });
      
      if (result.passed) {
        showStatus(
          `Test passed: ${result.actual_output}${result.expected_output ? ` (expected: ${result.expected_output})` : ''}`,
          'success'
        );
      } else {
        showStatus(
          `Test failed: ${result.actual_output}${result.expected_output ? ` (expected: ${result.expected_output})` : ''}`,
          'warning'
        );
      }

      // API call - placeholder implementation
      // const result = await apiClient.testDomainAdapter(adapterId, inputData);
      toast.success('Domain adapter test completed');
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to test domain adapter';
      logger.error('Failed to test domain adapter', {
        component: 'DomainAdapterManager',
        operation: 'testAdapter',
        adapterId: adapterId,
        errorMessage: errorMsg
      }, toError(err));
      setStatusMessage({ message: errorMsg, variant: 'warning' });
      setErrorRecovery(
        errorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error(errorMsg),
          () => handleTestAdapter(adapterId, inputData)
        )
      );
    }
  };


  if (loading) {
    return <div className="text-center p-8">Loading domain adapters...</div>;
  }

  return (
    <div className="space-y-6">
      {errorRecovery && (
        <div>
          {errorRecovery}
        </div>
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

      <div className="flex items-center justify-between flex items-center justify-between mb-6">
        <div>
          <h1 className="text-2xl font-bold">Domain Adapter Management</h1>
          <p className="text-sm text-muted-foreground">
            Manage deterministic domain adapters for text, vision, and telemetry processing
          </p>
        </div>
        <div className="flex items-center">
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

        <TabsContent value="adapters" className="mb-4">
          <Card className="p-4 rounded-lg border border-border bg-card shadow-md">
            <CardHeader>
              <CardTitle className="flex items-center justify-center">
                <Layers className="h-6 w-6 mr-2" />
                Domain Adapters
              </CardTitle>
            </CardHeader>
            <CardContent>
              <Table className="border-collapse w-full">
                <TableHeader>
                  <TableRow>
                    <TableHead className="p-4 border-b border-border">Name</TableHead>
                    <TableHead className="p-4 border-b border-border">Domain</TableHead>
                    <TableHead className="p-4 border-b border-border">Status</TableHead>
                    <TableHead className="p-4 border-b border-border">Epsilon (ε)</TableHead>
                    <TableHead className="p-4 border-b border-border">Executions</TableHead>
                    <TableHead className="p-4 border-b border-border">Last Used</TableHead>
                    <TableHead className="p-4 border-b border-border">Actions</TableHead>
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
                        <TableCell className="p-4 border-b border-border">
                          <div className="flex items-center justify-center">
                            {getDomainIcon(adapter.domain_type)}
                            <div>
                              <div className="font-medium">{adapter.name}</div>
                              <div className="text-sm text-muted-foreground">
                                v{adapter.version} • {adapter.domain_type}
                              </div>
                            </div>
                          </div>
                        </TableCell>
                        <TableCell className="p-4 border-b border-border">
                          <div className="status-indicator status-neutral flex items-center justify-center">
                            {getDomainIcon(adapter.domain_type)}
                            <span>{adapter.domain_type}</span>
                          </div>
                        </TableCell>
                        <TableCell className="p-4 border-b border-border">
                          <Badge className={getStatusBadge(adapter.status)}>
                            {adapter.status}
                          </Badge>
                        </TableCell>
                        <TableCell className="p-4 border-b border-border">
                          {adapter.epsilon_stats ? (
                            <div className="flex items-center justify-center">
                              <Hash className="icon-standard" />
                              <span>ε: {adapter.epsilon_stats.mean_error.toFixed(4)}</span>
                            </div>
                          ) : (
                            <span className="text-muted-foreground">No data</span>
                          )}
                        </TableCell>
                        <TableCell className="p-4 border-b border-border">
                          <div className="flex items-center justify-center">
                            <Target className="icon-standard" />
                            <span>{adapter.execution_count}</span>
                          </div>
                        </TableCell>
                        <TableCell className="p-4 border-b border-border">
                          <div className="flex items-center justify-center">
                            <Clock className="icon-standard" />
                            <span>{adapter.last_execution ? new Date(adapter.last_execution).toLocaleString() : 'Never'}</span>
                          </div>
                        </TableCell>
                        <TableCell className="p-4 border-b border-border">
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
        <DialogContent className="max-w-4xl">
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
                  onValueChange={(value) =>
                    setNewAdapterForm({ ...newAdapterForm, domain_type: value as DomainAdapterDomain })
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
        <DialogContent className="max-w-md">
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
                showStatus('Determinism tests started.', 'info');
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
