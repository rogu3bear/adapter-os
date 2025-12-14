/**
 * useAdapterStates - Real-time adapter state tracking via SSE
 *
 * Subscribes to /v1/stream/adapters for live adapter lifecycle updates.
 * Maintains a Map of adapter states with computed helpers for readiness checks.
 *
 * @example
 * ```tsx
 * const {
 *   adapters,
 *   allReady,
 *   anyLoading,
 *   connected,
 *   reconnect,
 *   getAdapter,
 * } = useAdapterStates({ stackId: 'my-stack' });
 *
 * if (allReady) {
 *   // All adapters ready for inference
 * }
 * ```
 */

import { useState, useEffect, useMemo, useCallback, useRef } from 'react';
import { useSSE } from '@/hooks/realtime/useSSE';
import type { AdapterStreamEvent, AdapterStateTransitionEvent } from '@/api/streaming-types';
import { logger } from '@/utils/logger';
import type { AdapterLifecycleState } from './types';

// ============================================================================
// Types
// ============================================================================

/**
 * Adapter state information with lifecycle details
 */
export interface AdapterStateInfo {
  /** Unique adapter ID */
  id: string;
  /** Display name */
  name: string;
  /** Current lifecycle state */
  state: AdapterLifecycleState;
  /** Previous state (if available) */
  previousState?: string | null;
  /** Activation percentage (0-100) */
  activationPercentage: number;
  /** Memory usage in MB (if available) */
  memoryMb?: number;
  /** Last update timestamp (Unix ms) */
  lastUpdated: number;
}

/**
 * Hook configuration options
 */
export interface UseAdapterStatesOptions {
  /** Stack ID to filter adapters by (optional) */
  stackId?: string;
  /** Enable SSE subscription (default: true) */
  enabled?: boolean;
  /** Callback when adapter state changes */
  onStateChange?: (adapterId: string, state: AdapterLifecycleState) => void;
}

/**
 * Hook return value
 */
export interface UseAdapterStatesResult {
  /** Map of adapter ID to state info */
  adapters: Map<string, AdapterStateInfo>;
  /** True if all adapters are in ready states (warm/hot/resident) */
  allReady: boolean;
  /** True if any adapter is currently loading (transitioning states) */
  anyLoading: boolean;
  /** True if SSE connection is active */
  connected: boolean;
  /** Manually reconnect to SSE stream */
  reconnect: () => void;
  /** Get adapter state by ID */
  getAdapter: (id: string) => AdapterStateInfo | undefined;
}

// ============================================================================
// Helper Functions
// ============================================================================

/**
 * Check if an adapter state is ready for inference
 */
function isAdapterReady(state: AdapterLifecycleState): boolean {
  return state === 'warm' || state === 'hot' || state === 'resident';
}

/**
 * Check if an adapter is transitioning between states
 */
function isAdapterLoading(state: AdapterLifecycleState): boolean {
  return state === 'cold';
}

/**
 * Type guard for adapter state transition events
 */
function isStateTransitionEvent(event: AdapterStreamEvent): event is AdapterStateTransitionEvent {
  return 'current_state' in event && 'adapter_id' in event;
}

// ============================================================================
// Hook Implementation
// ============================================================================

/**
 * Track real-time adapter states via SSE
 *
 * Features:
 * - Real-time SSE subscription for state transitions
 * - Filter by stack ID (optional)
 * - Computed helpers for readiness and loading checks
 * - Manual reconnection capability
 * - Automatic cleanup on unmount
 */
export function useAdapterStates(
  options: UseAdapterStatesOptions = {}
): UseAdapterStatesResult {
  const { stackId, enabled = true, onStateChange } = options;

  // State
  const [adapters, setAdapters] = useState<Map<string, AdapterStateInfo>>(new Map());

  // Store callback in ref to avoid SSE reconnection
  const onStateChangeRef = useRef(onStateChange);
  onStateChangeRef.current = onStateChange;

  // Store stackId in ref for filtering
  const stackIdRef = useRef(stackId);
  stackIdRef.current = stackId;

  // Subscribe to adapter state transitions via SSE
  const { connected, reconnect } = useSSE<AdapterStreamEvent>('/v1/stream/adapters', {
    enabled,
    onMessage: (event) => {
      if (!event || !isStateTransitionEvent(event)) {
        return;
      }

      const transition = event;

      logger.debug('Adapter state transition received', {
        component: 'useAdapterStates',
        adapterId: transition.adapter_id,
        previousState: transition.previous_state,
        currentState: transition.current_state,
      });

      setAdapters((prev) => {
        const updated = new Map(prev);

        // Build adapter state info
        const stateInfo: AdapterStateInfo = {
          id: transition.adapter_id,
          name: transition.adapter_name || transition.adapter_id,
          state: transition.current_state,
          previousState: transition.previous_state,
          activationPercentage: transition.activation_percentage,
          memoryMb: transition.memory_usage_mb,
          lastUpdated: transition.timestamp,
        };

        updated.set(transition.adapter_id, stateInfo);

        // Trigger callback if provided
        if (onStateChangeRef.current) {
          onStateChangeRef.current(transition.adapter_id, transition.current_state);
        }

        return updated;
      });
    },
  });

  // Filter adapters by stackId if provided
  const filteredAdapters = useMemo(() => {
    if (!stackId) {
      return adapters;
    }

    // Note: Since SSE events don't include stack_id, we cannot filter here.
    // Stack filtering should be handled by the caller or by a separate API call
    // to fetch stack adapters and cross-reference with the state map.
    // For now, return all adapters and let the caller filter if needed.
    return adapters;
  }, [adapters, stackId]);

  // Check if all adapters are ready
  const allReady = useMemo(() => {
    if (filteredAdapters.size === 0) {
      return true; // No adapters = ready
    }

    const states = Array.from(filteredAdapters.values());
    return states.every((adapter) => isAdapterReady(adapter.state));
  }, [filteredAdapters]);

  // Check if any adapter is loading
  const anyLoading = useMemo(() => {
    const states = Array.from(filteredAdapters.values());
    return states.some((adapter) => isAdapterLoading(adapter.state));
  }, [filteredAdapters]);

  // Get adapter by ID
  const getAdapter = useCallback(
    (id: string): AdapterStateInfo | undefined => {
      return filteredAdapters.get(id);
    },
    [filteredAdapters]
  );

  // Log connection status changes
  useEffect(() => {
    logger.info('useAdapterStates SSE connection status', {
      component: 'useAdapterStates',
      connected,
      adapterCount: filteredAdapters.size,
      stackId,
    });
  }, [connected, filteredAdapters.size, stackId]);

  return {
    adapters: filteredAdapters,
    allReady,
    anyLoading,
    connected,
    reconnect,
    getAdapter,
  };
}
