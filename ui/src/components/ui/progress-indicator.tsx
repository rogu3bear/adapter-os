//! Enhanced Progress Indicator Component
//!
//! Provides trust-building progress indicators with time estimates and confidence signals.
//! Now supports real-time updates, operation persistence, and enhanced ETA calculations.
//!
//! Citations:
//! - docs/Smashing Design Techniques.md L1-L50 - Psychology of trust in AI
//! - ui/src/components/SingleFileAdapterTrainer.tsx L450-L500 - Current progress implementation
//! - ui/src/hooks/useProgressOperation.ts L1-L100 - Progress operation management

import React, { useEffect, useState } from 'react';
import { Progress } from './progress';
import { Badge } from './badge';
import { Clock, CheckCircle, AlertTriangle, X } from 'lucide-react';
import { useProgressOperation } from '../../hooks/useProgressOperation';

export interface ProgressIndicatorProps {
  progress: number; // 0-100
  status: string;
  eta?: string; // Estimated time remaining
  confidence?: number; // 0-100, confidence level
  variant?: 'default' | 'success' | 'warning' | 'error';
  className?: string;
  operationId?: string; // For real-time updates
  onCancel?: () => void; // Cancel callback
  showCancel?: boolean; // Whether to show cancel button
  autoUpdate?: boolean; // Enable automatic progress updates
}

export function ProgressIndicator({
  progress,
  status,
  eta,
  confidence,
  variant = 'default',
  className = '',
  operationId,
  onCancel,
  showCancel = false,
  autoUpdate = false
}: ProgressIndicatorProps) {
  const { operation, update, cancel } = useProgressOperation(operationId);
  const [currentProgress, setCurrentProgress] = useState(progress);
  const [currentStatus, setCurrentStatus] = useState(status);
  const [currentEta, setCurrentEta] = useState(eta);
  const [currentVariant, setCurrentVariant] = useState(variant);

  // Sync with operation data if available
  useEffect(() => {
    if (operation) {
      setCurrentProgress(operation.state.progress);
      setCurrentStatus(operation.state.status);
      setCurrentEta(operation.state.eta);
      setCurrentVariant(operation.state.variant || 'default');
    } else if (!autoUpdate) {
      setCurrentProgress(progress);
      setCurrentStatus(status);
      setCurrentEta(eta);
      setCurrentVariant(variant);
    }
  }, [operation, progress, status, eta, variant, autoUpdate]);

  // Handle cancel
  const handleCancel = () => {
    if (operationId) {
      cancel(operationId);
    }
    onCancel?.();
  };
  const getVariantStyles = () => {
    switch (currentVariant) {
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
    switch (currentVariant) {
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
        value={currentProgress}
        className={`w-full [&>div]:${getProgressColor()}`}
      />
      <div className={`flex items-center justify-between text-sm ${getVariantStyles()}`}>
        <div className="flex items-center gap-2">
          <span className="font-medium">{currentStatus}</span>
          {confidence && (
            <Badge variant="outline" className="text-xs">
              {confidence}% complete
            </Badge>
          )}
        </div>
        <div className="flex items-center gap-2">
          {currentEta && (
            <div className="flex items-center gap-1 text-muted-foreground">
              <Clock className="h-3 w-3" />
              <span>~{currentEta} remaining</span>
            </div>
          )}
          {showCancel && currentProgress < 100 && (
            <button
              onClick={handleCancel}
              className="flex items-center gap-1 text-muted-foreground hover:text-foreground transition-colors"
              title="Cancel operation"
            >
              <X className="h-3 w-3" />
            </button>
          )}
        </div>
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
  operationId?: string;
  onCancel?: () => void;
  showCancel?: boolean;
}

export function ContextualLoading({
  type,
  progress,
  eta,
  className = '',
  operationId,
  onCancel,
  showCancel = false
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
            operationId={operationId}
            onCancel={onCancel}
            showCancel={showCancel}
            className="mt-2"
          />
        )}
      </div>
    </div>
  );
}

export default ProgressIndicator;
