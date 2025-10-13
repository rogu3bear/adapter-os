import { useState, useEffect } from 'react';

export interface ProgressiveDisclosureConfig {
  key: string;
  defaultVisible?: boolean;
  persist?: boolean;
}

export function useProgressiveDisclosure(config: ProgressiveDisclosureConfig) {
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

  const toggle = () => setIsVisible(prev => !prev);
  const show = () => setIsVisible(true);
  const hide = () => setIsVisible(false);

  return {
    isVisible,
    toggle,
    show,
    hide,
  };
}
