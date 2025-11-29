// 【ui/src/components/TrainingPage.tsx§25-40】 - Replace manual polling with standardized hook
import React, { useEffect, useState } from 'react';
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
import { Link, useLocation, useNavigate } from 'react-router-dom';
import { Brain, Activity, Clock, CheckCircle, XCircle, AlertTriangle, Play, Pause, Square, RefreshCw, Trash2 } from 'lucide-react';
import { Progress } from './ui/progress';
import { usePolling } from '../hooks/usePolling';
import { LastUpdated } from './ui/last-updated';
import { ErrorRecovery, errorRecoveryTemplates } from './ui/error-recovery';
import { ConfigPageHeader } from './ui/page-headers/ConfigPageHeader';
import { useProgressOperation } from '../hooks/useProgressOperation';
import { HelpTooltip } from './ui/help-tooltip';
import { useRBAC } from '../hooks/useRBAC';
import { PageErrorsProvider, PageErrors, usePageErrors } from '@/components/ui/page-error-boundary';
import { useAdapterStacks } from '../hooks/useAdmin';
import { TrainingJobMonitor } from './TrainingJobMonitor';

function TrainingPageContent({ selectedTenant }: { selectedTenant?: string } = {}) {
  const { can, userRole } = useRBAC();
  const { errors, addError, clearError } = usePageErrors();
  const { data: stacks = [] } = useAdapterStacks();
  const location = useLocation();
  const navigate = useNavigate();

  // Determine polling speed based on active jobs
  const [hasActiveJobs, setHasActiveJobs] = useState(false);

  // 【ui/src/hooks/usePolling.ts】 - Standardized polling hook
  // Use 'fast' (5s) for active jobs, 'slow' (30s) for completed jobs view
  const { data: response, isLoading: loading, lastUpdated, error, refetch: refreshData } = usePolling(
    () => apiClient.listTrainingJobs(),
    hasActiveJobs ? 'fast' : 'slow',
    {
      showLoadingIndicator: true,
      onError: (err) => {
        logger.error('Failed to fetch training jobs', { component: 'TrainingPage' }, err);
      },
      onSuccess: (resp) => {
        // Update polling speed based on active jobs
        const jobs = (resp as { jobs?: Array<{ status: string }> })?.jobs || [];
        const active = jobs.some(j => j.status === 'running' || j.status === 'queued');
        setHasActiveJobs(active || false);
      }
    }
  );

  // Extract jobs array from response
  const data = response?.jobs;

  // Handle null data case from usePolling hook - use established pattern
  const trainingJobs = data ?? [];

  const [selectedJob, setSelectedJob] = useState<string | null>(null);
  const [isWizardOpen, setIsWizardOpen] = useState(false);
  const [trainingConfig, setTrainingConfig] = useState<any>(null); // State to hold training config for wizard
  const [cancellingJobs, setCancellingJobs] = useState<Set<string>>(new Set()); // Track jobs being cancelled

  useEffect(() => {
    const shouldOpenWizard = (location.state as { openTrainingWizard?: boolean } | null)?.openTrainingWizard;
    if (shouldOpenWizard) {
      setIsWizardOpen(true);
      navigate(location.pathname, { replace: true, state: {} });
    }
  }, [location.pathname, location.state, navigate]);

  // Monitor all active training jobs for completion notifications
  const handleAdapterCreated = (adapterId: string, jobId: string) => {
    logger.info('Adapter created from training', {
      component: 'TrainingPage',
      adapterId,
      jobId,
    });
    refreshData(); // Refresh to show new adapter links
  };

  // Progress tracking for training operations
  const { operation: activeTrainingOperation, start: startTrainingOperation, cancel: cancelTrainingOperation } = useProgressOperation();

  const getStatusIcon = (status: string) => {
    switch (status) {
      case 'running': return <Activity className="h-4 w-4 text-gray-400 animate-pulse" />;
      case 'completed': return <CheckCircle className="h-4 w-4 text-gray-600" />;
      case 'failed': return <XCircle className="h-4 w-4 text-gray-700" />;
      case 'queued': return <Clock className="h-4 w-4 text-gray-500" />;
      default: return <AlertTriangle className="h-4 w-4 text-gray-500" />;
    }
  };

  const getStatusDescription = (status: string): string => {
    switch (status) {
      case 'running': return 'Training is actively in progress';
      case 'completed': return 'Training finished successfully';
      case 'failed': return 'Training encountered an error and stopped';
      case 'queued': return 'Waiting in queue to start';
      case 'paused': return 'Training temporarily paused';
      case 'cancelled': return 'Training was cancelled by user';
      default: return 'Unknown status';
    }
  };

  const handleStartTraining = () => {
    clearError('start-training');
    setIsWizardOpen(true);
  };

  const [pausingJobs, setPausingJobs] = useState<Set<string>>(new Set()); // Track jobs being paused
  const [resumingJobs, setResumingJobs] = useState<Set<string>>(new Set()); // Track jobs being resumed

  const handleJobAction = async (jobId: string, action: 'pause' | 'stop' | 'resume') => {
    clearError('job-action');
    try {
      if (action === 'stop') {
        // Track cancelling state
        setCancellingJobs(prev => new Set(prev).add(jobId));

        // Start progress tracking for cancellation
        if (selectedTenant) {
          startTrainingOperation('training', jobId, selectedTenant);
        }

        await apiClient.cancelTraining(jobId);

        // Success - remove from cancelling set
        setCancellingJobs(prev => {
          const newSet = new Set(prev);
          newSet.delete(jobId);
          return newSet;
        });

        // Refresh to get updated status
        refreshData();
      } else if (action === 'pause') {
        // Track pausing state
        setPausingJobs(prev => new Set(prev).add(jobId));

        await apiClient.pauseTrainingSession(jobId);

        // Success - remove from pausing set
        setPausingJobs(prev => {
          const newSet = new Set(prev);
          newSet.delete(jobId);
          return newSet;
        });

        // Refresh to get updated status
        refreshData();
      } else if (action === 'resume') {
        // Track resuming state
        setResumingJobs(prev => new Set(prev).add(jobId));

        await apiClient.resumeTrainingSession(jobId);

        // Success - remove from resuming set
        setResumingJobs(prev => {
          const newSet = new Set(prev);
          newSet.delete(jobId);
          return newSet;
        });

        // Refresh to get updated status
        refreshData();
      }
    } catch (err) {
      // Remove from all tracking sets on error
      setCancellingJobs(prev => {
        const newSet = new Set(prev);
        newSet.delete(jobId);
        return newSet;
      });
      setPausingJobs(prev => {
        const newSet = new Set(prev);
        newSet.delete(jobId);
        return newSet;
      });
      setResumingJobs(prev => {
        const newSet = new Set(prev);
        newSet.delete(jobId);
        return newSet;
      });

      const error = err instanceof Error ? err : new Error(`Failed to ${action} job`);
      addError('job-action', error.message, () => handleJobAction(jobId, action));
      logger.error(`Failed to ${action} training job`, { component: 'TrainingPage', jobId, action }, error);
    }
  };

  const handleDeleteJob = async (jobId: string) => {
    clearError('delete-job');
    try {
      // For now, delete is equivalent to cancel for non-running jobs
      await apiClient.cancelTraining(jobId);
      refreshData();
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Failed to delete job');
      addError('delete-job', error.message, () => handleDeleteJob(jobId));
      logger.error('Failed to delete training job', { component: 'TrainingPage', jobId }, error);
    }
  };

  return (
    <div className="space-y-6">
      {/* Monitor all active training jobs */}
      <TrainingJobMonitor jobs={trainingJobs} onAdapterCreated={handleAdapterCreated} />

      <ConfigPageHeader
        title="Training Management"
        description="Manage training jobs, templates, and monitoring"
        primaryAction={can('training:start') ? {
          label: 'Start Training',
          icon: Brain,
          onClick: handleStartTraining
        } : undefined}
      />

      {/* Show disabled button with tooltip for non-permitted users */}
      {!can('training:start') && (
        <div className="flex justify-end -mt-4">
          <Button
            disabled
            title="Requires training:start permission"
            className="opacity-50 cursor-not-allowed"
          >
            <Brain className="h-4 w-4 mr-2" />
            Start Training
          </Button>
        </div>
      )}

      {lastUpdated && <LastUpdated timestamp={lastUpdated} className="mt-1" />}

      {/* Consolidated Error Display */}
      <PageErrors errors={errors} />

      {/* Error Recovery - Main fetch error */}
      {error && errorRecoveryTemplates.trainingError(
        () => refreshData(),
        () => setIsWizardOpen(true)
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
                    <TableHead role="columnheader" scope="col">
                      <HelpTooltip helpId="training-job-id">
                        ID
                      </HelpTooltip>
                    </TableHead>
                    <TableHead role="columnheader" scope="col">
                      <HelpTooltip helpId="training-dataset">
                        Dataset
                      </HelpTooltip>
                    </TableHead>
                    <TableHead role="columnheader" scope="col">
                      <HelpTooltip helpId="training-status">
                        Status
                      </HelpTooltip>
                    </TableHead>
                    <TableHead role="columnheader" scope="col">
                      <HelpTooltip helpId="training-progress">
                        Progress
                      </HelpTooltip>
                    </TableHead>
                    <TableHead role="columnheader" scope="col">
                      <HelpTooltip helpId="training-loss">
                        Loss
                      </HelpTooltip>
                    </TableHead>
                    <TableHead role="columnheader" scope="col">
                      <HelpTooltip helpId="training-created">
                        Created
                      </HelpTooltip>
                    </TableHead>
                    <TableHead role="columnheader" scope="col">
                      <HelpTooltip helpId="training-actions">
                        Actions
                      </HelpTooltip>
                    </TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  <VirtualizedTableRows items={trainingJobs} estimateSize={60}>
                    {(job) => {
                      const jobTyped = job as typeof trainingJobs[0];
                      return (
                        <TableRow key={jobTyped.id}>
                          <TableCell className="font-medium">{jobTyped.adapter_name || jobTyped.id}</TableCell>
                          <TableCell className="text-muted-foreground">
                            {jobTyped.dataset_id || '-'}
                          </TableCell>
                          <TableCell>
                            <HelpTooltip helpId={`status-${jobTyped.status}`} side="right">
                              <Badge variant="outline" title={getStatusDescription(jobTyped.status)}>
                                {getStatusIcon(jobTyped.status)}
                                <span className="ml-1">{jobTyped.status}</span>
                              </Badge>
                            </HelpTooltip>
                          </TableCell>
                          <TableCell>
                            <div className="flex items-center gap-2">
                              <Progress value={jobTyped.progress} className="w-24" />
                              <span className="text-sm">{jobTyped.progress}%</span>
                            </div>
                          </TableCell>
                          <TableCell className="text-muted-foreground">
                            {jobTyped.loss?.toFixed(4) || '-'}
                          </TableCell>
                          <TableCell>{new Date(jobTyped.started_at).toLocaleString()}</TableCell>
                          <TableCell>
                            <div className="flex gap-2">
                              <Button
                                size="sm"
                                variant="outline"
                                onClick={() => setSelectedJob(jobTyped.id)}
                                title="View training details"
                              >
                                <Activity className="h-4 w-4" />
                              </Button>
                              {jobTyped.status === 'running' && (
                                <>
                                  <Button
                                    size="sm"
                                    variant={pausingJobs.has(jobTyped.id) ? "secondary" : "outline"}
                                    onClick={() => handleJobAction(jobTyped.id, 'pause')}
                                    disabled={!can('training:cancel') || cancellingJobs.has(jobTyped.id) || pausingJobs.has(jobTyped.id)}
                                    title={!can('training:cancel') ? 'Requires training:cancel permission' : `Pause ${jobTyped.adapter_name}`}
                                    aria-label={`Pause ${jobTyped.adapter_name}`}
                                  >
                                    {pausingJobs.has(jobTyped.id) ? (
                                      <RefreshCw className="h-4 w-4 animate-spin" />
                                    ) : (
                                      <Pause className="h-4 w-4" />
                                    )}
                                  </Button>
                                  <Button
                                    size="sm"
                                    variant={cancellingJobs.has(jobTyped.id) ? "secondary" : "destructive"}
                                    onClick={() => handleJobAction(jobTyped.id, 'stop')}
                                    disabled={!can('training:cancel') || cancellingJobs.has(jobTyped.id) || pausingJobs.has(jobTyped.id)}
                                    title={!can('training:cancel') ? 'Requires training:cancel permission' : `Stop ${jobTyped.adapter_name}`}
                                    aria-label={`Stop ${jobTyped.adapter_name}`}
                                  >
                                    {cancellingJobs.has(jobTyped.id) ? (
                                      <RefreshCw className="h-4 w-4 animate-spin" />
                                    ) : (
                                      <Square className="h-4 w-4" />
                                    )}
                                  </Button>
                                </>
                              )}
                              {jobTyped.status === 'paused' && (
                                <>
                                  <Button
                                    size="sm"
                                    variant={resumingJobs.has(jobTyped.id) ? "secondary" : "default"}
                                    onClick={() => handleJobAction(jobTyped.id, 'resume')}
                                    disabled={!can('training:start') || resumingJobs.has(jobTyped.id)}
                                    title={!can('training:start') ? 'Requires training:start permission' : `Resume ${jobTyped.adapter_name}`}
                                    aria-label={`Resume ${jobTyped.adapter_name}`}
                                  >
                                    {resumingJobs.has(jobTyped.id) ? (
                                      <RefreshCw className="h-4 w-4 animate-spin" />
                                    ) : (
                                      <Play className="h-4 w-4" />
                                    )}
                                  </Button>
                                  <Button
                                    size="sm"
                                    variant={cancellingJobs.has(jobTyped.id) ? "secondary" : "destructive"}
                                    onClick={() => handleJobAction(jobTyped.id, 'stop')}
                                    disabled={!can('training:cancel') || cancellingJobs.has(jobTyped.id) || resumingJobs.has(jobTyped.id)}
                                    title={!can('training:cancel') ? 'Requires training:cancel permission' : `Stop ${jobTyped.adapter_name}`}
                                    aria-label={`Stop ${jobTyped.adapter_name}`}
                                  >
                                    {cancellingJobs.has(jobTyped.id) ? (
                                      <RefreshCw className="h-4 w-4 animate-spin" />
                                    ) : (
                                      <Square className="h-4 w-4" />
                                    )}
                                  </Button>
                                </>
                              )}
                              {(jobTyped.status === 'completed' || jobTyped.status === 'failed' || jobTyped.status === 'cancelled') && (
                                <Button
                                  size="sm"
                                  variant="ghost"
                                  onClick={() => handleDeleteJob(jobTyped.id)}
                                  disabled={!can('training:cancel')}
                                  title={!can('training:cancel') ? 'Requires training:cancel permission' : `Delete job ${jobTyped.id}`}
                                  aria-label={`Delete ${jobTyped.adapter_name}`}
                                >
                                  <Trash2 className="h-4 w-4" />
                                </Button>
                              )}
                              {jobTyped.status === 'completed' && (
                                <div className="flex items-center gap-2">
                                  {jobTyped.adapter_id && (
                                    <Link to={`/adapters/${jobTyped.adapter_id}`}>
                                      <Button size="sm" variant="outline" aria-label={`View adapter ${jobTyped.adapter_id}`}>
                                        View Adapter
                                      </Button>
                                    </Link>
                                  )}
                                  {jobTyped.stack_id && (
                                    <Link to={`/admin/stacks`}>
                                      <Button size="sm" variant="outline" aria-label={`View stack ${jobTyped.stack_id}`}>
                                        View Stack
                                      </Button>
                                    </Link>
                                  )}
                                  {!jobTyped.stack_id && jobTyped.adapter_id && (() => {
                                    const stack = stacks.find(s => s.adapter_ids?.includes(jobTyped.adapter_id!));
                                    return stack ? (
                                      <Link to={`/admin/stacks`}>
                                        <Button size="sm" variant="outline" aria-label={`View stack ${stack.id}`}>
                                          View Stack
                                        </Button>
                                      </Link>
                                    ) : null;
                                  })()}
                                  <Link to="/inference">
                                    <Button size="sm" variant="default" aria-label={`Test ${jobTyped.adapter_name}`}>
                                      Test in Chat
                                    </Button>
                                  </Link>
                                </div>
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
              refreshData();
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

// Wrap with PageErrorsProvider
export function TrainingPage(props: { selectedTenant?: string }) {
  return (
    <PageErrorsProvider>
      <TrainingPageContent {...props} />
    </PageErrorsProvider>
  );
}
