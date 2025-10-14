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
import { ScrollArea } from './ui/scroll-area';
import { 
  Bug, 
  Play, 
  Square, 
  Download, 
  Upload,
  Eye,
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
  Terminal,
  Code,
  Settings,
  RefreshCw,
  Filter,
  Search
} from 'lucide-react';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from './ui/dropdown-menu';
import apiClient from '../api/client';
import { toast } from 'sonner';

interface ProcessDebuggerProps {
  workerId: string;
  workerName?: string;
  onClose?: () => void;
}

interface ProcessLog {
  id: string;
  worker_id: string;
  level: string;
  message: string;
  timestamp: string;
  metadata_json?: string;
}

interface ProcessCrashDump {
  id: string;
  worker_id: string;
  crash_type: string;
  stack_trace?: string;
  memory_snapshot_json?: string;
  crash_timestamp: string;
  recovery_action?: string;
  recovered_at?: string;
}

interface ProcessDebugSession {
  id: string;
  worker_id: string;
  session_type: string;
  status: string;
  config_json: string;
  started_at: string;
  ended_at?: string;
  results_json?: string;
}

interface ProcessTroubleshootingStep {
  id: string;
  worker_id: string;
  step_name: string;
  step_type: string;
  status: string;
  command?: string;
  output?: string;
  error_message?: string;
  started_at: string;
  completed_at?: string;
}

export function ProcessDebugger({ workerId, workerName, onClose }: ProcessDebuggerProps) {
  const [activeTab, setActiveTab] = useState('logs');
  const [logs, setLogs] = useState<ProcessLog[]>([]);
  const [crashes, setCrashes] = useState<ProcessCrashDump[]>([]);
  const [debugSessions, setDebugSessions] = useState<ProcessDebugSession[]>([]);
  const [troubleshootingSteps, setTroubleshootingSteps] = useState<ProcessTroubleshootingStep[]>([]);
  const [loading, setLoading] = useState(true);
  const [showDebugModal, setShowDebugModal] = useState(false);
  const [showTroubleshootModal, setShowTroubleshootModal] = useState(false);
  
  // Filters
  const [logLevelFilter, setLogLevelFilter] = useState<string>('');
  const [searchFilter, setSearchFilter] = useState<string>('');

  const fetchLogs = async () => {
    try {
      setLoading(true);
      // Citation: ui/src/api/client.ts L748-L755
      const data = await apiClient.getProcessLogs(workerId, { level: logLevelFilter });
      setLogs(data);
    } catch (error) {
      console.error('Failed to fetch logs:', error);
      toast.error('Failed to load process logs');
    } finally {
      setLoading(false);
    }
  };

  const fetchCrashes = async () => {
    try {
      // Citation: ui/src/api/client.ts L758-L760
      const data = await apiClient.getProcessCrashes(workerId);
      setCrashes(data);
    } catch (error) {
      console.error('Failed to fetch crashes:', error);
      toast.error('Failed to load crash dumps');
    }
  };

  const fetchDebugSessions = async () => {
    try {
      // Citation: ui/src/api/client.ts L762-L767
      const data = await apiClient.startDebugSession(workerId, {
        session_type: 'interactive',
        max_duration_ms: 300000, // 5 minutes
      });
      setDebugSessions([data]);
    } catch (error) {
      console.error('Failed to fetch debug sessions:', error);
      toast.error('Failed to load debug sessions');
    }
  };

  const fetchTroubleshootingSteps = async () => {
    try {
      // Citation: ui/src/api/client.ts L769-L774
      const data = await apiClient.runTroubleshootingStep(workerId, {
        step_type: 'memory_analysis',
        parameters: { threshold: 0.8 }
      });
      setTroubleshootingSteps([data]);
    } catch (error) {
      console.error('Failed to fetch troubleshooting steps:', error);
      toast.error('Failed to load troubleshooting steps');
    }
  };

  useEffect(() => {
    fetchLogs();
    fetchCrashes();
    fetchDebugSessions();
    fetchTroubleshootingSteps();
  }, [workerId]);

  const getLogLevelColor = (level: string) => {
    switch (level) {
      case 'error':
      case 'fatal':
        return 'text-red-600 bg-red-50';
      case 'warn':
        return 'text-yellow-600 bg-yellow-50';
      case 'info':
        return 'text-blue-600 bg-blue-50';
      case 'debug':
        return 'text-gray-600 bg-gray-50';
      default:
        return 'text-gray-600 bg-gray-50';
    }
  };

  const getLogLevelIcon = (level: string) => {
    switch (level) {
      case 'error':
      case 'fatal':
        return <XCircle className="h-4 w-4" />;
      case 'warn':
        return <AlertTriangle className="h-4 w-4" />;
      case 'info':
        return <CheckCircle className="h-4 w-4" />;
      case 'debug':
        return <Bug className="h-4 w-4" />;
      default:
        return <Activity className="h-4 w-4" />;
    }
  };

  const filteredLogs = logs.filter(log => {
    const matchesLevel = !logLevelFilter || log.level === logLevelFilter;
    const matchesSearch = !searchFilter || 
      log.message.toLowerCase().includes(searchFilter.toLowerCase()) ||
      log.level.toLowerCase().includes(searchFilter.toLowerCase());
    return matchesLevel && matchesSearch;
  });

  if (loading) {
    return <div className="text-center p-8">Loading process debugger...</div>;
  }

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex-between">
        <div>
          <h2 className="text-2xl font-bold">Process Debugger</h2>
          <p className="text-sm text-muted-foreground">
            Debug and troubleshoot worker process: {workerName || workerId}
          </p>
        </div>
        <div className="flex gap-2">
          <Button variant="outline" size="sm" onClick={fetchLogs}>
            <RefreshCw className="h-4 w-4 mr-2" />
            Refresh
          </Button>
          <Button onClick={() => setShowDebugModal(true)}>
            <Bug className="h-4 w-4 mr-2" />
            Start Debug Session
          </Button>
          <Button variant="outline" onClick={() => setShowTroubleshootModal(true)}>
            <Settings className="h-4 w-4 mr-2" />
            Troubleshoot
          </Button>
          {onClose && (
            <Button variant="outline" onClick={onClose}>
              Close
            </Button>
          )}
        </div>
      </div>

      {/* Debug Tabs */}
      <Tabs value={activeTab} onValueChange={setActiveTab}>
        <TabsList>
          <TabsTrigger value="logs">
            <FileText className="h-4 w-4 mr-2" />
            Logs
          </TabsTrigger>
          <TabsTrigger value="crashes">
            <AlertTriangle className="h-4 w-4 mr-2" />
            Crashes
          </TabsTrigger>
          <TabsTrigger value="sessions">
            <Terminal className="h-4 w-4 mr-2" />
            Debug Sessions
          </TabsTrigger>
          <TabsTrigger value="troubleshoot">
            <Settings className="h-4 w-4 mr-2" />
            Troubleshooting
          </TabsTrigger>
        </TabsList>

        {/* Logs Tab */}
        <TabsContent value="logs" className="space-y-4">
          {/* Log Filters */}
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <Filter className="h-4 w-4" />
                Log Filters
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                <div>
                  <Label htmlFor="log-level">Log Level</Label>
                  <Select value={logLevelFilter} onValueChange={setLogLevelFilter}>
                    <SelectTrigger>
                      <SelectValue placeholder="All levels" />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="">All levels</SelectItem>
                      <SelectItem value="debug">Debug</SelectItem>
                      <SelectItem value="info">Info</SelectItem>
                      <SelectItem value="warn">Warning</SelectItem>
                      <SelectItem value="error">Error</SelectItem>
                      <SelectItem value="fatal">Fatal</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
                <div>
                  <Label htmlFor="search">Search</Label>
                  <Input
                    id="search"
                    placeholder="Search logs..."
                    value={searchFilter}
                    onChange={(e) => setSearchFilter(e.target.value)}
                  />
                </div>
              </div>
            </CardContent>
          </Card>

          {/* Logs Table */}
          <Card>
            <CardHeader>
              <CardTitle>
                Process Logs ({filteredLogs.length})
              </CardTitle>
            </CardHeader>
            <CardContent>
              <ScrollArea className="h-96">
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Level</TableHead>
                      <TableHead>Message</TableHead>
                      <TableHead>Timestamp</TableHead>
                      <TableHead>Actions</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {filteredLogs.map((log) => (
                      <TableRow key={log.id}>
                        <TableCell>
                          <Badge className={`gap-1 ${getLogLevelColor(log.level)}`}>
                            {getLogLevelIcon(log.level)}
                            {log.level.toUpperCase()}
                          </Badge>
                        </TableCell>
                        <TableCell className="font-mono text-sm">
                          {log.message}
                        </TableCell>
                        <TableCell className="text-xs">
                          {new Date(log.timestamp).toLocaleString()}
                        </TableCell>
                        <TableCell>
                          <Button variant="ghost" size="sm">
                            <Eye className="h-4 w-4" />
                          </Button>
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              </ScrollArea>
            </CardContent>
          </Card>
        </TabsContent>

        {/* Crashes Tab */}
        <TabsContent value="crashes" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle>
                Process Crashes ({crashes.length})
              </CardTitle>
            </CardHeader>
            <CardContent>
              {crashes.length === 0 ? (
                <div className="text-center p-8 text-muted-foreground">
                  No crash dumps found for this worker.
                </div>
              ) : (
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Crash Type</TableHead>
                      <TableHead>Timestamp</TableHead>
                      <TableHead>Recovery Action</TableHead>
                      <TableHead>Status</TableHead>
                      <TableHead>Actions</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {crashes.map((crash) => (
                      <TableRow key={crash.id}>
                        <TableCell>
                          <Badge variant="destructive">
                            {crash.crash_type}
                          </Badge>
                        </TableCell>
                        <TableCell className="text-xs">
                          {new Date(crash.crash_timestamp).toLocaleString()}
                        </TableCell>
                        <TableCell>
                          {crash.recovery_action || 'None'}
                        </TableCell>
                        <TableCell>
                          <Badge variant={crash.recovered_at ? 'default' : 'destructive'}>
                            {crash.recovered_at ? 'Recovered' : 'Failed'}
                          </Badge>
                        </TableCell>
                        <TableCell>
                          <Button variant="ghost" size="sm">
                            <Eye className="h-4 w-4" />
                          </Button>
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              )}
            </CardContent>
          </Card>
        </TabsContent>

        {/* Debug Sessions Tab */}
        <TabsContent value="sessions" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle>
                Debug Sessions ({debugSessions.length})
              </CardTitle>
            </CardHeader>
            <CardContent>
              {debugSessions.length === 0 ? (
                <div className="text-center p-8 text-muted-foreground">
                  No debug sessions found for this worker.
                </div>
              ) : (
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Type</TableHead>
                      <TableHead>Status</TableHead>
                      <TableHead>Started</TableHead>
                      <TableHead>Duration</TableHead>
                      <TableHead>Actions</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {debugSessions.map((session) => (
                      <TableRow key={session.id}>
                        <TableCell>
                          <Badge variant="outline">
                            {session.session_type}
                          </Badge>
                        </TableCell>
                        <TableCell>
                          <Badge variant={session.status === 'active' ? 'default' : 'secondary'}>
                            {session.status}
                          </Badge>
                        </TableCell>
                        <TableCell className="text-xs">
                          {new Date(session.started_at).toLocaleString()}
                        </TableCell>
                        <TableCell className="text-xs">
                          {session.ended_at 
                            ? `${Math.floor((new Date(session.ended_at).getTime() - new Date(session.started_at).getTime()) / 1000)}s`
                            : 'Running'
                          }
                        </TableCell>
                        <TableCell>
                          <Button variant="ghost" size="sm">
                            <Eye className="h-4 w-4" />
                          </Button>
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              )}
            </CardContent>
          </Card>
        </TabsContent>

        {/* Troubleshooting Tab */}
        <TabsContent value="troubleshoot" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle>
                Troubleshooting Steps ({troubleshootingSteps.length})
              </CardTitle>
            </CardHeader>
            <CardContent>
              {troubleshootingSteps.length === 0 ? (
                <div className="text-center p-8 text-muted-foreground">
                  No troubleshooting steps found for this worker.
                </div>
              ) : (
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Step Name</TableHead>
                      <TableHead>Type</TableHead>
                      <TableHead>Status</TableHead>
                      <TableHead>Started</TableHead>
                      <TableHead>Actions</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {troubleshootingSteps.map((step) => (
                      <TableRow key={step.id}>
                        <TableCell className="font-medium">
                          {step.step_name}
                        </TableCell>
                        <TableCell>
                          <Badge variant="outline">
                            {step.step_type}
                          </Badge>
                        </TableCell>
                        <TableCell>
                          <Badge variant={
                            step.status === 'completed' ? 'default' :
                            step.status === 'failed' ? 'destructive' :
                            step.status === 'running' ? 'secondary' : 'outline'
                          }>
                            {step.status}
                          </Badge>
                        </TableCell>
                        <TableCell className="text-xs">
                          {new Date(step.started_at).toLocaleString()}
                        </TableCell>
                        <TableCell>
                          <Button variant="ghost" size="sm">
                            <Eye className="h-4 w-4" />
                          </Button>
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              )}
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>

      {/* Debug Session Modal */}
      <Dialog open={showDebugModal} onOpenChange={setShowDebugModal}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Start Debug Session</DialogTitle>
          </DialogHeader>
          <div className="space-y-4">
            <Alert>
              <AlertTriangle className="h-4 w-4" />
              <AlertDescription>
                Debug sessions will collect detailed information about the worker process.
                This may impact performance.
              </AlertDescription>
            </Alert>
            <div>
              <Label htmlFor="session-type">Session Type</Label>
              <Select>
                <SelectTrigger>
                  <SelectValue placeholder="Select session type" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="live">Live Debugging</SelectItem>
                  <SelectItem value="replay">Replay Analysis</SelectItem>
                  <SelectItem value="analysis">Performance Analysis</SelectItem>
                </SelectContent>
              </Select>
            </div>
          </div>
          <div className="flex justify-end space-x-2">
            <Button variant="outline" onClick={() => setShowDebugModal(false)}>
              Cancel
            </Button>
            <Button onClick={() => {
              toast.success('Debug session started');
              setShowDebugModal(false);
            }}>
              Start Session
            </Button>
          </div>
        </DialogContent>
      </Dialog>

      {/* Troubleshooting Modal */}
      <Dialog open={showTroubleshootModal} onOpenChange={setShowTroubleshootModal}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Run Troubleshooting Step</DialogTitle>
          </DialogHeader>
          <div className="space-y-4">
            <div>
              <Label htmlFor="step-name">Step Name</Label>
              <Select>
                <SelectTrigger>
                  <SelectValue placeholder="Select troubleshooting step" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="memory-check">Memory Usage Check</SelectItem>
                  <SelectItem value="deadlock-detection">Deadlock Detection</SelectItem>
                  <SelectItem value="performance-profile">Performance Profiling</SelectItem>
                  <SelectItem value="adapter-health">Adapter Health Check</SelectItem>
                  <SelectItem value="network-test">Network Connectivity Test</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div>
              <Label htmlFor="step-type">Step Type</Label>
              <Select>
                <SelectTrigger>
                  <SelectValue placeholder="Select step type" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="diagnostic">Diagnostic</SelectItem>
                  <SelectItem value="recovery">Recovery</SelectItem>
                  <SelectItem value="prevention">Prevention</SelectItem>
                </SelectContent>
              </Select>
            </div>
          </div>
          <div className="flex justify-end space-x-2">
            <Button variant="outline" onClick={() => setShowTroubleshootModal(false)}>
              Cancel
            </Button>
            <Button onClick={() => {
              toast.success('Troubleshooting step started');
              setShowTroubleshootModal(false);
            }}>
              Run Step
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}
