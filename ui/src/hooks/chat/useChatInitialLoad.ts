/**
 * useChatInitialLoad - Coordinated initial load hook for ChatPage
 *
 * Wraps the three initial queries (stacks, default-stack, sessions) with:
 * - Timeout warning after 10 seconds (soft timeout)
 * - Hard timeout error after 30 seconds
 * - Error classification (auth, backend_down, no_workers, system_not_ready, hard_timeout)
 * - Auto-retry for 503 System Not Ready errors (every 5 seconds)
 * - Unified refetchAll function
 *
 * 【2025-01-20†ui-never-spins-forever】
 */

import { useState, useEffect, useCallback, useRef, useMemo } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useAdapterStacks, useGetDefaultStack } from '@/hooks/admin/useAdmin';
import { useChatSessionsApi } from '@/hooks/chat/useChatSessionsApi';
import type { AdapterStack } from '@/api/adapter-types';
import type { ApiError } from '@/api/client';
import { logger } from '@/utils/logger';

// ============================================================================
// Configuration
// ============================================================================

const DEFAULT_TIMEOUT_MS = 10_000; // 10 seconds before showing warning
const DEFAULT_HARD_TIMEOUT_MS = 30_000; // 30 seconds before showing hard timeout error
const SYSTEM_NOT_READY_RETRY_MS = 5_000; // 5 seconds auto-retry for 503

// ============================================================================
// Types
// ============================================================================

export type ChatInitialLoadErrorType =
  | 'auth'
  | 'backend_down'
  | 'no_workers'
  | 'system_not_ready'
  | 'hard_timeout'
  | 'unknown'
  | null;

export interface ChatInitialLoadState {
  // Combined states
  isLoading: boolean;
  isSuccess: boolean;
  isError: boolean;
  isTimedOut: boolean;
  isHardTimedOut: boolean;

  // Error classification
  errorType: ChatInitialLoadErrorType;
  errors: Error[];

  // Data
  stacks: AdapterStack[];
  defaultStack: AdapterStack | null;
  sessionsHook: ReturnType<typeof useChatSessionsApi>;

  // Actions
  refetchAll: () => Promise<void>;

  // For 503 auto-retry
  isAutoRetrying: boolean;
  nextRetryInSeconds: number | null;
}

export interface UseChatInitialLoadOptions {
  /** Soft timeout (warning) in milliseconds (default: 10s) */
  timeoutMs?: number;
  /** Hard timeout (error) in milliseconds (default: 30s) */
  hardTimeoutMs?: number;
  /** Auto-retry interval for 503 errors (default: 5s) */
  systemNotReadyRetryMs?: number;
}

// ============================================================================
// Error Classification
// ============================================================================

function classifyError(error: Error): ChatInitialLoadErrorType {
  const apiError = error as ApiError;
  const status = apiError.status;
  const code = apiError.code;

  // 401 Unauthorized
  if (status === 401) {
    return 'auth';
  }

  // 503 with system not ready indicator
  if (status === 503) {
    if (code === 'SYSTEM_NOT_READY' || code === 'SERVICE_UNAVAILABLE') {
      return 'system_not_ready';
    }
  }

  // Worker unavailable
  if (
    code === 'WORKER_UNAVAILABLE' ||
    code === 'NO_WORKERS' ||
    code === 'NO_WORKER_AVAILABLE'
  ) {
    return 'no_workers';
  }

  // Network/backend issues
  if (
    !navigator.onLine ||
    status === 0 ||
    status === 502 ||
    status === 504 ||
    error.message.includes('fetch') ||
    error.message.includes('network') ||
    error.message.includes('Failed to fetch') ||
    error.message.includes('NetworkError')
  ) {
    return 'backend_down';
  }

  return 'unknown';
}

// ============================================================================
// Hook Implementation
// ============================================================================

export function useChatInitialLoad(
  tenantId: string,
  options: UseChatInitialLoadOptions = {}
): ChatInitialLoadState {
  const {
    timeoutMs = DEFAULT_TIMEOUT_MS,
    hardTimeoutMs = DEFAULT_HARD_TIMEOUT_MS,
    systemNotReadyRetryMs = SYSTEM_NOT_READY_RETRY_MS,
  } = options;

  // -------------------------------------------------------------------------
  // Underlying hooks
  // -------------------------------------------------------------------------

  const queryClient = useQueryClient();
  const stacksQuery = useAdapterStacks();
  const defaultStackQuery = useGetDefaultStack(tenantId);
  const sessionsHook = useChatSessionsApi(tenantId, { sourceType: 'general' });

  // -------------------------------------------------------------------------
  // Timeout tracking
  // -------------------------------------------------------------------------

  const [isTimedOut, setIsTimedOut] = useState(false);
  const [isHardTimedOut, setIsHardTimedOut] = useState(false);
  const timeoutRef = useRef<NodeJS.Timeout | null>(null);
  const hardTimeoutRef = useRef<NodeJS.Timeout | null>(null);
  const loadStartRef = useRef<number>(Date.now());

  // -------------------------------------------------------------------------
  // Auto-retry state for 503
  // -------------------------------------------------------------------------

  const [isAutoRetrying, setIsAutoRetrying] = useState(false);
  const [nextRetryInSeconds, setNextRetryInSeconds] = useState<number | null>(null);
  const autoRetryRef = useRef<NodeJS.Timeout | null>(null);
  const countdownRef = useRef<NodeJS.Timeout | null>(null);

  // -------------------------------------------------------------------------
  // Compute combined states
  // -------------------------------------------------------------------------

  const isLoading = useMemo(() => {
    return stacksQuery.isLoading || defaultStackQuery.isLoading || sessionsHook.isLoading;
  }, [stacksQuery.isLoading, defaultStackQuery.isLoading, sessionsHook.isLoading]);

  const errors = useMemo(() => {
    const errs: Error[] = [];
    if (stacksQuery.error) errs.push(stacksQuery.error);
    if (defaultStackQuery.error) errs.push(defaultStackQuery.error);
    // sessionsHook doesn't expose error directly, but handles it internally
    return errs;
  }, [stacksQuery.error, defaultStackQuery.error]);

  const isError = errors.length > 0;

  const isSuccess = useMemo(() => {
    return (
      stacksQuery.isSuccess &&
      (defaultStackQuery.isSuccess || !tenantId) &&
      !sessionsHook.isLoading &&
      !isError
    );
  }, [
    stacksQuery.isSuccess,
    defaultStackQuery.isSuccess,
    tenantId,
    sessionsHook.isLoading,
    isError,
  ]);

  // Classify error type from first error or hard timeout
  const errorType = useMemo<ChatInitialLoadErrorType>(() => {
    // Hard timeout takes precedence when still loading
    if (isHardTimedOut && isLoading) return 'hard_timeout';
    if (errors.length === 0) return null;
    return classifyError(errors[0]);
  }, [errors, isHardTimedOut, isLoading]);

  // -------------------------------------------------------------------------
  // Data
  // -------------------------------------------------------------------------

  const stacks = stacksQuery.data ?? [];
  const defaultStack = defaultStackQuery.data ?? null;

  // -------------------------------------------------------------------------
  // Timeout effect (soft warning at 10s, hard error at 30s)
  // -------------------------------------------------------------------------

  useEffect(() => {
    // Reset timeout on mount or when loading restarts
    if (isLoading && !isError) {
      loadStartRef.current = Date.now();
      setIsTimedOut(false);
      setIsHardTimedOut(false);

      // Clear existing timeouts
      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current);
      }
      if (hardTimeoutRef.current) {
        clearTimeout(hardTimeoutRef.current);
      }

      // Set soft timeout (warning)
      timeoutRef.current = setTimeout(() => {
        setIsTimedOut(true);
        logger.warn('Chat initial load soft timeout (warning)', {
          component: 'useChatInitialLoad',
          operation: 'softTimeout',
          timeoutMs,
          elapsedMs: Date.now() - loadStartRef.current,
        });
      }, timeoutMs);

      // Set hard timeout (error)
      hardTimeoutRef.current = setTimeout(() => {
        setIsHardTimedOut(true);
        logger.error('Chat initial load hard timeout (error)', {
          component: 'useChatInitialLoad',
          operation: 'hardTimeout',
          hardTimeoutMs,
          elapsedMs: Date.now() - loadStartRef.current,
        });
      }, hardTimeoutMs);
    }

    // Clear timeouts when loading completes or errors
    if (!isLoading || isError) {
      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current);
        timeoutRef.current = null;
      }
      if (hardTimeoutRef.current) {
        clearTimeout(hardTimeoutRef.current);
        hardTimeoutRef.current = null;
      }
      if (!isError) {
        setIsTimedOut(false);
        setIsHardTimedOut(false);
      }
    }

    return () => {
      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current);
      }
      if (hardTimeoutRef.current) {
        clearTimeout(hardTimeoutRef.current);
      }
    };
  }, [isLoading, isError, timeoutMs, hardTimeoutMs]);

  // -------------------------------------------------------------------------
  // Refetch all queries
  // -------------------------------------------------------------------------

  const refetchAll = useCallback(async () => {
    logger.info('Refetching all initial queries', {
      component: 'useChatInitialLoad',
      operation: 'refetchAll',
    });

    // Reset timeout states
    setIsTimedOut(false);
    setIsHardTimedOut(false);
    loadStartRef.current = Date.now();

    // Invalidate sessions query to trigger refetch
    queryClient.invalidateQueries({ queryKey: ['chat-sessions', tenantId] });

    // Refetch all queries
    await Promise.all([stacksQuery.refetch(), defaultStackQuery.refetch()]);
  }, [queryClient, tenantId, stacksQuery, defaultStackQuery]);

  // -------------------------------------------------------------------------
  // Auto-retry for 503 System Not Ready
  // -------------------------------------------------------------------------

  useEffect(() => {
    // Only auto-retry for system_not_ready errors
    if (errorType !== 'system_not_ready') {
      // Clear any existing auto-retry timers
      if (autoRetryRef.current) {
        clearTimeout(autoRetryRef.current);
        autoRetryRef.current = null;
      }
      if (countdownRef.current) {
        clearInterval(countdownRef.current);
        countdownRef.current = null;
      }
      setIsAutoRetrying(false);
      setNextRetryInSeconds(null);
      return;
    }

    // Start auto-retry
    setIsAutoRetrying(true);
    setNextRetryInSeconds(Math.ceil(systemNotReadyRetryMs / 1000));

    // Countdown timer
    countdownRef.current = setInterval(() => {
      setNextRetryInSeconds((prev) => {
        if (prev === null || prev <= 1) return null;
        return prev - 1;
      });
    }, 1000);

    // Schedule retry
    autoRetryRef.current = setTimeout(() => {
      logger.info('Auto-retrying after system_not_ready', {
        component: 'useChatInitialLoad',
        operation: 'autoRetry',
      });
      refetchAll();
    }, systemNotReadyRetryMs);

    return () => {
      if (autoRetryRef.current) {
        clearTimeout(autoRetryRef.current);
      }
      if (countdownRef.current) {
        clearInterval(countdownRef.current);
      }
    };
  }, [errorType, systemNotReadyRetryMs, refetchAll]);

  // -------------------------------------------------------------------------
  // Return state
  // -------------------------------------------------------------------------

  return {
    isLoading,
    isSuccess,
    isError,
    isTimedOut,
    isHardTimedOut,
    errorType,
    errors,
    stacks,
    defaultStack,
    sessionsHook,
    refetchAll,
    isAutoRetrying,
    nextRetryInSeconds,
  };
}

export default useChatInitialLoad;
