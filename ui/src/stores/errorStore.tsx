import React, { createContext, useContext, useState, useCallback, useMemo, useRef, type ReactNode } from 'react';

export type ErrorCategory =
  | 'network'
  | 'auth'
  | 'resource'
  | 'adapter'
  | 'training'
  | 'model'
  | 'upload'
  | 'inference'
  | 'server'
  | 'ui'
  | 'unknown';

export interface CapturedError {
  id: string;
  category: ErrorCategory;
  code?: string;
  message: string;
  stack?: string;
  timestamp: Date;
  context?: Record<string, unknown>;
  component?: string;
  operation?: string;
  httpStatus?: number;
  dismissed: boolean;
}

interface ErrorStoreContextValue {
  errors: CapturedError[];
  maxErrors: number;

  // Actions
  captureError: (error: Partial<CapturedError> & { message: string }) => string | null;
  dismissError: (id: string) => void;
  dismissCategory: (category: ErrorCategory) => void;
  clearAll: () => void;
  clearDismissed: () => void;

  // Selectors
  getByCategory: (category: ErrorCategory) => CapturedError[];
  getCategoryCounts: () => Record<ErrorCategory, number>;
  getActiveCount: () => number;
}

// Categorize error based on code or message
function categorizeError(code?: unknown, message?: string, httpStatus?: number): ErrorCategory {
  const lowerMessage = message?.toLowerCase() || '';
  // Ensure code is a string before calling toLowerCase
  const lowerCode = typeof code === 'string' ? code.toLowerCase() : '';

  // Network errors
  if (lowerCode.includes('network') || lowerCode === 'timeout' ||
      lowerMessage.includes('network') || lowerMessage.includes('fetch') ||
      lowerMessage.includes('connection')) {
    return 'network';
  }

  // Auth errors
  if (lowerCode === 'unauthorized' || lowerCode === 'forbidden' ||
      lowerCode === 'session_expired' || httpStatus === 401 || httpStatus === 403) {
    return 'auth';
  }

  // Resource errors
  if (lowerCode.includes('memory') || lowerCode.includes('disk') ||
      lowerCode === 'resource_busy' || lowerMessage.includes('memory') ||
      lowerMessage.includes('storage')) {
    return 'resource';
  }

  // Adapter errors
  if (lowerCode.includes('adapter') || lowerMessage.includes('adapter')) {
    return 'adapter';
  }

  // Training errors
  if (lowerCode.includes('training') || lowerMessage.includes('training') ||
      lowerMessage.includes('dataset')) {
    return 'training';
  }

  // Model errors
  if (lowerCode.includes('model') || lowerMessage.includes('model')) {
    return 'model';
  }

  // Upload errors
  if (lowerCode.includes('file') || lowerCode.includes('upload') ||
      lowerMessage.includes('upload') || lowerMessage.includes('file size')) {
    return 'upload';
  }

  // Inference errors
  if (lowerCode.includes('inference') || lowerCode.includes('prompt') ||
      lowerMessage.includes('inference') || lowerMessage.includes('generation')) {
    return 'inference';
  }

  // Server errors
  if (httpStatus && httpStatus >= 500) {
    return 'server';
  }

  // UI/React errors
  if (lowerMessage.includes('react') || lowerMessage.includes('render') ||
      lowerMessage.includes('component') || lowerMessage.includes('hook')) {
    return 'ui';
  }

  return 'unknown';
}

function generateId(): string {
  return `err_${Date.now()}_${Math.random().toString(36).slice(2, 9)}`;
}

// Generate a fingerprint for deduplication (same error within 5 seconds = duplicate)
function getErrorFingerprint(error: Partial<CapturedError>): string {
  return `${error.code || ''}_${error.message}_${error.httpStatus || ''}_${error.component || ''}`;
}

const DEDUP_WINDOW_MS = 5000; // 5 seconds

const ErrorStoreContext = createContext<ErrorStoreContextValue | null>(null);

// Global store reference for captureException helper
let globalCaptureError: ErrorStoreContextValue['captureError'] | null = null;

export function ErrorStoreProvider({ children }: { children: ReactNode }) {
  const [errors, setErrors] = useState<CapturedError[]>([]);
  const recentFingerprints = useRef<Map<string, number>>(new Map());
  const maxErrors = 100;

  const captureError = useCallback((errorInput: Partial<CapturedError> & { message: string }) => {
    // Deduplication: check if same error occurred recently
    const fingerprint = getErrorFingerprint(errorInput);
    const now = Date.now();
    const lastSeen = recentFingerprints.current.get(fingerprint);

    if (lastSeen && (now - lastSeen) < DEDUP_WINDOW_MS) {
      // Duplicate within window, skip
      return null;
    }

    // Update fingerprint timestamp
    recentFingerprints.current.set(fingerprint, now);

    // Clean old fingerprints periodically
    if (recentFingerprints.current.size > 100) {
      const cutoff = now - DEDUP_WINDOW_MS;
      for (const [key, timestamp] of recentFingerprints.current) {
        if (timestamp < cutoff) {
          recentFingerprints.current.delete(key);
        }
      }
    }

    const id = generateId();
    const category = errorInput.category || categorizeError(
      errorInput.code,
      errorInput.message,
      errorInput.httpStatus
    );

    const captured: CapturedError = {
      id,
      category,
      message: errorInput.message,
      code: errorInput.code,
      stack: errorInput.stack,
      timestamp: errorInput.timestamp || new Date(),
      context: errorInput.context,
      component: errorInput.component,
      operation: errorInput.operation,
      httpStatus: errorInput.httpStatus,
      dismissed: false,
    };

    setErrors((prev) => {
      const newErrors = [captured, ...prev];
      // Keep only the most recent errors
      if (newErrors.length > maxErrors) {
        newErrors.pop();
      }
      return newErrors;
    });

    // Log in dev mode
    if (import.meta.env.DEV) {
      // eslint-disable-next-line no-console -- intentional dev-mode logging
      console.groupCollapsed(`[ErrorStore] ${category.toUpperCase()}: ${errorInput.message}`);
      // eslint-disable-next-line no-console -- intentional dev-mode logging
      console.log('Error:', captured);
      if (errorInput.stack) {
        // eslint-disable-next-line no-console -- intentional dev-mode logging
        console.log('Stack:', errorInput.stack);
      }
      // eslint-disable-next-line no-console -- intentional dev-mode logging
      console.groupEnd();
    }

    return id;
  }, []);

  // Set global reference
  globalCaptureError = captureError;

  const dismissError = useCallback((id: string) => {
    setErrors((prev) =>
      prev.map((e) => (e.id === id ? { ...e, dismissed: true } : e))
    );
  }, []);

  const dismissCategory = useCallback((category: ErrorCategory) => {
    setErrors((prev) =>
      prev.map((e) => (e.category === category ? { ...e, dismissed: true } : e))
    );
  }, []);

  const clearAll = useCallback(() => {
    setErrors([]);
  }, []);

  const clearDismissed = useCallback(() => {
    setErrors((prev) => prev.filter((e) => !e.dismissed));
  }, []);

  const getByCategory = useCallback(
    (category: ErrorCategory) => errors.filter((e) => e.category === category),
    [errors]
  );

  const getCategoryCounts = useCallback(() => {
    const counts: Record<ErrorCategory, number> = {
      network: 0,
      auth: 0,
      resource: 0,
      adapter: 0,
      training: 0,
      model: 0,
      upload: 0,
      inference: 0,
      server: 0,
      ui: 0,
      unknown: 0,
    };

    for (const error of errors) {
      if (!error.dismissed) {
        counts[error.category]++;
      }
    }

    return counts;
  }, [errors]);

  const getActiveCount = useCallback(
    () => errors.filter((e) => !e.dismissed).length,
    [errors]
  );

  const value = useMemo(
    () => ({
      errors,
      maxErrors,
      captureError,
      dismissError,
      dismissCategory,
      clearAll,
      clearDismissed,
      getByCategory,
      getCategoryCounts,
      getActiveCount,
    }),
    [
      errors,
      captureError,
      dismissError,
      dismissCategory,
      clearAll,
      clearDismissed,
      getByCategory,
      getCategoryCounts,
      getActiveCount,
    ]
  );

  return (
    <ErrorStoreContext.Provider value={value}>
      {children}
    </ErrorStoreContext.Provider>
  );
}

export function useErrorStore(): ErrorStoreContextValue {
  const context = useContext(ErrorStoreContext);
  if (!context) {
    throw new Error('useErrorStore must be used within an ErrorStoreProvider');
  }
  return context;
}

/**
 * Safe version of useErrorStore that returns null if not within ErrorStoreProvider.
 * Use this for optional error tracking in components that may render outside the provider.
 */
export function useErrorStoreSafe(): ErrorStoreContextValue | null {
  return useContext(ErrorStoreContext);
}

// Helper to capture errors from try/catch blocks (works outside React components)
export function captureException(
  error: unknown,
  context?: { component?: string; operation?: string; extra?: Record<string, unknown> }
): string | null {
  if (!globalCaptureError) {
    // eslint-disable-next-line no-console -- warning for misuse before provider mounted
    console.warn('[ErrorStore] captureException called before ErrorStoreProvider mounted');
    return null;
  }

  if (error instanceof Error) {
    const anyError = error as unknown as { code?: unknown; status?: number };
    // Ensure code is a string (DOMException.code is a number)
    const code = typeof anyError.code === 'string' ? anyError.code :
                 typeof anyError.code === 'number' ? String(anyError.code) :
                 error.name || undefined;
    return globalCaptureError({
      message: error.message,
      stack: error.stack,
      code,
      httpStatus: anyError.status,
      component: context?.component,
      operation: context?.operation,
      context: context?.extra,
    });
  }

  return globalCaptureError({
    message: String(error),
    component: context?.component,
    operation: context?.operation,
    context: context?.extra,
  });
}
