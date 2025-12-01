//! Enhanced Action History Hook
//!
//! Provides advanced action history management with filtering, search, export, and replay capabilities.
//! Extends useActionHistory with production-grade features.

import { useState, useCallback, useRef, useEffect, useMemo } from 'react';
import { logger, toError } from '@/utils/logger';
import {
  ActionHistoryItem,
  ActionType,
  ResourceType,
  ActionStatus,
  HistoryFilterOptions,
  HistorySearchOptions,
  PaginationOptions,
  HistoryExportOptions,
  HistoryReplayOptions,
  ReplayResult,
  ActionStats,
  HistoryStorageOptions,
} from '@/types/history';

interface EnhancedHistoryState {
  actions: ActionHistoryItem[];
  currentIndex: number;
  selectedIds: Set<string>;
  filters: HistoryFilterOptions;
  searchQuery: string;
  pagination: PaginationOptions;
}

const DEFAULT_MAX_SIZE = 1000;
const STORAGE_KEY = 'aos_action_history';
const STATS_UPDATE_INTERVAL = 5000; // 5 seconds

export function useEnhancedActionHistory(options: HistoryStorageOptions = {}) {
  const {
    maxSize = DEFAULT_MAX_SIZE,
    persistToLocalStorage = true,
    autoCleanup = true,
    cleanupInterval = 60000, // 1 minute
  } = options;

  const [state, setState] = useState<EnhancedHistoryState>(() => {
    const initial: EnhancedHistoryState = {
      actions: [],
      currentIndex: -1,
      selectedIds: new Set(),
      filters: {},
      searchQuery: '',
      pagination: { page: 0, pageSize: 50 },
    };

    // Load from localStorage if available
    if (persistToLocalStorage && typeof window !== 'undefined') {
      try {
        const stored = localStorage.getItem(STORAGE_KEY);
        if (stored) {
          const parsed = JSON.parse(stored);
          return {
            ...initial,
            actions: parsed.actions || [],
            currentIndex: parsed.currentIndex || -1,
          };
        }
      } catch (error) {
        logger.warn('Failed to load history from localStorage', { component: 'useEnhancedActionHistory' });
      }
    }

    return initial;
  });

  const stateRef = useRef(state);
  const cleanupTimerRef = useRef<NodeJS.Timeout>();

  // Keep ref in sync
  useEffect(() => {
    stateRef.current = state;
  }, [state]);

  // Persist to localStorage
  useEffect(() => {
    if (!persistToLocalStorage || typeof window === 'undefined') return;

    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify({
        actions: state.actions.slice(-maxSize),
        currentIndex: state.currentIndex,
      }));
    } catch (error) {
      logger.warn('Failed to persist history to localStorage', { component: 'useEnhancedActionHistory' });
    }
  }, [state.actions, state.currentIndex, persistToLocalStorage, maxSize]);

  // Auto cleanup old actions
  useEffect(() => {
    if (!autoCleanup) return;

    cleanupTimerRef.current = setInterval(() => {
      setState((prev) => {
        // Remove actions older than 30 days
        const cutoffTime = Date.now() - (30 * 24 * 60 * 60 * 1000);
        const filtered = prev.actions.filter((action) => action.timestamp > cutoffTime);

        if (filtered.length !== prev.actions.length) {
          logger.info('Cleaned up old history actions', {
            component: 'useEnhancedActionHistory',
            removedCount: prev.actions.length - filtered.length,
          });
        }

        return { ...prev, actions: filtered };
      });
    }, cleanupInterval);

    return () => {
      if (cleanupTimerRef.current) {
        clearInterval(cleanupTimerRef.current);
      }
    };
  }, [autoCleanup, cleanupInterval]);

  // Add action
  const addAction = useCallback((action: Omit<ActionHistoryItem, 'id' | 'timestamp'>) => {
    const item: ActionHistoryItem = {
      ...action,
      id: `${Date.now()}-${Math.random().toString(36).substr(2, 9)}`,
      timestamp: Date.now(),
    };

    setState((prev) => {
      const newActions = prev.actions.slice(0, prev.currentIndex + 1);
      newActions.push(item);

      const trimmed = newActions.slice(-maxSize);
      const newIndex = Math.min(prev.currentIndex + 1, maxSize - 1);

      return {
        ...prev,
        actions: trimmed,
        currentIndex: newIndex,
      };
    });

    logger.debug('Action added to history', {
      component: 'useEnhancedActionHistory',
      actionType: action.action,
      actionId: item.id,
    });
  }, [maxSize]);

  // Filter actions
  const filterActions = useCallback((options: HistoryFilterOptions): ActionHistoryItem[] => {
    return stateRef.current.actions.filter((action) => {
      if (options.actionTypes && !options.actionTypes.includes(action.action)) return false;
      if (options.resourceTypes && !options.resourceTypes.includes(action.resource)) return false;
      if (options.statuses && !options.statuses.includes(action.status)) return false;
      if (options.startDate && action.timestamp < options.startDate) return false;
      if (options.endDate && action.timestamp > options.endDate) return false;
      if (options.userIds && action.userId && !options.userIds.includes(action.userId)) return false;
      if (options.tenantIds && action.tenantId && !options.tenantIds.includes(action.tenantId)) return false;
      if (options.tags && !options.tags.some((tag) => action.tags?.includes(tag))) return false;
      return true;
    });
  }, []);

  // Search actions
  const searchActions = useCallback((query: string, actions: ActionHistoryItem[], options?: HistorySearchOptions): ActionHistoryItem[] => {
    if (!query.trim()) return actions;

    const searchFields = options?.searchFields || ['description'];
    const caseSensitive = options?.caseSensitive || false;
    const lowerQuery = caseSensitive ? query : query.toLowerCase();

    return actions.filter((action) => {
      return searchFields.some((field) => {
        let value = '';
        if (field === 'description') value = action.description;
        else if (field === 'metadata') value = JSON.stringify(action.metadata || {});
        else if (field === 'errorMessage') value = action.errorMessage || '';

        const searchValue = caseSensitive ? value : value.toLowerCase();
        return searchValue.includes(lowerQuery);
      });
    });
  }, []);

  // Set filters
  const setFilter = useCallback((filters: HistoryFilterOptions) => {
    setState((prev) => ({
      ...prev,
      filters,
      pagination: { ...prev.pagination, page: 0 }, // Reset to first page
    }));
  }, []);

  // Set search query
  const setSearch = useCallback((query: string) => {
    setState((prev) => ({
      ...prev,
      searchQuery: query,
      pagination: { ...prev.pagination, page: 0 }, // Reset to first page
    }));
  }, []);

  // Set pagination
  const setPagination = useCallback((pagination: PaginationOptions) => {
    setState((prev) => ({
      ...prev,
      pagination,
    }));
  }, []);

  // Toggle action selection
  const toggleSelection = useCallback((actionId: string) => {
    setState((prev) => {
      const newSelected = new Set(prev.selectedIds);
      if (newSelected.has(actionId)) {
        newSelected.delete(actionId);
      } else {
        newSelected.add(actionId);
      }
      return { ...prev, selectedIds: newSelected };
    });
  }, []);

  // Select all visible actions
  const selectAll = useCallback(() => {
    setState((prev) => {
      const filtered = filterActions(prev.filters);
      const searched = searchActions(prev.searchQuery, filtered);
      const newSelected = new Set(searched.map((action) => action.id));
      return { ...prev, selectedIds: newSelected };
    });
  }, [filterActions, searchActions]);

  // Clear selection
  const clearSelection = useCallback(() => {
    setState((prev) => ({
      ...prev,
      selectedIds: new Set(),
    }));
  }, []);

  // Get filtered and searched actions
  const filteredActions = useMemo(() => {
    let result = filterActions(state.filters);
    result = searchActions(state.searchQuery, result);
    return result;
  }, [state.filters, state.searchQuery, filterActions, searchActions]);

  // Paginate actions
  const paginatedActions = useMemo(() => {
    const start = state.pagination.page * state.pagination.pageSize;
    const end = start + state.pagination.pageSize;
    return filteredActions.slice(start, end);
  }, [filteredActions, state.pagination]);

  // Undo
  const undo = useCallback(async () => {
    if (state.currentIndex < 0) return false;

    const action = stateRef.current.actions[stateRef.current.currentIndex];
    if (!action) return false;

    try {
      await action.undo();
      setState((prev) => ({
        ...prev,
        currentIndex: Math.max(-1, prev.currentIndex - 1),
      }));
      logger.info('Action undone', { component: 'useEnhancedActionHistory', actionId: action.id });
      return true;
    } catch (error) {
      logger.error('Failed to undo action', { component: 'useEnhancedActionHistory', actionId: action.id }, toError(error));
      return false;
    }
  }, [state.currentIndex]);

  // Redo
  const redo = useCallback(async () => {
    if (state.currentIndex >= state.actions.length - 1) return false;

    const action = stateRef.current.actions[stateRef.current.currentIndex + 1];
    if (!action || !action.redo) return false;

    try {
      await action.redo();
      setState((prev) => ({
        ...prev,
        currentIndex: Math.min(prev.currentIndex + 1, prev.actions.length - 1),
      }));
      logger.info('Action redone', { component: 'useEnhancedActionHistory', actionId: action.id });
      return true;
    } catch (error) {
      logger.error('Failed to redo action', { component: 'useEnhancedActionHistory', actionId: action.id }, toError(error));
      return false;
    }
  }, [state.currentIndex, state.actions.length]);

  // Replay action
  const replayAction = useCallback(async (actionId: string, dryRun: boolean = false): Promise<boolean> => {
    const action = stateRef.current.actions.find((a) => a.id === actionId);
    if (!action) {
      logger.warn('Action not found for replay', { component: 'useEnhancedActionHistory', actionId });
      return false;
    }

    try {
      logger.info('Replaying action', { component: 'useEnhancedActionHistory', actionId, dryRun });

      if (!dryRun && action.redo) {
        await action.redo();
      }

      return true;
    } catch (error) {
      logger.error('Failed to replay action', { component: 'useEnhancedActionHistory', actionId }, toError(error));
      return false;
    }
  }, []);

  // Replay multiple actions
  const replayActions = useCallback(async (options: HistoryReplayOptions): Promise<ReplayResult> => {
    const startTime = Date.now();
    let successCount = 0;
    let failureCount = 0;
    let skippedCount = 0;
    const errors: ReplayResult['errors'] = [];

    logger.info('Starting action replay', {
      component: 'useEnhancedActionHistory',
      actionCount: options.actions.length,
      dryRun: options.dryRun,
      stopOnError: options.stopOnError,
    });

    for (let i = 0; i < options.actions.length; i++) {
      const action = options.actions[i];

      if (!action.redo) {
        skippedCount++;
        continue;
      }

      if (options.dryRun) {
        successCount++;
        continue;
      }

      try {
        await action.redo();
        successCount++;
      } catch (error) {
        failureCount++;
        errors.push({
          actionId: action.id,
          error: toError(error).message,
          index: i,
        });

        if (options.stopOnError) {
          logger.warn('Stopping replay due to error', { component: 'useEnhancedActionHistory', actionId: action.id });
          break;
        }
      }
    }

    const durationMs = Date.now() - startTime;

    logger.info('Action replay completed', {
      component: 'useEnhancedActionHistory',
      successCount,
      failureCount,
      skippedCount,
      durationMs,
    });

    return {
      totalActions: options.actions.length,
      successCount,
      failureCount,
      skippedCount,
      errors,
      durationMs,
    };
  }, []);

  // Export history
  const exportHistory = useCallback(async (options: HistoryExportOptions): Promise<string> => {
    let actionsToExport: ActionHistoryItem[] = [];

    if (options.scope === 'filtered') {
      actionsToExport = filterActions(state.filters);
      actionsToExport = searchActions(state.searchQuery, actionsToExport);
    } else if (options.scope === 'selected') {
      actionsToExport = state.actions.filter((action) => state.selectedIds.has(action.id));
    } else {
      actionsToExport = state.actions;
    }

    if (options.format === 'json') {
      return JSON.stringify(
        actionsToExport.map((action) => ({
          id: action.id,
          action: action.action,
          resource: action.resource,
          timestamp: new Date(action.timestamp).toISOString(),
          description: action.description,
          status: action.status,
          duration: action.duration,
          userId: action.userId,
          tenantId: action.tenantId,
          tags: action.tags,
          ...(options.includeMetadata && { metadata: action.metadata }),
          ...(action.errorMessage && { errorMessage: action.errorMessage }),
        })),
        null,
        2
      );
    } else if (options.format === 'csv') {
      const headers = ['ID', 'Action', 'Resource', 'Timestamp', 'Description', 'Status', 'Duration (ms)', 'User ID', 'Tenant ID', 'Tags'];
      const rows = actionsToExport.map((action) => [
        action.id,
        action.action,
        action.resource,
        new Date(action.timestamp).toISOString(),
        `"${(action.description || '').replace(/"/g, '""')}"`,
        action.status,
        action.duration || '',
        action.userId || '',
        action.tenantId || '',
        (action.tags || []).join(';'),
      ]);
      return [headers, ...rows].map((row) => row.join(',')).join('\n');
    } else if (options.format === 'markdown') {
      let markdown = '# Action History Export\n\n';
      markdown += `**Exported:** ${new Date().toISOString()}\n`;
      markdown += `**Total Actions:** ${actionsToExport.length}\n\n`;

      actionsToExport.forEach((action) => {
        markdown += `## ${action.description}\n\n`;
        markdown += `- **ID:** \`${action.id}\`\n`;
        markdown += `- **Action:** ${action.action}\n`;
        markdown += `- **Resource:** ${action.resource}\n`;
        markdown += `- **Timestamp:** ${new Date(action.timestamp).toISOString()}\n`;
        markdown += `- **Status:** ${action.status}\n`;
        if (action.duration) markdown += `- **Duration:** ${action.duration}ms\n`;
        if (action.userId) markdown += `- **User:** ${action.userId}\n`;
        if (action.tenantId) markdown += `- **Tenant:** ${action.tenantId}\n`;
        if (action.tags && action.tags.length > 0) markdown += `- **Tags:** ${action.tags.join(', ')}\n`;
        if (action.errorMessage) markdown += `- **Error:** ${action.errorMessage}\n`;
        markdown += '\n';
      });

      return markdown;
    }

    throw new Error(`Unsupported export format: ${options.format}`);
  }, [state.actions, state.filters, state.searchQuery, state.selectedIds, filterActions, searchActions]);

  // Calculate stats
  const stats = useMemo((): ActionStats => {
    const allActions = state.actions;
    const actionsByType: Record<ActionType, number> = {} as Record<ActionType, number>;
    const actionsByResource: Record<ResourceType, number> = {} as Record<ResourceType, number>;
    let successCount = 0;
    let totalDuration = 0;
    let actionCount = 0;

    // Initialize counters
    const actionTypes: ActionType[] = ['create', 'update', 'delete', 'load', 'unload', 'swap', 'train', 'deploy', 'rollback', 'configure', 'other'];
    const resourceTypes: ResourceType[] = ['adapter', 'stack', 'training', 'model', 'policy', 'node', 'tenant', 'other'];
    actionTypes.forEach((type) => { actionsByType[type] = 0; });
    resourceTypes.forEach((type) => { actionsByResource[type] = 0; });

    // Count actions
    allActions.forEach((action) => {
      actionsByType[action.action]++;
      actionsByResource[action.resource]++;
      if (action.status === 'success') successCount++;
      if (action.duration) totalDuration += action.duration;
      actionCount++;
    });

    // Build timeline
    const timelineMap = new Map<number, number>();
    const bucketSize = 3600000; // 1 hour
    allActions.forEach((action) => {
      const bucket = Math.floor(action.timestamp / bucketSize) * bucketSize;
      timelineMap.set(bucket, (timelineMap.get(bucket) || 0) + 1);
    });

    const actionsOverTime = Array.from(timelineMap.entries())
      .map(([timestamp, count]) => ({ timestamp, count }))
      .sort((a, b) => a.timestamp - b.timestamp);

    // Find most common action
    let mostCommonAction: ActionType | null = null;
    let maxCount = 0;
    for (const [type, count] of Object.entries(actionsByType)) {
      if (count > maxCount) {
        maxCount = count;
        mostCommonAction = type as ActionType;
      }
    }

    return {
      totalActions: actionCount,
      actionsByType,
      actionsByResource,
      successRate: actionCount > 0 ? (successCount / actionCount) * 100 : 0,
      averageDuration: actionCount > 0 ? totalDuration / actionCount : 0,
      mostCommonAction,
      actionsOverTime,
      recentActions: allActions.slice(-10).reverse(),
    };
  }, [state.actions]);

  // Clear history
  const clearHistory = useCallback(() => {
    setState((prev) => ({
      ...prev,
      actions: [],
      currentIndex: -1,
      selectedIds: new Set(),
    }));
    logger.info('History cleared', { component: 'useEnhancedActionHistory' });
  }, []);

  // Get action by ID
  const getActionById = useCallback((id: string): ActionHistoryItem | undefined => {
    return stateRef.current.actions.find((action) => action.id === id);
  }, []);

  const canUndo = state.currentIndex >= 0;
  const canRedo = state.currentIndex < state.actions.length - 1;

  return {
    // History management
    addAction,
    undo,
    redo,
    canUndo,
    canRedo,
    clearHistory,
    getActionById,

    // Filtering and search
    setFilter,
    setSearch,
    filterActions,
    searchActions,
    filteredActions,
    paginatedActions,

    // Selection
    toggleSelection,
    selectAll,
    clearSelection,
    selectedCount: state.selectedIds.size,
    isSelected: (id: string) => state.selectedIds.has(id),

    // Pagination
    setPagination,
    pagination: state.pagination,
    totalPages: Math.ceil(filteredActions.length / state.pagination.pageSize),

    // Current state
    allActions: state.actions,
    currentAction: state.actions[state.currentIndex] || null,
    historyCount: state.actions.length,

    // Replay
    replayAction,
    replayActions,

    // Export
    exportHistory,

    // Analytics
    stats,
  };
}

export default useEnhancedActionHistory;
