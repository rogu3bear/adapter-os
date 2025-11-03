//! Cancellable Operation Hook
//!
//! Provides cancellation support for long-running operations with proper cleanup.
//! Integrates with AbortController for request cancellation and operation tracking.
//!
//! Citations:
//! - docs/Smashing Design Techniques.md L1-L50 - Trust-building UX patterns
//! - ui/src/api/client.ts L1-L50 - API client cancellation support

import { useState, useCallback, useRef, useEffect } from 'react';
import { logger } from '../utils/logger';

export interface CancellableOperationState {
  isRunning: boolean;
  isCancelling: boolean;
  error?: Error;
  controller?: AbortController;
}

export interface UseCancellableOperationReturn<T> {
  state: CancellableOperationState;
  start: (operation: (signal: AbortSignal) => Promise<T>, operationName?: string) => Promise<T | undefined>;
  cancel: () => void;
  reset: () => void;
}

/**
 * Hook for managing cancellable operations
 */
export function useCancellableOperation<T = any>(): UseCancellableOperationReturn<T> {
  const [state, setState] = useState<CancellableOperationState>({
    isRunning: false,
    isCancelling: false,
  });

  const controllerRef = useRef<AbortController | null>(null);
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      // Cancel any ongoing operation when component unmounts
      if (controllerRef.current) {
        controllerRef.current.abort();
      }
    };
  }, []);

  const start = useCallback(async (
    operation: (signal: AbortSignal) => Promise<T>,
    operationName: string = 'operation'
  ): Promise<T | undefined> => {
    // Cancel any existing operation
    if (controllerRef.current) {
      controllerRef.current.abort();
    }

    // Create new controller
    const controller = new AbortController();
    controllerRef.current = controller;

    setState({
      isRunning: true,
      isCancelling: false,
      controller,
      error: undefined,
    });

    logger.info('Starting cancellable operation', {
      component: 'useCancellableOperation',
      operation: 'start',
      operationName,
    });

    try {
      const result = await operation(controller.signal);

      // Only update state if component is still mounted
      if (mountedRef.current) {
        setState(prev => ({
          ...prev,
          isRunning: false,
          isCancelling: false,
        }));
      }

      logger.info('Cancellable operation completed successfully', {
        component: 'useCancellableOperation',
        operation: 'start',
        operationName,
      });

      return result;
    } catch (error: any) {
      // Check if this was an abort error
      if (error.name === 'AbortError' || controller.signal.aborted) {
        logger.info('Cancellable operation was cancelled', {
          component: 'useCancellableOperation',
          operation: 'start',
          operationName,
        });

        if (mountedRef.current) {
          setState(prev => ({
            ...prev,
            isRunning: false,
            isCancelling: false,
          }));
        }
        return undefined;
      }

      // Other error
      logger.error('Cancellable operation failed', {
        component: 'useCancellableOperation',
        operation: 'start',
        operationName,
        error: error.message,
      }, error);

      if (mountedRef.current) {
        setState(prev => ({
          ...prev,
          isRunning: false,
          isCancelling: false,
          error,
        }));
      }

      throw error;
    }
  }, []);

  const cancel = useCallback(() => {
    if (controllerRef.current && !controllerRef.current.signal.aborted) {
      logger.info('Cancelling operation', {
        component: 'useCancellableOperation',
        operation: 'cancel',
      });

      controllerRef.current.abort();

      setState(prev => ({
        ...prev,
        isCancelling: true,
      }));
    }
  }, []);

  const reset = useCallback(() => {
    if (controllerRef.current) {
      controllerRef.current.abort();
      controllerRef.current = null;
    }

    setState({
      isRunning: false,
      isCancelling: false,
      error: undefined,
    });

    logger.info('Reset cancellable operation state', {
      component: 'useCancellableOperation',
      operation: 'reset',
    });
  }, []);

  return {
    state,
    start,
    cancel,
    reset,
  };
}

/**
 * Hook for managing multiple concurrent cancellable operations
 */
export function useCancellableOperations() {
  const operations = useRef<Map<string, UseCancellableOperationReturn<any>>>(new Map());

  const register = useCallback(<T = any>(id: string): UseCancellableOperationReturn<T> => {
    if (!operations.current.has(id)) {
      // eslint-disable-next-line react-hooks/rules-of-hooks
      const operation = useCancellableOperation<T>();
      operations.current.set(id, operation);
    }
    return operations.current.get(id)!;
  }, []);

  const cancel = useCallback((id: string) => {
    const operation = operations.current.get(id);
    if (operation) {
      operation.cancel();
    }
  }, []);

  const cancelAll = useCallback(() => {
    for (const operation of operations.current.values()) {
      operation.cancel();
    }
  }, []);

  const reset = useCallback((id: string) => {
    const operation = operations.current.get(id);
    if (operation) {
      operation.reset();
    }
  }, []);

  const resetAll = useCallback(() => {
    for (const operation of operations.current.values()) {
      operation.reset();
    }
  }, []);

  const getState = useCallback((id: string) => {
    const operation = operations.current.get(id);
    return operation?.state;
  }, []);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      cancelAll();
    };
  }, [cancelAll]);

  return {
    register,
    cancel,
    cancelAll,
    reset,
    resetAll,
    getState,
  };
}
