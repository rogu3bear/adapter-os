/**
 * ChatControlBar - Top control bar for the chat interface
 *
 * Handles stack selection, collection selection, adapter loading status,
 * toggle buttons for debugger/router activity, export functionality,
 * and adapter mount indicators.
 */

import React from 'react';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  Layers,
  History,
  ChevronLeft,
  Activity,
  Database,
  Bug,
  FileText,
} from 'lucide-react';
import { AdapterLoadingStatus } from './AdapterLoadingStatus';
import type { AdapterLifecycleState } from './AdapterLoadingStatus';
import { AdapterMountIndicators, type AdapterMountItem, type AdapterMountTransition } from './AdapterMountIndicators';
import { ChatTagsManager } from './ChatTagsManager';
import type { AdapterStack } from '@/api/adapter-types';
import type { Collection } from '@/api/document-types';
import type { RouterDecision } from '@/hooks/chat/useChatRouterDecisions';

// ============================================================================
// Types
// ============================================================================

export interface DocumentContext {
  documentId: string;
  documentName: string;
  collectionId?: string;
}

export interface DatasetContext {
  datasetId: string;
  datasetName: string;
  collectionId?: string;
}

/**
 * Flexible adapter state item that supports both legacy (adapterId) and new (id) formats
 */
export interface AdapterStateItem {
  id?: string;
  adapterId?: string;
  name: string;
  state: AdapterLifecycleState;
  isLoading?: boolean;
  error?: string;
}

export interface ChatControlBarProps {
  // Stack selection
  stacks: AdapterStack[];
  selectedStackId: string;
  onStackChange: (stackId: string) => void;
  stackSelectorRef?: React.RefObject<HTMLButtonElement>;

  // Selected stack details
  selectedStack?: AdapterStack | null;

  // Collection/Knowledge base selection
  collections: Collection[];
  selectedCollectionId: string | null;
  onCollectionChange: (collectionId: string | null) => void;

  // Base model loading
  isBaseOnlyMode?: boolean;
  autoLoadEnabled?: boolean;
  isLoadingModels?: boolean;
  baseModelLabel?: string;
  onLoadBaseModelOnly?: () => void;

  // Adapter states
  adapterStates: Map<string, AdapterStateItem>;

  // Adapter mount indicators
  adapterMountItems: AdapterMountItem[];
  adapterTransitions: AdapterMountTransition[];
  lastDecision?: RouterDecision | null;
  isStreaming?: boolean;

  // Toggle states and handlers
  isHistoryOpen: boolean;
  onToggleHistory: () => void;
  isChatHistoryUnsupported?: boolean;
  chatHistoryUnsupportedMessage?: string;

  isRouterActivityOpen: boolean;
  onToggleRouterActivity: () => void;

  isDebuggerOpen: boolean;
  onToggleDebugger: () => void;

  // Export
  currentSessionId?: string | null;
  messagesCount: number;
  ExportButton?: React.ComponentType;

  // Document/Dataset context badges
  documentContext?: DocumentContext | null;
  datasetContext?: DatasetContext | null;

  // Layout offsets
  rightPanelsOpen?: boolean;

  // Additional class names
  className?: string;
}

// ============================================================================
// Component
// ============================================================================

export function ChatControlBar({
  // Stack selection
  stacks,
  selectedStackId,
  onStackChange,
  stackSelectorRef,
  selectedStack,

  // Collection selection
  collections,
  selectedCollectionId,
  onCollectionChange,

  // Base model loading
  isBaseOnlyMode,
  autoLoadEnabled,
  isLoadingModels,
  baseModelLabel,
  onLoadBaseModelOnly,

  // Adapter states
  adapterStates,

  // Adapter mount indicators
  adapterMountItems,
  adapterTransitions,
  lastDecision,
  isStreaming,

  // Toggle states
  isHistoryOpen,
  onToggleHistory,
  isChatHistoryUnsupported,
  chatHistoryUnsupportedMessage,

  isRouterActivityOpen,
  onToggleRouterActivity,

  isDebuggerOpen,
  onToggleDebugger,

  // Export
  currentSessionId,
  messagesCount,
  ExportButton,

  // Context badges
  documentContext,
  datasetContext,

  // Layout
  rightPanelsOpen,

  className,
}: ChatControlBarProps) {
  const handleCollectionChange = (value: string) => {
    onCollectionChange(value === 'none' ? null : value);
  };

  return (
    <div className={className}>
      {/* Main control bar */}
      <div className={`border-b px-4 py-3 transition-all ${isHistoryOpen ? 'ml-80' : ''} ${rightPanelsOpen ? 'mr-96' : ''}`}>
        <div className="flex items-center justify-between mb-2">
          <div className="flex items-center gap-3">
            {/* History toggle */}
            <Button
              variant="ghost"
              size="sm"
              onClick={onToggleHistory}
              aria-label={
                isChatHistoryUnsupported
                  ? chatHistoryUnsupportedMessage
                  : isHistoryOpen
                    ? 'Close history'
                    : 'Open history'
              }
            >
              {isHistoryOpen ? (
                <ChevronLeft className="h-4 w-4" />
              ) : (
                <History className="h-4 w-4" />
              )}
            </Button>

            {isChatHistoryUnsupported && (
              <span className="text-xs text-muted-foreground">
                {chatHistoryUnsupportedMessage}
              </span>
            )}

            <Layers className="h-5 w-5 text-muted-foreground" aria-hidden="true" />

            {/* Document context badge */}
            {documentContext && (
              <Badge variant="secondary" className="gap-1">
                <FileText className="h-3 w-3" />
                {documentContext.documentName}
              </Badge>
            )}

            {/* Dataset context badge */}
            {datasetContext && (
              <Badge variant="secondary" className="gap-1">
                <Database className="h-3 w-3" />
                {datasetContext.datasetName}
              </Badge>
            )}

            {/* Stack selector */}
            <div className="flex items-center gap-2">
              <span className="text-sm text-muted-foreground">Stack:</span>
              <Select
                value={selectedStackId}
                onValueChange={onStackChange}
                aria-label="Select adapter stack"
                aria-describedby={stacks.length === 0 ? 'no-stacks-hint' : undefined}
              >
                <SelectTrigger
                  ref={stackSelectorRef}
                  className="w-[calc(var(--base-unit)*75)]"
                  aria-label="Select adapter stack"
                >
                  <SelectValue placeholder="Select a stack" />
                </SelectTrigger>
                <SelectContent>
                  {stacks.map((stack) => (
                    <SelectItem key={stack.id} value={stack.id}>
                      {stack.name}
                      {stack.description && (
                        <span className="text-muted-foreground ml-2">
                          ({stack.description})
                        </span>
                      )}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              {stacks.length === 0 && (
                <span id="no-stacks-hint" className="sr-only">
                  No adapter stacks available. Please create a stack first.
                </span>
              )}
            </div>

            {/* Load base model button */}
            {autoLoadEnabled && (
              <div className="flex items-center gap-2">
                <Button
                  variant="outline"
                  size="sm"
                  onClick={onLoadBaseModelOnly}
                  disabled={!selectedStackId || isLoadingModels}
                >
                  Load base model and chat without adapters
                </Button>
                {baseModelLabel && (
                  <span className="text-xs text-muted-foreground">{baseModelLabel}</span>
                )}
              </div>
            )}

            {/* Stack adapter count badge */}
            {selectedStack && (
              <Badge
                variant="outline"
                aria-label={`${selectedStack.adapter_ids?.length || 0} adapters in stack`}
              >
                {selectedStack.adapter_ids?.length || 0} adapter
                {(selectedStack.adapter_ids?.length || 0) !== 1 ? 's' : ''}
              </Badge>
            )}

            {/* Adapter loading status indicator */}
            {adapterStates.size > 0 && (
              <AdapterLoadingStatus
                stackId={selectedStackId}
                adapters={Array.from(adapterStates.values()).map((state) => ({
                  id: state.id ?? state.adapterId ?? '',
                  name: state.name,
                  state: state.state,
                  isLoading: state.isLoading,
                  error: state.error,
                }))}
                compact
              />
            )}

            <Database className="h-5 w-5 text-muted-foreground" aria-hidden="true" />

            {/* Collection/Knowledge base selector */}
            <div className="flex items-center gap-2">
              <span className="text-sm text-muted-foreground">Knowledge Base:</span>
              <Select
                value={selectedCollectionId || 'none'}
                onValueChange={handleCollectionChange}
                aria-label="Select knowledge base"
              >
                <SelectTrigger
                  className="w-[calc(var(--base-unit)*50)]"
                  aria-label="Select knowledge base"
                >
                  <SelectValue placeholder="No knowledge base" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="none">No knowledge base</SelectItem>
                  {collections.map((collection) => (
                    <SelectItem
                      key={collection.collection_id}
                      value={collection.collection_id}
                    >
                      {collection.name}
                      {collection.document_count > 0 && (
                        <span className="text-muted-foreground ml-2">
                          ({collection.document_count} docs)
                        </span>
                      )}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>

            {/* Router activity toggle */}
            <Button
              variant="ghost"
              size="sm"
              onClick={onToggleRouterActivity}
              aria-label={
                isRouterActivityOpen ? 'Close router activity' : 'Open router activity'
              }
              title="View router decision history"
            >
              <Activity className="h-4 w-4" />
            </Button>

            {/* Debugger toggle */}
            <Button
              variant={isDebuggerOpen ? 'secondary' : 'ghost'}
              size="sm"
              onClick={onToggleDebugger}
              aria-label={
                isDebuggerOpen ? 'Close neural debugger' : 'Open neural debugger'
              }
              title="Live neural debugger"
            >
              <Bug className="h-4 w-4" />
            </Button>

            {/* Export button */}
            {currentSessionId && messagesCount > 0 && ExportButton && <ExportButton />}
          </div>
        </div>

        {/* Session Tags */}
        {currentSessionId && (
          <div className="mt-2">
            <ChatTagsManager sessionId={currentSessionId} />
          </div>
        )}

        {/* Adapter mount indicators */}
        {adapterMountItems.length > 0 && (
          <div
            className={`px-4 pb-2 transition-all ${isHistoryOpen ? 'ml-80' : ''} ${rightPanelsOpen ? 'mr-96' : ''}`}
          >
            <AdapterMountIndicators
              adapters={adapterMountItems}
              transitions={adapterTransitions}
              activeAdapterId={lastDecision?.adapterId}
              isStreaming={isStreaming}
            />
          </div>
        )}
      </div>
    </div>
  );
}

export default ChatControlBar;
