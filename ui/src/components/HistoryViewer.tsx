//! History Viewer Component
//!
//! Advanced timeline view of action history with filtering, search, and replay capabilities.

import React, { useState } from 'react';
import {
  Clock,
  CheckCircle,
  AlertCircle,
  RotateCcw,
  Download,
  Search,
  Filter,
  Copy,
  Play,
  BarChart3,
} from 'lucide-react';
import { Button } from './ui/button';
import { Input } from './ui/input';
import { Card } from './ui/card';
import { Badge } from './ui/badge';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs';
import { ScrollArea } from './ui/scroll-area';
import { Empty } from './ui/empty-state';
import { ExportDialog, ExportOptions } from './ui/export-dialog';
import { ConfirmationDialog } from './ui/confirmation-dialog';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from './ui/select';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from './ui/dropdown-menu';
import { logger } from '../utils/logger';
import useEnhancedActionHistory from '../hooks/useEnhancedActionHistory';
import {
  ActionHistoryItem,
  ActionType,
  ResourceType,
  ActionStatus,
  HistoryFilterOptions,
} from '../types/history';

interface HistoryViewerProps {
  onReplayAction?: (action: ActionHistoryItem) => Promise<boolean>;
  showStats?: boolean;
  showReplay?: boolean;
  maxVisible?: number;
}

export function HistoryViewer({
  onReplayAction,
  showStats = true,
  showReplay = true,
  maxVisible = 100,
}: HistoryViewerProps) {
  const {
    paginatedActions,
    filteredActions,
    setFilter,
    setSearch,
    stats,
    undo,
    redo,
    canUndo,
    canRedo,
    exportHistory,
    replayAction,
    replayActions,
    toggleSelection,
    selectAll,
    clearSelection,
    selectedCount,
    isSelected,
    clearHistory,
    setPagination,
    pagination,
    totalPages,
  } = useEnhancedActionHistory({ maxSize: maxVisible });

  const [searchQuery, setSearchQuery] = useState('');
  const [activeFilters, setActiveFilters] = useState<HistoryFilterOptions>({});
  const [showFilters, setShowFilters] = useState(false);
  const [showExport, setShowExport] = useState(false);
  const [showReplayConfirm, setShowReplayConfirm] = useState(false);
  const [showClearConfirm, setShowClearConfirm] = useState(false);
  const [selectedActionId, setSelectedActionId] = useState<string | null>(null);
  const [isReplaying, setIsReplaying] = useState(false);

  const handleSearch = (value: string) => {
    setSearchQuery(value);
    setSearch(value);
  };

  const handleFilterChange = (filters: HistoryFilterOptions) => {
    setActiveFilters(filters);
    setFilter(filters);
  };

  const handleExport = async (options: ExportOptions) => {
    try {
      const data = await exportHistory({
        format: options.format,
        scope: options.scope,
        includeMetadata: true,
      });

      const filename = `history-${Date.now()}.${options.format === 'markdown' ? 'md' : options.format}`;
      const element = document.createElement('a');
      element.setAttribute('href', `data:text/plain;charset=utf-8,${encodeURIComponent(data)}`);
      element.setAttribute('download', filename);
      element.style.display = 'none';
      document.body.appendChild(element);
      element.click();
      document.body.removeChild(element);

      logger.info('History exported', {
        component: 'HistoryViewer',
        format: options.format,
        count: filteredActions.length,
      });
    } catch (error) {
      logger.error('Failed to export history', { component: 'HistoryViewer' }, error as Error);
    }

    setShowExport(false);
  };

  const handleReplayAction = async (actionId: string) => {
    setSelectedActionId(actionId);
    setShowReplayConfirm(true);
  };

  const confirmReplayAction = async () => {
    if (!selectedActionId) return;

    setIsReplaying(true);
    try {
      const success = onReplayAction
        ? await onReplayAction(
          paginatedActions.find((a) => a.id === selectedActionId)!
        )
        : await replayAction(selectedActionId);

      if (success) {
        logger.info('Action replayed', {
          component: 'HistoryViewer',
          actionId: selectedActionId,
        });
      }
    } catch (error) {
      logger.error('Failed to replay action', { component: 'HistoryViewer' }, error as Error);
    } finally {
      setIsReplaying(false);
      setShowReplayConfirm(false);
    }
  };

  const handleClearHistory = async () => {
    clearHistory();
    setShowClearConfirm(false);
    logger.info('History cleared', { component: 'HistoryViewer' });
  };

  const getStatusIcon = (status: ActionStatus) => {
    switch (status) {
      case 'success':
        return <CheckCircle className="h-4 w-4 text-green-600" />;
      case 'failed':
        return <AlertCircle className="h-4 w-4 text-red-600" />;
      case 'cancelled':
        return <AlertCircle className="h-4 w-4 text-yellow-600" />;
      default:
        return <Clock className="h-4 w-4 text-blue-600" />;
    }
  };

  const getActionColor = (action: ActionType) => {
    const colors: Record<ActionType, string> = {
      create: 'bg-green-100 text-green-800',
      update: 'bg-blue-100 text-blue-800',
      delete: 'bg-red-100 text-red-800',
      load: 'bg-purple-100 text-purple-800',
      unload: 'bg-gray-100 text-gray-800',
      swap: 'bg-orange-100 text-orange-800',
      train: 'bg-cyan-100 text-cyan-800',
      deploy: 'bg-indigo-100 text-indigo-800',
      rollback: 'bg-rose-100 text-rose-800',
      configure: 'bg-amber-100 text-amber-800',
      other: 'bg-slate-100 text-slate-800',
    };
    return colors[action] || colors.other;
  };

  return (
    <div className="space-y-4 p-4">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-bold">Action History</h2>
          <p className="text-sm text-muted-foreground">
            {filteredActions.length} actions
            {Object.keys(activeFilters).length > 0 && ` (${Object.keys(activeFilters).length} filter${Object.keys(activeFilters).length === 1 ? '' : 's'})`}
          </p>
        </div>
        <div className="flex gap-2">
          <Button variant="outline" size="sm" onClick={undo} disabled={!canUndo} title="Undo (Cmd+Z)">
            <RotateCcw className="h-4 w-4 mr-2" />
            Undo
          </Button>
          <Button variant="outline" size="sm" onClick={redo} disabled={!canRedo} title="Redo (Cmd+Shift+Z)">
            <RotateCcw className="h-4 w-4 mr-2 rotate-180" />
            Redo
          </Button>
          {showStats && (
            <Button
              variant="outline"
              size="sm"
              asChild
            >
              <a href="#stats">
                <BarChart3 className="h-4 w-4 mr-2" />
                Stats
              </a>
            </Button>
          )}
        </div>
      </div>

      {/* Search and Filters */}
      <div className="flex gap-2">
        <div className="flex-1 relative">
          <Search className="absolute left-3 top-3 h-4 w-4 text-muted-foreground" />
          <Input
            placeholder="Search actions..."
            value={searchQuery}
            onChange={(e) => handleSearch(e.target.value)}
            className="pl-10"
          />
        </div>
        <Button
          variant={showFilters ? 'default' : 'outline'}
          size="sm"
          onClick={() => setShowFilters(!showFilters)}
        >
          <Filter className="h-4 w-4 mr-2" />
          Filters
        </Button>
        <Button
          variant="outline"
          size="sm"
          onClick={() => setShowExport(true)}
        >
          <Download className="h-4 w-4 mr-2" />
          Export
        </Button>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="outline" size="sm">More</Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            {selectedCount > 0 && (
              <>
                <DropdownMenuItem
                  onClick={() => clearSelection()}
                >
                  Clear Selection ({selectedCount})
                </DropdownMenuItem>
                {showReplay && (
                  <DropdownMenuItem
                    onClick={() => setShowReplayConfirm(true)}
                  >
                    <Play className="h-4 w-4 mr-2" />
                    Replay Selected
                  </DropdownMenuItem>
                )}
              </>
            )}
            <DropdownMenuItem
              onClick={() => setShowClearConfirm(true)}
              className="text-red-600"
            >
              Clear All History
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>

      {/* Filters Panel */}
      {showFilters && (
        <Card className="p-4 space-y-4">
          <div className="grid grid-cols-2 gap-4 md:grid-cols-4">
            {/* Action Type Filter */}
            <div>
              <label className="text-sm font-medium">Action Type</label>
              <Select
                value={activeFilters.actionTypes?.[0] || ''}
                onValueChange={(value) => {
                  if (value) {
                    handleFilterChange({ ...activeFilters, actionTypes: [value as ActionType] });
                  } else {
                    const { actionTypes, ...rest } = activeFilters;
                    handleFilterChange(rest);
                  }
                }}
              >
                <SelectTrigger>
                  <SelectValue placeholder="All" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="">All</SelectItem>
                  <SelectItem value="create">Create</SelectItem>
                  <SelectItem value="update">Update</SelectItem>
                  <SelectItem value="delete">Delete</SelectItem>
                  <SelectItem value="load">Load</SelectItem>
                  <SelectItem value="unload">Unload</SelectItem>
                </SelectContent>
              </Select>
            </div>

            {/* Resource Type Filter */}
            <div>
              <label className="text-sm font-medium">Resource Type</label>
              <Select
                value={activeFilters.resourceTypes?.[0] || ''}
                onValueChange={(value) => {
                  if (value) {
                    handleFilterChange({ ...activeFilters, resourceTypes: [value as ResourceType] });
                  } else {
                    const { resourceTypes, ...rest } = activeFilters;
                    handleFilterChange(rest);
                  }
                }}
              >
                <SelectTrigger>
                  <SelectValue placeholder="All" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="">All</SelectItem>
                  <SelectItem value="adapter">Adapter</SelectItem>
                  <SelectItem value="stack">Stack</SelectItem>
                  <SelectItem value="training">Training</SelectItem>
                  <SelectItem value="model">Model</SelectItem>
                </SelectContent>
              </Select>
            </div>

            {/* Status Filter */}
            <div>
              <label className="text-sm font-medium">Status</label>
              <Select
                value={activeFilters.statuses?.[0] || ''}
                onValueChange={(value) => {
                  if (value) {
                    handleFilterChange({ ...activeFilters, statuses: [value as ActionStatus] });
                  } else {
                    const { statuses, ...rest } = activeFilters;
                    handleFilterChange(rest);
                  }
                }}
              >
                <SelectTrigger>
                  <SelectValue placeholder="All" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="">All</SelectItem>
                  <SelectItem value="success">Success</SelectItem>
                  <SelectItem value="failed">Failed</SelectItem>
                  <SelectItem value="pending">Pending</SelectItem>
                </SelectContent>
              </Select>
            </div>

            {/* Clear Filters */}
            <div className="flex items-end">
              <Button
                variant="outline"
                size="sm"
                onClick={() => {
                  handleFilterChange({});
                  setSearchQuery('');
                }}
                className="w-full"
              >
                Clear Filters
              </Button>
            </div>
          </div>
        </Card>
      )}

      {/* Tabs for Different Views */}
      <Tabs defaultValue="timeline" className="space-y-4">
        <TabsList>
          <TabsTrigger value="timeline">Timeline</TabsTrigger>
          <TabsTrigger value="list">List</TabsTrigger>
          {showStats && <TabsTrigger value="stats">Analytics</TabsTrigger>}
        </TabsList>

        {/* Timeline View */}
        <TabsContent value="timeline" className="space-y-4">
          {paginatedActions.length === 0 ? (
            <Empty
              title="No actions yet"
              description="Actions will appear here as you interact with the system"
            />
          ) : (
            <ScrollArea className="h-[600px] border rounded-lg p-4">
              <div className="space-y-4">
                {paginatedActions.map((action, index) => (
                  <div key={action.id} className="flex gap-4">
                    <div className="flex flex-col items-center">
                      {getStatusIcon(action.status)}
                      {index < paginatedActions.length - 1 && (
                        <div className="h-8 w-0.5 bg-border my-2" />
                      )}
                    </div>
                    <Card className="flex-1 p-3 cursor-pointer hover:shadow-md transition-shadow">
                      <div className="flex items-start justify-between gap-2">
                        <div className="flex-1 space-y-1">
                          <div className="flex items-center gap-2">
                            <Badge className={getActionColor(action.action)}>
                              {action.action}
                            </Badge>
                            <Badge variant="outline">{action.resource}</Badge>
                            <span className="text-xs text-muted-foreground">
                              {new Date(action.timestamp).toLocaleString()}
                            </span>
                          </div>
                          <p className="text-sm font-medium">{action.description}</p>
                          {action.duration && (
                            <p className="text-xs text-muted-foreground">
                              Duration: {action.duration}ms
                            </p>
                          )}
                          {action.errorMessage && (
                            <p className="text-xs text-red-600">{action.errorMessage}</p>
                          )}
                        </div>
                        <div className="flex gap-1">
                          <Button
                            variant="ghost"
                            size="sm"
                            onClick={() => toggleSelection(action.id)}
                            className={isSelected(action.id) ? 'bg-accent' : ''}
                          >
                            <Copy className="h-4 w-4" />
                          </Button>
                          {showReplay && action.redo && (
                            <Button
                              variant="ghost"
                              size="sm"
                              onClick={() => handleReplayAction(action.id)}
                              disabled={isReplaying}
                            >
                              <Play className="h-4 w-4" />
                            </Button>
                          )}
                        </div>
                      </div>
                    </Card>
                  </div>
                ))}
              </div>
            </ScrollArea>
          )}
        </TabsContent>

        {/* List View */}
        <TabsContent value="list">
          {paginatedActions.length === 0 ? (
            <Empty
              title="No actions found"
              description="Try adjusting your filters or search query"
            />
          ) : (
            <div className="space-y-2">
              {paginatedActions.map((action) => (
                <Card key={action.id} className="p-3 hover:shadow-md transition-shadow">
                  <div className="flex items-center justify-between gap-2">
                    <div className="flex items-center gap-2 flex-1">
                      {getStatusIcon(action.status)}
                      <div className="flex-1">
                        <p className="text-sm font-medium">{action.description}</p>
                        <p className="text-xs text-muted-foreground">
                          {new Date(action.timestamp).toLocaleString()}
                        </p>
                      </div>
                    </div>
                    <div className="flex items-center gap-2">
                      <Badge variant="outline">{action.action}</Badge>
                      <Badge variant="outline">{action.resource}</Badge>
                    </div>
                  </div>
                </Card>
              ))}
            </div>
          )}
        </TabsContent>

        {/* Stats View */}
        {showStats && (
          <TabsContent value="stats" id="stats" className="space-y-4">
            <div className="grid grid-cols-2 gap-4 md:grid-cols-4">
              <Card className="p-4 space-y-1">
                <p className="text-sm text-muted-foreground">Total Actions</p>
                <p className="text-2xl font-bold">{stats.totalActions}</p>
              </Card>
              <Card className="p-4 space-y-1">
                <p className="text-sm text-muted-foreground">Success Rate</p>
                <p className="text-2xl font-bold">{stats.successRate.toFixed(1)}%</p>
              </Card>
              <Card className="p-4 space-y-1">
                <p className="text-sm text-muted-foreground">Avg Duration</p>
                <p className="text-2xl font-bold">{stats.averageDuration.toFixed(0)}ms</p>
              </Card>
              <Card className="p-4 space-y-1">
                <p className="text-sm text-muted-foreground">Most Common</p>
                <p className="text-2xl font-bold capitalize">{stats.mostCommonAction || 'N/A'}</p>
              </Card>
            </div>

            <Card className="p-4">
              <h3 className="font-semibold mb-4">Action Distribution</h3>
              <div className="space-y-2">
                {Object.entries(stats.actionsByType)
                  .filter(([, count]) => count > 0)
                  .sort((a, b) => b[1] - a[1])
                  .map(([type, count]) => (
                    <div key={type} className="flex items-center gap-2">
                      <span className="text-sm w-20">{type}</span>
                      <div className="flex-1 h-2 bg-muted rounded-full overflow-hidden">
                        <div
                          className="h-full bg-blue-500"
                          style={{
                            width: `${(count / stats.totalActions) * 100}%`,
                          }}
                        />
                      </div>
                      <span className="text-sm font-medium w-8 text-right">{count}</span>
                    </div>
                  ))}
              </div>
            </Card>
          </TabsContent>
        )}
      </Tabs>

      {/* Pagination */}
      {totalPages > 1 && (
        <div className="flex items-center justify-center gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={() => setPagination({ ...pagination, page: Math.max(0, pagination.page - 1) })}
            disabled={pagination.page === 0}
          >
            Previous
          </Button>
          <span className="text-sm text-muted-foreground">
            Page {pagination.page + 1} of {totalPages}
          </span>
          <Button
            variant="outline"
            size="sm"
            onClick={() => setPagination({ ...pagination, page: Math.min(totalPages - 1, pagination.page + 1) })}
            disabled={pagination.page === totalPages - 1}
          >
            Next
          </Button>
        </div>
      )}

      {/* Export Dialog */}
      <ExportDialog
        open={showExport}
        onOpenChange={setShowExport}
        onExport={handleExport}
        itemName="actions"
        hasFilters={Object.keys(activeFilters).length > 0}
        defaultFormat="json"
      />

      {/* Replay Confirmation */}
      <ConfirmationDialog
        open={showReplayConfirm}
        onOpenChange={setShowReplayConfirm}
        title="Replay Action"
        description="This will execute the action again. Are you sure?"
        onConfirm={confirmReplayAction}
        isLoading={isReplaying}
      />

      {/* Clear History Confirmation */}
      <ConfirmationDialog
        open={showClearConfirm}
        onOpenChange={setShowClearConfirm}
        title="Clear History"
        description="This will permanently delete all action history. This cannot be undone."
        onConfirm={handleClearHistory}
        isDangerous
      />
    </div>
  );
}

export default HistoryViewer;
