/**
 * DatasetsTab - Datasets grid/list with search for the Workbench left rail
 *
 * Displays available datasets in a folder-like grid view (default) or table view.
 * Clicking a card opens the detail drawer; clicking "Talk" scopes the chat.
 */

import { useState, useMemo, useCallback, useEffect, useRef } from 'react';
import { useNavigate } from 'react-router-dom';
import { Search, Database, X, LayoutGrid, List } from 'lucide-react';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { cn } from '@/lib/utils';
import { useDatasetChatOptional } from '@/contexts/DatasetChatContext';
import { useWorkbench } from '@/contexts/WorkbenchContext';
import { useTraining } from '@/hooks/training';
import { DatasetCard } from './DatasetCard';
import { DatasetDetailDrawer } from './DatasetDetailDrawer';
import type { Dataset } from '@/api/training-types';

// Storage key for view mode persistence
const VIEW_MODE_STORAGE_KEY = 'datasetLibrary:viewMode';

type ViewMode = 'grid' | 'list';

interface DatasetsTabProps {
  /** Callback when a dataset is selected */
  onSelectDataset?: (dataset: {
    id: string;
    name: string;
    collectionId?: string;
    versionId?: string;
  }) => void;
  /** Callback to clear the active dataset */
  onClearDataset?: () => void;
}

// Read view mode from localStorage
function getStoredViewMode(): ViewMode {
  if (typeof window === 'undefined') return 'grid';
  try {
    const stored = window.localStorage.getItem(VIEW_MODE_STORAGE_KEY);
    if (stored === 'grid' || stored === 'list') return stored;
  } catch {
    // ignore
  }
  return 'grid';
}

export function DatasetsTab({
  onSelectDataset,
  onClearDataset,
}: DatasetsTabProps) {
  const navigate = useNavigate();
  const [searchQuery, setSearchQuery] = useState('');
  const [viewMode, setViewModeState] = useState<ViewMode>(getStoredViewMode);
  const [selectedDataset, setSelectedDataset] = useState<Dataset | null>(null);
  const [isDrawerOpen, setIsDrawerOpen] = useState(false);
  const [focusedIndex, setFocusedIndex] = useState<number>(-1);
  const gridRef = useRef<HTMLDivElement>(null);

  const datasetContext = useDatasetChatOptional();
  const { setActiveLeftTab } = useWorkbench();
  const { useDatasets, useValidateDataset } = useTraining;
  const { data: datasetsResponse, isLoading } = useDatasets();
  const validateDataset = useValidateDataset();

  // Persist view mode to localStorage
  const setViewMode = useCallback((mode: ViewMode) => {
    setViewModeState(mode);
    try {
      window.localStorage.setItem(VIEW_MODE_STORAGE_KEY, mode);
    } catch {
      // ignore
    }
  }, []);

  const datasets = datasetsResponse?.datasets ?? [];

  const filteredDatasets = useMemo(() => {
    if (!searchQuery.trim()) return datasets;
    const query = searchQuery.toLowerCase();
    return datasets.filter(
      (dataset) =>
        dataset.name.toLowerCase().includes(query) ||
        dataset.description?.toLowerCase().includes(query)
    );
  }, [datasets, searchQuery]);

  const activeDatasetId = datasetContext?.activeDatasetId;

  // Handle "Talk to dataset" action - sets scope, switches to Sessions tab, focuses input
  const handleTalkToDataset = useCallback(
    (dataset: Dataset) => {
      // 1. Set dataset scope in context
      if (datasetContext) {
        datasetContext.setActiveDataset({
          id: dataset.id,
          name: dataset.name,
          versionId: dataset.dataset_version_id ?? undefined,
        });
      }
      onSelectDataset?.({
        id: dataset.id,
        name: dataset.name,
        versionId: dataset.dataset_version_id ?? undefined,
      });

      // 2. Close drawer if open
      setIsDrawerOpen(false);

      // 3. Switch to Sessions tab
      setActiveLeftTab('sessions');

      // 4. Focus chat input after DOM update
      requestAnimationFrame(() => {
        const chatInput = document.querySelector<HTMLElement>(
          '[data-testid="chat-input"], textarea[placeholder*="Message"], input[placeholder*="Message"]'
        );
        chatInput?.focus();
      });
    },
    [datasetContext, onSelectDataset, setActiveLeftTab]
  );

  // Handle card click - opens drawer
  const handleCardSelect = useCallback((dataset: Dataset) => {
    setSelectedDataset(dataset);
    setIsDrawerOpen(true);
  }, []);

  // Handle clear active dataset
  const handleClearDataset = useCallback(() => {
    if (datasetContext) {
      datasetContext.clearActiveDataset();
    }
    onClearDataset?.();
  }, [datasetContext, onClearDataset]);

  // Handle validate action
  const handleValidate = useCallback(
    (datasetId: string) => {
      validateDataset.mutate(datasetId);
    },
    [validateDataset]
  );

  // Handle train action - navigate to dataset detail page where training can be started
  const handleTrain = useCallback(
    (datasetId: string) => {
      setIsDrawerOpen(false);
      navigate(`/training/datasets/${datasetId}`);
    },
    [navigate]
  );

  // Keyboard navigation for grid view
  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (viewMode !== 'grid' || filteredDatasets.length === 0) return;

      const cols = 2; // 2-column grid
      const total = filteredDatasets.length;

      switch (e.key) {
        case 'ArrowRight':
          e.preventDefault();
          setFocusedIndex((prev) => (prev + 1) % total);
          break;
        case 'ArrowLeft':
          e.preventDefault();
          setFocusedIndex((prev) => (prev - 1 + total) % total);
          break;
        case 'ArrowDown':
          e.preventDefault();
          setFocusedIndex((prev) => Math.min(prev + cols, total - 1));
          break;
        case 'ArrowUp':
          e.preventDefault();
          setFocusedIndex((prev) => Math.max(prev - cols, 0));
          break;
        case 'Enter':
        case ' ':
          e.preventDefault();
          if (focusedIndex >= 0 && focusedIndex < total) {
            handleCardSelect(filteredDatasets[focusedIndex]);
          }
          break;
        case 'Escape':
          if (isDrawerOpen) {
            e.preventDefault();
            setIsDrawerOpen(false);
          }
          break;
      }
    },
    [viewMode, filteredDatasets, focusedIndex, isDrawerOpen, handleCardSelect]
  );

  // Focus the correct card when focusedIndex changes
  useEffect(() => {
    if (focusedIndex >= 0 && gridRef.current) {
      const card = gridRef.current.querySelector(
        `[data-card-index="${focusedIndex}"]`
      ) as HTMLElement;
      card?.focus();
    }
  }, [focusedIndex]);

  return (
    <div className="flex h-full flex-col" data-testid="datasets-tab">
      {/* Header with search and view toggle */}
      <div className="flex-none space-y-2 p-3 border-b">
        <div className="flex items-center gap-2">
          <div className="relative flex-1">
            <Search className="absolute left-2.5 top-2.5 h-4 w-4 text-muted-foreground" />
            <Input
              placeholder="Search datasets..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="pl-8 h-9"
              data-testid="datasets-search"
            />
          </div>
          {/* View toggle */}
          <div className="flex items-center border rounded-md">
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant={viewMode === 'grid' ? 'secondary' : 'ghost'}
                  size="icon"
                  className="h-8 w-8 rounded-r-none"
                  onClick={() => setViewMode('grid')}
                  data-testid="view-grid-button"
                >
                  <LayoutGrid className="h-4 w-4" />
                </Button>
              </TooltipTrigger>
              <TooltipContent>Grid view</TooltipContent>
            </Tooltip>
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant={viewMode === 'list' ? 'secondary' : 'ghost'}
                  size="icon"
                  className="h-8 w-8 rounded-l-none border-l"
                  onClick={() => setViewMode('list')}
                  data-testid="view-list-button"
                >
                  <List className="h-4 w-4" />
                </Button>
              </TooltipTrigger>
              <TooltipContent>List view</TooltipContent>
            </Tooltip>
          </div>
        </div>

        {/* Active dataset indicator */}
        {activeDatasetId && (
          <div className="flex items-center justify-between p-2 rounded-md bg-primary/10 border border-primary/20">
            <div className="flex items-center gap-2 min-w-0">
              <Database className="h-4 w-4 text-primary flex-none" />
              <span className="text-sm font-medium truncate">
                {datasetContext?.activeDatasetName ?? 'Active'}
              </span>
            </div>
            <Button
              variant="ghost"
              size="icon"
              className="h-6 w-6 flex-none"
              onClick={handleClearDataset}
              data-testid="clear-dataset-button"
            >
              <X className="h-3.5 w-3.5" />
            </Button>
          </div>
        )}
      </div>

      {/* Datasets grid/list */}
      <ScrollArea className="flex-1">
        {isLoading ? (
          <div className="p-4 text-center text-sm text-muted-foreground">
            Loading datasets...
          </div>
        ) : filteredDatasets.length === 0 ? (
          <div className="p-4 text-center text-sm text-muted-foreground">
            {searchQuery ? 'No datasets found' : 'No datasets available'}
          </div>
        ) : viewMode === 'grid' ? (
          // Grid view with keyboard navigation
          <div
            ref={gridRef}
            className="grid grid-cols-2 gap-2 p-2"
            onKeyDown={handleKeyDown}
            role="grid"
            aria-label="Datasets grid"
          >
            {filteredDatasets.map((dataset, index) => (
              <div
                key={dataset.id}
                data-card-index={index}
                tabIndex={index === focusedIndex ? 0 : -1}
                className="outline-none"
              >
                <DatasetCard
                  dataset={dataset}
                  isActive={dataset.id === activeDatasetId}
                  onSelect={() => handleCardSelect(dataset)}
                  onTalk={() => handleTalkToDataset(dataset)}
                />
              </div>
            ))}
          </div>
        ) : (
          // List view (compact)
          <div className="p-2 space-y-1">
            {filteredDatasets.map((dataset) => (
              <DatasetListItem
                key={dataset.id}
                dataset={dataset}
                isActive={dataset.id === activeDatasetId}
                onSelect={() => handleCardSelect(dataset)}
                onTalk={() => handleTalkToDataset(dataset)}
              />
            ))}
          </div>
        )}
      </ScrollArea>

      {/* Detail drawer */}
      <DatasetDetailDrawer
        dataset={selectedDataset}
        isOpen={isDrawerOpen}
        onClose={() => setIsDrawerOpen(false)}
        onTalk={handleTalkToDataset}
        onValidate={handleValidate}
        onTrain={handleTrain}
      />
    </div>
  );
}

// List item component for compact view
interface DatasetListItemProps {
  dataset: Dataset;
  isActive: boolean;
  onSelect: () => void;
  onTalk: () => void;
}

function DatasetListItem({ dataset, isActive, onSelect, onTalk }: DatasetListItemProps) {
  const isValid = dataset.validation_status === 'valid';

  const handleTalkClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (isValid) {
      onTalk();
    }
  };

  return (
    <div
      className={cn(
        'group flex items-center gap-2 rounded-md px-2 py-2 cursor-pointer transition-colors',
        isActive ? 'bg-primary/10 border border-primary/20' : 'hover:bg-muted'
      )}
      onClick={onSelect}
      role="button"
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          onSelect();
        }
      }}
      data-testid={`dataset-${dataset.id}`}
    >
      <Database className="h-4 w-4 flex-none text-muted-foreground" />
      <div className="flex-1 min-w-0">
        <span className="font-medium text-sm truncate block">{dataset.name}</span>
        {dataset.dataset_version_id && (
          <span className="text-[10px] text-muted-foreground font-mono">
            v:{dataset.dataset_version_id.slice(0, 8)}
          </span>
        )}
      </div>
      {isValid && (
        <Button
          variant="ghost"
          size="icon"
          className="h-6 w-6 opacity-0 group-hover:opacity-100 transition-opacity"
          onClick={handleTalkClick}
          title="Talk to this dataset"
        >
          <Database className="h-3.5 w-3.5" />
        </Button>
      )}
    </div>
  );
}

export default DatasetsTab;
