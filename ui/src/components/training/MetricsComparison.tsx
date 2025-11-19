import React, { useState, useMemo } from 'react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '../ui/card';
import { Button } from '../ui/button';
import { Badge } from '../ui/badge';
import { Switch } from '../ui/switch';
import { Label } from '../ui/label';
import {
  LineChart,
  Line,
  BarChart,
  Bar,
  AreaChart,
  Area,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  Legend,
  ResponsiveContainer,
  Cell,
  ReferenceLine,
} from 'recharts';
import {
  TrendingDown,
  TrendingUp,
  Download,
  Maximize2,
  Activity,
  Zap,
  MemoryStick,
  Gauge,
  Eye,
  EyeOff,
} from 'lucide-react';
import { TrainingJob, TrainingMetrics } from '../../api/types';

interface MetricsComparisonProps {
  jobs: TrainingJob[];
  metricsHistory?: Map<string, TrainingMetrics[]>; // Job ID -> metrics timeline
  className?: string;
}

interface ChartDataPoint {
  epoch: number;
  time?: number; // Relative time in seconds from start
  [key: string]: number | undefined; // Dynamic keys for each job
}

// Color-blind friendly palette (Tol Bright scheme)
const JOB_COLORS = [
  '#4477AA', // Blue
  '#EE6677', // Red
  '#228833', // Green
  '#CCBB44', // Yellow
  '#66CCEE', // Cyan
  '#AA3377', // Purple
  '#BBBBBB', // Grey
];

// Helper to generate smooth curves using moving average
const smoothData = (data: number[], windowSize: number = 5): number[] => {
  const smoothed: number[] = [];
  for (let i = 0; i < data.length; i++) {
    const start = Math.max(0, i - Math.floor(windowSize / 2));
    const end = Math.min(data.length, i + Math.ceil(windowSize / 2));
    const window = data.slice(start, end);
    const avg = window.reduce((sum, val) => sum + val, 0) / window.length;
    smoothed.push(avg);
  }
  return smoothed;
};

export const MetricsComparison: React.FC<MetricsComparisonProps> = ({
  jobs,
  metricsHistory = new Map(),
  className = '',
}) => {
  // State for interactive features
  const [visibleJobs, setVisibleJobs] = useState<Set<string>>(
    new Set(jobs.map(j => j.id))
  );
  const [logScale, setLogScale] = useState(false);
  const [smoothing, setSmoothing] = useState(true);
  const [showValidation, setShowValidation] = useState(true);

  // Toggle job visibility
  const toggleJob = (jobId: string) => {
    setVisibleJobs(prev => {
      const next = new Set(prev);
      if (next.has(jobId)) {
        next.delete(jobId);
      } else {
        next.add(jobId);
      }
      return next;
    });
  };

  // Build loss curve data
  const lossData = useMemo(() => {
    const dataPoints: ChartDataPoint[] = [];
    const maxEpochs = Math.max(
      ...jobs.map(j => j.total_epochs || j.config?.epochs || 0)
    );

    for (let epoch = 0; epoch <= maxEpochs; epoch++) {
      const point: ChartDataPoint = { epoch };
      jobs.forEach(job => {
        const history = metricsHistory.get(job.id);
        if (history && history[epoch]) {
          const loss = history[epoch].loss;
          const valLoss = history[epoch].validation_loss;
          point[`${job.id}_loss`] = loss;
          if (showValidation && valLoss !== undefined) {
            point[`${job.id}_val_loss`] = valLoss;
          }
        } else if (epoch === (job.current_epoch || 0)) {
          // Use current metrics if available
          point[`${job.id}_loss`] = job.current_loss;
          if (showValidation && job.metrics?.validation_loss !== undefined) {
            point[`${job.id}_val_loss`] = job.metrics.validation_loss;
          }
        }
      });
      dataPoints.push(point);
    }
    return dataPoints;
  }, [jobs, metricsHistory, showValidation]);

  // Build performance data (tokens/second over time)
  const performanceData = useMemo(() => {
    const dataPoints: ChartDataPoint[] = [];
    const maxEpochs = Math.max(
      ...jobs.map(j => j.total_epochs || j.config?.epochs || 0)
    );

    for (let epoch = 0; epoch <= maxEpochs; epoch++) {
      const point: ChartDataPoint = { epoch };
      jobs.forEach(job => {
        const history = metricsHistory.get(job.id);
        if (history && history[epoch]) {
          point[`${job.id}_tokens_per_second`] = history[epoch].tokens_per_second;
        } else if (epoch === (job.current_epoch || 0)) {
          point[`${job.id}_tokens_per_second`] = job.tokens_per_second;
        }
      });
      dataPoints.push(point);
    }
    return dataPoints;
  }, [jobs, metricsHistory]);

  // Build resource usage data
  const resourceData = useMemo(() => {
    const dataPoints: ChartDataPoint[] = [];
    const maxEpochs = Math.max(
      ...jobs.map(j => j.total_epochs || j.config?.epochs || 0)
    );

    for (let epoch = 0; epoch <= maxEpochs; epoch++) {
      const point: ChartDataPoint = { epoch };
      jobs.forEach(job => {
        const history = metricsHistory.get(job.id);
        if (history && history[epoch]) {
          point[`${job.id}_gpu`] = history[epoch].gpu_utilization || 0;
          point[`${job.id}_memory`] = history[epoch].memory_usage || 0;
        } else if (epoch === (job.current_epoch || 0) && job.metrics) {
          point[`${job.id}_gpu`] = job.metrics.gpu_utilization || 0;
          point[`${job.id}_memory`] = job.metrics.memory_usage || 0;
        }
      });
      dataPoints.push(point);
    }
    return dataPoints;
  }, [jobs, metricsHistory]);

  // Calculate statistics
  const statistics = useMemo(() => {
    return jobs.map(job => {
      const history = metricsHistory.get(job.id) || [];
      const losses = history.map(m => m.loss).filter(l => l !== undefined);
      const bestEpoch = losses.indexOf(Math.min(...losses));
      const bestLoss = losses.length > 0 ? Math.min(...losses) : job.current_loss || 0;

      // Calculate convergence rate (loss reduction per epoch)
      let convergenceRate = 0;
      if (losses.length > 1) {
        const firstLoss = losses[0];
        const lastLoss = losses[losses.length - 1];
        convergenceRate = (firstLoss - lastLoss) / losses.length;
      }

      // Average performance
      const throughputs = history.map(m => m.tokens_per_second).filter(t => t !== undefined);
      const avgThroughput = throughputs.length > 0
        ? throughputs.reduce((a, b) => a + b, 0) / throughputs.length
        : job.tokens_per_second || 0;

      return {
        jobId: job.id,
        jobName: job.adapter_name,
        bestEpoch,
        bestLoss,
        currentLoss: job.current_loss || 0,
        convergenceRate,
        avgThroughput,
      };
    });
  }, [jobs, metricsHistory]);

  // Find best performer
  const bestJob = useMemo(() => {
    if (statistics.length === 0) return null;
    return statistics.reduce((best, current) =>
      current.bestLoss < best.bestLoss ? current : best
    );
  }, [statistics]);

  // Export chart as PNG (simplified - would need html2canvas or similar)
  const exportChart = (chartId: string) => {
    // Placeholder for export functionality
    console.log(`Exporting chart: ${chartId}`);
    // TODO: Implement with html2canvas or similar library
  };

  // Custom tooltip
  const CustomTooltip = ({ active, payload, label }: any) => {
    if (!active || !payload || payload.length === 0) return null;

    return (
      <div className="bg-background border border-border rounded-lg p-3 shadow-lg">
        <div className="font-medium mb-2">Epoch {label}</div>
        <div className="space-y-1">
          {payload.map((entry: any, index: number) => {
            const jobId = entry.dataKey.split('_')[0];
            const job = jobs.find(j => j.id === jobId);
            if (!job || !visibleJobs.has(jobId)) return null;

            return (
              <div key={index} className="flex items-center justify-between gap-4 text-sm">
                <div className="flex items-center gap-2">
                  <div
                    className="w-3 h-3 rounded-full"
                    style={{ backgroundColor: entry.color }}
                  />
                  <span className="text-muted-foreground">{job.adapter_name}</span>
                </div>
                <span className="font-mono font-medium">
                  {typeof entry.value === 'number' ? entry.value.toFixed(4) : '—'}
                </span>
              </div>
            );
          })}
        </div>
      </div>
    );
  };

  return (
    <div className={`space-y-6 ${className}`}>
      {/* Header with Controls */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-bold">Training Metrics Comparison</h2>
          <p className="text-sm text-muted-foreground">
            Comparing {jobs.length} training job{jobs.length !== 1 ? 's' : ''}
          </p>
        </div>
        <div className="flex items-center gap-4">
          <div className="flex items-center gap-2">
            <Switch
              id="smoothing"
              checked={smoothing}
              onCheckedChange={setSmoothing}
            />
            <Label htmlFor="smoothing" className="text-sm">Smoothing</Label>
          </div>
          <div className="flex items-center gap-2">
            <Switch
              id="log-scale"
              checked={logScale}
              onCheckedChange={setLogScale}
            />
            <Label htmlFor="log-scale" className="text-sm">Log Scale</Label>
          </div>
          <div className="flex items-center gap-2">
            <Switch
              id="validation"
              checked={showValidation}
              onCheckedChange={setShowValidation}
            />
            <Label htmlFor="validation" className="text-sm">Validation</Label>
          </div>
        </div>
      </div>

      {/* Job Legend (Interactive) */}
      <Card>
        <CardHeader>
          <CardTitle className="text-sm">Jobs</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex flex-wrap gap-2">
            {jobs.map((job, idx) => (
              <button
                key={job.id}
                onClick={() => toggleJob(job.id)}
                className="transition-opacity hover:opacity-80"
              >
                <Badge
                  variant={visibleJobs.has(job.id) ? 'default' : 'outline'}
                  className="gap-2 cursor-pointer"
                  style={{
                    backgroundColor: visibleJobs.has(job.id)
                      ? JOB_COLORS[idx % JOB_COLORS.length]
                      : 'transparent',
                    borderColor: JOB_COLORS[idx % JOB_COLORS.length],
                  }}
                >
                  {visibleJobs.has(job.id) ? (
                    <Eye className="h-3 w-3" />
                  ) : (
                    <EyeOff className="h-3 w-3" />
                  )}
                  {job.adapter_name}
                </Badge>
              </button>
            ))}
          </div>
        </CardContent>
      </Card>

      {/* Summary Statistics */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        {statistics.map((stat, idx) => {
          const isBest = bestJob?.jobId === stat.jobId;
          const job = jobs.find(j => j.id === stat.jobId);
          if (!job) return null;

          return (
            <Card key={stat.jobId} className={isBest ? 'ring-2 ring-primary' : ''}>
              <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
                <CardTitle className="text-sm font-medium truncate">
                  {stat.jobName}
                </CardTitle>
                {isBest && <Badge variant="default">Best</Badge>}
              </CardHeader>
              <CardContent className="space-y-3">
                <div>
                  <div className="text-xs text-muted-foreground">Best Loss</div>
                  <div className="text-2xl font-bold">{stat.bestLoss.toFixed(4)}</div>
                  <div className="text-xs text-muted-foreground">
                    Epoch {stat.bestEpoch}
                  </div>
                </div>
                <div className="flex items-center gap-2 text-sm">
                  {stat.convergenceRate > 0 ? (
                    <TrendingDown className="h-4 w-4 text-green-600" />
                  ) : (
                    <TrendingUp className="h-4 w-4 text-yellow-600" />
                  )}
                  <span className="text-muted-foreground">
                    {Math.abs(stat.convergenceRate).toFixed(4)}/epoch
                  </span>
                </div>
                <div className="flex items-center gap-2 text-sm">
                  <Zap className="h-4 w-4 text-muted-foreground" />
                  <span>{stat.avgThroughput.toFixed(0)} tokens/s</span>
                </div>
              </CardContent>
            </Card>
          );
        })}
      </div>

      {/* Loss Curve Overlay */}
      <Card>
        <CardHeader className="flex flex-row items-center justify-between">
          <div>
            <CardTitle className="flex items-center gap-2">
              <TrendingDown className="h-5 w-5" />
              Training & Validation Loss
            </CardTitle>
            <CardDescription>
              Loss curves over training epochs
              {showValidation && ' (solid = training, dashed = validation)'}
            </CardDescription>
          </div>
          <Button
            variant="outline"
            size="sm"
            onClick={() => exportChart('loss-curve')}
          >
            <Download className="h-4 w-4 mr-1" />
            Export
          </Button>
        </CardHeader>
        <CardContent>
          <ResponsiveContainer width="100%" height={400}>
            <LineChart data={lossData}>
              <CartesianGrid strokeDasharray="3 3" className="stroke-muted" />
              <XAxis
                dataKey="epoch"
                label={{ value: 'Epoch', position: 'insideBottom', offset: -5 }}
                className="text-xs"
                tick={{ fill: 'currentColor' }}
              />
              <YAxis
                scale={logScale ? 'log' : 'linear'}
                domain={logScale ? ['auto', 'auto'] : [0, 'auto']}
                label={{ value: 'Loss', angle: -90, position: 'insideLeft' }}
                className="text-xs"
                tick={{ fill: 'currentColor' }}
              />
              <Tooltip content={<CustomTooltip />} />
              <Legend
                wrapperStyle={{ paddingTop: '20px' }}
                iconType="line"
              />
              {jobs.map((job, idx) => {
                if (!visibleJobs.has(job.id)) return null;
                const color = JOB_COLORS[idx % JOB_COLORS.length];
                return (
                  <React.Fragment key={job.id}>
                    {/* Training loss */}
                    <Line
                      type="monotone"
                      dataKey={`${job.id}_loss`}
                      name={`${job.adapter_name} (train)`}
                      stroke={color}
                      strokeWidth={2}
                      dot={false}
                      activeDot={{ r: 4 }}
                    />
                    {/* Validation loss */}
                    {showValidation && (
                      <Line
                        type="monotone"
                        dataKey={`${job.id}_val_loss`}
                        name={`${job.adapter_name} (val)`}
                        stroke={color}
                        strokeWidth={2}
                        strokeDasharray="5 5"
                        dot={false}
                        activeDot={{ r: 4 }}
                      />
                    )}
                  </React.Fragment>
                );
              })}
              {/* Best epoch indicator */}
              {bestJob && (
                <ReferenceLine
                  x={bestJob.bestEpoch}
                  stroke="#10b981"
                  strokeDasharray="3 3"
                  label={{
                    value: 'Best',
                    position: 'top',
                    fill: '#10b981',
                    fontSize: 12,
                  }}
                />
              )}
            </LineChart>
          </ResponsiveContainer>
        </CardContent>
      </Card>

      {/* Performance Comparison (Tokens/Second) */}
      <Card>
        <CardHeader className="flex flex-row items-center justify-between">
          <div>
            <CardTitle className="flex items-center gap-2">
              <Zap className="h-5 w-5" />
              Training Performance
            </CardTitle>
            <CardDescription>
              Tokens processed per second over time
            </CardDescription>
          </div>
          <Button
            variant="outline"
            size="sm"
            onClick={() => exportChart('performance')}
          >
            <Download className="h-4 w-4 mr-1" />
            Export
          </Button>
        </CardHeader>
        <CardContent>
          <ResponsiveContainer width="100%" height={300}>
            <AreaChart data={performanceData}>
              <CartesianGrid strokeDasharray="3 3" className="stroke-muted" />
              <XAxis
                dataKey="epoch"
                label={{ value: 'Epoch', position: 'insideBottom', offset: -5 }}
                className="text-xs"
                tick={{ fill: 'currentColor' }}
              />
              <YAxis
                label={{ value: 'Tokens/Second', angle: -90, position: 'insideLeft' }}
                className="text-xs"
                tick={{ fill: 'currentColor' }}
              />
              <Tooltip content={<CustomTooltip />} />
              <Legend wrapperStyle={{ paddingTop: '20px' }} />
              {jobs.map((job, idx) => {
                if (!visibleJobs.has(job.id)) return null;
                const color = JOB_COLORS[idx % JOB_COLORS.length];
                return (
                  <Area
                    key={job.id}
                    type="monotone"
                    dataKey={`${job.id}_tokens_per_second`}
                    name={job.adapter_name}
                    fill={color}
                    fillOpacity={0.2}
                    stroke={color}
                    strokeWidth={2}
                  />
                );
              })}
            </AreaChart>
          </ResponsiveContainer>
        </CardContent>
      </Card>

      {/* Resource Usage Comparison */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
        {/* GPU Utilization */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Gauge className="h-5 w-5" />
              GPU Utilization
            </CardTitle>
            <CardDescription>
              GPU usage percentage over time
            </CardDescription>
          </CardHeader>
          <CardContent>
            <ResponsiveContainer width="100%" height={250}>
              <LineChart data={resourceData}>
                <CartesianGrid strokeDasharray="3 3" className="stroke-muted" />
                <XAxis
                  dataKey="epoch"
                  className="text-xs"
                  tick={{ fill: 'currentColor' }}
                />
                <YAxis
                  domain={[0, 100]}
                  className="text-xs"
                  tick={{ fill: 'currentColor' }}
                />
                <Tooltip content={<CustomTooltip />} />
                {jobs.map((job, idx) => {
                  if (!visibleJobs.has(job.id)) return null;
                  return (
                    <Line
                      key={job.id}
                      type="monotone"
                      dataKey={`${job.id}_gpu`}
                      name={job.adapter_name}
                      stroke={JOB_COLORS[idx % JOB_COLORS.length]}
                      strokeWidth={2}
                      dot={false}
                    />
                  );
                })}
              </LineChart>
            </ResponsiveContainer>
          </CardContent>
        </Card>

        {/* Memory Usage */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <MemoryStick className="h-5 w-5" />
              Memory Usage
            </CardTitle>
            <CardDescription>
              GPU memory consumption (GB)
            </CardDescription>
          </CardHeader>
          <CardContent>
            <ResponsiveContainer width="100%" height={250}>
              <LineChart data={resourceData}>
                <CartesianGrid strokeDasharray="3 3" className="stroke-muted" />
                <XAxis
                  dataKey="epoch"
                  className="text-xs"
                  tick={{ fill: 'currentColor' }}
                />
                <YAxis
                  className="text-xs"
                  tick={{ fill: 'currentColor' }}
                />
                <Tooltip content={<CustomTooltip />} />
                {jobs.map((job, idx) => {
                  if (!visibleJobs.has(job.id)) return null;
                  return (
                    <Line
                      key={job.id}
                      type="monotone"
                      dataKey={`${job.id}_memory`}
                      name={job.adapter_name}
                      stroke={JOB_COLORS[idx % JOB_COLORS.length]}
                      strokeWidth={2}
                      dot={false}
                    />
                  );
                })}
              </LineChart>
            </ResponsiveContainer>
          </CardContent>
        </Card>
      </div>

      {/* Convergence Analysis */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Activity className="h-5 w-5" />
            Convergence Analysis
          </CardTitle>
          <CardDescription>
            Loss reduction rate per epoch
          </CardDescription>
        </CardHeader>
        <CardContent>
          <ResponsiveContainer width="100%" height={300}>
            <BarChart
              data={statistics}
              layout="horizontal"
            >
              <CartesianGrid strokeDasharray="3 3" className="stroke-muted" />
              <XAxis
                type="category"
                dataKey="jobName"
                className="text-xs"
                tick={{ fill: 'currentColor' }}
              />
              <YAxis
                type="number"
                className="text-xs"
                tick={{ fill: 'currentColor' }}
                label={{ value: 'Loss Reduction/Epoch', angle: -90, position: 'insideLeft' }}
              />
              <Tooltip
                contentStyle={{
                  backgroundColor: 'hsl(var(--background))',
                  border: '1px solid hsl(var(--border))',
                  borderRadius: '8px',
                }}
              />
              <Bar dataKey="convergenceRate" radius={[8, 8, 0, 0]}>
                {statistics.map((stat, idx) => (
                  <Cell
                    key={stat.jobId}
                    fill={JOB_COLORS[idx % JOB_COLORS.length]}
                  />
                ))}
              </Bar>
            </BarChart>
          </ResponsiveContainer>
        </CardContent>
      </Card>
    </div>
  );
};

export default MetricsComparison;
