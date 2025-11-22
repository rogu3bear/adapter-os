//! Async Action Hook with Loading/Error States
//!
//! Provides a React Query-compatible hook for executing async actions with
//! comprehensive loading, error, and success state management.
//!
//! # Usage
//! ```tsx
//! const { execute, isLoading, error, data, reset } = useAsyncAction(
//!   async (id: string) => await api.deleteAdapter(id),
//!   { onSuccess: () => toast.success('Deleted!') }
//! );
//! ```

import { useState, useCallback, useRef, useMemo } from 'react';
import { useMutation, useQueryClient, UseMutationOptions } from '@tanstack/react-query';
import { logger, toError } from '../utils/logger';
import { toast } from 'sonner';

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

const initialState = <TData>(): AsyncActionState<TData> => ({
  data: null,
  isLoading: false,
  isIdle: true,
  isSuccess: false,
  isError: false,
  error: null,
  executionCount: 0,
});

/**
 * Hook for executing async actions with comprehensive state management.
 * Integrates with React Query for cache invalidation and mutation tracking.
 *
 * @param actionFn - The async function to execute
 * @param options - Configuration options
 * @returns State and control functions for the async action
 */
export function useAsyncAction<TData, TVariables = void>(
  actionFn: (variables: TVariables) => Promise<TData>,
  options: UseAsyncActionOptions<TData, TVariables> = {}
): UseAsyncActionReturn<TData, TVariables> {
  const {
    onSuccess,
    onError,
    onSettled,
    componentName = 'useAsyncAction',
    operationName = 'async_action',
    invalidateKeys = [],
    successToast,
    errorToast,
    useReactQuery = false,
    retry = 0,
  } = options;

  const queryClient = useQueryClient();
  const [state, setState] = useState<AsyncActionState<TData>>(initialState);
  const actionFnRef = useRef(actionFn);
  actionFnRef.current = actionFn;

  // React Query mutation for cache integration
  const mutationOptions: UseMutationOptions<TData, Error, TVariables> = useMemo(() => ({
    mutationFn: (variables: TVariables) => actionFnRef.current(variables),
    retry,
    onSuccess: (data, variables) => {
      // Invalidate specified query keys
      invalidateKeys.forEach(key => {
        queryClient.invalidateQueries({ queryKey: key });
      });

      if (successToast) {
        toast.success(successToast);
      }

      if (onSuccess) {
        onSuccess(data, variables);
      }

      logger.info('Async action succeeded', {
        component: componentName,
        operation: operationName,
      });
    },
    onError: (error, variables) => {
      const errorMessage = typeof errorToast === 'function'
        ? errorToast(error)
        : errorToast;

      if (errorMessage) {
        toast.error(errorMessage);
      }

      logger.error('Async action failed', {
        component: componentName,
        operation: operationName,
      }, toError(error));

      if (onError) {
        onError(error, variables);
      }
    },
    onSettled: (data, error, variables) => {
      if (onSettled) {
        onSettled(data ?? null, error ?? null, variables);
      }
    },
  }), [
    queryClient,
    invalidateKeys,
    successToast,
    errorToast,
    onSuccess,
    onError,
    onSettled,
    componentName,
    operationName,
    retry,
  ]);

  const mutation = useMutation(mutationOptions);

  // Manual state management for non-React Query mode
  const executeManual = useCallback(async (variables: TVariables): Promise<TData | null> => {
    setState(prev => ({
      ...prev,
      isLoading: true,
      isIdle: false,
      isError: false,
      error: null,
    }));

    try {
      const result = await actionFnRef.current(variables);

      setState(prev => ({
        data: result,
        isLoading: false,
        isIdle: false,
        isSuccess: true,
        isError: false,
        error: null,
        executionCount: prev.executionCount + 1,
      }));

      // Invalidate specified query keys
      invalidateKeys.forEach(key => {
        queryClient.invalidateQueries({ queryKey: key });
      });

      if (successToast) {
        toast.success(successToast);
      }

      if (onSuccess) {
        onSuccess(result, variables);
      }

      logger.info('Async action succeeded', {
        component: componentName,
        operation: operationName,
      });

      if (onSettled) {
        onSettled(result, null, variables);
      }

      return result;
    } catch (err) {
      const error = err instanceof Error ? err : new Error(String(err));

      setState(prev => ({
        data: null,
        isLoading: false,
        isIdle: false,
        isSuccess: false,
        isError: true,
        error,
        executionCount: prev.executionCount + 1,
      }));

      const errorMessage = typeof errorToast === 'function'
        ? errorToast(error)
        : errorToast;

      if (errorMessage) {
        toast.error(errorMessage);
      }

      logger.error('Async action failed', {
        component: componentName,
        operation: operationName,
      }, toError(err));

      if (onError) {
        onError(error, variables);
      }

      if (onSettled) {
        onSettled(null, error, variables);
      }

      return null;
    }
  }, [
    queryClient,
    invalidateKeys,
    successToast,
    errorToast,
    onSuccess,
    onError,
    onSettled,
    componentName,
    operationName,
  ]);

  const reset = useCallback(() => {
    if (useReactQuery) {
      mutation.reset();
    } else {
      setState(initialState());
    }
  }, [useReactQuery, mutation]);

  // Return React Query mutation state or manual state
  if (useReactQuery) {
    return {
      data: mutation.data ?? null,
      isLoading: mutation.isPending,
      isIdle: mutation.isIdle,
      isSuccess: mutation.isSuccess,
      isError: mutation.isError,
      error: mutation.error,
      executionCount: 0, // React Query doesn't track this
      execute: async (variables: TVariables) => {
        try {
          return await mutation.mutateAsync(variables);
        } catch {
          return null;
        }
      },
      mutate: mutation.mutate,
      mutateAsync: mutation.mutateAsync,
      reset,
    };
  }

  return {
    ...state,
    execute: executeManual,
    mutate: (variables: TVariables) => { executeManual(variables); },
    mutateAsync: async (variables: TVariables) => {
      const result = await executeManual(variables);
      if (result === null && state.error) {
        throw state.error;
      }
      return result as TData;
    },
    reset,
  };
}

/**
 * Convenience hook for creating a confirmed action that shows a confirmation dialog.
 * Wraps useAsyncAction with confirmation logic.
 *
 * @param actionFn - The async function to execute after confirmation
 * @param options - Configuration options including confirmation message
 * @returns State and control functions including a confirm function
 */
export function useConfirmedAction<TData, TVariables = void>(
  actionFn: (variables: TVariables) => Promise<TData>,
  options: UseAsyncActionOptions<TData, TVariables> & {
    confirmMessage?: string | ((variables: TVariables) => string);
    confirmTitle?: string;
  } = {}
) {
  const { confirmMessage = 'Are you sure?', confirmTitle = 'Confirm Action', ...actionOptions } = options;
  const asyncAction = useAsyncAction(actionFn, actionOptions);
  const [pendingVariables, setPendingVariables] = useState<TVariables | null>(null);
  const [showConfirm, setShowConfirm] = useState(false);

  const requestConfirmation = useCallback((variables: TVariables) => {
    setPendingVariables(variables);
    setShowConfirm(true);
  }, []);

  const confirm = useCallback(async () => {
    if (pendingVariables !== null) {
      setShowConfirm(false);
      const result = await asyncAction.execute(pendingVariables);
      setPendingVariables(null);
      return result;
    }
    return null;
  }, [asyncAction, pendingVariables]);

  const cancel = useCallback(() => {
    setShowConfirm(false);
    setPendingVariables(null);
  }, []);

  const message = typeof confirmMessage === 'function' && pendingVariables !== null
    ? confirmMessage(pendingVariables)
    : confirmMessage as string;

  return {
    ...asyncAction,
    requestConfirmation,
    confirm,
    cancel,
    showConfirm,
    confirmTitle,
    confirmMessage: message,
    pendingVariables,
  };
}
