/**
 * Streaming Service for SSE Event Subscriptions
 *
 * Provides a high-level interface for subscribing to server-sent event (SSE) streams.
 * Handles connection lifecycle, error recovery, and type-safe event parsing.
 *
 * Supported Endpoints:
 * - /v1/streams/training - Training job progress events
 * - /v1/streams/discovery - Adapter discovery/search events
 * - /v1/streams/contacts - Contact/collaboration events
 * - /v1/streams/file-changes - File system change events
 * - /v1/stream/metrics - System metrics (5-sec interval)
 * - /v1/stream/telemetry - Telemetry events
 * - /v1/stream/adapters - Adapter lifecycle state transitions
 *
 * Usage Example:
 * ```typescript
 * const service = StreamingService.getInstance();
 *
 * // Subscribe to training progress
 * const trainingStream = service.subscribeToTraining({
 *   onMessage: (event) => console.log('Training progress:', event),
 *   onError: (error) => console.error('Stream error:', error),
 * });
 *
 * // Later: unsubscribe
 * trainingStream.unsubscribe();
 * ```
 */

import { logger, toError } from '@/utils/logger';
import {
  TrainingStreamEvent,
  DiscoveryStreamEvent,
  ContactStreamEvent,
  FileChangeStreamEvent,
  MetricsStreamEvent,
  TelemetryStreamEvent,
  AdapterStreamEvent,
  StreamConfig,
  parseStreamEvent,
} from '@/api/streaming-types';
import apiClient from '@/api/client';

// ============================================================================
// Subscription Types
// ============================================================================

/**
 * Handle for a stream subscription
 */
export interface StreamSubscription {
  /** Unsubscribe from the stream */
  unsubscribe: () => void;

  /** Manually reconnect the stream */
  reconnect: () => void;

  /** Get current connection state */
  isConnected: () => boolean;

  /** Get the last error that occurred, if any */
  getError: () => Error | null;

  /** Get the current reconnection attempt count */
  getReconnectAttempts: () => number;
}

/**
 * Internal subscription state
 */
interface InternalSubscription<T = unknown> {
  endpoint: string;
  eventSource: EventSource | null;
  config: StreamConfig;
  reconnectAttempts: number;
  reconnectTimeout: ReturnType<typeof setTimeout> | null;
  isActive: boolean;
  lastError: Error | null;
}

// ============================================================================
// Streaming Service
// ============================================================================

class StreamingService {
  private static instance: StreamingService;

  private subscriptions: Map<string, InternalSubscription> = new Map();
  private baseUrl: string;
  private maxReconnectAttempts = 10;
  private initialBackoffMs = 1000;
  private maxBackoffMs = 30000;

  private constructor() {
    this.baseUrl = (import.meta as { env?: { VITE_API_URL?: string } }).env?.VITE_API_URL || '/api';
    logger.info('StreamingService initialized', {
      component: 'StreamingService',
      baseUrl: this.baseUrl,
    });
  }

  /**
   * Get or create the singleton instance
   */
  public static getInstance(): StreamingService {
    if (!StreamingService.instance) {
      StreamingService.instance = new StreamingService();
    }
    return StreamingService.instance;
  }

  /**
   * Internal method to establish a stream subscription
   */
  private subscribe<T = unknown>(
    endpoint: string,
    subscriptionId: string,
    config: StreamConfig = {}
  ): StreamSubscription {
    const {
      enabled = true,
      onMessage,
      onError,
      onOpen,
      onClose,
      autoReconnect = true,
      maxReconnectAttempts = this.maxReconnectAttempts,
      initialBackoffMs = this.initialBackoffMs,
      maxBackoffMs = this.maxBackoffMs,
    } = config;

    if (!enabled) {
      return {
        unsubscribe: () => {},
        reconnect: () => {},
        isConnected: () => false,
        getError: () => null,
        getReconnectAttempts: () => 0,
      };
    }

    // Clean up any existing subscription
    if (this.subscriptions.has(subscriptionId)) {
      this.subscriptions.get(subscriptionId)?.isActive && this.unsubscribe(subscriptionId);
    }

    const subscription: InternalSubscription<T> = {
      endpoint,
      eventSource: null,
      config,
      reconnectAttempts: 0,
      reconnectTimeout: null,
      isActive: true,
      lastError: null,
    };

    const connect = () => {
      if (!subscription.isActive) return;

      try {
        // Close existing connection
        if (subscription.eventSource) {
          subscription.eventSource.close();
          subscription.eventSource = null;
        }

        // Get authentication token
        const token = apiClient.getToken();
        const url = token
          ? `${this.baseUrl}${endpoint}?token=${encodeURIComponent(token)}`
          : `${this.baseUrl}${endpoint}`;

        subscription.eventSource = new EventSource(url);

        subscription.eventSource.onopen = () => {
          logger.info('Stream connection established', {
            component: 'StreamingService',
            endpoint,
            subscriptionId,
          });
          subscription.reconnectAttempts = 0;
          subscription.lastError = null; // Clear error on successful connection
          if (onOpen) onOpen();
        };

        subscription.eventSource.onmessage = (event: MessageEvent) => {
          try {
            const parsed = parseStreamEvent<T>(event.data);
            if (onMessage) {
              onMessage(parsed);
            }
          } catch (e) {
            logger.error('Failed to parse stream message', {
              component: 'StreamingService',
              endpoint,
              subscriptionId,
            }, toError(e));
          }
        };

        subscription.eventSource.onerror = (event: Event) => {
          if (!subscription.isActive) return;

          // Track the error for later retrieval
          subscription.lastError = new Error('Stream connection error');

          logger.warn('Stream connection error', {
            component: 'StreamingService',
            endpoint,
            subscriptionId,
            reconnectAttempts: subscription.reconnectAttempts,
          });

          if (onError) onError(event);

          // Close and attempt reconnection
          subscription.eventSource?.close();
          subscription.eventSource = null;

          if (autoReconnect && subscription.reconnectAttempts < maxReconnectAttempts) {
            const backoffMs = Math.min(
              initialBackoffMs * Math.pow(2, subscription.reconnectAttempts),
              maxBackoffMs
            );
            subscription.reconnectAttempts += 1;

            logger.info('Scheduling stream reconnection', {
              component: 'StreamingService',
              endpoint,
              subscriptionId,
              backoffMs,
              attempt: subscription.reconnectAttempts,
            });

            subscription.reconnectTimeout = setTimeout(() => {
              connect();
            }, backoffMs);
          } else if (subscription.reconnectAttempts >= maxReconnectAttempts) {
            logger.error('Stream max reconnection attempts exceeded', {
              component: 'StreamingService',
              endpoint,
              subscriptionId,
              maxAttempts: maxReconnectAttempts,
            });
          }
        };
      } catch (e) {
        logger.error('Failed to establish stream connection', {
          component: 'StreamingService',
          endpoint,
          subscriptionId,
        }, toError(e));

        if (autoReconnect && subscription.reconnectAttempts < maxReconnectAttempts) {
          const backoffMs = Math.min(
            initialBackoffMs * Math.pow(2, subscription.reconnectAttempts),
            maxBackoffMs
          );
          subscription.reconnectAttempts += 1;

          subscription.reconnectTimeout = setTimeout(() => {
            connect();
          }, backoffMs);
        }
      }
    };

    // Store subscription and connect
    this.subscriptions.set(subscriptionId, subscription);
    connect();

    // Return subscription handle
    return {
      unsubscribe: () => this.unsubscribe(subscriptionId),
      reconnect: () => {
        subscription.reconnectAttempts = 0;
        subscription.lastError = null;
        connect();
      },
      isConnected: () => subscription.eventSource?.readyState === EventSource.OPEN,
      getError: () => subscription.lastError,
      getReconnectAttempts: () => subscription.reconnectAttempts,
    };
  }

  /**
   * Unsubscribe from a stream
   */
  private unsubscribe(subscriptionId: string): void {
    const subscription = this.subscriptions.get(subscriptionId);
    if (!subscription) return;

    subscription.isActive = false;

    if (subscription.eventSource) {
      subscription.eventSource.close();
      subscription.eventSource = null;
    }

    if (subscription.reconnectTimeout) {
      clearTimeout(subscription.reconnectTimeout);
      subscription.reconnectTimeout = null;
    }

    this.subscriptions.delete(subscriptionId);

    logger.info('Stream unsubscribed', {
      component: 'StreamingService',
      endpoint: subscription.endpoint,
      subscriptionId,
    });
  }

  /**
   * Unsubscribe from all streams
   */
  public unsubscribeAll(): void {
    const subscriptionIds = Array.from(this.subscriptions.keys());
    subscriptionIds.forEach((id) => this.unsubscribe(id));
  }

  // ========================================================================
  // Endpoint-Specific Methods
  // ========================================================================

  /**
   * Subscribe to training job progress events
   * Endpoint: `/v1/streams/training`
   */
  public subscribeToTraining(config: StreamConfig = {}): StreamSubscription {
    const subscriptionId = `training-${Date.now()}-${crypto.randomUUID().slice(0, 8)}`;
    return this.subscribe<TrainingStreamEvent>('/v1/streams/training', subscriptionId, config);
  }

  /**
   * Subscribe to adapter discovery events
   * Endpoint: `/v1/streams/discovery`
   */
  public subscribeToDiscovery(config: StreamConfig = {}): StreamSubscription {
    const subscriptionId = `discovery-${Date.now()}-${crypto.randomUUID().slice(0, 8)}`;
    return this.subscribe<DiscoveryStreamEvent>('/v1/streams/discovery', subscriptionId, config);
  }

  /**
   * Subscribe to contact/collaboration events
   * Endpoint: `/v1/streams/contacts`
   */
  public subscribeToContacts(config: StreamConfig = {}): StreamSubscription {
    const subscriptionId = `contacts-${Date.now()}-${crypto.randomUUID().slice(0, 8)}`;
    return this.subscribe<ContactStreamEvent>('/v1/streams/contacts', subscriptionId, config);
  }

  /**
   * Subscribe to file change events
   * Endpoint: `/v1/streams/file-changes`
   */
  public subscribeToFileChanges(config: StreamConfig = {}): StreamSubscription {
    const subscriptionId = `filechanges-${Date.now()}-${crypto.randomUUID().slice(0, 8)}`;
    return this.subscribe<FileChangeStreamEvent>('/v1/streams/file-changes', subscriptionId, config);
  }

  /**
   * Subscribe to system metrics (5-sec interval)
   * Endpoint: `/v1/stream/metrics`
   */
  public subscribeToMetrics(config: StreamConfig = {}): StreamSubscription {
    const subscriptionId = `metrics-${Date.now()}-${crypto.randomUUID().slice(0, 8)}`;
    return this.subscribe<MetricsStreamEvent>('/v1/stream/metrics', subscriptionId, config);
  }

  /**
   * Subscribe to telemetry events
   * Endpoint: `/v1/stream/telemetry`
   */
  public subscribeToTelemetry(config: StreamConfig = {}): StreamSubscription {
    const subscriptionId = `telemetry-${Date.now()}-${crypto.randomUUID().slice(0, 8)}`;
    return this.subscribe<TelemetryStreamEvent>('/v1/stream/telemetry', subscriptionId, config);
  }

  /**
   * Subscribe to adapter lifecycle state transitions
   * Endpoint: `/v1/stream/adapters`
   */
  public subscribeToAdapters(config: StreamConfig = {}): StreamSubscription {
    const subscriptionId = `adapters-${Date.now()}-${crypto.randomUUID().slice(0, 8)}`;
    return this.subscribe<AdapterStreamEvent>('/v1/stream/adapters', subscriptionId, config);
  }

  /**
   * Get all active subscriptions
   */
  public getActiveSubscriptions(): Array<{
    id: string;
    endpoint: string;
    connected: boolean;
  }> {
    return Array.from(this.subscriptions.entries()).map(([id, sub]) => ({
      id,
      endpoint: sub.endpoint,
      connected: sub.eventSource?.readyState === EventSource.OPEN,
    }));
  }

  /**
   * Get subscription count
   */
  public getSubscriptionCount(): number {
    return this.subscriptions.size;
  }
}

// Export singleton instance
export const streamingService = StreamingService.getInstance();

// Export the service class for testing
export default StreamingService;
