/**
 * Real-time Data Hooks
 *
 * Unified real-time data fetching with SSE + polling fallback.
 * Consolidates duplicate implementations across the codebase.
 */

// Core realtime hooks
export {
  usePolling,
  type PollingConfig,
  type PollingSpeed as PollingSpeedType,
  type UsePollingReturn,
} from './usePolling';

export {
  useSSE,
  type UseSSEOptions,
} from './useSSE';

export {
  useLiveData,
  type UseLiveDataOptions,
  type UseLiveDataReturn,
  type ConnectionStatus,
  type DataFreshnessLevel,
  type PollingSpeed,
} from './useLiveData';

export {
  useActivityFeed,
  type ActivityEvent,
  type UseActivityFeedOptions,
  type UseActivityFeedReturn,
} from './useActivityFeed';

export {
  useActivityEvents,
  type UseActivityEventsOptions,
  type UseActivityEventsReturn,
} from './useActivityEvents';

// Unified SSE with polling fallback
export {
  useSSEWithPollingFallback,
  type UseSSEWithPollingFallbackOptions,
  type UseSSEWithPollingFallbackReturn,
  type ConnectionStatus as SSEConnectionStatus,
  type PollingSpeed as SSEPollingSpeed,
} from './useSSEWithPollingFallback';

// Additional realtime hooks
export { useCanonicalState } from './useCanonicalState';
export { useLiveDataStatus } from './useLiveDataStatus';
export { useNotifications } from './useNotifications';
export { useSessionExpiryHandler } from './useSessionExpiryHandler';
export { useSessionTelemetry } from './useSessionTelemetry';
export { useRouterEvents, type UseRouterEventsResult } from './useRouterEvents';
