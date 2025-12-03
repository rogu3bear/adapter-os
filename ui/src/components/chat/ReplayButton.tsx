import { useState, useEffect, useCallback } from 'react';
import { Loader2, RefreshCw, AlertTriangle, CheckCircle2, XCircle, AlertCircle } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Tooltip, TooltipContent, TooltipTrigger, TooltipProvider } from '@/components/ui/tooltip';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import { useToast } from '@/hooks/use-toast';
import type {
  ReplayAvailabilityResponse,
  ReplayResponse,
  ReplayStatus,
} from '@/api/replay-types';
import { getReplayStatusColor, getReplayStatusLabel } from '@/api/replay-types';
import { apiClient } from '@/api/client';
import { logger } from '@/utils/logger';

interface ReplayButtonProps {
  inferenceId: string;
  onReplayComplete?: (response: ReplayResponse) => void;
  className?: string;
}

/**
 * ReplayButton Component
 *
 * Provides UI for checking replay availability and executing deterministic replay
 * of an inference operation. Shows different states based on availability:
 *
 * - available: green dot, immediate execution
 * - approximate: yellow dot, confirmation dialog required
 * - degraded: orange dot, confirmation dialog with warnings
 * - unavailable: gray dot, disabled with tooltip explaining why
 *
 * Based on PRD-02 Deterministic Replay feature.
 *
 * @example
 * ```tsx
 * <ReplayButton
 *   inferenceId="chatcmpl-abc123"
 *   onReplayComplete={(result) => console.log('Replay completed', result)}
 * />
 * ```
 */
export function ReplayButton({
  inferenceId,
  onReplayComplete,
  className,
}: ReplayButtonProps) {
  const [availability, setAvailability] = useState<ReplayAvailabilityResponse | null>(null);
  const [isCheckingAvailability, setIsCheckingAvailability] = useState(false);
  const [isReplaying, setIsReplaying] = useState(false);
  const [showConfirmDialog, setShowConfirmDialog] = useState(false);
  const { toast } = useToast();

  /**
   * Check replay availability on mount
   */
  const checkAvailability = useCallback(async () => {
    setIsCheckingAvailability(true);

    try {
      logger.debug('Checking replay availability', {
        component: 'ReplayButton',
        inference_id: inferenceId,
      });

      // Note: This endpoint may not be fully implemented yet.
      // The backend has the handler in replay_inference.rs but it returns
      // a placeholder response until the DB schema is ready.
      const response = await fetch(`/api/v1/replay/check/${inferenceId}`, {
        method: 'GET',
        headers: {
          'Content-Type': 'application/json',
          // Auth handled by API interceptors
        },
      });

      if (!response.ok) {
        throw new Error(`Failed to check replay availability: ${response.statusText}`);
      }

      const data: ReplayAvailabilityResponse = await response.json();
      setAvailability(data);

      logger.debug('Replay availability checked', {
        component: 'ReplayButton',
        inference_id: inferenceId,
        status: data.status,
        can_replay_exact: data.can_replay_exact,
        can_replay_approximate: data.can_replay_approximate,
      });
    } catch (error) {
      logger.error('Failed to check replay availability', {
        component: 'ReplayButton',
        inference_id: inferenceId,
        error: error instanceof Error ? error.message : String(error),
      });

      // Set unavailable status on error
      setAvailability({
        inference_id: inferenceId,
        status: 'unavailable',
        can_replay_exact: false,
        can_replay_approximate: false,
        unavailable_reasons: [
          error instanceof Error ? error.message : 'Unknown error occurred',
        ],
        approximation_warnings: [],
        replay_key: undefined,
      });
    } finally {
      setIsCheckingAvailability(false);
    }
  }, [inferenceId]);

  useEffect(() => {
    checkAvailability();
  }, [checkAvailability]);

  /**
   * Execute replay
   */
  const executeReplay = useCallback(async (allowApproximate = false) => {
    setIsReplaying(true);
    setShowConfirmDialog(false);

    try {
      logger.info('Executing replay', {
        component: 'ReplayButton',
        inference_id: inferenceId,
        allow_approximate: allowApproximate,
      });

      const response = await fetch('/api/v1/replay', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          inference_id: inferenceId,
          allow_approximate: allowApproximate,
        }),
      });

      if (!response.ok) {
        throw new Error(`Replay failed: ${response.statusText}`);
      }

      const result: ReplayResponse = await response.json();

      logger.info('Replay completed', {
        component: 'ReplayButton',
        inference_id: inferenceId,
        replay_id: result.replay_id,
        match_status: result.match_status,
      });

      toast({
        title: 'Replay Completed',
        description: `Match status: ${result.match_status}`,
        variant: result.match_status === 'exact' ? 'default' : 'default',
      });

      onReplayComplete?.(result);
    } catch (error) {
      logger.error('Replay execution failed', {
        component: 'ReplayButton',
        inference_id: inferenceId,
        error: error instanceof Error ? error.message : String(error),
      });

      toast({
        title: 'Replay Failed',
        description: error instanceof Error ? error.message : 'Unknown error occurred',
        variant: 'destructive',
      });
    } finally {
      setIsReplaying(false);
    }
  }, [inferenceId, onReplayComplete, toast]);

  /**
   * Handle button click
   */
  const handleClick = useCallback(() => {
    if (!availability) return;

    const { status } = availability;

    if (status === 'available') {
      // Exact replay available - execute immediately
      executeReplay(false);
    } else if (status === 'approximate' || status === 'degraded') {
      // Approximate/degraded replay - show confirmation
      setShowConfirmDialog(true);
    }
    // unavailable: button is disabled, nothing to do
  }, [availability, executeReplay]);

  /**
   * Get status indicator
   */
  const getStatusIndicator = (status: ReplayStatus) => {
    switch (status) {
      case 'available':
        return <CheckCircle2 className="h-3 w-3 text-green-600" />;
      case 'approximate':
        return <AlertCircle className="h-3 w-3 text-yellow-600" />;
      case 'degraded':
        return <AlertTriangle className="h-3 w-3 text-orange-600" />;
      case 'unavailable':
        return <XCircle className="h-3 w-3 text-gray-400" />;
      default:
        return null;
    }
  };

  /**
   * Get tooltip content
   */
  const getTooltipContent = () => {
    if (!availability) {
      return 'Checking replay availability...';
    }

    const { status, unavailable_reasons, approximation_warnings } = availability;

    if (status === 'unavailable') {
      return (
        <div className="space-y-1">
          <div className="font-semibold">Replay Unavailable</div>
          {unavailable_reasons.length > 0 && (
            <ul className="text-xs list-disc pl-4 space-y-0.5">
              {unavailable_reasons.map((reason, idx) => (
                <li key={idx}>{reason}</li>
              ))}
            </ul>
          )}
        </div>
      );
    }

    if (status === 'approximate' || status === 'degraded') {
      return (
        <div className="space-y-1">
          <div className="font-semibold">{getReplayStatusLabel(status)} Replay</div>
          {approximation_warnings.length > 0 && (
            <ul className="text-xs list-disc pl-4 space-y-0.5">
              {approximation_warnings.map((warning, idx) => (
                <li key={idx}>{warning}</li>
              ))}
            </ul>
          )}
          <div className="text-xs italic mt-1">Click to replay with confirmation</div>
        </div>
      );
    }

    return 'Exact replay available - click to execute';
  };

  const isDisabled = !availability || availability.status === 'unavailable' || isReplaying;

  return (
    <TooltipProvider>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            variant="ghost"
            size="icon-sm"
            onClick={handleClick}
            disabled={isDisabled}
            className={className}
            aria-label="Replay inference"
          >
            {isCheckingAvailability || isReplaying ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <div className="relative">
                <RefreshCw className="h-4 w-4" />
                {availability && (
                  <div className="absolute -top-1 -right-1">
                    {getStatusIndicator(availability.status)}
                  </div>
                )}
              </div>
            )}
          </Button>
        </TooltipTrigger>
        <TooltipContent side="top">
          {getTooltipContent()}
        </TooltipContent>
      </Tooltip>

      {/* Confirmation dialog for approximate/degraded replay */}
      <AlertDialog open={showConfirmDialog} onOpenChange={setShowConfirmDialog}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {availability?.status === 'degraded' ? 'Degraded' : 'Approximate'} Replay
            </AlertDialogTitle>
            <AlertDialogDescription asChild>
              <div className="space-y-3">
                <p>
                  This inference cannot be replayed exactly due to changed conditions.
                  Replay will proceed in {availability?.status} mode.
                </p>

                {availability && availability.approximation_warnings.length > 0 && (
                  <div className="bg-yellow-50 dark:bg-yellow-950/20 border border-yellow-200 dark:border-yellow-800 rounded-md p-3">
                    <div className="flex items-start gap-2">
                      <AlertTriangle className="h-4 w-4 text-yellow-600 dark:text-yellow-400 mt-0.5" />
                      <div className="space-y-1 flex-1">
                        <div className="font-semibold text-sm text-yellow-900 dark:text-yellow-100">
                          Warnings:
                        </div>
                        <ul className="text-sm text-yellow-800 dark:text-yellow-200 list-disc pl-4 space-y-0.5">
                          {availability.approximation_warnings.map((warning, idx) => (
                            <li key={idx}>{warning}</li>
                          ))}
                        </ul>
                      </div>
                    </div>
                  </div>
                )}

                <p className="text-sm text-muted-foreground">
                  Results may differ from the original inference. Do you want to continue?
                </p>
              </div>
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={() => executeReplay(true)}>
              Continue Replay
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </TooltipProvider>
  );
}
