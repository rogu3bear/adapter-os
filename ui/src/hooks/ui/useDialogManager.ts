import { useState, useCallback } from 'react';

/**
 * Generic dialog state with optional typed data
 */
interface DialogState<T = unknown> {
  open: boolean;
  data: T | null;
}

/**
 * Generic dialog manager factory
 *
 * Creates a type-safe dialog manager hook for managing multiple dialogs with typed data.
 *
 * @template T - Union type of dialog IDs (string literals)
 * @template D - Record mapping each dialog ID to its data type
 *
 * @param dialogTypes - Array of dialog type identifiers
 * @returns Hook function for managing dialogs
 *
 * @example
 * ```typescript
 * // Define dialog types and their data
 * const useMyDialogs = createDialogManager<
 *   'create' | 'edit' | 'delete',
 *   {
 *     create: undefined;
 *     edit: { id: string; name: string };
 *     delete: { id: string; name: string };
 *   }
 * >(['create', 'edit', 'delete']);
 *
 * // Use in component
 * const dialogs = useMyDialogs();
 *
 * // Open with data
 * dialogs.openDialog('edit', { id: '123', name: 'Example' });
 *
 * // Check state
 * if (dialogs.isOpen('edit')) {
 *   const data = dialogs.getData('edit'); // Type: { id: string; name: string } | null
 * }
 *
 * // Close
 * dialogs.closeDialog('edit');
 * ```
 */
export function createDialogManager<
  T extends string,
  D extends Record<T, unknown>
>(dialogTypes: readonly T[]) {
  type State = { [K in T]: DialogState<D[K]> };

  const initialState = dialogTypes.reduce((acc, type) => {
    acc[type] = { open: false, data: null };
    return acc;
  }, {} as State);

  return function useDialogManager() {
    const [state, setState] = useState<State>(initialState);

    /**
     * Open a dialog with optional typed data
     */
    const openDialog = useCallback(<K extends T>(type: K, data?: D[K]) => {
      setState(prev => ({
        ...prev,
        [type]: { open: true, data: data ?? null },
      }));
    }, []);

    /**
     * Close a specific dialog and clear its data
     */
    const closeDialog = useCallback((type: T) => {
      setState(prev => ({
        ...prev,
        [type]: { open: false, data: null },
      }));
    }, []);

    /**
     * Close all dialogs and clear all data
     */
    const closeAllDialogs = useCallback(() => {
      setState(initialState);
    }, []);

    /**
     * Check if a specific dialog is open
     */
    const isOpen = useCallback((type: T): boolean => {
      return state[type].open;
    }, [state]);

    /**
     * Get typed data for a specific dialog
     */
    const getData = useCallback(<K extends T>(type: K): D[K] | null => {
      return state[type].data as D[K] | null;
    }, [state]);

    return {
      openDialog,
      closeDialog,
      closeAllDialogs,
      isOpen,
      getData,
      state,
    };
  };
}

// ============================================================================
// Pre-built Dialog Managers
// ============================================================================

/**
 * Pre-built dialog manager for adapter operations
 *
 * @example
 * ```typescript
 * const dialogs = useAdapterDialogs();
 *
 * // Open delete confirmation
 * dialogs.openDialog('delete', {
 *   adapterId: 'adapter-123',
 *   adapterName: 'My Adapter'
 * });
 *
 * // Open health dialog
 * dialogs.openDialog('health', { adapter: myAdapter });
 *
 * // Check if open
 * if (dialogs.isOpen('delete')) {
 *   const data = dialogs.getData('delete');
 *   console.log(data?.adapterId); // Type-safe access
 * }
 *
 * // Close
 * dialogs.closeDialog('delete');
 * ```
 */
export const useAdapterDialogs = createDialogManager<
  'create' | 'import' | 'export' | 'delete' | 'health' | 'training',
  {
    create: undefined;
    import: undefined;
    export: { adapters: string[] };
    delete: { adapterId: string; adapterName: string };
    health: { adapter: { id: string; name: string } };
    training: { adapter: { id: string; name: string } };
  }
>(['create', 'import', 'export', 'delete', 'health', 'training'] as const);

/**
 * Pre-built dialog manager for chat operations
 *
 * @example
 * ```typescript
 * const dialogs = useChatDialogs();
 *
 * // Open share dialog
 * dialogs.openDialog('share', { sessionId: 'session-123' });
 *
 * // Open delete confirmation
 * dialogs.openDialog('delete', {
 *   sessionId: 'session-123',
 *   sessionName: 'My Session'
 * });
 *
 * // Check state
 * if (dialogs.isOpen('share')) {
 *   const data = dialogs.getData('share');
 *   console.log(data?.sessionId); // Type-safe
 * }
 * ```
 */
export const useChatDialogs = createDialogManager<
  'share' | 'tags' | 'archive' | 'delete',
  {
    share: { sessionId: string };
    tags: { sessionId: string };
    archive: { sessionId: string };
    delete: { sessionId: string; sessionName: string };
  }
>(['share', 'tags', 'archive', 'delete'] as const);

/**
 * Pre-built dialog manager for training operations
 *
 * @example
 * ```typescript
 * const dialogs = useTrainingDialogs();
 *
 * // Open job detail dialog
 * dialogs.openDialog('jobDetail', { jobId: 'job-123' });
 *
 * // Open cancel confirmation
 * dialogs.openDialog('cancel', {
 *   jobId: 'job-123',
 *   jobName: 'Training Run #5'
 * });
 * ```
 */
export const useTrainingDialogs = createDialogManager<
  'create' | 'cancel' | 'delete' | 'jobDetail' | 'datasetDetail',
  {
    create: undefined;
    cancel: { jobId: string; jobName: string };
    delete: { jobId: string; jobName: string };
    jobDetail: { jobId: string };
    datasetDetail: { datasetId: string };
  }
>(['create', 'cancel', 'delete', 'jobDetail', 'datasetDetail'] as const);

/**
 * Pre-built dialog manager for document operations
 *
 * @example
 * ```typescript
 * const dialogs = useDocumentDialogs();
 *
 * // Open upload dialog
 * dialogs.openDialog('upload', { collectionId: 'coll-123' });
 *
 * // Open delete confirmation
 * dialogs.openDialog('delete', {
 *   documentId: 'doc-123',
 *   documentName: 'Report.pdf'
 * });
 * ```
 */
export const useDocumentDialogs = createDialogManager<
  'upload' | 'delete' | 'reprocess' | 'viewChunks',
  {
    upload: { collectionId?: string };
    delete: { documentId: string; documentName: string };
    reprocess: { documentId: string };
    viewChunks: { documentId: string; documentName: string };
  }
>(['upload', 'delete', 'reprocess', 'viewChunks'] as const);
