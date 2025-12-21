/**
 * MissingPinnedAdaptersBanner - Session-level banner for unavailable pinned adapters
 *
 * Displays a prominent warning when pinned adapters are unavailable during a chat session.
 * Supports two fallback modes:
 * - 'stack_only': All pinned adapters unavailable, using stack routing only
 * - 'partial': Some pinned adapters unavailable, using partial routing
 *
 * Citation: [2025-12-02†pinned-adapters] Pinned adapter unavailability handling
 */

import React from 'react';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { AlertTriangle, X } from 'lucide-react';
import { cn } from '@/lib/utils';

// ============================================================================
// Types
// ============================================================================

export interface MissingPinnedAdaptersBannerProps {
  /** List of unavailable pinned adapter IDs */
  unavailablePinnedAdapters: string[];
  /** Fallback routing mode when adapters are unavailable */
  pinnedRoutingFallback?: 'stack_only' | 'partial';
  /** Callback when user dismisses the banner */
  onDismiss?: () => void;
  /** Additional CSS classes */
  className?: string;
}

// ============================================================================
// Helper Functions
// ============================================================================

/**
 * Get warning message based on fallback mode
 */
function getFallbackMessage(
  fallback: 'stack_only' | 'partial' | undefined,
  adapterCount: number
): string {
  if (fallback === 'stack_only') {
    return 'All pinned adapters for this session are unavailable. Using stack-only routing.';
  }

  if (fallback === 'partial') {
    return 'Some pinned adapters for this session are unavailable.';
  }

  // Default message if fallback mode not specified
  return adapterCount > 1
    ? `${adapterCount} pinned adapters are unavailable for this session.`
    : 'A pinned adapter is unavailable for this session.';
}

/**
 * Format adapter list for display
 */
function formatAdapterList(adapters: string[]): string {
  if (adapters.length === 0) return '';
  if (adapters.length === 1) return adapters[0];
  if (adapters.length === 2) return adapters.join(' and ');

  const lastAdapter = adapters[adapters.length - 1];
  const otherAdapters = adapters.slice(0, -1).join(', ');
  return `${otherAdapters}, and ${lastAdapter}`;
}

// ============================================================================
// Main Component
// ============================================================================

/**
 * MissingPinnedAdaptersBanner - Displays warning for unavailable pinned adapters
 *
 * This banner appears at the top of the chat area when pinned adapters specified
 * for a session are unavailable. It provides clear feedback about the fallback
 * routing strategy being used.
 *
 * The banner is dismissible but will reappear on new affected messages to ensure
 * the user remains aware of the degraded routing state.
 */
export function MissingPinnedAdaptersBanner({
  unavailablePinnedAdapters,
  pinnedRoutingFallback,
  onDismiss,
  className,
}: MissingPinnedAdaptersBannerProps) {
  // Don't render if no unavailable adapters
  if (!unavailablePinnedAdapters || unavailablePinnedAdapters.length === 0) {
    return null;
  }

  const message = getFallbackMessage(pinnedRoutingFallback, unavailablePinnedAdapters.length);
  const adapterList = formatAdapterList(unavailablePinnedAdapters);

  return (
    <Alert
      variant="default"
      className={cn(
        'border-orange-500/50 bg-orange-50/50 text-orange-900',
        'dark:border-orange-500/30 dark:bg-orange-950/20 dark:text-orange-100',
        className
      )}
    >
      <AlertTriangle className="h-4 w-4 text-orange-600 dark:text-orange-400" />
      <AlertTitle className="flex items-center justify-between">
        <span className="font-semibold">Pinned Adapters Unavailable</span>
        {onDismiss && (
          <Button
            variant="ghost"
            size="icon-xs"
            onClick={onDismiss}
            className="h-5 w-5 hover:bg-orange-100 dark:hover:bg-orange-900/50"
            aria-label="Dismiss warning"
          >
            <X className="h-3 w-3" />
          </Button>
        )}
      </AlertTitle>
      <AlertDescription>
        <p className="text-sm">{message}</p>
        {unavailablePinnedAdapters.length > 0 && (
          <p className="text-xs text-orange-700 dark:text-orange-300 mt-2">
            <strong>Unavailable:</strong>{' '}
            <code className="bg-orange-100 dark:bg-orange-900/50 px-1.5 py-0.5 rounded text-xs">
              {adapterList}
            </code>
          </p>
        )}
        {pinnedRoutingFallback === 'stack_only' && (
          <p className="text-xs text-orange-700 dark:text-orange-300 mt-2">
            The system will route requests using only the adapters from your configured stack until
            the pinned adapters become available.
          </p>
        )}
        {pinnedRoutingFallback === 'partial' && (
          <p className="text-xs text-orange-700 dark:text-orange-300 mt-2">
            The system will route requests using available pinned adapters and stack adapters as
            fallback.
          </p>
        )}
      </AlertDescription>
    </Alert>
  );
}

export default MissingPinnedAdaptersBanner;
