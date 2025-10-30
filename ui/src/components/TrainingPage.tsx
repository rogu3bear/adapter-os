import React, { useState, useEffect } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from './ui/table';
import { Dialog, DialogContent } from './ui/dialog';
import { TrainingWizard } from './TrainingWizard';
import { TrainingMonitor } from './TrainingMonitor';
import { TrainingTemplates } from './TrainingTemplates';
import apiClient from '../api/client';
import { TrainingJob } from '../api/types';
import { toast } from 'sonner';
import { logger, toError } from '../utils/logger';
import { Link } from 'react-router-dom';
import { Brain, Activity, Clock, CheckCircle, XCircle, AlertTriangle, Play, Pause, Square } from 'lucide-react';
import { Progress } from './ui/progress';

export function TrainingPage() {
  const [trainingJobs, setTrainingJobs] = useState<TrainingJob[]>([]);
  const [selectedJob, setSelectedJob] = useState<string | null>(null);
  const [isWizardOpen, setIsWizardOpen] = useState(false);
  const [loading, setLoading] = useState(true);
  const [trainingConfig, setTrainingConfig] = useState<any>(null); // State to hold training config for wizard

  useEffect(() => {
    const fetchJobs = async () => {
      try {
        const jobs = await apiClient.listTrainingJobs();
        setTrainingJobs(jobs);
      } catch (err) {
        logger.error('Failed to fetch training jobs', { component: 'TrainingPage' }, toError(err));
        toast.error('Failed to load training jobs');
      } finally {
        setLoading(false);
      }
    };
    fetchJobs();
    const interval = setInterval(fetchJobs, 5000); // Poll every 5s
    return () => clearInterval(interval);
  }, []);

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
    try {
      if (action === 'stop') {
        await apiClient.cancelTraining(jobId);
        toast.success('Job stopped successfully');
      } else {
        toast.info(`${action} is not supported yet`);
      }
    } catch (err) {
      toast.error(`Failed to ${action} job`);
    }
  };

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-bold">Training Management</h1>
          <p className="text-muted-foreground">Manage training jobs, templates, and monitoring</p>
        </div>
        <Button onClick={handleStartTraining}>
          <Brain className="mr-2 h-4 w-4" />
          Start Training
        </Button>
      </div>

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
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Name</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead>Progress</TableHead>
                  <TableHead>Started</TableHead>
                  <TableHead>Actions</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {trainingJobs.map(job => (
                  <TableRow key={job.id}>
                    <TableCell className="font-medium">{job.adapter_name}</TableCell>
                    <TableCell>
                      <Badge variant="outline">
                        {getStatusIcon(job.status)}
                        {job.status}
                      </Badge>
                    </TableCell>
                    <TableCell>
                      <Progress value={job.progress} className="w-24" />
                      {job.progress}%
                    </TableCell>
                    <TableCell>{new Date(job.started_at).toLocaleString()}</TableCell>
                    <TableCell>
                      <div className="flex gap-2">
                        <Button size="sm" variant="outline" onClick={() => setSelectedJob(job.id)}>
                          <Activity className="h-4 w-4" />
                        </Button>
                        {job.status === 'running' && (
                          <>
                            <Button size="sm" variant="outline" onClick={() => handleJobAction(job.id, 'pause')}>
                              <Pause className="h-4 w-4" />
                            </Button>
                            <Button size="sm" variant="destructive" onClick={() => handleJobAction(job.id, 'stop')}>
                              <Square className="h-4 w-4" />
                            </Button>
                          </>
                        )}
                        {job.status === 'completed' && (
                          <Link to="/testing">
                            <Button size="sm" variant="default">
                              Test Adapter
                            </Button>
                          </Link>
                        )}
                      </div>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
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
