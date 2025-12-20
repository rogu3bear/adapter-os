/**
 * System Hook Mock Factories
 *
 * Factory functions for creating mock system-related hook return values.
 */

import { vi, type Mock } from 'vitest';

// ============================================================================
// useLiveData Mock
// ============================================================================

/**
 * Return type for useLiveData hook mock
 */
export interface UseLiveDataMockReturn<T = unknown> {
  data: T | null;
  isLoading: boolean;
  error: Error | null;
  sseConnected: boolean;
  connectionStatus: 'polling' | 'sse' | 'disconnected';
  lastUpdated: Date | null;
  freshnessLevel: 'fresh' | 'recent' | 'stale' | 'very_stale' | 'live';
  refetch: Mock;
  reconnect: Mock;
  toggleSSE: Mock;
}

/**
 * Options for createUseLiveDataMock factory
 */
export interface UseLiveDataMockOptions<T = unknown> {
  /** Data payload (default: null) */
  data?: T | null;
  /** Loading state (default: false) */
  isLoading?: boolean;
  /** Error object (default: null) */
  error?: Error | null;
  /** SSE connected (default: false) */
  sseConnected?: boolean;
  /** Connection status (default: 'polling') */
  connectionStatus?: 'polling' | 'sse' | 'disconnected';
  /** Last update timestamp (default: null) */
  lastUpdated?: Date | null;
  /** Data freshness level (default: 'recent') */
  freshnessLevel?: 'fresh' | 'recent' | 'stale' | 'very_stale' | 'live';
}

/**
 * Create a mock return value for useLiveData hook
 *
 * @example
 * ```typescript
 * // Empty state
 * const empty = createUseLiveDataMock();
 *
 * // With data
 * const withData = createUseLiveDataMock({
 *   data: { metrics: { cpu: 50, memory: 60 } },
 *   freshnessLevel: 'live',
 *   sseConnected: true,
 * });
 *
 * // Loading state
 * const loading = createUseLiveDataMock({ isLoading: true });
 * ```
 */
export function createUseLiveDataMock<T = unknown>(
  options: UseLiveDataMockOptions<T> = {}
): UseLiveDataMockReturn<T> {
  return {
    data: options.data ?? null,
    isLoading: options.isLoading ?? false,
    error: options.error ?? null,
    sseConnected: options.sseConnected ?? false,
    connectionStatus: options.connectionStatus ?? 'polling',
    lastUpdated: options.lastUpdated ?? null,
    freshnessLevel: options.freshnessLevel ?? 'recent',
    refetch: vi.fn().mockResolvedValue(undefined),
    reconnect: vi.fn(),
    toggleSSE: vi.fn(),
  };
}

// ============================================================================
// useSystemState Mock
// ============================================================================

/**
 * Return type for useSystemState hook mock
 */
export interface UseSystemStateMockReturn {
  data: unknown | null;
  isLoading: boolean;
  error: Error | null;
  isLive: boolean;
  lastUpdated: Date | null;
  refetch: Mock;
}

/**
 * Options for createUseSystemStateMock factory
 */
export interface UseSystemStateMockOptions {
  /** System data (default: null) */
  data?: unknown | null;
  /** Loading state (default: false) */
  isLoading?: boolean;
  /** Error object (default: null) */
  error?: Error | null;
  /** Live updates enabled (default: false) */
  isLive?: boolean;
  /** Last update timestamp (default: null) */
  lastUpdated?: Date | null;
}

/**
 * Create a mock return value for useSystemState hook
 */
export function createUseSystemStateMock(
  options: UseSystemStateMockOptions = {}
): UseSystemStateMockReturn {
  return {
    data: options.data ?? null,
    isLoading: options.isLoading ?? false,
    error: options.error ?? null,
    isLive: options.isLive ?? false,
    lastUpdated: options.lastUpdated ?? null,
    refetch: vi.fn().mockResolvedValue(undefined),
  };
}
