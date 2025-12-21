//! Optimistic Update Hook with Rollback Support
//!
//! Provides optimistic UI updates with automatic rollback on failure.
//! Integrates with React Query for cache management and invalidation.
//!
//! # Usage
//! ```tsx
//! const { update, isUpdating, rollback } = useOptimisticUpdate<Adapter[]>(
//!   ['adapters'],
//!   async (adapters, newAdapter) => {
//!     await api.createAdapter(newAdapter);
//!     return [...adapters, newAdapter];
//!   },
//!   { optimisticFn: (adapters, newAdapter) => [...adapters, newAdapter] }
//! );
//! ```

import { useState, useCallback, useRef } from 'react';
import { useQueryClient, QueryKey } from '@tanstack/react-query';
import { logger, toError } from '@/utils/logger';
import { toast } from 'sonner';

export interface OptimisticUpdateState<TData> {
  /** Whether an optimistic update is in progress */
  isUpdating: boolean;
  /** Whether the update succeeded */
  isSuccess: boolean;
  /** Whether the update failed and was rolled back */
  isRolledBack: boolean;
  /** The error from the last update, if any */
  error: Error | null;
  /** Previous data for manual rollback */
  previousData: TData | null;
  /** Current optimistic data */
  optimisticData: TData | null;
}

export interface UseOptimisticUpdateOptions<TData, TVariables> {
  /** Function to compute optimistic data */
  optimisticFn: (currentData: TData, variables: TVariables) => TData;
  /** Callback when update succeeds */
  onSuccess?: (data: TData, variables: TVariables) => void;
  /** Callback when update fails (before rollback) */
  onError?: (error: Error, variables: TVariables, previousData: TData | null) => void;
  /** Callback after rollback completes */
  onRollback?: (previousData: TData | null, variables: TVariables) => void;
  /** Callback when update settles */
  onSettled?: (data: TData | null, error: Error | null, variables: TVariables) => void;
  /** Component name for logging */
  componentName?: string;
  /** Operation name for logging */
  operationName?: string;
  /** Additional query keys to invalidate on success */
  invalidateKeys?: QueryKey[];
  /** Show success toast automatically */
  successToast?: string;
  /** Show error toast automatically (with rollback info) */
  errorToast?: string | ((error: Error) => string);
  /** Show rollback toast */
  rollbackToast?: string;
  /** Delay before applying optimistic update (ms) - useful for debouncing */
  optimisticDelay?: number;
}

export interface UseOptimisticUpdateReturn<TData, TVariables> extends OptimisticUpdateState<TData> {
  /** Execute the update with optimistic UI */
  update: (variables: TVariables) => Promise<TData | null>;
  /** Manually rollback to previous state */
  rollback: () => void;
  /** Reset the state */
  reset: () => void;
}

const initialState = <TData>(): OptimisticUpdateState<TData> => ({
  isUpdating: false,
  isSuccess: false,
  isRolledBack: false,
  error: null,
  previousData: null,
  optimisticData: null,
});

/**
 * Hook for optimistic UI updates with automatic rollback on failure.
 * Integrates with React Query for cache management.
 *
 * @param queryKey - The React Query key for the data being updated
 * @param updateFn - The async function that performs the actual update
 * @param options - Configuration options
 * @returns State and control functions for optimistic updates
 */
export function useOptimisticUpdate<TData, TVariables = void>(
  queryKey: QueryKey,
  updateFn: (currentData: TData, variables: TVariables) => Promise<TData>,
  options: UseOptimisticUpdateOptions<TData, TVariables>
): UseOptimisticUpdateReturn<TData, TVariables> {
  const {
    optimisticFn,
    onSuccess,
    onError,
    onRollback,
    onSettled,
    componentName = 'useOptimisticUpdate',
    operationName = 'optimistic_update',
    invalidateKeys = [],
    successToast,
    errorToast,
    rollbackToast = 'Changes reverted',
    optimisticDelay = 0,
  } = options;

  const queryClient = useQueryClient();
  const [state, setState] = useState<OptimisticUpdateState<TData>>(initialState);
  const updateFnRef = useRef(updateFn);
  const optimisticFnRef = useRef(optimisticFn);
  updateFnRef.current = updateFn;
  optimisticFnRef.current = optimisticFn;

  // Track in-flight updates for proper rollback
  const inFlightRef = useRef<{
    previousData: TData | null;
    variables: TVariables | null;
    timeoutId: ReturnType<typeof setTimeout> | null;
  }>({
    previousData: null,
    variables: null,
    timeoutId: null,
  });

  const rollback = useCallback(() => {
    const { previousData } = inFlightRef.current;

    if (previousData !== null) {
      // Restore previous cache data
      queryClient.setQueryData(queryKey, previousData);

      setState(prev => ({
        ...prev,
        isUpdating: false,
        isRolledBack: true,
        optimisticData: null,
      }));

      if (rollbackToast) {
        toast.info(rollbackToast);
      }

      if (onRollback && inFlightRef.current.variables !== null) {
        onRollback(previousData, inFlightRef.current.variables);
      }

      logger.info('Optimistic update rolled back', {
        component: componentName,
        operation: operationName,
      });
    }

    // Clear in-flight state
    if (inFlightRef.current.timeoutId) {
      clearTimeout(inFlightRef.current.timeoutId);
    }
    inFlightRef.current = { previousData: null, variables: null, timeoutId: null };
  }, [queryClient, queryKey, rollbackToast, onRollback, componentName, operationName]);

  const update = useCallback(async (variables: TVariables): Promise<TData | null> => {
    // Cancel any pending optimistic update
    if (inFlightRef.current.timeoutId) {
      clearTimeout(inFlightRef.current.timeoutId);
    }

    // Get current data from cache
    const currentData = queryClient.getQueryData<TData>(queryKey);
    if (currentData === undefined) {
      logger.warn('No data in cache for optimistic update', {
        component: componentName,
        operation: operationName,
        queryKey: JSON.stringify(queryKey),
      });
      return null;
    }

    // Store previous data for potential rollback
    inFlightRef.current = {
      previousData: currentData,
      variables,
      timeoutId: null,
    };

    setState(prev => ({
      ...prev,
      isUpdating: true,
      isSuccess: false,
      isRolledBack: false,
      error: null,
      previousData: currentData,
    }));

    // Apply optimistic update
    const applyOptimistic = () => {
      const optimisticData = optimisticFnRef.current(currentData, variables);
      queryClient.setQueryData(queryKey, optimisticData);
      setState(prev => ({ ...prev, optimisticData }));

      logger.debug('Applied optimistic update', {
        component: componentName,
        operation: operationName,
      });
    };

    if (optimisticDelay > 0) {
      inFlightRef.current.timeoutId = setTimeout(applyOptimistic, optimisticDelay);
    } else {
      applyOptimistic();
    }

    try {
      // Execute the actual update
      const result = await updateFnRef.current(currentData, variables);

      // Update succeeded - set final data
      queryClient.setQueryData(queryKey, result);

      // Invalidate related queries
      for (const key of invalidateKeys) {
        queryClient.invalidateQueries({ queryKey: key });
      }

      setState({
        isUpdating: false,
        isSuccess: true,
        isRolledBack: false,
        error: null,
        previousData: currentData,
        optimisticData: null,
      });

      if (successToast) {
        toast.success(successToast);
      }

      if (onSuccess) {
        onSuccess(result, variables);
      }

      logger.info('Optimistic update succeeded', {
        component: componentName,
        operation: operationName,
      });

      if (onSettled) {
        onSettled(result, null, variables);
      }

      // Clear in-flight state
      inFlightRef.current = { previousData: null, variables: null, timeoutId: null };

      return result;
    } catch (err) {
      const error = err instanceof Error ? err : new Error(String(err));

      // Rollback to previous data
      queryClient.setQueryData(queryKey, currentData);

      setState({
        isUpdating: false,
        isSuccess: false,
        isRolledBack: true,
        error,
        previousData: currentData,
        optimisticData: null,
      });

      const errorMessage = typeof errorToast === 'function'
        ? errorToast(error)
        : errorToast;

      // Show either error message OR rollback toast, not both
      if (errorMessage) {
        toast.error(errorMessage);
      } else if (rollbackToast) {
        toast.info(rollbackToast);
      }

      logger.error('Optimistic update failed, rolled back', {
        component: componentName,
        operation: operationName,
      }, toError(err));

      if (onError) {
        onError(error, variables, currentData);
      }

      if (onRollback) {
        onRollback(currentData, variables);
      }

      if (onSettled) {
        onSettled(null, error, variables);
      }

      // Clear in-flight state
      inFlightRef.current = { previousData: null, variables: null, timeoutId: null };

      return null;
    }
  }, [
    queryClient,
    queryKey,
    optimisticDelay,
    invalidateKeys,
    successToast,
    errorToast,
    rollbackToast,
    onSuccess,
    onError,
    onRollback,
    onSettled,
    componentName,
    operationName,
  ]);

  const reset = useCallback(() => {
    if (inFlightRef.current.timeoutId) {
      clearTimeout(inFlightRef.current.timeoutId);
    }
    inFlightRef.current = { previousData: null, variables: null, timeoutId: null };
    setState(initialState());
  }, []);

  return {
    ...state,
    update,
    rollback,
    reset,
  };
}

/**
 * Hook for optimistic list operations (add, remove, update items).
 * Provides convenience methods for common list manipulations.
 *
 * @param queryKey - The React Query key for the list data
 * @param options - Configuration options
 * @returns Methods for optimistic list operations
 */
export function useOptimisticList<TItem extends { id: string | number }>(
  queryKey: QueryKey,
  options: Omit<UseOptimisticUpdateOptions<TItem[], TItem>, 'optimisticFn'> & {
    addFn: (item: TItem) => Promise<TItem>;
    updateFn?: (item: TItem) => Promise<TItem>;
    removeFn?: (id: TItem['id']) => Promise<void>;
  }
) {
  const { addFn, updateFn, removeFn, ...baseOptions } = options;
  const queryClient = useQueryClient();

  const addItem = useOptimisticUpdate<TItem[], TItem>(
    queryKey,
    async (currentData, item) => {
      const newItem = await addFn(item);
      return [...currentData, newItem];
    },
    {
      ...baseOptions,
      optimisticFn: (currentData, item) => [...currentData, item],
      operationName: `${baseOptions.operationName || 'list'}_add`,
    }
  );

  const updateItem = updateFn
    ? useOptimisticUpdate<TItem[], TItem>(
        queryKey,
        async (currentData, item) => {
          const updatedItem = await updateFn(item);
          return currentData.map(i => i.id === updatedItem.id ? updatedItem : i);
        },
        {
          ...baseOptions,
          optimisticFn: (currentData, item) =>
            currentData.map(i => i.id === item.id ? item : i),
          operationName: `${baseOptions.operationName || 'list'}_update`,
        }
      )
    : null;

  const removeItem = removeFn
    ? useOptimisticUpdate<TItem[], TItem['id']>(
        queryKey,
        async (currentData, id) => {
          await removeFn(id);
          return currentData.filter(i => i.id !== id);
        },
        {
          // Spread baseOptions but omit onSuccess/onError/onRollback/onSettled since they have wrong signature
          componentName: baseOptions.componentName,
          operationName: `${baseOptions.operationName || 'list'}_remove`,
          invalidateKeys: baseOptions.invalidateKeys,
          successToast: baseOptions.successToast,
          errorToast: baseOptions.errorToast,
          rollbackToast: baseOptions.rollbackToast,
          optimisticDelay: baseOptions.optimisticDelay,
          optimisticFn: (currentData, id) => currentData.filter(i => i.id !== id),
        }
      )
    : null;

  return {
    add: addItem,
    update: updateItem,
    remove: removeItem,
    // Direct cache access for reading current data
    getData: () => queryClient.getQueryData<TItem[]>(queryKey) ?? [],
  };
}
