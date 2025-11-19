//! Enhanced Action History Types
//!
//! Comprehensive types for advanced history management with filtering, search, and replay capabilities.

export type ActionType = 'create' | 'update' | 'delete' | 'load' | 'unload' | 'swap' | 'train' | 'deploy' | 'rollback' | 'configure' | 'other';
export type ResourceType = 'adapter' | 'stack' | 'training' | 'model' | 'policy' | 'node' | 'tenant' | 'other';
export type ActionStatus = 'pending' | 'success' | 'failed' | 'cancelled';

export interface ActionHistoryItem<T = any> {
  id: string;
  action: ActionType;
  resource: ResourceType;
  timestamp: number;
  description: string;
  status: ActionStatus;
  undo: () => Promise<void> | void;
  redo?: () => Promise<void> | void;
  metadata?: T;
  errorMessage?: string;
  duration?: number; // milliseconds
  userId?: string;
  tenantId?: string;
  tags?: string[];
}

export interface HistoryFilterOptions {
  actionTypes?: ActionType[];
  resourceTypes?: ResourceType[];
  statuses?: ActionStatus[];
  startDate?: number;
  endDate?: number;
  userIds?: string[];
  tenantIds?: string[];
  tags?: string[];
}

export interface HistorySearchOptions {
  query: string;
  searchFields?: ('description' | 'metadata' | 'errorMessage')[];
  caseSensitive?: boolean;
}

export interface PaginationOptions {
  page: number;
  pageSize: number;
}

export interface HistoryExportOptions {
  format: 'json' | 'csv' | 'markdown';
  scope: 'all' | 'filtered' | 'selected';
  includeMetadata?: boolean;
  dateRange?: {
    start: number;
    end: number;
  };
}

export interface HistoryReplayOptions {
  actions: ActionHistoryItem[];
  dryRun?: boolean;
  stopOnError?: boolean;
  batchSize?: number;
}

export interface ReplayResult {
  totalActions: number;
  successCount: number;
  failureCount: number;
  skippedCount: number;
  errors: {
    actionId: string;
    error: string;
    index: number;
  }[];
  durationMs: number;
}

export interface ActionStats {
  totalActions: number;
  actionsByType: Record<ActionType, number>;
  actionsByResource: Record<ResourceType, number>;
  successRate: number;
  averageDuration: number;
  mostCommonAction: ActionType | null;
  actionsOverTime: {
    timestamp: number;
    count: number;
  }[];
  recentActions: ActionHistoryItem[];
}

export interface HistoryStorageOptions {
  maxSize?: number; // max actions to keep
  persistToLocalStorage?: boolean;
  autoCleanup?: boolean;
  cleanupInterval?: number; // ms
}
