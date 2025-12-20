/**
 * DatasetChatContext - Manages state for dataset-scoped chat
 *
 * Provides shared state and actions for coordinating between dataset selection
 * and chat interface context scoping.
 *
 * Persists active dataset scope to sessionStorage keyed by chat session ID,
 * so each chat session has its own isolated dataset scope.
 *
 * Storage model:
 * - Key: `sessionScope:{sessionId}` - per-session dataset scope (unified with stack selection)
 * - Uses useSessionScope hook for all storage operations
 */

import { createContext, useContext, useState, useCallback, useEffect, useRef, ReactNode } from 'react';
import { useSessionScope } from '@/hooks/chat/useSessionScope';
import type { SessionScope } from '@/types/session-scope';
import { logger } from '@/utils/logger';

interface DatasetChatState {
  /** Currently active dataset ID */
  activeDatasetId: string | null;
  /** Dataset display name */
  activeDatasetName: string | null;
  /** Collection ID for RAG scoping (if dataset is derived from a collection) */
  collectionId: string | null;
  /** Dataset version ID for deterministic replay */
  datasetVersionId: string | null;
  /** Timestamp when scope was set (ISO string) */
  scopedAt: string | null;
}

interface DatasetChatActions {
  /** Set the active dataset for chat context */
  setActiveDataset: (dataset: {
    id: string;
    name: string;
    collectionId?: string;
    versionId?: string;
  }) => void;
  /** Clear the active dataset (clears only current session's record) */
  clearActiveDataset: () => void;
}

interface DatasetChatContextValue extends DatasetChatState, DatasetChatActions {
  /** Current session ID (for debugging) */
  currentSessionId: string | null;
}

const DatasetChatContext = createContext<DatasetChatContextValue | null>(null);

interface DatasetChatProviderProps {
  children: ReactNode;
  /** Chat session ID - dataset scope is keyed by this. Optional - when not provided, dataset scope is not persisted. */
  sessionId?: string | null;
  /** Initial dataset to scope chat to */
  initialDataset?: {
    id: string;
    name: string;
    collectionId?: string;
    versionId?: string;
  };
}

// Empty state constant
const EMPTY_STATE: DatasetChatState = {
  activeDatasetId: null,
  activeDatasetName: null,
  collectionId: null,
  datasetVersionId: null,
  scopedAt: null,
};

// Helper to convert SessionScope to DatasetChatState
function sessionScopeToState(scope: SessionScope): DatasetChatState {
  return {
    activeDatasetId: scope.activeDatasetId,
    activeDatasetName: scope.activeDatasetName,
    collectionId: scope.collectionId,
    datasetVersionId: scope.datasetVersionId,
    scopedAt: scope.scopedAt,
  };
}

export function DatasetChatProvider({
  children,
  sessionId,
  initialDataset,
}: DatasetChatProviderProps) {
  // Get session scope utilities
  const sessionScope = useSessionScope();

  // Track the previous sessionId to detect changes
  const prevSessionIdRef = useRef<string | null | undefined>(undefined);

  // Track if we've migrated legacy keys for this session
  const hasMigratedRef = useRef(false);

  // Initialize state from props first, then fall back to session-keyed storage
  const [state, setState] = useState<DatasetChatState>(() => {
    if (initialDataset) {
      return {
        activeDatasetId: initialDataset.id,
        activeDatasetName: initialDataset.name,
        collectionId: initialDataset.collectionId ?? null,
        datasetVersionId: initialDataset.versionId ?? null,
        scopedAt: new Date().toISOString(),
      };
    }

    // If sessionId is provided, load from session-keyed storage
    if (sessionId) {
      const scope = sessionScope.getSessionScope(sessionId);
      return sessionScopeToState(scope);
    }

    return EMPTY_STATE;
  });

  // Migrate legacy global keys once on mount if sessionId is available
  useEffect(() => {
    if (sessionId && !hasMigratedRef.current) {
      sessionScope.migrateGlobalKeys(sessionId);
      hasMigratedRef.current = true;

      // Reload state after migration in case legacy data was migrated
      const scope = sessionScope.getSessionScope(sessionId);
      const migratedState = sessionScopeToState(scope);
      if (migratedState.activeDatasetId && !state.activeDatasetId) {
        setState(migratedState);
      }
    }
  }, [sessionId, sessionScope, state.activeDatasetId]);

  // Handle sessionId changes - load that session's scope
  useEffect(() => {
    // Skip on initial mount
    if (prevSessionIdRef.current === undefined) {
      prevSessionIdRef.current = sessionId;
      return;
    }

    // If sessionId changed, load the new session's scope
    if (prevSessionIdRef.current !== sessionId) {
      prevSessionIdRef.current = sessionId;
      hasMigratedRef.current = false; // Reset migration flag for new session

      if (sessionId) {
        const scope = sessionScope.getSessionScope(sessionId);
        const loadedState = sessionScopeToState(scope);
        setState(loadedState);
      } else {
        // No session - clear state
        setState(EMPTY_STATE);
      }
    }
  }, [sessionId, sessionScope]);

  const setActiveDataset = useCallback(
    (dataset: { id: string; name: string; collectionId?: string; versionId?: string }) => {
      if (!sessionId) {
        logger.warn('setActiveDataset called without sessionId', {
          component: 'DatasetChatContext',
          operation: 'setActiveDataset',
        });
        return;
      }

      const newState: DatasetChatState = {
        activeDatasetId: dataset.id,
        activeDatasetName: dataset.name,
        collectionId: dataset.collectionId ?? null,
        datasetVersionId: dataset.versionId ?? null,
        scopedAt: new Date().toISOString(),
      };

      // Update local state
      setState(newState);

      // Persist to session-scoped storage
      sessionScope.setDatasetScope(sessionId, {
        activeDatasetId: dataset.id,
        activeDatasetName: dataset.name,
        collectionId: dataset.collectionId,
        datasetVersionId: dataset.versionId,
      });
    },
    [sessionId, sessionScope]
  );

  const clearActiveDataset = useCallback(() => {
    if (!sessionId) {
      logger.warn('clearActiveDataset called without sessionId', {
        component: 'DatasetChatContext',
        operation: 'clearActiveDataset',
      });
      return;
    }

    // Clear from storage for current session only
    sessionScope.clearDatasetScope(sessionId);
    setState(EMPTY_STATE);
  }, [sessionId, sessionScope]);

  const value: DatasetChatContextValue = {
    ...state,
    currentSessionId: sessionId ?? null,
    setActiveDataset,
    clearActiveDataset,
  };

  return (
    <DatasetChatContext.Provider value={value}>
      {children}
    </DatasetChatContext.Provider>
  );
}

/**
 * Hook to access dataset chat context
 * @throws Error if used outside of DatasetChatProvider
 */
export function useDatasetChat(): DatasetChatContextValue {
  const context = useContext(DatasetChatContext);
  if (!context) {
    throw new Error('useDatasetChat must be used within a DatasetChatProvider');
  }
  return context;
}

/**
 * Hook to access dataset chat context without throwing
 * Returns null if outside provider
 */
export function useDatasetChatOptional(): DatasetChatContextValue | null {
  return useContext(DatasetChatContext);
}

export default DatasetChatContext;
