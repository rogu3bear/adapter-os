import React, { useState, useRef, useEffect, useCallback, useMemo, useRef as useReactRef } from 'react';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';
import { Input } from '@/components/ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Badge } from '@/components/ui/badge';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { ChatMessageComponent, type ChatMessage, type EvidenceItem } from './chat/ChatMessage';
import apiClient from '@/api/client';
import { logger, toError } from '@/utils/logger';
import { toast } from 'sonner';
import { Send, Loader2, Layers, History, X, ChevronLeft, ChevronRight, Plus, Trash2, Edit2, Activity, Database } from 'lucide-react';
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
  const [isLoading, setIsLoading] = useState(false);
  const [isLoadingRouterDecision, setIsLoadingRouterDecision] = useState(false);
  const [currentRequestId, setCurrentRequestId] = useState<string | null>(null);
  const [isHistoryOpen, setIsHistoryOpen] = useState(false);
  const [isRouterActivityOpen, setIsRouterActivityOpen] = useState(false);
  const [editingSessionId, setEditingSessionId] = useState<string | null>(null);
  const [newSessionName, setNewSessionName] = useState('');
  const [showContext, setShowContext] = useState(true);
  const scrollAreaRef = useRef<HTMLDivElement>(null);
  const abortControllerRef = useRef<AbortController | null>(null);

  // Adapter loading state
  const [adapterStates, setAdapterStates] = useState<Map<string, AdapterState>>(new Map());
  const [showAdapterPrompt, setShowAdapterPrompt] = useState(false);
  const [isLoadingAdapters, setIsLoadingAdapters] = useState(false);
  const [pendingMessage, setPendingMessage] = useState<string | null>(null);

  const tenantId = selectedTenant || 'default';
  const { data: stacks = [] } = useAdapterStacks();
  const { data: defaultStack } = useGetDefaultStack(tenantId);
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

  // Subscribe to adapter state transitions via SSE
  useSSE<AdapterStreamEvent>('/v1/stream/adapters', {
    enabled: !!selectedStackId,
    onMessage: (event) => {
      if (event && 'current_state' in event) {
        const transition = event as AdapterStateTransitionEvent;
        setAdapterStates((prev) => {
          const updated = new Map(prev);
          const existing = updated.get(transition.adapter_id);
          if (existing) {
            updated.set(transition.adapter_id, {
              ...existing,
              state: transition.current_state,
              isLoading: false,
            });
          }
          return updated;
        });
      }
    },
  });

  // Memoize selected stack
  const selectedStack = useMemo(
    () => stacks.find(s => s.id === selectedStackId),
    [stacks, selectedStackId]
  );

  // Build adapter states from stack when stack changes
  useEffect(() => {
    if (selectedStack?.adapters) {
      const states = new Map<string, AdapterState>();
      selectedStack.adapters.forEach((adapter) => {
        states.set(adapter.id || adapter.adapter_id || '', {
          id: adapter.id || adapter.adapter_id || '',
          name: adapter.name || adapter.adapter_id || 'Unknown',
          state: (adapter.lifecycle_state as AdapterLifecycleState) || 'unloaded',
        });
      });
      setAdapterStates(states);
    } else if (selectedStack?.adapter_ids) {
      // Fallback: create basic states from IDs
      const states = new Map<string, AdapterState>();
      selectedStack.adapter_ids.forEach((id) => {
        states.set(id, {
          id,
          name: id,
          state: 'unloaded', // Unknown - will update via SSE
        });
      });
      setAdapterStates(states);
    }
  }, [selectedStack]);

  // Check if all adapters are ready for inference
  const allAdaptersReady = useMemo(() => {
    if (adapterStates.size === 0) return true; // No adapters = ok
    const states = Array.from(adapterStates.values());
    return states.every((a) =>
      a.state === 'hot' || a.state === 'warm' || a.state === 'resident'
    );
  }, [adapterStates]);

  // Handle loading all adapters
  const handleLoadAllAdapters = useCallback(async () => {
    setIsLoadingAdapters(true);
    try {
      const adapterIds = Array.from(adapterStates.keys());
      // Load each adapter that isn't ready
      for (const adapterId of adapterIds) {
        const adapter = adapterStates.get(adapterId);
        if (adapter && adapter.state !== 'hot' && adapter.state !== 'warm' && adapter.state !== 'resident') {
          // Update state to loading
          setAdapterStates((prev) => {
            const updated = new Map(prev);
            const existing = updated.get(adapterId);
            if (existing) {
              updated.set(adapterId, { ...existing, isLoading: true });
            }
            return updated;
          });
          // Trigger load via API
          try {
            await apiClient.loadAdapter(adapterId);
          } catch (err) {
            logger.error('Failed to load adapter', { adapterId }, toError(err));
            setAdapterStates((prev) => {
              const updated = new Map(prev);
              const existing = updated.get(adapterId);
              if (existing) {
                updated.set(adapterId, { ...existing, isLoading: false, error: 'Failed to load' });
              }
              return updated;
            });
          }
        }
      }
      // Close prompt and send pending message if any
      setShowAdapterPrompt(false);
      if (pendingMessage) {
        setInput(pendingMessage);
        setPendingMessage(null);
        // Wait a moment for states to update, then send
        setTimeout(() => {
          // The handleSend will be triggered by the user or we can auto-send
        }, 1000);
      }
    } finally {
      setIsLoadingAdapters(false);
    }
  }, [adapterStates, pendingMessage]);

  // Handle continuing without loading adapters
  const handleContinueAnyway = useCallback(() => {
    setShowAdapterPrompt(false);
    if (pendingMessage) {
      setInput(pendingMessage);
      setPendingMessage(null);
    }
  }, [pendingMessage]);

  // Use React Query to cache stack fetches with retry
  const fetchStackWithRetry = useCallback(async (stackId: string): Promise<AdapterStack | null> => {
    // Use React Query for caching, with retry built-in
    try {
      const stack = await queryClient.fetchQuery({
        queryKey: ['adapter-stack', stackId],
        queryFn: async () => {
          const result = await apiClient.getAdapterStack(stackId);
          if (!result.adapter_ids || result.adapter_ids.length === 0) {
            throw new Error('Stack has no adapter IDs');
          }
          return result;
        },
        staleTime: 60000, // Cache for 1 minute
        retry: 3,
        retryDelay: (attemptIndex) => Math.min(1000 * 2 ** attemptIndex, 4000),
      });
      return stack;
    } catch (err) {
      logger.error('Failed to fetch stack after retries', {
        component: 'ChatInterface',
        stackId,
      }, toError(err));
      return null;
    }
  }, [queryClient]);

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

  // Use React Query to cache router decisions
  const fetchRouterDecision = useCallback(async (requestId: string): Promise<ExtendedRouterDecision | null> => {
    setIsLoadingRouterDecision(true);
    try {
      // Use React Query to cache router decisions
      const routerView = await queryClient.fetchQuery({
        queryKey: ['router-decision', requestId],
        queryFn: () => apiClient.getSessionRouterView(requestId),
        staleTime: 30000, // Cache for 30 seconds
        retry: 1, // Only retry once for router decisions
      });

      // Convert SessionRouterViewResponse to RouterDecision format
      if (!routerView.steps || routerView.steps.length === 0) {
        return null;
      }

      const firstStep = routerView.steps[0];

      // Map adapter_idx to actual adapter IDs using cached stack fetch
      let adapterIdMap = new Map<number, string>();
      let stackFetchFailed = false;

      if (routerView.stack_id) {
        const stack = await fetchStackWithRetry(routerView.stack_id);
        if (stack && stack.adapter_ids && stack.adapter_ids.length > 0) {
          stack.adapter_ids.forEach((adapterId, idx) => {
            adapterIdMap.set(idx, adapterId);
          });
        } else {
          stackFetchFailed = true;
          logger.warn('Stack fetch returned empty or null', {
            component: 'ChatInterface',
            requestId,
            stackId: routerView.stack_id,
          });
          toast.error('Unable to resolve adapter IDs. Showing adapter indices instead.', {
            description: 'Stack information could not be loaded. Router decision may show adapter indices instead of names.',
          });
        }
      }

      // Map adapter indices to adapter IDs
      const selectedAdapters: string[] = [];
      const scores: Record<string, number> = {};

      firstStep.adapters_fired.forEach(a => {
        if (a.selected) {
          const adapterId = adapterIdMap.get(a.adapter_idx) || `adapter-${a.adapter_idx}`;
          selectedAdapters.push(adapterId);
          scores[adapterId] = a.gate_value;
        }
      });

      // Build candidates array with proper mapping
      const candidates: RouterCandidateInfo[] = firstStep.adapters_fired.map(a => {
        const adapterId = adapterIdMap.get(a.adapter_idx) || `adapter-${a.adapter_idx}`;
        return {
          adapter_idx: a.adapter_idx,
          adapter_id: adapterId,
          raw_score: a.gate_value,
          gate_q15: Math.round(a.gate_value * 32767),
          gate_float: a.gate_value,
          selected: a.selected,
        };
      });

      // Create extended router decision with proper types
      const mappedDecision: ExtendedRouterDecision = {
        request_id: routerView.request_id,
        selected_adapters: selectedAdapters,
        scores,
        timestamp: firstStep.timestamp,
        latency_ms: 0,
        entropy: firstStep.entropy,
        tau: firstStep.tau,
        step: firstStep.step,
        k_value: selectedAdapters.length,
        candidates,
        adapter_map: adapterIdMap, // Store map for debugging
      };

      return mappedDecision;
    } catch (err) {
      logger.error('Failed to fetch router decision', {
        component: 'ChatInterface',
        requestId,
      }, toError(err));
      return null;
    }
  }, []);

  const handleSend = useCallback(async () => {
    if (!input.trim() || isLoading) return;

    // Check if adapters are ready before sending
    if (!allAdaptersReady && adapterStates.size > 0) {
      setPendingMessage(input.trim());
      setShowAdapterPrompt(true);
      return;
    }

    const userMessage: ChatMessage = {
      id: `user-${Date.now()}`,
      role: 'user',
      content: input.trim(),
      timestamp: new Date(),
    };

    setMessages(prev => [...prev, userMessage]);
    
    // Save user message to session
    if (currentSessionId) {
      addMessage(currentSessionId, userMessage);
    }
    
    setInput('');
    setIsLoading(true);

    // Create placeholder assistant message
    const assistantMessageId = `assistant-${Date.now()}`;
    const assistantMessage: ChatMessage = {
      id: assistantMessageId,
      role: 'assistant',
      content: '',
      timestamp: new Date(),
      isStreaming: true,
    };
    setMessages(prev => [...prev, assistantMessage]);

    // Resolve stack to adapter IDs (use memoized selectedStack)
    const adapterIds = selectedStack?.adapter_ids || undefined;

    if (!adapterIds || adapterIds.length === 0) {
      toast.error('Please select a stack with adapters');
      setIsLoading(false);
      setMessages(prev => prev.filter(m => m.id !== assistantMessageId));
      return;
    }

    // Create abort controller for cancellation
    abortControllerRef.current = new AbortController();
    const requestId = `chat-${Date.now()}`;
    setCurrentRequestId(requestId);

    try {
      let fullText = '';
      let tokenCount = 0;

      await apiClient.streamInfer(
        {
          prompt: userMessage.content,
          max_tokens: 500,
          temperature: 0.7,
          adapter_stack: adapterIds,
          ...(selectedCollectionId && {
            collection_id: selectedCollectionId,
          }),
          ...(documentContext && {
            document_id: documentContext.documentId,
            collection_id: documentContext.collectionId,
          }),
        },
        {
          onToken: (token: string) => {
            tokenCount++;
            fullText += token;
            setMessages(prev =>
              prev.map(msg =>
                msg.id === assistantMessageId
                  ? { ...msg, content: fullText }
                  : msg
              )
            );
          },
          onComplete: async (fullText: string, finishReason: string | null) => {
            // Fetch router decision and evidence (with loading state)
            const routerDecision = await fetchRouterDecision(requestId);

            // Fetch evidence data if session has collection_id
            // Note: This assumes the session has a collection_id property
            // If not available, evidence will be empty array
            const evidence = await fetchMessageEvidence(assistantMessageId);

            const completedMessage: ChatMessage = {
              id: assistantMessageId,
              role: 'assistant',
              content: fullText,
              timestamp: new Date(),
              requestId,
              routerDecision,
              evidence,
              isVerified: evidence.length > 0, // Mark as verified if evidence exists
              verifiedAt: evidence.length > 0 ? new Date().toISOString() : undefined,
              isStreaming: false,
            };

            setMessages(prev =>
              prev.map(msg =>
                msg.id === assistantMessageId ? completedMessage : msg
              )
            );

            // Save to session (debounced)
            if (currentSessionId) {
              debouncedUpdateSession.debouncedFn(currentSessionId, {
                messages: [...messages.filter(m => m.id !== assistantMessageId), completedMessage],
              });
            }

            setIsLoading(false);
            setCurrentRequestId(null);
          },
          onError: (error: Error) => {
            logger.error('Chat inference error', {
              component: 'ChatInterface',
              requestId,
              error: error.message,
            }, error);
            toast.error(`Inference failed: ${error.message}`);
            setMessages(prev => prev.filter(m => m.id !== assistantMessageId));
            setIsLoading(false);
            setCurrentRequestId(null);
          },
        },
        abortControllerRef.current.signal
      );
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Inference failed');
      if (error.name !== 'AbortError') {
        toast.error(`Inference failed: ${error.message}`);
        logger.error('Chat inference failed', {
          component: 'ChatInterface',
          stackId: selectedStackId,
        }, toError(err));
      }
      setMessages(prev => prev.filter(m => m.id !== assistantMessageId));
      setIsLoading(false);
      setCurrentRequestId(null);
    }
  }, [input, isLoading, selectedStackId, stacks, fetchRouterDecision, allAdaptersReady, adapterStates]);

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

  // Get recent sessions (last 10, sorted by updatedAt)
  const recentSessions = useMemo(() => {
    return sessions
      .sort((a, b) => b.updatedAt.getTime() - a.updatedAt.getTime())
      .slice(0, 10);
  }, [sessions]);

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
        onOpenChange={setShowAdapterPrompt}
        adapters={Array.from(adapterStates.values())}
        onLoadAll={handleLoadAllAdapters}
        onContinueAnyway={handleContinueAnyway}
        isLoading={isLoadingAdapters}
      />

      {/* History Sidebar */}
      {isHistoryOpen && (
        <div className="absolute left-0 top-0 bottom-0 w-80 bg-background border-r z-10 flex flex-col">
          <div className="border-b px-4 py-3 flex items-center justify-between">
            <h3 className="font-semibold text-sm flex items-center gap-2">
              <History className="h-4 w-4" />
              Conversation History
            </h3>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setIsHistoryOpen(false)}
              aria-label="Close history"
            >
              <X className="h-4 w-4" />
            </Button>
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
                              <div className="flex items-center gap-1 ml-2">
                                <Button
                                  variant="ghost"
                                  size="sm"
                                  className="h-6 w-6 p-0"
                                  onClick={(e) => {
                                    e.stopPropagation();
                                    setEditingSessionId(session.id);
                                    setNewSessionName(session.name);
                                  }}
                                >
                                  <Edit2 className="h-3 w-3" />
                                </Button>
                                <Button
                                  variant="ghost"
                                  size="sm"
                                  className="h-6 w-6 p-0 text-destructive hover:text-destructive"
                                  onClick={(e) => handleDeleteSession(session.id, e)}
                                >
                                  <Trash2 className="h-3 w-3" />
                                </Button>
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
      )}

      {/* Router Activity Sidebar */}
      <RouterActivitySidebar
        open={isRouterActivityOpen}
        onClose={() => setIsRouterActivityOpen(false)}
        stackId={selectedStackId}
      />

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
      <div className={`border-b px-4 py-3 flex items-center justify-between transition-all ${isHistoryOpen ? 'ml-80' : ''} ${isRouterActivityOpen ? 'mr-96' : ''}`}>
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
              adapters={Array.from(adapterStates.values())}
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

      {/* Messages area */}
      <ScrollArea 
        className={`flex-1 transition-all ${isHistoryOpen ? 'ml-80' : ''} ${isRouterActivityOpen ? 'mr-96' : ''}`}
        ref={scrollAreaRef}
        aria-label="Chat messages"
        role="log"
        aria-live="polite"
        aria-atomic="false"
      >
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
                message={message}
                onViewDocument={handleViewDocumentClick}
              />
            ))
          )}
          {isLoadingRouterDecision && (
            <div className="text-xs text-muted-foreground px-4" role="status" aria-live="polite">
              Loading router decision details...
            </div>
          )}
        </div>
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
            disabled={isLoading || !selectedStackId}
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
            disabled={isLoading || !input.trim() || !selectedStackId}
            size="lg"
            aria-label={isLoading ? "Sending message..." : "Send message"}
          >
            {isLoading ? (
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
    </div>
  );
}
