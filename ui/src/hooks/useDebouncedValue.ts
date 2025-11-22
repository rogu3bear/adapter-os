//! Debounced Value Hook for Search Inputs
//!
//! Provides debounced values for search inputs and other rapidly changing values.
//! Integrates with React Query for search-as-you-type patterns.
//!
//! # Usage
//! ```tsx
//! const [search, setSearch] = useState('');
//! const debouncedSearch = useDebouncedValue(search, 300);
//!
//! // Use with React Query
//! const { data } = useQuery({
//!   queryKey: ['search', debouncedSearch],
//!   queryFn: () => api.search(debouncedSearch),
//!   enabled: debouncedSearch.length > 2,
//! });
//! ```

import { useState, useEffect, useCallback, useRef, useMemo } from 'react';
import { useQueryClient } from '@tanstack/react-query';

export interface UseDebouncedValueOptions {
  /** Delay in milliseconds (default: 300) */
  delay?: number;
  /** Leading edge - trigger immediately on first change */
  leading?: boolean;
  /** Trailing edge - trigger after delay (default: true) */
  trailing?: boolean;
  /** Maximum wait time before forcing execution (useful for long typing) */
  maxWait?: number;
  /** Callback when debounced value changes */
  onChange?: (value: string) => void;
}

export interface UseDebouncedValueReturn<T> {
  /** The debounced value */
  debouncedValue: T;
  /** Whether a debounce is pending */
  isPending: boolean;
  /** Cancel the pending debounce */
  cancel: () => void;
  /** Flush the debounce immediately */
  flush: () => void;
}

/**
 * Hook that debounces a value, delaying updates until after a specified delay.
 * Useful for search inputs to avoid excessive API calls.
 *
 * @param value - The value to debounce
 * @param delayOrOptions - Delay in ms or options object
 * @returns The debounced value and control functions
 */
export function useDebouncedValue<T>(
  value: T,
  delayOrOptions: number | UseDebouncedValueOptions = 300
): UseDebouncedValueReturn<T> {
  const options = typeof delayOrOptions === 'number'
    ? { delay: delayOrOptions }
    : delayOrOptions;

  const {
    delay = 300,
    leading = false,
    trailing = true,
    maxWait,
    onChange,
  } = options;

  const [debouncedValue, setDebouncedValue] = useState<T>(value);
  const [isPending, setIsPending] = useState(false);

  const timeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const maxWaitTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const lastCallTimeRef = useRef<number>(0);
  const lastInvokeTimeRef = useRef<number>(0);
  const pendingValueRef = useRef<T>(value);
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;

  const invokeFunc = useCallback((newValue: T) => {
    lastInvokeTimeRef.current = Date.now();
    setDebouncedValue(newValue);
    setIsPending(false);
    if (onChangeRef.current) {
      onChangeRef.current(String(newValue));
    }
  }, []);

  const cancel = useCallback(() => {
    if (timeoutRef.current) {
      clearTimeout(timeoutRef.current);
      timeoutRef.current = null;
    }
    if (maxWaitTimeoutRef.current) {
      clearTimeout(maxWaitTimeoutRef.current);
      maxWaitTimeoutRef.current = null;
    }
    setIsPending(false);
  }, []);

  const flush = useCallback(() => {
    if (timeoutRef.current) {
      clearTimeout(timeoutRef.current);
      timeoutRef.current = null;
    }
    if (maxWaitTimeoutRef.current) {
      clearTimeout(maxWaitTimeoutRef.current);
      maxWaitTimeoutRef.current = null;
    }
    invokeFunc(pendingValueRef.current);
  }, [invokeFunc]);

  useEffect(() => {
    const now = Date.now();
    lastCallTimeRef.current = now;
    pendingValueRef.current = value;

    const isFirstCall = debouncedValue === undefined;
    const timeSinceLastInvoke = now - lastInvokeTimeRef.current;

    // Leading edge
    if (leading && (isFirstCall || timeSinceLastInvoke >= delay)) {
      invokeFunc(value);
      return;
    }

    setIsPending(true);

    // Clear existing timeout
    if (timeoutRef.current) {
      clearTimeout(timeoutRef.current);
    }

    // Trailing edge
    if (trailing) {
      timeoutRef.current = setTimeout(() => {
        invokeFunc(pendingValueRef.current);
        timeoutRef.current = null;
      }, delay);
    }

    // Max wait
    if (maxWait !== undefined && !maxWaitTimeoutRef.current) {
      const remainingWait = maxWait - timeSinceLastInvoke;
      if (remainingWait > 0) {
        maxWaitTimeoutRef.current = setTimeout(() => {
          invokeFunc(pendingValueRef.current);
          maxWaitTimeoutRef.current = null;
        }, remainingWait);
      }
    }

    return () => {
      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current);
      }
    };
  }, [value, delay, leading, trailing, maxWait, invokeFunc, debouncedValue]);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current);
      }
      if (maxWaitTimeoutRef.current) {
        clearTimeout(maxWaitTimeoutRef.current);
      }
    };
  }, []);

  return {
    debouncedValue,
    isPending,
    cancel,
    flush,
  };
}

/**
 * Hook for debounced search with React Query integration.
 * Automatically prefetches results and manages search state.
 *
 * @param options - Configuration options
 * @returns Search state and handlers
 */
export function useDebouncedSearch<TData>(options: {
  /** Query key prefix for the search */
  queryKey: string[];
  /** Function to fetch search results */
  searchFn: (query: string) => Promise<TData>;
  /** Debounce delay in ms */
  delay?: number;
  /** Minimum query length to trigger search */
  minLength?: number;
  /** Enable prefetching of likely next queries */
  prefetch?: boolean;
  /** Initial search value */
  initialValue?: string;
  /** Callback when search value changes */
  onSearch?: (query: string) => void;
}) {
  const {
    queryKey,
    searchFn,
    delay = 300,
    minLength = 1,
    prefetch = false,
    initialValue = '',
    onSearch,
  } = options;

  const queryClient = useQueryClient();
  const [inputValue, setInputValue] = useState(initialValue);
  const searchFnRef = useRef(searchFn);
  searchFnRef.current = searchFn;

  const { debouncedValue, isPending, cancel, flush } = useDebouncedValue(inputValue, {
    delay,
    onChange: onSearch,
  });

  const isEnabled = debouncedValue.length >= minLength;

  // Memoize the full query key
  const fullQueryKey = useMemo(
    () => [...queryKey, debouncedValue],
    [queryKey, debouncedValue]
  );

  // Prefetch likely next queries
  useEffect(() => {
    if (prefetch && isEnabled && debouncedValue.length > 0) {
      // Prefetch with one more character removed (backspace)
      const shorterQuery = debouncedValue.slice(0, -1);
      if (shorterQuery.length >= minLength) {
        queryClient.prefetchQuery({
          queryKey: [...queryKey, shorterQuery],
          queryFn: () => searchFnRef.current(shorterQuery),
          staleTime: 60000, // 1 minute
        });
      }
    }
  }, [prefetch, isEnabled, debouncedValue, queryKey, minLength, queryClient]);

  const clear = useCallback(() => {
    setInputValue('');
    cancel();
  }, [cancel]);

  const handleChange = useCallback((value: string) => {
    setInputValue(value);
  }, []);

  return {
    /** Current input value */
    inputValue,
    /** Debounced search value */
    searchValue: debouncedValue,
    /** Whether debounce is pending */
    isPending,
    /** Whether search is enabled (meets minLength) */
    isEnabled,
    /** Full query key for React Query */
    queryKey: fullQueryKey,
    /** Set input value */
    setInputValue: handleChange,
    /** Clear search */
    clear,
    /** Cancel pending debounce */
    cancel,
    /** Flush debounce immediately */
    flush,
  };
}

/**
 * Simple hook that just returns a debounced value.
 * Use this when you don't need the control functions.
 *
 * @param value - The value to debounce
 * @param delay - Delay in milliseconds
 * @returns The debounced value
 */
export function useDebounce<T>(value: T, delay: number = 300): T {
  const [debouncedValue, setDebouncedValue] = useState<T>(value);

  useEffect(() => {
    const timer = setTimeout(() => {
      setDebouncedValue(value);
    }, delay);

    return () => {
      clearTimeout(timer);
    };
  }, [value, delay]);

  return debouncedValue;
}

/**
 * Hook for debounced callback execution.
 * Returns a debounced version of the callback.
 *
 * @param callback - The callback to debounce
 * @param delay - Delay in milliseconds
 * @returns Debounced callback with cancel and flush methods
 */
export function useDebouncedCallback<T extends (...args: Parameters<T>) => ReturnType<T>>(
  callback: T,
  delay: number = 300
): {
  debouncedFn: (...args: Parameters<T>) => void;
  cancel: () => void;
  flush: () => void;
  isPending: boolean;
} {
  const callbackRef = useRef(callback);
  callbackRef.current = callback;

  const timeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const argsRef = useRef<Parameters<T> | null>(null);
  const [isPending, setIsPending] = useState(false);

  const cancel = useCallback(() => {
    if (timeoutRef.current) {
      clearTimeout(timeoutRef.current);
      timeoutRef.current = null;
    }
    argsRef.current = null;
    setIsPending(false);
  }, []);

  const flush = useCallback(() => {
    if (timeoutRef.current && argsRef.current) {
      clearTimeout(timeoutRef.current);
      timeoutRef.current = null;
      callbackRef.current(...argsRef.current);
      argsRef.current = null;
      setIsPending(false);
    }
  }, []);

  const debouncedFn = useCallback((...args: Parameters<T>) => {
    argsRef.current = args;
    setIsPending(true);

    if (timeoutRef.current) {
      clearTimeout(timeoutRef.current);
    }

    timeoutRef.current = setTimeout(() => {
      if (argsRef.current) {
        callbackRef.current(...argsRef.current);
        argsRef.current = null;
      }
      timeoutRef.current = null;
      setIsPending(false);
    }, delay);
  }, [delay]);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current);
      }
    };
  }, []);

  return {
    debouncedFn,
    cancel,
    flush,
    isPending,
  };
}
