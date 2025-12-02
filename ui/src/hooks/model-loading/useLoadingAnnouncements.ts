/**
 * useLoadingAnnouncements - Screen reader accessibility for model loading
 *
 * Generates appropriate announcements for screen readers based on loading state.
 * Announcements are debounced and only made at significant milestones to avoid spam.
 *
 * Features:
 * - Milestone-based announcements (0%, 50%, 90%, 100%)
 * - Error and partial failure announcements
 * - Debounced to prevent announcement spam
 * - Returns string for aria-live region
 *
 * @example
 * ```tsx
 * const { announcement } = useLoadingAnnouncements({
 *   isLoading: true,
 *   progress: 75,
 *   error: null,
 * });
 *
 * return (
 *   <div aria-live="polite" aria-atomic="true" className="sr-only">
 *     {announcement}
 *   </div>
 * );
 * ```
 *
 * Copyright JKCA | 2025 James KC Auchterlonie
 */

import { useState, useEffect, useRef } from 'react';

// ============================================================================
// Types
// ============================================================================

/**
 * Loading phase for state-specific announcements
 */
export type LoadingPhase = 'idle' | 'starting' | 'loading' | 'completing' | 'complete' | 'error';

/**
 * Loading state input for announcements
 */
export interface LoadingAnnouncementState {
  /** Loading operation in progress */
  isLoading: boolean;

  /** Loading progress (0-100) */
  progress?: number;

  /** Error message if load failed */
  error?: string | null;

  /** Current loading phase (optional, inferred from progress if not provided) */
  phase?: LoadingPhase;

  /** Additional context message */
  statusMessage?: string | null;

  /** Number of partial failures (e.g., some adapters failed) */
  partialFailureCount?: number;

  /** Total items being loaded (for context) */
  totalItems?: number;
}

/**
 * Hook configuration options
 */
export interface UseLoadingAnnouncementsOptions {
  /** Input loading state */
  state: LoadingAnnouncementState;

  /** Debounce delay in ms (default: 300) */
  debounceMs?: number;

  /** Enable announcements (default: true) */
  enabled?: boolean;

  /** Custom milestone thresholds (default: [0, 50, 90, 100]) */
  milestones?: number[];
}

/**
 * Hook return value
 */
export interface UseLoadingAnnouncementsResult {
  /** Current announcement string for aria-live region */
  announcement: string;

  /** Last milestone announced */
  lastMilestone: number | null;

  /** Whether an announcement is pending */
  isPending: boolean;
}

// ============================================================================
// Constants
// ============================================================================

/** Default milestone thresholds for announcements */
const DEFAULT_MILESTONES = [0, 50, 90, 100];

/** Debounce delay in milliseconds */
const DEFAULT_DEBOUNCE_MS = 300;

// ============================================================================
// Helper Functions
// ============================================================================

/**
 * Infer loading phase from progress if not explicitly provided
 */
function inferPhase(progress: number, isLoading: boolean, hasError: boolean): LoadingPhase {
  if (hasError) {
    return 'error';
  }

  if (!isLoading && progress >= 100) {
    return 'complete';
  }

  if (progress === 0 && isLoading) {
    return 'starting';
  }

  if (progress >= 90 && isLoading) {
    return 'completing';
  }

  if (isLoading) {
    return 'loading';
  }

  return 'idle';
}

/**
 * Find the nearest milestone for a given progress value
 */
function findNearestMilestone(progress: number, milestones: number[]): number | null {
  if (progress < 0 || progress > 100) {
    return null;
  }

  // Find the milestone that progress has just reached or passed
  for (let i = milestones.length - 1; i >= 0; i--) {
    if (progress >= milestones[i]) {
      return milestones[i];
    }
  }

  return null;
}

/**
 * Generate announcement text based on loading state
 */
function generateAnnouncement(state: LoadingAnnouncementState): string {
  const {
    isLoading,
    progress = 0,
    error,
    statusMessage,
    partialFailureCount = 0,
    totalItems,
  } = state;

  const hasError = !!error;
  const phase = state.phase || inferPhase(progress, isLoading, hasError);

  // Error announcements
  if (hasError) {
    return `Error: ${error}`;
  }

  // Partial failure announcement
  if (partialFailureCount > 0 && !isLoading) {
    const itemText = totalItems !== undefined ? ` out of ${totalItems}` : '';
    return `Loading complete with ${partialFailureCount} failure${partialFailureCount > 1 ? 's' : ''}${itemText}. Chat available with reduced functionality.`;
  }

  // Phase-based announcements
  switch (phase) {
    case 'starting':
      return statusMessage || 'Loading has started. Please wait.';

    case 'loading':
      if (progress >= 50 && progress < 90) {
        return statusMessage || `Loading is ${progress}% complete.`;
      }
      if (progress < 50) {
        return statusMessage || 'Loading in progress.';
      }
      return statusMessage || 'Loading in progress.';

    case 'completing':
      return statusMessage || 'Loading is almost complete.';

    case 'complete':
      return statusMessage || 'Loading complete. Chat is ready.';

    case 'idle':
    default:
      return '';
  }
}

// ============================================================================
// Hook Implementation
// ============================================================================

/**
 * Generate screen reader announcements for model loading
 *
 * Features:
 * - Debounced announcements to prevent spam
 * - Milestone-based updates (0%, 50%, 90%, 100%)
 * - Error and partial failure announcements
 * - Contextual messages based on loading phase
 *
 * Usage:
 * Place the returned announcement in an aria-live="polite" region
 * with sr-only styling for screen reader-only announcements.
 */
export function useLoadingAnnouncements(
  options: UseLoadingAnnouncementsOptions
): UseLoadingAnnouncementsResult {
  const {
    state,
    debounceMs = DEFAULT_DEBOUNCE_MS,
    enabled = true,
    milestones = DEFAULT_MILESTONES,
  } = options;

  // Current announcement text
  const [announcement, setAnnouncement] = useState('');

  // Last announced milestone
  const [lastMilestone, setLastMilestone] = useState<number | null>(null);

  // Pending announcement flag
  const [isPending, setIsPending] = useState(false);

  // Debounce timer ref
  const debounceTimerRef = useRef<NodeJS.Timeout | null>(null);

  // Previous state ref for comparison
  const previousStateRef = useRef<LoadingAnnouncementState>(state);

  // Effect to generate and debounce announcements
  useEffect(() => {
    if (!enabled) {
      return;
    }

    const currentProgress = state.progress ?? 0;
    const previousProgress = previousStateRef.current.progress ?? 0;
    const currentError = state.error;
    const previousError = previousStateRef.current.error;

    // Check if we've reached a new milestone
    const currentMilestone = findNearestMilestone(currentProgress, milestones);
    const shouldAnnounce =
      // New milestone reached
      (currentMilestone !== null && currentMilestone !== lastMilestone) ||
      // Error state changed
      (currentError && currentError !== previousError) ||
      // Partial failures announced when loading completes
      (state.partialFailureCount && state.partialFailureCount > 0 && !state.isLoading && previousStateRef.current.isLoading);

    if (shouldAnnounce) {
      setIsPending(true);

      // Clear existing timer
      if (debounceTimerRef.current) {
        clearTimeout(debounceTimerRef.current);
      }

      // Debounce announcement
      debounceTimerRef.current = setTimeout(() => {
        const newAnnouncement = generateAnnouncement(state);
        setAnnouncement(newAnnouncement);
        setIsPending(false);

        // Update last milestone
        if (currentMilestone !== null && !currentError) {
          setLastMilestone(currentMilestone);
        }
      }, debounceMs);
    }

    // Update previous state ref
    previousStateRef.current = state;

    // Cleanup timer on unmount
    return () => {
      if (debounceTimerRef.current) {
        clearTimeout(debounceTimerRef.current);
      }
    };
  }, [
    state,
    enabled,
    debounceMs,
    milestones,
    lastMilestone,
  ]);

  // Reset announcement when loading completes successfully
  useEffect(() => {
    if (!state.isLoading && !state.error && state.progress === 100) {
      // Clear announcement after a delay to let screen reader finish
      const clearTimer = setTimeout(() => {
        setAnnouncement('');
        setLastMilestone(null);
      }, 3000);

      return () => clearTimeout(clearTimer);
    }
  }, [state.isLoading, state.error, state.progress]);

  return {
    announcement,
    lastMilestone,
    isPending,
  };
}

export default useLoadingAnnouncements;
