/**
 * Specialized React hooks for SSE streaming endpoints
 *
 * Provides convenient type-safe hooks for each streaming endpoint with proper
 * memoization and cleanup. Each hook manages its own EventSource connection
 * and provides reconnection capabilities.
 *
 * Available Hooks:
 * - useTrainingStream() - Training job progress
 * - useDiscoveryStream() - Adapter discovery events
 * - useContactsStream() - Collaboration/contact events
 * - useFileChangesStream() - File system changes
 * - useMetricsStream() - System metrics (5-sec interval)
 * - useTelemetryStream() - Telemetry events
 * - useAdaptersStream() - Adapter lifecycle transitions
 *
 * Each hook returns: { data, error, connected, reconnect, lastUpdated }
 */

import { useCallback, useMemo } from 'react';
import { useSSE } from './useSSE';
import { UseSSEOptions } from './useSSE';
import {
  TrainingStreamEvent,
  DiscoveryStreamEvent,
  ContactStreamEvent,
  FileChangeStreamEvent,
  MetricsStreamEvent,
  TelemetryStreamEvent,
  AdapterStreamEvent,
  StackPolicyStreamEvent,
} from '../api/streaming-types';

/**
 * Extended SSE hook result with additional metadata
 */
export interface StreamHookResult<T> {
  data: T | null;
  error: string | null;
  connected: boolean;
  reconnect: () => void;
  lastUpdated?: string;
}

/**
 * Base hook for training stream events
 * Endpoint: /v1/streams/training
 *
 * Usage:
 * ```tsx
 * const { data, error, connected } = useTrainingStream({
 *   onMessage: (event) => console.log('Training progress:', event),
 *   onError: (err) => console.error('Stream error:', err),
 * });
 * ```
 */
export function useTrainingStream(
  options: UseSSEOptions<TrainingStreamEvent> = {}
): StreamHookResult<TrainingStreamEvent> {
  const memoizedOptions = useMemo(() => options, [options.enabled, options.onError, options.onMessage]);
  const { data, error, connected, reconnect } = useSSE<TrainingStreamEvent>(
    '/v1/streams/training',
    memoizedOptions
  );

  return useMemo(
    () => ({
      data,
      error,
      connected,
      reconnect,
      lastUpdated: data?.timestamp,
    }),
    [data, error, connected, reconnect]
  );
}

/**
 * Base hook for adapter discovery stream events
 * Endpoint: /v1/streams/discovery
 *
 * Usage:
 * ```tsx
 * const { data, error, connected } = useDiscoveryStream({
 *   onMessage: (event) => console.log('Discovery event:', event),
 * });
 * ```
 */
export function useDiscoveryStream(
  options: UseSSEOptions<DiscoveryStreamEvent> = {}
): StreamHookResult<DiscoveryStreamEvent> {
  const memoizedOptions = useMemo(() => options, [options.enabled, options.onError, options.onMessage]);
  const { data, error, connected, reconnect } = useSSE<DiscoveryStreamEvent>(
    '/v1/streams/discovery',
    memoizedOptions
  );

  return useMemo(
    () => ({
      data,
      error,
      connected,
      reconnect,
      lastUpdated: data?.timestamp,
    }),
    [data, error, connected, reconnect]
  );
}

/**
 * Base hook for contact/collaboration stream events
 * Endpoint: /v1/streams/contacts
 *
 * Usage:
 * ```tsx
 * const { data, error, connected } = useContactsStream({
 *   onMessage: (event) => console.log('Contact event:', event),
 * });
 * ```
 */
export function useContactsStream(
  options: UseSSEOptions<ContactStreamEvent> = {}
): StreamHookResult<ContactStreamEvent> {
  const memoizedOptions = useMemo(() => options, [options.enabled, options.onError, options.onMessage]);
  const { data, error, connected, reconnect } = useSSE<ContactStreamEvent>(
    '/v1/streams/contacts',
    memoizedOptions
  );

  return useMemo(
    () => ({
      data,
      error,
      connected,
      reconnect,
      lastUpdated: data?.timestamp,
    }),
    [data, error, connected, reconnect]
  );
}

/**
 * Base hook for file change stream events
 * Endpoint: /v1/streams/file-changes
 *
 * Usage:
 * ```tsx
 * const { data, error, connected } = useFileChangesStream({
 *   onMessage: (event) => {
 *     if ('changes' in event) {
 *       console.log('Batch changes:', event.changes);
 *     } else {
 *       console.log('Single file change:', event);
 *     }
 *   },
 * });
 * ```
 */
export function useFileChangesStream(
  options: UseSSEOptions<FileChangeStreamEvent> = {}
): StreamHookResult<FileChangeStreamEvent> {
  const memoizedOptions = useMemo(() => options, [options.enabled, options.onError, options.onMessage]);
  const { data, error, connected, reconnect } = useSSE<FileChangeStreamEvent>(
    '/v1/streams/file-changes',
    memoizedOptions
  );

  return useMemo(
    () => ({
      data,
      error,
      connected,
      reconnect,
      lastUpdated: data?.timestamp,
    }),
    [data, error, connected, reconnect]
  );
}

/**
 * Base hook for system metrics stream events
 * Endpoint: /v1/stream/metrics (5-sec interval)
 *
 * Usage:
 * ```tsx
 * const { data, error, connected } = useMetricsStream({
 *   onMessage: (event) => {
 *     if ('system' in event) {
 *       console.log('CPU:', event.system.cpu_percent + '%');
 *       console.log('Memory:', event.system.memory_percent + '%');
 *     }
 *   },
 * });
 * ```
 */
export function useMetricsStream(
  options: UseSSEOptions<MetricsStreamEvent> = {}
): StreamHookResult<MetricsStreamEvent> {
  const memoizedOptions = useMemo(() => options, [options.enabled, options.onError, options.onMessage]);
  const { data, error, connected, reconnect } = useSSE<MetricsStreamEvent>(
    '/v1/stream/metrics',
    memoizedOptions
  );

  return useMemo(
    () => ({
      data,
      error,
      connected,
      reconnect,
      lastUpdated: data && 'timestamp' in data ? data.timestamp : undefined,
    }),
    [data, error, connected, reconnect]
  );
}

/**
 * Base hook for telemetry stream events
 * Endpoint: /v1/stream/telemetry
 *
 * Usage:
 * ```tsx
 * const { data, error, connected } = useTelemetryStream({
 *   onMessage: (event) => {
 *     console.log(`${event.action} - ${event.status}`, event.metadata);
 *   },
 * });
 * ```
 */
export function useTelemetryStream(
  options: UseSSEOptions<TelemetryStreamEvent> = {}
): StreamHookResult<TelemetryStreamEvent> {
  const memoizedOptions = useMemo(() => options, [options.enabled, options.onError, options.onMessage]);
  const { data, error, connected, reconnect } = useSSE<TelemetryStreamEvent>(
    '/v1/stream/telemetry',
    memoizedOptions
  );

  return useMemo(
    () => ({
      data,
      error,
      connected,
      reconnect,
      lastUpdated: data?.timestamp,
    }),
    [data, error, connected, reconnect]
  );
}

/**
 * Base hook for adapter lifecycle stream events
 * Endpoint: /v1/stream/adapters
 *
 * Usage:
 * ```tsx
 * const { data, error, connected } = useAdaptersStream({
 *   onMessage: (event) => {
 *     if ('previous_state' in event) {
 *       console.log(`Adapter state: ${event.previous_state} → ${event.new_state}`);
 *     }
 *   },
 * });
 * ```
 */
export function useAdaptersStream(
  options: UseSSEOptions<AdapterStreamEvent> = {}
): StreamHookResult<AdapterStreamEvent> {
  const memoizedOptions = useMemo(() => options, [options.enabled, options.onError, options.onMessage]);
  const { data, error, connected, reconnect } = useSSE<AdapterStreamEvent>(
    '/v1/stream/adapters',
    memoizedOptions
  );

  return useMemo(
    () => ({
      data,
      error,
      connected,
      reconnect,
      lastUpdated: data?.timestamp ? new Date(data.timestamp).toISOString() : undefined,
    }),
    [data, error, connected, reconnect]
  );
}

/**
 * Base hook for stack policy stream events (PRD-GOV-01)
 * Endpoint: /v1/stream/stack-policies/{stackId}
 *
 * Provides real-time updates for:
 * - Compliance score changes
 * - Policy violations detected
 * - Policy violations resolved
 * - Policy assignments/revocations
 *
 * Usage:
 * ```tsx
 * const { data, error, connected } = useStackPolicyStream('stack-123', {
 *   onMessage: (event) => {
 *     if (event.event_type === 'violation_detected') {
 *       toast.error(`Policy violation: ${event.message}`);
 *     }
 *   },
 * });
 * ```
 */
export function useStackPolicyStream(
  stackId: string,
  options: UseSSEOptions<StackPolicyStreamEvent> = {}
): StreamHookResult<StackPolicyStreamEvent> {
  const memoizedOptions = useMemo(() => options, [options.enabled, options.onError, options.onMessage]);
  const endpoint = stackId ? `/v1/stream/stack-policies/${encodeURIComponent(stackId)}` : '';

  const { data, error, connected, reconnect } = useSSE<StackPolicyStreamEvent>(
    endpoint,
    {
      ...memoizedOptions,
      enabled: memoizedOptions.enabled !== false && !!stackId,
    }
  );

  return useMemo(
    () => ({
      data,
      error,
      connected,
      reconnect,
      lastUpdated: data?.timestamp,
    }),
    [data, error, connected, reconnect]
  );
}

// ============================================================================
// Convenience Hooks with State Aggregation
// ============================================================================

/**
 * Hook to track all active stream connections
 * Useful for monitoring/debugging stream health
 */
export function useAllStreamsStatus() {
  const training = useTrainingStream({ enabled: false });
  const discovery = useDiscoveryStream({ enabled: false });
  const contacts = useContactsStream({ enabled: false });
  const fileChanges = useFileChangesStream({ enabled: false });
  const metrics = useMetricsStream({ enabled: false });
  const telemetry = useTelemetryStream({ enabled: false });
  const adapters = useAdaptersStream({ enabled: false });

  return useMemo(
    () => ({
      training: training.connected,
      discovery: discovery.connected,
      contacts: contacts.connected,
      fileChanges: fileChanges.connected,
      metrics: metrics.connected,
      telemetry: telemetry.connected,
      adapters: adapters.connected,
      allConnected: [
        training.connected,
        discovery.connected,
        contacts.connected,
        fileChanges.connected,
        metrics.connected,
        telemetry.connected,
        adapters.connected,
      ].every((c) => c),
    }),
    [training.connected, discovery.connected, contacts.connected, fileChanges.connected, metrics.connected, telemetry.connected, adapters.connected]
  );
}
