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
import { Switch } from './ui/switch';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs';
import { Progress } from './ui/progress';
import { Alert, AlertDescription } from './ui/alert';
import { EmptyState } from './ui/empty-state';
import { 
  Plus, 
  Code, 
  Settings, 
  Play, 
  Pause, 
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
  FileText
} from 'lucide-react';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from './ui/dropdown-menu';
import apiClient from '../api/client';
import { User } from '../api/types';
import { useSSE } from '../hooks/useSSE';
import { toast } from 'sonner';
import { TrainingMonitor } from './TrainingMonitor';
import { CodeIntelligenceTraining } from './CodeIntelligenceTraining';
import { TrainingTemplates } from './TrainingTemplates';
import { ResourceMonitor } from './ResourceMonitor';
import { AdapterStateVisualization } from './AdapterStateVisualization';
import { AdapterLifecycleManager } from './AdapterLifecycleManager';
import { AdapterMemoryMonitor } from './AdapterMemoryMonitor';
import { DomainAdapterManager } from './DomainAdapterManager';

interface AdaptersProps {
  user: User;
  selectedTenant: string;
}

interface Adapter {
  id: string;
  adapter_id: string;
  name: string;
  hash_b3: string;
  rank: number;
  tier: number;
  languages_json?: string;
  framework?: string;
  
  // Code intelligence fields
  category: 'code' | 'framework' | 'codebase' | 'ephemeral';
  scope: 'global' | 'tenant' | 'repo' | 'commit';
  framework_id?: string;
  framework_version?: string;
  repo_id?: string;
  commit_sha?: string;
  intent?: string;
  
  // Lifecycle state management
  current_state: 'unloaded' | 'cold' | 'warm' | 'hot' | 'resident';
  pinned: boolean;
  memory_bytes: number;
  last_activated?: string;
  activation_count: number;
  
  created_at: string;
  updated_at: string;
  active: boolean;
}

interface TrainingJob {
  id: string;
  adapter_name: string;
  status: 'queued' | 'running' | 'completed' | 'failed' | 'cancelled';
  progress: number;
  started_at: string;
  estimated_completion?: string;
  config: TrainingConfig;
  logs: string[];
  metrics: TrainingMetrics;
}

interface TrainingConfig {
  tenant_id: string;
  repo_id?: string;
  commit_sha?: string;
  rank: number;
  alpha: number;
  targets: string[];
  epochs: number;
  learning_rate: number;
  batch_size: number;
  category: 'code' | 'framework' | 'codebase' | 'ephemeral';
  scope: 'global' | 'tenant' | 'repo' | 'commit';
  framework_id?: string;
  framework_version?: string;
  intent?: string;
}

interface TrainingMetrics {
  current_epoch: number;
  total_epochs: number;
  loss: number;
  validation_loss: number;
  learning_rate: number;
  gpu_utilization: number;
  memory_usage: number;
  tokens_per_second: number;
}

interface AdaptersProps {
  user: User;
  selectedTenant: string;
}

export function Adapters({ user, selectedTenant }: AdaptersProps) {
  const [adapters, setAdapters] = useState<Adapter[]>([]);
  const [trainingJobs, setTrainingJobs] = useState<TrainingJob[]>([]);
  const [loading, setLoading] = useState(true);
  const [isCreateDialogOpen, setIsCreateDialogOpen] = useState(false);
  const [isTrainingDialogOpen, setIsTrainingDialogOpen] = useState(false);
  const [selectedAdapter, setSelectedAdapter] = useState<Adapter | null>(null);
  const [activeTab, setActiveTab] = useState('adapters');
  const [selectedTrainingJob, setSelectedTrainingJob] = useState<string | null>(null);
  const [trainingConfig, setTrainingConfig] = useState<Partial<TrainingConfig>>({});
  const [deleteConfirmId, setDeleteConfirmId] = useState<string | null>(null);

  // SSE connection for real-time adapter state updates
  const { data: sseAdapters } = useSSE<Adapter[]>('/v1/stream/adapters');

  // Remove mock data - using real API now
  /* Mock data removed
  const mockAdapters: Adapter[] = [
    {
      id: '1',
      adapter_id: 'python-general-v1',
      name: 'python-general-v1',
      hash_b3: 'b3:abc123...',
      rank: 16,
      tier: 1,
      languages_json: '["python"]',
      framework: 'python',
      category: 'code',
      scope: 'global',
      current_state: 'hot',
      pinned: false,
      memory_bytes: 16 * 1024 * 1024,
      last_activated: '2024-02-15T10:30:00Z',
      activation_count: 1247,
      created_at: '2024-01-15T10:30:00Z',
      updated_at: '2024-02-15T10:30:00Z',
      active: true
    },
    {
      id: '2',
      adapter_id: 'django-specific-v2',
      name: 'django-specific-v2',
      hash_b3: 'b3:def456...',
      rank: 12,
      tier: 2,
      languages_json: '["python"]',
      framework: 'django',
      framework_id: 'django',
      framework_version: '4.2',
      category: 'framework',
      scope: 'global',
      current_state: 'warm',
      pinned: false,
      memory_bytes: 16 * 1024 * 1024,
      last_activated: '2024-02-15T09:45:00Z',
      activation_count: 89,
      created_at: '2024-01-20T14:15:00Z',
      updated_at: '2024-02-15T09:45:00Z',
      active: true
    },
    {
      id: '3',
      adapter_id: 'acme-payments-v1',
      name: 'acme-payments-v1',
      hash_b3: 'b3:ghi789...',
      rank: 24,
      tier: 3,
      languages_json: '["python", "javascript"]',
      framework: 'python',
      repo_id: 'acme/payments',
      commit_sha: 'abc123def456',
      intent: 'payments',
      category: 'codebase',
      scope: 'tenant',
      current_state: 'resident',
      pinned: true,
      memory_bytes: 24 * 1024 * 1024,
      last_activated: '2024-02-15T10:25:00Z',
      activation_count: 2341,
      created_at: '2024-02-01T09:45:00Z',
      updated_at: '2024-02-15T10:25:00Z',
      active: true
    },
    {
      id: '4',
      adapter_id: 'temp-debug-v1',
      name: 'temp-debug-v1',
      hash_b3: 'b3:jkl012...',
      rank: 8,
      tier: 4,
      languages_json: '["python"]',
      framework: 'python',
      intent: 'debugging',
      category: 'ephemeral',
      scope: 'global',
      current_state: 'cold',
      pinned: false,
      memory_bytes: 8 * 1024 * 1024,
      last_activated: '2024-02-15T08:30:00Z',
      activation_count: 12,
      created_at: '2024-02-15T08:00:00Z',
      updated_at: '2024-02-15T08:30:00Z',
      active: true
    }
  ]; */

  useEffect(() => {
    const fetchData = async () => {
      try {
        const adaptersData = await apiClient.listAdapters();
        setAdapters(adaptersData);
        // Training jobs API not yet implemented
        setTrainingJobs([]);
      } catch (err) {
        const errorMsg = err instanceof Error ? err.message : 'Failed to fetch adapters';
        console.error(errorMsg, err);
        toast.error(errorMsg);
      } finally {
        setLoading(false);
      }
    };
    fetchData();
  }, [selectedTenant]);

  // Update adapters from SSE stream
  useEffect(() => {
    if (sseAdapters) {
      setAdapters(sseAdapters);
    }
  }, [sseAdapters]);

  const handleDeleteAdapter = async (adapterId: string) => {
    try {
      await apiClient.deleteAdapter(adapterId);
      setAdapters(adapters.filter(a => a.adapter_id !== adapterId));
      setDeleteConfirmId(null);
      toast.success('Adapter deleted successfully');
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to delete adapter';
      toast.error(errorMsg);
    }
  };

  const handlePinToggle = async (adapter: Adapter) => {
    try {
      if (adapter.pinned) {
        await apiClient.unpinAdapter(adapter.adapter_id);
        toast.success('Adapter unpinned');
      } else {
        await apiClient.pinAdapter(adapter.adapter_id);
        toast.success('Adapter pinned');
      }
      // Refresh adapters list
      const adaptersData = await apiClient.listAdapters();
      setAdapters(adaptersData);
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to toggle pin';
      toast.error(errorMsg);
    }
  };

  const handlePromoteState = async (adapterId: string) => {
    try {
      const result = await apiClient.promoteAdapterState(adapterId);
      toast.success(`State promoted: ${result.old_state} → ${result.new_state}`);
      // Refresh adapters list
      const adaptersData = await apiClient.listAdapters();
      setAdapters(adaptersData);
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to promote adapter state';
      toast.error(errorMsg);
    }
  };

  const handleDownloadManifest = async (adapterId: string) => {
    try {
      const manifest = await apiClient.downloadAdapterManifest(adapterId);
      const blob = new Blob([JSON.stringify(manifest, null, 2)], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `${adapterId}-manifest.json`;
      a.click();
      URL.revokeObjectURL(url);
      toast.success('Manifest downloaded');
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to download manifest';
      toast.error(errorMsg);
    }
  };

  const [showHealthModal, setShowHealthModal] = useState(false);
  const [healthData, setHealthData] = useState<any | null>(null);

  const handleViewHealth = async (adapterId: string) => {
    try {
      const health = await apiClient.getAdapterHealth(adapterId);
      setHealthData(health);
      setShowHealthModal(true);
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to fetch adapter health';
      toast.error(errorMsg);
    }
  };

  const getStateIcon = (state: string) => {
    switch (state) {
      case 'unloaded': return <Square className="h-4 w-4 text-gray-500" />;
      case 'cold': return <Snowflake className="h-4 w-4 text-blue-500" />;
      case 'warm': return <Thermometer className="h-4 w-4 text-orange-500" />;
      case 'hot': return <Flame className="h-4 w-4 text-red-500" />;
      case 'resident': return <Anchor className="h-4 w-4 text-purple-500" />;
      default: return <Activity className="h-4 w-4 text-gray-500" />;
    }
  };

  const getStateBadge = (state: string) => {
    const variants = {
      unloaded: 'bg-gray-100 text-gray-800',
      cold: 'bg-blue-100 text-blue-800',
      warm: 'bg-orange-100 text-orange-800',
      hot: 'bg-red-100 text-red-800',
      resident: 'bg-purple-100 text-purple-800'
    };
    return variants[state as keyof typeof variants] || 'bg-gray-100 text-gray-800';
  };

  const getCategoryIcon = (category: string) => {
    switch (category) {
      case 'code': return <Code className="h-4 w-4" />;
      case 'framework': return <Layers className="h-4 w-4" />;
      case 'codebase': return <GitBranch className="h-4 w-4" />;
      case 'ephemeral': return <Clock className="h-4 w-4" />;
      default: return <Code className="h-4 w-4" />;
    }
  };

  if (loading) {
    return <div className="text-center p-8">Loading adapters...</div>;
  }

  return (
    <div className="space-y-6">
      <div className="flex-between section-header">
        <div>
          <h1 className="section-title">Adapter Management</h1>
          <p className="section-description">
            Train, manage, and monitor LoRA adapters for your models
          </p>
        </div>
        <div className="flex-standard">
          <Button onClick={() => setIsTrainingDialogOpen(true)}>
            <Brain className="icon-standard mr-2" />
            Train Adapter
          </Button>
          <Button onClick={() => setIsCreateDialogOpen(true)}>
            <Plus className="icon-standard mr-2" />
            Register Adapter
          </Button>
        </div>
      </div>

      <Tabs value={activeTab} onValueChange={setActiveTab}>
        <TabsList>
          <TabsTrigger value="adapters">Adapters</TabsTrigger>
          <TabsTrigger value="domain-adapters">Domain Adapters</TabsTrigger>
          <TabsTrigger value="lifecycle">Lifecycle</TabsTrigger>
          <TabsTrigger value="memory">Memory</TabsTrigger>
          <TabsTrigger value="state-viz">State Visualization</TabsTrigger>
          <TabsTrigger value="training">Training Jobs</TabsTrigger>
          <TabsTrigger value="templates">Templates</TabsTrigger>
          <TabsTrigger value="code-intelligence">Code Intelligence</TabsTrigger>
          <TabsTrigger value="resources">Resources</TabsTrigger>
          <TabsTrigger value="analytics">Analytics</TabsTrigger>
        </TabsList>

        <TabsContent value="adapters" className="form-field">
          <Card className="card-standard">
            <CardHeader>
              <CardTitle className="flex-center">
                <Code className="icon-large mr-2" />
                Registered Adapters
              </CardTitle>
            </CardHeader>
            <CardContent>
              <Table className="table-standard">
                <TableHeader>
                  <TableRow>
                    <TableHead className="table-cell-standard">Name</TableHead>
                    <TableHead className="table-cell-standard">Category</TableHead>
                    <TableHead className="table-cell-standard">State</TableHead>
                    <TableHead className="table-cell-standard">Memory</TableHead>
                    <TableHead className="table-cell-standard">Activations</TableHead>
                    <TableHead className="table-cell-standard">Last Used</TableHead>
                    <TableHead className="table-cell-standard">Actions</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {adapters.length === 0 ? (
                    <TableRow>
                      <TableCell colSpan={7} className="h-32">
                        <EmptyState
                          icon={Code}
                          title="No Adapters Registered"
                          description="Get started by registering your first adapter or training a new one from your codebase. Use the Register or Train buttons above to begin."
                        />
                      </TableCell>
                    </TableRow>
                  ) : (
                    adapters.map((adapter) => (
                      <TableRow key={adapter.id}>
                      <TableCell className="table-cell-standard">
                        <div className="flex-center">
                          {getCategoryIcon(adapter.category)}
                          <div>
                            <div className="font-medium">{adapter.name}</div>
                            <div className="text-sm text-muted-foreground">
                              Tier {adapter.tier} • Rank {adapter.rank}
                            </div>
                          </div>
                        </div>
                      </TableCell>
                      <TableCell className="table-cell-standard">
                        <div className="status-indicator status-neutral flex-center">
                          {getCategoryIcon(adapter.category)}
                          <span>{adapter.category}</span>
                        </div>
                      </TableCell>
                      <TableCell className="table-cell-standard">
                        <div className="flex-center">
                          {getStateIcon(adapter.current_state)}
                          <div className={`status-indicator ${
                            adapter.current_state === 'hot' ? 'status-error' :
                            adapter.current_state === 'warm' ? 'status-warning' :
                            adapter.current_state === 'cold' ? 'status-info' :
                            adapter.current_state === 'resident' ? 'status-success' :
                            'status-neutral'
                          }`}>
                            {adapter.current_state}
                          </div>
                          {adapter.pinned && (
                            <Pin className="icon-standard text-purple-500" />
                          )}
                        </div>
                      </TableCell>
                      <TableCell className="table-cell-standard">
                        <div className="flex-center">
                          <MemoryStick className="icon-standard" />
                          <span>{Math.round(adapter.memory_bytes / 1024 / 1024)} MB</span>
                        </div>
                      </TableCell>
                      <TableCell className="table-cell-standard">
                        <div className="flex-center">
                          <Target className="icon-standard" />
                          <span>{adapter.activation_count}</span>
                        </div>
                      </TableCell>
                      <TableCell className="table-cell-standard">
                        <div className="flex-center">
                          <Clock className="icon-standard" />
                          <span>{adapter.last_activated ? new Date(adapter.last_activated).toLocaleString() : 'Never'}</span>
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
                            <DropdownMenuItem onClick={() => handlePinToggle(adapter)}>
                              <Pin className="mr-2 h-4 w-4" />
                              {adapter.pinned ? 'Unpin' : 'Pin'}
                            </DropdownMenuItem>
                            <DropdownMenuItem onClick={() => handlePromoteState(adapter.adapter_id)}>
                              <ArrowUp className="mr-2 h-4 w-4" />
                              Promote State
                            </DropdownMenuItem>
                            <DropdownMenuItem onClick={() => handleViewHealth(adapter.adapter_id)}>
                              <Activity className="mr-2 h-4 w-4" />
                              View Health
                            </DropdownMenuItem>
                            <DropdownMenuItem onClick={() => handleDownloadManifest(adapter.adapter_id)}>
                              <Download className="mr-2 h-4 w-4" />
                              Download Manifest
                            </DropdownMenuItem>
                            <DropdownMenuItem onClick={() => setDeleteConfirmId(adapter.adapter_id)}>
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

        <TabsContent value="domain-adapters" className="space-y-4">
          <DomainAdapterManager user={user} selectedTenant={selectedTenant} />
        </TabsContent>

        <TabsContent value="lifecycle" className="space-y-4">
          <AdapterLifecycleManager 
            adapters={adapters}
            onAdapterUpdate={(adapterId, updates) => {
              setAdapters(prev => prev.map(adapter => 
                adapter.adapter_id === adapterId 
                  ? { ...adapter, ...updates }
                  : adapter
              ));
            }}
            onAdapterEvict={(adapterId) => {
              setAdapters(prev => prev.filter(adapter => adapter.adapter_id !== adapterId));
            }}
            onAdapterPin={(adapterId, pinned) => {
              setAdapters(prev => prev.map(adapter => 
                adapter.adapter_id === adapterId 
                  ? { ...adapter, pinned }
                  : adapter
              ));
            }}
            onPolicyUpdate={(category, policy) => {
              // TODO: Update policy in backend
              console.log('Policy updated:', category, policy);
            }}
          />
        </TabsContent>

        <TabsContent value="memory" className="space-y-4">
          <AdapterMemoryMonitor 
            adapters={adapters}
            totalMemory={8 * 1024 * 1024 * 1024} // 8GB total memory
            onEvictAdapter={(adapterId) => {
              setAdapters(prev => prev.filter(adapter => adapter.adapter_id !== adapterId));
            }}
            onPinAdapter={(adapterId, pinned) => {
              setAdapters(prev => prev.map(adapter => 
                adapter.adapter_id === adapterId 
                  ? { ...adapter, pinned }
                  : adapter
              ));
            }}
            onUpdateMemoryLimit={(category, limit) => {
              // TODO: Update memory limit in backend
              console.log('Memory limit updated:', category, limit);
            }}
          />
        </TabsContent>

        <TabsContent value="state-viz" className="space-y-4">
          <AdapterStateVisualization 
            adapters={adapters.map(adapter => ({
              adapter_id: adapter.adapter_id,
              adapter_idx: parseInt(adapter.id),
              state: adapter.current_state,
              pinned: adapter.pinned,
              memory_bytes: adapter.memory_bytes,
              category: adapter.category,
              scope: adapter.scope,
              last_activated: adapter.last_activated,
              activation_count: adapter.activation_count
            }))}
            totalMemory={8 * 1024 * 1024 * 1024} // 8GB total memory
          />
        </TabsContent>

        <TabsContent value="training" className="space-y-4">
          {selectedTrainingJob ? (
            <TrainingMonitor 
              jobId={selectedTrainingJob} 
              onClose={() => setSelectedTrainingJob(null)} 
            />
          ) : (
            <Card>
              <CardHeader>
                <CardTitle className="flex items-center">
                  <Brain className="mr-2 h-5 w-5" />
                  Training Jobs
                </CardTitle>
              </CardHeader>
              <CardContent>
                <div className="space-y-4">
                  {trainingJobs.map((job) => (
                    <Card key={job.id} className="cursor-pointer hover:shadow-md transition-shadow">
                      <CardContent className="pt-6">
                        <div className="flex items-center justify-between mb-4">
                          <div className="flex items-center space-x-2">
                            <Brain className="h-5 w-5" />
                            <h3 className="font-medium">{job.adapter_name}</h3>
                            <Badge className={getStateBadge(job.status)}>
                              {job.status}
                            </Badge>
                          </div>
                          <div className="flex space-x-2">
                            <Button 
                              variant="outline" 
                              size="sm"
                              onClick={() => setSelectedTrainingJob(job.id)}
                            >
                              <Eye className="h-4 w-4" />
                            </Button>
                            <Button variant="outline" size="sm">
                              <Pause className="h-4 w-4" />
                            </Button>
                            <Button variant="outline" size="sm">
                              <Square className="h-4 w-4" />
                            </Button>
                          </div>
                        </div>
                        
                        <div className="space-y-3">
                          <div className="flex items-center space-x-4">
                            <div className="flex-1">
                              <div className="flex items-center justify-between text-sm mb-1">
                                <span>Progress</span>
                                <span>{job.progress}%</span>
                              </div>
                              <Progress value={job.progress} />
                            </div>
                          </div>

                          <div className="grid grid-cols-2 md:grid-cols-4 gap-4 text-sm">
                            <div className="flex items-center space-x-2">
                              <Cpu className="h-4 w-4" />
                              <span>GPU: {job.metrics.gpu_utilization}%</span>
                            </div>
                            <div className="flex items-center space-x-2">
                              <MemoryStick className="h-4 w-4" />
                              <span>Memory: {job.metrics.memory_usage}GB</span>
                            </div>
                            <div className="flex items-center space-x-2">
                              <Zap className="h-4 w-4" />
                              <span>{job.metrics.tokens_per_second} tok/s</span>
                            </div>
                            <div className="flex items-center space-x-2">
                              <Target className="h-4 w-4" />
                              <span>Loss: {job.metrics.loss.toFixed(4)}</span>
                            </div>
                          </div>

                          <div className="text-sm text-muted-foreground">
                            Epoch {job.metrics.current_epoch}/{job.metrics.total_epochs} • 
                            Started {new Date(job.started_at).toLocaleString()}
                            {job.estimated_completion && (
                              <> • ETA {new Date(job.estimated_completion).toLocaleString()}</>
                            )}
                          </div>

                          <div className="bg-gray-50 p-3 rounded-md">
                            <div className="text-sm font-medium mb-2">Recent Logs</div>
                            <div className="space-y-1 text-xs font-mono">
                              {job.logs.slice(-3).map((log, idx) => (
                                <div key={idx} className="text-muted-foreground">{log}</div>
                              ))}
                            </div>
                          </div>
                        </div>
                      </CardContent>
                    </Card>
                  ))}
                </div>
              </CardContent>
            </Card>
          )}
        </TabsContent>

        <TabsContent value="templates" className="space-y-4">
          <TrainingTemplates 
            onTemplateSelect={(template) => {
              setTrainingConfig({
                rank: template.rank,
                alpha: template.alpha,
                epochs: template.epochs,
                learning_rate: template.learning_rate,
                batch_size: template.batch_size,
                targets: template.targets,
                category: template.category
              });
              setIsTrainingDialogOpen(true);
            }}
          />
        </TabsContent>

        <TabsContent value="code-intelligence" className="space-y-4">
          <CodeIntelligenceTraining 
            onConfigSelect={setTrainingConfig}
            initialConfig={trainingConfig}
          />
        </TabsContent>

        <TabsContent value="resources" className="space-y-4">
          <ResourceMonitor />
        </TabsContent>

        <TabsContent value="analytics" className="space-y-4">
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
            <Card>
              <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
                <CardTitle className="text-sm font-medium">Total Adapters</CardTitle>
                <Code className="h-4 w-4 text-muted-foreground" />
              </CardHeader>
              <CardContent>
                <div className="text-2xl font-bold">{adapters.length}</div>
                <p className="text-xs text-muted-foreground">
                  +2 from last month
                </p>
              </CardContent>
            </Card>

            <Card>
              <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
                <CardTitle className="text-sm font-medium">Active Training</CardTitle>
                <Activity className="h-4 w-4 text-muted-foreground" />
              </CardHeader>
              <CardContent>
                <div className="text-2xl font-bold">
                  {trainingJobs.filter(job => job.status === 'running').length}
                </div>
                <p className="text-xs text-muted-foreground">
                  Jobs in progress
                </p>
              </CardContent>
            </Card>

            <Card>
              <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
                <CardTitle className="text-sm font-medium">Total Activations</CardTitle>
                <Target className="h-4 w-4 text-muted-foreground" />
              </CardHeader>
              <CardContent>
                <div className="text-2xl font-bold">
                  {adapters.reduce((sum, adapter) => sum + adapter.activation_count, 0)}
                </div>
                <p className="text-xs text-muted-foreground">
                  Across all adapters
                </p>
              </CardContent>
            </Card>

            <Card>
              <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
                <CardTitle className="text-sm font-medium">Memory Usage</CardTitle>
                <MemoryStick className="h-4 w-4 text-muted-foreground" />
              </CardHeader>
              <CardContent>
                <div className="text-2xl font-bold">
                  {Math.round(adapters.reduce((sum, adapter) => sum + adapter.memory_bytes, 0) / 1024 / 1024)} MB
                </div>
                <p className="text-xs text-muted-foreground">
                  Total allocated
                </p>
              </CardContent>
            </Card>
          </div>

          <Card>
            <CardHeader>
              <CardTitle>Training Performance</CardTitle>
            </CardHeader>
            <CardContent>
              <div className="h-64 flex items-center justify-center text-muted-foreground">
                Training performance charts would go here
              </div>
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>

      {/* Training Dialog */}
      <Dialog open={isTrainingDialogOpen} onOpenChange={setIsTrainingDialogOpen}>
        <DialogContent className="modal-large">
          <DialogHeader>
            <DialogTitle>Train New Adapter</DialogTitle>
          </DialogHeader>
          <TrainingWizard onClose={() => setIsTrainingDialogOpen(false)} />
        </DialogContent>
      </Dialog>

      {/* Register Adapter Dialog */}
      <Dialog open={isCreateDialogOpen} onOpenChange={setIsCreateDialogOpen}>
        <DialogContent className="modal-standard">
          <DialogHeader>
            <DialogTitle>Register Adapter</DialogTitle>
          </DialogHeader>
          <div className="form-field">
            <Alert>
              <AlertTriangle className="icon-standard" />
              <AlertDescription>
                Adapter registration form coming soon. This feature will be available once the adapter management API is complete.
              </AlertDescription>
            </Alert>
            <div className="flex-standard justify-end">
              <Button variant="outline" onClick={() => setIsCreateDialogOpen(false)}>
                Close
              </Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>

      {/* Delete Confirmation Dialog */}
      <DeleteConfirmDialog
        open={deleteConfirmId !== null}
        adapterId={deleteConfirmId}
        onConfirm={handleDeleteAdapter}
        onCancel={() => setDeleteConfirmId(null)}
      />

      {/* Health Modal */}
      <Dialog open={showHealthModal} onOpenChange={setShowHealthModal}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Adapter Health</DialogTitle>
          </DialogHeader>
          {healthData && (
            <div className="space-y-4">
              <div>
                <Label>Status</Label>
                <Badge variant={healthData.is_healthy ? 'default' : 'destructive'}>
                  {healthData.is_healthy ? 'Healthy' : 'Unhealthy'}
                </Badge>
              </div>
              <div>
                <Label>Load Time</Label>
                <p>{healthData.load_time_ms}ms</p>
              </div>
              <div>
                <Label>Memory Usage</Label>
                <p>{Math.round(healthData.memory_usage_bytes / 1024 / 1024)} MB</p>
              </div>
              {healthData.error_message && (
                <div>
                  <Label>Error</Label>
                  <Alert variant="destructive">
                    <AlertDescription>{healthData.error_message}</AlertDescription>
                  </Alert>
                </div>
              )}
            </div>
          )}
        </DialogContent>
      </Dialog>
    </div>
  );
}

// Training Wizard Component (Stubbed)
function TrainingWizard({ onClose }: { onClose: () => void }) {
  return (
    <div className="space-y-6">
      <Alert>
        <AlertTriangle className="h-4 w-4" />
        <AlertDescription>
          Training API not yet implemented. This feature will be available once the training orchestration subsystem is complete. For now, focus on adapter management and deployment.
        </AlertDescription>
      </Alert>
      <div className="flex justify-end">
        <Button onClick={onClose}>Close</Button>
      </div>
    </div>
  );
}

// Training Wizard Component (Original - commented out)
/*
function TrainingWizardOriginal({ onClose }: { onClose: () => void }) {
  const [step, setStep] = useState(1);
  const [config, setConfig] = useState<Partial<TrainingConfig>>({
    rank: 16,
    alpha: 32,
    epochs: 3,
    learning_rate: 0.001,
    batch_size: 32,
    category: 'code',
    scope: 'global'
  });

  const steps = [
    { id: 1, title: 'Configuration', description: 'Set training parameters' },
    { id: 2, title: 'Data Source', description: 'Choose training data' },
    { id: 3, title: 'Review', description: 'Confirm settings' }
  ];

  return (
    <div className="space-y-6">
      <div className="flex items-center space-x-4">
        {steps.map((s) => (
          <div key={s.id} className="flex items-center">
            <div className={`flex items-center justify-center w-8 h-8 rounded-full ${
              step >= s.id ? 'bg-primary text-primary-foreground' : 'bg-gray-200'
            }`}>
              {s.id}
            </div>
            <div className="ml-2">
              <div className="text-sm font-medium">{s.title}</div>
              <div className="text-xs text-muted-foreground">{s.description}</div>
            </div>
            {s.id < steps.length && <div className="w-8 h-px bg-gray-200 ml-4" />}
          </div>
        ))}
      </div>

      {step === 1 && (
        <div className="space-y-4">
          <div className="grid grid-cols-2 gap-4">
            <div>
              <Label htmlFor="name">Adapter Name</Label>
              <Input id="name" placeholder="my-adapter-v1" />
            </div>
            <div>
              <Label htmlFor="category">Category</Label>
              <Select>
                <SelectTrigger>
                  <SelectValue placeholder="Select category" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="code">Code</SelectItem>
                  <SelectItem value="framework">Framework</SelectItem>
                  <SelectItem value="codebase">Codebase</SelectItem>
                  <SelectItem value="ephemeral">Ephemeral</SelectItem>
                </SelectContent>
              </Select>
            </div>
          </div>

          <div className="grid grid-cols-3 gap-4">
            <div>
              <Label htmlFor="rank">Rank</Label>
              <Input 
                id="rank" 
                type="number" 
                value={config.rank} 
                onChange={(e) => setConfig({...config, rank: parseInt(e.target.value)})}
              />
            </div>
            <div>
              <Label htmlFor="alpha">Alpha</Label>
              <Input 
                id="alpha" 
                type="number" 
                value={config.alpha} 
                onChange={(e) => setConfig({...config, alpha: parseInt(e.target.value)})}
              />
            </div>
            <div>
              <Label htmlFor="epochs">Epochs</Label>
              <Input 
                id="epochs" 
                type="number" 
                value={config.epochs} 
                onChange={(e) => setConfig({...config, epochs: parseInt(e.target.value)})}
              />
            </div>
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div>
              <Label htmlFor="learning_rate">Learning Rate</Label>
              <Input 
                id="learning_rate" 
                type="number" 
                step="0.0001"
                value={config.learning_rate} 
                onChange={(e) => setConfig({...config, learning_rate: parseFloat(e.target.value)})}
              />
            </div>
            <div>
              <Label htmlFor="batch_size">Batch Size</Label>
              <Input 
                id="batch_size" 
                type="number" 
                value={config.batch_size} 
                onChange={(e) => setConfig({...config, batch_size: parseInt(e.target.value)})}
              />
            </div>
          </div>
        </div>
      )}

      {step === 2 && (
        <div className="space-y-4">
          <div>
            <Label htmlFor="scope">Scope</Label>
            <Select>
              <SelectTrigger>
                <SelectValue placeholder="Select scope" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="global">Global</SelectItem>
                <SelectItem value="tenant">Tenant</SelectItem>
                <SelectItem value="repo">Repository</SelectItem>
                <SelectItem value="commit">Commit</SelectItem>
              </SelectContent>
            </Select>
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div>
              <Label htmlFor="repo_id">Repository ID</Label>
              <Input id="repo_id" placeholder="acme/payments" />
            </div>
            <div>
              <Label htmlFor="commit_sha">Commit SHA</Label>
              <Input id="commit_sha" placeholder="abc123def456" />
            </div>
          </div>

          <div>
            <Label htmlFor="framework">Framework</Label>
            <Select>
              <SelectTrigger>
                <SelectValue placeholder="Select framework (optional)" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="django">Django</SelectItem>
                <SelectItem value="react">React</SelectItem>
                <SelectItem value="fastapi">FastAPI</SelectItem>
                <SelectItem value="nextjs">Next.js</SelectItem>
              </SelectContent>
            </Select>
          </div>
        </div>
      )}

      {step === 3 && (
        <div className="space-y-4">
          <Alert>
            <CheckCircle className="h-4 w-4" />
            <AlertDescription>
              Ready to start training. This will consume GPU resources and may take several hours.
            </AlertDescription>
          </Alert>

          <div className="bg-gray-50 p-4 rounded-md">
            <h4 className="font-medium mb-2">Training Configuration</h4>
            <div className="space-y-1 text-sm">
              <div>Name: {config.name || 'my-adapter-v1'}</div>
              <div>Category: {config.category}</div>
              <div>Rank: {config.rank} • Alpha: {config.alpha}</div>
              <div>Epochs: {config.epochs} • Learning Rate: {config.learning_rate}</div>
              <div>Batch Size: {config.batch_size}</div>
            </div>
          </div>
        </div>
      )}

      <div className="flex justify-between">
        <Button variant="outline" onClick={onClose}>
          Cancel
        </Button>
        <div className="flex space-x-2">
          {step > 1 && (
            <Button variant="outline" onClick={() => setStep(step - 1)}>
              Previous
            </Button>
          )}
          {step < 3 ? (
            <Button onClick={() => setStep(step + 1)}>
              Next
            </Button>
          ) : (
            <Button onClick={() => {
              // TODO: Start training
              onClose();
            }}>
              Start Training
            </Button>
          )}
        </div>
      </div>
    </div>
  );
}

// Register Adapter Form Component
function RegisterAdapterForm({ onClose }: { onClose: () => void }) {
  const [formData, setFormData] = useState({
    name: '',
    adapter_hash: '',
    capability_tags: '',
    tier: 'persistent',
    rank: 16,
    framework: '',
    framework_version: ''
  });

  return (
    <div className="space-y-4">
      <div>
        <Label htmlFor="name">Adapter Name</Label>
        <Input 
          id="name" 
          value={formData.name}
          onChange={(e) => setFormData({...formData, name: e.target.value})}
          placeholder="my-adapter-v1"
        />
      </div>

      <div>
        <Label htmlFor="adapter_hash">Adapter Hash</Label>
        <Input 
          id="adapter_hash" 
          value={formData.adapter_hash}
          onChange={(e) => setFormData({...formData, adapter_hash: e.target.value})}
          placeholder="b3:abc123..."
        />
      </div>

      <div>
        <Label htmlFor="capability_tags">Capability Tags</Label>
        <Input 
          id="capability_tags" 
          value={formData.capability_tags}
          onChange={(e) => setFormData({...formData, capability_tags: e.target.value})}
          placeholder="python,django,web"
        />
      </div>

      <div className="grid grid-cols-2 gap-4">
        <div>
          <Label htmlFor="tier">Tier</Label>
          <Select>
            <SelectTrigger>
              <SelectValue placeholder="Select tier" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="persistent">Persistent</SelectItem>
              <SelectItem value="ephemeral">Ephemeral</SelectItem>
            </SelectContent>
          </Select>
        </div>
        <div>
          <Label htmlFor="rank">Rank</Label>
          <Input 
            id="rank" 
            type="number" 
            value={formData.rank}
            onChange={(e) => setFormData({...formData, rank: parseInt(e.target.value)})}
          />
        </div>
      </div>

      <div className="grid grid-cols-2 gap-4">
        <div>
          <Label htmlFor="framework">Framework</Label>
          <Input 
            id="framework" 
            value={formData.framework}
            onChange={(e) => setFormData({...formData, framework: e.target.value})}
            placeholder="django"
          />
        </div>
        <div>
          <Label htmlFor="framework_version">Framework Version</Label>
          <Input 
            id="framework_version" 
            value={formData.framework_version}
            onChange={(e) => setFormData({...formData, framework_version: e.target.value})}
            placeholder="4.2"
          />
        </div>
      </div>

      <div className="flex justify-end space-x-2">
        <Button variant="outline" onClick={onClose}>
          Cancel
        </Button>
        <Button onClick={() => {
          // TODO: Register adapter - wire to apiClient.registerAdapter()
          toast.info('Adapter registration coming soon');
          onClose();
        }}>
          Register Adapter
        </Button>
      </div>
    </div>
  );
}
*/

// Delete Confirmation Dialog
function DeleteConfirmDialog({ 
  open, 
  adapterId, 
  onConfirm, 
  onCancel 
}: { 
  open: boolean; 
  adapterId: string | null; 
  onConfirm: (id: string) => void; 
  onCancel: () => void;
}) {
  if (!open || !adapterId) return null;

  return (
    <Dialog open={open} onOpenChange={(open) => !open && onCancel()}>
      <DialogContent className="modal-standard">
        <DialogHeader>
          <DialogTitle>Confirm Delete</DialogTitle>
        </DialogHeader>
        <Alert variant="destructive">
          <AlertTriangle className="icon-standard" />
          <AlertDescription>
            Are you sure you want to delete adapter <code className="font-mono">{adapterId}</code>? This action cannot be undone.
          </AlertDescription>
        </Alert>
        <div className="flex-standard justify-end">
          <Button variant="outline" onClick={onCancel}>
            Cancel
          </Button>
          <Button variant="destructive" onClick={() => onConfirm(adapterId)}>
            Delete Adapter
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
