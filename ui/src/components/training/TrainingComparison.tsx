import React, { useState, useMemo } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '../ui/card';
import { Button } from '../ui/button';
import { Badge } from '../ui/badge';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '../ui/table';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from '../ui/dialog';
import { Checkbox } from '../ui/checkbox';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '../ui/select';
import {
  Download,
  GitCompare,
  TrendingUp,
  TrendingDown,
  Minus,
  CheckCircle2,
  XCircle,
  AlertCircle,
  Calendar,
  Clock,
  Zap,
  BarChart3,
  FileJson,
  FileSpreadsheet,
  FileText
} from 'lucide-react';
import { TrainingJob, TrainingConfig } from '../../api/types';
import { toast } from 'sonner';

interface TrainingComparisonProps {
  jobs: TrainingJob[];
  onClose?: () => void;
}

interface ComparisonMetric {
  name: string;
  getValue: (job: TrainingJob) => string | number | undefined;
  format?: (value: any) => string;
  compare?: (a: any, b: any) => 'better' | 'worse' | 'equal';
}

export function TrainingComparison({ jobs: allJobs, onClose }: TrainingComparisonProps) {
  const [selectedJobIds, setSelectedJobIds] = useState<string[]>([]);
  const [isSelectOpen, setIsSelectOpen] = useState(false);
  const [filterStatus, setFilterStatus] = useState<string>('all');
  const [sortBy, setSortBy] = useState<'date' | 'loss' | 'duration'>('date');

  // Filter and sort available jobs
  const availableJobs = useMemo(() => {
    let filtered = allJobs;

    // Filter by status
    if (filterStatus !== 'all') {
      filtered = filtered.filter(job => job.status === filterStatus);
    }

    // Sort
    return filtered.sort((a, b) => {
      switch (sortBy) {
        case 'date':
          return new Date(b.created_at).getTime() - new Date(a.created_at).getTime();
        case 'loss':
          const lossA = a.current_loss || Infinity;
          const lossB = b.current_loss || Infinity;
          return lossA - lossB;
        case 'duration':
          const durationA = a.completed_at && a.started_at
            ? new Date(a.completed_at).getTime() - new Date(a.started_at).getTime()
            : 0;
          const durationB = b.completed_at && b.started_at
            ? new Date(b.completed_at).getTime() - new Date(b.started_at).getTime()
            : 0;
          return durationB - durationA;
        default:
          return 0;
      }
    });
  }, [allJobs, filterStatus, sortBy]);

  // Get selected jobs for comparison
  const selectedJobs = useMemo(() => {
    return selectedJobIds
      .map(id => allJobs.find(job => job.id === id))
      .filter((job): job is TrainingJob => job !== undefined);
  }, [selectedJobIds, allJobs]);

  // Toggle job selection
  const toggleJobSelection = (jobId: string) => {
    setSelectedJobIds(prev => {
      if (prev.includes(jobId)) {
        return prev.filter(id => id !== jobId);
      } else if (prev.length < 4) {
        return [...prev, jobId];
      } else {
        toast.error('Maximum 4 jobs can be compared');
        return prev;
      }
    });
  };

  // Define comparison metrics
  const metrics: ComparisonMetric[] = [
    {
      name: 'Final Loss',
      getValue: (job) => job.current_loss,
      format: (val) => val?.toFixed(4) || 'N/A',
      compare: (a, b) => {
        if (a === undefined || b === undefined) return 'equal';
        return a < b ? 'better' : a > b ? 'worse' : 'equal';
      }
    },
    {
      name: 'Status',
      getValue: (job) => job.status,
      format: (val) => val || 'unknown'
    },
    {
      name: 'Progress',
      getValue: (job) => job.progress_pct || job.progress,
      format: (val) => val !== undefined ? `${val}%` : 'N/A'
    },
    {
      name: 'Epochs',
      getValue: (job) => job.total_epochs,
      format: (val) => val?.toString() || 'N/A'
    },
    {
      name: 'Current Epoch',
      getValue: (job) => job.current_epoch,
      format: (val) => val?.toString() || 'N/A'
    },
    {
      name: 'Learning Rate',
      getValue: (job) => job.learning_rate || job.config?.learning_rate,
      format: (val) => val?.toExponential(2) || 'N/A'
    },
    {
      name: 'Tokens/Second',
      getValue: (job) => job.tokens_per_second,
      format: (val) => val?.toFixed(0) || 'N/A',
      compare: (a, b) => {
        if (a === undefined || b === undefined) return 'equal';
        return a > b ? 'better' : a < b ? 'worse' : 'equal';
      }
    },
    {
      name: 'Duration',
      getValue: (job) => {
        if (job.completed_at && job.started_at) {
          const ms = new Date(job.completed_at).getTime() - new Date(job.started_at).getTime();
          return ms / 1000; // seconds
        }
        return undefined;
      },
      format: (val) => {
        if (val === undefined) return 'N/A';
        const hours = Math.floor(val / 3600);
        const minutes = Math.floor((val % 3600) / 60);
        const seconds = Math.floor(val % 60);
        if (hours > 0) return `${hours}h ${minutes}m`;
        if (minutes > 0) return `${minutes}m ${seconds}s`;
        return `${seconds}s`;
      },
      compare: (a, b) => {
        if (a === undefined || b === undefined) return 'equal';
        return a < b ? 'better' : a > b ? 'worse' : 'equal';
      }
    }
  ];

  // Define configuration comparison fields
  const configFields: Array<{ name: string; key: keyof TrainingConfig }> = [
    { name: 'Rank', key: 'rank' },
    { name: 'Alpha', key: 'alpha' },
    { name: 'Batch Size', key: 'batch_size' },
    { name: 'Learning Rate', key: 'learning_rate' },
    { name: 'Epochs', key: 'epochs' },
    { name: 'Max Seq Length', key: 'max_seq_length' },
    { name: 'Warmup Steps', key: 'warmup_steps' },
    { name: 'Gradient Accumulation', key: 'gradient_accumulation_steps' },
    { name: 'Category', key: 'category' },
    { name: 'Scope', key: 'scope' },
    { name: 'Framework ID', key: 'framework_id' },
    { name: 'Framework Version', key: 'framework_version' }
  ];

  // Check if values are different across jobs
  const areValuesDifferent = (values: any[]): boolean => {
    if (values.length <= 1) return false;
    const first = JSON.stringify(values[0]);
    return values.some(v => JSON.stringify(v) !== first);
  };

  // Get comparison indicator
  const getComparisonIcon = (comparison: 'better' | 'worse' | 'equal') => {
    switch (comparison) {
      case 'better':
        return <TrendingUp className="size-4 text-success" />;
      case 'worse':
        return <TrendingDown className="size-4 text-error" />;
      case 'equal':
        return <Minus className="size-4 text-muted-foreground" />;
    }
  };

  // Export as CSV
  const exportAsCSV = () => {
    if (selectedJobs.length === 0) {
      toast.error('No jobs selected for export');
      return;
    }

    const headers = ['Metric', ...selectedJobs.map(job => job.adapter_name || job.id)];
    const rows = metrics.map(metric => [
      metric.name,
      ...selectedJobs.map(job => {
        const value = metric.getValue(job);
        return metric.format ? metric.format(value) : String(value);
      })
    ]);

    const csv = [headers, ...rows].map(row => row.join(',')).join('\n');
    const blob = new Blob([csv], { type: 'text/csv' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `training-comparison-${Date.now()}.csv`;
    a.click();
    URL.revokeObjectURL(url);
    toast.success('Exported as CSV');
  };

  // Export as JSON
  const exportAsJSON = () => {
    if (selectedJobs.length === 0) {
      toast.error('No jobs selected for export');
      return;
    }

    const comparison = {
      timestamp: new Date().toISOString(),
      jobs: selectedJobs.map(job => ({
        id: job.id,
        name: job.adapter_name,
        status: job.status,
        created_at: job.created_at,
        metrics: Object.fromEntries(
          metrics.map(metric => [metric.name, metric.getValue(job)])
        ),
        config: job.config
      }))
    };

    const blob = new Blob([JSON.stringify(comparison, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `training-comparison-${Date.now()}.json`;
    a.click();
    URL.revokeObjectURL(url);
    toast.success('Exported as JSON');
  };

  // Export as Markdown report
  const exportAsMarkdown = () => {
    if (selectedJobs.length === 0) {
      toast.error('No jobs selected for export');
      return;
    }

    const lines = [
      '# Training Job Comparison Report',
      '',
      `Generated: ${new Date().toLocaleString()}`,
      '',
      '## Summary',
      '',
      ...selectedJobs.map((job, i) =>
        `${i + 1}. **${job.adapter_name || job.id}** - ${job.status} (${job.progress_pct || 0}%)`
      ),
      '',
      '## Metrics Comparison',
      '',
      '| Metric | ' + selectedJobs.map(job => job.adapter_name || job.id).join(' | ') + ' |',
      '|--------|' + selectedJobs.map(() => '--------').join('|') + '|',
      ...metrics.map(metric =>
        `| ${metric.name} | ` +
        selectedJobs.map(job => {
          const value = metric.getValue(job);
          return metric.format ? metric.format(value) : String(value);
        }).join(' | ') + ' |'
      ),
      '',
      '## Configuration Comparison',
      '',
      ...configFields.map(field => {
        const values = selectedJobs.map(job => job.config?.[field.key]);
        const isDifferent = areValuesDifferent(values);
        return `**${field.name}${isDifferent ? ' ⚠️' : ''}:** ${values.map(v => v || 'N/A').join(', ')}`;
      })
    ];

    const markdown = lines.join('\n');
    const blob = new Blob([markdown], { type: 'text/markdown' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `training-comparison-${Date.now()}.md`;
    a.click();
    URL.revokeObjectURL(url);
    toast.success('Exported as Markdown');
  };

  return (
    <div className="space-y-6">
      {/* Header */}
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-3">
              <GitCompare className="size-5" />
              <CardTitle>Training Job Comparison</CardTitle>
            </div>
            {onClose && (
              <Button variant="outline" size="sm" onClick={onClose}>
                Close
              </Button>
            )}
          </div>
        </CardHeader>
        <CardContent>
          <div className="flex flex-wrap gap-3">
            {/* Job Selection Dialog */}
            <Dialog open={isSelectOpen} onOpenChange={setIsSelectOpen}>
              <DialogTrigger asChild>
                <Button variant="outline">
                  <CheckCircle2 className="size-4" />
                  Select Jobs ({selectedJobIds.length}/4)
                </Button>
              </DialogTrigger>
              <DialogContent className="max-w-3xl max-h-[80vh] overflow-hidden flex flex-col">
                <DialogHeader>
                  <DialogTitle>Select Training Jobs</DialogTitle>
                  <DialogDescription>
                    Choose 2-4 training jobs to compare. Prefer completed jobs for accurate comparison.
                  </DialogDescription>
                </DialogHeader>

                {/* Filters */}
                <div className="flex gap-3 py-4">
                  <Select value={filterStatus} onValueChange={setFilterStatus}>
                    <SelectTrigger className="w-40">
                      <SelectValue placeholder="Filter by status" />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="all">All Status</SelectItem>
                      <SelectItem value="completed">Completed</SelectItem>
                      <SelectItem value="running">Running</SelectItem>
                      <SelectItem value="failed">Failed</SelectItem>
                      <SelectItem value="cancelled">Cancelled</SelectItem>
                    </SelectContent>
                  </Select>

                  <Select value={sortBy} onValueChange={(val) => setSortBy(val as any)}>
                    <SelectTrigger className="w-40">
                      <SelectValue placeholder="Sort by" />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="date">Date</SelectItem>
                      <SelectItem value="loss">Loss</SelectItem>
                      <SelectItem value="duration">Duration</SelectItem>
                    </SelectContent>
                  </Select>
                </div>

                {/* Job List */}
                <div className="flex-1 overflow-y-auto border rounded-md">
                  <Table>
                    <TableHeader>
                      <TableRow>
                        <TableHead className="w-12"></TableHead>
                        <TableHead>Name</TableHead>
                        <TableHead>Status</TableHead>
                        <TableHead>Date</TableHead>
                        <TableHead className="text-right">Loss</TableHead>
                      </TableRow>
                    </TableHeader>
                    <TableBody>
                      {availableJobs.map(job => (
                        <TableRow
                          key={job.id}
                          className="cursor-pointer hover:bg-accent"
                          onClick={() => toggleJobSelection(job.id)}
                        >
                          <TableCell>
                            <Checkbox
                              checked={selectedJobIds.includes(job.id)}
                              onCheckedChange={() => toggleJobSelection(job.id)}
                            />
                          </TableCell>
                          <TableCell className="font-medium">{job.adapter_name || job.id}</TableCell>
                          <TableCell>
                            <Badge
                              variant={
                                job.status === 'completed' ? 'success' :
                                job.status === 'running' ? 'info' :
                                job.status === 'failed' ? 'error' :
                                'neutral'
                              }
                            >
                              {job.status}
                            </Badge>
                          </TableCell>
                          <TableCell className="text-sm text-muted-foreground">
                            {new Date(job.created_at).toLocaleDateString()}
                          </TableCell>
                          <TableCell className="text-right">
                            {job.current_loss?.toFixed(4) || 'N/A'}
                          </TableCell>
                        </TableRow>
                      ))}
                    </TableBody>
                  </Table>
                </div>

                <DialogFooter>
                  <Button variant="outline" onClick={() => setIsSelectOpen(false)}>
                    Cancel
                  </Button>
                  <Button
                    onClick={() => setIsSelectOpen(false)}
                    disabled={selectedJobIds.length < 2}
                  >
                    Compare {selectedJobIds.length} Jobs
                  </Button>
                </DialogFooter>
              </DialogContent>
            </Dialog>

            {/* Export Buttons */}
            {selectedJobs.length >= 2 && (
              <>
                <Button variant="outline" size="sm" onClick={exportAsCSV}>
                  <FileSpreadsheet className="size-4" />
                  Export CSV
                </Button>
                <Button variant="outline" size="sm" onClick={exportAsJSON}>
                  <FileJson className="size-4" />
                  Export JSON
                </Button>
                <Button variant="outline" size="sm" onClick={exportAsMarkdown}>
                  <FileText className="size-4" />
                  Export Markdown
                </Button>
              </>
            )}
          </div>
        </CardContent>
      </Card>

      {/* Comparison View */}
      {selectedJobs.length >= 2 ? (
        <>
          {/* Summary Statistics */}
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <BarChart3 className="size-5" />
                Summary Statistics
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
                {selectedJobs.map((job, idx) => (
                  <div key={job.id} className="p-4 border rounded-lg space-y-2">
                    <div className="font-medium truncate">{job.adapter_name || job.id}</div>
                    <div className="space-y-1 text-sm">
                      <div className="flex justify-between">
                        <span className="text-muted-foreground">Final Loss:</span>
                        <span className="font-mono">{job.current_loss?.toFixed(4) || 'N/A'}</span>
                      </div>
                      <div className="flex justify-between">
                        <span className="text-muted-foreground">Tokens/sec:</span>
                        <span className="font-mono">{job.tokens_per_second?.toFixed(0) || 'N/A'}</span>
                      </div>
                      <div className="flex justify-between">
                        <span className="text-muted-foreground">Status:</span>
                        <Badge variant={job.status === 'completed' ? 'success' : 'neutral'} className="text-xs">
                          {job.status}
                        </Badge>
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            </CardContent>
          </Card>

          {/* Metrics Comparison Table */}
          <Card>
            <CardHeader>
              <CardTitle>Performance Metrics</CardTitle>
            </CardHeader>
            <CardContent>
              <div className="overflow-x-auto">
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Metric</TableHead>
                      {selectedJobs.map((job, idx) => (
                        <TableHead key={job.id} className="text-center">
                          <div className="font-medium truncate max-w-[200px]">
                            {job.adapter_name || `Job ${idx + 1}`}
                          </div>
                        </TableHead>
                      ))}
                      <TableHead className="text-center">Difference</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {metrics.map(metric => {
                      const values = selectedJobs.map(job => metric.getValue(job));
                      const isDifferent = areValuesDifferent(values);
                      const baseline = values[0];

                      return (
                        <TableRow key={metric.name} className={isDifferent ? 'bg-warning/10' : ''}>
                          <TableCell className="font-medium">{metric.name}</TableCell>
                          {values.map((value, idx) => {
                            const formatted = metric.format ? metric.format(value) : String(value);
                            const comparison = metric.compare && baseline !== undefined && value !== undefined
                              ? metric.compare(value, baseline)
                              : 'equal';

                            return (
                              <TableCell key={idx} className="text-center">
                                <div className="flex items-center justify-center gap-2">
                                  <span>{formatted}</span>
                                  {idx > 0 && getComparisonIcon(comparison)}
                                </div>
                              </TableCell>
                            );
                          })}
                          <TableCell className="text-center">
                            {isDifferent ? (
                              <Badge variant="warning" className="text-xs">Different</Badge>
                            ) : (
                              <Badge variant="neutral" className="text-xs">Identical</Badge>
                            )}
                          </TableCell>
                        </TableRow>
                      );
                    })}
                  </TableBody>
                </Table>
              </div>
            </CardContent>
          </Card>

          {/* Configuration Comparison */}
          <Card>
            <CardHeader>
              <CardTitle>Configuration Comparison</CardTitle>
            </CardHeader>
            <CardContent>
              <div className="space-y-6">
                {/* Side-by-side configuration */}
                <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
                  {selectedJobs.map(job => (
                    <div key={job.id} className="space-y-3">
                      <div className="font-medium border-b pb-2">
                        {job.adapter_name || job.id}
                      </div>
                      {configFields.map(field => (
                        <div key={field.key} className="text-sm">
                          <div className="text-muted-foreground">{field.name}</div>
                          <div className="font-mono">
                            {job.config?.[field.key]?.toString() || 'N/A'}
                          </div>
                        </div>
                      ))}
                    </div>
                  ))}
                </div>

                {/* Hyperparameter Diff Table */}
                <div className="pt-4 border-t">
                  <h4 className="font-medium mb-4">Hyperparameter Differences</h4>
                  <Table>
                    <TableHeader>
                      <TableRow>
                        <TableHead>Parameter</TableHead>
                        {selectedJobs.map((job, idx) => (
                          <TableHead key={job.id}>Job {idx + 1}</TableHead>
                        ))}
                        <TableHead>Difference</TableHead>
                      </TableRow>
                    </TableHeader>
                    <TableBody>
                      {configFields.map(field => {
                        const values = selectedJobs.map(job => job.config?.[field.key]);
                        const isDifferent = areValuesDifferent(values);

                        return (
                          <TableRow key={field.key} className={isDifferent ? 'bg-warning/10' : ''}>
                            <TableCell className="font-medium">{field.name}</TableCell>
                            {values.map((value, idx) => (
                              <TableCell key={idx}>{value?.toString() || 'N/A'}</TableCell>
                            ))}
                            <TableCell>
                              {isDifferent ? (
                                <Badge variant="warning">
                                  <AlertCircle className="size-3" />
                                  Different
                                </Badge>
                              ) : (
                                <Badge variant="neutral">Identical</Badge>
                              )}
                            </TableCell>
                          </TableRow>
                        );
                      })}
                    </TableBody>
                  </Table>
                </div>

                {/* Targets Comparison */}
                <div className="pt-4 border-t">
                  <h4 className="font-medium mb-4">Target Layers</h4>
                  <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
                    {selectedJobs.map(job => (
                      <div key={job.id} className="p-3 border rounded">
                        <div className="font-medium text-sm mb-2">{job.adapter_name || job.id}</div>
                        <div className="flex flex-wrap gap-1">
                          {job.config?.targets?.map(target => (
                            <Badge key={target} variant="outline" className="text-xs">
                              {target}
                            </Badge>
                          )) || <span className="text-xs text-muted-foreground">No targets</span>}
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              </div>
            </CardContent>
          </Card>
        </>
      ) : (
        <Card>
          <CardContent className="py-12">
            <div className="text-center text-muted-foreground">
              <GitCompare className="size-12 mx-auto mb-4 opacity-50" />
              <p className="text-lg font-medium">No jobs selected</p>
              <p className="text-sm mt-2">Select 2-4 training jobs to compare their performance and configuration</p>
              <Button
                variant="outline"
                className="mt-4"
                onClick={() => setIsSelectOpen(true)}
              >
                Select Jobs
              </Button>
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}
