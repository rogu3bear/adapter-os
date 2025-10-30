//! View Transitions API Hook
//!
//! Provides smooth page transitions using the View Transitions API with fallbacks.
//!
//! Citations:
//! - docs/Web Animations API.md L1-L100 - View transitions specification
//! - ui/src/layout/RootLayout.tsx L200-L250 - Current navigation implementation

import React from 'react';
import { useNavigate } from 'react-router-dom';

/**
 * Hook for smooth page transitions using View Transitions API
 *
 * Provides seamless navigation with visual transitions between pages.
 * Falls back gracefully for browsers without View Transitions support.
 *
 * @returns {Function} transitionTo - Function to navigate with transition
 */
export function useViewTransition() {
  const navigate = useNavigate();

  const transitionTo = (path: string, options?: { state?: any; replace?: boolean }) => {
    // Check if View Transitions API is supported
    if ('startViewTransition' in document) {
      try {
        // Use View Transitions API for smooth transition
        document.startViewTransition(() => {
          navigate(path, options);
        });
      } catch (error) {
        // Fallback to regular navigation if transition fails
        console.warn('View transition failed, falling back to regular navigation:', error);
        navigate(path, options);
      }
    } else {
      // Fallback for browsers without View Transitions API
      navigate(path, options);
    }
  };

  return transitionTo;
}

/**
 * Utility function for programmatic view transitions
 *
 * Can be used outside of React components for imperative transitions.
 */
export function createViewTransition(
  updateCallback: () => void,
  options?: { classNames?: string[] }
): Promise<void> {
  return new Promise((resolve) => {
    if ('startViewTransition' in document) {
      const transition = document.startViewTransition(() => {
        updateCallback();
        resolve();
      });

      // Add custom class names if provided
      if (options?.classNames) {
        transition.ready.then(() => {
          const pseudoElements = [
            '::view-transition-old(root)',
            '::view-transition-new(root)'
          ];

          pseudoElements.forEach(pseudo => {
            const element = document.querySelector(pseudo) as HTMLElement;
            if (element) {
              element.classList.add(...options.classNames!);
            }
          });
        });
      }
    } else {
      // Fallback - execute immediately
      updateCallback();
      resolve();
    }
  });
}

/**
 * Hook for transition-aware state updates
 *
 * Wraps state updates with view transitions for smooth visual changes.
 */
export function useTransitionState<T>(
  initialState: T
): [T, (updater: T | ((prev: T) => T)) => Promise<void>] {
  const [state, setState] = React.useState(initialState);

  const transitionSetState = React.useCallback(
    (updater: T | ((prev: T) => T)): Promise<void> => {
      return new Promise((resolve) => {
        const newState = typeof updater === 'function'
          ? (updater as (prev: T) => T)(state)
          : updater;

        if ('startViewTransition' in document) {
          document.startViewTransition(() => {
            setState(newState);
            resolve();
          });
        } else {
          setState(newState);
          resolve();
        }
      });
    },
    [state]
  );

  return [state, transitionSetState];
}
