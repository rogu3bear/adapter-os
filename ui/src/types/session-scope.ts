/**
 * Session Scope Types
 *
 * Defines the shape of per-session state that persists across page refreshes.
 * Used to unify dataset scope and stack selection storage.
 *
 * Key format: sessionScope:{sessionId}
 * Storage: sessionStorage (survives refresh, scoped to tab)
 */

/**
 * Per-session scope state.
 * This is the single source of truth for dataset and stack selection per chat session.
 */
export interface SessionScope {
  // Dataset scope
  /** Currently active dataset ID for RAG context */
  activeDatasetId: string | null;
  /** Dataset name for display */
  activeDatasetName: string | null;
  /** Specific dataset version ID for deterministic replay */
  datasetVersionId: string | null;
  /** Collection ID for RAG retrieval */
  collectionId: string | null;
  /** ISO timestamp when scope was set */
  scopedAt: string | null;

  // Stack selection
  /** Currently selected adapter stack ID */
  selectedStackId: string | null;
  /** Stack name for display */
  stackName: string | null;
}

/**
 * Default session scope - no dataset or stack selected.
 */
export const DEFAULT_SESSION_SCOPE: SessionScope = {
  activeDatasetId: null,
  activeDatasetName: null,
  datasetVersionId: null,
  collectionId: null,
  scopedAt: null,
  selectedStackId: null,
  stackName: null,
};

/**
 * Partial update for dataset scope only.
 */
export interface DatasetScopeUpdate {
  activeDatasetId: string;
  activeDatasetName: string;
  datasetVersionId?: string;
  collectionId?: string;
}

/**
 * Partial update for stack selection only.
 */
export interface StackSelectionUpdate {
  selectedStackId: string;
  stackName?: string;
}

/**
 * Generate the sessionStorage key for a given session ID.
 */
export function getSessionScopeKey(sessionId: string): string {
  return `sessionScope:${sessionId}`;
}

/**
 * Legacy global keys that should be migrated and deleted.
 */
export const LEGACY_GLOBAL_KEYS = [
  'datasetChat:activeDatasetId',
  'datasetChat:activeDatasetName',
  'datasetChat:collectionId',
  'datasetChat:datasetVersionId',
  'datasetChat:state',
] as const;
