import React from 'react';
import { Progress } from '@/components/ui/progress';
import { cn } from '@/components/ui/utils';

interface FlowProgressProps {
  currentStep: number;
  totalSteps: number;
  className?: string;
}

export function FlowProgress({ currentStep, totalSteps, className }: FlowProgressProps) {
  const progress = (currentStep / totalSteps) * 100;

  return (
    <div className={cn('space-y-2', className)}>
      <div className="flex items-center justify-between text-sm">
        <span className="text-muted-foreground">Progress</span>
        <span className="font-medium">
          Step {currentStep} of {totalSteps}
        </span>
      </div>
      <Progress value={progress} className="h-2" />
    </div>
  );
}

