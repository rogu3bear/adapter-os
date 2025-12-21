import { useEffect, useState } from 'react';

/**
 * Hook to detect user's motion preferences via prefers-reduced-motion media query.
 *
 * This hook respects the user's system-level preference for reduced motion,
 * which is crucial for accessibility - especially for users with vestibular
 * disorders who can experience motion sickness from animations.
 *
 * @returns {boolean} True if the user prefers reduced motion, false otherwise
 *
 * @example
 * ```tsx
 * function MyComponent() {
 *   const prefersReducedMotion = useReducedMotion();
 *
 *   return (
 *     <div className={prefersReducedMotion ? '' : 'animate-fadeIn'}>
 *       Content
 *     </div>
 *   );
 * }
 * ```
 */
export function useReducedMotion(): boolean {
  // Check if window is defined (SSR safety)
  const getInitialState = (): boolean => {
    if (typeof window === 'undefined' || !window.matchMedia) {
      return false;
    }

    const mediaQuery = window.matchMedia('(prefers-reduced-motion: reduce)');
    return mediaQuery.matches;
  };

  const [prefersReducedMotion, setPrefersReducedMotion] = useState(getInitialState);

  useEffect(() => {
    // Skip if window is not defined (SSR) or matchMedia is not available
    if (typeof window === 'undefined' || !window.matchMedia) {
      return;
    }

    const mediaQuery = window.matchMedia('(prefers-reduced-motion: reduce)');

    // Update state when media query changes
    const handleChange = (event: MediaQueryListEvent | MediaQueryList) => {
      setPrefersReducedMotion(event.matches);
    };

    // Set initial value
    setPrefersReducedMotion(mediaQuery.matches);

    // Listen for changes
    // Use addEventListener for modern browsers, addListener for older ones
    if (mediaQuery.addEventListener) {
      mediaQuery.addEventListener('change', handleChange);
    } else {
      // Fallback for older browsers
      mediaQuery.addListener(handleChange);
    }

    // Cleanup
    return () => {
      if (mediaQuery.removeEventListener) {
        mediaQuery.removeEventListener('change', handleChange);
      } else {
        // Fallback for older browsers
        mediaQuery.removeListener(handleChange);
      }
    };
  }, []);

  return prefersReducedMotion;
}
