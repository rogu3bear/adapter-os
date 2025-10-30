import { useCallback, useEffect, useRef } from 'react';

/**
 * useAnnounce - simple announcer hook for screen readers.
 * Renders to a shared live region element in RootLayout via #sr-announcer.
 */
export function useAnnounce() {
  return useCallback((message: string) => {
    const el = document.getElementById('sr-announcer');
    if (el) {
      // Clear first to retrigger announcement in some SRs
      el.textContent = '';
      // Delay ensures DOM updates register as distinct
      setTimeout(() => { el.textContent = message; }, 10);
    }
  }, []);
}

/**
 * useFocusTrap - traps focus within a container while active.
 * Useful for custom modals or panels.
 */
export function useFocusTrap(containerRef: React.RefObject<HTMLElement>, active: boolean) {
  useEffect(() => {
    if (!active) return;
    const root = containerRef.current;
    if (!root) return;

    const focusable = () => Array.from(
      root.querySelectorAll<HTMLElement>(
        'a[href], button:not([disabled]), textarea, input, select, [tabindex]:not([tabindex="-1"])'
      )
    ).filter(el => !el.hasAttribute('disabled') && el.getAttribute('aria-hidden') !== 'true');

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key !== 'Tab') return;
      const list = focusable();
      if (list.length === 0) return;
      const first = list[0];
      const last = list[list.length - 1];
      const current = document.activeElement as HTMLElement | null;
      if (e.shiftKey) {
        if (current === first || !root.contains(current)) {
          last.focus();
          e.preventDefault();
        }
      } else {
        if (current === last) {
          first.focus();
          e.preventDefault();
        }
      }
    };

    root.addEventListener('keydown', handleKeyDown);
    return () => root.removeEventListener('keydown', handleKeyDown);
  }, [containerRef, active]);
}

/**
 * useFocusRestore - capture current focus and restore on cleanup.
 */
export function useFocusRestore(active: boolean) {
  const lastFocusedRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    if (active) {
      lastFocusedRef.current = document.activeElement as HTMLElement | null;
    }
    return () => {
      if (active && lastFocusedRef.current) {
        try {
          lastFocusedRef.current.focus();
        } catch {}
      }
    };
  }, [active]);
}

/**
 * useKeyboardShortcuts - register global keyboard shortcuts with cleanup.
 * Supports '/', '?' with provided callbacks.
 */
export function useKeyboardShortcuts(handlers: {
  onSearch?: () => void;
  onHelp?: () => void;
}) {
  const { onSearch, onHelp } = handlers;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // Ignore if typing in input/textarea/select or with modifiers
      const tag = (e.target as HTMLElement)?.tagName?.toLowerCase();
      if (e.altKey || e.ctrlKey || e.metaKey) return;
      if (tag === 'input' || tag === 'textarea' || tag === 'select' || (e.target as HTMLElement)?.isContentEditable) {
        return;
      }
      if (e.key === '/' && onSearch) {
        e.preventDefault();
        onSearch();
      }
      if ((e.key === '?' || (e.shiftKey && e.key === '/')) && onHelp) {
        e.preventDefault();
        onHelp();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onSearch, onHelp]);
}

