//! Enhanced History Context Provider
//!
//! Global context for managing action history across the application.
//! Integrates with UndoRedoContext for seamless undo/redo functionality.

import { createContext, useContext, ReactNode } from 'react';
import useEnhancedActionHistory from '@/hooks/ui/useEnhancedActionHistory';
import {
  ActionHistoryItem,
  HistoryFilterOptions,
  HistoryExportOptions,
  HistoryReplayOptions,
  ReplayResult,
  ActionStats,
} from '@/types/history';

interface HistoryContextType {
  // History management
  addAction: (action: Omit<ActionHistoryItem, 'id' | 'timestamp'>) => void;
  undo: () => Promise<boolean>;
  redo: () => Promise<boolean>;
  canUndo: boolean;
  canRedo: boolean;
  clearHistory: () => void;
  getActionById: (id: string) => ActionHistoryItem | undefined;

  // Filtering and search
  setFilter: (filters: HistoryFilterOptions) => void;
  setSearch: (query: string) => void;
  filteredActions: ActionHistoryItem[];
  paginatedActions: ActionHistoryItem[];

  // Selection
  toggleSelection: (actionId: string) => void;
  selectAll: () => void;
  clearSelection: () => void;
  selectedCount: number;
  isSelected: (id: string) => boolean;

  // Pagination
  pagination: { page: number; pageSize: number };
  totalPages: number;
  setPagination: (pagination: { page: number; pageSize: number }) => void;

  // Current state
  allActions: ActionHistoryItem[];
  currentAction: ActionHistoryItem | null;
  historyCount: number;

  // Replay
  replayAction: (actionId: string, dryRun?: boolean) => Promise<boolean>;
  replayActions: (options: HistoryReplayOptions) => Promise<ReplayResult>;

  // Export
  exportHistory: (options: HistoryExportOptions) => Promise<string>;

  // Analytics
  stats: ActionStats;
}

const HistoryContext = createContext<HistoryContextType | undefined>(undefined);

interface HistoryProviderProps {
  children: ReactNode;
  maxSize?: number;
}

export function HistoryProvider({ children, maxSize = 1000 }: HistoryProviderProps) {
  const history = useEnhancedActionHistory({
    maxSize,
    persistToLocalStorage: true,
    autoCleanup: true,
    cleanupInterval: 60000,
  });

  const contextValue: HistoryContextType = {
    addAction: history.addAction,
    undo: history.undo,
    redo: history.redo,
    canUndo: history.canUndo,
    canRedo: history.canRedo,
    clearHistory: history.clearHistory,
    getActionById: history.getActionById,

    setFilter: history.setFilter,
    setSearch: history.setSearch,
    filteredActions: history.filteredActions,
    paginatedActions: history.paginatedActions,

    toggleSelection: history.toggleSelection,
    selectAll: history.selectAll,
    clearSelection: history.clearSelection,
    selectedCount: history.selectedCount,
    isSelected: history.isSelected,

    pagination: history.pagination,
    totalPages: history.totalPages,
    setPagination: history.setPagination,

    allActions: history.allActions,
    currentAction: history.currentAction,
    historyCount: history.historyCount,

    replayAction: history.replayAction,
    replayActions: history.replayActions,

    exportHistory: history.exportHistory,

    stats: history.stats,
  };

  return (
    <HistoryContext.Provider value={contextValue}>
      {children}
    </HistoryContext.Provider>
  );
}

export function useHistory() {
  const context = useContext(HistoryContext);
  if (context === undefined) {
    throw new Error('useHistory must be used within a HistoryProvider');
  }
  return context;
}

export default HistoryContext;
