//! Retry Hook for Failed Operations with Backoff
//!
//! Provides retry logic with exponential backoff for failed operations.
//! Integrates with React Query and existing retry utilities.
//!
//! # Usage
//! ```tsx
//! const { execute, isRetrying, attemptCount, reset } = useRetry(
//!   async () => await api.submitForm(data),
//!   { maxAttempts: 3, backoffMultiplier: 2 }
//! );
//!
//! // Execute with automatic retry
//! const result = await execute();
//! ```

import { useState, useCallback, useRef, useEffect, useMemo } from 'react';
import { useQueryClient, UseMutationOptions, useMutation } from '@tanstack/react-query';
import { logger, toError } from '@/utils/logger';
import { toast } from 'sonner';
import { isTransientError } from '@/utils/errorMessages';

export interface RetryConfig {
  /** Maximum number of retry attempts (default: 3) */
  maxAttempts?: number;
  /** Base delay in milliseconds (default: 1000) */
  baseDelay?: number;
  /** Maximum delay in milliseconds (default: 30000) */
  maxDelay?: number;
  /** Backoff multiplier (default: 2) */
  backoffMultiplier?: number;
  /** Jitter factor 0-1 (default: 0.1 for 10% jitter) */
  jitter?: number;
  /** Custom function to determine if error is retryable */
  retryableErrors?: (error: Error) => boolean;
  /** Timeout for each attempt in milliseconds */
  timeout?: number;
}

export interface RetryState<TData> {
  /** The data from successful execution */
  data: TData | null;
  /** Whether the operation is executing (including retries) */
  isLoading: boolean;
  /** Whether a retry is in progress */
  isRetrying: boolean;
  /** Whether the operation succeeded */
  isSuccess: boolean;
  /** Whether all retries failed */
  isError: boolean;
  /** The last error */
  error: Error | null;
  /** Current attempt number (1-based) */
  attemptCount: number;
  /** Total number of attempts made */
  totalAttempts: number;
  /** Time until next retry (milliseconds, 0 if not waiting) */
  nextRetryIn: number;
  /** Whether retries are exhausted */
  isExhausted: boolean;
}

export interface UseRetryOptions<TData, TVariables> extends RetryConfig {
  /** Callback on each retry attempt */
  onRetry?: (attempt: number, error: Error, delay: number) => void;
  /** Callback when operation succeeds */
  onSuccess?: (data: TData, variables: TVariables, attempts: number) => void;
  /** Callback when all retries fail */
  onError?: (error: Error, variables: TVariables, attempts: number) => void;
  /** Callback when operation settles */
  onSettled?: (data: TData | null, error: Error | null, variables: TVariables) => void;
  /** Component name for logging */
  componentName?: string;
  /** Operation name for logging */
  operationName?: string;
  /** Show toast notifications for retries */
  showRetryToast?: boolean;
  /** Show toast on final failure */
  showErrorToast?: boolean;
  /** Show toast on success after retries */
  showSuccessToast?: boolean;
  /** Query keys to invalidate on success */
  invalidateKeys?: string[][];
  /** Use React Query mutation */
  useReactQuery?: boolean;
}

export interface UseRetryReturn<TData, TVariables> extends RetryState<TData> {
  /** Execute the operation with retry logic */
  execute: (variables: TVariables) => Promise<TData | null>;
  /** Cancel ongoing retries */
  cancel: () => void;
  /** Reset state to initial */
  reset: () => void;
  /** Manually trigger a retry */
  retry: () => Promise<TData | null>;
}

const DEFAULT_CONFIG: Required<RetryConfig> = {
  maxAttempts: 3,
  baseDelay: 1000,
  maxDelay: 30000,
  backoffMultiplier: 2,
  jitter: 0.1,
  retryableErrors: isTransientError,
  timeout: 30000,
};

const initialState = <TData>(): RetryState<TData> => ({
  data: null,
  isLoading: false,
  isRetrying: false,
  isSuccess: false,
  isError: false,
  error: null,
  attemptCount: 0,
  totalAttempts: 0,
  nextRetryIn: 0,
  isExhausted: false,
});

/**
 * Calculate delay with exponential backoff and jitter.
 */
function calculateDelay(attempt: number, config: Required<RetryConfig>): number {
  const exponentialDelay = config.baseDelay * Math.pow(config.backoffMultiplier, attempt - 1);
  const jitterOffset = exponentialDelay * config.jitter * (Math.random() * 2 - 1);
  const delay = Math.min(exponentialDelay + jitterOffset, config.maxDelay);
  return Math.max(0, Math.round(delay));
}

/**
 * Sleep for specified milliseconds.
 */
function sleep(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}

/**
 * Hook for retry logic with exponential backoff.
 * Provides comprehensive state tracking and control functions.
 *
 * @param operationFn - The async function to execute
 * @param options - Configuration options
 * @returns Retry state and control functions
 */
export function useRetry<TData, TVariables = void>(
  operationFn: (variables: TVariables) => Promise<TData>,
  options: UseRetryOptions<TData, TVariables> = {}
): UseRetryReturn<TData, TVariables> {
  const config: Required<RetryConfig> = useMemo(() => ({
    ...DEFAULT_CONFIG,
    ...options,
  }), [options]);

  const {
    onRetry,
    onSuccess,
    onError,
    onSettled,
    componentName = 'useRetry',
    operationName = 'retry_operation',
    showRetryToast = true,
    showErrorToast = true,
    showSuccessToast = true,
    invalidateKeys = [],
    useReactQuery = false,
  } = options;

  const queryClient = useQueryClient();
  const [state, setState] = useState<RetryState<TData>>(initialState);
  const operationFnRef = useRef(operationFn);
  operationFnRef.current = operationFn;

  const cancelledRef = useRef(false);
  const lastVariablesRef = useRef<TVariables | null>(null);
  const retryTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const countdownIntervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Cleanup function
  const cleanup = useCallback(() => {
    if (retryTimeoutRef.current) {
      clearTimeout(retryTimeoutRef.current);
      retryTimeoutRef.current = null;
    }
    if (countdownIntervalRef.current) {
      clearInterval(countdownIntervalRef.current);
      countdownIntervalRef.current = null;
    }
  }, []);

  // Cancel retries
  const cancel = useCallback(() => {
    cancelledRef.current = true;
    cleanup();
    setState(prev => ({
      ...prev,
      isLoading: false,
      isRetrying: false,
      nextRetryIn: 0,
    }));
    logger.info('Retry operation cancelled', {
      component: componentName,
      operation: operationName,
    });
  }, [cleanup, componentName, operationName]);

  // Reset state
  const reset = useCallback(() => {
    cancelledRef.current = false;
    cleanup();
    setState(initialState());
    lastVariablesRef.current = null;
  }, [cleanup]);

  // Execute with retry logic
  const execute = useCallback(async (variables: TVariables): Promise<TData | null> => {
    cancelledRef.current = false;
    lastVariablesRef.current = variables;
    cleanup();

    setState(prev => ({
      ...prev,
      isLoading: true,
      isRetrying: false,
      isSuccess: false,
      isError: false,
      error: null,
      attemptCount: 0,
      totalAttempts: 0,
      nextRetryIn: 0,
      isExhausted: false,
    }));

    let lastError: Error | null = null;
    let attempt = 0;

    while (attempt < config.maxAttempts && !cancelledRef.current) {
      attempt++;

      setState(prev => ({
        ...prev,
        attemptCount: attempt,
        isRetrying: attempt > 1,
      }));

      try {
        // Execute with optional timeout
        let result: TData;
        if (config.timeout > 0) {
          const timeoutPromise = new Promise<never>((_, reject) => {
            setTimeout(() => reject(new Error('Operation timed out')), config.timeout);
          });
          result = await Promise.race([operationFnRef.current(variables), timeoutPromise]);
        } else {
          result = await operationFnRef.current(variables);
        }

        // Success!
        setState({
          data: result,
          isLoading: false,
          isRetrying: false,
          isSuccess: true,
          isError: false,
          error: null,
          attemptCount: attempt,
          totalAttempts: attempt,
          nextRetryIn: 0,
          isExhausted: false,
        });

        // Invalidate query keys
        invalidateKeys.forEach(key => {
          queryClient.invalidateQueries({ queryKey: key });
        });

        if (attempt > 1 && showSuccessToast) {
          toast.success(`Operation succeeded after ${attempt} attempts`);
        }

        logger.info('Retry operation succeeded', {
          component: componentName,
          operation: operationName,
          attempts: attempt,
        });

        if (onSuccess) {
          onSuccess(result, variables, attempt);
        }

        if (onSettled) {
          onSettled(result, null, variables);
        }

        return result;
      } catch (err) {
        lastError = err instanceof Error ? err : new Error(String(err));

        // Check if we should retry
        const shouldRetry = config.retryableErrors(lastError);

        if (!shouldRetry || attempt >= config.maxAttempts || cancelledRef.current) {
          // Final failure
          setState({
            data: null,
            isLoading: false,
            isRetrying: false,
            isSuccess: false,
            isError: true,
            error: lastError,
            attemptCount: attempt,
            totalAttempts: attempt,
            nextRetryIn: 0,
            isExhausted: true,
          });

          if (showErrorToast) {
            toast.error(`Operation failed after ${attempt} attempt${attempt > 1 ? 's' : ''}`);
          }

          logger.error('Retry operation failed', {
            component: componentName,
            operation: operationName,
            attempts: attempt,
            maxAttempts: config.maxAttempts,
            shouldRetry,
          }, toError(err));

          if (onError) {
            onError(lastError, variables, attempt);
          }

          if (onSettled) {
            onSettled(null, lastError, variables);
          }

          return null;
        }

        // Calculate delay and wait
        const delay = calculateDelay(attempt, config);

        logger.info('Retry operation failed, retrying', {
          component: componentName,
          operation: operationName,
          attempt,
          maxAttempts: config.maxAttempts,
          delay,
          error: lastError.message,
        });

        if (showRetryToast) {
          toast.info(`Retrying... (attempt ${attempt + 1}/${config.maxAttempts})`, {
            description: `Waiting ${Math.round(delay / 1000)}s`,
            duration: delay,
          });
        }

        if (onRetry) {
          onRetry(attempt, lastError, delay);
        }

        // Update countdown
        setState(prev => ({
          ...prev,
          error: lastError,
          nextRetryIn: delay,
        }));

        // Start countdown interval
        const startTime = Date.now();
        countdownIntervalRef.current = setInterval(() => {
          const elapsed = Date.now() - startTime;
          const remaining = Math.max(0, delay - elapsed);
          setState(prev => ({ ...prev, nextRetryIn: remaining }));
          if (remaining === 0 && countdownIntervalRef.current) {
            clearInterval(countdownIntervalRef.current);
            countdownIntervalRef.current = null;
          }
        }, 100);

        // Wait for delay
        await sleep(delay);

        // Clear countdown interval
        if (countdownIntervalRef.current) {
          clearInterval(countdownIntervalRef.current);
          countdownIntervalRef.current = null;
        }
      }
    }

    return null;
  }, [
    config,
    cleanup,
    queryClient,
    invalidateKeys,
    showRetryToast,
    showErrorToast,
    showSuccessToast,
    onRetry,
    onSuccess,
    onError,
    onSettled,
    componentName,
    operationName,
  ]);

  // Manual retry with last variables
  const retry = useCallback(async (): Promise<TData | null> => {
    if (lastVariablesRef.current !== null) {
      return execute(lastVariablesRef.current);
    }
    return null;
  }, [execute]);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      cancelledRef.current = true;
      cleanup();
    };
  }, [cleanup]);

  return {
    ...state,
    execute,
    cancel,
    reset,
    retry,
  };
}

/**
 * Hook for retry with React Query mutation integration.
 * Uses React Query's built-in retry mechanism with custom configuration.
 *
 * @param mutationFn - The mutation function
 * @param options - Configuration options
 * @returns React Query mutation with retry
 */
export function useRetryMutation<TData, TVariables = void>(
  mutationFn: (variables: TVariables) => Promise<TData>,
  options: UseRetryOptions<TData, TVariables> = {}
) {
  const config: Required<RetryConfig> = {
    ...DEFAULT_CONFIG,
    ...options,
  };

  const {
    onSuccess,
    onError,
    onSettled,
    componentName = 'useRetryMutation',
    operationName = 'retry_mutation',
    showErrorToast = true,
    showSuccessToast = true,
    invalidateKeys = [],
  } = options;

  const queryClient = useQueryClient();
  const [attemptCount, setAttemptCount] = useState(0);

  const mutationOptions: UseMutationOptions<TData, Error, TVariables> = {
    mutationFn,
    retry: config.maxAttempts - 1, // React Query counts initial attempt separately
    retryDelay: (attemptIndex) => {
      setAttemptCount(attemptIndex + 1);
      return calculateDelay(attemptIndex + 1, config);
    },
    onSuccess: (data, variables) => {
      invalidateKeys.forEach(key => {
        queryClient.invalidateQueries({ queryKey: key });
      });

      if (attemptCount > 0 && showSuccessToast) {
        toast.success(`Operation succeeded after ${attemptCount + 1} attempts`);
      }

      logger.info('Retry mutation succeeded', {
        component: componentName,
        operation: operationName,
        attempts: attemptCount + 1,
      });

      if (onSuccess) {
        onSuccess(data, variables, attemptCount + 1);
      }
    },
    onError: (error, variables) => {
      if (showErrorToast) {
        toast.error(`Operation failed after ${attemptCount + 1} attempts`);
      }

      logger.error('Retry mutation failed', {
        component: componentName,
        operation: operationName,
        attempts: attemptCount + 1,
      }, error);

      if (onError) {
        onError(error, variables, attemptCount + 1);
      }
    },
    onSettled: (data, error, variables) => {
      setAttemptCount(0);
      if (onSettled) {
        onSettled(data ?? null, error ?? null, variables);
      }
    },
  };

  const mutation = useMutation(mutationOptions);

  return {
    ...mutation,
    attemptCount,
    execute: mutation.mutateAsync,
  };
}

/**
 * Hook for creating a retry wrapper around any async function.
 * Returns a function that automatically retries on failure.
 *
 * @param config - Retry configuration
 * @returns A function that wraps async operations with retry logic
 */
export function useRetryWrapper(config: RetryConfig = {}) {
  const finalConfig: Required<RetryConfig> = useMemo(() => ({
    ...DEFAULT_CONFIG,
    ...config,
  }), [config]);

  return useCallback(
    async <T>(operation: () => Promise<T>): Promise<T> => {
      let lastError: Error | null = null;
      let attempt = 0;

      while (attempt < finalConfig.maxAttempts) {
        attempt++;

        try {
          return await operation();
        } catch (err) {
          lastError = err instanceof Error ? err : new Error(String(err));

          const shouldRetry = finalConfig.retryableErrors(lastError);
          if (!shouldRetry || attempt >= finalConfig.maxAttempts) {
            throw lastError;
          }

          const delay = calculateDelay(attempt, finalConfig);
          await sleep(delay);
        }
      }

      throw lastError || new Error('Retry exhausted');
    },
    [finalConfig]
  );
}
