/**
 * Lifecycle Badge Utilities
 *
 * Provides helper functions for color-coding lifecycle state badges
 * across adapter and stack views.
 *
 * Lifecycle states: draft, training, ready, active, deprecated, retired, failed
 *
 * Color mapping:
 * - draft → Outline (gray border, no fill)
 * - active → Default (green/brand color)
 * - deprecated → Secondary (yellow/orange warning)
 * - retired → Destructive (red danger)
 */

import type { LifecycleState } from '@/api/types';

/**
 * Get the badge variant for a lifecycle state
 *
 * @param state - The lifecycle state (draft, active, deprecated, retired)
 * @returns Badge variant for the given state
 */
export function getLifecycleVariant(
  state: LifecycleState | string | undefined
): 'default' | 'secondary' | 'destructive' | 'outline' {
  if (!state) return 'outline';

  switch (state.toLowerCase()) {
    case 'active':
      return 'default';       // Green/brand color (active production state)
    case 'ready':
      return 'default';       // Artifact verified, awaiting promotion
    case 'training':
      return 'secondary';     // In progress
    case 'draft':
      return 'outline';       // Gray border, no fill (in development)
    case 'deprecated':
      return 'secondary';     // Yellow/orange warning (phasing out)
    case 'retired':
    case 'failed':
      return 'destructive';   // Red danger (no longer in use or failed)
    default:
      return 'outline';       // Default to outline for unknown states
  }
}

/**
 * Get a human-readable description of a lifecycle state
 *
 * @param state - The lifecycle state
 * @returns Description of the lifecycle state
 */
export function getLifecycleDescription(state: LifecycleState | string | undefined): string {
  if (!state) return 'Unknown state';

  switch (state.toLowerCase()) {
    case 'active':
      return 'Active in production';
    case 'ready':
      return '.aos verified; awaiting promotion';
    case 'training':
      return 'Training in progress';
    case 'draft':
      return 'In development';
    case 'deprecated':
      return 'Being phased out';
    case 'retired':
      return 'No longer in use';
    case 'failed':
      return 'Training or validation failed';
    default:
      return `Unknown state: ${state}`;
  }
}

/**
 * Check if a lifecycle state is considered "healthy" (active or draft)
 *
 * @param state - The lifecycle state
 * @returns True if the state is active or draft
 */
export function isHealthyLifecycleState(state: LifecycleState | string | undefined): boolean {
  if (!state) return false;
  const normalized = state.toLowerCase();
  return normalized === 'active' || normalized === 'ready';
}
