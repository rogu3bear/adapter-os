/**
 * InlineModelLoadingBlock - Inline model status and loading control for chat
 *
 * Displays model status directly above the chat input with actionable CTAs.
 * Shows loading progress, error states with retry, and auto-load preference.
 *
 * States:
 * - no-model/checking: "Model not loaded" + Load button
 * - loading: "Loading model..." + spinner + progress bar
 * - error: Error message + Retry button
 * - ready: Component not rendered (handled by parent)
 *
 * Copyright JKCA | 2025 James KC Auchterlonie
 */

import React from 'react';
import { Server, Loader2, AlertTriangle, RefreshCw } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Progress } from '@/components/ui/progress';
import { cn } from '@/lib/utils';
import type { ModelStatusState } from '@/hooks/model-loading/types';

// ============================================================================
// Types
// ============================================================================

export interface InlineModelLoadingBlockProps {
  /** Current model status */
  modelStatus: ModelStatusState;

  /** Model display name (e.g., "Qwen2.5-7B") */
  modelName: string | null;

  /** Backend info (e.g., "CoreML", "Metal", "MLX") */
  backendInfo?: string;

  /** Error message if load failed */
  errorMessage: string | null;

  /** Whether a loading operation is in progress */
  isLoading: boolean;

  /** Loading progress (0-100) */
  progress?: number;

  /** Callback when user clicks "Load model" */
  onLoadModel: () => void;

  /** Callback when user clicks "Retry" after failure */
  onRetry?: () => void;

  /** Whether auto-load preference is enabled */
  autoLoadEnabled: boolean;

  /** Callback when auto-load preference changes */
  onAutoLoadChange: (enabled: boolean) => void;

  /** Additional CSS classes */
  className?: string;
}

// ============================================================================
// Component
// ============================================================================

export function InlineModelLoadingBlock({
  modelStatus,
  modelName,
  backendInfo,
  errorMessage,
  isLoading,
  progress,
  onLoadModel,
  onRetry,
  autoLoadEnabled,
  onAutoLoadChange,
  className,
}: InlineModelLoadingBlockProps) {
  const isError = modelStatus === 'error';
  const isChecking = modelStatus === 'checking';
  const isUnloading = modelStatus === 'unloading';

  // Determine display state
  const showLoading = isLoading || isChecking || isUnloading;
  const showProgress = isLoading && progress !== undefined && progress > 0 && progress < 100;

  // Status message based on state
  const getStatusMessage = (): string => {
    if (isError) return 'Model failed to load';
    if (isUnloading) return 'Model unloading...';
    if (isLoading) return 'Loading model...';
    if (isChecking) return 'Checking model status...';
    return 'Model not loaded';
  };

  // Secondary info line
  const getSecondaryInfo = (): string | null => {
    const parts: string[] = [];
    if (modelName) parts.push(modelName);
    if (backendInfo) parts.push(`via ${backendInfo}`);
    return parts.length > 0 ? parts.join(' ') : null;
  };

  const secondaryInfo = getSecondaryInfo();

  return (
    <div
      className={cn(
        'mb-3 p-3 rounded-lg border transition-colors',
        isError ? 'bg-destructive/10 border-destructive/30' : 'bg-muted/50 border-border',
        className
      )}
      role="status"
      aria-live="polite"
      data-testid="inline-model-loading-block"
    >
      <div className="flex items-center justify-between gap-4">
        {/* Left: Status */}
        <div className="flex items-center gap-3 min-w-0">
          {showLoading ? (
            <Loader2
              className="h-5 w-5 animate-spin text-muted-foreground flex-shrink-0"
              aria-hidden="true"
            />
          ) : isError ? (
            <AlertTriangle
              className="h-5 w-5 text-destructive flex-shrink-0"
              aria-hidden="true"
            />
          ) : (
            <Server
              className="h-5 w-5 text-muted-foreground flex-shrink-0"
              aria-hidden="true"
            />
          )}

          <div className="min-w-0">
            <p className="text-sm font-medium">{getStatusMessage()}</p>
            {secondaryInfo && (
              <p className="text-xs text-muted-foreground truncate">{secondaryInfo}</p>
            )}
            {isError && errorMessage && (
              <p className="text-xs text-destructive mt-1 line-clamp-2">{errorMessage}</p>
            )}
          </div>
        </div>

        {/* Right: Actions */}
        <div className="flex items-center gap-3 flex-shrink-0">
          {/* Auto-load checkbox (only when not loading/error) */}
          {!showLoading && !isError && (
            <div className="flex items-center gap-2">
              <Checkbox
                id="auto-load-chat-model"
                checked={autoLoadEnabled}
                onCheckedChange={(checked) => onAutoLoadChange(checked === true)}
                aria-label="Auto-load model next time"
              />
              <label
                htmlFor="auto-load-chat-model"
                className="text-xs text-muted-foreground cursor-pointer whitespace-nowrap"
              >
                Auto-load next time
              </label>
            </div>
          )}

          {/* Action button */}
          {isError ? (
            <Button
              variant="outline"
              size="sm"
              onClick={onRetry ?? onLoadModel}
              className="gap-1.5"
              data-testid="retry-load-button"
            >
              <RefreshCw className="h-3.5 w-3.5" aria-hidden="true" />
              Retry
            </Button>
          ) : !showLoading ? (
            <Button
              variant="default"
              size="sm"
              onClick={onLoadModel}
              data-testid="load-model-button"
            >
              Load model
            </Button>
          ) : null}
        </div>
      </div>

      {/* Progress bar */}
      {showProgress && (
        <div className="mt-3">
          <Progress value={progress} className="h-1.5" aria-label="Model loading progress" />
          <p className="text-xs text-muted-foreground mt-1">{progress}% complete</p>
        </div>
      )}
    </div>
  );
}

export default InlineModelLoadingBlock;
