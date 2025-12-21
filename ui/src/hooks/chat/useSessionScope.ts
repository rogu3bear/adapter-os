/**
 * Session Scope Management Hook
 *
 * Manages per-session state for dataset and stack selection using sessionStorage.
 * State persists across page refreshes but is scoped to the current browser tab.
 *
 * Key format: sessionScope:{sessionId}
 * Storage: sessionStorage (tab-scoped, survives refresh)
 *
 * @module useSessionScope
 */

import { useCallback } from 'react';
import {
  type SessionScope,
  type DatasetScopeUpdate,
  DEFAULT_SESSION_SCOPE,
  getSessionScopeKey,
  LEGACY_GLOBAL_KEYS,
} from '@/types/session-scope';
import { logger } from '@/utils/logger';

/**
 * Read a session scope from sessionStorage.
 *
 * @param sessionId - The session ID to read scope for
 * @returns The session scope, or default scope if not found
 */
function readSessionStorage(key: string): string | null {
  if (typeof window === 'undefined') return null;
  try {
    return window.sessionStorage.getItem(key);
  } catch {
    return null;
  }
}

/**
 * Write a session scope to sessionStorage.
 *
 * @param key - The storage key
 * @param value - The value to write
 */
function writeSessionStorage(key: string, value: string): void {
  if (typeof window === 'undefined') return;
  try {
    window.sessionStorage.setItem(key, value);
  } catch {
    // Ignore storage failures (quota exceeded, etc.)
  }
}

/**
 * Remove a key from sessionStorage.
 *
 * @param key - The storage key to remove
 */
function removeSessionStorage(key: string): void {
  if (typeof window === 'undefined') return;
  try {
    window.sessionStorage.removeItem(key);
  } catch {
    // Ignore failures
  }
}

/**
 * Session Scope Hook
 *
 * Provides functions to manage per-session state for dataset and stack selection.
 * All functions are stable callbacks that can be safely used in dependency arrays.
 *
 * @returns Object with session scope management functions
 */
export function useSessionScope() {
  /**
   * Read the current session scope from sessionStorage.
   *
   * @param sessionId - The session ID to read scope for
   * @returns The current session scope, or default scope if not found
   */
  const getSessionScope = useCallback((sessionId: string): SessionScope => {
    if (!sessionId) {
      logger.warn('getSessionScope called with empty sessionId', {
        component: 'useSessionScope',
        operation: 'getSessionScope',
      });
      return { ...DEFAULT_SESSION_SCOPE };
    }

    const key = getSessionScopeKey(sessionId);
    const stored = readSessionStorage(key);

    if (!stored) {
      return { ...DEFAULT_SESSION_SCOPE };
    }

    try {
      const parsed = JSON.parse(stored) as SessionScope;
      // Ensure all fields are present (handle legacy or partial data)
      return {
        ...DEFAULT_SESSION_SCOPE,
        ...parsed,
      };
    } catch (error) {
      logger.error('Failed to parse session scope', {
        component: 'useSessionScope',
        operation: 'getSessionScope',
        sessionId,
      }, error as Error);
      return { ...DEFAULT_SESSION_SCOPE };
    }
  }, []);

  /**
   * Update the session scope with a partial update.
   * Merges the provided updates with the existing scope.
   *
   * @param sessionId - The session ID to update
   * @param scope - Partial scope update to merge
   */
  const setSessionScope = useCallback((sessionId: string, scope: Partial<SessionScope>): void => {
    if (!sessionId) {
      logger.warn('setSessionScope called with empty sessionId', {
        component: 'useSessionScope',
        operation: 'setSessionScope',
      });
      return;
    }

    const key = getSessionScopeKey(sessionId);
    const current = getSessionScope(sessionId);
    const updated: SessionScope = {
      ...current,
      ...scope,
    };

    try {
      writeSessionStorage(key, JSON.stringify(updated));
      logger.debug('Session scope updated', {
        component: 'useSessionScope',
        operation: 'setSessionScope',
        sessionId,
        updates: scope,
      });
    } catch (error) {
      logger.error('Failed to update session scope', {
        component: 'useSessionScope',
        operation: 'setSessionScope',
        sessionId,
      }, error as Error);
    }
  }, [getSessionScope]);

  /**
   * Clear all scope for a session.
   * Resets to default scope state.
   *
   * @param sessionId - The session ID to clear scope for
   */
  const clearSessionScope = useCallback((sessionId: string): void => {
    if (!sessionId) {
      logger.warn('clearSessionScope called with empty sessionId', {
        component: 'useSessionScope',
        operation: 'clearSessionScope',
      });
      return;
    }

    const key = getSessionScopeKey(sessionId);
    removeSessionStorage(key);

    logger.debug('Session scope cleared', {
      component: 'useSessionScope',
      operation: 'clearSessionScope',
      sessionId,
    });
  }, []);

  /**
   * Set dataset scope for a session.
   * Updates dataset-related fields and sets a timestamp.
   *
   * @param sessionId - The session ID to update
   * @param dataset - Dataset scope update
   */
  const setDatasetScope = useCallback((sessionId: string, dataset: DatasetScopeUpdate): void => {
    const scopeUpdate: Partial<SessionScope> = {
      activeDatasetId: dataset.activeDatasetId,
      activeDatasetName: dataset.activeDatasetName,
      datasetVersionId: dataset.datasetVersionId || null,
      collectionId: dataset.collectionId || null,
      scopedAt: new Date().toISOString(),
    };

    setSessionScope(sessionId, scopeUpdate);

    logger.info('Dataset scope set', {
      component: 'useSessionScope',
      operation: 'setDatasetScope',
      sessionId,
      datasetId: dataset.activeDatasetId,
      datasetName: dataset.activeDatasetName,
    });
  }, [setSessionScope]);

  /**
   * Set stack selection for a session.
   * Updates stack-related fields.
   *
   * @param sessionId - The session ID to update
   * @param stackId - The stack ID to select
   * @param stackName - Optional stack name for display
   */
  const setStackSelection = useCallback((sessionId: string, stackId: string, stackName?: string): void => {
    const scopeUpdate: Partial<SessionScope> = {
      selectedStackId: stackId,
      stackName: stackName || null,
    };

    setSessionScope(sessionId, scopeUpdate);

    logger.info('Stack selection set', {
      component: 'useSessionScope',
      operation: 'setStackSelection',
      sessionId,
      stackId,
      stackName,
    });
  }, [setSessionScope]);

  /**
   * Clear only the dataset scope fields for a session.
   * Stack selection is preserved.
   *
   * @param sessionId - The session ID to update
   */
  const clearDatasetScope = useCallback((sessionId: string): void => {
    const scopeUpdate: Partial<SessionScope> = {
      activeDatasetId: null,
      activeDatasetName: null,
      datasetVersionId: null,
      collectionId: null,
      scopedAt: null,
    };

    setSessionScope(sessionId, scopeUpdate);

    logger.info('Dataset scope cleared', {
      component: 'useSessionScope',
      operation: 'clearDatasetScope',
      sessionId,
    });
  }, [setSessionScope]);

  /**
   * Clear only the stack selection fields for a session.
   * Dataset scope is preserved.
   *
   * @param sessionId - The session ID to update
   */
  const clearStackSelection = useCallback((sessionId: string): void => {
    const scopeUpdate: Partial<SessionScope> = {
      selectedStackId: null,
      stackName: null,
    };

    setSessionScope(sessionId, scopeUpdate);

    logger.info('Stack selection cleared', {
      component: 'useSessionScope',
      operation: 'clearStackSelection',
      sessionId,
    });
  }, [setSessionScope]);

  /**
   * Migrate legacy global dataset keys to session-scoped storage.
   * Reads old `datasetChat:*` keys, migrates them to the session scope, and deletes the old keys.
   *
   * This should be called once when loading a session to ensure backward compatibility.
   *
   * @param sessionId - The session ID to migrate data to
   */
  const migrateGlobalKeys = useCallback((sessionId: string): void => {
    if (!sessionId) {
      return;
    }

    try {
      // Check if there's any legacy data to migrate
      const legacyDatasetId = readSessionStorage('datasetChat:activeDatasetId');
      const legacyDatasetName = readSessionStorage('datasetChat:activeDatasetName');
      const legacyCollectionId = readSessionStorage('datasetChat:collectionId');
      const legacyDatasetVersionId = readSessionStorage('datasetChat:datasetVersionId');

      if (!legacyDatasetId && !legacyDatasetName && !legacyCollectionId && !legacyDatasetVersionId) {
        // No legacy data to migrate
        return;
      }

      // Check if session already has scope (don't overwrite existing data)
      const currentScope = getSessionScope(sessionId);
      if (currentScope.activeDatasetId) {
        // Session already has data, don't migrate
        logger.debug('Session already has scope, skipping migration', {
          component: 'useSessionScope',
          operation: 'migrateGlobalKeys',
          sessionId,
        });
      } else {
        // Migrate legacy data to session scope
        const migratedScope: Partial<SessionScope> = {
          activeDatasetId: legacyDatasetId,
          activeDatasetName: legacyDatasetName,
          collectionId: legacyCollectionId,
          datasetVersionId: legacyDatasetVersionId,
          scopedAt: legacyDatasetId ? new Date().toISOString() : null,
        };

        setSessionScope(sessionId, migratedScope);

        logger.info('Migrated legacy dataset keys to session scope', {
          component: 'useSessionScope',
          operation: 'migrateGlobalKeys',
          sessionId,
          migratedDatasetId: legacyDatasetId,
        });
      }

      // Delete all legacy keys (whether we migrated or not)
      LEGACY_GLOBAL_KEYS.forEach((key) => {
        removeSessionStorage(key);
      });

      logger.debug('Cleaned up legacy global keys', {
        component: 'useSessionScope',
        operation: 'migrateGlobalKeys',
        sessionId,
        keysRemoved: LEGACY_GLOBAL_KEYS.length,
      });
    } catch (error) {
      logger.error('Failed to migrate legacy global keys', {
        component: 'useSessionScope',
        operation: 'migrateGlobalKeys',
        sessionId,
      }, error as Error);
    }
  }, [getSessionScope, setSessionScope]);

  return {
    getSessionScope,
    setSessionScope,
    clearSessionScope,
    setDatasetScope,
    setStackSelection,
    clearDatasetScope,
    clearStackSelection,
    migrateGlobalKeys,
  };
}

// Default export
export default useSessionScope;
