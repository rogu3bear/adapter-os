/**
 * Async Hook Types
 *
 * Type definitions for async operation hooks including actions, operations,
 * retry logic, and cancellable operations.
 */

// ============================================================================
// useAsyncAction Types
// ============================================================================

export interface AsyncActionState<TData> {
  /** The data returned from the action */
  data: TData | null;
  /** Whether the action is currently executing */
  isLoading: boolean;
  /** Whether the action is idle (not loading and no error) */
  isIdle: boolean;
  /** Whether the action completed successfully */
  isSuccess: boolean;
  /** Whether the action failed */
  isError: boolean;
  /** The error from the last execution, if any */
  error: Error | null;
  /** Number of times the action has been executed */
  executionCount: number;
}

export interface UseAsyncActionOptions<TData, TVariables> {
  /** Callback when action succeeds */
  onSuccess?: (data: TData, variables: TVariables) => void;
  /** Callback when action fails */
  onError?: (error: Error, variables: TVariables) => void;
  /** Callback when action settles (success or error) */
  onSettled?: (data: TData | null, error: Error | null, variables: TVariables) => void;
  /** Component name for logging */
  componentName?: string;
  /** Operation name for logging */
  operationName?: string;
  /** Query keys to invalidate on success */
  invalidateKeys?: string[][];
  /** Show success toast automatically */
  successToast?: string;
  /** Show error toast automatically */
  errorToast?: string | ((error: Error) => string);
  /** Enable mutation through React Query for cache integration */
  useReactQuery?: boolean;
  /** Retry count for React Query mutation */
  retry?: number;
}

export interface UseAsyncActionReturn<TData, TVariables> extends AsyncActionState<TData> {
  /** Execute the async action */
  execute: (variables: TVariables) => Promise<TData | null>;
  /** Execute the async action (alias for execute) */
  mutate: (variables: TVariables) => void;
  /** Execute the async action and return a promise */
  mutateAsync: (variables: TVariables) => Promise<TData>;
  /** Reset the state to initial values */
  reset: () => void;
}

// ============================================================================
// useAsyncOperation Types
// ============================================================================

export interface AsyncOperationState<T> {
  data: T | null;
  isLoading: boolean;
  error: Error | null;
  isSuccess: boolean;
}

export interface UseAsyncOperationOptions {
  onSuccess?: (data: unknown) => void;
  onError?: (error: Error) => void;
  componentName?: string;
  operationName?: string;
}

export interface UseAsyncOperationReturn<T> extends AsyncOperationState<T> {
  execute: (...args: unknown[]) => Promise<T | null>;
  reset: () => void;
  retry: () => Promise<T | null>;
}

// ============================================================================
// useRetry Types
// ============================================================================

export interface RetryConfig {
  /** Maximum number of retry attempts (default: 3) */
  maxRetries?: number;
  /** Delay between retries in ms (default: 1000) */
  retryDelay?: number;
  /** Exponential backoff multiplier (default: 2) */
  backoffMultiplier?: number;
  /** Maximum delay between retries in ms (default: 30000) */
  maxRetryDelay?: number;
  /** Function to determine if error is retryable */
  shouldRetry?: (error: Error, attemptNumber: number) => boolean;
}

export interface RetryState<TData> {
  /** Current data */
  data: TData | null;
  /** Loading state */
  isLoading: boolean;
  /** Error state */
  error: Error | null;
  /** Number of retry attempts made */
  retryCount: number;
  /** Whether operation succeeded */
  isSuccess: boolean;
  /** Whether operation failed (after all retries) */
  isError: boolean;
}

export interface UseRetryOptions<TData, TVariables> extends RetryConfig {
  /** Callback on success */
  onSuccess?: (data: TData, variables: TVariables) => void;
  /** Callback on error (after all retries exhausted) */
  onError?: (error: Error, variables: TVariables) => void;
  /** Callback on each retry attempt */
  onRetry?: (error: Error, attemptNumber: number) => void;
  /** Component name for logging */
  componentName?: string;
  /** Operation name for logging */
  operationName?: string;
}

export interface UseRetryReturn<TData, TVariables> extends RetryState<TData> {
  /** Execute the operation with retry logic */
  execute: (variables: TVariables) => Promise<TData | null>;
  /** Reset state */
  reset: () => void;
  /** Cancel ongoing retries */
  cancel: () => void;
  /** Whether retries can be cancelled */
  canCancel: boolean;
}

// ============================================================================
// useCancellableOperation Types
// ============================================================================

export interface UseCancellableOperationReturn<T> {
  /** Current data */
  data: T | null;
  /** Loading state */
  isLoading: boolean;
  /** Error state */
  error: Error | null;
  /** Success state */
  isSuccess: boolean;
  /** Execute the operation */
  execute: (...args: unknown[]) => Promise<T | null>;
  /** Cancel the ongoing operation */
  cancel: () => void;
  /** Whether operation can be cancelled */
  canCancel: boolean;
  /** Reset state */
  reset: () => void;
}
