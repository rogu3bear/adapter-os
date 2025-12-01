import { useState, useCallback } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import apiClient from '@/api/client';
import { logger, toError } from '@/utils/logger';
import type { ExtendedRouterDecision, SessionRouterViewResponse, RouterCandidateInfo } from '@/api/api-types';
import type { AdapterStack } from '@/api/adapter-types';

/**
 * Router decision data structure for chat messages
 */
export interface RouterDecision {
  messageId: string;
  adapterId: string;
  adapterName?: string;
  confidence: number;
  routingPath: string[];
  timestamp: Date;
}

/**
 * Options for the useChatRouterDecisions hook
 */
export interface UseChatRouterDecisionsOptions {
  /** Stack ID to use for adapter ID resolution */
  stackId?: string;
  /** Whether to enable decision fetching */
  enabled?: boolean;
}

/**
 * Return type for the useChatRouterDecisions hook
 */
export interface UseChatRouterDecisionsReturn {
  // State
  /** Map of message IDs to their router decisions */
  decisions: Map<string, RouterDecision>;
  /** Whether a decision is currently being loaded */
  isLoadingDecision: boolean;
  /** The most recently fetched decision */
  lastDecision: RouterDecision | null;

  // Actions
  /** Fetch a router decision for a message */
  fetchDecision: (messageId: string, requestId: string) => Promise<RouterDecision | null>;
  /** Clear all cached decisions */
  clearDecisions: () => void;

  // History
  /** Array of all decisions in chronological order */
  decisionHistory: RouterDecision[];
}

/**
 * Hook for fetching and tracking router decisions for chat messages.
 *
 * This hook manages the lifecycle of router decision data, including:
 * - Fetching decisions from the API with retry logic
 * - Caching decisions by message ID
 * - Resolving adapter indices to adapter IDs and names
 * - Maintaining a history of all decisions
 *
 * @example
 * ```tsx
 * function ChatComponent({ stackId }: { stackId: string }) {
 *   const {
 *     decisions,
 *     isLoadingDecision,
 *     fetchDecision,
 *     decisionHistory
 *   } = useChatRouterDecisions({ stackId });
 *
 *   const handleMessageSent = async (messageId: string, requestId: string) => {
 *     const decision = await fetchDecision(messageId, requestId);
 *     if (decision) {
 *       console.log(`Message routed to adapter: ${decision.adapterName}`);
 *     }
 *   };
 *
 *   return (
 *     <div>
 *       {decisionHistory.map(decision => (
 *         <div key={decision.messageId}>
 *           {decision.adapterName} - {decision.confidence}
 *         </div>
 *       ))}
 *     </div>
 *   );
 * }
 * ```
 */
export function useChatRouterDecisions(
  options: UseChatRouterDecisionsOptions = {}
): UseChatRouterDecisionsReturn {
  const { stackId, enabled = true } = options;
  const queryClient = useQueryClient();

  // State
  const [decisions, setDecisions] = useState<Map<string, RouterDecision>>(new Map());
  const [isLoadingDecision, setIsLoadingDecision] = useState(false);
  const [lastDecision, setLastDecision] = useState<RouterDecision | null>(null);

  /**
   * Fetch stack information with retry logic
   */
  const fetchStackWithRetry = useCallback(async (stackIdParam: string): Promise<AdapterStack | null> => {
    try {
      const stack = await queryClient.fetchQuery({
        queryKey: ['adapter-stack', stackIdParam],
        queryFn: async () => {
          const result = await apiClient.getAdapterStack(stackIdParam);
          if (!result.adapter_ids || result.adapter_ids.length === 0) {
            throw new Error('Stack has no adapter IDs');
          }
          return result;
        },
        staleTime: 60000, // Cache for 1 minute
        retry: 3,
        retryDelay: (attemptIndex) => Math.min(1000 * 2 ** attemptIndex, 4000),
      });
      return stack;
    } catch (err) {
      logger.error('Failed to fetch stack after retries', {
        component: 'useChatRouterDecisions',
        stackId: stackIdParam,
      }, toError(err));
      return null;
    }
  }, [queryClient]);

  /**
   * Resolve adapter name from adapter ID
   */
  const resolveAdapterName = useCallback(async (adapterId: string): Promise<string | undefined> => {
    try {
      const adapter = await queryClient.fetchQuery({
        queryKey: ['adapter', adapterId],
        queryFn: () => apiClient.getAdapter(adapterId),
        staleTime: 300000, // Cache for 5 minutes
        retry: 1,
      });
      return adapter.name || adapter.id;
    } catch (err) {
      logger.warn('Failed to resolve adapter name', {
        component: 'useChatRouterDecisions',
        adapterId,
        details: toError(err).message,
      });
      return undefined;
    }
  }, [queryClient]);

  /**
   * Convert SessionRouterViewResponse to RouterDecision format
   */
  const convertToRouterDecision = useCallback(async (
    messageId: string,
    routerView: SessionRouterViewResponse,
    adapterIdMap: Map<number, string>
  ): Promise<RouterDecision | null> => {
    if (!routerView.steps || routerView.steps.length === 0) {
      return null;
    }

    const firstStep = routerView.steps[0];

    // Find the primary selected adapter (highest confidence)
    const selectedAdapter = firstStep.adapters_fired
      .filter(a => a.selected)
      .sort((a, b) => b.gate_value - a.gate_value)[0];

    if (!selectedAdapter) {
      return null;
    }

    const adapterId = adapterIdMap.get(selectedAdapter.adapter_idx) || `adapter-${selectedAdapter.adapter_idx}`;
    const adapterName = await resolveAdapterName(adapterId);

    // Build routing path from all selected adapters
    const routingPath = firstStep.adapters_fired
      .filter(a => a.selected)
      .sort((a, b) => b.gate_value - a.gate_value)
      .map(a => {
        const id = adapterIdMap.get(a.adapter_idx) || `adapter-${a.adapter_idx}`;
        return id;
      });

    return {
      messageId,
      adapterId,
      adapterName,
      confidence: selectedAdapter.gate_value,
      routingPath,
      timestamp: new Date(firstStep.timestamp),
    };
  }, [resolveAdapterName]);

  /**
   * Fetch router decision for a message
   */
  const fetchDecision = useCallback(async (
    messageId: string,
    requestId: string
  ): Promise<RouterDecision | null> => {
    if (!enabled) {
      return null;
    }

    setIsLoadingDecision(true);

    try {
      // Use React Query to cache router decisions
      const routerView = await queryClient.fetchQuery({
        queryKey: ['router-decision', requestId],
        queryFn: () => apiClient.getSessionRouterView(requestId),
        staleTime: 30000, // Cache for 30 seconds
        retry: 1, // Only retry once for router decisions
      });

      // Map adapter indices to actual adapter IDs using stack
      let adapterIdMap = new Map<number, string>();

      if (routerView.stack_id || stackId) {
        const stackIdToUse = routerView.stack_id || stackId;
        if (stackIdToUse) {
          const stack = await fetchStackWithRetry(stackIdToUse);
          if (stack && stack.adapter_ids && stack.adapter_ids.length > 0) {
            stack.adapter_ids.forEach((adapterId, idx) => {
              adapterIdMap.set(idx, adapterId);
            });
          } else {
            logger.warn('Stack fetch returned empty or null', {
              component: 'useChatRouterDecisions',
              requestId,
              stackId: stackIdToUse,
            });
          }
        }
      }

      // Convert to RouterDecision format
      const decision = await convertToRouterDecision(messageId, routerView, adapterIdMap);

      if (decision) {
        // Update state
        setDecisions(prev => new Map(prev).set(messageId, decision));
        setLastDecision(decision);

        logger.debug('Router decision fetched', {
          component: 'useChatRouterDecisions',
          messageId,
          requestId,
          adapterId: decision.adapterId,
          confidence: decision.confidence,
        });
      }

      return decision;
    } catch (err) {
      logger.error('Failed to fetch router decision', {
        component: 'useChatRouterDecisions',
        requestId,
        messageId,
      }, toError(err));
      return null;
    } finally {
      setIsLoadingDecision(false);
    }
  }, [enabled, stackId, queryClient, fetchStackWithRetry, convertToRouterDecision]);

  /**
   * Clear all cached decisions
   */
  const clearDecisions = useCallback(() => {
    setDecisions(new Map());
    setLastDecision(null);
    logger.debug('Router decisions cleared', {
      component: 'useChatRouterDecisions',
    });
  }, []);

  /**
   * Get decision history in chronological order
   */
  const decisionHistory = Array.from(decisions.values()).sort(
    (a, b) => a.timestamp.getTime() - b.timestamp.getTime()
  );

  return {
    decisions,
    isLoadingDecision,
    lastDecision,
    fetchDecision,
    clearDecisions,
    decisionHistory,
  };
}
