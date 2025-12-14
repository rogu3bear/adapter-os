import { useCallback, useEffect, useMemo, useState } from 'react';

const STORAGE_KEY = 'aos_layout_debug';

const normalizeFlag = (value: string | null): boolean => {
  if (!value) return false;
  const normalized = value.toLowerCase();
  return normalized === '1' || normalized === 'true' || normalized === 'yes' || normalized === 'on';
};

/**
 * Dev-only layout debug hook.
 * - Gate on import.meta.env.DEV to avoid shipping to prod bundles.
 * - Toggle via localStorage or ?layoutDebug=true query param.
 */
export function useLayoutDebug() {
  const [enabled, setEnabled] = useState(false);

  useEffect(() => {
    if (!import.meta.env.DEV) return;

    let initial = false;
    try {
      const params = new URLSearchParams(window.location.search);
      const paramValue = params.get('layoutDebug');
      const storedValue = localStorage.getItem(STORAGE_KEY);

      if (paramValue !== null) {
        initial = normalizeFlag(paramValue);
      } else if (storedValue !== null) {
        initial = normalizeFlag(storedValue);
      }
    } catch {
      initial = false;
    }

    setEnabled(initial);
  }, []);

  useEffect(() => {
    if (!import.meta.env.DEV) return;
    try {
      localStorage.setItem(STORAGE_KEY, enabled ? '1' : '0');
    } catch {
      // ignore storage errors in dev
    }
  }, [enabled]);

  const toggle = useCallback(() => {
    if (!import.meta.env.DEV) return;
    setEnabled((prev) => !prev);
  }, []);

  const value = useMemo(
    () => ({
      enabled: import.meta.env.DEV && enabled,
      toggle,
      setEnabled,
    }),
    [enabled, toggle],
  );

  // Expose a simple global toggle in dev for quick switching
  useEffect(() => {
    if (!import.meta.env.DEV) return;
    (window as typeof window & { __toggleLayoutDebug?: () => void }).__toggleLayoutDebug = () => {
      setEnabled((prev) => !prev);
    };
  }, []);

  return value;
}

