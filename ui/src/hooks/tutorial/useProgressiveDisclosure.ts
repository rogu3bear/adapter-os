import { useState, useEffect } from 'react';

export interface ProgressiveDisclosureConfig {
  key: string;
  defaultVisible?: boolean;
  persist?: boolean;
}


export interface UseProgressiveDisclosureReturn {
  isVisible: boolean;
  toggle: () => void;
  show: () => void;
  hide: () => void;
}

/**
 * Hook for managing progressive disclosure state with optional localStorage persistence.
 *
 * @param config - Configuration object
 * @param config.key - Unique key for localStorage persistence
 * @param config.defaultVisible - Initial visibility state (default: false)
 * @param config.persist - Whether to persist state in localStorage (default: true)
 * @returns Object with visibility state and control functions
 */
export function useProgressiveDisclosure(config: ProgressiveDisclosureConfig): UseProgressiveDisclosureReturn {
  const { key, defaultVisible = false, persist = true } = config;
  
  // Get initial state from localStorage if persistence is enabled
  const getInitialState = () => {
    if (persist) {
      const saved = localStorage.getItem(`progressive-disclosure-${key}`);
      return saved ? JSON.parse(saved) : defaultVisible;
    }
    return defaultVisible;
  };

  const [isVisible, setIsVisible] = useState(getInitialState);

  // Persist state changes to localStorage
  useEffect(() => {
    if (persist) {
      localStorage.setItem(`progressive-disclosure-${key}`, JSON.stringify(isVisible));
    }
  }, [isVisible, key, persist]);

  const toggle = () => setIsVisible((prev: boolean) => !prev);
  const show = () => setIsVisible(true);
  const hide = () => setIsVisible(false);

  return {
    isVisible,
    toggle,
    show,
    hide,
  };
}
