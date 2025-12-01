import React, { useMemo } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { ArrowRight, CheckCircle, Circle, AlertCircle, Loader2 } from 'lucide-react';
import { useNavigate } from 'react-router-dom';
import { usePolling } from '@/hooks/usePolling';
import { apiClient } from '@/api/client';
import type { TrainingJob } from '@/api/types';

interface PipelineStage {
  id: string;
  label: string;
  status: 'completed' | 'in_progress' | 'pending' | 'error';
  route: string;
}

export function MLPipelineWidget() {
  const navigate = useNavigate();

  // Fetch training jobs from API
  const { data: response, isLoading, error } = usePolling(
    () => apiClient.listTrainingJobs(),
    'normal',
    {
      operationName: 'MLPipelineWidget.listTrainingJobs',
      showLoadingIndicator: false
    }
  );

  // Extract jobs from paginated response
  const trainingJobs = response?.jobs;

  // Derive pipeline stages from training job data
  const stages: PipelineStage[] = useMemo(() => {
    if (!trainingJobs || trainingJobs.length === 0) {
      // Default state when no training jobs exist
      return [
        { id: 'train', label: 'Train', status: 'pending', route: '/training' },
        { id: 'test', label: 'Test', status: 'pending', route: '/testing' },
        { id: 'compare', label: 'Compare', status: 'pending', route: '/golden' },
        { id: 'promote', label: 'Promote', status: 'pending', route: '/promotion' },
        { id: 'deploy', label: 'Deploy', status: 'pending', route: '/adapters' }
      ];
    }

    // Determine stage status based on training jobs
    const latestJob = trainingJobs[0]; // Assuming sorted by most recent
    const hasCompletedTraining = trainingJobs.some(job => job.status === 'completed');
    const hasRunningTraining = trainingJobs.some(job => job.status === 'running');
    const hasFailedTraining = trainingJobs.some(job => job.status === 'failed');

    return [
      {
        id: 'train',
        label: 'Train',
        status: hasFailedTraining ? 'error' : hasRunningTraining ? 'in_progress' : hasCompletedTraining ? 'completed' : 'pending',
        route: '/training'
      },
      {
        id: 'test',
        label: 'Test',
        status: hasCompletedTraining && latestJob.adapter_id ? 'in_progress' : hasCompletedTraining ? 'pending' : 'pending',
        route: '/testing'
      },
      { id: 'compare', label: 'Compare', status: 'pending', route: '/golden' },
      { id: 'promote', label: 'Promote', status: 'pending', route: '/promotion' },
      { id: 'deploy', label: 'Deploy', status: 'pending', route: '/adapters' }
    ];
  }, [trainingJobs]);

  const getStatusIcon = (status: PipelineStage['status']) => {
    switch (status) {
      case 'completed':
        return <CheckCircle className="h-5 w-5 text-green-600" />;
      case 'in_progress':
        return <Circle className="h-5 w-5 text-blue-600 animate-pulse" />;
      case 'error':
        return <AlertCircle className="h-5 w-5 text-red-600" />;
      default:
        return <Circle className="h-5 w-5 text-muted-foreground" />;
    }
  };

  const getStatusColor = (status: PipelineStage['status']) => {
    switch (status) {
      case 'completed':
        return 'bg-green-100 text-green-800 border-green-200';
      case 'in_progress':
        return 'bg-blue-100 text-blue-800 border-blue-200';
      case 'error':
        return 'bg-red-100 text-red-800 border-red-200';
      default:
        return 'bg-gray-100 text-gray-600 border-gray-200';
    }
  };

  const currentStageIndex = stages.findIndex(s => s.status === 'in_progress');
  const nextStage = currentStageIndex >= 0 && currentStageIndex < stages.length - 1 
    ? stages[currentStageIndex + 1] 
    : null;

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center justify-between">
          <span>ML Pipeline</span>
          <Badge variant="outline">
            {stages.filter(s => s.status === 'completed').length}/{stages.length} Complete
          </Badge>
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        {/* Pipeline Visualization */}
        <div className="flex items-center justify-between">
          {stages.map((stage, idx) => (
            <React.Fragment key={stage.id}>
              <div className="flex flex-col items-center gap-2">
                <button
                  onClick={() => navigate(stage.route)}
                  className="flex flex-col items-center gap-1 hover:opacity-80 transition-opacity"
                >
                  {getStatusIcon(stage.status)}
                  <span className="text-xs font-medium">{stage.label}</span>
                </button>
              </div>
              {idx < stages.length - 1 && (
                <ArrowRight className="h-4 w-4 text-muted-foreground flex-shrink-0" />
              )}
            </React.Fragment>
          ))}
        </div>

        {/* Current Status */}
        {currentStageIndex >= 0 && (
          <div className={`p-3 rounded-lg border ${getStatusColor(stages[currentStageIndex].status)}`}>
            <div className="flex items-center justify-between">
              <div>
                <p className="text-sm font-medium">
                  Currently: {stages[currentStageIndex].label}
                </p>
                <p className="text-xs mt-0.5">
                  Pipeline is actively processing
                </p>
              </div>
              <Button
                size="sm"
                variant="outline"
                onClick={() => navigate(stages[currentStageIndex].route)}
              >
                View
              </Button>
            </div>
          </div>
        )}

        {/* Next Action */}
        {nextStage && (
          <div className="flex items-center justify-between p-3 bg-muted rounded-lg">
            <div className="text-sm">
              <span className="text-muted-foreground">Next: </span>
              <span className="font-medium">{nextStage.label}</span>
            </div>
            <Button
              size="sm"
              onClick={() => navigate(nextStage.route)}
            >
              Continue
              <ArrowRight className="h-4 w-4 ml-1" />
            </Button>
          </div>
        )}

        {/* Completed Message */}
        {stages.every(s => s.status === 'completed') && (
          <div className="p-3 bg-green-50 border border-green-200 rounded-lg text-center">
            <CheckCircle className="h-5 w-5 text-green-600 mx-auto mb-1" />
            <p className="text-sm font-medium text-green-900">
              Pipeline Complete!
            </p>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

