/**
 * useChatLoadingPersistence - SessionStorage persistence for loading state recovery
 *
 * Enables recovery of loading state after page refresh. Uses sessionStorage
 * to track in-progress adapter loading operations and restore them on mount.
 *
 * @example
 * ```tsx
 * const { persistedState, persist, clear } = useChatLoadingPersistence({
 *   stackId: 'my-stack',
 *   enabled: true,
 * });
 *
 * // On loading start
 * persist({
 *   stackId: 'my-stack',
 *   startedAt: Date.now(),
 *   adaptersToLoad: ['adapter-1', 'adapter-2'],
 *   lastUpdated: Date.now(),
 * });
 *
 * // On loading complete/error
 * clear();
 *
 * // On mount: check persistedState for recovery
 * if (persistedState && isSameStack && isRecent) {
 *   // Resume loading...
 * }
 * ```
 */

import { useState, useEffect, useCallback, useRef } from 'react';
import { logger, toError } from '@/utils/logger';
import { getWorkspaceScopedKey } from '@/utils/storage';

// ============================================================================
// Constants
// ============================================================================

const BASE_STORAGE_KEY = 'aos_chat_loading_state';
const LEGACY_STORAGE_KEY = 'aos_chat_loading_state'; // For migration
const MAX_RECOVERY_AGE_MS = 5 * 60 * 1000; // 5 minutes

// ============================================================================
// Types
// ============================================================================

/**
 * Persisted loading state
 */
export interface ChatLoadingState {
  /** Stack ID being loaded */
  stackId: string;
  /** Timestamp when loading started */
  startedAt: number;
  /** List of adapter IDs being loaded */
  adaptersToLoad: string[];
  /** Last update timestamp */
  lastUpdated: number;
}

/**
 * Hook configuration options
 */
export interface UseChatLoadingPersistenceOptions {
  /** Current stack ID (used for validation on recovery) */
  stackId?: string;
  /** Workspace ID for storage isolation */
  workspaceId?: string;
  /** Enable persistence (default: true) */
  enabled?: boolean;
}

/**
 * Hook return value
 */
export interface UseChatLoadingPersistenceReturn {
  /** Persisted state from sessionStorage (null if none or invalid) */
  persistedState: ChatLoadingState | null;
  /** Save loading state to sessionStorage */
  persist: (state: ChatLoadingState) => void;
  /** Clear persisted state from sessionStorage */
  clear: () => void;
  /** Check if persisted state is valid for recovery */
  isRecoverable: boolean;
}

// ============================================================================
// Helper Functions
// ============================================================================

/**
 * Safely read from sessionStorage
 */
function readFromStorage(storageKey: string): ChatLoadingState | null {
  try {
    const raw = sessionStorage.getItem(storageKey);
    if (!raw) {
      return null;
    }

    const parsed = JSON.parse(raw) as ChatLoadingState;

    // Validate structure
    if (
      typeof parsed.stackId !== 'string' ||
      typeof parsed.startedAt !== 'number' ||
      typeof parsed.lastUpdated !== 'number' ||
      !Array.isArray(parsed.adaptersToLoad)
    ) {
      logger.warn('Invalid persisted loading state structure', {
        component: 'useChatLoadingPersistence',
        operation: 'read',
      });
      return null;
    }

    return parsed;
  } catch (err) {
    logger.error(
      'Failed to read persisted loading state',
      { component: 'useChatLoadingPersistence', operation: 'read' },
      toError(err)
    );
    return null;
  }
}

/**
 * Safely write to sessionStorage
 */
function writeToStorage(storageKey: string, state: ChatLoadingState): boolean {
  try {
    sessionStorage.setItem(storageKey, JSON.stringify(state));
    return true;
  } catch (err: unknown) {
    // Handle quota exceeded or other storage errors
    const error = err as { name?: string; code?: number };
    if (error?.name === 'QuotaExceededError' || error?.code === 22) {
      logger.warn(
        'sessionStorage quota exceeded - continuing without persistence',
        { component: 'useChatLoadingPersistence', operation: 'write' }
      );
    } else {
      logger.error(
        'Failed to persist loading state',
        { component: 'useChatLoadingPersistence', operation: 'write' },
        toError(err)
      );
    }
    return false;
  }
}

/**
 * Safely clear from sessionStorage
 */
function clearFromStorage(storageKey: string): void {
  try {
    sessionStorage.removeItem(storageKey);
  } catch (err) {
    logger.error(
      'Failed to clear persisted loading state',
      { component: 'useChatLoadingPersistence', operation: 'clear' },
      toError(err)
    );
  }
}

/**
 * Migrate data from legacy global key to workspace-scoped key
 */
function migrateFromLegacyStorage(workspaceScopedKey: string): ChatLoadingState | null {
  try {
    const legacyData = sessionStorage.getItem(LEGACY_STORAGE_KEY);
    if (!legacyData) {
      return null;
    }

    const parsed = JSON.parse(legacyData) as ChatLoadingState;

    // Validate structure before migrating
    if (
      typeof parsed.stackId !== 'string' ||
      typeof parsed.startedAt !== 'number' ||
      typeof parsed.lastUpdated !== 'number' ||
      !Array.isArray(parsed.adaptersToLoad)
    ) {
      // Invalid legacy data, just remove it
      sessionStorage.removeItem(LEGACY_STORAGE_KEY);
      return null;
    }

    // Write to new workspace-scoped key
    sessionStorage.setItem(workspaceScopedKey, legacyData);

    // Remove legacy key
    sessionStorage.removeItem(LEGACY_STORAGE_KEY);

    logger.info('Migrated loading state from legacy to workspace-scoped storage', {
      component: 'useChatLoadingPersistence',
      operation: 'migrate',
      stackId: parsed.stackId,
    });

    return parsed;
  } catch (err) {
    logger.error(
      'Failed to migrate legacy loading state',
      { component: 'useChatLoadingPersistence', operation: 'migrate' },
      toError(err)
    );
    return null;
  }
}

/**
 * Check if a persisted state is recent enough to recover
 */
function isStateRecent(state: ChatLoadingState): boolean {
  const now = Date.now();
  const age = now - state.lastUpdated;
  return age < MAX_RECOVERY_AGE_MS;
}

/**
 * Check if persisted state matches current stack
 */
function isStateForStack(state: ChatLoadingState, stackId?: string): boolean {
  if (!stackId) {
    return false;
  }
  return state.stackId === stackId;
}

// ============================================================================
// Hook Implementation
// ============================================================================

/**
 * SessionStorage persistence for loading state recovery
 *
 * Features:
 * - Persists loading state to sessionStorage on start
 * - Recovers state on mount if < 5 minutes old and same stack
 * - Clears state on completion or error
 * - Handles storage quota errors gracefully (continues without persistence)
 * - Validates state structure and age before recovery
 */
export function useChatLoadingPersistence(
  options: UseChatLoadingPersistenceOptions = {}
): UseChatLoadingPersistenceReturn {
  const { stackId, workspaceId, enabled = true } = options;

  const [persistedState, setPersistedState] = useState<ChatLoadingState | null>(null);
  const [isRecoverable, setIsRecoverable] = useState(false);
  const isInitialMountRef = useRef(true);

  // Get workspace-scoped storage key
  const storageKey = getWorkspaceScopedKey(workspaceId || 'default', BASE_STORAGE_KEY);

  // Load persisted state on mount (with migration from legacy key)
  useEffect(() => {
    if (!enabled || !isInitialMountRef.current) {
      return;
    }

    isInitialMountRef.current = false;

    // Try to read from workspace-scoped key first
    let state = readFromStorage(storageKey);

    // If not found, try migrating from legacy key
    if (!state) {
      state = migrateFromLegacyStorage(storageKey);
    }

    if (!state) {
      logger.debug('No persisted loading state found', {
        component: 'useChatLoadingPersistence',
        operation: 'mount',
      });
      return;
    }

    // Validate state is recent and for current stack
    const isRecent = isStateRecent(state);
    const isCorrectStack = isStateForStack(state, stackId);

    if (!isRecent) {
      logger.debug('Persisted loading state is too old, clearing', {
        component: 'useChatLoadingPersistence',
        operation: 'mount',
        age: Date.now() - state.lastUpdated,
      });
      clearFromStorage(storageKey);
      return;
    }

    if (!isCorrectStack) {
      logger.debug('Persisted loading state is for different stack, ignoring', {
        component: 'useChatLoadingPersistence',
        operation: 'mount',
        persistedStackId: state.stackId,
        currentStackId: stackId,
      });
      // Don't clear - might be valid for another tab/session
      return;
    }

    logger.info('Found recoverable loading state', {
      component: 'useChatLoadingPersistence',
      operation: 'mount',
      stackId: state.stackId,
      adapterCount: state.adaptersToLoad.length,
      age: Date.now() - state.startedAt,
    });

    setPersistedState(state);
    setIsRecoverable(true);
  }, [enabled, stackId, storageKey]);

  // Persist loading state
  const persist = useCallback(
    (state: ChatLoadingState) => {
      if (!enabled) {
        return;
      }

      const success = writeToStorage(storageKey, state);
      if (success) {
        setPersistedState(state);
        setIsRecoverable(true);

        logger.debug('Persisted loading state', {
          component: 'useChatLoadingPersistence',
          operation: 'persist',
          stackId: state.stackId,
          adapterCount: state.adaptersToLoad.length,
        });
      } else {
        // Continue without persistence (already logged in writeToStorage)
        setPersistedState(null);
        setIsRecoverable(false);
      }
    },
    [enabled, storageKey]
  );

  // Clear persisted state
  const clear = useCallback(() => {
    if (!enabled) {
      return;
    }

    clearFromStorage(storageKey);
    setPersistedState(null);
    setIsRecoverable(false);

    logger.debug('Cleared persisted loading state', {
      component: 'useChatLoadingPersistence',
      operation: 'clear',
    });
  }, [enabled, storageKey]);

  return {
    persistedState,
    persist,
    clear,
    isRecoverable,
  };
}
