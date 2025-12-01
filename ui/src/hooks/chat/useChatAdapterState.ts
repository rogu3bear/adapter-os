/**
 * useChatAdapterState - Track adapter readiness for chat sessions
 *
 * Monitors adapter lifecycle states via SSE and provides utilities
 * for ensuring adapters are ready before chat inference.
 *
 * @example
 * ```tsx
 * const {
 *   adapterStates,
 *   allAdaptersReady,
 *   unreadyAdapters,
 *   loadAllAdapters,
 *   showAdapterPrompt,
 *   dismissAdapterPrompt,
 * } = useChatAdapterState({
 *   stackId: 'my-stack-id',
 *   enabled: true,
 *   onAdapterStateChange: (adapterId, state) => {
 *     console.log(`Adapter ${adapterId} is now ${state}`);
 *   },
 * });
 *
 * // Before sending chat message
 * if (!allAdaptersReady) {
 *   await loadAllAdapters();
 * }
 * ```
 */

import { useState, useEffect, useMemo, useCallback, useRef } from 'react';
import { useSSE } from '@/hooks/useSSE';
import { useAdapterStacks } from '@/hooks/useAdmin';
import apiClient from '@/api/client';
import { logger, toError } from '@/utils/logger';
import type { AdapterStreamEvent, AdapterStateTransitionEvent } from '@/api/streaming-types';

// ============================================================================
// Types
// ============================================================================

/**
 * Adapter lifecycle state
 */
export type AdapterLifecycleState = 'unloaded' | 'cold' | 'warm' | 'hot' | 'resident';

/**
 * Adapter readiness state for chat
 */
export interface AdapterReadinessState {
  /** Unique adapter ID */
  adapterId: string;
  /** Display name */
  name: string;
  /** Current lifecycle state */
  state: AdapterLifecycleState;
  /** Loading in progress */
  isLoading: boolean;
  /** Error message if load failed */
  error?: string;
  /** Memory usage in MB (if available) */
  memoryMb?: number;
}

/**
 * Hook configuration options
 */
export interface UseChatAdapterStateOptions {
  /** Stack ID to monitor (undefined = use default stack) */
  stackId?: string;
  /** Enable state tracking (default: true) */
  enabled?: boolean;
  /** Callback when an adapter state changes */
  onAdapterStateChange?: (adapterId: string, state: AdapterLifecycleState) => void;
}

/**
 * Hook return value
 */
export interface UseChatAdapterStateReturn {
  // State
  /** Map of adapter ID to readiness state */
  adapterStates: Map<string, AdapterReadinessState>;
  /** True if checking adapter readiness */
  isCheckingAdapters: boolean;
  /** True if all adapters are in ready states (warm/hot/resident) */
  allAdaptersReady: boolean;
  /** List of adapter IDs that are not ready */
  unreadyAdapters: string[];
  /** True if SSE connection is active */
  sseConnected: boolean;

  // Actions
  /** Load all unready adapters to warm state */
  loadAllAdapters: () => Promise<void>;
  /** Check if all adapters are ready (returns boolean) */
  checkAdapterReadiness: () => boolean;

  // Pre-chat prompt
  /** Show adapter readiness prompt before first message */
  showAdapterPrompt: boolean;
  /** Dismiss the prompt without loading */
  dismissAdapterPrompt: () => void;
  /** Continue with unready adapters (dismisses prompt) */
  continueWithUnready: () => void;
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
 * Type guard for adapter state transition events
 */
function isStateTransitionEvent(event: AdapterStreamEvent): event is AdapterStateTransitionEvent {
  return 'current_state' in event && 'adapter_id' in event;
}

// ============================================================================
// Hook Implementation
// ============================================================================

/**
 * Track adapter readiness state for chat sessions
 *
 * Features:
 * - Real-time SSE subscription for state transitions
 * - Bulk adapter loading capability
 * - Pre-chat readiness checks
 * - Automatic state synchronization with backend
 */
export function useChatAdapterState(
  options: UseChatAdapterStateOptions = {}
): UseChatAdapterStateReturn {
  const { stackId, enabled = true, onAdapterStateChange } = options;

  // State
  const [adapterStates, setAdapterStates] = useState<Map<string, AdapterReadinessState>>(new Map());
  const [isCheckingAdapters, setIsCheckingAdapters] = useState(false);
  const [showAdapterPrompt, setShowAdapterPrompt] = useState(false);
  const [userDismissedPrompt, setUserDismissedPrompt] = useState(false);

  // Fetch stack data
  const { data: stacks = [] } = useAdapterStacks();

  // Store callback in ref to avoid SSE reconnection
  const onAdapterStateChangeRef = useRef(onAdapterStateChange);
  onAdapterStateChangeRef.current = onAdapterStateChange;

  // Find the selected stack
  const selectedStack = useMemo(
    () => stacks.find((s) => s.id === stackId),
    [stacks, stackId]
  );

  // Subscribe to adapter state transitions via SSE
  const { connected: sseConnected } = useSSE<AdapterStreamEvent>('/v1/stream/adapters', {
    enabled: enabled && !!stackId,
    onMessage: (event) => {
      if (!event || !isStateTransitionEvent(event)) {
        return;
      }

      const transition = event;
      logger.debug('Adapter state transition received', {
        component: 'useChatAdapterState',
        adapterId: transition.adapter_id,
        previousState: transition.previous_state,
        currentState: transition.current_state,
      });

      setAdapterStates((prev) => {
        const updated = new Map(prev);
        const existing = updated.get(transition.adapter_id);

        if (existing) {
          // Update existing adapter state
          updated.set(transition.adapter_id, {
            ...existing,
            state: transition.current_state,
            isLoading: false,
            memoryMb: transition.memory_usage_mb,
          });

          // Trigger callback if provided
          if (onAdapterStateChangeRef.current) {
            onAdapterStateChangeRef.current(transition.adapter_id, transition.current_state);
          }
        } else {
          // Create new entry for unknown adapter (shouldn't happen, but be defensive)
          logger.warn('Received state transition for unknown adapter', {
            component: 'useChatAdapterState',
            adapterId: transition.adapter_id,
          });
          updated.set(transition.adapter_id, {
            adapterId: transition.adapter_id,
            name: transition.adapter_name || transition.adapter_id,
            state: transition.current_state,
            isLoading: false,
            memoryMb: transition.memory_usage_mb,
          });
        }

        return updated;
      });
    },
  });

  // Initialize adapter states from stack
  useEffect(() => {
    if (!selectedStack) {
      setAdapterStates(new Map());
      return;
    }

    const states = new Map<string, AdapterReadinessState>();

    // Build states from stack adapters
    if (selectedStack.adapters && selectedStack.adapters.length > 0) {
      selectedStack.adapters.forEach((adapter) => {
        const adapterId = adapter.id || adapter.adapter_id || '';
        states.set(adapterId, {
          adapterId,
          name: adapter.name || adapter.adapter_id || 'Unknown',
          state: (adapter.lifecycle_state as AdapterLifecycleState) || 'unloaded',
          isLoading: false,
        });
      });
    } else if (selectedStack.adapter_ids && selectedStack.adapter_ids.length > 0) {
      // Fallback to adapter_ids if adapters array not available
      selectedStack.adapter_ids.forEach((adapterId) => {
        states.set(adapterId, {
          adapterId,
          name: adapterId,
          state: 'unloaded', // Will be updated via SSE
          isLoading: false,
        });
      });
    }

    setAdapterStates(states);

    logger.info('Initialized adapter states from stack', {
      component: 'useChatAdapterState',
      stackId: selectedStack.id,
      adapterCount: states.size,
    });
  }, [selectedStack]);

  // Check if all adapters are ready
  const allAdaptersReady = useMemo(() => {
    if (adapterStates.size === 0) {
      return true; // No adapters = ready
    }

    const states = Array.from(adapterStates.values());
    return states.every((adapter) => isAdapterReady(adapter.state));
  }, [adapterStates]);

  // Get list of unready adapter IDs
  const unreadyAdapters = useMemo(() => {
    const states = Array.from(adapterStates.values());
    return states
      .filter((adapter) => !isAdapterReady(adapter.state))
      .map((adapter) => adapter.adapterId);
  }, [adapterStates]);

  // Show prompt when adapters are not ready (only if user hasn't dismissed)
  useEffect(() => {
    if (!userDismissedPrompt && !allAdaptersReady && adapterStates.size > 0) {
      setShowAdapterPrompt(true);
    } else {
      setShowAdapterPrompt(false);
    }
  }, [allAdaptersReady, adapterStates.size, userDismissedPrompt]);

  // Load all unready adapters
  const loadAllAdapters = useCallback(async () => {
    if (allAdaptersReady) {
      logger.debug('All adapters already ready, skipping load', {
        component: 'useChatAdapterState',
      });
      return;
    }

    setIsCheckingAdapters(true);

    try {
      const adaptersToLoad = Array.from(adapterStates.values()).filter(
        (adapter) => !isAdapterReady(adapter.state)
      );

      logger.info('Loading adapters for chat readiness', {
        component: 'useChatAdapterState',
        count: adaptersToLoad.length,
        adapterIds: adaptersToLoad.map((a) => a.adapterId),
      });

      // Set loading state for all adapters being loaded
      setAdapterStates((prev) => {
        const updated = new Map(prev);
        adaptersToLoad.forEach((adapter) => {
          const existing = updated.get(adapter.adapterId);
          if (existing) {
            updated.set(adapter.adapterId, { ...existing, isLoading: true, error: undefined });
          }
        });
        return updated;
      });

      // Load each adapter (sequentially to avoid overwhelming the system)
      for (const adapter of adaptersToLoad) {
        try {
          logger.debug('Loading adapter', {
            component: 'useChatAdapterState',
            adapterId: adapter.adapterId,
          });

          await apiClient.loadAdapter(adapter.adapterId);

          // Success - SSE will update the state, but we can optimistically update
          setAdapterStates((prev) => {
            const updated = new Map(prev);
            const existing = updated.get(adapter.adapterId);
            if (existing) {
              updated.set(adapter.adapterId, {
                ...existing,
                isLoading: false,
              });
            }
            return updated;
          });
        } catch (err) {
          const error = toError(err);
          logger.error('Failed to load adapter', {
            component: 'useChatAdapterState',
            adapterId: adapter.adapterId,
          }, error);

          setAdapterStates((prev) => {
            const updated = new Map(prev);
            const existing = updated.get(adapter.adapterId);
            if (existing) {
              updated.set(adapter.adapterId, {
                ...existing,
                isLoading: false,
                error: error.message || 'Failed to load',
              });
            }
            return updated;
          });
        }
      }

      // Dismiss the prompt after loading
      setShowAdapterPrompt(false);
      setUserDismissedPrompt(true);

      logger.info('Finished loading adapters', {
        component: 'useChatAdapterState',
        successful: adaptersToLoad.filter((a) => !adapterStates.get(a.adapterId)?.error).length,
        failed: adaptersToLoad.filter((a) => adapterStates.get(a.adapterId)?.error).length,
      });
    } finally {
      setIsCheckingAdapters(false);
    }
  }, [adapterStates, allAdaptersReady]);

  // Check adapter readiness (synchronous)
  const checkAdapterReadiness = useCallback(() => {
    return allAdaptersReady;
  }, [allAdaptersReady]);

  // Dismiss adapter prompt
  const dismissAdapterPrompt = useCallback(() => {
    setShowAdapterPrompt(false);
    setUserDismissedPrompt(true);
    logger.debug('User dismissed adapter readiness prompt', {
      component: 'useChatAdapterState',
    });
  }, []);

  // Continue with unready adapters
  const continueWithUnready = useCallback(() => {
    setShowAdapterPrompt(false);
    setUserDismissedPrompt(true);
    logger.warn('User chose to continue with unready adapters', {
      component: 'useChatAdapterState',
      unreadyCount: unreadyAdapters.length,
      unreadyAdapters,
    });
  }, [unreadyAdapters]);

  return {
    // State
    adapterStates,
    isCheckingAdapters,
    allAdaptersReady,
    unreadyAdapters,
    sseConnected,

    // Actions
    loadAllAdapters,
    checkAdapterReadiness,

    // Pre-chat prompt
    showAdapterPrompt,
    dismissAdapterPrompt,
    continueWithUnready,
  };
}
