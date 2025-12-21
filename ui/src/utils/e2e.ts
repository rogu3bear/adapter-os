/**
 * E2E helpers
 *
 * Centralizes detection of E2E mode and small utilities that help
 * stabilize Cypress selectors (e.g., toast markers, animation shutoff).
 */

import { toast } from 'sonner';

let e2eFlag: boolean | null = null;

export function isE2EMode(): boolean {
  if (e2eFlag !== null) return e2eFlag;
  const env = import.meta.env as Record<string, string | boolean | undefined>;
  const flag =
    env.VITE_E2E_MODE === '1' ||
    env.VITE_E2E_MODE === 'true' ||
    env.E2E_MODE === '1' ||
    env.E2E_MODE === 'true' ||
    // Allow overriding via window for test harnesses
    (typeof window !== 'undefined' && (window as unknown as { __AOS_E2E__?: boolean }).__AOS_E2E__ === true);
  e2eFlag = Boolean(flag);
  return e2eFlag;
}

/** Disable animations/transitions globally to reduce visual jitter during E2E. */
export function applyE2EModeStyles() {
  if (!isE2EMode()) return;
  if (typeof document === 'undefined') return;
  document.documentElement.classList.add('e2e-mode');
}

/**
 * Marks the document for E2E runs so global CSS can disable
 * animations/transitions. (Alias for compatibility)
 */
export const applyE2EVisualGuards = applyE2EModeStyles;

/**
 * Returns 0ms delays when E2E mode is enabled to remove debounce jitter.
 */
export const e2eSafeDelay = (delay: number | undefined): number => {
  if (!isE2EMode()) return delay ?? 0;
  return 0;
};

let toastsPatched = false;

/**
 * Tag success/error toasts with data-testid attributes so Cypress can rely on them.
 * Uses Sonner's data attributes; no DOM structure assumptions beyond data-sonner-toast.
 */
export function patchToastTestIds() {
  if (toastsPatched) return;
  toastsPatched = true;

  const markToasts = () => {
    if (typeof document === 'undefined') return;
    document
      .querySelectorAll<HTMLElement>('[data-sonner-toast][data-type="success"]')
      .forEach(el => el.setAttribute('data-testid', 'toast-success'));
    document
      .querySelectorAll<HTMLElement>('[data-sonner-toast][data-type="error"]')
      .forEach(el => el.setAttribute('data-testid', 'toast-error'));
  };

  const wrap =
    <T extends (...args: any[]) => any>(fn: T, type: 'success' | 'error') =>
    (...args: Parameters<T>): ReturnType<T> => {
      const result = fn(...args);
      // Next tick to ensure toast DOM is rendered
      queueMicrotask(markToasts);
      return result;
    };

  toast.success = wrap(toast.success, 'success');
  toast.error = wrap(toast.error, 'error');

  // Mark any existing toasts (dev hot reload / storybook)
  queueMicrotask(markToasts);
}

/**
 * Patch toast helpers so E2E selectors can target success/error toasts.
 * Applied once at startup; no-ops outside E2E mode. (Alias for compatibility)
 */
export const applyE2EToastGuards = patchToastTestIds;
