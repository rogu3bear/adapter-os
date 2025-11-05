import { useState, useEffect, useCallback, useRef } from 'react';
import { logger, toError } from '../utils/logger';

export interface WizardPersistenceConfig<T> {
  /** Unique key for localStorage */
  storageKey: string;
  /** Initial state if no saved state exists */
  initialState: T;
  /** Auto-save on every state change (default: true) */
  autoSave?: boolean;
  /** Debounce delay for auto-save in ms (default: 500) */
  debounceMs?: number;
  /** Callback when saved state is detected on mount */
  onSavedStateDetected?: (savedState: T) => void;
}

export interface WizardPersistenceReturn<T> {
  /** Current state */
  state: T;
  /** Update state (automatically persisted if autoSave is enabled) */
  setState: (updates: Partial<T> | ((prev: T) => T)) => void;
  /** Manually save current state */
  saveState: () => void;
  /** Clear saved state from localStorage */
  clearState: () => void;
  /** Check if saved state exists */
  hasSavedState: boolean;
  /** Load saved state (replaces current state) */
  loadSavedState: () => T | null;
}

/**
 * Hook for persisting wizard state to localStorage with auto-save support
 * 
 * Handles localStorage quota errors gracefully and provides resume functionality
 * 
 * @example
 * ```tsx
 * const { state, setState, clearState, hasSavedState, loadSavedState } = useWizardPersistence({
 *   storageKey: 'training-wizard',
 *   initialState: { name: '', category: null },
 *   onSavedStateDetected: (saved) => {
 *     // Show resume dialog
 *   }
 * });
 * ```
 */
export function useWizardPersistence<T extends Record<string, any>>(
  config: WizardPersistenceConfig<T>
): WizardPersistenceReturn<T> {
  const {
    storageKey,
    initialState,
    autoSave = true,
    debounceMs = 500,
    onSavedStateDetected,
  } = config;

  const fullStorageKey = `aos_wizard_${storageKey}`;
  const [state, setStateInternal] = useState<T>(() => {
    // Load saved state on mount
    try {
      const saved = localStorage.getItem(fullStorageKey);
      if (saved) {
        const parsed = JSON.parse(saved);
        if (onSavedStateDetected) {
          // Use setTimeout to avoid calling during render
          setTimeout(() => onSavedStateDetected(parsed), 0);
        }
        return parsed;
      }
    } catch (err) {
      logger.error(
        'Failed to load saved wizard state',
        { component: 'useWizardPersistence', operation: 'load', storageKey },
        toError(err)
      );
    }
    return initialState;
  });

  const [hasSavedState, setHasSavedState] = useState(() => {
    try {
      return localStorage.getItem(fullStorageKey) !== null;
    } catch {
      return false;
    }
  });

  const debounceTimerRef = useRef<NodeJS.Timeout | null>(null);
  const isInitialMountRef = useRef(true);

  // Check for saved state on mount
  useEffect(() => {
    if (isInitialMountRef.current) {
      isInitialMountRef.current = false;
      try {
        const saved = localStorage.getItem(fullStorageKey);
        setHasSavedState(saved !== null);
      } catch {
        setHasSavedState(false);
      }
    }
  }, [fullStorageKey]);

  const saveStateToStorage = useCallback(
    (stateToSave: T) => {
      try {
        localStorage.setItem(fullStorageKey, JSON.stringify(stateToSave));
        setHasSavedState(true);
      } catch (err: any) {
        // Handle quota exceeded or other storage errors
        if (err?.name === 'QuotaExceededError' || err?.code === 22) {
          logger.warn(
            'localStorage quota exceeded - clearing old wizard states',
            { component: 'useWizardPersistence', operation: 'save', storageKey }
          );
          // Try to clear and retry (could implement LRU eviction here)
          try {
            // Clear this specific state as last resort
            localStorage.removeItem(fullStorageKey);
            logger.warn('Cleared wizard state due to quota', {
              component: 'useWizardPersistence',
              operation: 'quotaCleanup',
              storageKey,
            });
          } catch (clearErr) {
            logger.error(
              'Failed to clear wizard state after quota error',
              { component: 'useWizardPersistence', operation: 'quotaCleanup', storageKey },
              toError(clearErr)
            );
          }
        } else {
          logger.error(
            'Failed to save wizard state',
            { component: 'useWizardPersistence', operation: 'save', storageKey },
            toError(err)
          );
        }
      }
    },
    [fullStorageKey, storageKey]
  );

  const setState = useCallback(
    (updates: Partial<T> | ((prev: T) => T)) => {
      setStateInternal((prev) => {
        const newState =
          typeof updates === 'function' ? updates(prev) : { ...prev, ...updates };

        if (autoSave) {
          // Clear existing debounce timer
          if (debounceTimerRef.current) {
            clearTimeout(debounceTimerRef.current);
          }

          // Debounce auto-save
          debounceTimerRef.current = setTimeout(() => {
            saveStateToStorage(newState);
          }, debounceMs);
        }

        return newState;
      });
    },
    [autoSave, debounceMs, saveStateToStorage]
  );

  const saveState = useCallback(() => {
    // Clear debounce timer to save immediately
    if (debounceTimerRef.current) {
      clearTimeout(debounceTimerRef.current);
      debounceTimerRef.current = null;
    }
    saveStateToStorage(state);
  }, [state, saveStateToStorage]);

  const clearState = useCallback(() => {
    try {
      localStorage.removeItem(fullStorageKey);
      setHasSavedState(false);
    } catch (err) {
      logger.error(
        'Failed to clear wizard state',
        { component: 'useWizardPersistence', operation: 'clear', storageKey },
        toError(err)
      );
    }
  }, [fullStorageKey, storageKey]);

  const loadSavedState = useCallback((): T | null => {
    try {
      const saved = localStorage.getItem(fullStorageKey);
      if (saved) {
        const parsed = JSON.parse(saved);
        setStateInternal(parsed);
        return parsed;
      }
    } catch (err) {
      logger.error(
        'Failed to load saved wizard state',
        { component: 'useWizardPersistence', operation: 'load', storageKey },
        toError(err)
      );
    }
    return null;
  }, [fullStorageKey, storageKey]);

  // Cleanup debounce timer on unmount
  useEffect(() => {
    return () => {
      if (debounceTimerRef.current) {
        clearTimeout(debounceTimerRef.current);
      }
    };
  }, []);

  return {
    state,
    setState,
    saveState,
    clearState,
    hasSavedState,
    loadSavedState,
  };
}

