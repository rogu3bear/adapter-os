import { useState, useCallback } from 'react';
import { logger, toError } from '@/utils/logger';

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

export function useAsyncOperation<T>(
  asyncFn: (...args: unknown[]) => Promise<T>,
  options: UseAsyncOperationOptions = {}
): UseAsyncOperationReturn<T> {
  const { onSuccess, onError, componentName, operationName } = options;

  const [state, setState] = useState<AsyncOperationState<T>>({
    data: null,
    isLoading: false,
    error: null,
    isSuccess: false,
  });

  const [lastArgs, setLastArgs] = useState<unknown[]>([]);

  const execute = useCallback(
    async (...args: unknown[]): Promise<T | null> => {
      setLastArgs(args);
      setState(prev => ({
        ...prev,
        isLoading: true,
        error: null,
        isSuccess: false,
      }));

      try {
        const result = await asyncFn(...args);
        setState({
          data: result,
          isLoading: false,
          error: null,
          isSuccess: true,
        });
        if (onSuccess) {
          onSuccess(result);
        }
        return result;
      } catch (err) {
        const error = err instanceof Error ? err : new Error(String(err));
        setState({
          data: null,
          isLoading: false,
          error,
          isSuccess: false,
        });

        if (componentName || operationName) {
          logger.error(`Async operation failed: ${operationName || 'unknown'}`, {
            component: componentName || 'useAsyncOperation',
            operation: operationName,
          }, toError(err));
        }

        if (onError) {
          onError(error);
        }
        return null;
      }
    },
    [asyncFn, onSuccess, onError, componentName, operationName]
  );

  const retry = useCallback(async (): Promise<T | null> => {
    return execute(...lastArgs);
  }, [execute, lastArgs]);

  const reset = useCallback(() => {
    setState({
      data: null,
      isLoading: false,
      error: null,
      isSuccess: false,
    });
    setLastArgs([]);
  }, []);

  return {
    ...state,
    execute,
    reset,
    retry,
  };
}

// Convenience hook for mutations with automatic error/success toasts
export function useMutation<T>(
  mutationFn: (...args: unknown[]) => Promise<T>,
  options: UseAsyncOperationOptions & {
    successMessage?: string;
    errorMessage?: string;
  } = {}
): UseAsyncOperationReturn<T> {
  return useAsyncOperation(mutationFn, options);
}
