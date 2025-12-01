import React, { useState, useRef, useEffect, useCallback, useMemo, useRef as useReactRef } from 'react';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';
import { Input } from '@/components/ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Badge } from '@/components/ui/badge';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { ChatMessageComponent, type ChatMessage, type EvidenceItem } from './chat/ChatMessage';
import apiClient from '@/api/client';
import { logger, toError } from '@/utils/logger';
import { toast } from 'sonner';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';
import { Send, Loader2, Layers, History, X, ChevronLeft, ChevronRight, Plus, Trash2, Edit2, Activity, Database, Archive } from 'lucide-react';
import { useAdapterStacks, useGetDefaultStack } from '@/hooks/useAdmin';
import { useChatSessionsApi } from '@/hooks/useChatSessionsApi';
import { useCollections } from '@/hooks/useCollectionsApi';
import { useQueryClient } from '@tanstack/react-query';
import { useDebouncedCallback } from '@/hooks/useDebouncedValue';
import type { AdapterStack, RoutingDecision, RouterCandidateInfo, ExtendedRouterDecision } from '@/api/types';
import type { ChatSession } from '@/types/chat';
import { RouterActivitySidebar } from './chat/RouterActivitySidebar';
import { AdapterLoadingStatus, type AdapterState, type AdapterLifecycleState } from './chat/AdapterLoadingStatus';
import { PreChatAdapterPrompt } from './chat/PreChatAdapterPrompt';
import { AdapterLoadingProgress, type AdapterLoadingItem } from './chat/AdapterLoadingProgress';
import { useSSE } from '@/hooks/useSSE';
import type { AdapterStreamEvent, AdapterStateTransitionEvent } from '@/api/streaming-types';
import { ChatSearchBar } from './chat/ChatSearchBar';
import { ChatSessionActions } from './chat/ChatSessionActions';
import { ChatTagsManager } from './chat/ChatTagsManager';
import { ChatShareDialog } from './chat/ChatShareDialog';
import { ChatArchivePanel } from './chat/ChatArchivePanel';
import {
  useChatStreaming,
  useChatAdapterState,
  useChatRouterDecisions
} from '@/hooks/chat';

interface ChatInterfaceProps {
  selectedTenant: string;
  initialStackId?: string;
  sessionId?: string; // Optional: load existing session
  /** Document context for document-specific chat */
  documentContext?: {
    documentId: string;
    documentName: string;
    collectionId?: string;
  };
  /** Callback when user wants to view a document (for evidence navigation) */
  onViewDocument?: (documentId: string, pageNumber?: number, highlightText?: string) => void;
}

export function ChatInterface({ selectedTenant, initialStackId, sessionId, documentContext, onViewDocument }: ChatInterfaceProps) {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState('');
  const [selectedStackId, setSelectedStackId] = useState<string>(initialStackId || '');
  const [selectedCollectionId, setSelectedCollectionId] = useState<string | null>(null);
  const [currentSessionId, setCurrentSessionId] = useState<string | null>(sessionId || null);
  const [isHistoryOpen, setIsHistoryOpen] = useState(false);
  const [isRouterActivityOpen, setIsRouterActivityOpen] = useState(false);
  const [editingSessionId, setEditingSessionId] = useState<string | null>(null);
  const [newSessionName, setNewSessionName] = useState('');
  const [showContext, setShowContext] = useState(true);
  const scrollAreaRef = useRef<HTMLDivElement>(null);

  // New state for chat features
  const [searchQuery, setSearchQuery] = useState('');
  const [isArchivePanelOpen, setIsArchivePanelOpen] = useState(false);
  const [shareDialogSessionId, setShareDialogSessionId] = useState<string | null>(null);
  const [tagsDialogSessionId, setTagsDialogSessionId] = useState<string | null>(null);

  // Use selectedTenant for API hooks that support undefined (default stack)
  // Keep tenantId fallback for other hooks that require a string
  const tenantId = selectedTenant || 'default';
  const { data: stacks = [] } = useAdapterStacks();
  const { data: defaultStack } = useGetDefaultStack(selectedTenant);
  const { data: collections = [] } = useCollections();
  const queryClient = useQueryClient();
  const {
    sessions,
    isLoading: isLoadingSessions,
    createSession,
    updateSession,
    addMessage,
    updateMessage,
    deleteSession,
    getSession,
    updateSessionCollection,
  } = useChatSessionsApi(tenantId);

  // Memoize selected stack (needed before hooks)
  const selectedStack = useMemo(
    () => stacks.find(s => s.id === selectedStackId),
    [stacks, selectedStackId]
  );

  // Streaming message state (for in-progress messages)
  const [streamingMessageId, setStreamingMessageId] = useState<string | null>(null);

  // Chat streaming hook
  const {
    isStreaming,
    streamedText,
    currentRequestId,
    sendMessage,
    cancelStream,
    resetStream,
    tokensReceived,
    streamDuration
  } = useChatStreaming({
    sessionId: currentSessionId,
    stackId: selectedStackId,
    collectionId: selectedCollectionId || undefined,
    onMessageSent: (message) => {
      // Add user message to messages
      setMessages(prev => [...prev, message]);
      if (currentSessionId) {
        addMessage(currentSessionId, message);
      }

      // Create placeholder streaming message
      const assistantId = `assistant-${Date.now()}`;
      setStreamingMessageId(assistantId);
      setMessages(prev => [...prev, {
        id: assistantId,
        role: 'assistant',
        content: '',
        timestamp: new Date(),
        isStreaming: true,
      }]);
    },
    onStreamComplete: async (response) => {
      // Fetch router decision and evidence
      let routerDecision = null;
      if (currentRequestId) {
        const decision = await fetchDecision(response.id, currentRequestId);
        routerDecision = decision;
      }

      // Fetch evidence
      const evidence = await fetchMessageEvidence(response.id);
      const completedMessage = {
        ...response,
        routerDecision: routerDecision as ExtendedRouterDecision | null,
        evidence,
        isVerified: evidence.length > 0,
        verifiedAt: evidence.length > 0 ? new Date().toISOString() : undefined,
        isStreaming: false,
      };

      // Replace streaming message with completed message
      setMessages(prev => prev.map(msg =>
        msg.id === streamingMessageId ? completedMessage : msg
      ));

      setStreamingMessageId(null);

      if (currentSessionId) {
        debouncedUpdateSession.debouncedFn(currentSessionId, {
          messages: messages.map(m => m.id === streamingMessageId ? completedMessage : m),
        });
      }
    },
    onError: (error) => {
      logger.error('Chat streaming error', { component: 'ChatInterface' }, error);
      // Remove streaming message on error
      if (streamingMessageId) {
        setMessages(prev => prev.filter(m => m.id !== streamingMessageId));
        setStreamingMessageId(null);
      }
    },
  });

  // Adapter state tracking hook
  const {
    adapterStates,
    isCheckingAdapters,
    allAdaptersReady,
    unreadyAdapters,
    loadAllAdapters,
    checkAdapterReadiness,
    showAdapterPrompt,
    dismissAdapterPrompt,
    continueWithUnready
  } = useChatAdapterState({
    stackId: selectedStackId,
    enabled: true,
  });

  // Router decisions hook
  const {
    decisions,
    isLoadingDecision,
    lastDecision,
    fetchDecision,
    clearDecisions,
    decisionHistory
  } = useChatRouterDecisions({
    stackId: selectedStackId,
  });

  // Set default stack on mount
  useEffect(() => {
    if (!selectedStackId && defaultStack?.id) {
      setSelectedStackId(defaultStack.id);
    }
  }, [defaultStack, selectedStackId]);

  // Load session if sessionId prop is provided
  useEffect(() => {
    if (sessionId && sessionId !== currentSessionId) {
      const session = getSession(sessionId);
      if (session) {
        setCurrentSessionId(sessionId);
        setMessages(session.messages);
        setSelectedStackId(session.stackId);
        // Note: collection_id is on the backend session, not local session
        // We'll fetch it when needed
      }
    } else if (!currentSessionId && selectedStackId && !isLoadingSessions) {
      // Create new session if none exists and stack is selected
      const stack = stacks.find(s => s.id === selectedStackId);
      if (stack) {
        const newSession = createSession(
          `Chat with ${stack.name || 'Stack'}`,
          selectedStackId,
          stack.name,
          selectedCollectionId || undefined
        );
        setCurrentSessionId(newSession.id);
      }
    }
  }, [sessionId, currentSessionId, selectedStackId, stacks, isLoadingSessions, getSession, createSession, selectedCollectionId]);

  // Debounced session save to avoid performance issues
  // Use useRef to avoid dependency issues
  const updateSessionRef = useRef(updateSession);
  updateSessionRef.current = updateSession;

  const debouncedUpdateSession = useDebouncedCallback(
    (sessionId: string, updates: Partial<ChatSession>) => {
      updateSessionRef.current(sessionId, updates);
    },
    500 // 500ms debounce
  );

  // Save messages to session whenever they change (debounced)
  useEffect(() => {
    if (currentSessionId && messages.length > 0) {
      debouncedUpdateSession.debouncedFn(currentSessionId, { messages });
    }
    // Cleanup: flush on unmount to ensure final save
    return () => {
      if (currentSessionId && messages.length > 0) {
        debouncedUpdateSession.flush();
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [messages, currentSessionId]); // debouncedUpdateSession excluded from deps

  // Auto-scroll to bottom when new messages arrive
  useEffect(() => {
    if (scrollAreaRef.current) {
      const scrollContainer = scrollAreaRef.current.querySelector('[data-radix-scroll-area-viewport]');
      if (scrollContainer) {
        scrollContainer.scrollTop = scrollContainer.scrollHeight;
      }
    }
  }, [messages]);


  // Fetch evidence data for a message
  const fetchMessageEvidence = useCallback(async (messageId: string): Promise<EvidenceItem[]> => {
    try {
      const response = await fetch(`/api/v1/chat/messages/${messageId}/evidence`);
      if (response.ok) {
        return await response.json();
      }
      return [];
    } catch (err) {
      logger.error('Failed to fetch message evidence', {
        component: 'ChatInterface',
        messageId,
      }, toError(err));
      return [];
    }
  }, []);


  const handleSend = useCallback(async () => {
    if (!input.trim() || isStreaming) return;

    // Check if adapters are ready before sending
    if (!allAdaptersReady && adapterStates.size > 0) {
      // Show adapter prompt if not ready
      // The PreChatAdapterPrompt component will handle showing the prompt
      toast.warning('Some adapters are not ready. Please load them first.');
      return;
    }

    // Resolve stack to adapter IDs
    const adapterIds = selectedStack?.adapter_ids || [];

    if (!adapterIds || adapterIds.length === 0) {
      toast.error('Please select a stack with adapters');
      return;
    }

    // Clear input immediately
    const messageContent = input.trim();
    setInput('');

    // Send message using the streaming hook
    await sendMessage(messageContent, adapterIds);
  }, [input, isStreaming, selectedStack, allAdaptersReady, adapterStates, sendMessage]);

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  // selectedStack is memoized earlier in the component
  const adapterCount = selectedStack?.adapter_ids?.length ?? selectedStack?.adapters?.length ?? 0;
  const stackLabel = selectedStack?.name || 'No stack selected';
  const isDefaultStack = Boolean(
    defaultStack?.id && selectedStack?.id && selectedStack.id === defaultStack.id
  );
  const stackDetails = selectedStack?.lifecycle_state ?? selectedStack?.description ?? null;
  const baseModelLabel = 'Not provided';

  // Get recent sessions (last 10, sorted by updatedAt), filtered by search query
  const recentSessions = useMemo(() => {
    let filtered = sessions;

    // Filter by search query
    if (searchQuery.trim()) {
      const query = searchQuery.toLowerCase();
      filtered = sessions.filter(session =>
        session.name.toLowerCase().includes(query) ||
        session.messages.some(msg => msg.content.toLowerCase().includes(query))
      );
    }

    return filtered
      .sort((a, b) => b.updatedAt.getTime() - a.updatedAt.getTime())
      .slice(0, 10);
  }, [sessions, searchQuery]);

  // Get preview text from first user message
  const getSessionPreview = (session: typeof sessions[0]) => {
    const firstUserMessage = session.messages.find(m => m.role === 'user');
    if (firstUserMessage) {
      return firstUserMessage.content.slice(0, 50) + (firstUserMessage.content.length > 50 ? '...' : '');
    }
    return 'No messages yet';
  };

  const handleLoadSession = useCallback((sessionId: string) => {
    const session = getSession(sessionId);
    if (session) {
      setMessages(session.messages);
      setCurrentSessionId(sessionId);
      if (session.stackId) {
        setSelectedStackId(session.stackId);
      }
      setIsHistoryOpen(false);
    }
  }, [getSession]);

  const handleCreateSession = useCallback(() => {
    if (!selectedStackId) {
      toast.error('Please select a stack first');
      return;
    }
    const selectedStack = stacks.find(s => s.id === selectedStackId);
    const newSession = createSession(
      `Session ${new Date().toLocaleString()}`,
      selectedStackId,
      selectedStack?.name
    );
    setCurrentSessionId(newSession.id);
    setMessages([]);
    setNewSessionName('');
    setIsHistoryOpen(false);
    toast.success('New session created');
  }, [selectedStackId, stacks, createSession]);

  const handleDeleteSession = useCallback((sessionId: string, e: React.MouseEvent) => {
    e.stopPropagation();
    if (window.confirm('Are you sure you want to delete this session?')) {
      deleteSession(sessionId);
      if (currentSessionId === sessionId) {
        setCurrentSessionId(null);
        setMessages([]);
      }
      toast.success('Session deleted');
    }
  }, [deleteSession, currentSessionId]);

  const handleRenameSession = useCallback((sessionId: string, newName: string) => {
    // Validate session name
    const trimmedName = newName.trim();
    if (!trimmedName || trimmedName.length === 0) {
      toast.error('Session name cannot be empty');
      return;
    }
    if (trimmedName.length > 100) {
      toast.error('Session name must be 100 characters or less');
      return;
    }
    
    updateSession(sessionId, { name: trimmedName });
    setEditingSessionId(null);
    toast.success('Session renamed');
  }, [updateSession]);

  // Handler for viewing document evidence
  const handleViewDocumentClick = useCallback((documentId: string, pageNumber?: number, highlightText?: string) => {
    // Use the provided callback if available
    if (onViewDocument) {
      onViewDocument(documentId, pageNumber, highlightText);
    } else {
      // Fallback behavior: log and show toast
      logger.info('View document requested', {
        component: 'ChatInterface',
        documentId,
        pageNumber,
        highlightText,
      });
      toast.info(`Opening document ${documentId}${pageNumber ? ` (page ${pageNumber})` : ''}`);
    }
  }, [onViewDocument]);

  // Handler for collection change
  const handleCollectionChange = useCallback(async (collectionId: string) => {
    const newCollectionId = collectionId === 'none' ? null : collectionId;
    setSelectedCollectionId(newCollectionId);

    // Update current session's collection if session exists
    if (currentSessionId) {
      try {
        await updateSessionCollection(currentSessionId, newCollectionId);
        toast.success(newCollectionId ? 'Collection selected' : 'Collection cleared');
      } catch (error) {
        logger.error('Failed to update session collection', {
          component: 'ChatInterface',
          sessionId: currentSessionId,
          collectionId: newCollectionId,
        }, toError(error));
        toast.error('Failed to update collection');
      }
    }
  }, [currentSessionId, updateSessionCollection]);

  // Get selected collection name for display
  const selectedCollectionName = useMemo(() => {
    if (!selectedCollectionId) return 'No collection';
    const collection = collections.find(c => c.collection_id === selectedCollectionId);
    return collection?.name || 'Unknown';
  }, [selectedCollectionId, collections]);

  return (
    <div className="flex flex-col h-full relative">
      {/* Pre-Chat Adapter Loading Prompt */}
      <PreChatAdapterPrompt
        open={showAdapterPrompt}
        onOpenChange={dismissAdapterPrompt}
        adapters={Array.from(adapterStates.values()).map(state => ({
          id: state.adapterId,
          name: state.name,
          state: state.state,
          isLoading: state.isLoading,
          error: state.error,
        }))}
        onLoadAll={loadAllAdapters}
        onContinueAnyway={continueWithUnready}
        isLoading={isCheckingAdapters}
      />

      {/* History Sidebar */}
      {isHistoryOpen && (
        <SectionErrorBoundary sectionName="Session History">
          <div className="absolute left-0 top-0 bottom-0 w-80 bg-background border-r z-10 flex flex-col">
            <div className="border-b px-4 py-3 flex items-center justify-between">
              <h3 className="font-semibold text-sm flex items-center gap-2">
                <History className="h-4 w-4" />
                Conversation History
              </h3>
              <div className="flex items-center gap-2">
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => setIsArchivePanelOpen(true)}
                  aria-label="Open archive"
                  title="View archived sessions"
                >
                  <Archive className="h-4 w-4" />
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => setIsHistoryOpen(false)}
                  aria-label="Close history"
                >
                  <X className="h-4 w-4" />
                </Button>
              </div>
            </div>

            {/* Search Bar */}
            <div className="px-4 py-2 border-b">
              <ChatSearchBar
                onSelectSession={(sessionId) => handleLoadSession(sessionId)}
                onSelectMessage={(sessionId, messageId) => {
                  handleLoadSession(sessionId);
                  // TODO: After loading, scroll to the specific message
                  // For now, just load the session - message scrolling can be added later
                  if (messageId) {
                    logger.info('Search navigated to message', { sessionId, messageId });
                  }
                }}
                placeholder="Search sessions..."
              />
            </div>

            {/* Create New Session */}
            <div className="border-b px-4 py-2">
              <Button
                variant="outline"
                size="sm"
                className="w-full"
                onClick={handleCreateSession}
                disabled={!selectedStackId}
              >
                <Plus className="h-4 w-4 mr-2" />
                New Session
              </Button>
            </div>

            <ScrollArea className="flex-1">
              <div className="p-2 space-y-1">
                {isLoadingSessions ? (
                  <div className="text-center py-8 text-sm text-muted-foreground">
                    Loading sessions...
                  </div>
                ) : recentSessions.length === 0 ? (
                  <div className="text-center py-8 text-sm text-muted-foreground">
                    No conversation history
                  </div>
                ) : (
                  recentSessions.map(session => (
                    <div
                      key={session.id}
                      className={`p-3 rounded-lg border cursor-pointer transition-colors hover:bg-muted ${
                        currentSessionId === session.id ? 'bg-muted border-primary' : ''
                      }`}
                      onClick={() => handleLoadSession(session.id)}
                    >
                      <div className="flex items-start justify-between gap-2">
                        <div className="flex-1 min-w-0">
                          {editingSessionId === session.id ? (
                            <Input
                              value={newSessionName}
                              onChange={(e) => setNewSessionName(e.target.value)}
                              onBlur={() => {
                                if (newSessionName.trim()) {
                                  handleRenameSession(session.id, newSessionName.trim());
                                } else {
                                  setEditingSessionId(null);
                                }
                              }}
                              onKeyDown={(e) => {
                                if (e.key === 'Enter' && newSessionName.trim()) {
                                  handleRenameSession(session.id, newSessionName.trim());
                                } else if (e.key === 'Escape') {
                                  setEditingSessionId(null);
                                }
                              }}
                              className="h-7 text-sm mb-1"
                              autoFocus
                              onClick={(e) => e.stopPropagation()}
                            />
                          ) : (
                            <>
                              <div className="flex items-center justify-between">
                                <p className="text-sm font-medium truncate">{session.name}</p>
                                <div className="flex items-center gap-1 ml-2" onClick={(e) => e.stopPropagation()}>
                                  <ChatSessionActions
                                    sessionId={session.id}
                                    tenantId={tenantId}
                                    onRename={() => {
                                      setEditingSessionId(session.id);
                                      setNewSessionName(session.name);
                                    }}
                                    onManageTags={() => setTagsDialogSessionId(session.id)}
                                    onSetCategory={() => {
                                      // TODO: Implement category dialog
                                      toast.info('Category management coming soon');
                                    }}
                                    onShare={() => setShareDialogSessionId(session.id)}
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
                                <span className="text-xs text-muted-foreground">•</span>
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
      )}

      {/* Router Activity Sidebar */}
      <SectionErrorBoundary sectionName="Router Activity">
        <RouterActivitySidebar
          open={isRouterActivityOpen}
          onClose={() => setIsRouterActivityOpen(false)}
          stackId={selectedStackId}
        />
      </SectionErrorBoundary>

      {/* Currently Loaded Panel */}
      <div className={`px-4 mt-2 ${isHistoryOpen ? 'ml-80' : ''} ${isRouterActivityOpen ? 'mr-96' : ''}`}>
        <Card>
          <CardHeader className="flex flex-row items-start justify-between space-y-0">
            <div className="space-y-1">
              <CardTitle className="text-base">Currently Loaded</CardTitle>
              <p className="text-xs text-muted-foreground">
                Stack context for this chat session.
              </p>
              {isDefaultStack && (
                <Badge variant="secondary" className="w-fit" aria-label="This is the default adapter stack for your tenant">
                  Default stack for tenant
                </Badge>
              )}
            </div>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setShowContext(!showContext)}
              aria-label={showContext ? 'Hide stack context' : 'Show stack context'}
            >
              {showContext ? 'Hide' : 'Show'}
            </Button>
          </CardHeader>
          {showContext && (
            <CardContent className="grid grid-cols-1 sm:grid-cols-4 gap-3">
              <div>
                <p className="text-xs text-muted-foreground">Stack</p>
                <p className="font-medium truncate">{stackLabel}</p>
                {stackDetails && (
                  <p className="text-xs text-muted-foreground truncate">{stackDetails}</p>
                )}
              </div>
              <div>
                <p className="text-xs text-muted-foreground">Adapters</p>
                <p className="font-medium">{adapterCount || '—'}</p>
              </div>
              <div>
                <p className="text-xs text-muted-foreground">Collection</p>
                <p className="font-medium truncate">{selectedCollectionName}</p>
              </div>
              <div>
                <p className="text-xs text-muted-foreground">Base model</p>
                <p className="font-medium text-muted-foreground">{baseModelLabel}</p>
              </div>
            </CardContent>
          )}
        </Card>
      </div>

      {/* Header with stack selector */}
      <div className={`border-b px-4 py-3 transition-all ${isHistoryOpen ? 'ml-80' : ''} ${isRouterActivityOpen ? 'mr-96' : ''}`}>
        <div className="flex items-center justify-between mb-2">
          <div className="flex items-center gap-3">
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setIsHistoryOpen(!isHistoryOpen)}
            aria-label={isHistoryOpen ? "Close history" : "Open history"}
          >
            {isHistoryOpen ? (
              <ChevronLeft className="h-4 w-4" />
            ) : (
              <History className="h-4 w-4" />
            )}
          </Button>
          <Layers className="h-5 w-5 text-muted-foreground" aria-hidden="true" />
          <div className="flex items-center gap-2">
            <span className="text-sm text-muted-foreground">Stack:</span>
            <Select 
              value={selectedStackId} 
              onValueChange={setSelectedStackId}
              aria-label="Select adapter stack"
              aria-describedby={stacks.length === 0 ? "no-stacks-hint" : undefined}
            >
              <SelectTrigger className="w-[300px]">
                <SelectValue placeholder="Select a stack" />
              </SelectTrigger>
              <SelectContent>
                {stacks.map(stack => (
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
          {selectedStack && (
            <Badge variant="outline" aria-label={`${selectedStack.adapter_ids?.length || 0} adapters in stack`}>
              {selectedStack.adapter_ids?.length || 0} adapter
              {(selectedStack.adapter_ids?.length || 0) !== 1 ? 's' : ''}
            </Badge>
          )}
          {/* Adapter loading status indicator */}
          {adapterStates.size > 0 && (
            <AdapterLoadingStatus
              stackId={selectedStackId}
              adapters={Array.from(adapterStates.values()).map(state => ({
                id: state.adapterId,
                name: state.name,
                state: state.state,
                isLoading: state.isLoading,
                error: state.error,
              }))}
              compact
            />
          )}
          <Database className="h-5 w-5 text-muted-foreground" aria-hidden="true" />
          <div className="flex items-center gap-2">
            <span className="text-sm text-muted-foreground">Collection:</span>
            <Select
              value={selectedCollectionId || 'none'}
              onValueChange={handleCollectionChange}
              aria-label="Select collection"
            >
              <SelectTrigger className="w-[200px]">
                <SelectValue placeholder="No collection" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="none">No collection</SelectItem>
                {collections.map(collection => (
                  <SelectItem key={collection.collection_id} value={collection.collection_id}>
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
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setIsRouterActivityOpen(!isRouterActivityOpen)}
            aria-label={isRouterActivityOpen ? "Close router activity" : "Open router activity"}
            title="View router decision history"
          >
            <Activity className="h-4 w-4" />
          </Button>
          </div>
        </div>

        {/* Session Tags */}
        {currentSessionId && (
          <div className="mt-2">
            <ChatTagsManager sessionId={currentSessionId} />
          </div>
        )}
      </div>

      {/* Messages area */}
      <ScrollArea
        className={`flex-1 transition-all ${isHistoryOpen ? 'ml-80' : ''} ${isRouterActivityOpen ? 'mr-96' : ''}`}
        ref={scrollAreaRef}
        aria-label="Chat messages"
        role="log"
        aria-live="polite"
        aria-atomic="false"
      >
        <SectionErrorBoundary sectionName="Chat Messages">
          <div className="py-4">
            {messages.length === 0 ? (
              <div
                className="flex items-center justify-center h-full text-muted-foreground"
                role="status"
                aria-live="polite"
              >
                <div className="text-center">
                  <Layers className="h-12 w-12 mx-auto mb-4 opacity-50" aria-hidden="true" />
                  <p className="text-lg font-medium">Start a conversation</p>
                  <p className="text-sm mt-1">
                    Select a stack and send a message to begin
                  </p>
                </div>
              </div>
            ) : (
              messages.map(message => (
                <ChatMessageComponent
                  key={message.id}
                  message={
                    // Update streaming message with current streamed text
                    message.id === streamingMessageId
                      ? { ...message, content: streamedText }
                      : message
                  }
                  onViewDocument={handleViewDocumentClick}
                />
              ))
            )}
            {isLoadingDecision && (
              <div className="text-xs text-muted-foreground px-4" role="status" aria-live="polite">
                Loading router decision details...
              </div>
            )}
          </div>
        </SectionErrorBoundary>
      </ScrollArea>

      {/* Input area */}
      <div className={`border-t px-4 py-3 transition-all ${isHistoryOpen ? 'ml-80' : ''} ${isRouterActivityOpen ? 'mr-96' : ''}`}>
        <form 
          onSubmit={(e) => { e.preventDefault(); handleSend(); }}
          className="flex gap-2"
          aria-label="Chat message input"
        >
          <Textarea
            value={input}
            onChange={e => setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Type your message... (Enter to send, Shift+Enter for new line)"
            className="min-h-[60px] resize-none"
            disabled={isStreaming || !selectedStackId}
            aria-label="Message input"
            aria-describedby={!selectedStackId ? "stack-required-hint" : undefined}
          />
          {!selectedStackId && (
            <span id="stack-required-hint" className="sr-only">
              Please select an adapter stack before sending messages
            </span>
          )}
          <Button
            type="submit"
            onClick={handleSend}
            disabled={isStreaming || !input.trim() || !selectedStackId}
            size="lg"
            aria-label={isStreaming ? "Sending message..." : "Send message"}
          >
            {isStreaming ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <Send className="h-4 w-4" />
            )}
          </Button>
        </form>
        {!selectedStackId && (
          <p className="text-xs text-muted-foreground mt-2">
            Please select a stack to start chatting
          </p>
        )}
      </div>

      {/* Share Dialog */}
      {shareDialogSessionId && (
        <ChatShareDialog
          sessionId={shareDialogSessionId}
          open={!!shareDialogSessionId}
          onOpenChange={(open) => {
            if (!open) setShareDialogSessionId(null);
          }}
        />
      )}

      {/* Tags Manager Dialog */}
      {tagsDialogSessionId && (
        <Dialog open={!!tagsDialogSessionId} onOpenChange={(open) => {
          if (!open) setTagsDialogSessionId(null);
        }}>
          <DialogContent className="max-w-md">
            <DialogHeader>
              <DialogTitle>Manage Tags</DialogTitle>
            </DialogHeader>
            <ChatTagsManager sessionId={tagsDialogSessionId} />
          </DialogContent>
        </Dialog>
      )}

      {/* Archive Panel Dialog */}
      {isArchivePanelOpen && (
        <div className="fixed inset-0 bg-background/80 backdrop-blur-sm z-50 flex items-center justify-center">
          <div className="bg-background border rounded-lg shadow-lg p-6 max-w-4xl w-full max-h-[90vh] overflow-auto">
            <div className="flex items-center justify-between mb-4">
              <h2 className="text-lg font-semibold">Archive & Trash</h2>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => setIsArchivePanelOpen(false)}
              >
                <X className="h-4 w-4" />
              </Button>
            </div>
            <SectionErrorBoundary sectionName="Archive Panel">
              <ChatArchivePanel />
            </SectionErrorBoundary>
          </div>
        </div>
      )}
    </div>
  );
}
