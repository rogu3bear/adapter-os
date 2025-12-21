/**
 * Async Operation State Types
 *
 * Generic state management for async operations and loading states.
 *
 * Citations:
 * - ui/src/hooks/async/useAsyncOperation.ts - Async operation hook
 * - ui/src/hooks/async/useAsyncAction.ts - Async action hook
 */

/**
 * Async operation status
 */
export type AsyncStatus = 'idle' | 'pending' | 'success' | 'error';

/**
 * Generic async operation state
 */
export interface AsyncOperationState<T = unknown, E = Error> {
  /** Current operation status */
  status: AsyncStatus;
  /** Operation result data */
  data: T | null;
  /** Operation error */
  error: E | null;
  /** Whether operation is currently running */
  isLoading: boolean;
  /** Whether operation succeeded */
  isSuccess: boolean;
  /** Whether operation failed */
  isError: boolean;
  /** Whether operation is idle (not started) */
  isIdle: boolean;
}

/**
 * Async operation actions
 */
export interface AsyncOperationActions<T = unknown, E = Error> {
  /** Execute the async operation */
  execute: (...args: unknown[]) => Promise<T>;
  /** Reset to idle state */
  reset: () => void;
  /** Set success state manually */
  setSuccess: (data: T) => void;
  /** Set error state manually */
  setError: (error: E) => void;
}

/**
 * Complete async operation state with actions
 */
export interface AsyncOperationStateWithActions<T = unknown, E = Error>
  extends AsyncOperationState<T, E>,
    AsyncOperationActions<T, E> {}

/**
 * Retry configuration
 */
export interface RetryConfig {
  /** Maximum number of retry attempts */
  maxAttempts: number;
  /** Delay between retries in ms */
  retryDelay: number;
  /** Backoff strategy */
  backoffStrategy?: 'linear' | 'exponential';
  /** Whether to retry on specific error types */
  shouldRetry?: (error: Error) => boolean;
}

/**
 * Retry state
 */
export interface RetryState {
  /** Current attempt number */
  attempt: number;
  /** Maximum attempts allowed */
  maxAttempts: number;
  /** Whether currently retrying */
  isRetrying: boolean;
  /** Last error */
  lastError: Error | null;
  /** Time until next retry (ms) */
  nextRetryIn?: number;
}

/**
 * Retry actions
 */
export interface RetryActions {
  /** Manually trigger retry */
  retry: () => void;
  /** Cancel pending retry */
  cancelRetry: () => void;
  /** Reset retry state */
  resetRetry: () => void;
}

/**
 * Cancellable operation state
 */
export interface CancellableOperationState {
  /** Whether operation is cancellable */
  isCancellable: boolean;
  /** Whether operation is cancelled */
  isCancelled: boolean;
  /** Cancellation reason */
  cancellationReason?: string;
}

/**
 * Cancellable operation actions
 */
export interface CancellableOperationActions {
  /** Cancel the operation */
  cancel: (reason?: string) => void;
}

/**
 * Progress state
 */
export interface ProgressState {
  /** Progress percentage (0-100) */
  percentage: number;
  /** Current step/item */
  current: number;
  /** Total steps/items */
  total: number;
  /** Status message */
  message?: string;
  /** Estimated time remaining (ms) */
  estimatedTimeRemaining?: number;
}

/**
 * Loading state with progress
 */
export interface LoadingStateWithProgress extends ProgressState {
  /** Whether loading */
  isLoading: boolean;
  /** Loading phase/stage */
  phase?: string;
}

/**
 * Optimistic update state
 */
export interface OptimisticUpdateState<T = unknown> {
  /** Original data before optimistic update */
  originalData: T | null;
  /** Optimistically updated data */
  optimisticData: T | null;
  /** Whether currently in optimistic state */
  isOptimistic: boolean;
  /** Whether optimistic update succeeded */
  isConfirmed: boolean;
  /** Whether optimistic update was rolled back */
  isRolledBack: boolean;
}

/**
 * Optimistic update actions
 */
export interface OptimisticUpdateActions<T = unknown> {
  /** Apply optimistic update */
  applyOptimistic: (data: T) => void;
  /** Confirm optimistic update */
  confirm: () => void;
  /** Rollback optimistic update */
  rollback: () => void;
}

/**
 * Debounced state
 */
export interface DebouncedState<T = unknown> {
  /** Current value */
  value: T;
  /** Debounced value */
  debouncedValue: T;
  /** Whether value is pending debounce */
  isPending: boolean;
}

/**
 * Debounced actions
 */
export interface DebouncedActions<T = unknown> {
  /** Set value (will be debounced) */
  setValue: (value: T) => void;
  /** Flush debounce immediately */
  flush: () => void;
  /** Cancel pending debounce */
  cancel: () => void;
}
