import React, { useState, useCallback, useMemo } from 'react';
import { History, X, Plus, Archive, Trash2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { ScrollArea } from '@/components/ui/scroll-area';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';
import { ChatSearchBar } from './ChatSearchBar';
import { ChatSessionActions } from './ChatSessionActions';
import type { ChatSession } from '@/types/chat';
import { logger } from '@/utils/logger';

/**
 * Props for ChatHistorySidebar component
 */
export interface ChatHistorySidebarProps {
  /** List of chat sessions to display */
  sessions: ChatSession[];
  /** Currently active session ID */
  activeSessionId: string | null;
  /** Whether the sidebar is open */
  isOpen: boolean;
  /** Tenant ID for session actions */
  tenantId: string;
  /** Whether sessions are loading */
  isLoadingSessions?: boolean;
  /** Whether a stack is selected (required for creating new sessions) */
  hasSelectedStack: boolean;
  /** Session ID that is currently streaming (delete disabled for this session) */
  activeStreamingSessionId?: string | null;
  /** Callback when sidebar is closed */
  onClose: () => void;
  /** Callback when a session is loaded */
  onLoadSession: (sessionId: string) => void;
  /** Callback when a new session is created */
  onCreateSession: () => void;
  /** Callback when a session is deleted */
  onDeleteSession: (sessionId: string, event: React.MouseEvent) => void;
  /** Callback when a session is renamed */
  onRenameSession: (sessionId: string, newName: string) => void;
  /** Callback when archive panel should open */
  onOpenArchive: () => void;
  /** Callback when tags dialog should open for a session */
  onManageTags: (sessionId: string) => void;
  /** Callback when category dialog should open for a session */
  onSetCategory: (sessionId: string) => void;
  /** Callback when share dialog should open for a session */
  onShare: (sessionId: string) => void;
  /** Function to generate a preview text for a session */
  getSessionPreview?: (session: ChatSession) => string;
}

/**
 * Default function to generate a session preview from the first user message
 */
const defaultGetSessionPreview = (session: ChatSession): string => {
  const firstUserMessage = session.messages.find(m => m.role === 'user');
  if (firstUserMessage) {
    return firstUserMessage.content.slice(0, 50) + (firstUserMessage.content.length > 50 ? '...' : '');
  }
  return 'No messages yet';
};

/**
 * ChatHistorySidebar - Sidebar component for displaying and managing chat session history
 *
 * Features:
 * - Session list with search/filter capability
 * - Session creation, deletion, and renaming
 * - Session preview with message count and date
 * - Actions menu for tags, categories, sharing, and archive
 * - Keyboard navigation for session editing
 *
 * @example
 * ```tsx
 * <ChatHistorySidebar
 *   sessions={sessions}
 *   activeSessionId={currentSessionId}
 *   isOpen={isHistoryOpen}
 *   tenantId={tenantId}
 *   hasSelectedStack={!!selectedStackId}
 *   onClose={() => setIsHistoryOpen(false)}
 *   onLoadSession={handleLoadSession}
 *   onCreateSession={handleCreateSession}
 *   onDeleteSession={handleDeleteSession}
 *   onRenameSession={handleRenameSession}
 *   onOpenArchive={() => setIsArchivePanelOpen(true)}
 *   onManageTags={(id) => setTagsDialogSessionId(id)}
 *   onSetCategory={(id) => setCategoryDialogSessionId(id)}
 *   onShare={(id) => setShareDialogSessionId(id)}
 * />
 * ```
 */
export function ChatHistorySidebar({
  sessions,
  activeSessionId,
  isOpen,
  tenantId,
  isLoadingSessions = false,
  hasSelectedStack,
  activeStreamingSessionId,
  onClose,
  onLoadSession,
  onCreateSession,
  onDeleteSession,
  onRenameSession,
  onOpenArchive,
  onManageTags,
  onSetCategory,
  onShare,
  getSessionPreview = defaultGetSessionPreview,
}: ChatHistorySidebarProps) {
  // Internal state for editing
  const [editingSessionId, setEditingSessionId] = useState<string | null>(null);
  const [newSessionName, setNewSessionName] = useState('');

  // Internal state for search/filtering
  const [searchQuery, setSearchQuery] = useState('');

  // Filter sessions based on search query
  const filteredSessions = useMemo(() => {
    if (!searchQuery.trim()) {
      return sessions;
    }
    const query = searchQuery.toLowerCase();
    return sessions.filter(session =>
      session.name.toLowerCase().includes(query) ||
      session.messages.some(msg =>
        msg.content.toLowerCase().includes(query)
      )
    );
  }, [sessions, searchQuery]);

  // Handle starting edit mode for a session
  const handleStartEdit = useCallback((sessionId: string, currentName: string) => {
    setEditingSessionId(sessionId);
    setNewSessionName(currentName);
  }, []);

  // Handle completing edit (blur or enter)
  const handleCompleteEdit = useCallback((sessionId: string) => {
    const trimmedName = newSessionName.trim();
    if (trimmedName) {
      onRenameSession(sessionId, trimmedName);
    }
    setEditingSessionId(null);
    setNewSessionName('');
  }, [newSessionName, onRenameSession]);

  // Handle canceling edit
  const handleCancelEdit = useCallback(() => {
    setEditingSessionId(null);
    setNewSessionName('');
  }, []);

  // Handle keyboard events during editing
  const handleEditKeyDown = useCallback((e: React.KeyboardEvent, sessionId: string) => {
    if (e.key === 'Enter' && newSessionName.trim()) {
      handleCompleteEdit(sessionId);
    } else if (e.key === 'Escape') {
      handleCancelEdit();
    }
  }, [newSessionName, handleCompleteEdit, handleCancelEdit]);

  // Handle session selection from search
  const handleSelectSession = useCallback((sessionId: string) => {
    onLoadSession(sessionId);
  }, [onLoadSession]);

  // Handle message selection from search (load session and optionally scroll to message)
  const handleSelectMessage = useCallback((sessionId: string, messageId: string) => {
    onLoadSession(sessionId);
    // TODO: After loading, scroll to the specific message
    // For now, just load the session - message scrolling can be added later
    if (messageId) {
      logger.info('Search navigated to message', { sessionId, messageId });
    }
  }, [onLoadSession]);

  if (!isOpen) {
    return null;
  }

  return (
    <SectionErrorBoundary sectionName="Session History">
      <div className="absolute left-0 top-0 bottom-0 w-80 bg-background border-r z-10 flex flex-col">
        {/* Header */}
        <div className="border-b px-4 py-3 flex items-center justify-between">
          <h3 className="font-semibold text-sm flex items-center gap-2">
            <History className="h-4 w-4" />
            Conversation History
          </h3>
          <div className="flex items-center gap-2">
            <Button
              variant="ghost"
              size="sm"
              onClick={onOpenArchive}
              aria-label="Open archive"
              title="View archived sessions"
            >
              <Archive className="h-4 w-4" />
            </Button>
            <Button
              variant="ghost"
              size="sm"
              onClick={onClose}
              aria-label="Close history"
            >
              <X className="h-4 w-4" />
            </Button>
          </div>
        </div>

        {/* Search Bar */}
        <div className="px-4 py-2 border-b">
          <ChatSearchBar
            value={searchQuery}
            onChange={setSearchQuery}
            onSelectSession={handleSelectSession}
            onSelectMessage={handleSelectMessage}
            placeholder="Search sessions..."
          />
        </div>

        {/* Create New Session */}
        <div className="border-b px-4 py-2">
          <Button
            variant="outline"
            size="sm"
            className="w-full"
            onClick={onCreateSession}
            disabled={!hasSelectedStack}
          >
            <Plus className="h-4 w-4 mr-2" />
            New Session
          </Button>
        </div>

        {/* Session List */}
        <ScrollArea className="flex-1">
          <div className="p-2 space-y-1">
            {isLoadingSessions ? (
              <div className="text-center py-8 text-sm text-muted-foreground">
                Loading sessions...
              </div>
            ) : filteredSessions.length === 0 ? (
              <div className="text-center py-8 text-sm text-muted-foreground">
                {searchQuery ? 'No matching sessions' : 'No conversation history'}
              </div>
            ) : (
              filteredSessions.map(session => (
                <div
                  key={session.id}
                  role="button"
                  tabIndex={0}
                  className={`group p-3 rounded-lg border cursor-pointer transition-colors hover:bg-muted ${
                    activeSessionId === session.id ? 'bg-muted border-primary' : ''
                  }`}
                  onClick={() => onLoadSession(session.id)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                      e.preventDefault();
                      onLoadSession(session.id);
                    }
                  }}
                >
                  <div className="flex items-start justify-between gap-2">
                    <div className="flex-1 min-w-0">
                      {editingSessionId === session.id ? (
                        <Input
                          value={newSessionName}
                          onChange={(e) => setNewSessionName(e.target.value)}
                          onBlur={() => {
                            if (newSessionName.trim()) {
                              handleCompleteEdit(session.id);
                            } else {
                              handleCancelEdit();
                            }
                          }}
                          onKeyDown={(e) => handleEditKeyDown(e, session.id)}
                          className="h-7 text-sm mb-1"
                          autoFocus
                          onClick={(e) => e.stopPropagation()}
                        />
                      ) : (
                        <>
                          <div className="flex items-center justify-between">
                            <p className="text-sm font-medium truncate">{session.name}</p>
                            <div
                              role="button"
                              tabIndex={0}
                              className="flex items-center gap-1 ml-2"
                              onClick={(e) => e.stopPropagation()}
                              onKeyDown={(e) => e.stopPropagation()}
                            >
                              <Button
                                variant="ghost"
                                size="icon"
                                className="h-6 w-6 opacity-0 group-hover:opacity-100 transition-opacity"
                                onClick={(e) => onDeleteSession(session.id, e)}
                                disabled={session.id === activeStreamingSessionId}
                                aria-label={`Delete session ${session.name}`}
                                title={session.id === activeStreamingSessionId ? 'Cannot delete while streaming' : undefined}
                              >
                                <Trash2 className="h-3 w-3 text-destructive" />
                              </Button>
                              <ChatSessionActions
                                sessionId={session.id}
                                tenantId={tenantId}
                                onRename={() => handleStartEdit(session.id, session.name)}
                                onManageTags={() => onManageTags(session.id)}
                                onSetCategory={() => onSetCategory(session.id)}
                                onShare={() => onShare(session.id)}
                              />
                            </div>
                          </div>
                          <p className="text-xs text-muted-foreground mt-1 line-clamp-2">
                            {getSessionPreview(session)}
                          </p>
                          <div className="flex items-center gap-2 mt-2">
                            <span className="text-xs text-muted-foreground">
                              {session.messages.length} message{session.messages.length !== 1 ? 's' : ''}
                            </span>
                            <span className="text-xs text-muted-foreground">&#x2022;</span>
                            <span className="text-xs text-muted-foreground">
                              {new Date(session.updatedAt).toLocaleDateString()}
                            </span>
                          </div>
                        </>
                      )}
                    </div>
                  </div>
                </div>
              ))
            )}
          </div>
        </ScrollArea>
      </div>
    </SectionErrorBoundary>
  );
}

export default ChatHistorySidebar;
