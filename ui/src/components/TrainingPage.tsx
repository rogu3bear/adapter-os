// 【ui/src/components/TrainingPage.tsx§25-40】 - Replace manual polling with standardized hook
import React, { useState } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from './ui/table';
import { VirtualizedTableRows } from './ui/virtualized-table';
import { Dialog, DialogContent } from './ui/dialog';
import { TrainingWizard } from './TrainingWizard';
import { TrainingMonitor } from './TrainingMonitor';
import { TrainingTemplates } from './TrainingTemplates';
import apiClient from '../api/client';
import { TrainingJob } from '../api/types';
import { logger, toError } from '../utils/logger';
import { Link } from 'react-router-dom';
import { Brain, Activity, Clock, CheckCircle, XCircle, AlertTriangle, Play, Pause, Square } from 'lucide-react';
import { Progress } from './ui/progress';
import { usePolling } from '../hooks/usePolling';
import { LastUpdated } from './ui/last-updated';
import { ErrorRecovery, ErrorRecoveryTemplates } from './ui/error-recovery';
import { ConfigPageHeader } from './ui/page-headers/ConfigPageHeader';

export function TrainingPage() {
  // 【ui/src/hooks/usePolling.ts】 - Standardized polling hook
  const { data: trainingJobs = [], isLoading: loading, lastUpdated, error, refetch: refreshData } = usePolling(
    () => apiClient.listTrainingJobs(),
    'fast', // Training progress needs frequent updates
    {
      showLoadingIndicator: true,
      onError: (err) => {
        logger.error('Failed to fetch training jobs', { component: 'TrainingPage' }, err);
      }
    }
  );

  const [selectedJob, setSelectedJob] = useState<string | null>(null);
  const [isWizardOpen, setIsWizardOpen] = useState(false);
  const [trainingConfig, setTrainingConfig] = useState<any>(null); // State to hold training config for wizard
  const [actionError, setActionError] = useState<Error | null>(null);

  const getStatusIcon = (status: string) => {
    switch (status) {
      case 'running': return <Activity className="h-4 w-4 text-blue-500 animate-pulse" />;
      case 'completed': return <CheckCircle className="h-4 w-4 text-green-500" />;
      case 'failed': return <XCircle className="h-4 w-4 text-red-500" />;
      case 'queued': return <Clock className="h-4 w-4 text-yellow-500" />;
      default: return <AlertTriangle className="h-4 w-4 text-gray-500" />;
    }
  };

  const handleStartTraining = () => {
    setIsWizardOpen(true);
  };

  const handleJobAction = async (jobId: string, action: 'pause' | 'stop' | 'resume') => {
    setActionError(null);
    try {
      if (action === 'stop') {
        await apiClient.cancelTraining(jobId);
        // Success - could show success feedback but not critical
      } else {
        // Not supported - show info, not error
        setActionError(new Error(`${action} is not supported yet`));
      }
    } catch (err) {
      const error = err instanceof Error ? err : new Error(`Failed to ${action} job`);
      setActionError(error);
      logger.error(`Failed to ${action} training job`, { component: 'TrainingPage', jobId, action }, error);
    }
  };

  return (
    <div className="space-y-6">
      <ConfigPageHeader
        title="Training Management"
        description="Manage training jobs, templates, and monitoring"
        primaryAction={{
          label: 'Start Training',
          icon: Brain,
          onClick: handleStartTraining
        }}
      />
      {lastUpdated && <LastUpdated timestamp={lastUpdated} className="mt-1" />}

      {/* Error Recovery */}
      {error && ErrorRecoveryTemplates.trainingError(
        () => refreshData(),
        () => setIsWizardOpen(true)
      )}
      {actionError && (
        <ErrorRecovery
          title="Action Failed"
          message={actionError.message}
          recoveryActions={[
            { label: 'Retry', action: () => setActionError(null) },
            { label: 'View Logs', action: () => {/* Navigate to logs */} }
          ]}
        />
      )}

      {/* Training Jobs Table */}
      <Card>
        <CardHeader>
          <CardTitle>Training Jobs</CardTitle>
        </CardHeader>
        <CardContent>
          {loading ? (
            <div className="text-center py-8">Loading jobs...</div>
          ) : trainingJobs.length === 0 ? (
            <div className="text-center py-8 text-muted-foreground">No training jobs found</div>
          ) : (
            <div className="max-h-[600px] overflow-auto" data-virtual-container>
              <Table role="table" aria-label="Training jobs">
                <TableHeader>
                  <TableRow role="row">
                    <TableHead role="columnheader" scope="col">Name</TableHead>
                    <TableHead role="columnheader" scope="col">Status</TableHead>
                    <TableHead role="columnheader" scope="col">Progress</TableHead>
                    <TableHead role="columnheader" scope="col">Started</TableHead>
                    <TableHead role="columnheader" scope="col">Actions</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  <VirtualizedTableRows items={trainingJobs} estimateSize={60}>
                    {(job) => {
                      const jobTyped = job as typeof trainingJobs[0];
                      return (
                        <TableRow key={jobTyped.id}>
                          <TableCell className="font-medium">{jobTyped.adapter_name}</TableCell>
                          <TableCell>
                            <Badge variant="outline">
                              {getStatusIcon(jobTyped.status)}
                              {jobTyped.status}
                            </Badge>
                          </TableCell>
                          <TableCell>
                            <Progress value={jobTyped.progress} className="w-24" />
                            {jobTyped.progress}%
                          </TableCell>
                          <TableCell>{new Date(jobTyped.started_at).toLocaleString()}</TableCell>
                          <TableCell>
                            <div className="flex gap-2">
                              <Button size="sm" variant="outline" onClick={() => setSelectedJob(jobTyped.id)}>
                                <Activity className="h-4 w-4" />
                              </Button>
                              {jobTyped.status === 'running' && (
                                <>
                                  <Button size="sm" variant="outline" onClick={() => handleJobAction(jobTyped.id, 'pause')} aria-label={`Pause ${jobTyped.adapter_name}`}>
                                    <Pause className="h-4 w-4" />
                                  </Button>
                                  <Button size="sm" variant="destructive" onClick={() => handleJobAction(jobTyped.id, 'stop')} aria-label={`Stop ${jobTyped.adapter_name}`}>
                                    <Square className="h-4 w-4" />
                                  </Button>
                                </>
                              )}
                              {jobTyped.status === 'completed' && (
                                <Link to="/testing">
                                  <Button size="sm" variant="default" aria-label={`Test ${jobTyped.adapter_name}`}>
                                    Test Adapter
                                  </Button>
                                </Link>
                              )}
                            </div>
                          </TableCell>
                        </TableRow>
                      );
                    }}
                  </VirtualizedTableRows>
                </TableBody>
              </Table>
            </div>
          )}
        </CardContent>
      </Card>

      {/* Training Templates */}
      <TrainingTemplates onTemplateSelect={(template) => {
        // Handle template selection, e.g., open wizard with prefilled config
        setTrainingConfig(template);
        setIsWizardOpen(true);
      }} />

      {/* Training Wizard Dialog */}
      <Dialog open={isWizardOpen} onOpenChange={setIsWizardOpen}>
        <DialogContent className="max-w-4xl">
          <TrainingWizard
            onComplete={(jobId) => {
              setIsWizardOpen(false);
              setSelectedJob(jobId);
            }}
            onCancel={() => setIsWizardOpen(false)}
          />
        </DialogContent>
      </Dialog>

      {/* Job Monitor Dialog */}
      <Dialog open={!!selectedJob} onOpenChange={() => setSelectedJob(null)}>
        <DialogContent className="max-w-6xl">
          {selectedJob && <TrainingMonitor jobId={selectedJob} />}
        </DialogContent>
      </Dialog>
    </div>
  );
}
