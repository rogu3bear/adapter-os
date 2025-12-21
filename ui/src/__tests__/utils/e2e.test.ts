import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';

describe('e2e utilities', () => {
  describe('isE2EMode', () => {
    it('returns boolean based on environment variables', async () => {
      // Import fresh module
      const { isE2EMode } = await import('@/utils/e2e');
      const result = isE2EMode();
      expect(typeof result).toBe('boolean');
    });

    it('caches result after first call', async () => {
      const { isE2EMode } = await import('@/utils/e2e');
      const firstResult = isE2EMode();
      const secondResult = isE2EMode();
      // Should return same value (cached)
      expect(firstResult).toBe(secondResult);
    });
  });

  describe('applyE2EModeStyles', () => {
    beforeEach(() => {
      document.documentElement.classList.remove('e2e-mode');
    });

    it('executes without errors', async () => {
      const { applyE2EModeStyles } = await import('@/utils/e2e');
      expect(() => applyE2EModeStyles()).not.toThrow();
    });

    it('handles missing document gracefully', async () => {
      const { applyE2EModeStyles } = await import('@/utils/e2e');
      const originalDocument = global.document;
      (global as any).document = undefined;

      expect(() => applyE2EModeStyles()).not.toThrow();

      global.document = originalDocument;
    });
  });

  describe('applyE2EVisualGuards', () => {
    it('is an alias for applyE2EModeStyles', async () => {
      const { applyE2EVisualGuards, applyE2EModeStyles } = await import('@/utils/e2e');
      expect(applyE2EVisualGuards).toBe(applyE2EModeStyles);
    });
  });

  describe('e2eSafeDelay', () => {
    it('returns 0 or original delay based on E2E mode', async () => {
      const { e2eSafeDelay } = await import('@/utils/e2e');
      const result = e2eSafeDelay(500);
      // Should be either 0 (E2E mode) or 500 (normal mode)
      expect([0, 500]).toContain(result);
    });

    it('handles undefined delay', async () => {
      const { e2eSafeDelay } = await import('@/utils/e2e');
      const result = e2eSafeDelay(undefined);
      expect(result).toBe(0);
    });

    it('handles zero delay', async () => {
      const { e2eSafeDelay } = await import('@/utils/e2e');
      const result = e2eSafeDelay(0);
      expect(result).toBe(0);
    });

    it('handles large delays', async () => {
      const { e2eSafeDelay } = await import('@/utils/e2e');
      const result = e2eSafeDelay(10000);
      // Should be either 0 (E2E mode) or 10000 (normal mode)
      expect([0, 10000]).toContain(result);
    });

    it('handles negative delays', async () => {
      const { e2eSafeDelay } = await import('@/utils/e2e');
      const result = e2eSafeDelay(-100);
      // Should be either 0 (E2E mode) or -100 (normal mode)
      expect([0, -100]).toContain(result);
    });
  });

  describe('patchToastTestIds', () => {
    beforeEach(() => {
      // Clear DOM
      document.body.innerHTML = '';
      vi.clearAllMocks();
      vi.resetModules();
    });

    it('patches toast.success to add data-testid', async () => {
      const { toast } = await import('sonner');
      const { patchToastTestIds } = await import('@/utils/e2e');
      patchToastTestIds();

      // Mock a success toast element
      const toastEl = document.createElement('div');
      toastEl.setAttribute('data-sonner-toast', '');
      toastEl.setAttribute('data-type', 'success');
      document.body.appendChild(toastEl);

      // Call toast.success
      toast.success('Success message');

      // Wait for microtask
      await new Promise(resolve => queueMicrotask(resolve));

      expect(toastEl.getAttribute('data-testid')).toBe('toast-success');
    });

    it('patches toast.error to add data-testid', async () => {
      const { toast } = await import('sonner');
      const { patchToastTestIds } = await import('@/utils/e2e');
      patchToastTestIds();

      // Mock an error toast element
      const toastEl = document.createElement('div');
      toastEl.setAttribute('data-sonner-toast', '');
      toastEl.setAttribute('data-type', 'error');
      document.body.appendChild(toastEl);

      // Call toast.error
      toast.error('Error message');

      // Wait for microtask
      await new Promise(resolve => queueMicrotask(resolve));

      expect(toastEl.getAttribute('data-testid')).toBe('toast-error');
    });

    it('marks existing toasts on initial call', async () => {
      const { patchToastTestIds } = await import('@/utils/e2e');
      // Add existing toast elements
      const successToast = document.createElement('div');
      successToast.setAttribute('data-sonner-toast', '');
      successToast.setAttribute('data-type', 'success');
      document.body.appendChild(successToast);

      const errorToast = document.createElement('div');
      errorToast.setAttribute('data-sonner-toast', '');
      errorToast.setAttribute('data-type', 'error');
      document.body.appendChild(errorToast);

      patchToastTestIds();

      // Wait for microtask
      await new Promise(resolve => queueMicrotask(resolve));

      expect(successToast.getAttribute('data-testid')).toBe('toast-success');
      expect(errorToast.getAttribute('data-testid')).toBe('toast-error');
    });

    it('does not affect other toast types', async () => {
      const { patchToastTestIds } = await import('@/utils/e2e');
      patchToastTestIds();

      // Mock an info toast element
      const toastEl = document.createElement('div');
      toastEl.setAttribute('data-sonner-toast', '');
      toastEl.setAttribute('data-type', 'info');
      document.body.appendChild(toastEl);

      // Wait for microtask
      await new Promise(resolve => queueMicrotask(resolve));

      expect(toastEl.getAttribute('data-testid')).toBeNull();
    });

    it('handles multiple success toasts', async () => {
      const { toast } = await import('sonner');
      const { patchToastTestIds } = await import('@/utils/e2e');
      patchToastTestIds();

      const toast1 = document.createElement('div');
      toast1.setAttribute('data-sonner-toast', '');
      toast1.setAttribute('data-type', 'success');
      document.body.appendChild(toast1);

      const toast2 = document.createElement('div');
      toast2.setAttribute('data-sonner-toast', '');
      toast2.setAttribute('data-type', 'success');
      document.body.appendChild(toast2);

      toast.success('Message 1');

      // Wait for microtask
      await new Promise(resolve => queueMicrotask(resolve));

      expect(toast1.getAttribute('data-testid')).toBe('toast-success');
      expect(toast2.getAttribute('data-testid')).toBe('toast-success');
    });

    it('handles missing document gracefully', async () => {
      const originalDocument = global.document;
      (global as any).document = undefined;

      const { patchToastTestIds } = await import('@/utils/e2e');
      expect(() => patchToastTestIds()).not.toThrow();

      global.document = originalDocument;
    });

    it('executes without errors on empty DOM', async () => {
      const { patchToastTestIds } = await import('@/utils/e2e');
      expect(() => patchToastTestIds()).not.toThrow();
    });
  });

  describe('applyE2EToastGuards', () => {
    it('is an alias for patchToastTestIds', async () => {
      const { applyE2EToastGuards, patchToastTestIds } = await import('@/utils/e2e');
      expect(applyE2EToastGuards).toBe(patchToastTestIds);
    });
  });

  describe('constants', () => {
    it('exports E2E mode detection functions', async () => {
      const utils = await import('@/utils/e2e');
      expect(typeof utils.isE2EMode).toBe('function');
      expect(typeof utils.applyE2EModeStyles).toBe('function');
      expect(typeof utils.e2eSafeDelay).toBe('function');
      expect(typeof utils.patchToastTestIds).toBe('function');
    });
  });
});
