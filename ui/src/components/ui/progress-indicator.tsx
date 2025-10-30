//! Enhanced Progress Indicator Component
//!
//! Provides trust-building progress indicators with time estimates and confidence signals.
//!
//! Citations:
//! - docs/Smashing Design Techniques.md L1-L50 - Psychology of trust in AI
//! - ui/src/components/SingleFileAdapterTrainer.tsx L450-L500 - Current progress implementation

import React from 'react';
import { Progress } from './progress';
import { Badge } from './badge';
import { Clock, CheckCircle, AlertTriangle } from 'lucide-react';

export interface ProgressIndicatorProps {
  progress: number; // 0-100
  status: string;
  eta?: string; // Estimated time remaining
  confidence?: number; // 0-100, confidence level
  variant?: 'default' | 'success' | 'warning' | 'error';
  className?: string;
}

export function ProgressIndicator({
  progress,
  status,
  eta,
  confidence,
  variant = 'default',
  className = ''
}: ProgressIndicatorProps) {
  const getVariantStyles = () => {
    switch (variant) {
      case 'success':
        return 'text-green-600';
      case 'warning':
        return 'text-amber-600';
      case 'error':
        return 'text-red-600';
      default:
        return 'text-blue-600';
    }
  };

  const getProgressColor = () => {
    switch (variant) {
      case 'success':
        return 'bg-green-500';
      case 'warning':
        return 'bg-amber-500';
      case 'error':
        return 'bg-red-500';
      default:
        return 'bg-blue-500';
    }
  };

  return (
    <div className={`space-y-2 ${className}`}>
      <Progress
        value={progress}
        className={`w-full [&>div]:${getProgressColor()}`}
      />
      <div className={`flex items-center justify-between text-sm ${getVariantStyles()}`}>
        <div className="flex items-center gap-2">
          <span className="font-medium">{status}</span>
          {confidence && (
            <Badge variant="outline" className="text-xs">
              {confidence}% complete
            </Badge>
          )}
        </div>
        {eta && (
          <div className="flex items-center gap-1 text-muted-foreground">
            <Clock className="h-3 w-3" />
            <span>~{eta} remaining</span>
          </div>
        )}
      </div>
    </div>
  );
}

// Contextual loading messages for different operations
export const LoadingStates = {
  adapterLoad: "Loading adapter into memory... (may take 30-60 seconds)",
  training: "Training adapter on your data... (typically 2-15 minutes)",
  inference: "Generating response... (usually <5 seconds)",
  validation: "Validating configuration... (usually <2 seconds)",
  upload: "Uploading and processing file... (depends on file size)",
  analysis: "Analyzing your code... (usually <10 seconds)"
} as const;

export type LoadingStateType = keyof typeof LoadingStates;

// Loading indicator with contextual messaging
interface ContextualLoadingProps {
  type: LoadingStateType;
  progress?: number;
  eta?: string;
  className?: string;
}

export function ContextualLoading({
  type,
  progress,
  eta,
  className = ''
}: ContextualLoadingProps) {
  return (
    <div className={`flex items-center gap-3 p-4 bg-muted rounded-lg ${className}`}>
      <div className="animate-spin rounded-full h-4 w-4 border-2 border-primary border-t-transparent" />
      <div className="flex-1">
        <p className="text-sm font-medium">{LoadingStates[type]}</p>
        {progress !== undefined && (
          <ProgressIndicator
            progress={progress}
            status="Processing..."
            eta={eta}
            className="mt-2"
          />
        )}
      </div>
    </div>
  );
}

export default ProgressIndicator;
