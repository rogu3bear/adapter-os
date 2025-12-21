import React, { useState, useEffect, useMemo, useCallback } from 'react';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  LineChart,
  Line,
  AreaChart,
  Area,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  Legend,
  ResponsiveContainer,
  ReferenceLine,
} from 'recharts';
import {
  TrendingDown,
  TrendingUp,
  Activity,
  Cpu,
  MemoryStick,
  Gauge,
  RefreshCw,
  Loader2,
  AlertTriangle,
} from 'lucide-react';
import { toast } from 'sonner';
import { apiClient } from '@/api/services';
import { TrainingJob, TrainingMetrics } from '@/api/training-types';
import { logger } from '@/utils/logger';
import { METRIC_COLORS } from '@/constants/chart-colors';

// Simulated extended metrics for demonstration
interface ExtendedMetrics extends TrainingMetrics {
  gradient_norm?: number;
  gpu_memory_gb?: number;
  step_time_ms?: number;
}

interface MetricsDataPoint {
  step: number;
  epoch?: number;
  training_loss?: number;
  validation_loss?: number;
  learning_rate?: number;
  gradient_norm?: number;
  gpu_memory_gb?: number;
  tokens_per_second?: number;
}

// Use design system chart colors for metrics
const COLORS = METRIC_COLORS;

export default function MLEngineerTrainingMetrics() {
  // Job selection state
  const [jobs, setJobs] = useState<TrainingJob[]>([]);
  const [selectedJobId, setSelectedJobId] = useState<string>('');
  const [selectedJob, setSelectedJob] = useState<TrainingJob | null>(null);

  // Metrics state
  const [metricsHistory, setMetricsHistory] = useState<MetricsDataPoint[]>([]);
  const [isLoading, setIsLoading] = useState<boolean>(false);
  const [isPolling, setIsPolling] = useState<boolean>(false);
  const [pollingInterval, setPollingInterval] = useState<number>(5000);

  // Chart display options
  const [logScale, setLogScale] = useState<boolean>(false);
  const [showValidation, setShowValidation] = useState<boolean>(true);
  const [smoothing, setSmoothing] = useState<boolean>(true);
  const [smoothingWindow, setSmoothingWindow] = useState<number>(5);

  // Load training jobs on mount
  useEffect(() => {
    const loadJobs = async () => {
      try {
        const response = await apiClient.listTrainingJobs();
        const jobList = response.jobs || [];
        setJobs(jobList);
        // Auto-select first running job
        const runningJob = jobList.find(j => j.status === 'running');
        if (runningJob) {
          setSelectedJobId(runningJob.id);
        } else if (jobList.length > 0) {
          setSelectedJobId(jobList[0].id);
        }
      } catch (error) {
        logger.error('Failed to load training jobs', { error });
        toast.error('Failed to load training jobs');
      }
    };
    loadJobs();
  }, []);

  // Fetch metrics for selected job
  const fetchMetrics = useCallback(async (jobId: string) => {
    if (!jobId) return;

    try {
      setIsLoading(true);
      const [job, metrics] = await Promise.all([
        apiClient.getTrainingJob(jobId),
        apiClient.getTrainingMetrics(jobId),
      ]);

      setSelectedJob(job);

      // Build metrics history from available data
      // In production, this would come from a streaming endpoint or metrics history API
      const currentStep = metrics.step || job.progress_pct || 0;
      const newDataPoint: MetricsDataPoint = {
        step: currentStep,
        epoch: metrics.epoch || job.current_epoch,
        training_loss: metrics.loss || job.current_loss,
        validation_loss: metrics.validation_loss,
        learning_rate: metrics.learning_rate || job.learning_rate,
        gradient_norm: (metrics as ExtendedMetrics).gradient_norm,
        gpu_memory_gb: metrics.memory_usage ? metrics.memory_usage / 1024 : undefined,
        tokens_per_second: metrics.tokens_per_second || job.tokens_per_second,
      };

      setMetricsHistory(prev => {
        // Add new data point, avoiding duplicates
        const exists = prev.some(p => p.step === newDataPoint.step);
        if (exists) {
          return prev.map(p => p.step === newDataPoint.step ? newDataPoint : p);
        }
        return [...prev, newDataPoint].sort((a, b) => a.step - b.step);
      });
    } catch (error) {
      logger.error('Failed to fetch metrics', { error, jobId });
    } finally {
      setIsLoading(false);
    }
  }, []);

  // Fetch metrics when job selection changes
  useEffect(() => {
    if (selectedJobId) {
      setMetricsHistory([]); // Reset history for new job
      fetchMetrics(selectedJobId);
    }
  }, [selectedJobId, fetchMetrics]);

  // Polling for live updates
  useEffect(() => {
    if (!isPolling || !selectedJobId) return;

    const interval = setInterval(() => {
      fetchMetrics(selectedJobId);
    }, pollingInterval);

    return () => clearInterval(interval);
  }, [isPolling, selectedJobId, pollingInterval, fetchMetrics]);

  // Auto-enable polling for running jobs
  useEffect(() => {
    if (selectedJob?.status === 'running') {
      setIsPolling(true);
    } else {
      setIsPolling(false);
    }
  }, [selectedJob?.status]);

  // Apply smoothing to data
  const smoothData = useCallback((data: number[], windowSize: number): number[] => {
    if (!smoothing || windowSize <= 1) return data;

    const smoothed: number[] = [];
    for (let i = 0; i < data.length; i++) {
      const start = Math.max(0, i - Math.floor(windowSize / 2));
      const end = Math.min(data.length, i + Math.ceil(windowSize / 2));
      const window = data.slice(start, end);
      const avg = window.reduce((sum, val) => sum + val, 0) / window.length;
      smoothed.push(avg);
    }
    return smoothed;
  }, [smoothing]);

  // Processed chart data
  const chartData = useMemo(() => {
    if (metricsHistory.length === 0) return [];

    const losses = metricsHistory.map(m => m.training_loss || 0);
    const smoothedLosses = smoothData(losses, smoothingWindow);

    return metricsHistory.map((m, i) => ({
      ...m,
      training_loss_smooth: smoothedLosses[i],
    }));
  }, [metricsHistory, smoothData, smoothingWindow]);

  // Statistics
  const stats = useMemo(() => {
    if (metricsHistory.length === 0) return null;

    const losses = metricsHistory.map(m => m.training_loss).filter((l): l is number => l !== undefined);
    const lrs = metricsHistory.map(m => m.learning_rate).filter((l): l is number => l !== undefined);
    const norms = metricsHistory.map(m => m.gradient_norm).filter((n): n is number => n !== undefined);
    const memories = metricsHistory.map(m => m.gpu_memory_gb).filter((m): m is number => m !== undefined);

    return {
      currentLoss: losses[losses.length - 1] || 0,
      minLoss: Math.min(...losses),
      maxLoss: Math.max(...losses),
      lossReduction: losses.length > 1 ? ((losses[0] - losses[losses.length - 1]) / losses[0] * 100) : 0,
      currentLR: lrs[lrs.length - 1] || 0,
      avgGradNorm: norms.length > 0 ? norms.reduce((a, b) => a + b, 0) / norms.length : 0,
      maxGradNorm: norms.length > 0 ? Math.max(...norms) : 0,
      avgMemory: memories.length > 0 ? memories.reduce((a, b) => a + b, 0) / memories.length : 0,
      peakMemory: memories.length > 0 ? Math.max(...memories) : 0,
    };
  }, [metricsHistory]);


  return (
    <div className="space-y-6 p-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-bold">ML Engineer Training Metrics</h2>
          <p className="text-sm text-muted-foreground">
            Real-time training metrics dashboard with loss curves and resource monitoring
          </p>
        </div>
        <div className="flex items-center gap-2">
          {selectedJob?.status === 'running' && (
            <Badge variant="default" className="gap-1 animate-pulse">
              <Activity className="h-3 w-3" />
              Live
            </Badge>
          )}
          <Badge variant="outline" className="gap-1">
            <Gauge className="h-3 w-3" />
            {metricsHistory.length} Points
          </Badge>
        </div>
      </div>

      {/* Job Selection and Controls */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-4">
        {/* Job Selector */}
        <Card>
          <CardHeader className="pb-3">
            <CardTitle className="text-sm">Training Job</CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <Select value={selectedJobId} onValueChange={setSelectedJobId}>
              <SelectTrigger>
                <SelectValue placeholder="Select job" />
              </SelectTrigger>
              <SelectContent>
                {jobs.map(job => (
                  <SelectItem key={job.id} value={job.id}>
                    <div className="flex items-center gap-2">
                      <span className={`h-2 w-2 rounded-full ${
                        job.status === 'running' ? 'bg-green-500' :
                        job.status === 'completed' ? 'bg-blue-500' :
                        job.status === 'failed' ? 'bg-red-500' : 'bg-gray-500'
                      }`} />
                      {job.adapter_name || job.id.slice(0, 8)}
                    </div>
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <Button
              variant="outline"
              size="sm"
              onClick={() => fetchMetrics(selectedJobId)}
              disabled={isLoading || !selectedJobId}
              className="w-full"
            >
              {isLoading ? (
                <Loader2 className="h-4 w-4 mr-2 animate-spin" />
              ) : (
                <RefreshCw className="h-4 w-4 mr-2" />
              )}
              Refresh
            </Button>
          </CardContent>
        </Card>

        {/* Chart Options */}
        <Card>
          <CardHeader className="pb-3">
            <CardTitle className="text-sm">Display Options</CardTitle>
          </CardHeader>
          <CardContent className="space-y-3">
            <div className="flex items-center justify-between">
              <Label className="text-xs">Log Scale</Label>
              <Switch checked={logScale} onCheckedChange={setLogScale} />
            </div>
            <div className="flex items-center justify-between">
              <Label className="text-xs">Show Validation</Label>
              <Switch checked={showValidation} onCheckedChange={setShowValidation} />
            </div>
            <div className="flex items-center justify-between">
              <Label className="text-xs">Smoothing</Label>
              <Switch checked={smoothing} onCheckedChange={setSmoothing} />
            </div>
          </CardContent>
        </Card>

        {/* Polling Controls */}
        <Card>
          <CardHeader className="pb-3">
            <CardTitle className="text-sm">Live Updates</CardTitle>
          </CardHeader>
          <CardContent className="space-y-3">
            <div className="flex items-center justify-between">
              <Label className="text-xs">Auto-refresh</Label>
              <Switch checked={isPolling} onCheckedChange={setIsPolling} />
            </div>
            {isPolling && (
              <div className="space-y-1">
                <Label className="text-xs">Interval (ms)</Label>
                <Select
                  value={pollingInterval.toString()}
                  onValueChange={v => setPollingInterval(parseInt(v))}
                >
                  <SelectTrigger className="h-8">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="1000">1s</SelectItem>
                    <SelectItem value="5000">5s</SelectItem>
                    <SelectItem value="10000">10s</SelectItem>
                    <SelectItem value="30000">30s</SelectItem>
                  </SelectContent>
                </Select>
              </div>
            )}
          </CardContent>
        </Card>
      </div>

      {/* Statistics Summary */}
      {stats && (
        <Card>
          <CardHeader className="pb-3">
            <CardTitle className="text-sm">Training Statistics</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="grid grid-cols-2 md:grid-cols-4 lg:grid-cols-8 gap-4">
              <div className="text-center">
                <div className="text-xl font-bold">{stats.currentLoss.toFixed(4)}</div>
                <div className="text-xs text-muted-foreground">Current Loss</div>
              </div>
              <div className="text-center">
                <div className="text-xl font-bold">{stats.minLoss.toFixed(4)}</div>
                <div className="text-xs text-muted-foreground">Min Loss</div>
              </div>
              <div className="text-center">
                <div className="text-xl font-bold flex items-center justify-center gap-1">
                  {stats.lossReduction > 0 ? (
                    <TrendingDown className="h-4 w-4 text-green-500" />
                  ) : (
                    <TrendingUp className="h-4 w-4 text-red-500" />
                  )}
                  {Math.abs(stats.lossReduction).toFixed(1)}%
                </div>
                <div className="text-xs text-muted-foreground">Loss Reduction</div>
              </div>
              <div className="text-center">
                <div className="text-xl font-bold">{stats.currentLR.toExponential(2)}</div>
                <div className="text-xs text-muted-foreground">Learning Rate</div>
              </div>
              <div className="text-center">
                <div className="text-xl font-bold">{stats.avgGradNorm.toFixed(3)}</div>
                <div className="text-xs text-muted-foreground">Avg Grad Norm</div>
              </div>
              <div className="text-center">
                <div className="text-xl font-bold flex items-center justify-center gap-1">
                  {stats.maxGradNorm > 1.0 && (
                    <AlertTriangle className="h-4 w-4 text-amber-500" />
                  )}
                  {stats.maxGradNorm.toFixed(3)}
                </div>
                <div className="text-xs text-muted-foreground">Max Grad Norm</div>
              </div>
              <div className="text-center">
                <div className="text-xl font-bold">{stats.avgMemory.toFixed(2)} GB</div>
                <div className="text-xs text-muted-foreground">Avg GPU Memory</div>
              </div>
              <div className="text-center">
                <div className="text-xl font-bold">{stats.peakMemory.toFixed(2)} GB</div>
                <div className="text-xs text-muted-foreground">Peak Memory</div>
              </div>
            </div>
          </CardContent>
        </Card>
      )}

      {/* Loss Curve */}
      <Card>
        <CardHeader className="pb-3">
          <CardTitle className="text-sm flex items-center gap-2">
            <TrendingDown className="h-4 w-4" />
            Loss Curve
          </CardTitle>
          <CardDescription>Training and validation loss over time</CardDescription>
        </CardHeader>
        <CardContent>
          <div className="h-80">
            <ResponsiveContainer width="100%" height="100%">
              <LineChart data={chartData}>
                <CartesianGrid strokeDasharray="3 3" />
                <XAxis
                  dataKey="step"
                  fontSize={10}
                  label={{ value: 'Step', position: 'insideBottom', offset: -5, fontSize: 10 }}
                />
                <YAxis
                  fontSize={10}
                  scale={logScale ? 'log' : 'auto'}
                  domain={logScale ? ['auto', 'auto'] : [0, 'auto']}
                  label={{ value: 'Loss', angle: -90, position: 'insideLeft', fontSize: 10 }}
                />
                <Tooltip
                  formatter={(value: number) => value.toFixed(4)}
                  labelFormatter={(label) => `Step ${label}`}
                />
                <Legend />
                <Line
                  type="monotone"
                  dataKey={smoothing ? 'training_loss_smooth' : 'training_loss'}
                  name="Training Loss"
                  stroke={COLORS.trainingLoss}
                  dot={false}
                  strokeWidth={2}
                />
                {showValidation && (
                  <Line
                    type="monotone"
                    dataKey="validation_loss"
                    name="Validation Loss"
                    stroke={COLORS.validationLoss}
                    dot={{ r: 3 }}
                    strokeWidth={2}
                    connectNulls
                  />
                )}
              </LineChart>
            </ResponsiveContainer>
          </div>
        </CardContent>
      </Card>

      {/* Learning Rate Schedule */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
        <Card>
          <CardHeader className="pb-3">
            <CardTitle className="text-sm flex items-center gap-2">
              <Gauge className="h-4 w-4" />
              Learning Rate Schedule
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="h-64">
              <ResponsiveContainer width="100%" height="100%">
                <AreaChart data={chartData}>
                  <CartesianGrid strokeDasharray="3 3" />
                  <XAxis dataKey="step" fontSize={10} />
                  <YAxis fontSize={10} tickFormatter={(v) => v.toExponential(1)} />
                  <Tooltip formatter={(value: number) => value.toExponential(4)} />
                  <Area
                    type="monotone"
                    dataKey="learning_rate"
                    name="Learning Rate"
                    stroke={COLORS.learningRate}
                    fill={COLORS.learningRate}
                    fillOpacity={0.3}
                  />
                </AreaChart>
              </ResponsiveContainer>
            </div>
          </CardContent>
        </Card>

        {/* Gradient Norm */}
        <Card>
          <CardHeader className="pb-3">
            <CardTitle className="text-sm flex items-center gap-2">
              <Activity className="h-4 w-4" />
              Gradient Norm
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="h-64">
              <ResponsiveContainer width="100%" height="100%">
                <LineChart data={chartData}>
                  <CartesianGrid strokeDasharray="3 3" />
                  <XAxis dataKey="step" fontSize={10} />
                  <YAxis fontSize={10} />
                  <Tooltip formatter={(value: number) => value.toFixed(4)} />
                  <ReferenceLine y={1.0} stroke="hsl(var(--chart-4))" strokeDasharray="5 5" label="Clip" />
                  <Line
                    type="monotone"
                    dataKey="gradient_norm"
                    name="Gradient Norm"
                    stroke={COLORS.gradientNorm}
                    dot={false}
                    strokeWidth={1.5}
                  />
                </LineChart>
              </ResponsiveContainer>
            </div>
          </CardContent>
        </Card>
      </div>

      {/* GPU Memory Timeline */}
      <Card>
        <CardHeader className="pb-3">
          <CardTitle className="text-sm flex items-center gap-2">
            <MemoryStick className="h-4 w-4" />
            GPU Memory Timeline
          </CardTitle>
          <CardDescription>Memory usage throughout training</CardDescription>
        </CardHeader>
        <CardContent>
          <div className="h-64">
            <ResponsiveContainer width="100%" height="100%">
              <AreaChart data={chartData}>
                <CartesianGrid strokeDasharray="3 3" />
                <XAxis dataKey="step" fontSize={10} />
                <YAxis
                  fontSize={10}
                  label={{ value: 'Memory (GB)', angle: -90, position: 'insideLeft', fontSize: 10 }}
                />
                <Tooltip formatter={(value: number) => `${value.toFixed(2)} GB`} />
                <Area
                  type="monotone"
                  dataKey="gpu_memory_gb"
                  name="GPU Memory"
                  stroke={COLORS.gpuMemory}
                  fill={COLORS.gpuMemory}
                  fillOpacity={0.4}
                />
              </AreaChart>
            </ResponsiveContainer>
          </div>
        </CardContent>
      </Card>

      {/* Job Details */}
      {selectedJob && (
        <Card>
          <CardHeader className="pb-3">
            <CardTitle className="text-sm">Job Details</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="grid grid-cols-2 md:grid-cols-4 gap-4 text-sm">
              <div>
                <Label className="text-xs text-muted-foreground">Job ID</Label>
                <div className="font-mono">{selectedJob.id.slice(0, 16)}</div>
              </div>
              <div>
                <Label className="text-xs text-muted-foreground">Adapter</Label>
                <div>{selectedJob.adapter_name || 'N/A'}</div>
              </div>
              <div>
                <Label className="text-xs text-muted-foreground">Status</Label>
                <Badge variant={
                  selectedJob.status === 'running' ? 'default' :
                  selectedJob.status === 'completed' ? 'secondary' :
                  'destructive'
                }>
                  {selectedJob.status}
                </Badge>
              </div>
              <div>
                <Label className="text-xs text-muted-foreground">Progress</Label>
                <div>{selectedJob.progress_pct?.toFixed(1) || 0}%</div>
              </div>
              <div>
                <Label className="text-xs text-muted-foreground">Epoch</Label>
                <div>{selectedJob.current_epoch || 0} / {selectedJob.total_epochs || selectedJob.config?.epochs || 0}</div>
              </div>
              <div>
                <Label className="text-xs text-muted-foreground">Batch Size</Label>
                <div>{selectedJob.config?.batch_size || 'N/A'}</div>
              </div>
              <div>
                <Label className="text-xs text-muted-foreground">Rank</Label>
                <div>{selectedJob.config?.rank || 'N/A'}</div>
              </div>
              <div>
                <Label className="text-xs text-muted-foreground">Alpha</Label>
                <div>{selectedJob.config?.alpha || 'N/A'}</div>
              </div>
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}
