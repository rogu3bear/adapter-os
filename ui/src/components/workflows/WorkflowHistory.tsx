// WorkflowHistory component - View and manage past workflow executions

import React, { useState, useMemo } from 'react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '../ui/card';
import { Button } from '../ui/button';
import { Badge } from '../ui/badge';
import { Input } from '../ui/input';
import { Label } from '../ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '../ui/select';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '../ui/table';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '../ui/dialog';
import {
  CheckCircle,
  XCircle,
  Clock,
  Download,
  RotateCcw,
  Search,
  Eye,
  Trash2,
  Filter,
  AlertCircle,
} from 'lucide-react';
import { WorkflowExecution, WorkflowStatus } from './types';
import { toast } from 'sonner';
import { useTimestamp } from '../../hooks/useTimestamp';

interface WorkflowHistoryProps {
  executions: WorkflowExecution[];
  onReplay?: (execution: WorkflowExecution) => void;
  onDelete?: (executionId: string) => void;
  onExport?: (execution: WorkflowExecution) => void;
}

const STATUS_CONFIG: Record<
  WorkflowStatus,
  { icon: React.ComponentType<any>; color: string; label: string }
> = {
  pending: {
    icon: Clock,
    color: 'text-gray-500',
    label: 'Pending',
  },
  running: {
    icon: Clock,
    color: 'text-blue-500',
    label: 'Running',
  },
  paused: {
    icon: AlertCircle,
    color: 'text-yellow-500',
    label: 'Paused',
  },
  completed: {
    icon: CheckCircle,
    color: 'text-green-500',
    label: 'Completed',
  },
  failed: {
    icon: XCircle,
    color: 'text-red-500',
    label: 'Failed',
  },
  cancelled: {
    icon: XCircle,
    color: 'text-gray-500',
    label: 'Cancelled',
  },
};

export function WorkflowHistory({
  executions,
  onReplay,
  onDelete,
  onExport,
}: WorkflowHistoryProps) {
  const [searchQuery, setSearchQuery] = useState('');
  const [statusFilter, setStatusFilter] = useState<string>('all');
  const [selectedExecution, setSelectedExecution] = useState<WorkflowExecution | null>(null);
  const [showDetails, setShowDetails] = useState(false);

  // Filter executions
  const filteredExecutions = useMemo(() => {
    return executions.filter((exec) => {
      // Search filter
      const matchesSearch =
        !searchQuery ||
        exec.templateName.toLowerCase().includes(searchQuery.toLowerCase()) ||
        exec.id.toLowerCase().includes(searchQuery.toLowerCase());

      // Status filter
      const matchesStatus = statusFilter === 'all' || exec.status === statusFilter;

      return matchesSearch && matchesStatus;
    });
  }, [executions, searchQuery, statusFilter]);

  const handleViewDetails = (execution: WorkflowExecution) => {
    setSelectedExecution(execution);
    setShowDetails(true);
  };

  const handleReplay = (execution: WorkflowExecution) => {
    if (onReplay) {
      onReplay(execution);
      toast.success(`Replaying workflow: ${execution.templateName}`);
    }
  };

  const handleDelete = (executionId: string) => {
    if (confirm('Are you sure you want to delete this workflow execution?')) {
      if (onDelete) {
        onDelete(executionId);
        toast.success('Workflow execution deleted');
      }
    }
  };

  const handleExport = (execution: WorkflowExecution) => {
    if (onExport) {
      onExport(execution);
      toast.success('Workflow report exported');
    } else {
      // Default export to JSON
      const dataStr = JSON.stringify(execution, null, 2);
      const dataBlob = new Blob([dataStr], { type: 'application/json' });
      const url = URL.createObjectURL(dataBlob);
      const link = document.createElement('a');
      link.href = url;
      link.download = `workflow-${execution.id}.json`;
      link.click();
      URL.revokeObjectURL(url);
      toast.success('Workflow exported as JSON');
    }
  };

  const calculateDuration = (execution: WorkflowExecution): string => {
    if (!execution.completedAt) return 'In progress';

    const start = new Date(execution.startedAt).getTime();
    const end = new Date(execution.completedAt).getTime();
    const durationMs = end - start;

    const minutes = Math.floor(durationMs / 60000);
    const seconds = Math.floor((durationMs % 60000) / 1000);

    return `${minutes}m ${seconds}s`;
  };

  const getSuccessRate = (execution: WorkflowExecution): number => {
    if (!execution.results || execution.results.length === 0) return 0;

    const successful = execution.results.filter((r) => r.status === 'success').length;
    return Math.round((successful / execution.results.length) * 100);
  };

  return (
    <div className="space-y-6">
      {/* Header */}
      <div>
        <h2 className="text-2xl font-bold">Workflow History</h2>
        <p className="text-sm text-muted-foreground mt-1">
          View and manage past workflow executions
        </p>
      </div>

      {/* Filters */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        <div className="space-y-2">
          <Label htmlFor="search">Search</Label>
          <div className="relative">
            <Search className="absolute left-2 top-2.5 h-4 w-4 text-muted-foreground" />
            <Input
              id="search"
              placeholder="Search by name or ID..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="pl-8"
            />
          </div>
        </div>

        <div className="space-y-2">
          <Label htmlFor="status">Status Filter</Label>
          <Select value={statusFilter} onValueChange={setStatusFilter}>
            <SelectTrigger id="status">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">All Statuses</SelectItem>
              <SelectItem value="completed">Completed</SelectItem>
              <SelectItem value="failed">Failed</SelectItem>
              <SelectItem value="running">Running</SelectItem>
              <SelectItem value="cancelled">Cancelled</SelectItem>
            </SelectContent>
          </Select>
        </div>
      </div>

      {/* Results Count */}
      <div className="flex items-center justify-between">
        <p className="text-sm text-muted-foreground">
          Showing {filteredExecutions.length} of {executions.length} executions
        </p>
        {(searchQuery || statusFilter !== 'all') && (
          <Button
            variant="ghost"
            size="sm"
            onClick={() => {
              setSearchQuery('');
              setStatusFilter('all');
            }}
          >
            Clear Filters
          </Button>
        )}
      </div>

      {/* Executions Table */}
      {filteredExecutions.length > 0 ? (
        <Card>
          <CardContent className="p-0">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Workflow</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead>Started</TableHead>
                  <TableHead>Duration</TableHead>
                  <TableHead>Success Rate</TableHead>
                  <TableHead className="text-right">Actions</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {filteredExecutions.map((execution) => {
                  const StatusIcon = STATUS_CONFIG[execution.status].icon;

                  return (
                    <TableRow key={execution.id}>
                      <TableCell>
                        <div className="space-y-1">
                          <p className="font-medium">{execution.templateName}</p>
                          <p className="text-xs text-muted-foreground font-mono">
                            {execution.id}
                          </p>
                        </div>
                      </TableCell>

                      <TableCell>
                        <Badge
                          variant="outline"
                          className={`${STATUS_CONFIG[execution.status].color}`}
                        >
                          <StatusIcon className="h-3 w-3 mr-1" />
                          {STATUS_CONFIG[execution.status].label}
                        </Badge>
                      </TableCell>

                      <TableCell>
                        <RelativeTime timestamp={execution.startedAt} />
                      </TableCell>

                      <TableCell>{calculateDuration(execution)}</TableCell>

                      <TableCell>
                        <div className="flex items-center gap-2">
                          <div className="flex-1 bg-muted rounded-full h-2 overflow-hidden">
                            <div
                              className={`h-full ${
                                getSuccessRate(execution) === 100
                                  ? 'bg-green-500'
                                  : getSuccessRate(execution) >= 50
                                  ? 'bg-yellow-500'
                                  : 'bg-red-500'
                              }`}
                              style={{ width: `${getSuccessRate(execution)}%` }}
                            />
                          </div>
                          <span className="text-xs font-medium">
                            {getSuccessRate(execution)}%
                          </span>
                        </div>
                      </TableCell>

                      <TableCell className="text-right">
                        <div className="flex items-center justify-end gap-1">
                          <Button
                            variant="ghost"
                            size="sm"
                            onClick={() => handleViewDetails(execution)}
                          >
                            <Eye className="h-4 w-4" />
                          </Button>

                          {onReplay && execution.status === 'completed' && (
                            <Button
                              variant="ghost"
                              size="sm"
                              onClick={() => handleReplay(execution)}
                            >
                              <RotateCcw className="h-4 w-4" />
                            </Button>
                          )}

                          <Button
                            variant="ghost"
                            size="sm"
                            onClick={() => handleExport(execution)}
                          >
                            <Download className="h-4 w-4" />
                          </Button>

                          {onDelete && (
                            <Button
                              variant="ghost"
                              size="sm"
                              onClick={() => handleDelete(execution.id)}
                            >
                              <Trash2 className="h-4 w-4 text-destructive" />
                            </Button>
                          )}
                        </div>
                      </TableCell>
                    </TableRow>
                  );
                })}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      ) : (
        <Card className="p-8">
          <div className="text-center space-y-3">
            <Filter className="h-12 w-12 text-muted-foreground mx-auto" />
            <h3 className="text-lg font-semibold">No executions found</h3>
            <p className="text-sm text-muted-foreground">
              {searchQuery || statusFilter !== 'all'
                ? 'Try adjusting your filters'
                : 'No workflow executions yet'}
            </p>
          </div>
        </Card>
      )}

      {/* Details Dialog */}
      {selectedExecution && (
        <Dialog open={showDetails} onOpenChange={setShowDetails}>
          <DialogContent className="max-w-3xl max-h-[80vh] overflow-y-auto">
            <DialogHeader>
              <DialogTitle>{selectedExecution.templateName}</DialogTitle>
              <DialogDescription>Execution ID: {selectedExecution.id}</DialogDescription>
            </DialogHeader>

            <div className="space-y-4">
              {/* Summary */}
              <Card>
                <CardHeader>
                  <CardTitle className="text-base">Summary</CardTitle>
                </CardHeader>
                <CardContent className="grid grid-cols-2 gap-4 text-sm">
                  <div>
                    <p className="text-muted-foreground">Status</p>
                    <p className="font-medium capitalize">{selectedExecution.status}</p>
                  </div>
                  <div>
                    <p className="text-muted-foreground">Duration</p>
                    <p className="font-medium">{calculateDuration(selectedExecution)}</p>
                  </div>
                  <div>
                    <p className="text-muted-foreground">Steps Completed</p>
                    <p className="font-medium">
                      {selectedExecution.currentStep} / {selectedExecution.totalSteps}
                    </p>
                  </div>
                  <div>
                    <p className="text-muted-foreground">Success Rate</p>
                    <p className="font-medium">{getSuccessRate(selectedExecution)}%</p>
                  </div>
                </CardContent>
              </Card>

              {/* Step Results */}
              {selectedExecution.results && selectedExecution.results.length > 0 && (
                <Card>
                  <CardHeader>
                    <CardTitle className="text-base">Step Results</CardTitle>
                  </CardHeader>
                  <CardContent className="space-y-3">
                    {selectedExecution.results.map((result) => {
                      const Icon =
                        result.status === 'success'
                          ? CheckCircle
                          : result.status === 'failure'
                          ? XCircle
                          : Clock;
                      const color =
                        result.status === 'success'
                          ? 'text-green-500'
                          : result.status === 'failure'
                          ? 'text-red-500'
                          : 'text-gray-500';

                      return (
                        <div key={result.stepId} className="flex items-start gap-3 p-3 border rounded-lg">
                          <Icon className={`h-5 w-5 ${color} flex-shrink-0 mt-0.5`} />
                          <div className="flex-1 min-w-0 space-y-1">
                            <div className="flex items-center justify-between">
                              <p className="font-medium text-sm">{result.stepTitle}</p>
                              {result.duration && (
                                <span className="text-xs text-muted-foreground">
                                  {Math.round(result.duration)}ms
                                </span>
                              )}
                            </div>
                            {result.error && (
                              <p className="text-xs text-red-600">{result.error}</p>
                            )}
                          </div>
                        </div>
                      );
                    })}
                  </CardContent>
                </Card>
              )}

              {/* Error Details */}
              {selectedExecution.error && (
                <Card>
                  <CardHeader>
                    <CardTitle className="text-base text-red-600">Error</CardTitle>
                  </CardHeader>
                  <CardContent>
                    <pre className="text-xs bg-red-50 dark:bg-red-950 p-3 rounded overflow-auto">
                      {selectedExecution.error}
                    </pre>
                  </CardContent>
                </Card>
              )}
            </div>

            <DialogFooter>
              <Button variant="outline" onClick={() => setShowDetails(false)}>
                Close
              </Button>
              <Button onClick={() => handleExport(selectedExecution)}>
                <Download className="h-4 w-4 mr-2" />
                Export Report
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      )}
    </div>
  );
}

// Helper component for relative timestamps
function RelativeTime({ timestamp }: { timestamp: string }) {
  const { relativeTime } = useTimestamp();
  return <span className="text-sm">{relativeTime(timestamp)}</span>;
}
