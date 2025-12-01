// WorkflowProgress component - Track and visualize workflow execution progress

import React from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Progress } from '@/components/ui/progress';
import { Alert, AlertDescription } from '@/components/ui/alert';
import {
  CheckCircle,
  Circle,
  AlertCircle,
  Clock,
  XCircle,
  Loader2,
  ChevronRight,
  MinusCircle,
} from 'lucide-react';
import { WorkflowProgress as WorkflowProgressType, WorkflowStep } from './types';

interface WorkflowProgressProps {
  progress: WorkflowProgressType;
  steps: WorkflowStep[];
  compact?: boolean;
}

export function WorkflowProgress({ progress, steps, compact = false }: WorkflowProgressProps) {
  const progressPercentage = (progress.currentStep / progress.totalSteps) * 100;
  const currentStepData = steps[progress.currentStep];
  const elapsedTime = Date.now() - new Date(progress.startedAt).getTime();
  const elapsedMinutes = Math.floor(elapsedTime / 60000);
  const elapsedSeconds = Math.floor((elapsedTime % 60000) / 1000);

  const getStepIcon = (stepId: string, index: number) => {
    const status = progress.stepStatus[stepId];

    switch (status) {
      case 'completed':
        return <CheckCircle className="h-5 w-5 text-green-500" />;
      case 'running':
        return <Loader2 className="h-5 w-5 text-blue-500 animate-spin" />;
      case 'failed':
        return <XCircle className="h-5 w-5 text-red-500" />;
      case 'skipped':
        return <MinusCircle className="h-5 w-5 text-gray-400" />;
      case 'pending':
        return <Circle className="h-5 w-5 text-muted-foreground" />;
      default:
        return <Circle className="h-5 w-5 text-muted-foreground" />;
    }
  };

  const getStepColor = (status: string) => {
    switch (status) {
      case 'completed':
        return 'bg-green-500';
      case 'running':
        return 'bg-blue-500';
      case 'failed':
        return 'bg-red-500';
      case 'skipped':
        return 'bg-gray-400';
      default:
        return 'bg-muted';
    }
  };

  if (compact) {
    return (
      <Card>
        <CardContent className="p-4">
          <div className="space-y-3">
            {/* Overall Progress */}
            <div className="space-y-2">
              <div className="flex items-center justify-between text-sm">
                <span className="font-medium">Workflow Progress</span>
                <span className="text-muted-foreground">
                  {progress.currentStep + 1} / {progress.totalSteps}
                </span>
              </div>
              <Progress value={progressPercentage} className="h-2" />
            </div>

            {/* Current Step */}
            {currentStepData && (
              <div className="flex items-center gap-2">
                <Loader2 className="h-4 w-4 animate-spin text-blue-500" />
                <span className="text-sm font-medium">{currentStepData.title}</span>
              </div>
            )}

            {/* Elapsed Time */}
            <div className="flex items-center gap-2 text-xs text-muted-foreground">
              <Clock className="h-3 w-3" />
              <span>
                {elapsedMinutes}m {elapsedSeconds}s elapsed
              </span>
            </div>
          </div>
        </CardContent>
      </Card>
    );
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>Workflow Progress</CardTitle>
      </CardHeader>
      <CardContent className="space-y-6">
        {/* Overall Progress Bar */}
        <div className="space-y-3">
          <div className="flex items-center justify-between">
            <div className="space-y-1">
              <p className="text-sm font-medium">
                Step {progress.currentStep + 1} of {progress.totalSteps}
              </p>
              <p className="text-xs text-muted-foreground">
                {Math.round(progressPercentage)}% complete
              </p>
            </div>
            <div className="flex items-center gap-2 text-sm text-muted-foreground">
              <Clock className="h-4 w-4" />
              <span>
                {elapsedMinutes}m {elapsedSeconds}s
              </span>
            </div>
          </div>
          <Progress value={progressPercentage} className="h-3" />
        </div>

        {/* Current Step Alert */}
        {currentStepData && progress.stepStatus[currentStepData.id] === 'running' && (
          <Alert>
            <Loader2 className="h-4 w-4 animate-spin" />
            <AlertDescription>
              <span className="font-medium">{currentStepData.title}</span>
              <br />
              <span className="text-sm text-muted-foreground">{currentStepData.description}</span>
            </AlertDescription>
          </Alert>
        )}

        {/* Step List */}
        <div className="space-y-2">
          {steps.map((step, index) => {
            const status = progress.stepStatus[step.id];
            const isActive = index === progress.currentStep;
            const stepData = progress.data[step.id];

            return (
              <div
                key={step.id}
                className={`relative flex items-start gap-3 p-3 rounded-lg border transition-all ${
                  isActive
                    ? 'bg-primary/5 border-primary'
                    : status === 'completed'
                    ? 'bg-green-50 border-green-200 dark:bg-green-950 dark:border-green-900'
                    : status === 'failed'
                    ? 'bg-red-50 border-red-200 dark:bg-red-950 dark:border-red-900'
                    : 'bg-muted/20 border-muted'
                }`}
              >
                {/* Step Number/Icon */}
                <div className="flex-shrink-0 mt-0.5">{getStepIcon(step.id, index)}</div>

                {/* Step Content */}
                <div className="flex-1 min-w-0 space-y-1">
                  <div className="flex items-center gap-2">
                    <p className="font-medium text-sm">{step.title}</p>
                    <Badge
                      variant="outline"
                      className={`text-xs ${
                        status === 'completed'
                          ? 'border-green-500 text-green-700 dark:text-green-400'
                          : status === 'running'
                          ? 'border-blue-500 text-blue-700 dark:text-blue-400'
                          : status === 'failed'
                          ? 'border-red-500 text-red-700 dark:text-red-400'
                          : status === 'skipped'
                          ? 'border-gray-400 text-gray-600 dark:text-gray-400'
                          : ''
                      }`}
                    >
                      {status === 'running' && 'In Progress'}
                      {status === 'completed' && 'Completed'}
                      {status === 'failed' && 'Failed'}
                      {status === 'skipped' && 'Skipped'}
                      {status === 'pending' && 'Pending'}
                    </Badge>
                  </div>

                  <p className="text-xs text-muted-foreground">{step.description}</p>

                  {/* Step Data Preview */}
                  {stepData && status === 'completed' && (
                    <div className="mt-2 p-2 bg-background rounded border text-xs font-mono">
                      {typeof stepData === 'string' ? (
                        <span className="text-muted-foreground">{stepData}</span>
                      ) : (
                        <span className="text-muted-foreground">
                          {JSON.stringify(stepData, null, 2).slice(0, 100)}...
                        </span>
                      )}
                    </div>
                  )}

                  {/* Help Text for Active Step */}
                  {isActive && step.helpText && (
                    <p className="text-xs text-muted-foreground italic mt-1">{step.helpText}</p>
                  )}
                </div>

                {/* Connector Line */}
                {index < steps.length - 1 && (
                  <div
                    className={`absolute left-[18px] top-12 w-0.5 h-6 ${getStepColor(
                      progress.stepStatus[steps[index + 1].id]
                    )}`}
                  />
                )}
              </div>
            );
          })}
        </div>

        {/* Summary Stats */}
        <div className="grid grid-cols-4 gap-4 pt-4 border-t">
          <div className="text-center">
            <p className="text-2xl font-bold text-green-600">
              {Object.values(progress.stepStatus).filter((s) => s === 'completed').length}
            </p>
            <p className="text-xs text-muted-foreground">Completed</p>
          </div>
          <div className="text-center">
            <p className="text-2xl font-bold text-blue-600">
              {Object.values(progress.stepStatus).filter((s) => s === 'running').length}
            </p>
            <p className="text-xs text-muted-foreground">Running</p>
          </div>
          <div className="text-center">
            <p className="text-2xl font-bold text-gray-600">
              {Object.values(progress.stepStatus).filter((s) => s === 'pending').length}
            </p>
            <p className="text-xs text-muted-foreground">Pending</p>
          </div>
          <div className="text-center">
            <p className="text-2xl font-bold text-red-600">
              {Object.values(progress.stepStatus).filter((s) => s === 'failed').length}
            </p>
            <p className="text-xs text-muted-foreground">Failed</p>
          </div>
        </div>
      </CardContent>
    </Card>
  );
}
