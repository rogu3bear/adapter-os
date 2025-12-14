import { useEffect } from 'react';

/**
 * Custom hook for keyboard navigation in diff visualization
 *
 * Keyboard shortcuts:
 * - N: Next change
 * - P: Previous change
 * - U: Toggle unified/side-by-side view
 * - C: Copy to clipboard
 */
export function useDiffKeyboardNav(
  onNext: () => void,
  onPrev: () => void,
  onToggleView?: () => void,
  onCopy?: () => void,
  enabled = true
) {
  useEffect(() => {
    if (!enabled) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      // Ignore if user is typing in an input
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) {
        return;
      }

      const key = e.key.toLowerCase();

      switch (key) {
        case 'n':
          e.preventDefault();
          onNext();
          break;
        case 'p':
          e.preventDefault();
          onPrev();
          break;
        case 'u':
          if (onToggleView) {
            e.preventDefault();
            onToggleView();
          }
          break;
        case 'c':
          if (onCopy && (e.metaKey || e.ctrlKey)) {
            e.preventDefault();
            onCopy();
          }
          break;
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [onNext, onPrev, onToggleView, onCopy, enabled]);
}
