import React, { useState, useMemo } from 'react';
import { MetricsComparison } from './MetricsComparison';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { TrainingJob, TrainingMetrics } from '@/api/types';
import { RefreshCw, Plus } from 'lucide-react';

/**
 * Example component demonstrating MetricsComparison usage
 *
 * This component shows how to:
 * - Load and compare multiple training jobs
 * - Generate synthetic metrics history for demonstration
 * - Integrate with the training monitoring system
 */
export const MetricsComparisonExample: React.FC = () => {
  // Generate example training jobs
  const generateExampleJobs = (): TrainingJob[] => {
    const configs = [
      {
        id: 'job-1',
        adapter_name: 'code-review-r16a32',
        rank: 16,
        alpha: 32,
        epochs: 20,
        learning_rate: 0.0003,
      },
      {
        id: 'job-2',
        adapter_name: 'code-review-r8a16',
        rank: 8,
        alpha: 16,
        epochs: 20,
        learning_rate: 0.0005,
      },
      {
        id: 'job-3',
        adapter_name: 'code-review-r32a64',
        rank: 32,
        alpha: 64,
        epochs: 20,
        learning_rate: 0.0002,
      },
    ];

    return configs.map(cfg => ({
      id: cfg.id,
      adapter_name: cfg.adapter_name,
      template_id: 'general-code',
      status: 'running' as const,
      progress: 75,
      progress_pct: 75,
      current_epoch: 15,
      total_epochs: cfg.epochs,
      current_loss: 0.12 + Math.random() * 0.1,
      learning_rate: cfg.learning_rate,
      tokens_per_second: 1200 + Math.random() * 300,
      created_at: new Date(Date.now() - Math.random() * 3600000).toISOString(),
      started_at: new Date(Date.now() - Math.random() * 1800000).toISOString(),
      config: {
        rank: cfg.rank,
        alpha: cfg.alpha,
        targets: ['q_proj', 'v_proj'],
        epochs: cfg.epochs,
        learning_rate: cfg.learning_rate,
        batch_size: 8,
        category: 'code' as const,
        scope: 'global' as const,
      },
      metrics: {
        loss: 0.12 + Math.random() * 0.1,
        tokens_per_second: 1200 + Math.random() * 300,
        learning_rate: cfg.learning_rate,
        current_epoch: 15,
        total_epochs: cfg.epochs,
        progress_pct: 75,
        validation_loss: 0.15 + Math.random() * 0.1,
        gpu_utilization: 85 + Math.random() * 10,
        memory_usage: 12 + Math.random() * 4,
      },
    }));
  };

  // Generate synthetic metrics history
  const generateMetricsHistory = (jobs: TrainingJob[]): Map<string, TrainingMetrics[]> => {
    const history = new Map<string, TrainingMetrics[]>();

    jobs.forEach(job => {
      const epochs = job.total_epochs || 20;
      const jobHistory: TrainingMetrics[] = [];

      // Generate realistic loss curves
      const initialLoss = 2.5 + Math.random() * 0.5;
      const convergenceRate = 0.08 + Math.random() * 0.04; // Different rates for different configs
      const noiseLevel = 0.02 + Math.random() * 0.01;

      for (let epoch = 0; epoch <= epochs; epoch++) {
        // Exponential decay with noise
        const baseLoss = initialLoss * Math.exp(-convergenceRate * epoch);
        const noise = (Math.random() - 0.5) * noiseLevel;
        const loss = Math.max(0.01, baseLoss + noise);

        // Validation loss slightly higher with more variance
        const valLoss = loss * (1.1 + Math.random() * 0.1);

        // Performance tends to stabilize
        const basePerf = 1000 + (job.config?.rank || 16) * 20;
        const perfVariance = 100;
        const tokens_per_second = basePerf + (Math.random() - 0.5) * perfVariance;

        // GPU and memory usage
        const gpu_utilization = 80 + Math.random() * 15;
        const memory_usage = 10 + (job.config?.rank || 16) * 0.3 + Math.random() * 2;

        jobHistory.push({
          loss,
          tokens_per_second,
          learning_rate: job.config?.learning_rate || 0.0003,
          current_epoch: epoch,
          total_epochs: epochs,
          progress_pct: (epoch / epochs) * 100,
          validation_loss: valLoss,
          gpu_utilization,
          memory_usage,
        });
      }

      history.set(job.id, jobHistory);
    });

    return history;
  };

  const [jobs, setJobs] = useState<TrainingJob[]>(generateExampleJobs());
  const [metricsHistory, setMetricsHistory] = useState<Map<string, TrainingMetrics[]>>(
    () => generateMetricsHistory(generateExampleJobs())
  );

  // Refresh data (simulates real-time updates)
  const refreshData = () => {
    const newJobs = generateExampleJobs();
    setJobs(newJobs);
    setMetricsHistory(generateMetricsHistory(newJobs));
  };

  return (
    <div className="space-y-6 p-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-bold">Training Metrics Comparison</h1>
          <p className="text-muted-foreground mt-1">
            Interactive visualization for comparing training jobs
          </p>
        </div>
        <div className="flex items-center gap-2">
          <Button variant="outline" onClick={refreshData}>
            <RefreshCw className="h-4 w-4 mr-2" />
            Refresh Data
          </Button>
        </div>
      </div>

      {/* Info Cards */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        <Card>
          <CardHeader className="pb-3">
            <CardTitle className="text-sm font-medium">Active Jobs</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{jobs.length}</div>
            <p className="text-xs text-muted-foreground mt-1">
              Training in progress
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="pb-3">
            <CardTitle className="text-sm font-medium">Best Loss</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {Math.min(...jobs.map(j => j.current_loss || Infinity)).toFixed(4)}
            </div>
            <p className="text-xs text-muted-foreground mt-1">
              Across all jobs
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="pb-3">
            <CardTitle className="text-sm font-medium">Avg Throughput</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {Math.round(
                jobs.reduce((sum, j) => sum + (j.tokens_per_second || 0), 0) / jobs.length
              ).toLocaleString()}
            </div>
            <p className="text-xs text-muted-foreground mt-1">
              tokens/second
            </p>
          </CardContent>
        </Card>
      </div>

      {/* Main Comparison Component */}
      <MetricsComparison
        jobs={jobs}
        metricsHistory={metricsHistory}
      />

      {/* Usage Guide */}
      <Card>
        <CardHeader>
          <CardTitle>Usage Guide</CardTitle>
          <CardDescription>
            How to use the metrics comparison visualizations
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div>
            <h4 className="font-medium mb-2">Interactive Features</h4>
            <ul className="space-y-2 text-sm text-muted-foreground">
              <li className="flex items-start gap-2">
                <Badge variant="outline" className="mt-0.5">1</Badge>
                <span>
                  <strong>Show/Hide Jobs:</strong> Click on job badges in the legend to toggle visibility
                </span>
              </li>
              <li className="flex items-start gap-2">
                <Badge variant="outline" className="mt-0.5">2</Badge>
                <span>
                  <strong>Scale Toggle:</strong> Switch between linear and logarithmic scale for loss curves
                </span>
              </li>
              <li className="flex items-start gap-2">
                <Badge variant="outline" className="mt-0.5">3</Badge>
                <span>
                  <strong>Smoothing:</strong> Enable curve smoothing using moving average to reduce noise
                </span>
              </li>
              <li className="flex items-start gap-2">
                <Badge variant="outline" className="mt-0.5">4</Badge>
                <span>
                  <strong>Validation Loss:</strong> Toggle validation loss overlay to identify overfitting
                </span>
              </li>
              <li className="flex items-start gap-2">
                <Badge variant="outline" className="mt-0.5">5</Badge>
                <span>
                  <strong>Export Charts:</strong> Download charts as PNG/SVG for reports (coming soon)
                </span>
              </li>
            </ul>
          </div>

          <div>
            <h4 className="font-medium mb-2">Chart Types</h4>
            <div className="grid grid-cols-1 md:grid-cols-2 gap-3 text-sm">
              <div className="flex items-start gap-2">
                <Badge>Loss</Badge>
                <span className="text-muted-foreground">
                  Training and validation loss curves with best epoch indicator
                </span>
              </div>
              <div className="flex items-start gap-2">
                <Badge>Performance</Badge>
                <span className="text-muted-foreground">
                  Tokens/second throughput over training
                </span>
              </div>
              <div className="flex items-start gap-2">
                <Badge>GPU</Badge>
                <span className="text-muted-foreground">
                  GPU utilization percentage
                </span>
              </div>
              <div className="flex items-start gap-2">
                <Badge>Memory</Badge>
                <span className="text-muted-foreground">
                  GPU memory consumption in GB
                </span>
              </div>
              <div className="flex items-start gap-2">
                <Badge>Convergence</Badge>
                <span className="text-muted-foreground">
                  Loss reduction rate comparison
                </span>
              </div>
            </div>
          </div>

          <div>
            <h4 className="font-medium mb-2">Integration Example</h4>
            <div className="bg-muted p-4 rounded-lg font-mono text-xs space-y-1">
              <div>import &#123; MetricsComparison &#125; from '@/components/training';</div>
              <div className="mt-2">// Fetch jobs and metrics from API</div>
              <div>const jobs = await apiClient.listTrainingJobs();</div>
              <div>const metricsHistory = new Map();</div>
              <div>jobs.forEach(job =&gt; &#123;</div>
              <div className="pl-4">const metrics = await apiClient.getTrainingMetrics(job.id);</div>
              <div className="pl-4">metricsHistory.set(job.id, metrics);</div>
              <div>&#125;);</div>
              <div className="mt-2">// Render comparison</div>
              <div>&lt;MetricsComparison jobs=&#123;jobs&#125; metricsHistory=&#123;metricsHistory&#125; /&gt;</div>
            </div>
          </div>

          <div>
            <h4 className="font-medium mb-2">Accessibility</h4>
            <ul className="space-y-2 text-sm text-muted-foreground">
              <li>• Color-blind friendly palette (Tol Bright scheme)</li>
              <li>• ARIA labels for all interactive elements</li>
              <li>• Keyboard navigation support</li>
              <li>• Responsive design for all screen sizes</li>
            </ul>
          </div>
        </CardContent>
      </Card>
    </div>
  );
};

export default MetricsComparisonExample;
