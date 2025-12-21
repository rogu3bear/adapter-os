/**
 * Realtime Hook Types
 *
 * Type definitions for realtime data hooks including SSE, polling,
 * live data, notifications, and activity feeds.
 */

// ============================================================================
// Common Types
// ============================================================================

export type PollingSpeed = 'slow' | 'normal' | 'fast' | 'realtime';
// SSE connection status - matches useSSEWithPollingFallback implementation
export type ConnectionStatus = 'sse' | 'polling' | 'disconnected';
export type DataFreshnessLevel = 'live' | 'fresh' | 'recent' | 'stale' | 'very_stale';

// ============================================================================
// useSSE Types
// ============================================================================

export interface UseSSEOptions<T = unknown> {
  /** SSE endpoint URL */
  endpoint: string;
  /** Event type to listen for */
  eventType?: string;
  /** Enable/disable SSE */
  enabled?: boolean;
  /** Transform SSE data */
  transform?: (data: unknown) => T;
  /** Callback on message */
  onMessage?: (data: T) => void;
  /** Callback on error */
  onError?: (error: Error) => void;
  /** Callback on connection open */
  onOpen?: () => void;
  /** Callback on connection close */
  onClose?: () => void;
  /** Auto-reconnect on error */
  autoReconnect?: boolean;
  /** Reconnect delay in ms */
  reconnectDelay?: number;
  /** Maximum reconnect attempts */
  maxReconnectAttempts?: number;
}

// ============================================================================
// useSSEWithPollingFallback Types
// ============================================================================

export interface UseSSEWithPollingFallbackOptions<T> {
  /** SSE endpoint path (e.g., '/v1/stream/metrics') */
  sseEndpoint?: string;

  /** SSE event type to listen for (e.g., 'metrics', 'adapters') */
  sseEventType?: string;

  /** Function to fetch data for polling/initial load */
  pollingFn: () => Promise<T>;

  /** Polling speed preset */
  pollingSpeed?: PollingSpeed;

  /** Enable/disable the hook */
  enabled?: boolean;

  /** Use SSE (if false, only uses polling) */
  useSSE?: boolean;

  /** Transform SSE data before storing */
  transformSSE?: (sseData: unknown) => T;

  /** Callback when SSE message received */
  onSSEMessage?: (data: unknown) => void;

  /** Callback on error */
  onError?: (error: Error, source: 'sse' | 'polling') => void;

  /** Callback when connection status changes */
  onConnectionChange?: (status: ConnectionStatus) => void;

  /** Circuit breaker: failures before opening (default: 5) */
  circuitBreakerThreshold?: number;

  /** Circuit breaker: reset delay in ms (default: 30000) */
  circuitBreakerResetMs?: number;

  /** SSE initial backoff in ms (default: 1000) */
  sseInitialBackoffMs?: number;

  /** Operation name for logging */
  operationName?: string;
}

export interface UseSSEWithPollingFallbackReturn<T> {
  /** Current data */
  data: T | null;
  /** Loading state */
  isLoading: boolean;
  /** Last error */
  error: Error | null;
  /** SSE connection status */
  isConnected: boolean;
  /** Overall connection status */
  connectionStatus: ConnectionStatus;
  /** Manually refetch data */
  refetch: () => Promise<void>;
  /** Manually reconnect SSE */
  reconnect: () => void;
}

// ============================================================================
// useLiveData Types
// ============================================================================

export interface UseLiveDataOptions<T> {
  /** SSE endpoint path (e.g., '/v1/stream/metrics') */
  sseEndpoint?: string;

  /** SSE event type to listen for (e.g., 'metrics', 'adapters') */
  sseEventType?: string;

  /** Function to fetch data for polling/initial load */
  fetchFn: () => Promise<T>;

  /** Polling speed preset */
  pollingSpeed?: PollingSpeed;

  /** Enable/disable the hook */
  enabled?: boolean;

  /** Transform SSE data before merging with state */
  transformSSE?: (sseData: unknown) => Partial<T>;

  /** How to combine SSE data with existing data */
  mergeStrategy?: 'replace' | 'merge';

  /** Callback when SSE message received */
  onSSEMessage?: (data: unknown) => void;

  /** Callback on error */
  onError?: (error: Error, source: 'sse' | 'polling') => void;

  /** Callback when connection status changes */
  onConnectionChange?: (status: ConnectionStatus) => void;

  /** Circuit breaker: failures before opening (default: 5) */
  circuitBreakerThreshold?: number;

  /** Circuit breaker: reset delay in ms (default: 30000) */
  circuitBreakerResetMs?: number;

  /** Operation name for logging */
  operationName?: string;
}

export interface UseLiveDataReturn<T> {
  /** Current data */
  data: T | null;

  /** Loading state */
  isLoading: boolean;

  /** Last error */
  error: Error | null;

  /** SSE connection status */
  sseConnected: boolean;

  /** Overall connection status */
  connectionStatus: ConnectionStatus;

  /** Timestamp of last update */
  lastUpdated: Date | null;

  /** Data freshness level */
  freshnessLevel: DataFreshnessLevel;

  /** Manually refetch data */
  refetch: () => Promise<void>;

  /** Manually reconnect SSE */
  reconnect: () => void;

  /** Toggle SSE on/off */
  toggleSSE: (enabled: boolean) => void;
}

// ============================================================================
// usePolling Types
// ============================================================================

export interface UsePollingReturn<T> {
  /** Current data */
  data: T | null;
  /** Loading state */
  isLoading: boolean;
  /** Error state */
  error: Error | null;
  /** Whether polling is active */
  isPolling: boolean;
  /** Start polling */
  startPolling: () => void;
  /** Stop polling */
  stopPolling: () => void;
  /** Manually refetch */
  refetch: () => Promise<void>;
  /** Update polling interval */
  setInterval: (intervalMs: number) => void;
}

// ============================================================================
// useNotifications Types
// ============================================================================

export interface UseNotificationsOptions {
  /** Enable/disable notifications */
  enabled?: boolean;
  /** SSE endpoint for notifications */
  endpoint?: string;
  /** Callback when notification received */
  onNotification?: (notification: unknown) => void;
  /** Auto-mark as read */
  autoMarkRead?: boolean;
}

export interface UseNotificationsReturn {
  /** All notifications */
  notifications: unknown[];
  /** Unread notifications */
  unreadNotifications: unknown[];
  /** Unread count */
  unreadCount: number;
  /** Mark notification as read */
  markAsRead: (notificationId: string) => Promise<void>;
  /** Mark all as read */
  markAllAsRead: () => Promise<void>;
  /** Clear all notifications */
  clearAll: () => Promise<void>;
  /** Loading state */
  isLoading: boolean;
  /** Error state */
  error: Error | null;
}

// ============================================================================
// useActivityEvents Types
// ============================================================================

export interface UseActivityEventsOptions {
  /** Enable/disable activity events */
  enabled?: boolean;
  /** Event types to listen for */
  eventTypes?: string[];
  /** Maximum events to keep */
  maxEvents?: number;
  /** Callback when event received */
  onEvent?: (event: unknown) => void;
}

export interface UseActivityEventsReturn {
  /** Recent events */
  events: unknown[];
  /** Loading state */
  isLoading: boolean;
  /** Error state */
  error: Error | null;
  /** Clear events */
  clearEvents: () => void;
  /** SSE connection status */
  isConnected: boolean;
}

// ============================================================================
// useActivityFeed Types
// ============================================================================

export interface UseActivityFeedOptions {
  /** Enable/disable activity feed */
  enabled?: boolean;
  /** Polling interval in ms */
  pollingInterval?: number;
  /** Maximum items in feed */
  maxItems?: number;
  /** Filter predicate */
  filter?: (item: unknown) => boolean;
}

export interface UseActivityFeedReturn {
  /** Activity items */
  items: unknown[];
  /** Loading state */
  isLoading: boolean;
  /** Error state */
  error: Error | null;
  /** Refetch feed */
  refetch: () => Promise<void>;
  /** Clear feed */
  clear: () => void;
  /** Whether feed is live */
  isLive: boolean;
}

// ============================================================================
// useSessionTelemetry Types
// ============================================================================

export interface UseSessionTelemetryOptions {
  /** Session ID to monitor */
  sessionId: string;
  /** Enable/disable telemetry */
  enabled?: boolean;
  /** Polling interval in ms */
  pollingInterval?: number;
}

export interface UseSessionTelemetryResult {
  /** Telemetry data */
  telemetry: unknown | null;
  /** Loading state */
  isLoading: boolean;
  /** Error state */
  error: Error | null;
  /** Refetch telemetry */
  refetch: () => Promise<void>;
}
