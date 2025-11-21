import React, { useState, useMemo } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '../ui/card';
import { Badge } from '../ui/badge';
import { Button } from '../ui/button';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '../ui/table';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '../ui/tabs';
import apiClient from '../../api/client';
import { TrainingJob, TrainingStatus } from '../../api/training-types';
import { usePolling } from '../../hooks/usePolling';
import {
  FlaskConical,
  Activity,
  CheckCircle,
  XCircle,
  Clock,
  AlertTriangle,
  RefreshCw,
  TrendingDown,
  Settings,
} from 'lucide-react';
import { logger } from '../../utils/logger';
import {
  LineChart,
  Line,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
  Legend,
} from 'recharts';

// Mock loss curve data for experiments
const generateLossCurve = (jobId: string) => {
  const steps = 50;
  const data = [];
  let loss = 2.5 + Math.random() * 0.5;
  for (let i = 0; i <= steps; i++) {
    loss = Math.max(0.1, loss - (Math.random() * 0.08 + 0.02));
    data.push({
      step: i * 10,
      loss: parseFloat(loss.toFixed(4)),
      validationLoss: parseFloat((loss + Math.random() * 0.2).toFixed(4)),
    });
  }
  return data;
};

export default function DataScientistExperimentTracker() {
  const [selectedExperiment, setSelectedExperiment] = useState<string | null>(null);

  // Poll for training jobs (experiments)
  const { data: experiments, isLoading, error, refetch } = usePolling<TrainingJob[]>(
    () => apiClient.listTrainingJobs(),
    'normal',
    {
      onError: (err) => {
        logger.error('Failed to fetch experiments', { component: 'DataScientistExperimentTracker' }, err);
      },
    }
  );

  const getStatusBadge = (status: TrainingStatus) => {
    switch (status) {
      case 'running':
        return (
          <Badge variant="outline" className="bg-blue-50 text-blue-700 border-blue-200">
            <Activity className="h-3 w-3 mr-1 animate-pulse" />
            Running
          </Badge>
        );
      case 'completed':
        return (
          <Badge variant="outline" className="bg-green-50 text-green-700 border-green-200">
            <CheckCircle className="h-3 w-3 mr-1" />
            Completed
          </Badge>
        );
      case 'failed':
        return (
          <Badge variant="outline" className="bg-red-50 text-red-700 border-red-200">
            <XCircle className="h-3 w-3 mr-1" />
            Failed
          </Badge>
        );
      case 'pending':
        return (
          <Badge variant="outline" className="bg-yellow-50 text-yellow-700 border-yellow-200">
            <Clock className="h-3 w-3 mr-1" />
            Pending
          </Badge>
        );
      case 'cancelled':
        return (
          <Badge variant="outline" className="bg-gray-50 text-gray-700 border-gray-200">
            <AlertTriangle className="h-3 w-3 mr-1" />
            Cancelled
          </Badge>
        );
      default:
        return <Badge variant="outline">{status}</Badge>;
    }
  };

  const selectedJob = useMemo(() => {
    return experiments?.find((e) => e.id === selectedExperiment);
  }, [experiments, selectedExperiment]);

  const lossCurveData = useMemo(() => {
    if (!selectedExperiment) return [];
    return generateLossCurve(selectedExperiment);
  }, [selectedExperiment]);

  const formatDuration = (startedAt?: string, completedAt?: string) => {
    if (!startedAt) return '-';
    const start = new Date(startedAt).getTime();
    const end = completedAt ? new Date(completedAt).getTime() : Date.now();
    const seconds = Math.floor((end - start) / 1000);
    if (seconds < 60) return `${seconds}s`;
    if (seconds < 3600) return `${Math.floor(seconds / 60)}m ${seconds % 60}s`;
    return `${Math.floor(seconds / 3600)}h ${Math.floor((seconds % 3600) / 60)}m`;
  };

  if (isLoading && !experiments) {
    return (
      <div className="flex items-center justify-center h-full">
        <Card className="w-full max-w-md">
          <CardContent className="pt-6 text-center">
            <RefreshCw className="h-8 w-8 mx-auto mb-4 animate-spin text-muted-foreground" />
            <p className="text-sm text-muted-foreground">Loading experiments...</p>
          </CardContent>
        </Card>
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex items-center justify-center h-full">
        <Card className="w-full max-w-md">
          <CardContent className="pt-6 text-center">
            <AlertTriangle className="h-8 w-8 mx-auto mb-4 text-red-500" />
            <p className="text-sm text-red-600">Failed to load experiments</p>
            <Button variant="outline" size="sm" className="mt-4" onClick={() => refetch()}>
              Retry
            </Button>
          </CardContent>
        </Card>
      </div>
    );
  }

  return (
    <div className="space-y-6 p-4">
      <Card>
        <CardHeader className="flex flex-row items-center justify-between">
          <div>
            <CardTitle className="flex items-center gap-2">
              <FlaskConical className="h-5 w-5" />
              Experiment Tracker
            </CardTitle>
            <p className="text-sm text-muted-foreground mt-1">
              Track and compare training experiments
            </p>
          </div>
          <Button variant="outline" size="sm" onClick={() => refetch()}>
            <RefreshCw className="h-4 w-4 mr-2" />
            Refresh
          </Button>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
            {/* Experiment List */}
            <div>
              <h3 className="text-sm font-medium mb-3">Experiments</h3>
              <div className="border rounded-lg overflow-hidden">
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Name</TableHead>
                      <TableHead>Status</TableHead>
                      <TableHead>Loss</TableHead>
                      <TableHead>Duration</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {experiments?.map((exp) => (
                      <TableRow
                        key={exp.id}
                        className={`cursor-pointer ${selectedExperiment === exp.id ? 'bg-muted' : ''}`}
                        onClick={() => setSelectedExperiment(exp.id)}
                      >
                        <TableCell>
                          <span className="font-medium text-sm">
                            {exp.adapter_name || exp.id.slice(0, 8)}
                          </span>
                        </TableCell>
                        <TableCell>{getStatusBadge(exp.status)}</TableCell>
                        <TableCell className="font-mono text-sm">
                          {exp.loss?.toFixed(4) || exp.current_loss?.toFixed(4) || '-'}
                        </TableCell>
                        <TableCell className="text-muted-foreground text-sm">
                          {formatDuration(exp.started_at, exp.completed_at)}
                        </TableCell>
                      </TableRow>
                    ))}
                    {(!experiments || experiments.length === 0) && (
                      <TableRow>
                        <TableCell colSpan={4} className="text-center text-muted-foreground py-8">
                          No experiments found
                        </TableCell>
                      </TableRow>
                    )}
                  </TableBody>
                </Table>
              </div>
            </div>

            {/* Experiment Details */}
            <div>
              {selectedJob ? (
                <Tabs defaultValue="hyperparams">
                  <TabsList className="mb-4">
                    <TabsTrigger value="hyperparams">
                      <Settings className="h-4 w-4 mr-2" />
                      Hyperparameters
                    </TabsTrigger>
                    <TabsTrigger value="curves">
                      <TrendingDown className="h-4 w-4 mr-2" />
                      Loss Curves
                    </TabsTrigger>
                  </TabsList>

                  <TabsContent value="hyperparams">
                    <Card>
                      <CardContent className="pt-4">
                        <Table>
                          <TableBody>
                            <TableRow>
                              <TableCell className="font-medium">Learning Rate</TableCell>
                              <TableCell className="font-mono">
                                {selectedJob.config?.learning_rate || selectedJob.learning_rate || '0.0001'}
                              </TableCell>
                            </TableRow>
                            <TableRow>
                              <TableCell className="font-medium">Epochs</TableCell>
                              <TableCell className="font-mono">
                                {selectedJob.config?.epochs || selectedJob.total_epochs || '10'}
                              </TableCell>
                            </TableRow>
                            <TableRow>
                              <TableCell className="font-medium">Batch Size</TableCell>
                              <TableCell className="font-mono">
                                {selectedJob.config?.batch_size || '32'}
                              </TableCell>
                            </TableRow>
                            <TableRow>
                              <TableCell className="font-medium">Rank</TableCell>
                              <TableCell className="font-mono">
                                {selectedJob.config?.rank || '16'}
                              </TableCell>
                            </TableRow>
                            <TableRow>
                              <TableCell className="font-medium">Alpha</TableCell>
                              <TableCell className="font-mono">
                                {selectedJob.config?.alpha || '32'}
                              </TableCell>
                            </TableRow>
                            <TableRow>
                              <TableCell className="font-medium">Weight Decay</TableCell>
                              <TableCell className="font-mono">
                                {selectedJob.config?.weight_decay || '0.01'}
                              </TableCell>
                            </TableRow>
                            <TableRow>
                              <TableCell className="font-medium">Progress</TableCell>
                              <TableCell className="font-mono">
                                {selectedJob.progress_pct ? `${selectedJob.progress_pct.toFixed(1)}%` : '-'}
                              </TableCell>
                            </TableRow>
                            <TableRow>
                              <TableCell className="font-medium">Tokens/sec</TableCell>
                              <TableCell className="font-mono">
                                {selectedJob.tokens_per_second?.toFixed(0) || '-'}
                              </TableCell>
                            </TableRow>
                          </TableBody>
                        </Table>
                      </CardContent>
                    </Card>
                  </TabsContent>

                  <TabsContent value="curves">
                    <Card>
                      <CardContent className="pt-4">
                        <div className="h-64">
                          <ResponsiveContainer width="100%" height="100%">
                            <LineChart data={lossCurveData}>
                              <CartesianGrid strokeDasharray="3 3" />
                              <XAxis
                                dataKey="step"
                                tick={{ fontSize: 12 }}
                                label={{ value: 'Step', position: 'insideBottom', offset: -5 }}
                              />
                              <YAxis
                                tick={{ fontSize: 12 }}
                                label={{ value: 'Loss', angle: -90, position: 'insideLeft' }}
                              />
                              <Tooltip />
                              <Legend />
                              <Line
                                type="monotone"
                                dataKey="loss"
                                stroke="#2563eb"
                                strokeWidth={2}
                                dot={false}
                                name="Training Loss"
                              />
                              <Line
                                type="monotone"
                                dataKey="validationLoss"
                                stroke="#dc2626"
                                strokeWidth={2}
                                dot={false}
                                name="Validation Loss"
                              />
                            </LineChart>
                          </ResponsiveContainer>
                        </div>
                      </CardContent>
                    </Card>
                  </TabsContent>
                </Tabs>
              ) : (
                <div className="border rounded-lg p-8 text-center text-muted-foreground">
                  <FlaskConical className="h-12 w-12 mx-auto mb-4 opacity-50" />
                  <p>Select an experiment to view details</p>
                </div>
              )}
            </div>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
