import { useState, useCallback } from 'react';
import type { Adapter } from '@/api/types';

/**
 * Dialog types supported by the adapter management interface
 */
export type DialogType =
  | 'create'
  | 'import'
  | 'export'
  | 'health'
  | 'training'
  | 'language'
  | 'upsert'
  | 'delete';

/**
 * Generic dialog state with optional typed data
 */
export interface DialogState<T = unknown> {
  isOpen: boolean;
  data?: T;
}

/**
 * Type-safe data structures for each dialog type
 */
export interface DialogDataTypes {
  create: never;
  import: never;
  export: { scope?: 'all' | 'selected' | 'filtered' };
  health: { adapter: Adapter };
  training: { adapter?: Adapter };
  language: never;
  upsert: { root?: string; path?: string; activate?: boolean };
  delete: { adapterId: string };
}

/**
 * Return type for the useAdapterDialogs hook
 */
export interface UseAdapterDialogsReturn {
  // Dialog states
  dialogs: Record<DialogType, DialogState>;

  // Open/close methods
  openDialog: <T extends DialogType>(type: T, data?: DialogDataTypes[T]) => void;
  closeDialog: (type: DialogType) => void;
  closeAllDialogs: () => void;

  // Convenience getters
  isDialogOpen: (type: DialogType) => boolean;
  getDialogData: <T extends DialogType>(type: T) => DialogDataTypes[T] | undefined;

  // Backwards compatibility getters
  isCreateDialogOpen: boolean;
  isImportDialogOpen: boolean;
  isExportDialogOpen: boolean;
  isHealthDialogOpen: boolean;
  isTrainingDialogOpen: boolean;
  isLanguageDialogOpen: boolean;
  isUpsertDialogOpen: boolean;
  isDeleteDialogOpen: boolean;

  // Backwards compatibility setters (for gradual migration)
  setIsCreateDialogOpen: (open: boolean) => void;
  setIsImportDialogOpen: (open: boolean) => void;
  setShowExportDialog: (open: boolean) => void;
  setIsTrainingDialogOpen: (open: boolean) => void;
  setIsLanguageDialogOpen: (open: boolean) => void;
  setUpsertOpen: (open: boolean) => void;
  setDeleteConfirmId: (id: string | null) => void;

  // Dialog-specific data accessors
  selectedAdapterForHealth: Adapter | null;
  deleteConfirmId: string | null;
  exportDialogScope: 'all' | 'selected' | 'filtered';
  upsertRoot: string;
  upsertPath: string;
  upsertActivate: boolean;

  // Dialog-specific data setters
  setSelectedAdapterForHealth: (adapter: Adapter | null) => void;
  setExportDialogScope: (scope: 'all' | 'selected' | 'filtered') => void;
  setUpsertRoot: (root: string) => void;
  setUpsertPath: (path: string) => void;
  setUpsertActivate: (activate: boolean) => void;
}

/**
 * Centralized dialog state management hook for adapter operations
 *
 * Consolidates all dialog/modal state from Adapters.tsx into a single hook
 * with type-safe data passing and convenience methods.
 *
 * @example
 * ```typescript
 * // Basic usage
 * const { openDialog, closeDialog, isDialogOpen } = useAdapterDialogs();
 *
 * // Open a simple dialog
 * openDialog('create');
 *
 * // Open dialog with data
 * openDialog('health', { adapter: myAdapter });
 * openDialog('delete', { adapterId: 'adapter-123' });
 *
 * // Check dialog state
 * if (isDialogOpen('training')) {
 *   // Training dialog is open
 * }
 *
 * // Get dialog data (type-safe)
 * const healthData = getDialogData('health');
 * if (healthData) {
 *   console.log(healthData.adapter.name);
 * }
 *
 * // Close specific dialog
 * closeDialog('create');
 *
 * // Close all dialogs
 * closeAllDialogs();
 * ```
 *
 * @example
 * ```typescript
 * // Backwards compatible usage (during migration)
 * const {
 *   isCreateDialogOpen,
 *   setIsCreateDialogOpen,
 *   selectedAdapterForHealth
 * } = useAdapterDialogs();
 *
 * // Works with existing code
 * setIsCreateDialogOpen(true);
 * ```
 *
 * @returns {UseAdapterDialogsReturn} Dialog state and control methods
 */
export function useAdapterDialogs(): UseAdapterDialogsReturn {
  // Internal state for all dialogs
  const [dialogs, setDialogs] = useState<Record<DialogType, DialogState>>({
    create: { isOpen: false },
    import: { isOpen: false },
    export: { isOpen: false },
    health: { isOpen: false },
    training: { isOpen: false },
    language: { isOpen: false },
    upsert: { isOpen: false },
    delete: { isOpen: false },
  });

  // Additional state for dialogs with complex data needs
  const [selectedAdapterForHealth, setSelectedAdapterForHealth] = useState<Adapter | null>(null);
  const [deleteConfirmId, setDeleteConfirmId] = useState<string | null>(null);
  const [exportDialogScope, setExportDialogScope] = useState<'all' | 'selected' | 'filtered'>('all');
  const [upsertRoot, setUpsertRoot] = useState('');
  const [upsertPath, setUpsertPath] = useState('');
  const [upsertActivate, setUpsertActivate] = useState(true);

  /**
   * Open a dialog with optional typed data
   */
  const openDialog = useCallback(<T extends DialogType>(
    type: T,
    data?: DialogDataTypes[T]
  ) => {
    setDialogs(prev => ({
      ...prev,
      [type]: { isOpen: true, data },
    }));

    // Sync additional state for backwards compatibility
    if (type === 'health' && data) {
      const healthData = data as DialogDataTypes['health'];
      setSelectedAdapterForHealth(healthData.adapter);
    } else if (type === 'delete' && data) {
      const deleteData = data as DialogDataTypes['delete'];
      setDeleteConfirmId(deleteData.adapterId);
    } else if (type === 'export' && data) {
      const exportData = data as DialogDataTypes['export'];
      if (exportData.scope) {
        setExportDialogScope(exportData.scope);
      }
    } else if (type === 'upsert' && data) {
      const upsertData = data as DialogDataTypes['upsert'];
      if (upsertData.root !== undefined) setUpsertRoot(upsertData.root);
      if (upsertData.path !== undefined) setUpsertPath(upsertData.path);
      if (upsertData.activate !== undefined) setUpsertActivate(upsertData.activate);
    }
  }, []);

  /**
   * Close a specific dialog
   */
  const closeDialog = useCallback((type: DialogType) => {
    setDialogs(prev => ({
      ...prev,
      [type]: { isOpen: false },
    }));

    // Clear associated state
    if (type === 'health') {
      setSelectedAdapterForHealth(null);
    } else if (type === 'delete') {
      setDeleteConfirmId(null);
    } else if (type === 'export') {
      setExportDialogScope('all');
    } else if (type === 'upsert') {
      setUpsertRoot('');
      setUpsertPath('');
      setUpsertActivate(true);
    }
  }, []);

  /**
   * Close all open dialogs
   */
  const closeAllDialogs = useCallback(() => {
    setDialogs({
      create: { isOpen: false },
      import: { isOpen: false },
      export: { isOpen: false },
      health: { isOpen: false },
      training: { isOpen: false },
      language: { isOpen: false },
      upsert: { isOpen: false },
      delete: { isOpen: false },
    });

    // Clear all associated state
    setSelectedAdapterForHealth(null);
    setDeleteConfirmId(null);
    setExportDialogScope('all');
    setUpsertRoot('');
    setUpsertPath('');
    setUpsertActivate(true);
  }, []);

  /**
   * Check if a specific dialog is open
   */
  const isDialogOpen = useCallback((type: DialogType): boolean => {
    return dialogs[type].isOpen;
  }, [dialogs]);

  /**
   * Get typed data for a specific dialog
   */
  const getDialogData = useCallback(<T extends DialogType>(
    type: T
  ): DialogDataTypes[T] | undefined => {
    return dialogs[type].data as DialogDataTypes[T] | undefined;
  }, [dialogs]);

  // Backwards compatibility setters
  const setIsCreateDialogOpen = useCallback((open: boolean) => {
    if (open) {
      openDialog('create');
    } else {
      closeDialog('create');
    }
  }, [openDialog, closeDialog]);

  const setIsImportDialogOpen = useCallback((open: boolean) => {
    if (open) {
      openDialog('import');
    } else {
      closeDialog('import');
    }
  }, [openDialog, closeDialog]);

  const setShowExportDialog = useCallback((open: boolean) => {
    if (open) {
      openDialog('export');
    } else {
      closeDialog('export');
    }
  }, [openDialog, closeDialog]);

  const setIsTrainingDialogOpen = useCallback((open: boolean) => {
    if (open) {
      openDialog('training');
    } else {
      closeDialog('training');
    }
  }, [openDialog, closeDialog]);

  const setIsLanguageDialogOpen = useCallback((open: boolean) => {
    if (open) {
      openDialog('language');
    } else {
      closeDialog('language');
    }
  }, [openDialog, closeDialog]);

  const setUpsertOpen = useCallback((open: boolean) => {
    if (open) {
      openDialog('upsert');
    } else {
      closeDialog('upsert');
    }
  }, [openDialog, closeDialog]);

  const setDeleteConfirmIdWrapper = useCallback((id: string | null) => {
    if (id) {
      openDialog('delete', { adapterId: id });
    } else {
      closeDialog('delete');
    }
  }, [openDialog, closeDialog]);

  return {
    // Core API
    dialogs,
    openDialog,
    closeDialog,
    closeAllDialogs,
    isDialogOpen,
    getDialogData,

    // Backwards compatibility getters
    isCreateDialogOpen: dialogs.create.isOpen,
    isImportDialogOpen: dialogs.import.isOpen,
    isExportDialogOpen: dialogs.export.isOpen,
    isHealthDialogOpen: dialogs.health.isOpen,
    isTrainingDialogOpen: dialogs.training.isOpen,
    isLanguageDialogOpen: dialogs.language.isOpen,
    isUpsertDialogOpen: dialogs.upsert.isOpen,
    isDeleteDialogOpen: dialogs.delete.isOpen,

    // Backwards compatibility setters
    setIsCreateDialogOpen,
    setIsImportDialogOpen,
    setShowExportDialog,
    setIsTrainingDialogOpen,
    setIsLanguageDialogOpen,
    setUpsertOpen,
    setDeleteConfirmId: setDeleteConfirmIdWrapper,

    // Dialog-specific data
    selectedAdapterForHealth,
    deleteConfirmId,
    exportDialogScope,
    upsertRoot,
    upsertPath,
    upsertActivate,

    // Dialog-specific setters
    setSelectedAdapterForHealth,
    setExportDialogScope,
    setUpsertRoot,
    setUpsertPath,
    setUpsertActivate,
  };
}
