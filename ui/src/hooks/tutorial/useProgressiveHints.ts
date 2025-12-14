import { useState, useEffect, useCallback } from 'react';

export interface ProgressiveHint {
  id: string;
  title: string;
  content: string;
  placement?: 'top' | 'bottom' | 'left' | 'right';
  trigger?: 'first-visit' | 'empty-state' | 'before-action' | 'custom';
  condition?: () => boolean;
}

export interface UseProgressiveHintsOptions {
  pageKey: string;
  hints: ProgressiveHint[];
  storagePrefix?: string;
}

export interface ProgressiveHintState {
  hint: ProgressiveHint;
  isVisible: boolean;
  isDismissed: boolean;
}

const DEFAULT_STORAGE_PREFIX = 'aos_hint';

export function useProgressiveHints({ 
  pageKey, 
  hints, 
  storagePrefix = DEFAULT_STORAGE_PREFIX 
}: UseProgressiveHintsOptions) {
  const [visibleHints, setVisibleHints] = useState<ProgressiveHintState[]>([]);
  const [dismissedIds, setDismissedIds] = useState<Set<string>>(new Set());

  // Load dismissed hints from localStorage
  useEffect(() => {
    const dismissed = new Set<string>();
    hints.forEach(hint => {
      const key = `${storagePrefix}_${pageKey}_${hint.id}`;
      const isDismissed = localStorage.getItem(key) === 'true';
      if (isDismissed) {
        dismissed.add(hint.id);
      }
    });
    setDismissedIds(dismissed);
  }, [pageKey, hints, storagePrefix]);

  // Determine which hints should be visible
  useEffect(() => {
    const states: ProgressiveHintState[] = hints.map(hint => {
      const isDismissed = dismissedIds.has(hint.id);
      
      if (isDismissed) {
        return {
          hint,
          isVisible: false,
          isDismissed: true
        };
      }

      // Check trigger conditions
      let shouldShow = false;
      
      switch (hint.trigger) {
        case 'first-visit': {
          const key = `${storagePrefix}_visited_${pageKey}`;
          const hasVisited = localStorage.getItem(key) === 'true';
          if (!hasVisited) {
            localStorage.setItem(key, 'true');
            shouldShow = true;
          }
          break;
        }
        case 'empty-state': {
          // Empty state check handled by condition
          shouldShow = hint.condition?.() ?? false;
          break;
        }
        case 'before-action': {
          // Before action hints handled by explicit show() call
          shouldShow = false;
          break;
        }
        case 'custom': {
          shouldShow = hint.condition?.() ?? false;
          break;
        }
        default: {
          // Default: show if condition passes or no condition
          shouldShow = hint.condition?.() ?? true;
          break;
        }
      }

      return {
        hint,
        isVisible: shouldShow,
        isDismissed: false
      };
    }).filter(state => state.isVisible || state.isDismissed);

    setVisibleHints(states);
  }, [hints, dismissedIds, pageKey, storagePrefix]);

  const dismissHint = useCallback((hintId: string) => {
    const key = `${storagePrefix}_${pageKey}_${hintId}`;
    localStorage.setItem(key, 'true');
    setDismissedIds(prev => new Set([...prev, hintId]));
    setVisibleHints(prev => prev.map(state => 
      state.hint.id === hintId 
        ? { ...state, isVisible: false, isDismissed: true }
        : state
    ));
  }, [pageKey, storagePrefix]);

  const showHint = useCallback((hintId: string) => {
    if (dismissedIds.has(hintId)) return;
    
    setVisibleHints(prev => prev.map(state => 
      state.hint.id === hintId 
        ? { ...state, isVisible: true }
        : state
    ));
  }, [dismissedIds]);

  const getVisibleHint = useCallback((): ProgressiveHintState | undefined => {
    return visibleHints.find(state => state.isVisible);
  }, [visibleHints]);

  return {
    visibleHints: visibleHints.filter(s => s.isVisible),
    dismissedIds,
    dismissHint,
    showHint,
    getVisibleHint,
    hasVisibleHints: visibleHints.some(s => s.isVisible)
  };
}

